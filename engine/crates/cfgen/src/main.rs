//! cfgen — CLI калькулятора денежных потоков поверх библиотеки cfcore.

use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use cfcore::engine::build_cashflow;
use cfcore::io::{read_deals, read_rates, write_errors_csv, CashflowWriter};
use cfcore::{Config, Severity};
use clap::{Parser, Subcommand};

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
        /// Файл сделок, формат 1 контракта (testdata/deals/*.csv)
        #[arg(long, value_name = "FILE")]
        deals: PathBuf,
        /// История индикаторов, формат 2 (testdata/rates/key_rate.csv)
        #[arg(long, value_name = "FILE")]
        rates: PathBuf,
        /// Куда положить cashflow.csv и errors.csv
        #[arg(long, value_name = "DIR")]
        out_dir: PathBuf,
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
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Run {
            deals,
            rates,
            out_dir,
        } => run(&deals, &rates, &out_dir),
        Command::Verify {
            result,
            golden_dir,
            tolerance,
        } => {
            bail!(
                "сверка будет реализована в ветке engine/verify \
                 (result={}, golden_dir={}, допуск {:.2} ₽)",
                result.display(),
                golden_dir.display(),
                tolerance
            )
        }
    }
}

fn run(deals_path: &Path, rates_path: &Path, out_dir: &Path) -> Result<()> {
    let deals = read_deals(deals_path)
        .with_context(|| format!("чтение сделок {}", deals_path.display()))?;
    let book = read_rates(rates_path)
        .with_context(|| format!("чтение ставок {}", rates_path.display()))?;
    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("создание каталога {}", out_dir.display()))?;

    let cashflow_path = out_dir.join("cashflow.csv");
    let errors_path = out_dir.join("errors.csv");
    let file = File::create(&cashflow_path)
        .with_context(|| format!("создание {}", cashflow_path.display()))?;
    let mut writer = CashflowWriter::from_writer(BufWriter::new(file))
        .with_context(|| format!("запись {}", cashflow_path.display()))?;

    let cfg = Config::default();
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
