//! RGM: регулярное погашение тела равными долями — гипотеза H1:
//!
//! ```text
//! доля = остаток на дату расчёта / число оставшихся дат погашения тела
//! ```
//!
//! Даты тела — сетка `regular_payment_rule` (явный `principal_payment`, если
//! задан, имеет приоритет). Проценты начисляются отдельно по сетке
//! `interest_payment_rule` на текущий остаток; последняя доля —
//! `close_payment` на весь остаток. Сам таймлайн считает `engine::build_events`.

use crate::model::{Config, Deal, Event};
use crate::rates::RateBook;

pub fn build(deal: &Deal, rates: &RateBook, cfg: &Config) -> Vec<Event> {
    super::build_events(deal, rates, cfg)
}
