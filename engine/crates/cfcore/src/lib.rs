//! cfcore — расчётное ядро калькулятора денежных потоков.
//!
//! По сделке (формат 1 docs/CONTRACT.md) и истории индикатора (формат 2) строит
//! расписание будущих платежей (формат 3): проценты, тело, пересмотры ставки,
//! субсидия. Алгоритмы — docs/METHODOLOGY.md, гипотезы H1–H9 — поля [`Config`].
//!
//! # Пример
//!
//! ```
//! use std::io::Cursor;
//! use cfcore::{generate, io::parse_deals, Config, RateBook, RateSeries};
//! use chrono::NaiveDate;
//!
//! let csv = "report_dttm,discriminator,deal_id,currency_id,start_principal_amt,current_principal_amt,deal_dttm,maturity_dttm,rate_type_cd,interest_rate,base_indicator_rate,base_indicator,interest_payment_rule_start_dttm,interest_payment_rule_period_cd,interest_payment_rule_day_num,day_convention_cd,regular_payment_rule_start_dttm,regular_payment_rule_period_cd,regular_payment_rule_day_num,repricing_rule_start_dttm,repricing_rule_period_cd,repricing_rule_day_num,accrued_interest,product,behavior_model_cd,interest_payment,principal_payment,repricing_dttm_schedule,repricing_dttm_rate_schedule,repricing_dttm_rate_schedule_fixed,cap_limit_interest_rate,floor_limit_interest_rate,sub_rate_type_cd,sub_interest_rate,sub_float_indicator,sub_payment_rule_period_cd,sub_payment_rule_day_num,sub_payment_rule_start_dttm,sub_repricing_rule_start_dttm,sub_repricing_rule_period_cd,sub_repricing_rule_day_num,sub_cap_limit_interest_rate,sub_floor_limit_interest_rate,sub_coeff_index,deal_report_dttm,sub_repricing_rule_first_event_flg,indicator_lag_cd,sub_indicator_lag_cd
//! 2022-08-31,SIMPLE,21,RUB,,1047077.3,2020-11-23,2023-04-30,FIXED,0.061,,,2020-11-23,1M,7,ACT/ACT ISDA,,ENDDATE,,,,,,,,,,,,,,,,,,,,,,,,,,,2022-08-31,,,
//! ";
//! let deals = parse_deals(Cursor::new(csv)).unwrap();
//! let d = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
//! let mut rates = RateBook::new();
//! rates.insert(RateSeries::new("CBKEY1M", vec![(d("2022-07-25"), 0.08)]));
//!
//! let (events, errors) = generate(&deals[0], &rates, &Config::default());
//! assert!(errors.is_empty());
//! let last = events.last().unwrap();
//! assert_eq!(last.payment_type.as_str(), "close_payment");
//! assert_eq!(last.principal_after, 0.0);
//!
//! // Портфель считается параллельно; порядок результатов = порядок сделок.
//! let results = cfcore::generate_portfolio(&deals, &rates, &Config::default());
//! assert_eq!(results[0].0.len(), events.len());
//! ```

pub mod dates;
pub mod daycount;
pub mod engine;
pub mod io;
pub mod model;
pub mod rates;
pub mod validate;

pub use model::{Config, Deal, DealError, Discriminator, Event, Severity, Stream};
pub use rates::{RateBook, RateSeries};

/// Кэшфлоу одной сделки: события формата 3 и замечания для `errors.csv`.
///
/// Чистая функция без состояния — её можно вызывать из любого числа потоков.
/// Сделка с ошибкой (`severity = error`) возвращает пустой список событий.
pub fn generate(deal: &Deal, rates: &RateBook, cfg: &Config) -> (Vec<Event>, Vec<DealError>) {
    engine::build_cashflow(deal, rates, cfg)
}

/// Кэшфлоу портфеля: сделки считаются параллельно на всех ядрах (rayon),
/// результат `i` соответствует сделке `deals[i]`.
///
/// Для файлов, которые не помещаются в память, — потоковый конвейер CLI
/// `cfgen run` (чтение пачками, расчёт, запись по порядку).
pub fn generate_portfolio(
    deals: &[Deal],
    rates: &RateBook,
    cfg: &Config,
) -> Vec<(Vec<Event>, Vec<DealError>)> {
    use rayon::prelude::*;
    deals.par_iter().map(|d| generate(d, rates, cfg)).collect()
}
