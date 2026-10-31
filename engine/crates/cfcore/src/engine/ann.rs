//! Аннуитет (ANN) — гипотеза H7, методика §5.1.
//!
//! Платёж считается от остатка на дату расчёта и пересчитывается после
//! каждого пересмотра ставки: `A = B · i / (1 − (1+i)^(−n))`,
//! `i = r · m / 12`, где `m` — длина периода тела в месяцах. Тело платежа
//! `P = A − проценты периода`; последний платёж — `close_payment` на весь
//! остаток. В выводе каждая дата аннуитета — две строки:
//! `interest_payment` и `principal_payment` (как в демо-данных фронтенда).
//! Сам цикл — общий для всех дискриминаторов, в `engine::run_timeline`.

use crate::model::{Config, Deal, Event};
use crate::rates::RateBook;

pub fn build(deal: &Deal, rates: &RateBook, cfg: &Config) -> Vec<Event> {
    super::run_timeline(deal, rates, cfg)
}
