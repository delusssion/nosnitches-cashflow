//! Чтение входных CSV контракта (формат 1 — сделки, формат 2 — ставки)
//! и запись результатов (формат 3 — кэшфлоу, errors.csv).

use std::collections::BTreeMap;
use std::io::{self, Read, Write};

use chrono::NaiveDate;
use csv::{ReaderBuilder, StringRecord, WriterBuilder};

use crate::dates;
use crate::model::{DayConvention, Deal, DealError, Discriminator, Event, PaymentRule, RateType};
use crate::rates::{RateBook, RateSeries};

/// Ошибка чтения входного файла: путь и место (строка) в сообщении.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("не удалось прочитать {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("{path}: {message}")]
    Format { path: String, message: String },
    #[error("{path}, строка {line}: {message}")]
    Row {
        path: String,
        line: usize,
        message: String,
    },
}

impl ParseError {
    fn from_csv(path: &str, err: csv::Error) -> Self {
        match err.position() {
            Some(pos) => ParseError::Row {
                path: path.to_string(),
                line: usize::try_from(pos.line()).unwrap_or(0),
                message: err.to_string(),
            },
            None => ParseError::Format {
                path: path.to_string(),
                message: err.to_string(),
            },
        }
    }
}

/// Колонки файла сделок — ровно 48 полей формата 1 из docs/CONTRACT.md.
pub const DEAL_COLUMNS: [&str; 48] = [
    "report_dttm",
    "discriminator",
    "deal_id",
    "currency_id",
    "start_principal_amt",
    "current_principal_amt",
    "deal_dttm",
    "maturity_dttm",
    "rate_type_cd",
    "interest_rate",
    "base_indicator_rate",
    "base_indicator",
    "interest_payment_rule_start_dttm",
    "interest_payment_rule_period_cd",
    "interest_payment_rule_day_num",
    "day_convention_cd",
    "regular_payment_rule_start_dttm",
    "regular_payment_rule_period_cd",
    "regular_payment_rule_day_num",
    "repricing_rule_start_dttm",
    "repricing_rule_period_cd",
    "repricing_rule_day_num",
    "accrued_interest",
    "product",
    "behavior_model_cd",
    "interest_payment",
    "principal_payment",
    "repricing_dttm_schedule",
    "repricing_dttm_rate_schedule",
    "repricing_dttm_rate_schedule_fixed",
    "cap_limit_interest_rate",
    "floor_limit_interest_rate",
    "sub_rate_type_cd",
    "sub_interest_rate",
    "sub_float_indicator",
    "sub_payment_rule_period_cd",
    "sub_payment_rule_day_num",
    "sub_payment_rule_start_dttm",
    "sub_repricing_rule_start_dttm",
    "sub_repricing_rule_period_cd",
    "sub_repricing_rule_day_num",
    "sub_cap_limit_interest_rate",
    "sub_floor_limit_interest_rate",
    "sub_coeff_index",
    "deal_report_dttm",
    "sub_repricing_rule_first_event_flg",
    "indicator_lag_cd",
    "sub_indicator_lag_cd",
];

// ---------- низкоуровневые разборы значений ----------

fn opt_str(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

fn req_str(s: &str, field: &str) -> Result<String, String> {
    opt_str(s).ok_or_else(|| format!("поле {field} обязательно, но пустое"))
}

fn opt_f64(s: &str, field: &str) -> Result<Option<f64>, String> {
    match opt_str(s) {
        None => Ok(None),
        Some(v) => v
            .parse::<f64>()
            .map(Some)
            .map_err(|_| format!("поле {field}: «{v}» не число")),
    }
}

fn req_f64(s: &str, field: &str) -> Result<f64, String> {
    opt_f64(s, field)?.ok_or_else(|| format!("поле {field} обязательно, но пустое"))
}

fn parse_date(s: &str, field: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d")
        .map_err(|_| format!("поле {field}: «{}» не дата YYYY-MM-DD", s.trim()))
}

fn opt_date(s: &str, field: &str) -> Result<Option<NaiveDate>, String> {
    match opt_str(s) {
        None => Ok(None),
        Some(v) => parse_date(&v, field).map(Some),
    }
}

fn opt_i32(s: &str, field: &str) -> Result<Option<i32>, String> {
    match opt_str(s) {
        None => Ok(None),
        Some(v) => v
            .parse::<i32>()
            .map(Some)
            .map_err(|_| format!("поле {field}: «{v}» не целое")),
    }
}

fn opt_bool_flag(s: &str) -> Option<bool> {
    match opt_str(s).as_deref() {
        None => None,
        Some("1") => Some(true),
        Some("0") => Some(false),
        Some(v) => Some(!v.is_empty()),
    }
}

// ---------- чтение сделок ----------

/// Прочитать сделки из файла (формат 1 контракта).
pub fn read_deals(path: &std::path::Path) -> Result<Vec<Deal>, ParseError> {
    let file = io::BufReader::new(std::fs::File::open(path).map_err(|e| ParseError::Io {
        path: path.display().to_string(),
        source: e,
    })?);
    parse_deals(file).map_err(|e| match e {
        RowError::Csv(err) => ParseError::from_csv(&path.display().to_string(), err),
        RowError::Msg { line, message } => ParseError::Row {
            path: path.display().to_string(),
            line,
            message,
        },
    })
}

/// Ошибка разбора одной строки входного CSV.
#[derive(Debug)]
pub enum RowError {
    Csv(csv::Error),
    Msg { line: usize, message: String },
}

/// Разбор сделок из потока. Используется и в тестах.
pub fn parse_deals<R: Read>(rdr: R) -> Result<Vec<Deal>, RowError> {
    let mut reader = ReaderBuilder::new().from_reader(rdr);
    let header: StringRecord = reader.headers().map_err(RowError::Csv)?.clone();
    let col = |name: &str| -> Result<usize, RowError> {
        header
            .iter()
            .position(|h| h.trim().trim_start_matches('\u{feff}') == name)
            .ok_or_else(|| RowError::Msg {
                line: 1,
                message: format!("в заголовке нет колонки {name}"),
            })
    };
    // позиции всех 48 колонок ищем один раз
    let mut pos = [0usize; 48];
    for (i, name) in DEAL_COLUMNS.iter().enumerate() {
        pos[i] = col(name)?;
    }
    let pos_prepay = header.iter().position(|h| {
        let s = h.trim().trim_start_matches('\u{feff}');
        s == "prepayment" || s == "prepayment_schedule"
    });

    let mut deals = Vec::new();
    for rec in reader.records() {
        let rec = rec.map_err(RowError::Csv)?;
        let line = rec
            .position()
            .map(|p| usize::try_from(p.line()).unwrap_or(0))
            .unwrap_or(0);
        match parse_deal_row(&rec, &pos, pos_prepay) {
            Ok(deal) => deals.push(deal),
            Err(message) => return Err(RowError::Msg { line, message }),
        }
    }
    Ok(deals)
}

fn parse_deal_row(
    rec: &StringRecord,
    pos: &[usize; 48],
    pos_prepay: Option<usize>,
) -> Result<Deal, String> {
    let g = |i: usize| -> &str { rec.get(pos[i]).unwrap_or("") };

    let day_convention = match req_str(g(15), "day_convention_cd")?.as_str() {
        "ACT/365" => DayConvention::Act365,
        // H5: ACT/ACT считаем как ACT/ACT ISDA
        "ACT/ACT" | "ACT/ACT ISDA" => DayConvention::ActActIsda,
        other => {
            return Err(format!(
                "поле day_convention_cd: неизвестная конвенция «{other}»"
            ))
        }
    };
    let rule = |i: usize, prefix: &str| -> Result<PaymentRule, String> {
        Ok(PaymentRule {
            start_dttm: opt_date(g(i), &format!("{prefix}_start_dttm"))?,
            period_cd: match opt_str(g(i + 1)) {
                None => None,
                Some(v) => Some(
                    dates::parse_period_cd(&v)
                        .ok_or_else(|| format!("поле {prefix}_period_cd: «{v}» не период"))?,
                ),
            },
            day_num: opt_i32(g(i + 2), &format!("{prefix}_day_num"))?,
        })
    };

    Ok(Deal {
        report_dttm: parse_date(g(0), "report_dttm")?,
        discriminator: match req_str(g(1), "discriminator")?.as_str() {
            "ANN" => Discriminator::Ann,
            "RGM" => Discriminator::Rgm,
            "SIMPLE" => Discriminator::Simple,
            other => return Err(format!("поле discriminator: «{other}» не ANN/RGM/SIMPLE")),
        },
        deal_id: req_str(g(2), "deal_id")?,
        currency_id: req_str(g(3), "currency_id")?,
        start_principal_amt: opt_f64(g(4), "start_principal_amt")?,
        current_principal_amt: req_f64(g(5), "current_principal_amt")?,
        deal_dttm: opt_date(g(6), "deal_dttm")?,
        maturity_dttm: parse_date(g(7), "maturity_dttm")?,

        rate_type_cd: match req_str(g(8), "rate_type_cd")?.as_str() {
            "FIXED" => RateType::Fixed,
            "FLOAT" => RateType::Float,
            other => return Err(format!("поле rate_type_cd: «{other}» не FIXED/FLOAT")),
        },
        interest_rate: req_f64(g(9), "interest_rate")?,
        base_indicator_rate: opt_f64(g(10), "base_indicator_rate")?,
        base_indicator: opt_str(g(11)),
        day_convention_cd: day_convention,
        accrued_interest: opt_f64(g(22), "accrued_interest")?,
        cap_limit_interest_rate: opt_f64(g(30), "cap_limit_interest_rate")?,
        floor_limit_interest_rate: opt_f64(g(31), "floor_limit_interest_rate")?,

        interest_payment_rule: rule(12, "interest_payment_rule")?,
        regular_payment_rule: rule(16, "regular_payment_rule")?,
        repricing_rule: rule(19, "repricing_rule")?,

        interest_payment: if g(25).trim().is_empty() {
            Vec::new()
        } else {
            dates::parse_date_list(g(25))?
        },
        principal_payment: if g(26).trim().is_empty() {
            Vec::new()
        } else {
            dates::parse_pairs(g(26))?
        },
        repricing_dttm_schedule: if g(27).trim().is_empty() {
            Vec::new()
        } else {
            dates::parse_date_list(g(27))?
        },
        repricing_dttm_rate_schedule: if g(28).trim().is_empty() {
            Vec::new()
        } else {
            dates::parse_date_list(g(28))?
        },
        repricing_dttm_rate_schedule_fixed: if g(29).trim().is_empty() {
            Vec::new()
        } else {
            dates::parse_pairs(g(29))?
        },

        product: opt_str(g(23)),
        behavior_model_cd: opt_str(g(24)),

        sub_rate_type_cd: opt_str(g(32)),
        sub_interest_rate: opt_f64(g(33), "sub_interest_rate")?,
        sub_float_indicator: opt_str(g(34)),
        // у субсидии колонки идут в порядке period, day, start — не как у
        // клиентских правил (start, period, day), поэтому собираем вручную
        sub_payment_rule: PaymentRule {
            period_cd: match opt_str(g(35)) {
                None => None,
                Some(v) => {
                    Some(dates::parse_period_cd(&v).ok_or_else(|| {
                        format!("поле sub_payment_rule_period_cd: «{v}» не период")
                    })?)
                }
            },
            day_num: opt_i32(g(36), "sub_payment_rule_day_num")?,
            start_dttm: opt_date(g(37), "sub_payment_rule_start_dttm")?,
        },
        sub_repricing_rule: rule(38, "sub_repricing_rule")?,
        sub_cap_limit_interest_rate: opt_f64(g(41), "sub_cap_limit_interest_rate")?,
        sub_floor_limit_interest_rate: opt_f64(g(42), "sub_floor_limit_interest_rate")?,
        sub_coeff_index: opt_f64(g(43), "sub_coeff_index")?,
        sub_repricing_rule_first_event_flg: opt_bool_flag(g(45)),

        deal_report_dttm: opt_date(g(44), "deal_report_dttm")?,
        indicator_lag_cd: opt_str(g(46)),
        sub_indicator_lag_cd: opt_str(g(47)),
        prepayment_schedule: match pos_prepay.and_then(|idx| rec.get(idx)) {
            Some(s) if !s.trim().is_empty() => dates::parse_pairs(s)?,
            _ => Vec::new(),
        },
    })
}

// ---------- чтение ставок ----------

/// Прочитать историю индикаторов из файла (формат 2: `indicator,date,rate`).
pub fn read_rates(path: &std::path::Path) -> Result<RateBook, ParseError> {
    let file = io::BufReader::new(std::fs::File::open(path).map_err(|e| ParseError::Io {
        path: path.display().to_string(),
        source: e,
    })?);
    parse_rates(file).map_err(|e| match e {
        RowError::Csv(err) => ParseError::from_csv(&path.display().to_string(), err),
        RowError::Msg { line, message } => ParseError::Row {
            path: path.display().to_string(),
            line,
            message,
        },
    })
}

fn parse_rates<R: Read>(rdr: R) -> Result<RateBook, RowError> {
    let mut reader = ReaderBuilder::new().from_reader(rdr);
    let header = reader.headers().map_err(RowError::Csv)?.clone();
    let find = |name: &str| {
        header
            .iter()
            .position(|h| h.trim().trim_start_matches('\u{feff}') == name)
    };
    let (Some(i_ind), Some(i_date), Some(i_rate)) = (find("indicator"), find("date"), find("rate"))
    else {
        return Err(RowError::Msg {
            line: 1,
            message: "ожидаются колонки indicator,date,rate".into(),
        });
    };

    let mut grouped: BTreeMap<String, Vec<(NaiveDate, f64)>> = BTreeMap::new();
    for rec in reader.records() {
        let rec = rec.map_err(RowError::Csv)?;
        let line = rec
            .position()
            .map(|p| usize::try_from(p.line()).unwrap_or(0))
            .unwrap_or(0);
        let indicator = rec.get(i_ind).unwrap_or("").trim();
        let date_s = rec.get(i_date).unwrap_or("");
        let rate_s = rec.get(i_rate).unwrap_or("");
        if indicator.is_empty() {
            return Err(RowError::Msg {
                line,
                message: "пустой indicator".into(),
            });
        }
        let date = parse_date(date_s, "date").map_err(|m| RowError::Msg { line, message: m })?;
        let rate: f64 = rate_s.trim().parse().map_err(|_| RowError::Msg {
            line,
            message: format!("поле rate: «{rate_s}» не число"),
        })?;
        grouped
            .entry(indicator.to_string())
            .or_default()
            .push((date, rate));
    }

    let mut book = RateBook::new();
    for (indicator, steps) in grouped {
        book.insert(RateSeries::new(indicator, steps));
    }
    Ok(book)
}

// ---------- запись результатов ----------

/// Заголовок файла кэшфлоу (формат 3 контракта).
pub const CASHFLOW_HEADER: [&str; 16] = [
    "report_dttm",
    "deal_id",
    "event_order",
    "payment_dttm",
    "payment_type",
    "stream",
    "amount",
    "principal_before",
    "principal_after",
    "rate",
    "base_rate",
    "accrual_start_dttm",
    "accrual_end_dttm",
    "accrual_days",
    "year_frac",
    "discriminator",
];

/// Потоковая запись кэшфлоу: заголовок пишется при создании, сделки
/// дописываются по мере расчёта — ничего не копится в памяти (цель E10:
/// миллион сделок меньше 500 МБ).
pub struct CashflowWriter<W: Write> {
    inner: W,
    buf: Vec<u8>,
}

impl<W: Write> CashflowWriter<W> {
    pub fn from_writer(mut w: W) -> csv::Result<Self> {
        w.write_all(CASHFLOW_HEADER.join(",").as_bytes())?;
        w.write_all(b"\n")?;
        Ok(Self {
            inner: w,
            buf: Vec::with_capacity(64 * 1024),
        })
    }

    pub fn write_deal(&mut self, deal: &Deal, events: &[Event]) -> csv::Result<()> {
        self.buf.clear();
        format_deal(&mut self.buf, deal, events);
        self.inner.write_all(&self.buf)?;
        Ok(())
    }

    pub fn flush(&mut self) -> csv::Result<()> {
        Ok(self.inner.flush()?)
    }
}

/// Запись `errors.csv`: `deal_id,severity,code,field,message`.
pub fn write_errors_csv<W: Write>(w: W, errors: &[DealError]) -> csv::Result<()> {
    let mut writer = WriterBuilder::new().from_writer(w);
    writer.write_record(["deal_id", "severity", "code", "field", "message"])?;
    for e in errors {
        writer.write_record([&e.deal_id, e.severity.as_str(), e.code, e.field, &e.message])?;
    }
    writer.flush()?;
    Ok(())
}

// ---------- потоковое чтение (B4, B9) ----------

/// Потоковое чтение сделок: по одной, буфер записи переиспользуется, память
/// не растёт с размером файла. Строка, которую не удалось разобрать, не рвёт
/// чтение: она возвращается как [`DealError`] с severity `error` (сделка
/// пропускается, остальные считаются дальше).
pub struct DealReader<R: Read> {
    reader: csv::Reader<R>,
    pos: [usize; 48],
    /// Необязательная колонка досрочек (`prepayment_schedule` / `prepayment`).
    pos_prepay: Option<usize>,
    record: StringRecord,
    done: bool,
}

impl<R: Read> DealReader<R> {
    /// Читает заголовок; ошибка — если в нём нет какой-то из 48 колонок.
    pub fn new(rdr: R) -> Result<Self, RowError> {
        let mut reader = ReaderBuilder::new().from_reader(rdr);
        let header: StringRecord = reader.headers().map_err(RowError::Csv)?.clone();
        let mut pos = [0usize; 48];
        for (i, name) in DEAL_COLUMNS.iter().enumerate() {
            pos[i] = header
                .iter()
                .position(|h| h.trim().trim_start_matches('\u{feff}') == *name)
                .ok_or_else(|| RowError::Msg {
                    line: 1,
                    message: format!("в заголовке нет колонки {name}"),
                })?;
        }
        let pos_prepay = header.iter().position(|h| {
            let s = h.trim().trim_start_matches('\u{feff}');
            s == "prepayment" || s == "prepayment_schedule"
        });
        Ok(Self {
            reader,
            pos,
            pos_prepay,
            record: StringRecord::new(),
            done: false,
        })
    }

    /// Следующая сделка: `Some(Ok)` — разобрана, `Some(Err)` — строка с
    /// ошибкой (в `errors.csv`), `None` — конец файла или ошибка ввода-вывода
    /// (она тоже возвращается один раз как `Some(Err)`).
    pub fn next_deal(&mut self) -> Option<Result<Deal, DealError>> {
        if self.done {
            return None;
        }
        match self.reader.read_record(&mut self.record) {
            Ok(false) => {
                self.done = true;
                None
            }
            Ok(true) => {
                let deal_id = self
                    .record
                    .get(self.pos[2])
                    .unwrap_or("")
                    .trim()
                    .to_string();
                Some(
                    parse_deal_row(&self.record, &self.pos, self.pos_prepay).map_err(|message| {
                        let (code, field) = classify_row_error(&message);
                        DealError {
                            deal_id,
                            severity: crate::model::Severity::Error,
                            code,
                            field,
                            message,
                        }
                    }),
                )
            }
            Err(err) => {
                if err.is_io_error() {
                    self.done = true;
                }
                Some(Err(DealError {
                    deal_id: String::new(),
                    severity: crate::model::Severity::Error,
                    code: "BAD_CSV",
                    field: "",
                    message: err.to_string(),
                }))
            }
        }
    }
}

/// Код и поле ошибки разбора по тексту сообщения «поле <имя>: …» (коды B4).
fn classify_row_error(message: &str) -> (&'static str, &'static str) {
    let field = message
        .strip_prefix("поле ")
        .and_then(|rest| rest.split([' ', ':']).next())
        .and_then(|name| DEAL_COLUMNS.iter().copied().find(|c| *c == name))
        .unwrap_or("");
    let code = if message.contains("обязательно") {
        "MISSING_FIELD"
    } else if message.contains("ANN/RGM/SIMPLE") {
        "UNKNOWN_DISCRIMINATOR"
    } else if message.contains("не период") {
        "BAD_PERIOD"
    } else if message.contains("не дата") {
        "BAD_DATE"
    } else if message.contains("не число") || message.contains("не целое") {
        "BAD_NUMBER"
    } else {
        "BAD_FIELD"
    };
    (code, field)
}

// ---------- быстрая запись формата 3 (B9) ----------

/// Пишет события сделки в CSV формата 3 в буфер `buf` (без заголовка).
///
/// Числа форматируются целочисленной арифметикой (`itoa`), без `format!` —
/// это горячий цикл: на миллионе синтетических сделок сотни миллионов строк.
/// Суммы — 2 знака, ставки — до 10 знаков без хвостовых нулей, доля года —
/// ровно 10 знаков. Буфер можно заполнять в любом потоке и писать потом.
pub fn format_deal(buf: &mut Vec<u8>, deal: &Deal, events: &[Event]) {
    let mut report = [0u8; 10];
    put_date(&mut report, deal.report_dttm);
    let mut id = Vec::with_capacity(deal.deal_id.len() + 2);
    push_field(&mut id, &deal.deal_id);
    let disc = deal.discriminator.as_str().as_bytes();
    let mut ib = itoa::Buffer::new();
    let mut d = [0u8; 10];
    for e in events {
        buf.extend_from_slice(&report);
        buf.push(b',');
        buf.extend_from_slice(&id);
        buf.push(b',');
        buf.extend_from_slice(ib.format(e.event_order).as_bytes());
        buf.push(b',');
        put_date(&mut d, e.payment_dttm);
        buf.extend_from_slice(&d);
        buf.push(b',');
        buf.extend_from_slice(e.payment_type.as_str().as_bytes());
        buf.push(b',');
        buf.extend_from_slice(e.stream.as_str().as_bytes());
        buf.push(b',');
        push_fixed(buf, e.amount, 2, false);
        buf.push(b',');
        push_fixed(buf, e.principal_before, 2, false);
        buf.push(b',');
        push_fixed(buf, e.principal_after, 2, false);
        buf.push(b',');
        if let Some(r) = e.rate {
            push_fixed(buf, r, 10, true);
        }
        buf.push(b',');
        if let Some(r) = e.base_rate {
            push_fixed(buf, r, 10, true);
        }
        buf.push(b',');
        if let Some(a) = e.accrual_start_dttm {
            put_date(&mut d, a);
            buf.extend_from_slice(&d);
        }
        buf.push(b',');
        if let Some(a) = e.accrual_end_dttm {
            put_date(&mut d, a);
            buf.extend_from_slice(&d);
        }
        buf.push(b',');
        if let Some(n) = e.accrual_days {
            buf.extend_from_slice(ib.format(n).as_bytes());
        }
        buf.push(b',');
        if let Some(y) = e.year_frac {
            push_fixed(buf, y, 10, false);
        }
        buf.push(b',');
        buf.extend_from_slice(disc);
        buf.push(b'\n');
    }
}

/// Поле CSV: в кавычках, только если есть запятая, кавычка или перевод строки.
fn push_field(buf: &mut Vec<u8>, s: &str) {
    if s.bytes().any(|b| matches!(b, b',' | b'"' | b'\n' | b'\r')) {
        buf.push(b'"');
        for b in s.bytes() {
            if b == b'"' {
                buf.push(b'"');
            }
            buf.push(b);
        }
        buf.push(b'"');
    } else {
        buf.extend_from_slice(s.as_bytes());
    }
}

/// Дата в `YYYY-MM-DD` без аллокаций.
fn put_date(out: &mut [u8; 10], d: NaiveDate) {
    use chrono::Datelike;
    let y = d.year().clamp(0, 9999) as u32;
    let (m, day) = (d.month(), d.day());
    out[0] = b'0' + (y / 1000) as u8;
    out[1] = b'0' + (y / 100 % 10) as u8;
    out[2] = b'0' + (y / 10 % 10) as u8;
    out[3] = b'0' + (y % 10) as u8;
    out[4] = b'-';
    out[5] = b'0' + (m / 10) as u8;
    out[6] = b'0' + (m % 10) as u8;
    out[7] = b'-';
    out[8] = b'0' + (day / 10) as u8;
    out[9] = b'0' + (day % 10) as u8;
}

/// Число с фиксированным числом знаков, как `format!("{v:.N}")`: `-0` пишется
/// как `0`, `trim` убирает хвостовые нули. Быстрый путь — целочисленный
/// (`itoa`); значения на границе половины младшего разряда (…,125) идут через
/// `format!`, чтобы округление совпадало с ним и с эталоном на Python точно.
fn push_fixed(buf: &mut Vec<u8>, v: f64, decimals: u32, trim: bool) {
    let scale = 10u64.pow(decimals);
    let x = v * scale as f64;
    let near_half = (x - x.floor() - 0.5).abs() < 1e-6;
    if near_half || !x.is_finite() || x.abs() >= 9.0e18 {
        let s = format!("{v:.prec$}", prec = decimals as usize);
        let s = if s.bytes().all(|b| matches!(b, b'-' | b'0' | b'.')) {
            s.trim_start_matches('-')
        } else {
            s.as_str()
        };
        let s = if trim && s.contains('.') {
            s.trim_end_matches('0').trim_end_matches('.')
        } else {
            s
        };
        buf.extend_from_slice(s.as_bytes());
        return;
    }
    let scaled = x.round();
    let neg = scaled < 0.0;
    let a = scaled.abs() as u64;
    let (int, mut frac) = (a / scale, a % scale);
    if neg && a != 0 {
        buf.push(b'-');
    }
    let mut ib = itoa::Buffer::new();
    buf.extend_from_slice(ib.format(int).as_bytes());
    let mut digits = decimals as usize;
    if trim {
        while digits > 0 && frac % 10 == 0 {
            frac /= 10;
            digits -= 1;
        }
        if digits == 0 {
            return;
        }
    }
    buf.push(b'.');
    let s = ib.format(frac);
    for _ in s.len()..digits {
        buf.push(b'0');
    }
    buf.extend_from_slice(s.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deal_columns_match_contract() {
        // страховка от случайной правки порядка колонок
        assert_eq!(DEAL_COLUMNS[0], "report_dttm");
        assert_eq!(DEAL_COLUMNS[47], "sub_indicator_lag_cd");
        assert_eq!(DEAL_COLUMNS.len(), 48);
        assert_eq!(CASHFLOW_HEADER.len(), 16);
    }

    #[test]
    fn fixed_point_formatting() {
        let cases: [(f64, u32, bool, &str); 12] = [
            (1234.5, 2, false, "1234.50"),
            (-0.004, 2, false, "0.00"),
            (-1896648.88, 2, false, "-1896648.88"),
            (0.0849315068493, 10, false, "0.0849315068"),
            (0.19, 10, true, "0.19"),
            (0.075, 10, true, "0.075"),
            (1.0, 10, true, "1"),
            (0.0, 10, true, "0"),
            (0.0, 2, false, "0.00"),
            (0.155, 10, true, "0.155"),
            (2605973.125, 2, false, "2605973.12"),
            (-360611.685, 2, false, "-360611.68"),
        ];
        for (v, dec, trim, want) in cases {
            let mut b = Vec::new();
            push_fixed(&mut b, v, dec, trim);
            assert_eq!(String::from_utf8(b).unwrap(), want, "{v} {dec} {trim}");
        }
    }

    #[test]
    fn row_errors_are_classified() {
        let cases = [
            (
                "поле current_principal_amt: «x» не число",
                "BAD_NUMBER",
                "current_principal_amt",
            ),
            (
                "поле maturity_dttm: «31.12.2022» не дата YYYY-MM-DD",
                "BAD_DATE",
                "maturity_dttm",
            ),
            (
                "поле discriminator: «XXX» не ANN/RGM/SIMPLE",
                "UNKNOWN_DISCRIMINATOR",
                "discriminator",
            ),
            (
                "поле deal_id обязательно, но пустое",
                "MISSING_FIELD",
                "deal_id",
            ),
            (
                "поле interest_payment_rule_period_cd: «7Q» не период",
                "BAD_PERIOD",
                "interest_payment_rule_period_cd",
            ),
        ];
        for (msg, code, field) in cases {
            assert_eq!(classify_row_error(msg), (code, field), "{msg}");
        }
    }
}
