"""Эталонные кэшфлоу: testdata/golden/deal_<id>.csv для набора эталонных сделок.

Запуск: python analytics/scripts/make_golden.py [--all]
С --all считает все 55 сделок в testdata/golden/all/ (для разбора, не эталон) и печатает предупреждения.
"""

from __future__ import annotations

import datetime as dt
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "reference"))
from cf_reference import DealError, generate_with_warnings, load_deals, load_key_rate, write_events  # noqa: E402
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


def main() -> None:
    deals = {int(d["deal_id"]): d for d in load_deals(DEALS_CSV)}
    key_rate = load_key_rate(KEY_RATE_CSV)
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
