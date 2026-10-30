//! Субсидия — отдельный поток платежей (`stream = sub`).
//!
//! Гипотеза H4: ставка субсидии равна `sub_coeff_index` (пусто = 1),
//! умноженному на индикатор, плюс `sub_interest_rate`; база начисления —
//! остаток тела. Свои сетки дат (`sub_payment_rule`) и свои пересмотры
//! (`sub_repricing_rule`, флаг `sub_repricing_rule_first_event_flg`),
//! свои cap/floor и лаг индикатора. Формулу ждём от экспертов
//! (docs/QUESTIONS.md) — включается флагом.

use crate::model::{Config, Deal, Event};
use crate::rates::RateBook;

pub fn build(_deal: &Deal, _rates: &RateBook, _cfg: &Config) -> Vec<Event> {
    // TODO(engine/subsidy): после ответа экспертов по формуле (H4).
    Vec::new()
}
