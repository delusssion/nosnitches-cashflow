//! История индикаторов (ключевая ставка ЦБ) и значение ставки на дату
//! с лагом индикатора и сдвигом сценария.

use std::collections::BTreeMap;

use chrono::NaiveDate;

use crate::dates;

/// Одна кривая: ступенчатая функция «значение действует с даты до следующей».
#[derive(Debug, Clone)]
pub struct RateSeries {
    pub indicator: String,
    steps: Vec<(NaiveDate, f64)>,
    /// Прогноз после даты расчёта — кривая команды 2 (базовый или стресс-сценарий).
    /// Пусто — действует H3: последнее известное значение на дату расчёта.
    forecast: Vec<(NaiveDate, f64)>,
}

impl RateSeries {
    /// Строит серию из шагов `(дата начала действия, значение)`:
    /// сортирует по дате, при дубле даты побеждает последнее значение.
    pub fn new(indicator: impl Into<String>, mut steps: Vec<(NaiveDate, f64)>) -> Self {
        steps.sort_by_key(|(d, _)| *d);
        // std-овский dedup держит первое из равных, а по контракту — последнее
        let mut deduped: Vec<(NaiveDate, f64)> = Vec::with_capacity(steps.len());
        for (d, r) in steps {
            match deduped.last_mut() {
                Some((last_d, last_r)) if *last_d == d => *last_r = r,
                _ => deduped.push((d, r)),
            }
        }
        Self {
            indicator: indicator.into(),
            steps: deduped,
            forecast: Vec::new(),
        }
    }

    /// Подключить кривую прогноза (команда 2). Ступенчатая, как история.
    pub fn with_forecast(mut self, steps: Vec<(NaiveDate, f64)>) -> Self {
        self.forecast = RateSeries::new(self.indicator.clone(), steps).steps;
        self
    }

    /// Есть ли у серии кривая прогноза.
    pub fn has_forecast(&self) -> bool {
        !self.forecast.is_empty()
    }

    /// Значение индикатора на дату. Для дат позже последнего известного шага
    /// возвращается последнее известное значение (гипотеза H3); для дат раньше
    /// первого — первое.
    pub fn value_on(&self, date: NaiveDate) -> f64 {
        match self.steps.len() {
            0 => 0.0,
            // индекс последнего шага с датой <= date
            _ => {
                let i = self.steps.partition_point(|(d, _)| *d <= date);
                self.steps[i.saturating_sub(1)].1
            }
        }
    }

    /// Значение с лагом индикатора (берём значение на `date + lag` месяцев)
    /// и сдвигом сценария (H3, в спринте 2 сдвиг 0).
    pub fn rate_with(&self, date: NaiveDate, lag_months: i32, shift: f64) -> f64 {
        self.value_on(dates::add_months(date, lag_months)) + shift
    }

    /// Значение индикатора для фиксинга на дату `at` с лагом `lag_months`.
    /// Смотр не дальше даты расчёта `report`: история после даты расчёта не
    /// «подглядывается». Для будущих дат — значение кривой прогноза (команда 2),
    /// если она есть и уже началась, иначе последнее известное на дату расчёта
    /// (H3); в обоих случаях плюс сдвиг сценария.
    pub fn fixing(&self, at: NaiveDate, lag_months: i32, report: NaiveDate, shift: f64) -> f64 {
        let look = dates::add_months(at, lag_months);
        if look < report {
            return self.value_on(look);
        }
        let i = self.forecast.partition_point(|(d, _)| *d <= look);
        let base = match i.checked_sub(1) {
            Some(k) => self.forecast[k].1,
            None => self.value_on(report),
        };
        base + shift
    }

    /// Первый и последний шаг серии (для сводки прогона).
    pub fn span(&self) -> Option<(NaiveDate, NaiveDate)> {
        match (self.steps.first(), self.steps.last()) {
            (Some((d1, _)), Some((d2, _))) => Some((*d1, *d2)),
            _ => None,
        }
    }
}

/// Все индикаторы одного прогона, например `CBKEY1M`.
#[derive(Debug, Clone, Default)]
pub struct RateBook {
    series: BTreeMap<String, RateSeries>,
}

impl RateBook {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, s: RateSeries) {
        self.series.insert(s.indicator.clone(), s);
    }

    /// Подключить кривые прогноза по индикаторам. Кривая без истории индикатора
    /// становится серией только из прогноза (до неё значение — первое на кривой).
    pub fn set_forecasts(&mut self, curves: BTreeMap<String, Vec<(NaiveDate, f64)>>) {
        for (indicator, steps) in curves {
            let series = match self.series.remove(&indicator) {
                Some(s) => s.with_forecast(steps),
                None => RateSeries::new(indicator.clone(), steps.clone()).with_forecast(steps),
            };
            self.series.insert(indicator, series);
        }
    }

    pub fn get(&self, indicator: &str) -> Option<&RateSeries> {
        self.series.get(indicator)
    }

    pub fn is_empty(&self) -> bool {
        self.series.is_empty()
    }

    pub fn len(&self) -> usize {
        self.series.len()
    }

    pub fn indicators(&self) -> impl Iterator<Item = &str> {
        self.series.keys().map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn series() -> RateSeries {
        RateSeries::new(
            "CBKEY1M",
            vec![
                (d("2022-09-19"), 0.075),
                (d("2022-02-28"), 0.20),
                (d("2022-02-28"), 0.095), // дубль даты: побеждает последний
            ],
        )
    }

    #[test]
    fn steps_sorted_and_deduped() {
        let s = series();
        assert_eq!(
            s.steps.first().map(|(d, r)| (*d, *r)),
            Some((d("2022-02-28"), 0.095))
        );
        assert_eq!(
            s.steps.last().map(|(d, r)| (*d, *r)),
            Some((d("2022-09-19"), 0.075))
        );
    }

    #[test]
    fn value_is_step_function() {
        let s = series();
        assert_eq!(s.value_on(d("2022-02-28")), 0.095); // в дату шага — новое значение
        assert_eq!(s.value_on(d("2022-09-18")), 0.095);
        assert_eq!(s.value_on(d("2022-09-19")), 0.075);
    }

    #[test]
    fn future_falls_back_to_last_known() {
        let s = series();
        assert_eq!(s.value_on(d("2026-10-30")), 0.075);
    }

    #[test]
    fn before_first_falls_back_to_first() {
        let s = series();
        assert_eq!(s.value_on(d("2020-01-03")), 0.095);
    }

    #[test]
    fn lag_and_shift_apply() {
        let s = series();
        // репрайсинг 2022-10-01, лаг -1M -> смотрим значение на 2022-09-01 (0.095) + сдвиг
        let r = s.rate_with(d("2022-10-01"), -1, 0.0);
        assert_eq!(r, 0.095);
        // на 2022-10-01 действует шаг 2022-09-19 (0.075) + сдвиг 0.01
        let r = s.rate_with(d("2022-10-01"), 0, 0.01);
        assert!((r - 0.085).abs() < 1e-12); // без бит-в-бит: 0.075 + 0.01 в f64
    }

    #[test]
    fn fixing_never_looks_past_report_date() {
        let s = series(); // шаги: 2022-02-28 (0.095), 2022-09-19 (0.075)
        let report = d("2022-08-31");
        // будущее заморожено на значении даты расчёта: без клампа
        // фиксинг 2022-10-01 увидел бы шаг 2022-09-19 (0.075)
        assert_eq!(s.fixing(d("2022-10-01"), 0, report, 0.0), 0.095);
        assert_eq!(s.fixing(d("2026-01-01"), 0, report, 0.0), 0.095);
        // лаг уводит смотр в прошлое — реальная история до даты расчёта
        assert_eq!(s.fixing(d("2022-10-01"), -1, report, 0.0), 0.095); // смотр 2022-09-01
                                                                       // сдвиг сценария применяется только к будущему
        let r = s.fixing(d("2022-10-01"), 0, report, 0.01);
        assert!((r - 0.105).abs() < 1e-12);
    }

    #[test]
    fn book_lookup() {
        let mut book = RateBook::new();
        book.insert(series());
        assert!(book.get("CBKEY1M").is_some());
        assert!(book.get("XXX").is_none());
        assert_eq!(book.len(), 1);
    }

    #[test]
    fn forecast_curve_replaces_h3_after_report() {
        let d = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        let report = d("2022-08-31");
        let s = RateSeries::new("CBKEY1M", vec![(d("2022-07-25"), 0.08)])
            .with_forecast(vec![(d("2022-10-01"), 0.10), (d("2023-01-01"), 0.12)]);
        assert!(s.has_forecast());
        // до даты расчёта — история
        assert_eq!(s.fixing(d("2022-08-01"), 0, report, 0.0), 0.08);
        // после даты расчёта, но до начала кривой — H3
        assert_eq!(s.fixing(d("2022-09-15"), 0, report, 0.0), 0.08);
        // на кривой — значение кривой (+ сдвиг сценария)
        assert_eq!(s.fixing(d("2022-11-15"), 0, report, 0.0), 0.10);
        assert!((s.fixing(d("2023-02-01"), 0, report, 0.01) - 0.13).abs() < 1e-12);
        // лаг смотрит назад: 2023-01-15 −1M → 2022-12-15 → 0.10
        assert_eq!(s.fixing(d("2023-01-15"), -1, report, 0.0), 0.10);
    }
}
