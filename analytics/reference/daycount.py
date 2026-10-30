"""Конвенции подсчёта дней: доля года между двумя датами."""

from __future__ import annotations

import calendar
import datetime as dt

# H5: ACT/ACT без уточнения считаем как ACT/ACT ISDA.
H5_ACT_ACT_IS_ISDA = True


def act_365(start: dt.date, end: dt.date) -> float:
    """ACT/365 Fixed: фактические дни / 365."""
    return (end - start).days / 365


def act_act_isda(start: dt.date, end: dt.date) -> float:
    """ACT/ACT ISDA: дни в високосных годах делим на 366, в обычных — на 365.

    Период режем по границам календарных лет: 2023-12-01 … 2024-02-01 =
    31/365 (декабрь 2023) + 31/366 (январь 2024).
    """
    total = 0.0
    cur = start
    while cur < end:
        next_year = dt.date(cur.year + 1, 1, 1)
        piece_end = min(next_year, end)
        days_in_year = 366 if calendar.isleap(cur.year) else 365
        total += (piece_end - cur).days / days_in_year
        cur = piece_end
    return total


def year_fraction(convention: str, start: dt.date, end: dt.date) -> float:
    if convention == "ACT/365":
        return act_365(start, end)
    if convention == "ACT/ACT ISDA" or (convention == "ACT/ACT" and H5_ACT_ACT_IS_ISDA):
        return act_act_isda(start, end)
    if convention == "ACT/ACT":
        return act_365(start, end)
    raise ValueError(f"неизвестная конвенция {convention!r}")
