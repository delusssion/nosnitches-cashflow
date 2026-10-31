//! Движки расписаний: ANN, RGM, SIMPLE + плавающая ставка и субсидия.
//!
//! Реализация следует docs/METHODOLOGY.md (ссылки § — по нему); эталон —
//! analytics/reference/cf_reference.py. Ядро — [`build_cashflow`]: чистая
//! функция без состояния, одинаковые входы дают одинаковый выход, поэтому
//! её можно звать из любого числа потоков.
//!
//! Модель начисления (§4–§5): по объединённым будущим датам событий идёт
//! курсор `cursor`; на отрезке `[cursor, p)` проценты копятся в `accrued`
//! по текущим остатку и ставке; в дату процентов `accrued` выплачивается и
//! обнуляется; репрайсинг с даты `p` меняет ставку; тело гасится по
//! алгоритму дискриминатора; последнее событие тела — `close_payment`.

pub mod ann;
pub mod float;
pub mod rgm;
pub mod simple;
pub mod subsidy;

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;

use crate::dates;
use crate::daycount::year_fraction;
use crate::model::{
    Config, DayConvention, Deal, DealError, Discriminator, Event, PaymentType, PeriodCd, RateType,
    Severity, Stream,
};
use crate::rates::RateBook;
use crate::validate;

/// Расчёт кэшфлоу одной сделки: клиентский поток + субсидия (§7), сортировка
/// по дате и рангу события в дату, сквозная нумерация `event_order`.
pub fn build_cashflow(deal: &Deal, rates: &RateBook, cfg: &Config) -> (Vec<Event>, Vec<DealError>) {
    let errors = validate::validate(deal);
    if errors.iter().any(|e| e.severity == Severity::Error) {
        return (Vec::new(), errors);
    }
    let mut events = run_timeline(deal, rates, cfg);
    events.extend(subsidy::build(deal, rates, cfg, &events));
    events.sort_by(|a, b| {
        a.payment_dttm.cmp(&b.payment_dttm).then(
            a.payment_type
                .order_rank()
                .cmp(&b.payment_type.order_rank()),
        )
    });
    for (i, e) in events.iter_mut().enumerate() {
        e.event_order = i as u32 + 1;
    }
    (events, errors)
}

// ---------- даты (§2) ----------------------------------------------------------------------

/// H6: событие ровно в дату расчёта — прошлое.
fn is_future(d: NaiveDate, deal: &Deal, cfg: &Config) -> bool {
    if cfg.event_on_report_date_is_past {
        d > deal.report_dttm
    } else {
        d >= deal.report_dttm
    }
}

/// Все даты процентов, прошлые и будущие (§2.1): явный список → правило
/// процентов → правило тела (не ENDDATE) → даты явного графика тела;
/// всегда плюс дата погашения. Даты позже Tm отбрасываются.
fn interest_dates_all(deal: &Deal) -> Vec<NaiveDate> {
    let mut dates: BTreeSet<NaiveDate> = if !deal.interest_payment.is_empty() {
        deal.interest_payment.iter().copied().collect()
    } else if deal.interest_payment_rule.period_cd.is_some() {
        dates::rule_schedule(&deal.interest_payment_rule, deal.maturity_dttm)
            .into_iter()
            .collect()
    } else if matches!(
        deal.regular_payment_rule.period_cd,
        Some(PeriodCd::Months(_)) | Some(PeriodCd::Days(_))
    ) {
        dates::rule_schedule(&deal.regular_payment_rule, deal.maturity_dttm)
            .into_iter()
            .collect()
    } else if !deal.principal_payment.is_empty() {
        deal.principal_payment.iter().map(|(d, _)| *d).collect()
    } else {
        BTreeSet::new()
    };
    dates.insert(deal.maturity_dttm);
    dates
        .into_iter()
        .filter(|d| *d <= deal.maturity_dttm)
        .collect()
}

/// Все даты тела ANN/RGM (§2.1): правило тела (ENDDATE — пусто), иначе даты
/// процентов; всегда плюс дата погашения. Явный график тела у ANN/RGM не
/// используется (словарь §1: поле только для SIMPLE).
fn principal_dates_all(deal: &Deal, interest_all: &[NaiveDate]) -> Vec<NaiveDate> {
    let mut dates: BTreeSet<NaiveDate> = match deal.regular_payment_rule.period_cd {
        Some(PeriodCd::Months(_)) | Some(PeriodCd::Days(_)) => {
            dates::rule_schedule(&deal.regular_payment_rule, deal.maturity_dttm)
                .into_iter()
                .collect()
        }
        Some(PeriodCd::EndDate) => BTreeSet::new(),
        None => interest_all.iter().copied().collect(),
    };
    dates.insert(deal.maturity_dttm);
    dates
        .into_iter()
        .filter(|d| *d <= deal.maturity_dttm)
        .collect()
}

/// Все даты пересмотра ставки клиента (§2.1): явный список ∪ правило.
/// Прошлые даты нужны для даты фиксинга (§6.2).
pub(crate) fn client_repricing_all(deal: &Deal) -> Vec<NaiveDate> {
    let mut out: BTreeSet<NaiveDate> = deal.repricing_dttm_schedule.iter().copied().collect();
    if deal.repricing_rule.period_cd.is_some() {
        out.extend(dates::rule_schedule(
            &deal.repricing_rule,
            deal.maturity_dttm,
        ));
    }
    out.into_iter().collect()
}

// ---------- ставки (§6) --------------------------------------------------------------------

/// Значение индикатора на дату фиксинга (§6.3): история со сдвигом на лаг;
/// для дат позже даты расчёта — последнее известное на T0 + сдвиг сценария
/// (H3). Если истории для кода нет вовсе — `base_indicator_rate` из сделки
/// (Q2 открыт).
fn indicator_value(
    rates: &RateBook,
    cfg: &Config,
    report: NaiveDate,
    fixing: NaiveDate,
    code: Option<&str>,
    lag: Option<&str>,
    fallback: Option<f64>,
) -> f64 {
    let lag_months = lag.and_then(dates::parse_lag_months).unwrap_or(0);
    match code.and_then(|c| rates.get(c)) {
        Some(series) => series.fixing(fixing, lag_months, report, cfg.scenario_shift),
        None => fallback.unwrap_or(0.0),
    }
}

/// Ставка клиентского потока в момент `t` (§6): FIXED — график фиксированной
/// части или `interest_rate` (без floor/cap, как в эталоне); FLOAT —
/// индикатор на дате фиксинга (последний пересмотр ≤ t, но не раньше даты
/// сделки) плюс маржа, затем floor/cap. Возвращает (ставка, индикатор).
fn client_leg_at(
    deal: &Deal,
    rates: &RateBook,
    cfg: &Config,
    repricing_all: &[NaiveDate],
    t: NaiveDate,
) -> (f64, Option<f64>) {
    let deal_date = deal.deal_dttm.unwrap_or(deal.report_dttm);
    let spread_at = |t: NaiveDate| {
        deal.repricing_dttm_rate_schedule_fixed
            .iter()
            .rev()
            .find(|(d, _)| *d <= t)
            .map(|(_, r)| *r)
            .unwrap_or(deal.interest_rate)
    };
    match deal.rate_type_cd {
        RateType::Fixed => (spread_at(t), None),
        RateType::Float => {
            let fixing = repricing_all
                .iter()
                .rev()
                .find(|d| **d <= t)
                .copied()
                .unwrap_or(deal_date)
                .max(deal_date);
            let base = indicator_value(
                rates,
                cfg,
                deal.report_dttm,
                fixing,
                deal.base_indicator.as_deref(),
                deal.indicator_lag_cd.as_deref(),
                deal.base_indicator_rate,
            );
            (
                float::clamp_rate(
                    base + spread_at(t),
                    deal.floor_limit_interest_rate,
                    deal.cap_limit_interest_rate,
                ),
                Some(base),
            )
        }
    }
}

/// Проценты с начала периода до даты расчёта (§4.2): отрезки режутся
/// пересмотрами ставки внутри периода, ставка каждого — фиксинг на его начало.
fn accrue_past(
    deal: &Deal,
    repricing_all: &[NaiveDate],
    leg_at: &dyn Fn(NaiveDate) -> (f64, Option<f64>),
    start: NaiveDate,
    balance: f64,
) -> f64 {
    let report = deal.report_dttm;
    let mut points: Vec<NaiveDate> = vec![start, report];
    points.extend(
        repricing_all
            .iter()
            .copied()
            .filter(|d| *d > start && *d < report),
    );
    points.sort();
    points.dedup();
    let mut total = 0.0;
    for w in 0..points.len().saturating_sub(1) {
        total += balance
            * leg_at(points[w]).0
            * year_fraction(points[w], points[w + 1], deal.day_convention_cd);
    }
    total
}

// ---------- аннуитет (§5.1, H7) ------------------------------------------------------------

/// Длина периода тела в месяцах для ставки периода аннуитета.
fn period_months(deal: &Deal) -> f64 {
    match deal.regular_payment_rule.period_cd {
        Some(PeriodCd::Months(m)) => f64::from(m),
        Some(PeriodCd::Days(d)) => f64::from(d) * 12.0 / 365.0,
        _ => 1.0,
    }
}

/// `A = B · i / (1 − (1+i)^(−n))`, ставка периода `i = r · m / 12` (H7).
fn annuity(balance: f64, annual_rate: f64, months: f64, n: usize) -> f64 {
    let i = annual_rate * months / 12.0;
    if n == 0 {
        return balance;
    }
    if i == 0.0 {
        return balance / f64::from(u32::try_from(n).unwrap_or(u32::MAX));
    }
    let n = n as f64;
    balance * i / (1.0 - (1.0 + i).powf(-n))
}

// ---------- проход по сделке (§5) ----------------------------------------------------------

#[allow(clippy::too_many_arguments)] // конструктор события, зеркалит emit эталона
fn mk_event(
    day: NaiveDate,
    ptype: PaymentType,
    stream: Stream,
    amount: f64,
    before: f64,
    after: f64,
    rate: f64,
    base: Option<f64>,
    acc: Option<(NaiveDate, NaiveDate)>,
    conv: DayConvention,
) -> Event {
    Event {
        event_order: 0,
        payment_dttm: day,
        payment_type: ptype,
        stream,
        amount,
        principal_before: before,
        principal_after: after,
        rate: Some(rate),
        base_rate: base,
        accrual_start_dttm: acc.map(|(a, _)| a),
        accrual_end_dttm: acc.map(|(_, b)| b),
        accrual_days: acc.map(|(a, b)| (b - a).num_days()),
        year_frac: acc.map(|(a, b)| year_fraction(a, b, conv)),
    }
}

/// Клиентский поток сделки — общий цикл для ANN/RGM/SIMPLE (§5).
fn run_timeline(deal: &Deal, rates: &RateBook, cfg: &Config) -> Vec<Event> {
    let report = deal.report_dttm;
    let maturity = deal.maturity_dttm;
    let deal_date = deal.deal_dttm.unwrap_or(report);
    let conv = deal.day_convention_cd;
    let tau = |a: NaiveDate, b: NaiveDate| year_fraction(a, b, conv);
    let mut balance = deal.current_principal_amt; // со знаком сделки
    if balance == 0.0 {
        return Vec::new(); // §5: нулевой остаток — событий нет
    }

    // 1. Даты.
    let interest_all = interest_dates_all(deal);
    let interest_future: Vec<NaiveDate> = interest_all
        .iter()
        .copied()
        .filter(|d| is_future(*d, deal, cfg))
        .collect();
    let accrual_start = interest_all
        .iter()
        .copied()
        .rfind(|d| !is_future(*d, deal, cfg))
        .unwrap_or(deal_date);

    let repricing_all = client_repricing_all(deal);
    let repricing_future: Vec<NaiveDate> = repricing_all
        .iter()
        .copied()
        .filter(|d| is_future(*d, deal, cfg) && *d < maturity)
        .collect();

    let mut planned: BTreeMap<NaiveDate, f64> = BTreeMap::new();
    let principal_future: Vec<NaiveDate> = match deal.discriminator {
        Discriminator::Simple => {
            for (d, a) in &deal.principal_payment {
                if is_future(*d, deal, cfg) {
                    *planned.entry(*d).or_insert(0.0) += a;
                }
            }
            let mut dates: BTreeSet<NaiveDate> =
                planned.keys().copied().filter(|d| *d <= maturity).collect();
            dates.insert(maturity);
            dates.into_iter().collect()
        }
        _ => principal_dates_all(deal, &interest_all)
            .into_iter()
            .filter(|d| is_future(*d, deal, cfg))
            .collect(),
    };
    if principal_future.is_empty() {
        return Vec::new();
    }

    // 2. Состояние на дату расчёта (§4.2, §6.2).
    let (mut rate, mut base) = client_leg_at(deal, rates, cfg, &repricing_all, report);
    let leg_at = |t: NaiveDate| client_leg_at(deal, rates, cfg, &repricing_all, t);

    let mut accrued = 0.0;
    let mut cursor = match deal.accrued_interest {
        Some(v) => {
            // Q13: поле задано — это накопленные проценты на T0 (со знаком)
            accrued = v.abs() * 1.0_f64.copysign(balance);
            report.max(accrual_start)
        }
        None => accrual_start, // досчитаем с начала периода по ставкам того времени
    };
    if cursor < report {
        accrued += accrue_past(deal, &repricing_all, &leg_at, cursor, balance);
        cursor = report;
    }
    let mut last_interest = accrual_start;

    let months = period_months(deal);
    let n_left = principal_future.len();
    let mut ann_payment = if deal.discriminator == Discriminator::Ann {
        annuity(balance, rate, months, n_left)
    } else {
        0.0
    };
    let rgm_share = if deal.discriminator == Discriminator::Rgm && n_left > 0 {
        balance / n_left as f64
    } else {
        0.0
    };

    // 3. Проход по объединённым будущим датам событий.
    let points: BTreeSet<NaiveDate> = interest_future
        .iter()
        .chain(principal_future.iter())
        .chain(repricing_future.iter())
        .copied()
        .collect();
    let mut events: Vec<Event> = Vec::new();
    let mut rate_used: f64;
    let mut base_used: Option<f64>;

    for p in points {
        // 3.1 начисление на отрезке [cursor, p)
        accrued += balance * rate * tau(cursor, p);
        rate_used = rate;
        base_used = base;
        cursor = p;

        // 3.2 пересмотр: новая ставка действует с даты p
        let mut repriced = false;
        if repricing_future.binary_search(&p).is_ok() {
            (rate, base) = client_leg_at(deal, rates, cfg, &repricing_all, p);
            events.push(mk_event(
                p,
                PaymentType::Repricing,
                Stream::Client,
                0.0,
                balance,
                balance,
                rate,
                base,
                None,
                conv,
            ));
            repriced = true;
        }

        // 3.3 проценты за период (last_interest, p]
        let mut interest_paid = 0.0;
        if interest_future.binary_search(&p).is_ok() {
            interest_paid = accrued;
            events.push(mk_event(
                p,
                PaymentType::InterestPayment,
                Stream::Client,
                accrued,
                balance,
                balance,
                rate_used,
                base_used,
                Some((last_interest, p)),
                conv,
            ));
            accrued = 0.0;
            last_interest = p;
        }

        // 3.5 погашение тела
        if principal_future.binary_search(&p).is_ok() {
            let is_last = p == *principal_future.last().expect("список тела непустой");
            let raw = if is_last {
                balance
            } else {
                match deal.discriminator {
                    Discriminator::Ann => ann_payment - interest_paid,
                    Discriminator::Rgm => rgm_share,
                    Discriminator::Simple => planned.get(&p).copied().unwrap_or(0.0),
                }
            };
            // не гасим больше остатка и не «выдаём» тело обратно
            let amount =
                f64::max(0.0, f64::min(raw.abs(), balance.abs())) * (1.0_f64.copysign(balance));
            let closes = is_last || (balance - amount).abs() < 1e-9;
            // тело гасится в дату без процентов — хвост периода платим здесь (§4.3)
            if closes && !interest_future.contains(&p) && accrued != 0.0 {
                events.push(mk_event(
                    p,
                    PaymentType::InterestPayment,
                    Stream::Client,
                    accrued,
                    balance,
                    balance,
                    rate_used,
                    base_used,
                    Some((last_interest, p)),
                    conv,
                ));
                accrued = 0.0;
            }
            let ptype = if closes {
                PaymentType::ClosePayment
            } else {
                PaymentType::PrincipalPayment
            };
            let after = if closes { 0.0 } else { balance - amount };
            events.push(mk_event(
                p,
                ptype,
                Stream::Client,
                amount,
                balance,
                after,
                rate,
                base,
                None,
                conv,
            ));
            balance = if closes { 0.0 } else { balance - amount };
            if balance == 0.0 {
                break;
            }
        }

        // 3.6 ANN: пересчёт платежа после пересмотра ставки (H7)
        if deal.discriminator == Discriminator::Ann && repriced {
            let n = principal_future.iter().filter(|d| **d > p).count();
            ann_payment = annuity(balance, rate, months, n);
        }
    }

    events
}

/// Общие хелперы тестов движков (base_deal меняется в каждом тесте).
#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    use crate::model::PaymentRule;

    pub(crate) fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    /// Минимальная корректная рублёвая FIXED-сделка; тесты меняют поля.
    pub(crate) fn base_deal() -> Deal {
        Deal {
            report_dttm: d("2022-08-31"),
            discriminator: Discriminator::Simple,
            deal_id: "T".into(),
            currency_id: "RUB".into(),
            start_principal_amt: None,
            current_principal_amt: 1_000_000.0,
            deal_dttm: None,
            maturity_dttm: d("2022-12-31"),
            rate_type_cd: RateType::Fixed,
            interest_rate: 0.12,
            base_indicator_rate: None,
            base_indicator: None,
            day_convention_cd: DayConvention::Act365,
            accrued_interest: None,
            cap_limit_interest_rate: None,
            floor_limit_interest_rate: None,
            interest_payment_rule: PaymentRule::default(),
            regular_payment_rule: PaymentRule::default(),
            repricing_rule: PaymentRule::default(),
            interest_payment: Vec::new(),
            principal_payment: Vec::new(),
            repricing_dttm_schedule: Vec::new(),
            repricing_dttm_rate_schedule: Vec::new(),
            repricing_dttm_rate_schedule_fixed: Vec::new(),
            product: None,
            behavior_model_cd: None,
            sub_rate_type_cd: None,
            sub_interest_rate: None,
            sub_float_indicator: None,
            sub_payment_rule: PaymentRule::default(),
            sub_repricing_rule: PaymentRule::default(),
            sub_cap_limit_interest_rate: None,
            sub_floor_limit_interest_rate: None,
            sub_coeff_index: None,
            sub_repricing_rule_first_event_flg: None,
            deal_report_dttm: None,
            indicator_lag_cd: None,
            sub_indicator_lag_cd: None,
        }
    }

    pub(crate) fn cfg() -> Config {
        Config::default()
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{base_deal as deal, cfg, d};
    use super::*;

    #[test]
    fn simple_segments_gap_close_and_accrued() {
        let mut dl = deal();
        dl.accrued_interest = Some(1000.0);
        dl.interest_payment = vec![d("2022-09-30"), d("2022-10-31"), d("2022-11-30")];
        // график покрывает 400 тыс. из 1 млн — добор 600 тыс. в close (H9)
        dl.principal_payment = vec![(d("2022-11-15"), 400_000.0)];

        let (events, errs) = build_cashflow(&dl, &RateBook::new(), &cfg());
        assert!(errs
            .iter()
            .any(|e| e.code == crate::model::codes::SCHEDULE_SUM_MISMATCH));

        let close = |days: f64, bal: f64| bal * 0.12 * days / 365.0;
        // 1: 30.09 — накопленные + начисление на 1 млн за 30 дней
        assert_eq!(events[0].payment_dttm, d("2022-09-30"));
        assert!((events[0].amount - (close(30.0, 1_000_000.0) + 1000.0)).abs() < 1e-6);
        // 2: 31.10 — 31 день на 1 млн
        assert!((events[1].amount - close(31.0, 1_000_000.0)).abs() < 1e-6);
        // 3: 15.11 — тело 400 тыс.
        assert_eq!(events[2].payment_type, PaymentType::PrincipalPayment);
        assert!((events[2].amount - 400_000.0).abs() < 1e-9);
        // 4: 30.11 — секции: 15 дней на 1 млн + 15 дней на 600 тыс.
        assert!(
            (events[3].amount - (close(15.0, 1_000_000.0) + close(15.0, 600_000.0))).abs() < 1e-6
        );
        assert_eq!(events[3].accrual_start_dttm, Some(d("2022-10-31")));
        assert_eq!(events[3].principal_before, 600_000.0);
        // 5–6: Tm — проценты за хвост (31 день на 600 тыс.) и close
        assert_eq!(events[4].payment_type, PaymentType::InterestPayment);
        assert!((events[4].amount - close(31.0, 600_000.0)).abs() < 1e-6);
        assert_eq!(events[5].payment_type, PaymentType::ClosePayment);
        assert!((events[5].amount - 600_000.0).abs() < 1e-9);
        assert_eq!(events[5].principal_after, 0.0);
        let body: f64 = events
            .iter()
            .filter(|e| e.payment_type != PaymentType::InterestPayment)
            .map(|e| e.amount)
            .sum();
        assert!((body - 1_000_000.0).abs() < 1e-6);
    }

    #[test]
    fn rgm_equal_shares_with_close() {
        let mut dl = deal();
        dl.discriminator = Discriminator::Rgm;
        dl.current_principal_amt = 900_000.0;
        dl.interest_rate = 0.0365;
        dl.maturity_dttm = d("2022-11-30");
        let grid = crate::model::PaymentRule {
            start_dttm: Some(d("2022-07-30")),
            period_cd: dates::parse_period_cd("1M"),
            day_num: Some(30),
        };
        dl.regular_payment_rule = grid.clone();
        dl.interest_payment_rule = grid;

        let (events, errs) = build_cashflow(&dl, &RateBook::new(), &cfg());
        assert!(errs.is_empty(), "{errs:?}");
        // сетка от якоря: 30.09, 30.10, 30.11 = 3 равные доли по 300 тыс.
        let shares: Vec<&Event> = events
            .iter()
            .filter(|e| e.payment_type != PaymentType::InterestPayment)
            .collect();
        assert_eq!(shares.len(), 3);
        for s in &shares {
            assert!((s.amount - 300_000.0).abs() < 1e-9);
        }
        let ints: Vec<&Event> = events
            .iter()
            .filter(|e| e.payment_type == PaymentType::InterestPayment)
            .collect();
        assert!(
            (ints[0].amount - 900_000.0 * 0.0365 * 31.0 / 365.0).abs() < 1e-6,
            "период с 30.08 (последняя прошлая дата процентов) — 31 день"
        );
        assert!((ints[1].amount - 600_000.0 * 0.0365 * 30.0 / 365.0).abs() < 1e-6);
        assert!((ints[2].amount - 300_000.0 * 0.0365 * 31.0 / 365.0).abs() < 1e-6);
        assert_eq!(
            events.last().unwrap().payment_type,
            PaymentType::ClosePayment
        );
        assert_eq!(events.last().unwrap().principal_after, 0.0);
    }

    #[test]
    fn deposit_negative_amounts() {
        let mut dl = deal();
        dl.current_principal_amt = -500_000.0;
        dl.interest_rate = 0.1;
        dl.maturity_dttm = d("2022-10-31");

        let (events, _) = build_cashflow(&dl, &RateBook::new(), &cfg());
        assert_eq!(events.len(), 2);
        assert!(
            events.iter().all(|e| e.amount < 0.0),
            "депозит: суммы со знаком минус"
        );
        assert_eq!(events[0].payment_type, PaymentType::InterestPayment);
        assert!((events[0].amount + 500_000.0 * 0.1 * 61.0 / 365.0).abs() < 1e-6);
        assert_eq!(events[1].payment_type, PaymentType::ClosePayment);
        assert!((events[1].amount + 500_000.0).abs() < 1e-9);
        assert_eq!(events[1].principal_after, 0.0);
    }

    #[test]
    fn event_on_report_date_is_past() {
        let mut dl = deal();
        dl.maturity_dttm = d("2022-10-31");
        dl.interest_payment = vec![d("2022-08-31")]; // ровно в дату расчёта (H6)

        let (events, _) = build_cashflow(&dl, &RateBook::new(), &cfg());
        assert!(events.iter().all(|e| e.payment_dttm > dl.report_dttm));
        assert_eq!(events.len(), 2); // проценты за хвост + close
    }

    #[test]
    fn overpaid_schedule_is_capped_and_closes_early() {
        let mut dl = deal();
        dl.maturity_dttm = d("2022-12-31");
        // график больше остатка (кейс сделки 20): платим сколько осталось,
        // остаток исчерпывается — close раньше Tm, остальные даты отбрасываются
        dl.principal_payment = vec![(d("2022-10-31"), 600_000.0), (d("2022-12-31"), 600_000.0)];

        let (events, _) = build_cashflow(&dl, &RateBook::new(), &cfg());
        let body: f64 = events
            .iter()
            .filter(|e| e.payment_type != PaymentType::InterestPayment)
            .map(|e| e.amount)
            .sum();
        assert!((body - 1_000_000.0).abs() < 1e-6, "собрано ровно остаток");
        let last = events.last().unwrap();
        assert_eq!(last.payment_type, PaymentType::ClosePayment);
        assert!((last.amount - 400_000.0).abs() < 1e-6);
        assert_eq!(last.principal_after, 0.0);
    }

    #[test]
    fn float_repricing_via_lag_uses_real_history() {
        let mut dl = deal();
        dl.rate_type_cd = RateType::Float;
        dl.interest_rate = 0.02; // маржа
        dl.base_indicator = Some("CBKEY1M".into());
        dl.indicator_lag_cd = Some("-2M".into());
        dl.maturity_dttm = d("2022-12-31");
        dl.interest_payment = vec![d("2022-09-30"), d("2022-10-31")];
        dl.repricing_dttm_schedule = vec![d("2022-09-15")];
        // история: до 01.06 — 0.095, с 10.07 — 0.11. Лаг -2M: стартовый
        // фиксинг (31.08) смотрит на 30.06 -> 0.095; репрайсинг 15.09
        // смотрит на 15.07 -> уже 0.11
        let mut book = RateBook::new();
        book.insert(crate::rates::RateSeries::new(
            "CBKEY1M",
            vec![(d("2022-06-01"), 0.095), (d("2022-07-10"), 0.11)],
        ));

        let (events, _) = build_cashflow(&dl, &book, &cfg());
        let repr: Vec<&Event> = events
            .iter()
            .filter(|e| e.payment_type == PaymentType::Repricing)
            .collect();
        assert_eq!(repr.len(), 1);
        assert_eq!(repr[0].payment_dttm, d("2022-09-15"));
        assert_eq!(repr[0].amount, 0.0);
        assert_eq!(repr[0].rate, Some(0.13)); // 0.11 + 0.02
        assert_eq!(repr[0].base_rate, Some(0.11));

        let ints: Vec<&Event> = events
            .iter()
            .filter(|e| e.payment_type == PaymentType::InterestPayment)
            .collect();
        assert_eq!(ints.len(), 3, "30.09, 31.10 и Tm 31.12");
        let want0 = 1_000_000.0 * (0.115 * 15.0 / 365.0 + 0.13 * 15.0 / 365.0);
        assert!((ints[0].amount - want0).abs() < 1e-6);
        assert_eq!(ints[0].rate, Some(0.13)); // ставка последнего отрезка периода
        assert_eq!(ints[0].base_rate, Some(0.11));
        assert!((ints[1].amount - 1_000_000.0 * 0.13 * 31.0 / 365.0).abs() < 1e-6);
    }

    #[test]
    fn float_margin_schedule_overrides_margin() {
        let mut dl = deal();
        dl.rate_type_cd = RateType::Float;
        dl.interest_rate = 0.02;
        dl.base_indicator = Some("CBKEY1M".into());
        dl.base_indicator_rate = Some(0.08); // истории нет — значение из сделки
        dl.maturity_dttm = d("2022-12-31");
        dl.interest_payment = vec![d("2022-10-31"), d("2022-11-30")];
        dl.repricing_dttm_rate_schedule_fixed = vec![(d("2022-11-01"), 0.05)];
        dl.repricing_dttm_schedule = vec![d("2022-11-01")];

        let (events, _) = build_cashflow(&dl, &RateBook::new(), &cfg());
        let repr: Vec<&Event> = events
            .iter()
            .filter(|e| e.payment_type == PaymentType::Repricing)
            .collect();
        assert_eq!(repr.len(), 1);
        assert_eq!(repr[0].rate, Some(0.13)); // 0.08 + 0.05

        let ints: Vec<&Event> = events
            .iter()
            .filter(|e| e.payment_type == PaymentType::InterestPayment)
            .collect();
        assert!((ints[0].amount - 1_000_000.0 * 0.10 * 61.0 / 365.0).abs() < 1e-6);
        assert_eq!(ints[0].rate, Some(0.10));
        // 30.11: 1 день по 0.10 + 29 дней по 0.13
        let want1 = 1_000_000.0 * (0.10 * 1.0 / 365.0 + 0.13 * 29.0 / 365.0);
        assert!((ints[1].amount - want1).abs() < 1e-6);
        assert_eq!(ints[1].rate, Some(0.13));
    }

    #[test]
    fn fixed_rate_schedule_changes_fixed_rate() {
        let mut dl = deal();
        dl.interest_rate = 0.09;
        dl.maturity_dttm = d("2022-12-31");
        dl.interest_payment = vec![d("2022-10-31"), d("2022-11-30")];
        // FIXED-сделка с графиком ставки (кейс сделки 44): с 15.10 ставка 0.12
        dl.repricing_dttm_rate_schedule_fixed = vec![(d("2022-10-15"), 0.12)];
        dl.repricing_dttm_schedule = vec![d("2022-10-15")];

        let (events, _) = build_cashflow(&dl, &RateBook::new(), &cfg());
        let repr: Vec<&Event> = events
            .iter()
            .filter(|e| e.payment_type == PaymentType::Repricing)
            .collect();
        assert_eq!(repr.len(), 1);
        assert_eq!(repr[0].rate, Some(0.12));
        assert_eq!(repr[0].base_rate, None, "у FIXED нет индикатора");

        let ints: Vec<&Event> = events
            .iter()
            .filter(|e| e.payment_type == PaymentType::InterestPayment)
            .collect();
        // 31.10: 45 дней по 0.09 + 16 дней по 0.12
        let want = 1_000_000.0 * (0.09 * 45.0 / 365.0 + 0.12 * 16.0 / 365.0);
        assert!((ints[0].amount - want).abs() < 1e-6);
        assert!((ints[1].amount - 1_000_000.0 * 0.12 * 30.0 / 365.0).abs() < 1e-6);
    }
}
