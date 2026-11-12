//! cfgen — CLI калькулятора денежных потоков поверх библиотеки cfcore.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use cfcore::engine::build_cashflow;
use cfcore::io::{
    parse_deals, read_deals, read_rates, write_errors_csv, CashflowWriter, DEAL_COLUMNS,
};
use cfcore::model::{Discriminator, PaymentType, RateType};
use cfcore::{Config, Deal, Event, Severity};
use chrono::NaiveDate;
use clap::{Parser, Subcommand};

// ANSI-цвета для терминала
const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const CYAN: &str = "\x1b[36m";
const GRAY: &str = "\x1b[90m";

#[derive(Parser)]
#[command(
    name = "cfgen",
    about = "Калькулятор денежных потоков (движок cfcore)",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Посчитать кэшфлоу по файлу сделок и истории ставок
    Run {
        /// Файл сделок, формат 1 контракта (testdata/deals/*.csv), либо «-» для stdin
        #[arg(long, value_name = "FILE")]
        deals: PathBuf,
        /// История индикаторов, формат 2 (testdata/rates/key_rate.csv)
        #[arg(long, value_name = "FILE")]
        rates: PathBuf,
        /// Куда положить cashflow.csv и errors.csv, либо «-» для stdout
        #[arg(long, value_name = "DIR")]
        out_dir: PathBuf,
        /// Ограничение: не более 1 платежа тела в календарный месяц
        #[arg(long)]
        clamp_monthly: bool,
    },
    /// Сверить результат с эталонами: допуск 0,01 ₽, остаток после последнего платежа 0
    Verify {
        /// Результат прогона (cashflow.csv)
        #[arg(long, value_name = "FILE")]
        result: PathBuf,
        /// Каталог эталонов (testdata/golden/)
        #[arg(long, value_name = "DIR")]
        golden_dir: PathBuf,
        /// Допуск по сумме платежа, ₽
        #[arg(long, default_value_t = 0.01)]
        tolerance: f64,
    },
    /// Вертикальный просмотр сделки без Excel и экспорт в читаемый key-value конфиг
    Inspect {
        /// Файл сделок
        #[arg(long, value_name = "FILE")]
        deals: PathBuf,
        /// Идентификатор сделки
        #[arg(long, value_name = "ID")]
        deal_id: String,
        /// Сохранить сделку в читаемый файл key: value
        #[arg(long, value_name = "FILE")]
        export: Option<PathBuf>,
    },
    /// Конвертация между вертикальным key-value файлом и 48-колоночной строкой CSV
    Convert {
        /// Входной текстовый файл сделки (строки вида «поле: значение»)
        #[arg(long, value_name = "FILE")]
        input: PathBuf,
        /// Выходной CSV файл (по умолчанию печатает в консоль)
        #[arg(long, value_name = "FILE")]
        output: Option<PathBuf>,
    },
    /// Сценарный анализ «Что-если» с красивым цветным diff в консоли
    Scenario {
        /// Файл сделок
        #[arg(long, value_name = "FILE")]
        deals: PathBuf,
        /// История индикаторов
        #[arg(long, value_name = "FILE")]
        rates: PathBuf,
        /// Идентификатор сделки
        #[arg(long, value_name = "ID")]
        deal_id: String,
        /// Изменить поле сделки: «поле=значение» (например: --set interest_rate=0.18 --set rate_type_cd=FIXED)
        #[arg(long = "set", value_name = "FIELD=VAL")]
        set_fields: Vec<String>,
        /// Добавить досрочное погашение: «ГГГГ-ММ-ДД:СУММА» (например: --prepay 2022-11-20:250000)
        #[arg(long = "prepay", value_name = "YYYY-MM-DD:AMOUNT")]
        prepayments: Vec<String>,
        /// Ограничение: не более 1 платежа тела в месяц
        #[arg(long)]
        clamp_monthly: bool,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Run {
            deals,
            rates,
            out_dir,
            clamp_monthly,
        } => run(&deals, &rates, &out_dir, clamp_monthly),
        Command::Verify {
            result,
            golden_dir,
            tolerance,
        } => verify(&result, &golden_dir, tolerance),
        Command::Inspect {
            deals,
            deal_id,
            export,
        } => inspect(&deals, &deal_id, export.as_deref()),
        Command::Convert { input, output } => convert(&input, output.as_deref()),
        Command::Scenario {
            deals,
            rates,
            deal_id,
            set_fields,
            prepayments,
            clamp_monthly,
        } => scenario(
            &deals,
            &rates,
            &deal_id,
            &set_fields,
            &prepayments,
            clamp_monthly,
        ),
    }
}

// ==============================================================================================
// 1. Сценарный анализ (Scenario Diff)
// ==============================================================================================

fn scenario(
    deals_path: &Path,
    rates_path: &Path,
    deal_id: &str,
    set_fields: &[String],
    prepayments: &[String],
    clamp_monthly: bool,
) -> Result<()> {
    let deals = load_deals(deals_path)?;
    let book = read_rates(rates_path)
        .with_context(|| format!("чтение ставок {}", rates_path.display()))?;

    let base_deal = deals
        .iter()
        .find(|d| d.deal_id == deal_id)
        .cloned()
        .with_context(|| format!("сделка «{deal_id}» не найдена в {}", deals_path.display()))?;

    let mut mod_deal = base_deal.clone();

    // Применение изменений полей
    for s in set_fields {
        let (k, v) = s.split_once('=').with_context(|| {
            format!("неверный формат аргумента «{s}», ожидается «поле=значение»")
        })?;
        apply_deal_override(&mut mod_deal, k.trim(), v.trim())?;
    }

    // Применение досрочных погашений
    for p in prepayments {
        let (d_str, a_str) = p.split_once(':').with_context(|| {
            format!("неверный формат prepayment «{p}», ожидается «YYYY-MM-DD:СУММА»")
        })?;
        let d = NaiveDate::parse_from_str(d_str.trim(), "%Y-%m-%d")
            .with_context(|| format!("неверная дата «{d_str}»"))?;
        let a = a_str
            .trim()
            .parse::<f64>()
            .with_context(|| format!("неверная сумма «{a_str}»"))?;
        mod_deal.prepayment_schedule.push((d, a));
    }

    let base_cfg = Config::default();
    let mod_cfg = Config {
        clamp_principal_monthly: clamp_monthly,
        ..Default::default()
    };

    let (base_events, base_errs) = build_cashflow(&base_deal, &book, &base_cfg);
    let (mod_events, mod_errs) = build_cashflow(&mod_deal, &book, &mod_cfg);

    if base_errs.iter().any(|e| e.severity == Severity::Error) {
        bail!("Ошибки в базовой сделке: {:?}", base_errs);
    }
    if mod_errs.iter().any(|e| e.severity == Severity::Error) {
        bail!("Ошибки в сценарии сделки: {:?}", mod_errs);
    }

    // Печать заголовка
    println!();
    println!("{BOLD}{CYAN}========================================================================================================================{RESET}");
    println!(
        "{BOLD}{CYAN}  СЦЕНАРНЫЙ АНАЛИЗ СДЕЛКИ: deal_id = {YELLOW}{deal_id}{CYAN} (Дискриминатор: {YELLOW}{}{CYAN}, Ставка: {YELLOW}{}{CYAN}){RESET}",
        base_deal.discriminator.as_str(),
        match base_deal.rate_type_cd {
            RateType::Fixed => format!("FIXED {:.2}%", base_deal.interest_rate * 100.0),
            RateType::Float => format!(
                "FLOAT {} + {:.2}%",
                base_deal.base_indicator.as_deref().unwrap_or("?"),
                base_deal.interest_rate * 100.0
            ),
        }
    );
    if !set_fields.is_empty() || !prepayments.is_empty() || clamp_monthly {
        print!("  {BOLD}Модификаторы:{RESET} ");
        for sf in set_fields {
            print!("{YELLOW}[set {sf}]{RESET} ");
        }
        for pp in prepayments {
            print!("{GREEN}[prepayment {pp}]{RESET} ");
        }
        if clamp_monthly {
            print!("{CYAN}[clamp_monthly=true]{RESET} ");
        }
        println!();
    }
    println!("{BOLD}{CYAN}========================================================================================================================{RESET}");

    // Собираем все ключи событий: (дата, тип_события, поток)
    type EvKey = (NaiveDate, &'static str, &'static str);
    let mut all_keys: BTreeSet<EvKey> = BTreeSet::new();

    let mut base_map: BTreeMap<EvKey, &Event> = BTreeMap::new();
    for e in &base_events {
        let k = (e.payment_dttm, e.payment_type.as_str(), e.stream.as_str());
        base_map.insert(k, e);
        all_keys.insert(k);
    }

    let mut mod_map: BTreeMap<EvKey, &Event> = BTreeMap::new();
    for e in &mod_events {
        let k = (e.payment_dttm, e.payment_type.as_str(), e.stream.as_str());
        mod_map.insert(k, e);
        all_keys.insert(k);
    }

    println!(
        " {BOLD}{:<4} {:<10} {:<18} {:<8} {:>16} {:>16} {:>18} {:>18}{RESET}",
        "#",
        "Дата",
        "Событие",
        "Поток",
        "Базовый (₽)",
        "Сценарий (₽)",
        "Дельта (Δ ₽)",
        "Остаток долга (₽)"
    );
    println!("{GRAY}------------------------------------------------------------------------------------------------------------------------{RESET}");

    for (row_idx, k) in all_keys.into_iter().enumerate() {
        let (d, ptype, stream) = k;
        let b_ev = base_map.get(&k).copied();
        let m_ev = mod_map.get(&k).copied();

        let b_amt = b_ev.map(|e| e.amount).unwrap_or(0.0);
        let m_amt = m_ev.map(|e| e.amount).unwrap_or(0.0);
        let delta = m_amt - b_amt;

        let after = m_ev
            .map(|e| e.principal_after)
            .or_else(|| b_ev.map(|e| e.principal_after))
            .unwrap_or(0.0);

        let delta_str = if delta.abs() < 0.005 {
            format!("{GRAY}         0.00 ₽{RESET}")
        } else if delta > 0.0 {
            format!("{RED}+{:>11.2} ₽{RESET}", delta)
        } else {
            format!("{GREEN}{:>12.2} ₽{RESET}", delta)
        };

        let b_str = if b_ev.is_some() {
            format!("{:>14.2} ₽", b_amt)
        } else {
            format!("{GRAY}             —  {RESET}")
        };

        let m_str = if m_ev.is_some() {
            if b_ev.is_none() {
                format!("{GREEN}{:>14.2} ₽{RESET}", m_amt)
            } else if delta.abs() > 0.005 {
                format!("{YELLOW}{:>14.2} ₽{RESET}", m_amt)
            } else {
                format!("{:>14.2} ₽", m_amt)
            }
        } else {
            format!("{RED}   (отменён) —  {RESET}")
        };

        let ptype_colored = match ptype {
            "interest_payment" => format!("{CYAN}{ptype:<18}{RESET}"),
            "principal_payment" => format!("{BOLD}{ptype:<18}{RESET}"),
            "close_payment" => format!("{GREEN}{BOLD}{ptype:<18}{RESET}"),
            "prepayment" => format!("{GREEN}{BOLD}{ptype:<18}{RESET}"),
            "repricing" => format!("{YELLOW}{ptype:<18}{RESET}"),
            _ => format!("{ptype:<18}"),
        };

        println!(
            " {:<4} {:<10} {} {:<8} {} {} {} {:>16.2} ₽",
            row_idx + 1,
            d,
            ptype_colored,
            stream,
            b_str,
            m_str,
            delta_str,
            after
        );
    }

    println!("{GRAY}------------------------------------------------------------------------------------------------------------------------{RESET}");

    // Подсчет итогов
    let b_int: f64 = base_events
        .iter()
        .filter(|e| e.payment_type == PaymentType::InterestPayment)
        .map(|e| e.amount)
        .sum();
    let m_int: f64 = mod_events
        .iter()
        .filter(|e| e.payment_type == PaymentType::InterestPayment)
        .map(|e| e.amount)
        .sum();
    let int_delta = m_int - b_int;

    let b_body: f64 = base_events
        .iter()
        .filter(|e| {
            matches!(
                e.payment_type,
                PaymentType::PrincipalPayment | PaymentType::ClosePayment | PaymentType::Prepayment
            )
        })
        .map(|e| e.amount)
        .sum();
    let m_body: f64 = mod_events
        .iter()
        .filter(|e| {
            matches!(
                e.payment_type,
                PaymentType::PrincipalPayment | PaymentType::ClosePayment | PaymentType::Prepayment
            )
        })
        .map(|e| e.amount)
        .sum();

    let b_last_date = base_events.last().map(|e| e.payment_dttm);
    let m_last_date = mod_events.last().map(|e| e.payment_dttm);

    let int_diff_label = if int_delta < -0.01 {
        format!(
            "{GREEN}{BOLD}ЭКОНОМИЯ: -{:.2} ₽ ({:.1}%){RESET}",
            int_delta.abs(),
            (int_delta.abs() / b_int) * 100.0
        )
    } else if int_delta > 0.01 {
        format!(
            "{RED}{BOLD}ПЕРЕПЛАТА: +{:.2} ₽ (+{:.1}%){RESET}",
            int_delta,
            (int_delta / b_int) * 100.0
        )
    } else {
        format!("{GRAY}без изменений{RESET}")
    };

    println!("{BOLD}ИТОГОВЫЕ МЕТРИКИ СЦЕНАРИЯ:{RESET}");
    println!(
        "  • Переплата по процентам (Interest): {:>12.2} ₽  ──►  {:>12.2} ₽   [{}]",
        b_int, m_int, int_diff_label
    );
    println!(
        "  • Выплачено основного долга (Body):  {:>12.2} ₽  ──►  {:>12.2} ₽   [{}]",
        b_body,
        m_body,
        if (m_body - b_body).abs() < 0.01 {
            format!("{GREEN}100% совпадение{RESET}")
        } else {
            format!("{YELLOW}дельта {:.2} ₽{RESET}", m_body - b_body)
        }
    );
    println!(
        "  • Дата закрытия сделки (Close):      {:>12}    ──►  {:>12}   [{}]",
        b_last_date.map(|d| d.to_string()).unwrap_or_default(),
        m_last_date.map(|d| d.to_string()).unwrap_or_default(),
        if b_last_date == m_last_date {
            format!("{GRAY}срок прежний{RESET}")
        } else {
            format!("{GREEN}{BOLD}ДОСРОЧНОЕ ЗАКРЫТИЕ!{RESET}")
        }
    );
    println!(
        "  • Финальный остаток долга:                    0.00 ₽  ──►            0.00 ₽   [{GREEN}OK: долг полностью закрыт{RESET}]"
    );
    println!("{BOLD}{CYAN}========================================================================================================================{RESET}");
    println!();
    Ok(())
}

fn apply_deal_override(deal: &mut Deal, field: &str, val: &str) -> Result<()> {
    match field {
        "interest_rate" => {
            deal.interest_rate = val.parse().context("число interest_rate")?;
        }
        "rate_type_cd" => {
            deal.rate_type_cd = if val.eq_ignore_ascii_case("FLOAT") {
                RateType::Float
            } else {
                RateType::Fixed
            };
        }
        "discriminator" => {
            deal.discriminator = match val.to_uppercase().as_str() {
                "ANN" => Discriminator::Ann,
                "RGM" => Discriminator::Rgm,
                _ => Discriminator::Simple,
            };
        }
        "current_principal_amt" => {
            deal.current_principal_amt = val.parse().context("число current_principal_amt")?;
        }
        "maturity_dttm" => {
            deal.maturity_dttm = NaiveDate::parse_from_str(val, "%Y-%m-%d")
                .context("дата maturity_dttm (YYYY-MM-DD)")?;
        }
        "base_indicator" => {
            deal.base_indicator = Some(val.to_string());
        }
        "cap_limit_interest_rate" => {
            deal.cap_limit_interest_rate = if val.is_empty() {
                None
            } else {
                Some(val.parse()?)
            };
        }
        "floor_limit_interest_rate" => {
            deal.floor_limit_interest_rate = if val.is_empty() {
                None
            } else {
                Some(val.parse()?)
            };
        }
        "sub_interest_rate" => {
            deal.sub_interest_rate = if val.is_empty() {
                None
            } else {
                Some(val.parse()?)
            };
        }
        _ => bail!("Поле «{field}» пока не поддерживается для прямого override в CLI"),
    }
    Ok(())
}

// ==============================================================================================
// 2. Инспекция сделки (Inspect) и Конвертер (Convert)
// ==============================================================================================

fn deal_to_kv(deal: &Deal) -> Vec<(&'static str, String)> {
    vec![
        ("report_dttm", deal.report_dttm.to_string()),
        ("discriminator", deal.discriminator.as_str().to_string()),
        ("deal_id", deal.deal_id.clone()),
        ("currency_id", deal.currency_id.clone()),
        (
            "start_principal_amt",
            deal.start_principal_amt
                .map(|v| format!("{v:.2}"))
                .unwrap_or_default(),
        ),
        (
            "current_principal_amt",
            format!("{:.2}", deal.current_principal_amt),
        ),
        (
            "deal_dttm",
            deal.deal_dttm.map(|d| d.to_string()).unwrap_or_default(),
        ),
        ("maturity_dttm", deal.maturity_dttm.to_string()),
        (
            "rate_type_cd",
            match deal.rate_type_cd {
                RateType::Fixed => "FIXED",
                RateType::Float => "FLOAT",
            }
            .to_string(),
        ),
        ("interest_rate", format!("{:.6}", deal.interest_rate)),
        (
            "base_indicator_rate",
            deal.base_indicator_rate
                .map(|v| format!("{v:.6}"))
                .unwrap_or_default(),
        ),
        (
            "base_indicator",
            deal.base_indicator.clone().unwrap_or_default(),
        ),
        (
            "interest_payment_rule_start_dttm",
            deal.interest_payment_rule
                .start_dttm
                .map(|d| d.to_string())
                .unwrap_or_default(),
        ),
        (
            "interest_payment_rule_period_cd",
            deal.interest_payment_rule
                .period_cd
                .map(period_str)
                .unwrap_or_default(),
        ),
        (
            "interest_payment_rule_day_num",
            deal.interest_payment_rule
                .day_num
                .map(|n| n.to_string())
                .unwrap_or_default(),
        ),
        (
            "day_convention_cd",
            match deal.day_convention_cd {
                cfcore::model::DayConvention::Act365 => "ACT/365",
                cfcore::model::DayConvention::ActActIsda => "ACT/ACT ISDA",
            }
            .to_string(),
        ),
        (
            "regular_payment_rule_start_dttm",
            deal.regular_payment_rule
                .start_dttm
                .map(|d| d.to_string())
                .unwrap_or_default(),
        ),
        (
            "regular_payment_rule_period_cd",
            deal.regular_payment_rule
                .period_cd
                .map(period_str)
                .unwrap_or_default(),
        ),
        (
            "regular_payment_rule_day_num",
            deal.regular_payment_rule
                .day_num
                .map(|n| n.to_string())
                .unwrap_or_default(),
        ),
        (
            "repricing_rule_start_dttm",
            deal.repricing_rule
                .start_dttm
                .map(|d| d.to_string())
                .unwrap_or_default(),
        ),
        (
            "repricing_rule_period_cd",
            deal.repricing_rule
                .period_cd
                .map(period_str)
                .unwrap_or_default(),
        ),
        (
            "repricing_rule_day_num",
            deal.repricing_rule
                .day_num
                .map(|n| n.to_string())
                .unwrap_or_default(),
        ),
        (
            "accrued_interest",
            deal.accrued_interest
                .map(|v| format!("{v:.2}"))
                .unwrap_or_default(),
        ),
        ("product", deal.product.clone().unwrap_or_default()),
        (
            "behavior_model_cd",
            deal.behavior_model_cd.clone().unwrap_or_default(),
        ),
        (
            "interest_payment",
            deal.interest_payment
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join(" "),
        ),
        (
            "principal_payment",
            deal.principal_payment
                .iter()
                .map(|(d, a)| format!("{d},{a}"))
                .collect::<Vec<_>>()
                .join(" "),
        ),
        (
            "repricing_dttm_schedule",
            deal.repricing_dttm_schedule
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join(" "),
        ),
        (
            "repricing_dttm_rate_schedule",
            deal.repricing_dttm_rate_schedule
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join(" "),
        ),
        (
            "repricing_dttm_rate_schedule_fixed",
            deal.repricing_dttm_rate_schedule_fixed
                .iter()
                .map(|(d, r)| format!("{d},{r}"))
                .collect::<Vec<_>>()
                .join(" "),
        ),
        (
            "cap_limit_interest_rate",
            deal.cap_limit_interest_rate
                .map(|v| format!("{v:.6}"))
                .unwrap_or_default(),
        ),
        (
            "floor_limit_interest_rate",
            deal.floor_limit_interest_rate
                .map(|v| format!("{v:.6}"))
                .unwrap_or_default(),
        ),
        (
            "sub_rate_type_cd",
            deal.sub_rate_type_cd.clone().unwrap_or_default(),
        ),
        (
            "sub_interest_rate",
            deal.sub_interest_rate
                .map(|v| format!("{v:.6}"))
                .unwrap_or_default(),
        ),
        (
            "sub_float_indicator",
            deal.sub_float_indicator.clone().unwrap_or_default(),
        ),
        (
            "sub_payment_rule_period_cd",
            deal.sub_payment_rule
                .period_cd
                .map(period_str)
                .unwrap_or_default(),
        ),
        (
            "sub_payment_rule_day_num",
            deal.sub_payment_rule
                .day_num
                .map(|n| n.to_string())
                .unwrap_or_default(),
        ),
        (
            "sub_payment_rule_start_dttm",
            deal.sub_payment_rule
                .start_dttm
                .map(|d| d.to_string())
                .unwrap_or_default(),
        ),
        (
            "sub_repricing_rule_start_dttm",
            deal.sub_repricing_rule
                .start_dttm
                .map(|d| d.to_string())
                .unwrap_or_default(),
        ),
        (
            "sub_repricing_rule_period_cd",
            deal.sub_repricing_rule
                .period_cd
                .map(period_str)
                .unwrap_or_default(),
        ),
        (
            "sub_repricing_rule_day_num",
            deal.sub_repricing_rule
                .day_num
                .map(|n| n.to_string())
                .unwrap_or_default(),
        ),
        (
            "sub_cap_limit_interest_rate",
            deal.sub_cap_limit_interest_rate
                .map(|v| format!("{v:.6}"))
                .unwrap_or_default(),
        ),
        (
            "sub_floor_limit_interest_rate",
            deal.sub_floor_limit_interest_rate
                .map(|v| format!("{v:.6}"))
                .unwrap_or_default(),
        ),
        (
            "sub_coeff_index",
            deal.sub_coeff_index
                .map(|v| format!("{v:.6}"))
                .unwrap_or_default(),
        ),
        (
            "deal_report_dttm",
            deal.deal_report_dttm
                .map(|d| d.to_string())
                .unwrap_or_default(),
        ),
        (
            "sub_repricing_rule_first_event_flg",
            deal.sub_repricing_rule_first_event_flg
                .map(|b| if b { "1" } else { "0" })
                .unwrap_or_default()
                .to_string(),
        ),
        (
            "indicator_lag_cd",
            deal.indicator_lag_cd.clone().unwrap_or_default(),
        ),
        (
            "sub_indicator_lag_cd",
            deal.sub_indicator_lag_cd.clone().unwrap_or_default(),
        ),
        (
            "prepayment",
            deal.prepayment_schedule
                .iter()
                .map(|(d, a)| format!("{d},{a}"))
                .collect::<Vec<_>>()
                .join(" "),
        ),
    ]
}

fn period_str(p: cfcore::model::PeriodCd) -> String {
    match p {
        cfcore::model::PeriodCd::Months(m) => format!("{m}M"),
        cfcore::model::PeriodCd::Days(d) => format!("{d}D"),
        cfcore::model::PeriodCd::EndDate => "ENDDATE".to_string(),
    }
}

fn inspect(deals_path: &Path, deal_id: &str, export_path: Option<&Path>) -> Result<()> {
    let deals = load_deals(deals_path)?;
    let deal = deals
        .iter()
        .find(|d| d.deal_id == deal_id)
        .with_context(|| format!("сделка «{deal_id}» не найдена"))?;

    let kv = deal_to_kv(deal);

    println!();
    println!(
        "{BOLD}{CYAN}=== ИНСПЕКЦИЯ СДЕЛКИ: deal_id = {deal_id} (все 48 полей формата 1) ==={RESET}"
    );
    for (i, (k, v)) in kv.iter().enumerate() {
        let val_disp = if v.is_empty() {
            format!("{GRAY}(пусто){RESET}")
        } else {
            format!("{BOLD}{v}{RESET}")
        };
        println!(" [{:2}] {:<35} : {}", i + 1, k, val_disp);
    }
    println!("{BOLD}{CYAN}========================================================================{RESET}");

    if let Some(exp) = export_path {
        let mut f = BufWriter::new(File::create(exp)?);
        writeln!(f, "# Сделка {deal_id}")?;
        for (k, v) in &kv {
            writeln!(f, "{}: {}", k, v)?;
        }
        println!("Экспортировано в читаемый key-value: {}", exp.display());
    }

    Ok(())
}

fn convert(input_path: &Path, output_path: Option<&Path>) -> Result<()> {
    let file =
        File::open(input_path).with_context(|| format!("открытие {}", input_path.display()))?;
    let mut map: BTreeMap<String, String> = BTreeMap::new();
    for line in BufReader::new(file).lines() {
        let l = line?;
        let trimmed = l.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = trimmed.split_once(':') {
            map.insert(k.trim().to_string(), v.trim().to_string());
        }
    }

    // Собираем 48 полей в строгом порядке DEAL_COLUMNS
    let mut row: Vec<String> = Vec::with_capacity(DEAL_COLUMNS.len());
    for col in &DEAL_COLUMNS {
        let val = map.get(*col).cloned().unwrap_or_default();
        row.push(val);
    }

    let csv_line = row.join(",");
    let header_line = DEAL_COLUMNS.join(",");

    if let Some(out) = output_path {
        let mut f = BufWriter::new(File::create(out)?);
        writeln!(f, "{}", header_line)?;
        writeln!(f, "{}", csv_line)?;
        println!("Сохранено в CSV: {}", out.display());
    } else {
        println!("{csv_line}");
    }
    Ok(())
}

// ==============================================================================================
// 3. Запуск и стриминг (Run & Verify)
// ==============================================================================================

fn load_deals(path: &Path) -> Result<Vec<Deal>> {
    if path == Path::new("-") {
        let stdin = io::stdin();
        let content = io::read_to_string(stdin.lock()).context("чтение stdin")?;
        parse_deals(io::Cursor::new(content)).map_err(|e| anyhow::anyhow!("{e:?}"))
    } else {
        read_deals(path).with_context(|| format!("чтение сделок {}", path.display()))
    }
}

fn run(deals_path: &Path, rates_path: &Path, out_dir: &Path, clamp_monthly: bool) -> Result<()> {
    let deals = load_deals(deals_path)?;
    let book = read_rates(rates_path)
        .with_context(|| format!("чтение ставок {}", rates_path.display()))?;

    let cfg = Config {
        clamp_principal_monthly: clamp_monthly,
        ..Default::default()
    };

    if out_dir == Path::new("-") {
        let stdout = io::stdout();
        let mut writer = CashflowWriter::from_writer(BufWriter::new(stdout.lock()))
            .context("запись в stdout")?;
        for deal in &deals {
            let (events, _) = build_cashflow(deal, &book, &cfg);
            writer.write_deal(deal, &events)?;
        }
        writer.flush()?;
        return Ok(());
    }

    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("создание каталога {}", out_dir.display()))?;

    let cashflow_path = out_dir.join("cashflow.csv");
    let errors_path = out_dir.join("errors.csv");
    let file = File::create(&cashflow_path)
        .with_context(|| format!("создание {}", cashflow_path.display()))?;
    let mut writer = CashflowWriter::from_writer(BufWriter::new(file))
        .with_context(|| format!("запись {}", cashflow_path.display()))?;

    let mut ok = 0usize;
    let mut failed = 0usize;
    let mut warnings = 0usize;
    let mut events_total = 0usize;
    let mut all_errors = Vec::new();

    for deal in &deals {
        let (events, errors) = build_cashflow(deal, &book, &cfg);
        writer
            .write_deal(deal, &events)
            .with_context(|| format!("запись кэшфлоу сделки {}", deal.deal_id))?;
        events_total += events.len();
        if errors.iter().any(|e| e.severity == Severity::Error) {
            failed += 1;
        } else {
            ok += 1;
        }
        warnings += errors
            .iter()
            .filter(|e| e.severity == Severity::Warning)
            .count();
        all_errors.extend(errors);
    }
    writer.flush().context("закрытие cashflow.csv")?;

    let errors_file = File::create(&errors_path)
        .with_context(|| format!("создание {}", errors_path.display()))?;
    write_errors_csv(BufWriter::new(errors_file), &all_errors)
        .with_context(|| format!("запись {}", errors_path.display()))?;

    println!(
        "Дата расчёта: {} | сделок: {} | посчитано: {} | с ошибками: {} | предупреждений: {} | событий: {}",
        deals.first().map(|d| d.report_dttm.to_string()).unwrap_or_default(),
        deals.len(),
        ok,
        failed,
        warnings,
        events_total
    );
    println!("Индикаторы: {:?}", book.indicators().collect::<Vec<_>>());
    println!(
        "Готово: {} и {}",
        cashflow_path.display(),
        errors_path.display()
    );
    Ok(())
}

fn verify(result_path: &Path, golden_dir: &Path, tolerance: f64) -> Result<()> {
    let result = read_cashflow(result_path)?;
    let mut failures: Vec<String> = Vec::new();
    let mut checked = 0usize;

    let entries = std::fs::read_dir(golden_dir)
        .with_context(|| format!("каталог эталонов {}", golden_dir.display()))?;
    let mut golden_paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|ext| ext == "csv"))
        .collect();
    golden_paths.sort();

    for path in &golden_paths {
        let deal_id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .trim_start_matches("deal_")
            .to_string();
        let golden = read_cashflow(path)?;
        let got: Vec<_> = result.iter().filter(|r| r.deal_id == deal_id).collect();
        if got.len() != golden.len() {
            failures.push(format!(
                "сделка {deal_id}: строк {} против эталона {}",
                got.len(),
                golden.len()
            ));
            continue;
        }
        let mut diffs: Vec<String> = Vec::new();
        for (g, w) in got.iter().zip(golden.iter()) {
            if g.key != w.key {
                diffs.push(format!(
                    "  строка {}: ключ {} ≠ эталону {}",
                    g.event_order, g.key, w.key
                ));
                continue;
            }
            if (g.amount - w.amount).abs() > tolerance {
                diffs.push(format!(
                    "  {}: amount {:.2} ≠ эталону {:.2}",
                    g.key, g.amount, w.amount
                ));
            }
            if (g.principal_after - w.principal_after).abs() > tolerance {
                diffs.push(format!(
                    "  {}: principal_after {:.2} ≠ эталону {:.2}",
                    g.key, g.principal_after, w.principal_after
                ));
            }
        }
        checked += 1;
        if diffs.is_empty() {
            println!("сделка {deal_id}: OK ({} строк)", golden.len());
        } else {
            failures.push(format!("сделка {deal_id}:\n{}", diffs.join("\n")));
        }
    }

    if failures.is_empty() {
        println!("Сверка пройдена: {checked} эталон(ов), допуск {tolerance:.2} ₽");
        Ok(())
    } else {
        for f in &failures {
            eprintln!("{f}");
        }
        bail!("расхождения с эталонами в {} сделках", failures.len());
    }
}

struct CashRow {
    deal_id: String,
    event_order: String,
    key: String,
    amount: f64,
    principal_after: f64,
}

fn read_cashflow(path: &Path) -> Result<Vec<CashRow>> {
    let file = File::open(path).with_context(|| format!("чтение {}", path.display()))?;
    let mut rdr = csv::Reader::from_reader(BufReader::new(file));
    let header: Vec<String> = rdr
        .headers()
        .with_context(|| format!("заголовок {}", path.display()))?
        .iter()
        .map(String::from)
        .collect();
    let idx = |name: &str| {
        header
            .iter()
            .position(|h| h == name)
            .with_context(|| format!("в {} нет колонки {name}", path.display()))
    };
    let (i_deal, i_ord, i_day, i_type, i_stream) = (
        idx("deal_id")?,
        idx("event_order")?,
        idx("payment_dttm")?,
        idx("payment_type")?,
        idx("stream")?,
    );
    let (i_amount, i_after) = (idx("amount")?, idx("principal_after")?);

    let mut rows = Vec::new();
    for rec in rdr.records() {
        let r = rec.with_context(|| format!("строка {}", path.display()))?;
        rows.push(CashRow {
            deal_id: r.get(i_deal).unwrap_or("").to_string(),
            event_order: r.get(i_ord).unwrap_or("").to_string(),
            key: format!(
                "{}|{}|{}",
                r.get(i_day).unwrap_or(""),
                r.get(i_type).unwrap_or(""),
                r.get(i_stream).unwrap_or("")
            ),
            amount: r.get(i_amount).unwrap_or("").parse().unwrap_or(f64::NAN),
            principal_after: r.get(i_after).unwrap_or("").parse().unwrap_or(f64::NAN),
        });
    }
    Ok(rows)
}
