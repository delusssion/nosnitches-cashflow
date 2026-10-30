//! Субсидия — отдельный поток `stream = sub` (методика §7, гипотеза H4).
//!
//! Ставка субсидии: FLOAT — `sub_coeff_index` (пусто → 1) × индикатор
//! (фиксинг на последнем пересмотре субсидии, лаг, H3) + `sub_interest_rate`,
//! затем sub_floor/sub_cap; FIXED — только `sub_interest_rate` (индикатор и
//! коэффициент игнорируются, сделки 52/54). База начисления — остаток тела
//! (путь остатка берётся из клиентских событий). Даты выплат — правило
//! `sub_payment_rule` плюс дата погашения; начисление — с последней даты
//! субсидии ≤ T0, а если правило началось после T0 — с его начала (сделка
//! 50). События: `sub_payment` и `sub_repricing`; суммы со знаком сделки.

use std::collections::BTreeSet;

use chrono::NaiveDate;

use crate::dates;
use crate::daycount::year_fraction;
use crate::model::{Config, Deal, Event, PaymentType, Stream};
use crate::rates::RateBook;

use super::{indicator_value, is_future};

pub fn build(deal: &Deal, rates: &RateBook, cfg: &Config, client: &[Event]) -> Vec<Event> {
    // Гейт: субсидия есть только при заданном sub_rate_type_cd (словарь §1).
    let is_float = match deal.sub_rate_type_cd.as_deref() {
        Some("FLOAT") => true,
        Some("FIXED") => false,
        _ => return Vec::new(),
    };
    let report = deal.report_dttm;
    let maturity = deal.maturity_dttm;
    let deal_date = deal.deal_dttm.unwrap_or(report);
    let conv = deal.day_convention_cd;
    let tau = |a: NaiveDate, b: NaiveDate| year_fraction(a, b, conv);

    // Даты выплат: правило sub_payment_rule ∪ {Tm} (§7).
    let mut sub_all: BTreeSet<NaiveDate> = dates::rule_schedule(&deal.sub_payment_rule, maturity)
        .into_iter()
        .collect();
    sub_all.insert(maturity);
    let sub_all: Vec<NaiveDate> = sub_all.into_iter().filter(|d| *d <= maturity).collect();
    let sub_future: Vec<NaiveDate> = sub_all
        .iter()
        .copied()
        .filter(|d| is_future(*d, deal, cfg))
        .collect();
    if sub_future.is_empty() {
        return Vec::new();
    }

    // Пересмотры ставки субсидии — только у FLOAT (§7).
    let sub_repricing_all: Vec<NaiveDate> = if is_float {
        dates::rule_schedule(&deal.sub_repricing_rule, maturity)
    } else {
        Vec::new()
    };
    let sub_repricing_future: Vec<NaiveDate> = sub_repricing_all
        .iter()
        .copied()
        .filter(|d| is_future(*d, deal, cfg) && *d < maturity)
        .collect();

    let sub_leg_at = |t: NaiveDate| -> (f64, Option<f64>) {
        if !is_float {
            // FIXED: только надбавка, индикатор и коэффициент игнорируются
            return (deal.sub_interest_rate.unwrap_or(0.0), None);
        }
        let fixing = sub_repricing_all
            .iter()
            .rev()
            .find(|d| **d <= t)
            .copied()
            .unwrap_or(deal_date)
            .max(deal_date);
        let base = indicator_value(
            rates,
            cfg,
            report,
            fixing,
            deal.sub_float_indicator
                .as_deref()
                .or(deal.base_indicator.as_deref()),
            deal.sub_indicator_lag_cd.as_deref(),
            deal.base_indicator_rate,
        );
        let coeff = deal.sub_coeff_index.unwrap_or(1.0); // H4: пусто → 1
        let rate = super::float::clamp_rate(
            coeff * base + deal.sub_interest_rate.unwrap_or(0.0),
            deal.sub_floor_limit_interest_rate,
            deal.sub_cap_limit_interest_rate,
        );
        (rate, Some(base))
    };

    // Начало начисления (§7): последняя дата субсидии ≤ T0, иначе начало
    // правила, но не раньше даты сделки (сделка 50).
    let rule_start = deal.sub_payment_rule.start_dttm.unwrap_or(deal_date);
    let sub_start = sub_all
        .iter()
        .copied()
        .rfind(|d| !is_future(*d, deal, cfg))
        .unwrap_or_else(|| rule_start.max(deal_date));

    let (mut rate, mut base) = sub_leg_at(report);
    let mut accrued = 0.0;
    let mut cursor = sub_start;
    if cursor < report {
        // проценты прошлого куска периода — по ставкам того времени
        let mut points: Vec<NaiveDate> = vec![cursor, report];
        points.extend(
            sub_repricing_all
                .iter()
                .copied()
                .filter(|d| *d > cursor && *d < report),
        );
        points.sort();
        points.dedup();
        let balance0 = deal.current_principal_amt;
        for w in 0..points.len().saturating_sub(1) {
            accrued += balance0 * sub_leg_at(points[w]).0 * tau(points[w], points[w + 1]);
        }
        cursor = report;
    }
    let mut last_sub = sub_start;

    // Путь остатка тела из клиентских событий (база начисления — H4).
    let mut steps: Vec<(NaiveDate, f64)> = vec![(report, deal.current_principal_amt)];
    for e in client {
        if matches!(
            e.payment_type,
            PaymentType::PrincipalPayment | PaymentType::ClosePayment
        ) {
            steps.push((e.payment_dttm, e.principal_after));
        }
    }
    let balance_at = |d: NaiveDate| -> f64 {
        steps
            .iter()
            .rev()
            .find(|(sd, _)| *sd <= d)
            .map(|(_, b)| *b)
            .unwrap_or(deal.current_principal_amt)
    };
    let mut balance = balance_at(cursor);

    // Точки: даты субсидии, её пересмотры и платежи тела (они меняют базу).
    let body_dates: BTreeSet<NaiveDate> = client
        .iter()
        .filter(|e| {
            matches!(
                e.payment_type,
                PaymentType::PrincipalPayment | PaymentType::ClosePayment
            )
        })
        .map(|e| e.payment_dttm)
        .collect();
    let points: BTreeSet<NaiveDate> = sub_future
        .iter()
        .chain(sub_repricing_future.iter())
        .chain(body_dates.iter())
        .copied()
        .collect();

    let mut events: Vec<Event> = Vec::new();
    let (mut rate_used, mut base_used) = (rate, base);
    for p in points {
        if cursor < p {
            accrued += balance * rate * tau(cursor, p);
            rate_used = rate;
            base_used = base;
            cursor = p;
        }
        if sub_repricing_future.binary_search(&p).is_ok() {
            (rate, base) = sub_leg_at(p);
            // остаток до платежа тела этой же даты (тело гасится после субсидии)
            events.push(Event {
                event_order: 0,
                payment_dttm: p,
                payment_type: PaymentType::SubRepricing,
                stream: Stream::Sub,
                amount: 0.0,
                principal_before: balance,
                principal_after: balance,
                rate: Some(rate),
                base_rate: base,
                accrual_start_dttm: None,
                accrual_end_dttm: None,
                accrual_days: None,
                year_frac: None,
            });
        }
        if sub_future.binary_search(&p).is_ok() {
            events.push(Event {
                event_order: 0,
                payment_dttm: p,
                payment_type: PaymentType::SubPayment,
                stream: Stream::Sub,
                amount: accrued,
                principal_before: balance,
                principal_after: balance,
                rate: Some(rate_used),
                base_rate: base_used,
                accrual_start_dttm: Some(last_sub),
                accrual_end_dttm: Some(p),
                accrual_days: Some((p - last_sub).num_days()),
                year_frac: Some(year_fraction(last_sub, p, conv)),
            });
            accrued = 0.0;
            last_sub = p;
        }
        if body_dates.contains(&p) {
            balance = balance_at(p);
            if balance == 0.0 {
                break; // тело закрыто — субсидия кончилась
            }
        }
    }

    // Хвост субсидии перед close: тело закрылось в дату без субсидии (§5, 3.5).
    if let Some(close_ev) = client
        .iter()
        .rev()
        .find(|e| e.payment_type == PaymentType::ClosePayment)
    {
        let c = close_ev.payment_dttm;
        if !sub_future.contains(&c) && accrued != 0.0 {
            events.push(Event {
                event_order: 0,
                payment_dttm: c,
                payment_type: PaymentType::SubPayment,
                stream: Stream::Sub,
                amount: accrued,
                principal_before: close_ev.principal_before,
                principal_after: close_ev.principal_before,
                rate: Some(rate_used),
                base_rate: base_used,
                accrual_start_dttm: Some(last_sub),
                accrual_end_dttm: Some(c),
                accrual_days: Some((c - last_sub).num_days()),
                year_frac: Some(year_fraction(last_sub, c, conv)),
            });
        }
    }

    events
}
