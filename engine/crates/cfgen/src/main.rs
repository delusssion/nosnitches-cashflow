//! cfgen — CLI калькулятора денежных потоков поверх библиотеки cfcore.

use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use cfcore::engine::build_cashflow;
use cfcore::io::{read_deals, read_rates, write_errors_csv, CashflowWriter};
use cfcore::{Config, Severity};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "cfgen",
    about = "Калькулятор денежных потоков (движок cfcore)",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Посчитать кэшфлоу по файлу сделок и истории ставок
    Run {
        /// Файл сделок, формат 1 контракта (testdata/deals/*.csv)
        #[arg(long, value_name = "FILE")]
        deals: PathBuf,
        /// История индикаторов, формат 2 (testdata/rates/key_rate.csv)
        #[arg(long, value_name = "FILE")]
        rates: PathBuf,
        /// Куда положить cashflow.csv и errors.csv
        #[arg(long, value_name = "DIR")]
        out_dir: PathBuf,
    },
    /// Сверить результат с эталонами: допуск 0,01 ₽, остаток после последнего платежа 0
    Verify {
        /// Результат прогона (cashflow.csv)
        #[arg(long, value_name = "FILE")]
        result: PathBuf,
        /// Каталог эталонов (testdata/golden/)
        #[arg(long, value_name = "DIR")]
        golden_dir: PathBuf,
        /// Допуск по сумме платежа, ₽
        #[arg(long, default_value_t = 0.01)]
        tolerance: f64,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Run {
            deals,
            rates,
            out_dir,
        } => run(&deals, &rates, &out_dir),
        Command::Verify {
            result,
            golden_dir,
            tolerance,
        } => verify(&result, &golden_dir, tolerance),
    }
}

/// Сверка результата прогона с эталонами (формат 4 контракта): по ключу
/// `deal_id + payment_dttm + payment_type + stream`, допуск по amount и
/// principal_after. Код возврата 1 при расхождениях — удобно для CI.
fn verify(result_path: &Path, golden_dir: &Path, tolerance: f64) -> Result<()> {
    let result = read_cashflow(result_path)?;
    let mut failures: Vec<String> = Vec::new();
    let mut checked = 0usize;

    let entries = std::fs::read_dir(golden_dir)
        .with_context(|| format!("каталог эталонов {}", golden_dir.display()))?;
    let mut golden_paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|ext| ext == "csv"))
        .collect();
    golden_paths.sort();

    for path in &golden_paths {
        let deal_id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .trim_start_matches("deal_")
            .to_string();
        let golden = read_cashflow(path)?;
        let got: Vec<_> = result.iter().filter(|r| r.deal_id == deal_id).collect();
        if got.len() != golden.len() {
            failures.push(format!(
                "сделка {deal_id}: строк {} против эталона {}",
                got.len(),
                golden.len()
            ));
            continue;
        }
        let mut diffs: Vec<String> = Vec::new();
        for (g, w) in got.iter().zip(golden.iter()) {
            if g.key != w.key {
                diffs.push(format!(
                    "  строка {}: ключ {} ≠ эталону {}",
                    g.event_order, g.key, w.key
                ));
                continue;
            }
            if (g.amount - w.amount).abs() > tolerance {
                diffs.push(format!(
                    "  {}: amount {:.2} ≠ эталону {:.2}",
                    g.key, g.amount, w.amount
                ));
            }
            if (g.principal_after - w.principal_after).abs() > tolerance {
                diffs.push(format!(
                    "  {}: principal_after {:.2} ≠ эталону {:.2}",
                    g.key, g.principal_after, w.principal_after
                ));
            }
        }
        checked += 1;
        if diffs.is_empty() {
            println!("сделка {deal_id}: OK ({} строк)", golden.len());
        } else {
            failures.push(format!("сделка {deal_id}:\n{}", diffs.join("\n")));
        }
    }

    if failures.is_empty() {
        println!("Сверка пройдена: {checked} эталон(ов), допуск {tolerance:.2} ₽");
        Ok(())
    } else {
        for f in &failures {
            eprintln!("{f}");
        }
        bail!("расхождения с эталонами в {} сделках", failures.len());
    }
}

struct CashRow {
    deal_id: String,
    event_order: String,
    key: String,
    amount: f64,
    principal_after: f64,
}

fn read_cashflow(path: &Path) -> Result<Vec<CashRow>> {
    let file = File::open(path).with_context(|| format!("чтение {}", path.display()))?;
    let mut rdr = csv::Reader::from_reader(BufReader::new(file));
    let header: Vec<String> = rdr
        .headers()
        .with_context(|| format!("заголовок {}", path.display()))?
        .iter()
        .map(String::from)
        .collect();
    let idx = |name: &str| {
        header
            .iter()
            .position(|h| h == name)
            .with_context(|| format!("в {} нет колонки {name}", path.display()))
    };
    let (i_deal, i_ord, i_day, i_type, i_stream) = (
        idx("deal_id")?,
        idx("event_order")?,
        idx("payment_dttm")?,
        idx("payment_type")?,
        idx("stream")?,
    );
    let (i_amount, i_after) = (idx("amount")?, idx("principal_after")?);

    let mut rows = Vec::new();
    for rec in rdr.records() {
        let r = rec.with_context(|| format!("строка {}", path.display()))?;
        rows.push(CashRow {
            deal_id: r.get(i_deal).unwrap_or("").to_string(),
            event_order: r.get(i_ord).unwrap_or("").to_string(),
            key: format!(
                "{}|{}|{}",
                r.get(i_day).unwrap_or(""),
                r.get(i_type).unwrap_or(""),
                r.get(i_stream).unwrap_or("")
            ),
            amount: r.get(i_amount).unwrap_or("").parse().unwrap_or(f64::NAN),
            principal_after: r.get(i_after).unwrap_or("").parse().unwrap_or(f64::NAN),
        });
    }
    Ok(rows)
}

fn run(deals_path: &Path, rates_path: &Path, out_dir: &Path) -> Result<()> {
    let deals = read_deals(deals_path)
        .with_context(|| format!("чтение сделок {}", deals_path.display()))?;
    let book = read_rates(rates_path)
        .with_context(|| format!("чтение ставок {}", rates_path.display()))?;
    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("создание каталога {}", out_dir.display()))?;

    let cashflow_path = out_dir.join("cashflow.csv");
    let errors_path = out_dir.join("errors.csv");
    let file = File::create(&cashflow_path)
        .with_context(|| format!("создание {}", cashflow_path.display()))?;
    let mut writer = CashflowWriter::from_writer(BufWriter::new(file))
        .with_context(|| format!("запись {}", cashflow_path.display()))?;

    let cfg = Config::default();
    let mut ok = 0usize;
    let mut failed = 0usize;
    let mut warnings = 0usize;
    let mut events_total = 0usize;
    let mut all_errors = Vec::new();

    for deal in &deals {
        let (events, errors) = build_cashflow(deal, &book, &cfg);
        writer
            .write_deal(deal, &events)
            .with_context(|| format!("запись кэшфлоу сделки {}", deal.deal_id))?;
        events_total += events.len();
        if errors.iter().any(|e| e.severity == Severity::Error) {
            failed += 1;
        } else {
            ok += 1;
        }
        warnings += errors
            .iter()
            .filter(|e| e.severity == Severity::Warning)
            .count();
        all_errors.extend(errors);
    }
    writer.flush().context("закрытие cashflow.csv")?;

    let errors_file = File::create(&errors_path)
        .with_context(|| format!("создание {}", errors_path.display()))?;
    write_errors_csv(BufWriter::new(errors_file), &all_errors)
        .with_context(|| format!("запись {}", errors_path.display()))?;

    println!(
        "Дата расчёта: {} | сделок: {} | посчитано: {} | с ошибками: {} | предупреждений: {} | событий: {}",
        deals.first().map(|d| d.report_dttm.to_string()).unwrap_or_default(),
        deals.len(),
        ok,
        failed,
        warnings,
        events_total
    );
    println!("Индикаторы: {:?}", book.indicators().collect::<Vec<_>>());
    println!(
        "Готово: {} и {}",
        cashflow_path.display(),
        errors_path.display()
    );
    Ok(())
}
