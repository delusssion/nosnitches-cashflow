//! Движки расписаний: ANN, RGM, SIMPLE + плавающая ставка и субсидия.

pub mod ann;
pub mod float;
pub mod rgm;
pub mod simple;
pub mod subsidy;

use crate::model::{Config, Deal, DealError, Event, Severity};
use crate::rates::RateBook;
use crate::validate;

/// Расчёт кэшфлоу одной сделки. Чистая функция: одинаковые входы дают
/// одинаковый выход, поэтому её можно звать из любого числа потоков —
/// результат прогона не зависит от параллельности.
///
/// Возвращает события (формат 3, без сквозной нумерации `event_order` ещё
/// не пронумерованы — нумерация при записи) и ошибки/предупреждения сделки.
pub fn build_cashflow(deal: &Deal, rates: &RateBook, cfg: &Config) -> (Vec<Event>, Vec<DealError>) {
    let errors = validate::validate(deal);
    if errors.iter().any(|e| e.severity == Severity::Error) {
        return (Vec::new(), errors);
    }

    // TODO(engine/simple-rgm):
    //   1. сетки дат: проценты (interest_payment_rule | interest_payment),
    //      тело (regular_payment_rule | principal_payment), репрайсинг;
    //   2. объединить в таймлайн, прошедшие даты (<= report_dttm, H6) отбросить;
    //   3. начисление по подпериодам между событиями, меняющими ставку/остаток;
    //   4. события по дискриминатору: SIMPLE / RGM / ANN;
    //   5. close_payment на весь остаток (principal_after = 0 строго);
    //   6. субсидия отдельным потоком (engine/subsidy).
    let _ = (rates, cfg);
    (Vec::new(), errors)
}
