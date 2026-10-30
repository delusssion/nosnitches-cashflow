# analytics

Методика, эталонные расчёты и разбор данных экспертов. Код эталона намеренно простой: он проверяется глазами и не зависит от движка на Rust.

```bash
cd analytics
python3 -m venv .venv && .venv/bin/pip install -r requirements.txt
.venv/bin/python scripts/xlsx_to_csv.py   # testdata/source/experts_calc.xlsx → testdata/deals, testdata/rates
.venv/bin/pytest -q
```

## Эталон

`reference/cf_reference.py` — независимая реализация [методики](../docs/METHODOLOGY.md): `generate(deal, key_rate, report_date)` возвращает события в формате 3 контракта. Гипотезы H1–H9 — флаги в начале файла.

```bash
.venv/bin/python scripts/make_golden.py        # testdata/golden/deal_<id>.csv — 13 эталонных сделок
.venv/bin/python scripts/make_golden.py --all  # все 55 сделок в testdata/golden/all/ (не коммитится, для разбора)
```

Эталонные сделки: 1 (ANN, ACT/ACT), 2 (RGM), 15 (депозит), 16 (SIMPLE, график), 21 (ENDDATE), 22 и 25 (FLOAT, cap/floor), 30 (45D), 35 (day_num −1), 42 (явный репрайсинг), 44 (явные даты, проценты в дату расчёта), 47 (субсидия FLOAT), 53 (субсидия с коэффициентом 0,9).
Тесты `tests/test_reference.py`: инварианты на всех 54 рублёвых сделках (остаток после последнего события 0, сумма тела = остатку, нет событий ≤ даты расчёта, порядок событий), контрольное число сделки 1 (проценты по ISDA 5 051 623,47 ₽), актуальность golden-файлов.
