//! Конвенции дней: ACT/365 и ACT/ACT ISDA.

use chrono::{Datelike, NaiveDate};

use crate::model::DayConvention;

/// Число дней между датами: `d2 - d1`.
pub fn days_between(d1: NaiveDate, d2: NaiveDate) -> i64 {
    (d2 - d1).num_days()
}

/// Доля года по конвенции сделки.
///
/// * ACT/365 — фактические дни / 365.
/// * ACT/ACT ISDA — фактические дни, поделённые на длину своего
///   календарного года (365 или 366), сегменты суммируются.
pub fn year_fraction(d1: NaiveDate, d2: NaiveDate, dc: DayConvention) -> f64 {
    match dc {
        DayConvention::Act365 => f64::from(days_between(d1, d2) as i32) / 365.0,
        DayConvention::ActActIsda => act_act_isda(d1, d2),
    }
}

fn is_leap(y: i32) -> bool {
    y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)
}

fn act_act_isda(d1: NaiveDate, d2: NaiveDate) -> f64 {
    if d2 <= d1 {
        return 0.0;
    }
    let mut acc = 0.0;
    let mut cur = d1;
    while cur < d2 {
        let next_year =
            NaiveDate::from_ymd_opt(cur.year() + 1, 1, 1).expect("следующий год валиден");
        let seg_end = next_year.min(d2);
        let days = (seg_end - cur).num_days();
        let denom = if is_leap(cur.year()) { 366.0 } else { 365.0 };
        acc += days as f64 / denom;
        cur = seg_end;
    }
    acc
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn act365_simple() {
        let yf = year_fraction(d("2022-08-31"), d("2022-09-30"), DayConvention::Act365);
        assert!((yf - 30.0 / 365.0).abs() < 1e-12);
    }

    #[test]
    fn act365_same_as_isda_within_plain_year() {
        let a = d("2022-01-01");
        let b = d("2022-07-01");
        assert_eq!(
            year_fraction(a, b, DayConvention::Act365),
            year_fraction(a, b, DayConvention::ActActIsda)
        );
    }

    #[test]
    fn isda_leap_year_period_uses_366() {
        // 2020-02-28 -> 2020-03-01: 2 дня високосного года -> 2/366
        let yf = year_fraction(d("2020-02-28"), d("2020-03-01"), DayConvention::ActActIsda);
        assert!((yf - 2.0 / 366.0).abs() < 1e-12);
        // обычный год: 2/365 (27.02 -> 01.03 = 2 дня)
        let yf = year_fraction(d("2021-02-27"), d("2021-03-01"), DayConvention::ActActIsda);
        assert!((yf - 2.0 / 365.0).abs() < 1e-12);
    }

    #[test]
    fn isda_crosses_year_boundary() {
        // 17 дней 2020 (високосный) + 14 дней 2021
        let yf = year_fraction(d("2020-12-15"), d("2021-01-15"), DayConvention::ActActIsda);
        let want = 17.0 / 366.0 + 14.0 / 365.0;
        assert!((yf - want).abs() < 1e-12);
    }

    #[test]
    fn isda_full_year_is_exactly_one() {
        assert!(
            (year_fraction(d("2021-05-01"), d("2022-05-01"), DayConvention::ActActIsda) - 1.0)
                .abs()
                < 1e-12
        );
        assert!(
            (year_fraction(d("2020-01-01"), d("2021-01-01"), DayConvention::ActActIsda) - 1.0)
                .abs()
                < 1e-12
        );
    }

    #[test]
    fn zero_and_inverted_periods() {
        assert_eq!(
            year_fraction(d("2022-08-31"), d("2022-08-31"), DayConvention::Act365),
            0.0
        );
        assert_eq!(
            year_fraction(d("2022-09-01"), d("2022-08-31"), DayConvention::ActActIsda),
            0.0
        );
    }
}
