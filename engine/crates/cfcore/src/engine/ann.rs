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
//! где `B` — остаток тела, `n` — число оставшихся платежей (включая текущий).
//! Проценты внутри периода начисляются по конвенции сделки (не по `i`!),
//! платёж тела равен `A − проценты`; поэтому последняя выплата отличается
//! от `A` на накопленную разницу day-count'ов и делается `close_payment` на
//! весь остаток. Накопленные к дате расчёта проценты добавляются к первому
//! платежу. A перевычисляется на дате платежа, если ставка изменилась
//! репрайсингом.

use chrono::NaiveDate;

use crate::model::{Config, Deal, Event, PaymentType, PeriodCd, Stream};
use crate::rates::RateBook;

use super::{deal_rate_at, repricing_plan, Segment, MONEY_TOL};

pub fn build(deal: &Deal, rates: &RateBook, cfg: &Config) -> Vec<Event> {
    let sign: f64 = if deal.current_principal_amt < 0.0 {
        -1.0
    } else {
        1.0
    };
    let mut balance = deal.current_principal_amt.abs();

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
    let repricing: Vec<NaiveDate> = repricing_plan(deal)
        .into_iter()
        .filter(|d| !is_past(*d) && *d <= deal.maturity_dttm)
        .collect();

    let (mut rate, mut base) = deal_rate_at(deal, rates, cfg, deal.report_dttm);
    let mut annual = rate; // годовая ставка, на которой стоит текущий A
    let mut segs: Vec<Segment> = vec![Segment {
        start: deal.report_dttm,
        balance,
        rate,
        base,
    }];
    let mut accrued_left = deal.accrued_interest;

    // H7: A = B·i/(1−(1+i)^(−n)) от остатка на дату расчёта
    let n_total = pay_dates.len();
    let annuity_from = |bal: f64, annual_rate: f64, n: f64| {
        let i = period_rate(deal, annual_rate);
        if i.abs() > 1e-12 {
            bal * i / (1.0 - (1.0 + i).powf(-n))
        } else {
            bal / n
        }
    };
    let mut annuity = annuity_from(balance, annual, n_total as f64);

    let mut events: Vec<Event> = Vec::new();
    let mut prev = deal.report_dttm; // начало текущего периода начисления
    let (mut pi, mut ri) = (0usize, 0usize);
    let mut pay_idx = 0usize; // сколько платежей уже сделано

    while pi < pay_dates.len() || ri < repricing.len() {
        let dp = pay_dates.get(pi).copied().unwrap_or(NaiveDate::MAX);
        let dr = repricing.get(ri).copied().unwrap_or(NaiveDate::MAX);
        let d = dp.min(dr);

        // репрайсинг раньше платежа (порядок контракта); новая ставка
        // действует С этой даты, период до неё начислен по старой
        if dr == d && balance > 0.0 {
            ri += 1;
            (rate, base) = deal_rate_at(deal, rates, cfg, d);
            events.push(Event {
                event_order: 0,
                payment_dttm: d,
                payment_type: PaymentType::Repricing,
                stream: Stream::Client,
                amount: 0.0,
                principal_before: sign * balance,
                principal_after: sign * balance,
                rate: Some(rate),
                base_rate: base,
                accrual_start_dttm: None,
                accrual_end_dttm: None,
                accrual_days: None,
                year_frac: None,
            });
            segs.push(Segment {
                start: d,
                balance,
                rate,
                base,
            });
        } else if dr == d {
            ri += 1;
        }

        if dp == d {
            pi += 1;
            // H7: ставка изменилась — аннуитет пересчитывается от текущего
            // остатка на оставшиеся платежи (включая текущий)
            if (rate - annual).abs() > 1e-12 {
                annual = rate;
                annuity = annuity_from(balance, annual, (n_total - pay_idx) as f64);
            }
            let accrual = super::segment_accrual(&segs, d, deal.day_convention_cd);
            let interest = accrual + std::mem::take(&mut accrued_left);
            let (seg_rate, seg_base) = {
                let last = segs.last().expect("секция начисления всегда есть");
                (last.rate, last.base)
            };
            if interest.abs() > 1e-9 {
                events.push(Event {
                    event_order: 0,
                    payment_dttm: d,
                    payment_type: PaymentType::InterestPayment,
                    stream: Stream::Client,
                    amount: sign * interest,
                    principal_before: sign * balance,
                    principal_after: sign * balance,
                    rate: Some(seg_rate),
                    base_rate: seg_base,
                    accrual_start_dttm: Some(prev),
                    accrual_end_dttm: Some(d),
                    accrual_days: Some((d - prev).num_days()),
                    year_frac: Some(crate::daycount::year_fraction(
                        prev,
                        d,
                        deal.day_convention_cd,
                    )),
                });
            }
            segs.clear();
            segs.push(Segment {
                start: d,
                balance,
                rate,
                base,
            });

            let principal = (annuity - accrual).clamp(0.0, balance);
            if principal > MONEY_TOL {
                balance -= principal;
                events.push(Event {
                    event_order: 0,
                    payment_dttm: d,
                    payment_type: PaymentType::PrincipalPayment,
                    stream: Stream::Client,
                    amount: sign * principal,
                    principal_before: sign * (balance + principal),
                    principal_after: sign * balance,
                    rate: Some(rate),
                    base_rate: base,
                    accrual_start_dttm: None,
                    accrual_end_dttm: None,
                    accrual_days: None,
                    year_frac: None,
                });
                // новый остаток — новая секция начисления
                segs.push(Segment {
                    start: d,
                    balance,
                    rate,
                    base,
                });
            }
            pay_idx += 1;
            prev = d;
        }
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

    #[test]
    fn annuity_recalculated_after_rate_change() {
        let mut dl = ann_deal();
        // FLOAT без истории: индикатор 0.12 из сделки, маржа 0 -> ставка 0.12
        dl.rate_type_cd = crate::model::RateType::Float;
        dl.interest_rate = 0.0;
        dl.base_indicator_rate = Some(0.12);
        dl.maturity_dttm = d("2022-11-30");
        dl.interest_payment = vec![d("2022-09-30"), d("2022-10-31"), d("2022-11-30")];
        // с 15.10 маржа 0.12 -> ставка 0.24; репрайсинг между платежами
        dl.repricing_dttm_rate_schedule_fixed = vec![(d("2022-10-15"), 0.12)];
        dl.repricing_dttm_schedule = vec![d("2022-10-15")];

        let (events, _) = crate::engine::build_cashflow(&dl, &RateBook::new(), &cfg());
        let repr: Vec<&crate::model::Event> = events
            .iter()
            .filter(|e| e.payment_type == PaymentType::Repricing)
            .collect();
        assert_eq!(repr.len(), 1);
        assert_eq!(repr[0].payment_dttm, d("2022-10-15"));
        assert_eq!(repr[0].rate, Some(0.24));

        // A1: 3 платежа при 1% в месяц
        let a1 = 1_000_000.0 * 0.01 / (1.0 - 1.01f64.powf(-3.0));
        let accrual1 = 1_000_000.0 * 0.12 * 30.0 / 365.0;
        let bal1 = 1_000_000.0 - (a1 - accrual1);
        // A2: пересчёт от остатка на 2 оставшихся платежа при 2% в месяц
        let a2 = bal1 * 0.02 / (1.0 - 1.02f64.powf(-2.0));

        let totals: Vec<f64> = events
            .iter()
            .filter(|e| e.payment_type != PaymentType::Repricing)
            .map(|e| e.amount)
            .collect();
        // события: инт1, тел1 (30.09), репрайсинг (15.10), инт2, тел2 (31.10),
        // инт3 + close (30.11)
        assert_eq!(events.len(), 7);
        // первый платёж — по A1 (проценты 30 дней по 0.12)
        assert!((totals[0] + totals[1] - a1).abs() < 1e-6);
        // второй платёж уже по новому A2: секции 15 дней по 0.12 и 16 по 0.24
        let accrual2 = bal1 * (0.12 * 15.0 / 365.0 + 0.24 * 16.0 / 365.0);
        let bal2 = bal1 - (a2 - accrual2);
        assert!((totals[2] + totals[3] - a2).abs() < 1e-6);
        // третий платёж: проценты за 30 дней по 0.24, тело — close на весь
        // остаток (он отличается от A2 на накопленную разницу day-count'ов)
        let accrual3 = bal2 * 0.24 * 30.0 / 365.0;
        assert!((totals[4] - accrual3).abs() < 1e-6);
        assert!((totals[5] - bal2).abs() < 1e-6);
        // close гасит остаток, тело сходится
        assert_eq!(
            events.last().unwrap().payment_type,
            PaymentType::ClosePayment
        );
        assert_eq!(events.last().unwrap().principal_after, 0.0);
        let body: f64 = totals.iter().skip(1).step_by(2).sum();
        assert!((body - 1_000_000.0).abs() < 1e-6);
    }
}
