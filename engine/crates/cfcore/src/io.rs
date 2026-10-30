//! Чтение входных CSV контракта (формат 1 — сделки, формат 2 — ставки)
//! и запись результатов (формат 3 — кэшфлоу, errors.csv).

use std::collections::BTreeMap;
use std::io::{self, Read, Write};

use chrono::NaiveDate;
use csv::{ReaderBuilder, StringRecord, Writer, WriterBuilder};

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

    let mut deals = Vec::new();
    for rec in reader.records() {
        let rec = rec.map_err(RowError::Csv)?;
        let line = rec
            .position()
            .map(|p| usize::try_from(p.line()).unwrap_or(0))
            .unwrap_or(0);
        match parse_deal_row(&rec, &pos) {
            Ok(deal) => deals.push(deal),
            Err(message) => return Err(RowError::Msg { line, message }),
        }
    }
    Ok(deals)
}

fn parse_deal_row(rec: &StringRecord, pos: &[usize; 48]) -> Result<Deal, String> {
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
        accrued_interest: opt_f64(g(22), "accrued_interest")?.unwrap_or(0.0),
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
    inner: Writer<W>,
}

impl<W: Write> CashflowWriter<W> {
    pub fn from_writer(w: W) -> csv::Result<Self> {
        let mut inner = WriterBuilder::new().from_writer(w);
        inner.write_record(CASHFLOW_HEADER)?;
        Ok(Self { inner })
    }

    pub fn write_deal(&mut self, deal: &Deal, events: &[Event]) -> csv::Result<()> {
        for e in events {
            self.inner.write_record([
                deal.report_dttm.to_string(),
                deal.deal_id.clone(),
                e.event_order.to_string(),
                e.payment_dttm.to_string(),
                e.payment_type.as_str().to_string(),
                e.stream.as_str().to_string(),
                format!("{:.2}", e.amount),
                format!("{:.2}", e.principal_before),
                format!("{:.2}", e.principal_after),
                fmt_opt_f64(e.rate),
                fmt_opt_f64(e.base_rate),
                fmt_opt_date(e.accrual_start_dttm),
                fmt_opt_date(e.accrual_end_dttm),
                e.accrual_days.map(|d| d.to_string()).unwrap_or_default(),
                fmt_opt_f64(e.year_frac),
                deal.discriminator.as_str().to_string(),
            ])?;
        }
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

fn fmt_opt_f64(v: Option<f64>) -> String {
    v.map(|x| x.to_string()).unwrap_or_default()
}

fn fmt_opt_date(v: Option<NaiveDate>) -> String {
    v.map(|d| d.to_string()).unwrap_or_default()
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
}
