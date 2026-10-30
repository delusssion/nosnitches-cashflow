# 00 — Общий контракт проекта `nosnitches-cashflow`

> Этот файл кладётся в репозиторий как `docs/CONTRACT.md` **до** того, как кто-то из команды запустит свои задачи в Claude Code. Все три файла задач ссылаются на него. Менять контракт можно только по договорённости всей команды, изменения — отдельным PR с пометкой `contract:`.

## Контекст (для Claude Code)

Университетская практика МАИ × Т1, кейс 01 «Econom Capital». Команда **nosnitches** делает модуль **калькулятора денежных потоков**:

- на вход — портфель кредитных и депозитных сделок банка (рубли), история ключевой ставки ЦБ и дата расчёта;
- на выход — **расписание всех будущих платежей** по каждой сделке: проценты, тело, репрайсинг ставки, субсидия.

Дисконтирование и NPV **не** входят в задачу калькулятора — это делает команда 3, используя наш код. Кривые ставок (базовые и стрессовые) готовит команда 2, подключим их в спринте 3.

Требования эксперта: калькулятор должен работать на **огромном** числе сделок и экономно расходовать CPU, поэтому ядро расчёта пишется на **Rust** (cargo workspace в `engine/`, CLI `cfgen`).

Участники:
- Соколов Александр — аналитик (методика, эталоны, `analytics/`, `docs/`)
- Уханов Фёдор — бэкенд (движок на Rust, `engine/`)
- Шалыгин Ярослав — фронтенд (просмотрщик и сверка, `web/`, генератор портфеля `tools/`)

## Подготовка окружения (каждый участник, один раз)

1. Установить `git` и GitHub CLI `gh`, войти: `gh auth login`.
2. Указать СВОЁ имя и почту для коммитов (по ним GitHub считает контрибьюторов):
   `git config --global user.name "Имя Фамилия"` и `git config --global user.email "<почта аккаунта GitHub>"`.
3. Для бэкенда: установить Rust через rustup (stable).
4. Отключить подписи Claude в коммитах и PR — в `~/.claude/settings.json` (глобально для твоего Claude Code):
   ```json
   {
     "attribution": { "commit": "", "pr": "" },
     "includeCoAuthoredBy": false
   }
   ```
   `attribution` — актуальная настройка, `includeCoAuthoredBy` — старая, оставлена на случай старой версии Claude Code.
   Тот же блок кладётся в репозиторий в `.claude/settings.json` (задача F0), чтобы работало у всех.
5. Репозиторий на GitHub: `nosnitches-cashflow`. В настройках репозитория:
   Settings → General → Pull Requests: включить «Allow squash merging», «Automatically delete head branches»;
   Settings → Branches → правило для `main`: «Require a pull request before merging» (0 обязательных ревью,
   иначе Claude Code не сможет смержить сам) и «Require status checks to pass».
6. Клонировать: `gh repo clone <org>/nosnitches-cashflow`.

## Git-процесс команды

- Ветка на каждую задачу: `web/…`, `engine/…`, `analytics/…`. Напрямую в `main` не пушим.
- Коммиты короткие, одна строка: `feat(engine): генератор дат по правилу периода`.
- В коммитах и PR нет упоминаний Claude/ИИ и строк `Co-Authored-By`.
- PR → зелёный CI → squash-merge → ветка удаляется.
- Исключение одно: самый первый коммит в пустой репозиторий (задача F0) идёт прямо в `main`, потому что PR нужна база.

## Структура репозитория

```
nosnitches-cashflow/
├── README.md
├── docs/
│   ├── CONTRACT.md            ← этот файл
│   ├── METHODOLOGY.md         ← алгоритмы и формулы (аналитик)
│   ├── EXCEL_ERRORS.md        ← разбор ошибок расчётного файла (аналитик)
│   ├── QUESTIONS.md           ← вопросы к экспертам и ответы (аналитик)
│   ├── DECISIONS.md           ← архитектурные решения, кратко (все)
│   └── DEMO.md                ← сценарий демо (фронтенд)
├── engine/                    ← Rust workspace (бэкенд)
├── web/                       ← React-приложение (фронтенд)
├── tools/gen-portfolio/       ← генератор синтетического портфеля (фронтенд)
├── analytics/                 ← Python-эталон, скрипты, Excel (аналитик)
└── testdata/
    ├── source/                ← исходный xlsx от экспертов, как есть
    ├── deals/test_deals.csv   ← 55 тестовых сделок (аналитик)
    ├── rates/key_rate.csv     ← история ключевой ставки (аналитик)
    ├── golden/deal_<id>.csv   ← эталонные кэшфлоу (аналитик)
    └── samples/               ← демонстрационные файлы, НЕ эталоны (фронтенд)
```

Правило: каждый участник пишет только в свои папки. `testdata/` — общая, но у каждой подпапки один владелец (указан выше).

## Общие соглашения по CSV

- Кодировка UTF-8 без BOM, разделитель `,`, первая строка — заголовок.
- Даты: `YYYY-MM-DD`. Никаких `00:00:00`.
- Числа: точка как десятичный разделитель, без разделителей тысяч.
- Пустое значение = поле не задано (null).
- Ставки — в долях: `0.075` = 7,5 %.

## Формат 1. Сделки — `testdata/deals/*.csv`

Колонки — ровно как в листе «Тестовые сделки» расчётного файла экспертов, в том же порядке (48 полей, колонка `Столбец1` выбрасывается):

```
report_dttm,discriminator,deal_id,currency_id,start_principal_amt,current_principal_amt,deal_dttm,maturity_dttm,rate_type_cd,interest_rate,base_indicator_rate,base_indicator,interest_payment_rule_start_dttm,interest_payment_rule_period_cd,interest_payment_rule_day_num,day_convention_cd,regular_payment_rule_start_dttm,regular_payment_rule_period_cd,regular_payment_rule_day_num,repricing_rule_start_dttm,repricing_rule_period_cd,repricing_rule_day_num,accrued_interest,product,behavior_model_cd,interest_payment,principal_payment,repricing_dttm_schedule,repricing_dttm_rate_schedule,repricing_dttm_rate_schedule_fixed,cap_limit_interest_rate,floor_limit_interest_rate,sub_rate_type_cd,sub_interest_rate,sub_float_indicator,sub_payment_rule_period_cd,sub_payment_rule_day_num,sub_payment_rule_start_dttm,sub_repricing_rule_start_dttm,sub_repricing_rule_period_cd,sub_repricing_rule_day_num,sub_cap_limit_interest_rate,sub_floor_limit_interest_rate,sub_coeff_index,deal_report_dttm,sub_repricing_rule_first_event_flg,indicator_lag_cd,sub_indicator_lag_cd
```

Ключевые значения:

| Поле | Значения |
|---|---|
| `discriminator` | `ANN` аннуитет · `RGM` регулярное погашение тела равными долями (гипотеза H1) · `SIMPLE` явный график тела или погашение в конце |
| `rate_type_cd` | `FIXED`, `FLOAT` |
| `day_convention_cd` | `ACT/365`, `ACT/ACT ISDA`, `ACT/ACT` (считаем как ISDA, гипотеза H5) |
| `*_period_cd` | `1M`, `2M`, `3M`, `5M`, `<N>D` (напр. `45D`, `153D`), `ENDDATE` |
| `*_day_num` | `1..31`; `-1` = последний день месяца; если в месяце нет такого дня — последний день месяца |
| `current_principal_amt` | `> 0` актив (кредит), `< 0` пассив (депозит) |
| `interest_payment` | даты через пробел: `2022-09-30 2022-10-31` (порядок и дубли не гарантированы — сортировать и убирать дубли) |
| `principal_payment` | пары `дата,сумма` через пробел: `2022-12-31,250000.0 2023-01-31,250000.0` |
| `repricing_dttm_schedule` | даты пересмотра ставки через пробел |
| `repricing_dttm_rate_schedule_fixed` | пары `дата,ставка` фиксированной части |
| `base_indicator`, `sub_float_indicator` | код индикатора, сейчас только `CBKEY1M` (ключевая ставка ЦБ) |
| `indicator_lag_cd` | лаг индикатора, напр. `-1M` |

## Формат 2. Ставки — `testdata/rates/key_rate.csv`

```
indicator,date,rate
CBKEY1M,2020-01-03,0.0625
```
Ступенчатая функция: значение действует с `date` до следующей даты.

## Формат 3. Результат калькулятора — кэшфлоу

Один файл на прогон, строки одной сделки идут подряд, внутри сделки — по `payment_dttm`, затем по `event_order`.

```
report_dttm,deal_id,event_order,payment_dttm,payment_type,stream,amount,principal_before,principal_after,rate,base_rate,accrual_start_dttm,accrual_end_dttm,accrual_days,year_frac,discriminator
```

| Колонка | Смысл |
|---|---|
| `event_order` | порядок события внутри сделки, с 1 |
| `payment_type` | `interest_payment`, `principal_payment`, `close_payment`, `repricing`, `sub_payment`, `sub_repricing`, `prepayment` (зарезервировано, пока не генерируется) |
| `stream` | `client` — платежи клиента; `sub` — субсидия |
| `amount` | сумма платежа со **знаком сделки** (депозит → отрицательные суммы); для `repricing`/`sub_repricing` = `0` |
| `principal_before/after` | остаток тела до и после события (со знаком сделки) |
| `rate` | ставка, действующая на период начисления (для repricing — новая ставка) |
| `base_rate` | значение индикатора, если ставка плавающая, иначе пусто |
| `accrual_start_dttm/end_dttm` | период начисления процентов для `interest_payment`/`sub_payment`, иначе пусто |
| `accrual_days`, `year_frac` | дни и доля года периода начисления |

Порядок событий в один день: `repricing`/`sub_repricing` → `interest_payment` → `sub_payment` → `principal_payment` → `close_payment`.

Правила:
- В выход попадают только события с `payment_dttm > report_dttm` (гипотеза H6: событие ровно в дату расчёта считается прошлым).
- Проценты, накопленные до даты расчёта, входят в первый будущий `interest_payment`.
- Последнее событие по телу — `close_payment` на весь остаток: после него `principal_after = 0` строго.
- Суммы в файле округлены до 2 знаков; внутри расчёта округления нет.

Ошибки сделок — отдельный файл `errors.csv`: `deal_id,severity,code,field,message`. `severity`: `error` (сделка не посчитана) или `warning` (посчитана, но есть замечание).

## Формат 4. Эталон — `testdata/golden/deal_<id>.csv`

Ровно формат 3, одна сделка на файл. Сверка: по ключу `deal_id + payment_dttm + payment_type + stream`, допуск `|Δamount| ≤ 0.01`, `|Δprincipal_after| ≤ 0.01`.

## Гипотезы, пока эксперты не ответили

Каждая гипотеза в коде включается флагом/константой с её номером, чтобы её можно было поменять одной правкой.

| № | Гипотеза |
|---|---|
| H1 | `RGM` — тело гасится равными долями: остаток на дату расчёта / число оставшихся дат погашения тела |
| H2 | `FLOAT`: ставка = значение индикатора + `interest_rate` (маржа), затем ограничение `floor`/`cap` |
| H3 | Будущее значение индикатора = последнее известное на `report_dttm` + сдвиг сценария (в спринте 2 сдвиг 0) |
| H4 | Ставка субсидии = `sub_coeff_index` (если пусто — 1) × индикатор + `sub_interest_rate`; база начисления — остаток тела |
| H5 | `ACT/ACT` = `ACT/ACT ISDA` |
| H6 | Событие в дату расчёта — прошлое |
| H7 | `ANN`: аннуитет пересчитывается от остатка на дату расчёта и при каждом репрайсинге; ставка периода = годовая × месяцы периода / 12 |
| H8 | Выходные/праздники не переносим |
| H9 | Если сумма явного графика тела ≠ остатку, разница уходит в `close_payment` в дату погашения + warning |
