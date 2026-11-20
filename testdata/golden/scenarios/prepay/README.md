# Сценарий prepay

досрочные погашения (H10): колонка prepayment_schedule.

Сделки: 1, 2, 15, 16, 22, 47. Входные сделки — `input/deals.csv`, эталоны — `deal_<id>.csv` (формат 4 контракта), посчитаны `analytics/scripts/make_golden.py`.

Сверка движка (из этого каталога):

```bash
cfgen run --deals input/deals.csv --rates ../../../rates/key_rate.csv --out-dir out
cfgen verify --result out/cashflow.csv --golden-dir .
```
