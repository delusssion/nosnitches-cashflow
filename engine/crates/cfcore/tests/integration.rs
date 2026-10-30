//! Интеграционные тесты: чтение CSV контракта, валидация и запуск движка.

use std::io::Cursor;

use cfcore::engine::build_cashflow;
use cfcore::io::{parse_deals, read_rates, RowError};
use cfcore::model::codes;
use cfcore::{Config, RateBook, Severity};

fn edge_deals() -> Vec<cfcore::Deal> {
    parse_deals(Cursor::new(include_str!("fixtures/deals_edge.csv"))).unwrap()
}

fn edge_rates() -> RateBook {
    read_rates("tests/fixtures/key_rate.csv".as_ref()).unwrap()
}

#[test]
fn edge_fixture_parses() {
    let deals = edge_deals();
    assert_eq!(deals.len(), 4);
    assert_eq!(deals[0].deal_id, "T-ANN-1");
    // H5: ACT/ACT на входе -> ACT/ACT ISDA
    assert_eq!(
        deals[0].day_convention_cd,
        cfcore::model::DayConvention::ActActIsda
    );
    assert_eq!(deals[0].accrued_interest, 123.45);
    // депозит: отрицательный остаток сохраняется как есть
    assert!(deals[1].current_principal_amt < 0.0);
    // FLOAT-сделка: маржа, индикатор, cap/floor и лаг прочитаны
    assert_eq!(deals[2].base_indicator.as_deref(), Some("CBKEY1M"));
    assert_eq!(deals[2].cap_limit_interest_rate, Some(0.1));
    assert_eq!(deals[2].floor_limit_interest_rate, Some(0.06));
    assert_eq!(deals[2].indicator_lag_cd.as_deref(), Some("-1M"));
}

#[test]
fn explicit_lists_sorted_and_deduped_on_read() {
    let deals = edge_deals();
    let gap = &deals[3];
    use chrono::NaiveDate;
    let d = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
    // даты процентов: сортировка, дубль убран, прошлая 2022-08-31 осталась
    // (фильтрация прошедших — задача движка, H6)
    assert_eq!(
        gap.interest_payment,
        vec![d("2022-08-31"), d("2022-09-30"), d("2022-10-31")]
    );
    // график тела: дубль даты побеждён последним значением, сумма 850 000
    assert_eq!(
        gap.principal_payment,
        vec![(d("2022-10-30"), 425_000.0), (d("2022-12-30"), 425_000.0)]
    );
}

#[test]
fn validation_flags_match_expectations() {
    let deals = edge_deals();
    let book = edge_rates();
    let cfg = Config::default();

    let (_, errs) = build_cashflow(&deals[0], &book, &cfg);
    assert!(
        errs.is_empty(),
        "чистая сделка не должна давать ошибок: {errs:?}"
    );

    let (_, errs) = build_cashflow(&deals[1], &book, &cfg);
    assert!(errs.is_empty(), "депозит валиден: {errs:?}");

    let (_, errs) = build_cashflow(&deals[2], &book, &cfg);
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].code, codes::CURRENCY_NOT_RUB);
    assert_eq!(errs[0].severity, Severity::Error);

    let (_, errs) = build_cashflow(&deals[3], &book, &cfg);
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].code, codes::PRINCIPAL_SCHEDULE_MISMATCH);
    assert_eq!(errs[0].severity, Severity::Warning);
}

#[test]
fn rates_fixture_groups_into_one_series() {
    let book = edge_rates();
    assert_eq!(book.len(), 1);
    let s = book.get("CBKEY1M").unwrap();
    assert!(s.span().is_some());
}

#[test]
fn broken_row_reports_line_number() {
    let csv = "report_dttm,discriminator,deal_id\n2022-08-31,ANN,NO-COLS\n";
    let err = parse_deals(Cursor::new(csv)).expect_err("должна быть ошибка заголовка");
    match err {
        RowError::Msg { line, .. } => assert_eq!(line, 1),
        other => panic!("ожидалась ошибка заголовка, получили {other:?}"),
    }
}

/// Демо-данные репозитория (testdata/samples) должны читаться нашим парсером —
/// это фиксирует совместимость формата движка с фронтендом.
#[test]
fn repo_samples_parse() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../testdata/samples/deals_sample.csv"
    );
    let deals = cfcore::io::read_deals(path.as_ref()).expect("демо-файл сделок должен читаться");
    assert!(!deals.is_empty());
    let cfg = Config::default();
    let book = RateBook::new();
    for deal in &deals {
        let (_, errs) = build_cashflow(deal, &book, &cfg);
        assert!(
            errs.iter().all(|e| e.severity == Severity::Warning),
            "демо-сделка {} не должна давать ошибок: {errs:?}",
            deal.deal_id
        );
    }
}
