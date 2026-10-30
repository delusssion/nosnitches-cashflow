//! SIMPLE: явный график тела либо погашение всего остатка в конце.
//!
//! Даты тела: явный `principal_payment`, если задан; иначе сетка
//! `regular_payment_rule` (частный случай `ENDDATE` — bullet в maturity).
//! Расхождение суммы графика с остатком (сделки 20 и 54) — предупреждение
//! `PRINCIPAL_SCHEDULE_MISMATCH`, разница уходит в `close_payment` (H9).
//! Сам таймлайн считает `engine::build_events`.

use crate::model::{Config, Deal, Event};
use crate::rates::RateBook;

pub fn build(deal: &Deal, rates: &RateBook, cfg: &Config) -> Vec<Event> {
    super::build_events(deal, rates, cfg)
}
