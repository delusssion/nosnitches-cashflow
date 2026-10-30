//! Модель данных: сделка (48 полей формата 1 контракта), событие кэшфлоу,
//! ошибка сделки и конфиг с гипотезами H1–H9.

use chrono::NaiveDate;

/// Тип погашения тела (`discriminator`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Discriminator {
    /// Аннуитет: тело и проценты одним платежом.
    Ann,
    /// Регулярное погашение тела равными долями (гипотеза H1).
    Rgm,
    /// Явный график тела либо погашение всего остатка в конце.
    Simple,
}

impl Discriminator {
    pub fn as_str(self) -> &'static str {
        match self {
            Discriminator::Ann => "ANN",
            Discriminator::Rgm => "RGM",
            Discriminator::Simple => "SIMPLE",
        }
    }
}

/// Тип ставки (`rate_type_cd`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateType {
    Fixed,
    Float,
}

/// Конвенция дней (`day_convention_cd`). `ACT/ACT` на входе трактуется
/// как `ACT/ACT ISDA` — гипотеза H5, маппинг происходит при чтении.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DayConvention {
    Act365,
    ActActIsda,
}

/// Периодичность правила (`*_period_cd`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeriodCd {
    Months(u32),
    Days(u32),
    /// Событие в дату окончания сделки (ENDDATE).
    EndDate,
}

/// Правило построения сетки дат: начало + периодичность + день события.
/// Одна структура для четырёх сеток: проценты, тело, репрайсинг, субсидия.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PaymentRule {
    pub start_dttm: Option<NaiveDate>,
    pub period_cd: Option<PeriodCd>,
    /// `1..31`; `-1` = последний день месяца; если в месяце нет такого дня —
    /// последний день месяца.
    pub day_num: Option<i32>,
}

impl PaymentRule {
    /// Правило не задано ни одним из полей.
    pub fn is_empty(&self) -> bool {
        self.start_dttm.is_none() && self.period_cd.is_none() && self.day_num.is_none()
    }
}

/// Сделка — ровно 48 полей формата 1 из docs/CONTRACT.md.
#[derive(Debug, Clone)]
pub struct Deal {
    // --- общие атрибуты ---
    pub report_dttm: NaiveDate,
    pub discriminator: Discriminator,
    pub deal_id: String,
    pub currency_id: String,
    pub start_principal_amt: Option<f64>,
    /// `> 0` — актив (кредит), `< 0` — пассив (депозит).
    pub current_principal_amt: f64,
    pub deal_dttm: Option<NaiveDate>,
    pub maturity_dttm: NaiveDate,

    // --- ставка ---
    pub rate_type_cd: RateType,
    /// У FIXED — ставка; у FLOAT — маржа поверх индикатора (гипотеза H2).
    pub interest_rate: f64,
    pub base_indicator_rate: Option<f64>,
    /// Код индикатора, например `CBKEY1M` (ключевая ставка ЦБ).
    pub base_indicator: Option<String>,
    pub day_convention_cd: DayConvention,
    /// Проценты, накопленные к дате расчёта; входят в первый будущий платеж.
    pub accrued_interest: f64,
    pub cap_limit_interest_rate: Option<f64>,
    pub floor_limit_interest_rate: Option<f64>,

    // --- правила сеток дат ---
    pub interest_payment_rule: PaymentRule,
    pub regular_payment_rule: PaymentRule,
    pub repricing_rule: PaymentRule,

    // --- явные списки и графики ---
    /// Даты уплаты процентов, отсортированы, без дублей.
    pub interest_payment: Vec<NaiveDate>,
    /// Явный график тела `дата -> сумма`, отсортирован, без дублей дат.
    pub principal_payment: Vec<(NaiveDate, f64)>,
    /// Явные даты пересмотра ставки.
    pub repricing_dttm_schedule: Vec<NaiveDate>,
    pub repricing_dttm_rate_schedule: Vec<NaiveDate>,
    /// График фиксированной части ставки `дата -> ставка`.
    pub repricing_dttm_rate_schedule_fixed: Vec<(NaiveDate, f64)>,

    // --- классификация ---
    pub product: Option<String>,
    pub behavior_model_cd: Option<String>,

    // --- субсидия ---
    pub sub_rate_type_cd: Option<String>,
    pub sub_interest_rate: Option<f64>,
    pub sub_float_indicator: Option<String>,
    pub sub_payment_rule: PaymentRule,
    pub sub_repricing_rule: PaymentRule,
    pub sub_cap_limit_interest_rate: Option<f64>,
    pub sub_floor_limit_interest_rate: Option<f64>,
    /// Коэффициент к индикатору в ставке субсидии (гипотеза H4; пусто = 1).
    pub sub_coeff_index: Option<f64>,
    /// Признак «первое событие репрайсинга субсидии уже было» (`1`/пусто).
    pub sub_repricing_rule_first_event_flg: Option<bool>,

    // --- прочее ---
    pub deal_report_dttm: Option<NaiveDate>,
    /// Лаг индикатора, например `-1M` (ждём ответа экспертов, docs/QUESTIONS.md).
    pub indicator_lag_cd: Option<String>,
    pub sub_indicator_lag_cd: Option<String>,
}

/// Тип события кэшфлоу — колонка `payment_type` формата 3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaymentType {
    InterestPayment,
    PrincipalPayment,
    ClosePayment,
    Repricing,
    SubPayment,
    SubRepricing,
    /// Зарезервировано: досрочное погашение (формулы пришлют эксперты).
    Prepayment,
}

impl PaymentType {
    pub fn as_str(self) -> &'static str {
        match self {
            PaymentType::InterestPayment => "interest_payment",
            PaymentType::PrincipalPayment => "principal_payment",
            PaymentType::ClosePayment => "close_payment",
            PaymentType::Repricing => "repricing",
            PaymentType::SubPayment => "sub_payment",
            PaymentType::SubRepricing => "sub_repricing",
            PaymentType::Prepayment => "prepayment",
        }
    }
}

/// Поток платежа — колонка `stream` формата 3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    /// Платежи клиента.
    Client,
    /// Субсидия.
    Sub,
}

impl Stream {
    pub fn as_str(self) -> &'static str {
        match self {
            Stream::Client => "client",
            Stream::Sub => "sub",
        }
    }
}

/// Одна строка выходного кэшфлоу (формат 3 контракта).
/// `report_dttm`, `deal_id` и `discriminator` берутся из сделки при записи.
#[derive(Debug, Clone)]
pub struct Event {
    /// Порядок события внутри сделки, с 1.
    pub event_order: u32,
    pub payment_dttm: NaiveDate,
    pub payment_type: PaymentType,
    pub stream: Stream,
    /// Сумма со знаком сделки (депозит — отрицательная);
    /// для `repricing`/`sub_repricing` — 0.
    pub amount: f64,
    /// Остаток тела до события, со знаком сделки.
    pub principal_before: f64,
    /// Остаток тела после события, со знаком сделки.
    pub principal_after: f64,
    /// Ставка, действующая на период начисления (для repricing — новая).
    pub rate: Option<f64>,
    /// Значение индикатора, если ставка плавающая, иначе `None`.
    pub base_rate: Option<f64>,
    pub accrual_start_dttm: Option<NaiveDate>,
    pub accrual_end_dttm: Option<NaiveDate>,
    pub accrual_days: Option<i64>,
    pub year_frac: Option<f64>,
}

/// Серьёзность проблемы со сделкой (файл `errors.csv`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Сделка не посчитана.
    Error,
    /// Посчитана, но есть замечание.
    Warning,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

/// Коды для `errors.csv`. Фиксированы здесь, чтобы совпадать с docs/DECISIONS.md.
pub mod codes {
    /// Валюта не RUB — сделка отклоняется (из валют только рубли, заметка эксперта).
    pub const CURRENCY_NOT_RUB: &str = "CURRENCY_NOT_RUB";
    /// Дата окончания не позже даты расчёта — считать нечего.
    pub const MATURITY_IN_PAST: &str = "MATURITY_IN_PAST";
    /// Сумма явного графика тела отличается от остатка (гипотеза H9:
    /// разница уходит в close_payment, сделка считается с предупреждением).
    pub const PRINCIPAL_SCHEDULE_MISMATCH: &str = "PRINCIPAL_SCHEDULE_MISMATCH";
}

/// Одна запись `errors.csv`: `deal_id,severity,code,field,message`.
#[derive(Debug, Clone)]
pub struct DealError {
    pub deal_id: String,
    pub severity: Severity,
    pub code: &'static str,
    pub field: &'static str,
    pub message: String,
}

/// Параметры расчёта. Каждая гипотеза из docs/CONTRACT.md, которую можно
/// выключить конфигом, — отдельное поле: ответ экспертов меняется одной правкой.
#[derive(Debug, Clone)]
pub struct Config {
    /// H3: сдвиг сценария, добавляемый к будущим значениям индикатора.
    /// В спринте 2 сдвиг нулевой.
    pub scenario_shift: f64,
    /// H6: событие ровно в дату расчёта считается прошедшим.
    pub event_on_report_date_is_past: bool,
    /// H9: расхождение явного графика тела с остатком уходит в close_payment.
    pub schedule_gap_to_close_payment: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            scenario_shift: 0.0,
            event_on_report_date_is_past: true,
            schedule_gap_to_close_payment: true,
        }
    }
}
