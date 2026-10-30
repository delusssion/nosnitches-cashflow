//! Проверка сделок: что можно считать, а что нельзя (файл errors.csv, §8).

use crate::model::{codes, Deal, DealError, RateType, Severity};

/// Допуск сверки суммы графика тела с остатком (§5.3).
const SCHEDULE_TOL: f64 = 0.005;

/// Проверки сделки. Ошибка (`Severity::Error`) — сделка не считается;
/// предупреждение — считается, но замечание попадает в errors.csv.
pub fn validate(deal: &Deal) -> Vec<DealError> {
    let mut out = Vec::new();

    if deal.currency_id != "RUB" {
        out.push(DealError {
            deal_id: deal.deal_id.clone(),
            severity: Severity::Error,
            code: codes::UNSUPPORTED_CURRENCY,
            field: "currency_id",
            message: format!("валюта «{}», считаем только RUB", deal.currency_id),
        });
    }

    // H6 (по умолчанию): событие в дату расчёта — прошлое
    if deal.maturity_dttm <= deal.report_dttm {
        out.push(DealError {
            deal_id: deal.deal_id.clone(),
            severity: Severity::Error,
            code: codes::MATURITY_BEFORE_REPORT,
            field: "maturity_dttm",
            message: format!(
                "погашение {} не позже даты расчёта {}",
                deal.maturity_dttm, deal.report_dttm
            ),
        });
    }

    if deal.rate_type_cd == RateType::Float && deal.base_indicator.is_none() {
        out.push(DealError {
            deal_id: deal.deal_id.clone(),
            severity: Severity::Error,
            code: codes::FLOAT_NO_INDICATOR,
            field: "base_indicator",
            message: "FLOAT без индикатора".to_string(),
        });
    }

    // Q14: FLOAT без правила и дат пересмотра — ставка зафиксирована на дату
    // сделки; считаем с предупреждением.
    if deal.rate_type_cd == RateType::Float
        && deal.base_indicator.is_some()
        && crate::engine::client_repricing_all(deal).is_empty()
    {
        out.push(DealError {
            deal_id: deal.deal_id.clone(),
            severity: Severity::Warning,
            code: codes::FLOAT_NO_REPRICING,
            field: "repricing_rule_period_cd",
            message: "ставка зафиксирована на дату сделки".to_string(),
        });
    }

    // H9 (§5.3): у SIMPLE сумма будущего графика тела против остатка.
    // Прошлые даты (≤ T0) не влияют на остаток; будущие после Tm в сумму
    // входят (платежи за сроком сделки всё равно не платятся).
    if deal.discriminator == crate::model::Discriminator::Simple
        && !deal.principal_payment.is_empty()
    {
        let future: f64 = deal
            .principal_payment
            .iter()
            .filter(|(d, _)| *d > deal.report_dttm)
            .map(|(_, a)| a)
            .sum();
        let balance = deal.current_principal_amt.abs();
        if (future - balance).abs() > SCHEDULE_TOL {
            out.push(DealError {
                deal_id: deal.deal_id.clone(),
                severity: Severity::Warning,
                code: codes::SCHEDULE_SUM_MISMATCH,
                field: "principal_payment",
                message: format!(
                    "график {future:.2} ≠ остатку {balance:.2} (H9); разница уходит в close_payment"
                ),
            });
        }
    }

    out
}
