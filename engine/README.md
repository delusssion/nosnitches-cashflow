# engine — движок калькулятора денежных потоков (Rust)

По портфелю сделок (формат 1 [`docs/CONTRACT.md`](../docs/CONTRACT.md)) и истории ключевой ставки (формат 2) строит расписание всех будущих платежей (формат 3): проценты, тело, досрочные погашения, пересмотры ставки, субсидия. Алгоритмы — [`docs/METHODOLOGY.md`](../docs/METHODOLOGY.md), гипотезы H1–H9 — поля `cfcore::Config`.

```
crates/cfcore   библиотека: даты, конвенции, ставки, движок, чтение и запись CSV
crates/cfgen    CLI поверх cfcore
```

## Сборка и проверки

```bash
cd engine
cargo build --release
cargo fmt --all --check && cargo clippy --all-targets -- -D warnings && cargo test --all
cargo bench -p cfcore          # criterion: одна сделка каждого типа
```

## CLI `cfgen`

```bash
# расчёт: out/cashflow.csv и out/errors.csv
cfgen run --deals ../testdata/deals/test_deals.csv --rates ../testdata/rates/key_rate.csv --out-dir out

# сверка с эталонами аналитика (допуск 0,01 ₽)
cfgen verify --result out/cashflow.csv --golden-dir ../testdata/golden

# сделка вертикально, сценарий «что если» с цветным diff
cfgen inspect  --deals ../testdata/deals/test_deals.csv --deal-id 22
cfgen scenario --deals ../testdata/deals/test_deals.csv --rates ../testdata/rates/key_rate.csv --deal-id 22 --prepay 2022-11-20:250000
```

Флаги `run`:

| Флаг | По умолчанию | |
|---|---|---|
| `--threads` | все ядра | потоков расчёта |
| `--chunk` | 4096 | сделок в пачке: столько читается, считается и пишется за раз — память ограничена пачкой |
| `--shift-bp` | 0 | сдвиг будущих значений индикатора, б.п. (H3, стресс-сценарий) |
| `--report-date` | из сделки | дата расчёта для всех сделок |
| `--clamp-monthly` | выкл. | не больше одного платежа тела в месяц |
| `--summary FILE` | — | сводка прогона в JSON: сделки, события, время, сделок/с |
| `--curves FILE` | — | кривые прогноза индикатора от команды 2: после даты расчёта вместо H3 |
| `--scenario NAME` | — | сценарий из файла кривых (колонка `scenario`): `base`, `stress`, … |

Кривые команды 2: колонки `indicator,date,rate` (формат 2 контракта) или `indicator_code,rate_date,rate_value`, необязательная `scenario`. Значение кривой действует с даты до следующей точки; до первой точки кривой — H3. Сдвиг `--shift-bp` прибавляется и к кривой. Если в файле несколько сценариев, `--scenario` обязателен.

```bash
cfgen run --deals deals.csv --rates key_rate.csv --curves curves.csv --scenario stress --out-dir out_stress
```

Досрочные погашения — необязательная колонка `prepayment_schedule` в файле сделок (`дата,сумма` через пробел).

## Как подключить (команда 3)

**Потоком, без промежуточных файлов.** `-` вместо пути: сделки из stdin, кэшфлоу в stdout, `errors.csv` и сводка — в stderr.

```bash
cat deals.csv | cfgen run --deals - --rates key_rate.csv --out-dir - > cashflow.csv 2> errors.csv
```

**Как библиотека Rust.**

```toml
[dependencies]
cfcore = { path = "../nosnitches-cashflow/engine/crates/cfcore" }
```

```rust
use cfcore::{generate, generate_portfolio, io::{read_deals, read_rates}, Config};

let deals = read_deals("deals.csv".as_ref())?;
let rates = read_rates("key_rate.csv".as_ref())?;
let (events, issues) = generate(&deals[0], &rates, &Config::default());   // одна сделка
let all = generate_portfolio(&deals, &rates, &Config::default());          // параллельно, порядок как во входе
```

`generate` — чистая функция без состояния, её можно вызывать из любого числа потоков. Для файлов, которые не помещаются в память, — `cfcore::io::DealReader` (чтение по одной сделке) и `cfcore::io::format_deal` (строки формата 3 в буфер).

## Производительность

1 000 000 синтетических сделок (187 млн событий, 22,7 ГБ CSV) на Apple M4, 10 потоков: ~30 с с записью на диск, 330–414 МБ памяти; без записи — 11–25 с. Подробности и замеры до/после — [`docs/DECISIONS.md`](../docs/DECISIONS.md).
