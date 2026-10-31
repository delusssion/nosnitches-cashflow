//! cfcore — расчётное ядро калькулятора денежных потоков.
//! Форматы данных и спорные трактовки — в docs/CONTRACT.md проекта.

pub mod dates;
pub mod daycount;
pub mod engine;
pub mod io;
pub mod model;
pub mod rates;
pub mod validate;

pub use model::{Config, Deal, DealError, Discriminator, Event, Severity, Stream};
pub use rates::{RateBook, RateSeries};
