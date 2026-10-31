//! Построение сеток дат по правилам (формат 1 контракта) и разбор явных
//! списков дат и графиков «дата -> сумма».

use chrono::{Datelike, Duration, NaiveDate};

use crate::model::{PaymentRule, PeriodCd};

pub fn is_leap(y: i32) -> bool {
    y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)
}

pub fn days_in_month(y: i32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if is_leap(y) {
                29
            } else {
                28
            }
        }
    }
}

/// Последний день месяца, в котором лежит `d`.
pub fn last_day_of_month(d: NaiveDate) -> NaiveDate {
    NaiveDate::from_ymd_opt(d.year(), d.month(), days_in_month(d.year(), d.month()))
        .expect("год и месяц взяты из валидной даты")
}

/// Прибавить `k` месяцев с прижатием дня к концу месяца: 31 января + 1M =
/// 28/29 февраля (а не «дрейф» на 2-3 марта, как при сложении через день).
pub fn add_months(d: NaiveDate, k: i32) -> NaiveDate {
    let m = i64::from(d.month()) - 1 + i64::from(k);
    let y = d.year() + i32::try_from(m.div_euclid(12)).expect("год в пределах диапазона");
    let mm = u32::try_from(m.rem_euclid(12) + 1).expect("месяц 1..=12");
    let day = d.day().min(days_in_month(y, mm));
    NaiveDate::from_ymd_opt(y, mm, day).expect("год, месяц и прижатый день валидны")
}

/// Переустановить день месяца: `-1` = последний день месяца,
/// дня нет (31-е в феврале) — последний день месяца.
pub fn with_day_num(d: NaiveDate, day_num: i32) -> NaiveDate {
    let last = days_in_month(d.year(), d.month());
    let day = if day_num < 0 {
        last
    } else {
        u32::try_from(day_num).unwrap_or(last).min(last)
    };
    NaiveDate::from_ymd_opt(d.year(), d.month(), day).expect("день прижат к концу месяца")
}

/// Разбор `*_period_cd`: `1M`, `2M`, `45D`, `153D`, `ENDDATE`.
pub fn parse_period_cd(s: &str) -> Option<PeriodCd> {
    let s = s.trim();
    if s.eq_ignore_ascii_case("ENDDATE") {
        return Some(PeriodCd::EndDate);
    }
    let (num, unit) = s.split_at(s.len().checked_sub(1)?);
    let n: u32 = num.parse().ok()?;
    if n == 0 {
        return None;
    }
    match unit {
        "M" => Some(PeriodCd::Months(n)),
        "D" => Some(PeriodCd::Days(n)),
        _ => None,
    }
}

/// Разбор лага индикатора в месяцах: `-1M` -> `-1`, `2M` -> `2`.
pub fn parse_lag_months(s: &str) -> Option<i32> {
    let s = s.trim();
    let body = s.strip_prefix('-').unwrap_or(s);
    let n = body.strip_suffix('M')?.parse::<i32>().ok()?;
    Some(if s.starts_with('-') { -n } else { n })
}

/// Все даты правила в интервале (`start_dttm`, `until`] — методика §2.2:
/// месяцы перебираются с k = 0 (первая дата может быть в месяце начала,
/// если `day_num` больше дня начала), дни — с k = 1. `ENDDATE` и пустой
/// период дат не дают (дату погашения добавляют вызывающие). Даты строго
/// возрастают, без дублей; будущее отсеивают вызывающие.
pub fn rule_schedule(rule: &PaymentRule, until: NaiveDate) -> Vec<NaiveDate> {
    let Some(anchor) = rule.start_dttm else {
        return Vec::new();
    };
    let day_num = rule.day_num;
    let mut out = Vec::new();
    match rule.period_cd {
        None | Some(PeriodCd::EndDate) => {}
        Some(PeriodCd::Months(n)) => {
            // шаг сетки = n месяцев (2M, 3M, 5M), а не один месяц
            let step = i32::try_from(n.max(1)).unwrap_or(i32::MAX);
            let mut k = 0i32;
            loop {
                let mut cand = add_months(anchor, k * step);
                if let Some(dn) = day_num {
                    cand = with_day_num(cand, dn);
                }
                if cand > until {
                    break;
                }
                if cand > anchor {
                    out.push(cand);
                }
                k += 1;
            }
        }
        Some(PeriodCd::Days(n)) => {
            let step = Duration::days(i64::from(n));
            let mut cand = anchor + step;
            while cand <= until {
                out.push(cand);
                cand += step;
            }
        }
    }
    out
}

/// Явный список дат через пробел (`interest_payment`): сортировка, дубликаты
/// убираются. Битый токен — ошибка с самим токеном в сообщении.
pub fn parse_date_list(s: &str) -> Result<Vec<NaiveDate>, String> {
    let mut dates = Vec::new();
    for tok in s.split_whitespace() {
        let d = NaiveDate::parse_from_str(tok, "%Y-%m-%d")
            .map_err(|_| format!("не разобрана дата «{tok}»"))?;
        dates.push(d);
    }
    dates.sort();
    dates.dedup();
    Ok(dates)
}

/// Явный график «дата,сумма» через пробел (`principal_payment` и
/// `repricing_dttm_rate_schedule_fixed`): сортировка по дате; суммы в одну
/// дату складываются (методика §2.1).
pub fn parse_pairs(s: &str) -> Result<Vec<(NaiveDate, f64)>, String> {
    let mut pairs = Vec::new();
    for tok in s.split_whitespace() {
        let (d, a) = tok
            .split_once(',')
            .ok_or_else(|| format!("нет запятой «дата,сумма» в «{tok}»"))?;
        let d = NaiveDate::parse_from_str(d, "%Y-%m-%d")
            .map_err(|_| format!("не разобрана дата «{d}»"))?;
        let a: f64 = a.parse().map_err(|_| format!("не разобрана сумма «{a}»"))?;
        pairs.push((d, a));
    }
    pairs.sort_by_key(|(d, _)| *d);
    let mut merged: Vec<(NaiveDate, f64)> = Vec::with_capacity(pairs.len());
    for (d, a) in pairs {
        match merged.last_mut() {
            Some((last_d, last_a)) if *last_d == d => *last_a += a,
            _ => merged.push((d, a)),
        }
    }
    Ok(merged)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn add_months_clamps_to_month_end() {
        assert_eq!(add_months(d("2020-01-31"), 1), d("2020-02-29")); // високосный
        assert_eq!(add_months(d("2021-01-31"), 1), d("2021-02-28"));
        assert_eq!(add_months(d("2020-02-29"), 1), d("2020-03-29"));
        assert_eq!(add_months(d("2020-01-31"), 13), d("2021-02-28"));
        assert_eq!(add_months(d("2020-10-31"), -1), d("2020-09-30"));
    }

    #[test]
    fn day_num_minus_one_and_clamp() {
        assert_eq!(with_day_num(d("2021-02-10"), -1), d("2021-02-28"));
        assert_eq!(with_day_num(d("2020-02-10"), -1), d("2020-02-29"));
        assert_eq!(with_day_num(d("2021-02-10"), 31), d("2021-02-28"));
        assert_eq!(with_day_num(d("2021-04-10"), 31), d("2021-04-30"));
    }

    #[test]
    fn period_cd_parsing() {
        assert_eq!(parse_period_cd("1M"), Some(PeriodCd::Months(1)));
        assert_eq!(parse_period_cd("45D"), Some(PeriodCd::Days(45)));
        assert_eq!(parse_period_cd("ENDDATE"), Some(PeriodCd::EndDate));
        assert_eq!(parse_period_cd("0M"), None);
        assert_eq!(parse_period_cd("X"), None);
        assert_eq!(parse_period_cd(""), None);
    }

    #[test]
    fn lag_parsing() {
        assert_eq!(parse_lag_months("-1M"), Some(-1));
        assert_eq!(parse_lag_months("2M"), Some(2));
        assert_eq!(parse_lag_months("45D"), None);
    }

    /// Окно (after, until] поверх полной сетки правила — как зовут движки.
    fn in_window(rule: &PaymentRule, after: NaiveDate, until: NaiveDate) -> Vec<NaiveDate> {
        rule_schedule(rule, until)
            .into_iter()
            .filter(|d| *d > after)
            .collect()
    }

    #[test]
    fn monthly_schedule_from_anchor_with_day_num() {
        let rule = PaymentRule {
            start_dttm: Some(d("2018-12-14")),
            period_cd: parse_period_cd("1M"),
            day_num: Some(14),
        };
        // окно с 2022-08-31 по 2022-12-14
        let got = in_window(&rule, d("2022-08-31"), d("2022-12-14"));
        let want = vec![
            d("2022-09-14"),
            d("2022-10-14"),
            d("2022-11-14"),
            d("2022-12-14"),
        ];
        assert_eq!(got, want);
    }

    #[test]
    fn monthly_k_zero_includes_anchor_month_when_day_num_later() {
        // методика §2.2: k = 0 — дата в месяце начала, если day_num позже
        let rule = PaymentRule {
            start_dttm: Some(d("2022-11-01")),
            period_cd: parse_period_cd("1M"),
            day_num: Some(20),
        };
        let got = rule_schedule(&rule, d("2023-02-28"));
        assert_eq!(
            got,
            vec![
                d("2022-11-20"),
                d("2022-12-20"),
                d("2023-01-20"),
                d("2023-02-20")
            ]
        );
        // день раньше дня начала — k=0 до начала, сетка со следующего месяца
        let rule2 = PaymentRule {
            start_dttm: Some(d("2022-11-25")),
            period_cd: parse_period_cd("1M"),
            day_num: Some(7),
        };
        assert_eq!(
            rule_schedule(&rule2, d("2023-01-31")),
            vec![d("2022-12-07"), d("2023-01-07")]
        );
    }

    #[test]
    fn monthly_day_num_minus_one_sticks_to_month_end() {
        let rule = PaymentRule {
            start_dttm: Some(d("2022-08-16")),
            period_cd: parse_period_cd("1M"),
            day_num: Some(-1),
        };
        let got = in_window(&rule, d("2022-08-31"), d("2022-11-30"));
        assert_eq!(got, vec![d("2022-09-30"), d("2022-10-31"), d("2022-11-30")]);
    }

    #[test]
    fn monthly_day_31_clamps_in_february() {
        let rule = PaymentRule {
            start_dttm: Some(d("2022-12-31")),
            period_cd: parse_period_cd("1M"),
            day_num: None,
        };
        let got = in_window(&rule, d("2022-12-31"), d("2023-03-31"));
        assert_eq!(got, vec![d("2023-01-31"), d("2023-02-28"), d("2023-03-31")]);
    }

    #[test]
    fn days_schedule_steps_by_n_days() {
        let rule = PaymentRule {
            start_dttm: Some(d("2022-08-01")),
            period_cd: parse_period_cd("45D"),
            day_num: None,
        };
        let got = in_window(&rule, d("2022-08-31"), d("2023-01-31"));
        assert_eq!(
            got,
            vec![
                d("2022-09-15"),
                d("2022-10-30"),
                d("2022-12-14"),
                d("2023-01-28")
            ]
        );
    }

    #[test]
    fn enddate_and_empty_rule_give_no_dates() {
        // ENDDATE и пустой период дат не дают — Tm добавляют вызывающие (§2.2)
        let rule = PaymentRule {
            start_dttm: Some(d("2022-08-16")),
            period_cd: parse_period_cd("ENDDATE"),
            day_num: None,
        };
        assert_eq!(
            rule_schedule(&rule, d("2023-03-14")),
            Vec::<NaiveDate>::new()
        );
        let empty = PaymentRule::default();
        assert_eq!(
            rule_schedule(&empty, d("2023-03-14")),
            Vec::<NaiveDate>::new()
        );
    }

    #[test]
    fn date_list_sorts_and_dedups() {
        let got = parse_date_list("2022-10-31 2022-09-30 2022-10-31").unwrap();
        assert_eq!(got, vec![d("2022-09-30"), d("2022-10-31")]);
        assert!(parse_date_list("2022-13-01").is_err());
    }

    #[test]
    fn pairs_sort_and_same_date_amounts_sum() {
        let got = parse_pairs("2022-11-30,52250.0 2022-10-31,50000.0 2022-11-30,53000.0").unwrap();
        assert_eq!(
            got,
            vec![(d("2022-10-31"), 50000.0), (d("2022-11-30"), 105250.0)]
        );
        assert!(parse_pairs("2022-11-30;52250.0").is_err());
    }
}
