//! Проверка сделок: что можно считать, а что нельзя (файл errors.csv).

use crate::model::{codes, Deal, DealError, Severity};

/// Допуск сверки денег в валидации — как приёмочный допуск контракта, 0,01 ₽.
const TOLERANCE: f64 = 0.01;

/// Проверки сделки. Ошибка (`Severity::Error`) — сделка не считается;
/// предупреждение — считается, но замечание попадает в errors.csv.
pub fn validate(deal: &Deal) -> Vec<DealError> {
    let mut out = Vec::new();

    if deal.currency_id != "RUB" {
        out.push(DealError {
            deal_id: deal.deal_id.clone(),
            severity: Severity::Error,
            code: codes::CURRENCY_NOT_RUB,
            field: "currency_id",
            message: format!(
                "валюта «{}»: считаем только рублёвые сделки",
                deal.currency_id
            ),
        });
    }

    if deal.maturity_dttm <= deal.report_dttm {
        out.push(DealError {
            deal_id: deal.deal_id.clone(),
            severity: Severity::Error,
            code: codes::MATURITY_IN_PAST,
            field: "maturity_dttm",
            message: format!(
                "срок {} не позже даты расчёта {} — будущих событий нет",
                deal.maturity_dttm, deal.report_dttm
            ),
        });
    }

    // H9: явный график тела против остатка. Прошлые даты графика (<= report_dttm)
    // уже не влияют на остаток, в сверку идут только будущие.
    if !deal.principal_payment.is_empty() {
        let future: f64 = deal
            .principal_payment
            .iter()
            .filter(|(d, _)| *d > deal.report_dttm)
            .map(|(_, a)| a)
            .sum();
        let balance = deal.current_principal_amt.abs();
        if (future - balance).abs() > TOLERANCE {
            out.push(DealError {
                deal_id: deal.deal_id.clone(),
                severity: Severity::Warning,
                code: codes::PRINCIPAL_SCHEDULE_MISMATCH,
                field: "principal_payment",
                message: format!(
                    "график тела покрывает {future:.2} из {balance:.2}; разница \
                     уходит в close_payment (гипотеза H9)"
                ),
            });
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DayConvention, Discriminator, PaymentRule, RateType};
    use chrono::NaiveDate;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    /// Минимальная корректная рублёвая сделка; отдельные тесты меняют поля.
    fn deal() -> Deal {
        Deal {
            report_dttm: d("2022-08-31"),
            discriminator: Discriminator::Simple,
            deal_id: "T1".into(),
            currency_id: "RUB".into(),
            start_principal_amt: None,
            current_principal_amt: 1_000_000.0,
            deal_dttm: Some(d("2022-08-01")),
            maturity_dttm: d("2023-08-01"),
            rate_type_cd: RateType::Fixed,
            interest_rate: 0.07,
            base_indicator_rate: None,
            base_indicator: None,
            day_convention_cd: DayConvention::ActActIsda,
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

    #[test]
    fn clean_deal_has_no_errors() {
        assert!(validate(&deal()).is_empty());
    }

    #[test]
    fn usd_is_error() {
        let mut dl = deal();
        dl.currency_id = "USD".into();
        let errs = validate(&dl);
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].code, codes::CURRENCY_NOT_RUB);
        assert_eq!(errs[0].severity, Severity::Error);
    }

    #[test]
    fn matured_deal_is_error() {
        let mut dl = deal();
        dl.maturity_dttm = d("2022-08-31"); // равно дате расчёта — уже прошло
        let errs = validate(&dl);
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].code, codes::MATURITY_IN_PAST);
    }

    #[test]
    fn schedule_gap_is_warning() {
        let mut dl = deal();
        // покрытие 850 тыс. из 1 млн (кейс сделки 54)
        dl.principal_payment = vec![(d("2022-12-30"), 850_000.0)];
        let errs = validate(&dl);
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].code, codes::PRINCIPAL_SCHEDULE_MISMATCH);
        assert_eq!(errs[0].severity, Severity::Warning);
    }

    #[test]
    fn schedule_overpay_is_warning_too() {
        let mut dl = deal();
        // график больше остатка (кейс сделки 20)
        dl.principal_payment = vec![(d("2022-12-30"), 600_000.0), (d("2023-03-30"), 600_000.0)];
        assert_eq!(validate(&dl).len(), 1);
    }

    #[test]
    fn schedule_past_dates_ignored() {
        let mut dl = deal();
        // прошлые платежи не должны ломать сверку: будущее покрывает остаток
        dl.principal_payment = vec![(d("2022-08-01"), 500_000.0), (d("2023-06-30"), 1_000_000.0)];
        assert!(validate(&dl).is_empty());
    }

    #[test]
    fn deposit_balance_checked_by_absolute_value() {
        let mut dl = deal();
        dl.current_principal_amt = -1_000_000.0; // депозит
        dl.principal_payment = vec![(d("2023-03-14"), 1_000_000.0)];
        assert!(validate(&dl).is_empty());
    }
}
