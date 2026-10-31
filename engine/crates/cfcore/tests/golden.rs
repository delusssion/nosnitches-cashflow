//! Приёмка движка: сверка с эталонами аналитика (analytics/reference,
//! A5) по всем сделкам из testdata/golden. Допуск контракта — 0,01 ₽ по
//! amount и principal_after, ключ строки — deal_id + payment_dttm +
//! payment_type + stream.

use std::collections::BTreeMap;
use std::io::Cursor;

use cfcore::engine::build_cashflow;
use cfcore::io::{parse_deals, read_rates};
use cfcore::{Config, RateBook};

fn repo_path(rel: &str) -> String {
    format!("{}/../../../{}", env!("CARGO_MANIFEST_DIR"), rel)
}

struct Row {
    amount: f64,
    principal_after: f64,
}

fn parse_rows(csv_text: &str) -> BTreeMap<String, Row> {
    let mut rdr = csv::ReaderBuilder::new().from_reader(Cursor::new(csv_text));
    let header: Vec<String> = rdr
        .headers()
        .expect("заголовок эталона")
        .iter()
        .map(String::from)
        .collect();
    let mut out = BTreeMap::new();
    for rec in rdr.records() {
        let r = rec.expect("эталон читается");
        let get = |name: &str| {
            let idx = header
                .iter()
                .position(|h| h == name)
                .expect("колонка эталона");
            r.get(idx).unwrap_or("").to_string()
        };
        let key = format!(
            "{}|{}|{}",
            get("payment_dttm"),
            get("payment_type"),
            get("stream")
        );
        out.insert(
            key,
            Row {
                amount: get("amount").parse().expect("amount число"),
                principal_after: get("principal_after")
                    .parse()
                    .expect("principal_after число"),
            },
        );
    }
    out
}

#[test]
fn engine_matches_analyst_goldens() {
    let deals = parse_deals(Cursor::new(
        std::fs::read_to_string(repo_path("testdata/deals/test_deals.csv"))
            .expect("test_deals.csv"),
    ))
    .expect("сделки читаются");
    let book: RateBook = read_rates(repo_path("testdata/rates/key_rate.csv").as_ref())
        .expect("история ключевой ставки читается");
    let cfg = Config::default();

    let golden_dir = repo_path("testdata/golden");
    let mut golden_files: Vec<std::path::PathBuf> = std::fs::read_dir(&golden_dir)
        .expect("каталог эталонов")
        .map(|e| e.expect("файл эталона").path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "csv"))
        .collect();
    golden_files.sort();
    assert!(golden_files.len() >= 13, "эталонов должно быть хотя бы 13");

    let mut failures: Vec<String> = Vec::new();
    for path in &golden_files {
        let deal_id = path
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .trim_start_matches("deal_")
            .to_string();
        let Some(deal) = deals.iter().find(|d| d.deal_id == deal_id) else {
            failures.push(format!("сделка {deal_id}: нет в test_deals.csv"));
            continue;
        };
        let (events, errs) = build_cashflow(deal, &book, &cfg);
        if errs.iter().any(|e| e.severity == cfcore::Severity::Error) {
            failures.push(format!("сделка {deal_id}: ошибки {errs:?}"));
            continue;
        }
        let got: BTreeMap<String, Row> = events
            .iter()
            .map(|e| {
                (
                    format!(
                        "{}|{}|{}",
                        e.payment_dttm,
                        e.payment_type.as_str(),
                        e.stream.as_str()
                    ),
                    Row {
                        amount: e.amount,
                        principal_after: e.principal_after,
                    },
                )
            })
            .collect();
        let want = parse_rows(&std::fs::read_to_string(path).expect("эталон читается"));

        if got.len() != want.len() {
            let extra: Vec<&String> = got.keys().filter(|k| !want.contains_key(*k)).collect();
            let missing: Vec<&String> = want.keys().filter(|k| !got.contains_key(*k)).collect();
            failures.push(format!(
                "сделка {deal_id}: строк {} против эталона {}; лишние {extra:?}; отсутствуют {missing:?}",
                got.len(),
                want.len()
            ));
            continue;
        }
        let mut diffs: Vec<String> = Vec::new();
        for (key, w) in &want {
            let Some(g) = got.get(key) else {
                diffs.push(format!("  {key}: строки нет"));
                continue;
            };
            if (g.amount - w.amount).abs() > 0.01 {
                diffs.push(format!(
                    "  {key}: amount {:.2} ≠ эталону {:.2}",
                    g.amount, w.amount
                ));
            }
            if (g.principal_after - w.principal_after).abs() > 0.01 {
                diffs.push(format!(
                    "  {key}: principal_after {:.2} ≠ эталону {:.2}",
                    g.principal_after, w.principal_after
                ));
            }
        }
        if diffs.is_empty() {
            println!("сделка {deal_id}: OK ({} строк)", want.len());
        } else {
            failures.push(format!(
                "сделка {deal_id}: {} расхождений\n{}",
                diffs.len(),
                diffs.join("\n")
            ));
        }
    }

    if !failures.is_empty() {
        panic!("расхождения с эталонами:\n{}", failures.join("\n"));
    }
}
