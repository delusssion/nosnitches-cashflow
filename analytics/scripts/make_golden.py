"""Эталонные кэшфлоу: testdata/golden/deal_<id>.csv для набора эталонных сделок.

Запуск: python analytics/scripts/make_golden.py [--all]
С --all считает все 55 сделок в testdata/golden/all/ (для разбора, не эталон) и печатает предупреждения.
"""

from __future__ import annotations

import datetime as dt
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "reference"))
import csv  # noqa: E402

from cf_reference import (  # noqa: E402
    DealError,
    Scenario,
    generate_with_warnings,
    load_deals,
    load_key_rate,
    write_events,
)
from common import DEALS_CSV, GOLDEN_DIR, KEY_RATE_CSV  # noqa: E402

# По одной сделке на каждый случай (задача A5).
GOLDEN_DEALS = {
    1: "ANN, 30 лет, ACT/ACT",
    2: "RGM",
    15: "депозит",
    16: "SIMPLE, явный график тела",
    21: "SIMPLE, ENDDATE",
    22: "FLOAT, cap/floor",
    25: "FLOAT, cap 0.1 / floor 0.06",
    30: "период 45D",
    35: "day_num -1",
    42: "явный график репрайсинга",
    44: "явные даты, проценты в дату расчёта",
    47: "субсидия FLOAT",
    53: "субсидия с коэффициентом 0.9",
}


# Сценарные эталоны спринта 3: testdata/golden/scenarios/<имя>/deal_<id>.csv,
# входные сделки — в input/deals.csv рядом (read_dir у cfgen verify не рекурсивный).
PREPAY = {  # сделка → досрочки «дата,сумма»
    1: "2025-06-22,500000",  # ANN, в плановую дату тела (22-е)
    2: "2023-03-10,300000",  # RGM, вне дат тела
    15: "2022-12-01,500000",  # депозит: частичное изъятие
    16: "2022-10-15,100000",  # SIMPLE с графиком
    22: "2022-11-20,250000",  # RGM FLOAT, в плановую дату тела (20-е)
    47: "2022-12-01,50000",  # ANN с субсидией
}
SCENARIOS = {
    "prepay": {
        "about": "досрочные погашения (H10): колонка prepayment_schedule",
        "deals": PREPAY,
        "scenario": Scenario(),
        "run": "cfgen run --deals input/deals.csv --rates ../../../rates/key_rate.csv --out-dir out",
    },
    "shift200": {
        "about": "стресс-сдвиг ключевой ставки +200 б.п. после даты расчёта (H3)",
        "deals": {i: "" for i in (1, 22, 25, 30, 35, 42, 47, 53)},
        "scenario": Scenario(shift_bp=200),
        "run": "cfgen run --deals input/deals.csv --rates ../../../rates/key_rate.csv --shift-bp 200 --out-dir out",
    },
}


def make_scenarios(deals: dict[int, dict[str, str]], key_rate) -> None:
    for name, spec in SCENARIOS.items():
        out_dir = GOLDEN_DIR / "scenarios" / name
        (out_dir / "input").mkdir(parents=True, exist_ok=True)
        header = list(next(iter(deals.values())).keys())
        with_prepay = any(spec["deals"].values())
        rows = []
        for i, sched in spec["deals"].items():
            deal = dict(deals[i])
            if with_prepay:
                deal["prepayment_schedule"] = sched
            rows.append(deal)
            report = dt.date.fromisoformat(deal["report_dttm"])
            events, _ = generate_with_warnings(deal, key_rate, report, spec["scenario"])
            write_events(out_dir / f"deal_{i}.csv", events)
        cols = header + (["prepayment_schedule"] if with_prepay else [])
        with (out_dir / "input" / "deals.csv").open("w", newline="", encoding="utf-8") as fh:
            w = csv.DictWriter(fh, fieldnames=cols, lineterminator="\n")
            w.writeheader()
            w.writerows(rows)
        (out_dir / "README.md").write_text(
            f"# Сценарий {name}\n\n{spec['about']}.\n\n"
            f"Сделки: {', '.join(map(str, spec['deals']))}. Входные сделки — `input/deals.csv`, "
            "эталоны — `deal_<id>.csv` (формат 4 контракта), посчитаны `analytics/scripts/make_golden.py`.\n\n"
            "Сверка движка (из этого каталога):\n\n"
            f"```bash\n{spec['run']}\ncfgen verify --result out/cashflow.csv --golden-dir .\n```\n",
            encoding="utf-8",
        )
        print(f"сценарий {name}: {len(rows)} сделок → {out_dir.relative_to(GOLDEN_DIR.parents[1])}")


def main() -> None:
    deals = {int(d["deal_id"]): d for d in load_deals(DEALS_CSV)}
    key_rate = load_key_rate(KEY_RATE_CSV)
    if "--all" not in sys.argv:
        make_scenarios(deals, key_rate)
    ids = sorted(deals) if "--all" in sys.argv else sorted(GOLDEN_DEALS)
    out_dir = GOLDEN_DIR / "all" if "--all" in sys.argv else GOLDEN_DIR
    for i in ids:
        deal = deals[i]
        report = dt.date.fromisoformat(deal["report_dttm"])
        try:
            events, warnings = generate_with_warnings(deal, key_rate, report)
        except DealError as e:
            print(f"{i:>3} ERROR {e.code}: {e.message}")
            continue
        write_events(out_dir / f"deal_{i}.csv", events)
        last = events[-1] if events else None
        note = GOLDEN_DEALS.get(i, "")
        print(f"{i:>3} {deal['discriminator']:<6} событий {len(events):>4}  остаток {last['principal_after'] if last else '-':>8}  {note}")
        for code, _, msg in warnings:
            print(f"      warning {code}: {msg}")


if __name__ == "__main__":
    main()
