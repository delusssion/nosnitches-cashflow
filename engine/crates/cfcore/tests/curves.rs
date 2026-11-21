//! Интеграционные тесты контракта QuantumTeam: прогнозные кривые curves.csv,
//! сценарии base и stress, проверка base_rate на датах пересмотра и сумм процентов.

use chrono::NaiveDate;
use std::fs;
use std::path::PathBuf;

use cfcore::engine::build_cashflow;
use cfcore::io::{parse_deals, read_curves, read_rates};
use cfcore::model::PaymentType;
use cfcore::Config;

fn repo_path(rel: &str) -> PathBuf {
    PathBuf::from(format!("{}/../../../{}", env!("CARGO_MANIFEST_DIR"), rel))
}

fn d(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

#[test]
fn quantum_team_base_vs_stress_on_float_deal() {
    let deals_csv = fs::read_to_string(repo_path("testdata/deals/test_deals.csv")).unwrap();
    let deals = parse_deals(deals_csv.as_bytes()).unwrap();
    let deal22 = deals.iter().find(|d| d.deal_id == "22").cloned().unwrap();

    let rates_path = repo_path("testdata/rates/key_rate.csv");
    let curves_path = repo_path("testdata/rates/curves.csv");

    // 1. Базовый сценарий (base)
    let mut book_base = read_rates(&rates_path).unwrap();
    let base_curves = read_curves(&curves_path, Some("base")).unwrap();
    book_base.set_forecasts(base_curves);

    // 2. Стресс сценарий (stress)
    let mut book_stress = read_rates(&rates_path).unwrap();
    let stress_curves = read_curves(&curves_path, Some("stress")).unwrap();
    book_stress.set_forecasts(stress_curves);

    let cfg = Config::default();

    let (events_base, errs_base) = build_cashflow(&deal22, &book_base, &cfg);
    let (events_stress, errs_stress) = build_cashflow(&deal22, &book_stress, &cfg);

    assert!(errs_base.is_empty(), "базовый прогон без ошибок");
    assert!(errs_stress.is_empty(), "стресс прогон без ошибок");

    // Проверка 1: До первой точки прогноза (до 2022-12-01) base_rate совпадает (0.08)
    let repr_sep_base = events_base
        .iter()
        .find(|e| e.payment_dttm == d("2022-09-15") && e.payment_type == PaymentType::Repricing)
        .unwrap();
    let repr_sep_stress = events_stress
        .iter()
        .find(|e| e.payment_dttm == d("2022-09-15") && e.payment_type == PaymentType::Repricing)
        .unwrap();
    assert_eq!(repr_sep_base.base_rate, Some(0.08));
    assert_eq!(repr_sep_stress.base_rate, Some(0.08));

    // Проверка 2: На пересмотре 2022-12-15 действует прогноз от 2022-12-01:
    // base -> 0.10, stress -> 0.12
    let repr_dec_base = events_base
        .iter()
        .find(|e| e.payment_dttm == d("2022-12-15") && e.payment_type == PaymentType::Repricing)
        .unwrap();
    let repr_dec_stress = events_stress
        .iter()
        .find(|e| e.payment_dttm == d("2022-12-15") && e.payment_type == PaymentType::Repricing)
        .unwrap();
    assert_eq!(repr_dec_base.base_rate, Some(0.10));
    assert_eq!(repr_dec_stress.base_rate, Some(0.12));

    // Проверка 3: На пересмотре 2023-01-15 действует прогноз от 2023-01-01:
    // base -> 0.09, stress -> 0.14
    let repr_jan_base = events_base
        .iter()
        .find(|e| e.payment_dttm == d("2023-01-15") && e.payment_type == PaymentType::Repricing)
        .unwrap();
    let repr_jan_stress = events_stress
        .iter()
        .find(|e| e.payment_dttm == d("2023-01-15") && e.payment_type == PaymentType::Repricing)
        .unwrap();
    assert_eq!(repr_jan_base.base_rate, Some(0.09));
    assert_eq!(repr_jan_stress.base_rate, Some(0.14));

    // Проверка 4: В стресс-сценарии проценты строго больше, чем в базовом
    let int_base: f64 = events_base
        .iter()
        .filter(|e| e.payment_type == PaymentType::InterestPayment)
        .map(|e| e.amount)
        .sum();
    let int_stress: f64 = events_stress
        .iter()
        .filter(|e| e.payment_type == PaymentType::InterestPayment)
        .map(|e| e.amount)
        .sum();
    assert!(
        int_stress > int_base,
        "проценты при стрессе ({int_stress:.2}) должны превышать базовые ({int_base:.2})"
    );

    // Проверка 5: Инвариант тела — в обоих сценариях погашено ровно 1 000 000 ₽, остаток 0.00 ₽
    let body_base: f64 = events_base
        .iter()
        .filter(|e| {
            matches!(
                e.payment_type,
                PaymentType::PrincipalPayment | PaymentType::ClosePayment
            )
        })
        .map(|e| e.amount)
        .sum();
    let body_stress: f64 = events_stress
        .iter()
        .filter(|e| {
            matches!(
                e.payment_type,
                PaymentType::PrincipalPayment | PaymentType::ClosePayment
            )
        })
        .map(|e| e.amount)
        .sum();
    assert!((body_base - 1_000_000.0).abs() < 0.01);
    assert!((body_stress - 1_000_000.0).abs() < 0.01);
    assert_eq!(events_base.last().unwrap().principal_after, 0.0);
    assert_eq!(events_stress.last().unwrap().principal_after, 0.0);
}
