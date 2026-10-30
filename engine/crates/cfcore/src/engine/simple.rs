//! SIMPLE: явный график тела либо погашение всего остатка в конце.
//!
//! Даты тела: явный `principal_payment`, если задан; иначе сетка
//! `regular_payment_rule` (частный случай `ENDDATE` — bullet в maturity).
//! Расхождение суммы графика с остатком (сделки 20 и 54) — предупреждение
//! `PRINCIPAL_SCHEDULE_MISMATCH`, разница уходит в `close_payment` (H9).

use crate::model::{Config, Deal, Event};
use crate::rates::RateBook;

pub fn build(_deal: &Deal, _rates: &RateBook, _cfg: &Config) -> Vec<Event> {
    // TODO(engine/simple): явный график / bullet + H9 + close_payment.
    Vec::new()
}
