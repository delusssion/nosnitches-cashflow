//! Движки расписаний: ANN, RGM, SIMPLE + плавающая ставка и субсидия.
//!
//! Ядро — [`build_cashflow`]: чистая функция без состояния, одинаковые входы
//! дают одинаковый выход, поэтому её можно звать из любого числа потоков —
//! результат прогона не зависит от параллельности.
//!
//! Проценты начисляются по отрезкам между событиями: платёж тела внутри
//! процентного периода режет его на секции, каждая секция начисляется на
//! остаток, действовавший в ней (см. docs/DECISIONS.md).

pub mod ann;
pub mod float;
pub mod rgm;
pub mod simple;
pub mod subsidy;

use chrono::NaiveDate;

use crate::dates;
use crate::daycount::year_fraction;
use crate::model::{
    Config, Deal, DealError, Discriminator, Event, PaymentType, RateType, Severity, Stream,
};
use crate::rates::RateBook;
use crate::validate;

/// Допуск на деньги — как приёмочный допуск контракта (0,01 ₽).
pub(crate) const MONEY_TOL: f64 = 0.01;

/// Расчёт кэшфлоу одной сделки. Возвращает события (формат 3 контракта,
/// с проставленным `event_order`) и ошибки/предупреждения сделки.
pub fn build_cashflow(deal: &Deal, rates: &RateBook, cfg: &Config) -> (Vec<Event>, Vec<DealError>) {
    let errors = validate::validate(deal);
    if errors.iter().any(|e| e.severity == Severity::Error) {
        return (Vec::new(), errors);
    }
    let mut events = match deal.discriminator {
        Discriminator::Ann => ann::build(deal, rates, cfg),
        Discriminator::Rgm => rgm::build(deal, rates, cfg),
        Discriminator::Simple => simple::build(deal, rates, cfg),
    };
    for (i, e) in events.iter_mut().enumerate() {
        e.event_order = i as u32 + 1;
    }
    (events, errors)
}

/// Ставка сделки на дату фиксинга.
///
/// FIXED — `interest_rate`. FLOAT (H2): индикатор с лагом + маржа
/// (`interest_rate`), затем `floor`/`cap`; на будущее значение история не
/// «подглядывается» — берётся последнее известное на дату расчёта + сдвиг
/// сценария (H3, см. `RateSeries::fixing`). Если истории нет вовсе,
/// используется `base_indicator_rate` из сделки.
pub(crate) fn deal_rate(deal: &Deal, rates: &RateBook, cfg: &Config, at: NaiveDate) -> f64 {
    match deal.rate_type_cd {
        RateType::Fixed => deal.interest_rate,
        RateType::Float => {
            let lag = deal
                .indicator_lag_cd
                .as_deref()
                .and_then(dates::parse_lag_months)
                .unwrap_or(0);
            let indicator = deal
                .base_indicator
                .as_deref()
                .and_then(|code| rates.get(code))
                .map(|s| s.fixing(at, lag, deal.report_dttm, cfg.scenario_shift))
                .or(deal.base_indicator_rate)
                .unwrap_or(0.0);
            float::clamp_rate(
                indicator + deal.interest_rate,
                deal.floor_limit_interest_rate,
                deal.cap_limit_interest_rate,
            )
        }
    }
}

/// Полный таймлайн сделки для RGM и SIMPLE: план тела, даты процентов,
/// проход по событиям. ANN идёт своим путём (ann.rs, H7).
pub(crate) fn build_events(deal: &Deal, rates: &RateBook, cfg: &Config) -> Vec<Event> {
    let principal = principal_plan(deal, cfg);
    let mut interest = interest_plan(deal);
    if interest.is_empty() {
        // ни явных дат, ни правила процентов — проценты по датам тела
        interest = principal.iter().map(|(d, _)| *d).collect();
    }
    run_timeline(deal, rates, cfg, principal, interest)
}

/// План платежей тела: явный график, если задан; иначе равные доли по сетке
/// правила (RGM, H1); иначе bullet — всё тело в дату погашения.
/// Недобор явного графика до остатка уходит в close_payment в дату
/// погашения (H9); перепокрытие гасится в таймлайне срезом платежа.
pub(crate) fn principal_plan(deal: &Deal, cfg: &Config) -> Vec<(NaiveDate, f64)> {
    let balance = deal.current_principal_amt.abs();
    let in_window = |d: NaiveDate| d > deal.report_dttm && d <= deal.maturity_dttm;

    let mut plan: Vec<(NaiveDate, f64)> = if !deal.principal_payment.is_empty() {
        deal.principal_payment
            .iter()
            .filter(|(d, _)| in_window(*d))
            .cloned()
            .collect()
    } else {
        let grid = dates::rule_schedule(
            &deal.regular_payment_rule,
            deal.report_dttm,
            deal.maturity_dttm,
        );
        if deal.discriminator == Discriminator::Rgm && !grid.is_empty() {
            // H1: остаток на дату расчёта / число оставшихся дат погашения тела
            let n = grid.len() as f64;
            grid.into_iter().map(|d| (d, balance / n)).collect()
        } else {
            vec![(deal.maturity_dttm, balance)]
        }
    };

    if !deal.principal_payment.is_empty() && cfg.schedule_gap_to_close_payment {
        let paid: f64 = plan.iter().map(|(_, a)| a).sum();
        let gap = balance - paid;
        if gap > MONEY_TOL {
            match plan.last_mut() {
                Some((d, a)) if *d == deal.maturity_dttm => *a += gap,
                _ => plan.push((deal.maturity_dttm, gap)),
            }
        }
    }
    if plan.is_empty() {
        plan.push((deal.maturity_dttm, balance));
    }
    plan
}

/// Даты уплаты процентов: явный список, если задан; иначе сетка
/// `interest_payment_rule`.
fn interest_plan(deal: &Deal) -> Vec<NaiveDate> {
    if !deal.interest_payment.is_empty() {
        deal.interest_payment.clone()
    } else {
        dates::rule_schedule(
            &deal.interest_payment_rule,
            deal.report_dttm,
            deal.maturity_dttm,
        )
    }
}

/// Начисленные проценты по секциям `(начало, остаток)` на интервале до `end`.
fn segment_accrual(
    segs: &[(NaiveDate, f64)],
    end: NaiveDate,
    rate: f64,
    conv: crate::model::DayConvention,
) -> f64 {
    let mut acc = 0.0;
    for w in 0..segs.len() {
        let seg_end = segs.get(w + 1).map_or(end, |(e, _)| *e);
        if seg_end > segs[w].0 {
            acc += segs[w].1 * rate * year_fraction(segs[w].0, seg_end, conv);
        }
    }
    acc
}

/// Проход по объединённому таймлайну событий. В одну дату порядок:
/// interest_payment -> principal_payment (контракт); последнее событие по
/// телу получает тип close_payment на весь остаток, после него
/// principal_after = 0 строго.
fn run_timeline(
    deal: &Deal,
    rates: &RateBook,
    cfg: &Config,
    mut principal: Vec<(NaiveDate, f64)>,
    mut interest: Vec<NaiveDate>,
) -> Vec<Event> {
    let sign: f64 = if deal.current_principal_amt < 0.0 {
        -1.0
    } else {
        1.0
    };
    let mut balance = deal.current_principal_amt.abs();
    // E4: FIXED постоянна; FLOAT заморожен на дате расчёта — репрайсинг в engine/float
    let rate = deal_rate(deal, rates, cfg, deal.report_dttm);

    // H6: событие в дату расчёта — прошлое
    let is_past = |d: NaiveDate| {
        if cfg.event_on_report_date_is_past {
            d <= deal.report_dttm
        } else {
            d < deal.report_dttm
        }
    };
    principal.retain(|(d, _)| !is_past(*d));
    interest.retain(|d| !is_past(*d) && *d <= deal.maturity_dttm);
    if principal.is_empty() {
        principal.push((deal.maturity_dttm, balance));
    }

    let mut events: Vec<Event> = Vec::new();
    // секции начисления с последнего процентного события: (начало, остаток)
    let mut segs: Vec<(NaiveDate, f64)> = vec![(deal.report_dttm, balance)];
    let mut accrued_left = deal.accrued_interest;
    let (mut pi, mut ii) = (0usize, 0usize);
    let mut last_principal: Option<usize> = None;

    while pi < principal.len() || ii < interest.len() {
        let dp = principal.get(pi).map_or(NaiveDate::MAX, |(d, _)| *d);
        let di = interest.get(ii).copied().unwrap_or(NaiveDate::MAX);
        let d = dp.min(di);

        if di == d {
            ii += 1;
            let start = segs[0].0;
            let acc = std::mem::take(&mut accrued_left)
                + segment_accrual(&segs, d, rate, deal.day_convention_cd);
            if acc.abs() > 1e-9 {
                events.push(Event {
                    event_order: 0,
                    payment_dttm: d,
                    payment_type: PaymentType::InterestPayment,
                    stream: Stream::Client,
                    amount: sign * acc,
                    principal_before: sign * balance,
                    principal_after: sign * balance,
                    rate: Some(rate),
                    base_rate: None,
                    accrual_start_dttm: Some(start),
                    accrual_end_dttm: Some(d),
                    accrual_days: Some((d - start).num_days()),
                    year_frac: Some(year_fraction(start, d, deal.day_convention_cd)),
                });
            }
            segs.clear();
            segs.push((d, balance));
        }

        if dp == d {
            pi += 1;
            let pay = principal[pi - 1].1.min(balance);
            if pay > MONEY_TOL {
                balance -= pay;
                events.push(Event {
                    event_order: 0,
                    payment_dttm: d,
                    payment_type: PaymentType::PrincipalPayment,
                    stream: Stream::Client,
                    amount: sign * pay,
                    principal_before: sign * (balance + pay),
                    principal_after: sign * balance,
                    rate: Some(rate),
                    base_rate: None,
                    accrual_start_dttm: None,
                    accrual_end_dttm: None,
                    accrual_days: None,
                    year_frac: None,
                });
                last_principal = Some(events.len() - 1);
                // новый остаток — новая секция начисления
                segs.push((d, balance));
            }
        }
    }

    // Последнее событие по телу — close_payment на весь остаток; в его дату,
    // если процентов там не было, добавляется платёж за хвост периода
    // начисления (кейс сделки 55: проценты до 30.06, тело 04.07).
    if let Some(mut idx) = last_principal {
        let close_date = events[idx].payment_dttm;
        if close_date > segs[0].0 {
            let acc = segment_accrual(&segs, close_date, rate, deal.day_convention_cd);
            if acc.abs() > 1e-9 {
                events.insert(
                    idx,
                    Event {
                        event_order: 0,
                        payment_dttm: close_date,
                        payment_type: PaymentType::InterestPayment,
                        stream: Stream::Client,
                        amount: sign * acc,
                        principal_before: sign * balance,
                        principal_after: sign * balance,
                        rate: Some(rate),
                        base_rate: None,
                        accrual_start_dttm: Some(segs[0].0),
                        accrual_end_dttm: Some(close_date),
                        accrual_days: Some((close_date - segs[0].0).num_days()),
                        year_frac: Some(year_fraction(
                            segs[0].0,
                            close_date,
                            deal.day_convention_cd,
                        )),
                    },
                );
                idx += 1;
            }
        }
        let close = &mut events[idx];
        close.payment_type = PaymentType::ClosePayment;
        close.amount = close.principal_before;
        close.principal_after = 0.0;
    }

    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DayConvention, PaymentRule};

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn deal() -> Deal {
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
            accrued_interest: 0.0,
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

    fn cfg() -> Config {
        Config::default()
    }

    #[test]
    fn simple_segments_gap_close_and_accrued() {
        let mut dl = deal();
        dl.accrued_interest = 1000.0;
        dl.interest_payment = vec![d("2022-09-30"), d("2022-10-31"), d("2022-11-30")];
        // график покрывает 400 тыс. из 1 млн — добор 600 тыс. в close_payment (H9)
        dl.principal_payment = vec![(d("2022-11-15"), 400_000.0)];

        let (events, errs) = build_cashflow(&dl, &RateBook::new(), &cfg());
        assert!(errs.iter().all(|e| e.severity == Severity::Warning));

        let amounts: Vec<f64> = events.iter().map(|e| e.amount).collect();
        let close = |days: f64, bal: f64| bal * 0.12 * days / 365.0;
        // 1: 30.09 — накопленные + начисление на 1 млн за 30 дней
        assert_eq!(events[0].payment_dttm, d("2022-09-30"));
        assert!((amounts[0] - (close(30.0, 1_000_000.0) + 1000.0)).abs() < 1e-6);
        // 2: 31.10 — 31 день на 1 млн
        assert!((amounts[1] - close(31.0, 1_000_000.0)).abs() < 1e-6);
        // 3: 15.11 — тело 400 тыс.
        assert_eq!(events[2].payment_type, PaymentType::PrincipalPayment);
        assert!((amounts[2] - 400_000.0).abs() < 1e-9);
        // 4: 30.11 — секции: 15 дней на 1 млн + 15 дней на 600 тыс.
        assert!((amounts[3] - (close(15.0, 1_000_000.0) + close(15.0, 600_000.0))).abs() < 1e-6);
        assert_eq!(events[3].accrual_start_dttm, Some(d("2022-10-31")));
        assert_eq!(events[3].principal_before, 600_000.0);
        // 5: 31.12 — хвост начисления 31 день на 600 тыс. в дату close
        assert_eq!(events[4].payment_type, PaymentType::InterestPayment);
        assert!((amounts[4] - close(31.0, 600_000.0)).abs() < 1e-6);
        // 6: close_payment гасит остаток в ноль строго
        assert_eq!(events[5].payment_type, PaymentType::ClosePayment);
        assert!((amounts[5] - 600_000.0).abs() < 1e-9);
        assert_eq!(events[5].principal_after, 0.0);
        assert_eq!(events[5].event_order, 6);
        // итог по телу равен остатку
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
        let grid = PaymentRule {
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
        // проценты: 30 дней на 900 тыс., 30 дней на 600 тыс., 31 день на 300 тыс.
        let ints: Vec<&Event> = events
            .iter()
            .filter(|e| e.payment_type == PaymentType::InterestPayment)
            .collect();
        assert!((ints[0].amount - 900_000.0 * 0.0365 * 30.0 / 365.0).abs() < 1e-6);
        assert!((ints[1].amount - 600_000.0 * 0.0365 * 30.0 / 365.0).abs() < 1e-6);
        assert!((ints[2].amount - 300_000.0 * 0.0365 * 31.0 / 365.0).abs() < 1e-6);
        // последняя доля — close_payment
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
        // ни правил, ни явных списков: проценты по датам тела — в конце срока

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
    fn overpaid_schedule_is_capped() {
        let mut dl = deal();
        dl.maturity_dttm = d("2022-12-31");
        // график больше остатка (кейс сделки 20): платим сколько осталось
        dl.principal_payment = vec![(d("2022-10-31"), 600_000.0), (d("2022-12-31"), 600_000.0)];

        let (events, _) = build_cashflow(&dl, &RateBook::new(), &cfg());
        let body: f64 = events
            .iter()
            .filter(|e| e.payment_type != PaymentType::InterestPayment)
            .map(|e| e.amount)
            .sum();
        assert!(
            (body - 1_000_000.0).abs() < 1e-6,
            "собрано ровно остаток, не больше"
        );
        let last = events.last().unwrap();
        assert_eq!(last.payment_type, PaymentType::ClosePayment);
        assert!((last.amount - 400_000.0).abs() < 1e-6);
        assert_eq!(last.principal_after, 0.0);
    }

    #[test]
    fn bullet_with_monthly_interest() {
        let mut dl = deal();
        dl.maturity_dttm = d("2023-02-28");
        dl.interest_payment_rule = PaymentRule {
            start_dttm: Some(d("2022-08-31")),
            period_cd: dates::parse_period_cd("1M"),
            day_num: None,
        };
        let (events, _) = build_cashflow(&dl, &RateBook::new(), &cfg());
        // 30.09, 30.10, 30.11, 30.12, 30.01 — проценты, 28.02 — проценты + close
        let ints: Vec<&Event> = events
            .iter()
            .filter(|e| e.payment_type == PaymentType::InterestPayment)
            .collect();
        assert_eq!(ints.len(), 6);
        // сетка прижатия: якорь 31-е, в феврале — 28-е
        assert_eq!(ints.last().unwrap().payment_dttm, d("2023-02-28"));
        let close = events.last().unwrap();
        assert_eq!(close.payment_type, PaymentType::ClosePayment);
        assert!((close.amount - 1_000_000.0).abs() < 1e-9);
    }

    #[test]
    fn ann_is_stub_until_engine_ann() {
        let mut dl = deal();
        dl.discriminator = Discriminator::Ann;
        let (events, _) = build_cashflow(&dl, &RateBook::new(), &cfg());
        assert!(events.is_empty(), "ANN появится в ветке engine/ann");
    }
}
