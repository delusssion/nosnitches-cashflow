"""Матрица покрытия: какие возможности калькулятора покрывает каждая тестовая сделка.

Запуск: python analytics/scripts/test_matrix.py — печатает таблицы для docs/TEST_MATRIX.md.
"""

from __future__ import annotations

from collections.abc import Callable

from common import DEALS_CSV
from make_golden import GOLDEN_DEALS
from profile_deals import load_deals

Deal = dict[str, str]

FEATURES: list[tuple[str, Callable[[Deal], bool]]] = [
    ("ANN", lambda d: d["discriminator"] == "ANN"),
    ("RGM", lambda d: d["discriminator"] == "RGM"),
    ("SIMPLE, явный график тела", lambda d: d["discriminator"] == "SIMPLE" and bool(d["principal_payment"])),
    ("ENDDATE / тело в конце", lambda d: d["regular_payment_rule_period_cd"] == "ENDDATE"
        or (d["discriminator"] == "SIMPLE" and not d["principal_payment"])),
    ("FLOAT", lambda d: d["rate_type_cd"] == "FLOAT"),
    ("cap / floor", lambda d: bool(d["cap_limit_interest_rate"] or d["floor_limit_interest_rate"])),
    ("период в днях (45D, 153D)", lambda d: any(d[f].endswith("D") for f in
        ("interest_payment_rule_period_cd", "regular_payment_rule_period_cd", "repricing_rule_period_cd"))),
    ("day_num −1", lambda d: "-1" in (d["interest_payment_rule_day_num"], d["regular_payment_rule_day_num"],
        d["repricing_rule_day_num"], d["sub_payment_rule_day_num"], d["sub_repricing_rule_day_num"])),
    ("явные даты процентов", lambda d: bool(d["interest_payment"])),
    ("явный график репрайсинга", lambda d: bool(d["repricing_dttm_schedule"])),
    ("accrued_interest", lambda d: d["accrued_interest"] != ""),
    ("депозит (остаток < 0)", lambda d: float(d["current_principal_amt"]) < 0),
    ("субсидия", lambda d: bool(d["sub_rate_type_cd"])),
    ("лаг индикатора", lambda d: bool(d["indicator_lag_cd"] or d["sub_indicator_lag_cd"])),
    ("ACT/365", lambda d: d["day_convention_cd"] == "ACT/365"),
    ("ACT/ACT ISDA / ACT/ACT", lambda d: d["day_convention_cd"] in ("ACT/ACT ISDA", "ACT/ACT")),
    ("USD — отказ", lambda d: d["currency_id"] != "RUB"),
]


def main() -> None:
    deals = load_deals(DEALS_CSV)
    ids = [int(d["deal_id"]) for d in deals]
    golden = set(GOLDEN_DEALS)

    print("| Возможность | Сделок | Сделки (**жирным** — есть в golden) | В golden |")
    print("|---|---|---|---|")
    for name, test in FEATURES:
        hit = [int(d["deal_id"]) for d in deals if test(d)]
        listed = ", ".join(f"**{i}**" if i in golden else str(i) for i in hit)
        in_golden = len([i for i in hit if i in golden])
        mark = "✓" if in_golden else "—"
        print(f"| {name} | {len(hit)} | {listed} | {mark} {in_golden} |")

    print()
    print("| Возможность | " + " | ".join(str(i) for i in ids) + " |")
    print("|---|" + "---|" * len(ids))
    print("| golden | " + " | ".join("G" if i in golden else "" for i in ids) + " |")
    for name, test in FEATURES:
        print(f"| {name} | " + " | ".join("●" if test(d) else "" for d in deals) + " |")


if __name__ == "__main__":
    main()
