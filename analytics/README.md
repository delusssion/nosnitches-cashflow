# analytics

Методика, эталонные расчёты и разбор данных экспертов. Код эталона намеренно простой: он проверяется глазами и не зависит от движка на Rust.

```bash
cd analytics
python3 -m venv .venv && .venv/bin/pip install -r requirements.txt
.venv/bin/python scripts/xlsx_to_csv.py   # testdata/source/experts_calc.xlsx → testdata/deals, testdata/rates
.venv/bin/pytest -q
```
