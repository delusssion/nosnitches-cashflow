"""Профиль качества тестовых сделок: аномалии по каждой сделке.

Запуск: python analytics/scripts/profile_deals.py [--md]
С флагом --md печатает markdown-таблицу для docs/DATA_QUALITY.md.
"""

from __future__ import annotations

import csv
import sys
from collections import Counter
from dataclasses import dataclass

from common import DEALS_CSV

REPORT_FIELD = "report_dttm"
EPS = 0.005


@dataclass
class Finding:
    deal_id: int
    severity: str  # error | warning | info
    code: str
    message: str


def load_deals(path=DEALS_CSV) -> list[dict[str, str]]:
    with path.open(encoding="utf-8") as f:
        return list(csv.DictReader(f))


def num(s: str) -> float | None:
    return float(s) if s != "" else None


def dates_list(s: str) -> list[str]:
    return s.split() if s else []


def pairs_list(s: str) -> list[tuple[str, float]]:
    out = []
    for item in s.split():
        d, a = item.split(",")
        out.append((d, float(a)))
    return out


def money(x: float) -> str:
    return f"{x:,.2f}".replace(",", " ")


def profile_deal(d: dict[str, str]) -> list[Finding]:
    i = int(d["deal_id"])
    rep = d[REPORT_FIELD]
    mat = d["maturity_dttm"]
    cur = num(d["current_principal_amt"]) or 0.0
    out: list[Finding] = []

    def add(sev: str, code: str, msg: str) -> None:
        out.append(Finding(i, sev, code, msg))

    # Валюта и знак
    if d["currency_id"] != "RUB":
        add("error", "UNSUPPORTED_CURRENCY", f"валюта {d['currency_id']} — калькулятор считает только рубли")
    if cur < 0:
        add("info", "LIABILITY", f"отрицательный остаток {money(cur)} — пассив (депозит)")
    start = num(d["start_principal_amt"])
    if start == 0 and cur != 0:
        add("warning", "START_ZERO", f"start_principal_amt = 0 при остатке {money(cur)}")

    # Явный график тела
    principal = pairs_list(d["principal_payment"])
    if principal:
        pdates = [p[0] for p in principal]
        if pdates != sorted(pdates):
            add("warning", "PRINCIPAL_UNSORTED", "даты графика тела не отсортированы")
        dup = [k for k, v in Counter(pdates).items() if v > 1]
        if dup:
            add("warning", "PRINCIPAL_DUP", f"дубли дат графика тела: {', '.join(dup)}")
        past = [p for p in principal if p[0] <= rep]
        if past:
            add("info", "PRINCIPAL_PAST", f"в графике тела {len(past)} дат(ы) ≤ даты расчёта — уже погашено")
        future_sum = sum(a for dd, a in principal if dd > rep)
        diff = future_sum - cur
        if abs(diff) > EPS:
            add(
                "warning",
                "SCHEDULE_SUM_MISMATCH",
                f"сумма будущего графика тела {money(future_sum)} ≠ остатку {money(cur)} (Δ {money(diff)})",
            )
        elif abs(diff) > 1e-9:
            add("info", "SCHEDULE_SUM_ROUNDING", f"график тела отличается от остатка на {money(diff)} (округление)")
        if max(pdates) < mat:
            add("warning", "PRINCIPAL_ENDS_BEFORE_MATURITY", f"график тела заканчивается {max(pdates)}, погашение {mat}")
        if max(pdates) > mat:
            add("warning", "PRINCIPAL_AFTER_MATURITY", "дата графика тела позже даты погашения")

    # Явные даты процентов
    idates = dates_list(d["interest_payment"])
    if idates:
        if idates != sorted(idates):
            add("warning", "INTEREST_UNSORTED", "даты процентов не отсортированы")
        dup = [k for k, v in Counter(idates).items() if v > 1]
        if dup:
            add("warning", "INTEREST_DUP", f"дубли дат процентов: {', '.join(dup)}")
        if rep in idates:
            add("warning", "INTEREST_ON_REPORT_DATE", f"дата процентов {rep} = дата расчёта (H6)")
        if any(x < rep for x in idates):
            add("info", "INTEREST_PAST", "в списке дат процентов есть прошедшие даты")
        if max(idates) < mat:
            add("warning", "INTEREST_ENDS_BEFORE_MATURITY", f"явные даты процентов кончаются {max(idates)}, погашение {mat}")
    if d["interest_payment_rule_period_cd"] and idates:
        add("warning", "INTEREST_RULE_AND_LIST", "заданы и правило, и явный список дат процентов")

    # Где брать даты процентов и тела
    disc = d["discriminator"]
    has_interest = bool(idates or d["interest_payment_rule_period_cd"])
    has_regular = bool(d["regular_payment_rule_period_cd"])
    if not has_interest:
        if has_regular:
            add("info", "INTEREST_FROM_REGULAR", "нет правила процентов — проценты платятся в даты тела")
        elif principal:
            add("info", "INTEREST_FROM_PRINCIPAL", "нет правила процентов — проценты в даты графика тела")
        else:
            add("warning", "NO_INTEREST_SCHEDULE", "нет ни правила, ни дат процентов — проценты в дату погашения")
    if disc in ("ANN", "RGM") and not has_regular:
        add("warning", "NO_REGULAR_RULE", f"{disc} без правила погашения тела — тело гасится в даты процентов")
    if disc == "SIMPLE" and not principal and d["regular_payment_rule_period_cd"] not in ("", "ENDDATE"):
        add("info", "SIMPLE_REGULAR", "SIMPLE с периодическим правилом тела")
    if disc == "SIMPLE" and not principal and not has_regular:
        add("info", "SIMPLE_NO_PRINCIPAL_RULE", "SIMPLE без графика и правила тела — тело в дату погашения")

    # Периоды
    for field in ("interest_payment_rule_period_cd", "regular_payment_rule_period_cd", "repricing_rule_period_cd"):
        p = d[field]
        if p.endswith("D"):
            add("info", "PERIOD_DAYS", f"{field} = {p} — период в днях")
        elif p and p not in ("1M", "3M", "ENDDATE"):
            add("info", "PERIOD_RARE", f"{field} = {p}")
    for field in ("interest_payment_rule_day_num", "regular_payment_rule_day_num", "repricing_rule_day_num", "sub_payment_rule_day_num", "sub_repricing_rule_day_num"):
        v = d[field]
        if v == "-1":
            add("info", "DAY_NUM_LAST", f"{field} = -1 (последний день месяца)")
        elif v and int(v) > 28:
            add("info", "DAY_NUM_CLAMP", f"{field} = {v} — в коротких месяцах сдвигается на последний день")
    for prefix in ("interest_payment_rule", "regular_payment_rule"):
        if d[f"{prefix}_period_cd"].endswith("M") and not d[f"{prefix}_day_num"]:
            add("info", "DAY_NUM_FROM_START", f"{prefix}: day_num не задан — берём день из даты начала правила")
    if d["regular_payment_rule_start_dttm"] and d["deal_dttm"] and d["regular_payment_rule_start_dttm"] < d["deal_dttm"]:
        add("info", "RULE_BEFORE_DEAL", "правило погашения начинается раньше даты сделки")
    if d["repricing_rule_start_dttm"] and not d["repricing_rule_period_cd"]:
        add("warning", "REPRICING_START_NO_PERIOD", "задано начало правила репрайсинга, но нет периода")

    # Ставка
    if d["rate_type_cd"] == "FLOAT":
        if not d["base_indicator"]:
            add("error", "FLOAT_NO_INDICATOR", "FLOAT без base_indicator")
        if not (d["repricing_rule_period_cd"] or d["repricing_dttm_schedule"]):
            add("warning", "FLOAT_NO_REPRICING", "FLOAT без правила и дат репрайсинга — ставка фиксируется на дату сделки")
        cap, floor = num(d["cap_limit_interest_rate"]), num(d["floor_limit_interest_rate"])
        if cap is not None and floor is not None and cap < floor:
            add("error", "CAP_BELOW_FLOOR", "cap < floor")
    else:
        if d["base_indicator"] or d["repricing_rule_period_cd"]:
            add("warning", "FIXED_WITH_FLOAT_FIELDS", "FIXED, но заданы поля плавающей ставки")
    if d["base_indicator_rate"]:
        add("info", "BASE_INDICATOR_RATE", f"base_indicator_rate = {d['base_indicator_rate']} (смысл не определён, Q2)")
    rschedule = dates_list(d["repricing_dttm_schedule"])
    if rschedule and d["deal_dttm"] and min(rschedule) < d["deal_dttm"]:
        add("info", "REPRICING_BEFORE_DEAL", "даты репрайсинга раньше даты сделки")
    if d["repricing_dttm_rate_schedule_fixed"]:
        add("info", "RATE_SCHEDULE_FIXED", "явный график фиксированной части ставки")
    if d["accrued_interest"]:
        add("info", "ACCRUED_GIVEN", f"accrued_interest = {d['accrued_interest']}")
    if d["day_convention_cd"] == "ACT/ACT":
        add("info", "ACT_ACT", "конвенция ACT/ACT — считаем как ISDA (H5)")
    if d["indicator_lag_cd"]:
        add("info", "INDICATOR_LAG", f"лаг индикатора {d['indicator_lag_cd']} (Q4)")

    # Субсидия
    if d["sub_rate_type_cd"]:
        add("info", "SUBSIDY", f"субсидия {d['sub_rate_type_cd']} {d['sub_interest_rate']}, коэффициент {d['sub_coeff_index'] or '—'}")
        if d["sub_rate_type_cd"] == "FIXED" and (d["sub_float_indicator"] or d["sub_coeff_index"] not in ("", "0")):
            add("warning", "SUB_FIXED_WITH_FLOAT_FIELDS", "субсидия FIXED, но заданы индикатор/коэффициент")
        if d["sub_payment_rule_start_dttm"] > rep:
            add("info", "SUB_STARTS_LATER", f"субсидия начинается {d['sub_payment_rule_start_dttm']}, после даты расчёта")
        if not d["sub_payment_rule_day_num"]:
            add("info", "SUB_DAY_NUM_FROM_START", "sub_payment_rule_day_num не задан — день из даты начала")
    return out


def profile(deals: list[dict[str, str]]) -> list[Finding]:
    return [f for d in deals for f in profile_deal(d)]


def main() -> None:
    findings = profile(load_deals())
    if "--md" in sys.argv:
        print("| Сделка | Уровень | Код | Описание |\n|---|---|---|---|")
        for f in findings:
            if f.severity != "info":
                print(f"| {f.deal_id} | {f.severity} | `{f.code}` | {f.message} |")
        return
    by_sev = Counter(f.severity for f in findings)
    print(f"Находок: {len(findings)} ({', '.join(f'{k} {v}' for k, v in by_sev.items())})")
    for f in findings:
        print(f"{f.deal_id:>3} {f.severity:<8} {f.code:<32} {f.message}")
    print("\nПо кодам:")
    for code, n in Counter(f.code for f in findings).most_common():
        ids = sorted({f.deal_id for f in findings if f.code == code})
        print(f"  {code:<32} {n:>3}  сделки {', '.join(map(str, ids))}")


if __name__ == "__main__":
    main()
