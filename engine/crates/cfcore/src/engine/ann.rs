//! Аннуитет (ANN): тело и проценты одним платежом, но в выводе —
//! двумя строками в одну дату: `interest_payment` и `principal_payment`
//! (как в демо-данных фронтенда).
//!
//! Гипотеза H7: аннуитет A пересчитывается от остатка на дату расчёта и
//! при каждом репрайсинге; ставка периода `i = годовая × месяцы периода / 12`:
//!
//! ```text
//! A = B · i / (1 − (1 + i)^(−n))
//! ```
//!
//! где `B` — остаток тела, `n` — число оставшихся платежей. Проценты внутри
//! периода начисляются по конвенции сделки (не по `i`!), платёж тела равен
//! `A − проценты`; поэтому последняя выплата отличается от `A` на
//! накопленную разницу day-count'ов и делается `close_payment` на весь
//! остаток. Накопленные к дате расчёта проценты добавляются к первому
//! платежу. Пересчёт A при репрайсинге — в engine/float.

use chrono::NaiveDate;

use crate::daycount::year_fraction;
use crate::model::{Config, Deal, Event, PaymentType, PeriodCd, Stream};
use crate::rates::RateBook;

use super::{deal_rate, MONEY_TOL};

pub fn build(deal: &Deal, rates: &RateBook, cfg: &Config) -> Vec<Event> {
    let sign: f64 = if deal.current_principal_amt < 0.0 {
        -1.0
    } else {
        1.0
    };
    let mut balance = deal.current_principal_amt.abs();
    let annual = deal_rate(deal, rates, cfg, deal.report_dttm);

    // H6: событие в дату расчёта — прошлое
    let is_past = |d: NaiveDate| {
        if cfg.event_on_report_date_is_past {
            d <= deal.report_dttm
        } else {
            d < deal.report_dttm
        }
    };
    let mut pay_dates: Vec<NaiveDate> = annuity_dates(deal)
        .into_iter()
        .filter(|d| !is_past(*d) && *d <= deal.maturity_dttm)
        .collect();
    pay_dates.sort_unstable();
    pay_dates.dedup();

    // единственная дата — это bullet, а не аннуитет: считаем как SIMPLE/RGM
    if pay_dates.len() <= 1 {
        return super::build_events(deal, rates, cfg);
    }

    // H7: A = B·i/(1−(1+i)^(−n))
    let n = pay_dates.len() as f64;
    let i = period_rate(deal, annual);
    let annuity = if i.abs() > 1e-12 {
        balance * i / (1.0 - (1.0 + i).powf(-n))
    } else {
        balance / n
    };

    let mut events: Vec<Event> = Vec::new();
    let mut prev = deal.report_dttm;
    let mut accrued_left = deal.accrued_interest;

    for &dt in pay_dates.iter() {
        let accrual = balance * annual * year_fraction(prev, dt, deal.day_convention_cd);
        let interest = accrual + std::mem::take(&mut accrued_left);
        if interest.abs() > 1e-9 {
            events.push(Event {
                event_order: 0,
                payment_dttm: dt,
                payment_type: PaymentType::InterestPayment,
                stream: Stream::Client,
                amount: sign * interest,
                principal_before: sign * balance,
                principal_after: sign * balance,
                rate: Some(annual),
                base_rate: None,
                accrual_start_dttm: Some(prev),
                accrual_end_dttm: Some(dt),
                accrual_days: Some((dt - prev).num_days()),
                year_frac: Some(year_fraction(prev, dt, deal.day_convention_cd)),
            });
        }

        let principal = (annuity - accrual).clamp(0.0, balance);
        if principal > MONEY_TOL {
            balance -= principal;
            events.push(Event {
                event_order: 0,
                payment_dttm: dt,
                payment_type: PaymentType::PrincipalPayment,
                stream: Stream::Client,
                amount: sign * principal,
                principal_before: sign * (balance + principal),
                principal_after: sign * balance,
                rate: Some(annual),
                base_rate: None,
                accrual_start_dttm: None,
                accrual_end_dttm: None,
                accrual_days: None,
                year_frac: None,
            });
        }
        prev = dt;
    }

    // Последняя фактическая выплата тела — close_payment на весь остаток.
    // Из-за разницы day-count'ов остаток может исчерпаться раньше
    // последней даты сетки (сделка 1: 3,6 млн на 28 лет), поэтому тип
    // ставится по факту, а не по расписанию.
    if let Some(idx) = events
        .iter()
        .rposition(|e| e.payment_type == PaymentType::PrincipalPayment)
    {
        let close = &mut events[idx];
        close.payment_type = PaymentType::ClosePayment;
        close.amount = close.principal_before;
        close.principal_after = 0.0;
    }

    events
}

/// Даты аннуитетных платежей: явный список, если задан; иначе сетка правила
/// процентов; иначе сетка правила тела (у сделок 1–3 в данных заполнено
/// только `regular_payment_rule`).
fn annuity_dates(deal: &Deal) -> Vec<NaiveDate> {
    if !deal.interest_payment.is_empty() {
        return deal.interest_payment.clone();
    }
    let grid = crate::dates::rule_schedule(
        &deal.interest_payment_rule,
        deal.report_dttm,
        deal.maturity_dttm,
    );
    if !grid.is_empty() {
        return grid;
    }
    crate::dates::rule_schedule(
        &deal.regular_payment_rule,
        deal.report_dttm,
        deal.maturity_dttm,
    )
}

/// Ставка периода для формулы аннуитета (H7): месяцы периода / 12.
fn period_rate(deal: &Deal, annual: f64) -> f64 {
    let period = deal
        .interest_payment_rule
        .period_cd
        .or(deal.regular_payment_rule.period_cd);
    match period {
        Some(PeriodCd::Months(m)) => annual * f64::from(m) / 12.0,
        Some(PeriodCd::Days(d)) => annual * f64::from(d) / 365.0,
        _ => annual / 12.0,
    }
}

#[cfg(test)]
mod tests {
    use crate::engine::testing::{base_deal, cfg, d};
    use crate::engine::MONEY_TOL;
    use crate::model::{Discriminator, PaymentRule, PaymentType};
    use crate::rates::RateBook;

    use super::{annuity_dates, period_rate};

    fn ann_deal() -> crate::model::Deal {
        let mut dl = base_deal();
        dl.discriminator = Discriminator::Ann;
        dl.maturity_dttm = d("2024-01-15");
        let rule = PaymentRule {
            start_dttm: Some(d("2022-01-15")),
            period_cd: crate::dates::parse_period_cd("1M"),
            day_num: Some(15),
        };
        dl.interest_payment_rule = rule.clone();
        dl.regular_payment_rule = rule;
        dl
    }

    #[test]
    fn annuity_two_rows_per_date_with_accrued() {
        let mut dl = ann_deal();
        dl.accrued_interest = 123.45;
        let (events, errs) = crate::engine::build_cashflow(&dl, &RateBook::new(), &cfg());
        assert!(errs.is_empty(), "{errs:?}");

        // сетка 15-х чисел: 2022-09-15 .. 2024-01-15 = 17 дат × 2 строки
        assert_eq!(events.len(), 34);
        assert_eq!(events[0].payment_dttm, d("2022-09-15"));
        assert_eq!(events[0].payment_type, PaymentType::InterestPayment);
        assert_eq!(events[1].payment_type, PaymentType::PrincipalPayment);

        // H7: A = B·i/(1−(1+i)^(−n)), i = 0.12/12, n = 17
        let a = 1_000_000.0 * 0.01 / (1.0 - 1.01f64.powf(-17.0));
        // первая строка: проценты 15 дней + накопленные; тело = A − проценты
        let accrual1 = 1_000_000.0 * 0.12 * 15.0 / 365.0;
        assert!((events[0].amount - (accrual1 + 123.45)).abs() < 1e-6);
        assert!((events[1].amount - (a - accrual1)).abs() < 1e-6);
        assert!((events[1].principal_after - (1_000_000.0 - (a - accrual1))).abs() < 1e-6);
        // за первую дату клиент платит A + накопленные
        assert!((events[0].amount + events[1].amount - (a + 123.45)).abs() < 1e-6);

        // вторая дата: проценты на новый остаток за 30 дней
        let bal1 = 1_000_000.0 - (a - accrual1);
        let accrual2 = bal1 * 0.12 * 30.0 / 365.0;
        assert_eq!(events[2].payment_dttm, d("2022-10-15"));
        assert!((events[2].amount - accrual2).abs() < 1e-6);

        // последняя строка — close_payment на весь остаток, ноль строго
        let last = events.last().unwrap();
        assert_eq!(last.payment_dttm, d("2024-01-15"));
        assert_eq!(last.payment_type, PaymentType::ClosePayment);
        assert_eq!(last.principal_after, 0.0);
        // сумма по телу равна остатку
        let body: f64 = events
            .iter()
            .filter(|e| e.payment_type != PaymentType::InterestPayment)
            .map(|e| e.amount)
            .sum();
        assert!((body - 1_000_000.0).abs() < 1e-6);
    }

    #[test]
    fn annuity_zero_rate_splits_evenly() {
        let mut dl = ann_deal();
        dl.interest_rate = 0.0;
        dl.interest_payment = vec![d("2022-09-30"), d("2022-10-31"), d("2022-11-30")];
        dl.maturity_dttm = d("2022-11-30");
        let (events, _) = crate::engine::build_cashflow(&dl, &RateBook::new(), &cfg());
        // нулевая ставка: процентные строки нулевые и не выводятся
        assert_eq!(events.len(), 3);
        assert!(events
            .iter()
            .all(|e| e.payment_type != PaymentType::InterestPayment));
        let last = events.last().unwrap();
        assert_eq!(last.payment_type, PaymentType::ClosePayment);
        assert_eq!(last.principal_after, 0.0);
        let body: f64 = events.iter().map(|e| e.amount).sum();
        assert!((body - 1_000_000.0).abs() < 1e-6);
    }

    #[test]
    fn annuity_single_date_delegates_to_bullet() {
        let mut dl = ann_deal();
        dl.interest_payment_rule.period_cd = crate::dates::parse_period_cd("ENDDATE");
        dl.regular_payment_rule.period_cd = crate::dates::parse_period_cd("ENDDATE");
        let (events, _) = crate::engine::build_cashflow(&dl, &RateBook::new(), &cfg());
        // одна дата: проценты за весь срок + close, без деления на доли
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].payment_type, PaymentType::InterestPayment);
        assert_eq!(events[1].payment_type, PaymentType::ClosePayment);
        assert_eq!(events[1].principal_after, 0.0);
    }

    #[test]
    fn annuity_dates_priority_and_period_rate() {
        let mut dl = ann_deal();
        // явный список приоритетнее правил
        dl.interest_payment = vec![d("2022-09-30"), d("2022-10-31")];
        assert_eq!(annuity_dates(&dl), vec![d("2022-09-30"), d("2022-10-31")]);

        // у сделок 1–3 правило процентов пустое — берётся правило тела
        let mut dl2 = base_deal();
        dl2.discriminator = Discriminator::Ann;
        dl2.regular_payment_rule = PaymentRule {
            start_dttm: Some(d("2020-06-22")),
            period_cd: crate::dates::parse_period_cd("1M"),
            day_num: None,
        };
        assert!(!annuity_dates(&dl2).is_empty());

        // ставка периода: 3M -> годовая/4, 45D -> годовая × 45/365, пусто -> /12
        let mut dl3 = base_deal();
        dl3.interest_payment_rule.period_cd = crate::dates::parse_period_cd("3M");
        assert!((period_rate(&dl3, 0.12) - 0.03).abs() < MONEY_TOL);
        dl3.interest_payment_rule.period_cd = crate::dates::parse_period_cd("45D");
        assert!((period_rate(&dl3, 0.365) - 0.045).abs() < 1e-12);
        dl3.interest_payment_rule.period_cd = None;
        assert!((period_rate(&dl3, 0.12) - 0.01).abs() < 1e-12);
    }
}
