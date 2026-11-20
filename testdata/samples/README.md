# testdata/samples

Это демо-данные для разработки UI, **не эталоны**. Эталоны — `testdata/golden`.

- `deals_sample.csv` — 3 сделки (формат 1 контракта), дата расчёта 2022-08-31:
  ANN 600 000 под 12 %, SIMPLE FLOAT по CBKEY1M + 3 % с репрайсингом, депозит −300 000 под 6 %.
- `cashflow_sample.csv` — их кэшфлоу (формат 3), посчитан упрощённо скриптом `web/scripts/make-sample.ts`.
  Индикатор для FLOAT взят условно: 8 % до 2022-10-01, 7,5 % после.

Пересоздать: `cd web && npm run make-sample`.

## engine/

Реальный вывод движка `cfgen run` по 55 тестовым сделкам (`testdata/deals/test_deals.csv`, ключевая ставка `testdata/rates/key_rate.csv`), движок на коммите `884910e`: `cashflow.csv` и `errors.csv`. Тоже не эталон — это то, что сверяется с эталонами на экране «Сверка» (кнопка «Движок против эталонов»).

## scenario/

Демо для экрана «Сценарий: было / стало» — два прогона `cfgen run` по сделкам 7, 22, 25: `deals_before.csv` → `cashflow_before.csv` (как в тестовых данных) и `deals_after.csv` → `cashflow_after.csv`: сделка 22 — досрочка 250 000 ₽ 20.11.2022 (колонка `prepayment_schedule`), сделка 7 — ставка 15 % → 20 %, сделка 25 — cap 10 % → 20 %. Не эталон.
