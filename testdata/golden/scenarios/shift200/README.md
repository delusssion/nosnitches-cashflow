# Сценарий shift200

стресс-сдвиг ключевой ставки +200 б.п. после даты расчёта (H3).

Сделки: 1, 22, 25, 30, 35, 42, 47, 53. Входные сделки — `input/deals.csv`, эталоны — `deal_<id>.csv` (формат 4 контракта), посчитаны `analytics/scripts/make_golden.py`.

Сверка движка (из этого каталога):

```bash
cfgen run --deals input/deals.csv --rates ../../../rates/key_rate.csv --shift-bp 200 --out-dir out
cfgen verify --result out/cashflow.csv --golden-dir .
```
