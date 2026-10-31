//! SIMPLE — явный график тела либо погашение в конце (методика §5.3):
//! будущие суммы графика платятся по датам, но не больше остатка; в дату
//! погашения — `close_payment` на весь остаток. Расхождение суммы графика
//! с остатком — warning `SCHEDULE_SUM_MISMATCH` (H9, разница уходит в
//! close). Сам цикл — общий для всех дискриминаторов, в
//! `engine::run_timeline`.

use crate::model::{Config, Deal, Event};
use crate::rates::RateBook;

pub fn build(deal: &Deal, rates: &RateBook, cfg: &Config) -> Vec<Event> {
    super::run_timeline(deal, rates, cfg)
}
