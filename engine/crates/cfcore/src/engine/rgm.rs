//! RGM: регулярное погашение тела равными долями — гипотеза H1:
//!
//! ```text
//! доля = остаток на дату расчёта / число оставшихся дат погашения тела
//! ```
//!
//! Даты тела — сетка `regular_payment_rule` (или явный `principal_payment`).
//! Проценты начисляются отдельно по сетке `interest_payment_rule` на текущий
//! остаток; последний платёж по телу — `close_payment` на весь остаток.

use crate::model::{Config, Deal, Event};
use crate::rates::RateBook;

pub fn build(_deal: &Deal, _rates: &RateBook, _cfg: &Config) -> Vec<Event> {
    // TODO(engine/rgm): равные доли H1 + close_payment.
    Vec::new()
}
