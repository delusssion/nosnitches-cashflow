//! Плавающая ставка: репрайсинг, маржа, cap/floor, фиксированная часть.
//!
//! Гипотеза H2: ставка периода = значение индикатора + `interest_rate`
//! (маржа), затем ограничение `floor <= ставка <= cap`.
//! Индикатор: история (формат 2) + лаг (`indicator_lag_cd`) + сдвиг
//! сценария (H3); для будущих дат — последнее известное значение.
//! Даты репрайсинга: `repricing_rule` и/или явные `repricing_dttm_schedule`,
//! фиксированная часть — по графику `repricing_dttm_rate_schedule_fixed`.
//! Смена ставки внутри процентного периода режет его на подпериоды.
//! У ANN после смены ставки аннуитет пересчитывается (H7).

/// Применить cap/floor к ставке (H2): `floor <= r <= cap`.
pub fn clamp_rate(rate: f64, floor: Option<f64>, cap: Option<f64>) -> f64 {
    let r = floor.map_or(rate, |f| rate.max(f));
    cap.map_or(r, |c| r.min(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_and_floor_clamp() {
        assert_eq!(clamp_rate(0.15, Some(0.10), None), 0.15);
        assert_eq!(clamp_rate(0.05, Some(0.10), None), 0.10);
        assert_eq!(clamp_rate(0.20, None, Some(0.18)), 0.18);
        assert_eq!(clamp_rate(0.12, Some(0.10), Some(0.18)), 0.12);
        assert_eq!(clamp_rate(0.12, None, None), 0.12);
    }
}
