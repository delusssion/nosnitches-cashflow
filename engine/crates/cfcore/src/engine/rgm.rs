//! RGM — погашение тела равными долями (H1, методика §5.2):
//! `P = B(T0) / n`, где `n` — число оставшихся дат тела. Проценты
//! начисляются на убывающий остаток; последний платёж — `close_payment`
//! на остаток. Пересмотр ставки долю тела не меняет.
//! Сам цикл — общий для всех дискриминаторов, в `engine::run_timeline`.

use crate::model::{Config, Deal, Event};
use crate::rates::RateBook;

pub fn build(deal: &Deal, rates: &RateBook, cfg: &Config) -> Vec<Event> {
    super::run_timeline(deal, rates, cfg)
}
