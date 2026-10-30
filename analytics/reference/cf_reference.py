"""Независимый эталонный расчёт кэшфлоу по методике docs/METHODOLOGY.md.

Код намеренно простой и построчный: его должно быть можно проверить глазами.
Он не связан с движком на Rust и не повторяет его логику.

Главная функция: generate(deal, key_rate, report_date) -> list[dict] — события в формате 3 docs/CONTRACT.md.
"""

from __future__ import annotations

import calendar
import csv
import datetime as dt
from bisect import bisect_right
from dataclasses import dataclass, field
from pathlib import Path

from daycount import year_fraction

# --- Гипотезы контракта (меняются одной правкой) -----------------------------------------
H1_RGM_EQUAL_SHARES = True  # RGM: остаток на дату расчёта / число оставшихся дат тела
H2_FLOAT_INDICATOR_PLUS_MARGIN = True  # FLOAT: индикатор + interest_rate, затем floor/cap
H3_FORECAST_SHIFT = 0.0  # будущий индикатор = последнее значение на дату расчёта + сдвиг
H4_SUB_COEFF_DEFAULT = 1.0  # субсидия: coeff (пусто → 1) × индикатор + sub_interest_rate
H6_REPORT_DATE_IS_PAST = True  # событие ровно в дату расчёта — прошлое
H7_ANN_RECALC_ON_REPRICING = True  # ANN: аннуитет от остатка на дату расчёта и при каждом репрайсинге
H8_NO_BUSINESS_DAY_SHIFT = True  # выходные и праздники не переносим (ничего не делаем)
H9_SCHEDULE_DIFF_TO_CLOSE = True  # разница явного графика тела и остатка — в close_payment + warning

# --- Наши решения по открытым вопросам (docs/QUESTIONS.md) -------------------------------
Q13_ACCRUED_FROM_FIELD = True  # accrued_interest задан → это проценты, накопленные на дату расчёта
Q14_FLOAT_WITHOUT_REPRICING_FIX_AT_DEAL = True  # FLOAT без репрайсинга: ставка на дату сделки

# Порядок событий в один день (контракт, формат 3).
DAY_ORDER = {
    "repricing": 0,
    "sub_repricing": 0,
    "interest_payment": 1,
    "sub_payment": 2,
    "principal_payment": 3,
    "close_payment": 4,
}

OUTPUT_COLUMNS = [
    "report_dttm", "deal_id", "event_order", "payment_dttm", "payment_type", "stream", "amount",
    "principal_before", "principal_after", "rate", "base_rate", "accrual_start_dttm",
    "accrual_end_dttm", "accrual_days", "year_frac", "discriminator",
]


class DealError(Exception):
    """Сделку посчитать нельзя (severity=error в errors.csv)."""

    def __init__(self, code: str, field_name: str, message: str):
        super().__init__(message)
        self.code, self.field, self.message = code, field_name, message


# --- Чтение входа --------------------------------------------------------------------------


def parse_date(s: str) -> dt.date | None:
    return dt.date.fromisoformat(s) if s else None


def parse_float(s: str) -> float | None:
    return float(s) if s != "" else None


def parse_int(s: str) -> int | None:
    return int(float(s)) if s != "" else None


def load_key_rate(path: Path) -> list[tuple[dt.date, float]]:
    with path.open(encoding="utf-8") as f:
        rows = [(dt.date.fromisoformat(r["date"]), float(r["rate"])) for r in csv.DictReader(f)]
    return sorted(rows)


def load_deals(path: Path) -> list[dict[str, str]]:
    with path.open(encoding="utf-8") as f:
        return list(csv.DictReader(f))


# --- Даты ----------------------------------------------------------------------------------


def last_day(year: int, month: int) -> int:
    return calendar.monthrange(year, month)[1]


def add_months(anchor: dt.date, months: int, day_num: int | None) -> dt.date:
    """Дата через `months` месяцев от `anchor`.

    day_num: число месяца; -1 — последний день; None — день из anchor.
    Если в месяце нет такого дня (31 в апреле, 30 в феврале) — последний день месяца.
    """
    m0 = anchor.month - 1 + months
    year, month = anchor.year + m0 // 12, m0 % 12 + 1
    want = anchor.day if day_num is None else day_num
    if want == -1:
        return dt.date(year, month, last_day(year, month))
    return dt.date(year, month, min(want, last_day(year, month)))


def rule_dates(start: dt.date | None, period: str, day_num: int | None, end: dt.date) -> list[dt.date]:
    """Даты по правилу «начало + период» в интервале (start, end].

    Период: '<N>M' — каждые N месяцев (число месяца — day_num или из start),
            '<N>D' — каждые N дней, 'ENDDATE' или пусто — дат нет (только погашение).
    """
    if start is None or period in ("", "ENDDATE"):
        return []
    n, unit = int(period[:-1]), period[-1]
    out: list[dt.date] = []
    k = 0
    while True:
        d = add_months(start, k * n, day_num) if unit == "M" else start + dt.timedelta(days=k * n)
        if d > end:
            return out
        if d > start:
            out.append(d)
        k += 1


def explicit_dates(s: str) -> list[dt.date]:
    """Явный список дат: сортируем и убираем дубли (контракт, формат 1)."""
    return sorted({dt.date.fromisoformat(x) for x in s.split()})


def explicit_amounts(s: str) -> dict[dt.date, float]:
    """Явный график «дата,сумма»: суммы в одну дату складываем."""
    out: dict[dt.date, float] = {}
    for item in s.split():
        d, a = item.split(",")
        day = dt.date.fromisoformat(d)
        out[day] = out.get(day, 0.0) + float(a)
    return dict(sorted(out.items()))


def shift_by_lag(d: dt.date, lag: str) -> dt.date:
    """Лаг индикатора: '-1M' → дата минус месяц, '-5D' → минус 5 дней."""
    if not lag:
        return d
    n, unit = int(lag[:-1]), lag[-1]
    return add_months(d, n, None) if unit == "M" else d + dt.timedelta(days=n)


def period_months(period: str) -> float:
    """Длина периода в месяцах для ставки периода аннуитета (H7)."""
    if period.endswith("M"):
        return float(period[:-1])
    if period.endswith("D"):
        return int(period[:-1]) * 12 / 365
    return 1.0


# --- Ставки --------------------------------------------------------------------------------


@dataclass
class Indicator:
    """Ключевая ставка ЦБ — ступенчатая функция; будущее — по H3."""

    history: list[tuple[dt.date, float]]
    report: dt.date

    def __post_init__(self) -> None:
        self.dates = [d for d, _ in self.history]

    def known(self, d: dt.date) -> float:
        i = bisect_right(self.dates, d) - 1
        if i < 0:
            raise ValueError(f"нет значения индикатора на {d}")
        return self.history[i][1]

    def value(self, d: dt.date) -> float:
        if d <= self.report:
            return self.known(d)
        return self.known(self.report) + H3_FORECAST_SHIFT


def clamp(rate: float, floor: float | None, cap: float | None) -> float:
    if floor is not None:
        rate = max(rate, floor)
    if cap is not None:
        rate = min(rate, cap)
    return rate


@dataclass
class RateLeg:
    """Ставка одного потока (клиент или субсидия) во времени."""

    kind: str  # FIXED | FLOAT
    spread: float  # FIXED — ставка, FLOAT — маржа (H2) или sub_interest_rate (H4)
    coeff: float  # множитель индикатора (1 для клиента, sub_coeff_index для субсидии)
    floor: float | None
    cap: float | None
    lag: str
    deal_date: dt.date
    repricing: list[dt.date]  # все даты пересмотра, в том числе прошлые
    spread_schedule: dict[dt.date, float] = field(default_factory=dict)  # явная фиксированная часть

    def fixing_date(self, t: dt.date) -> dt.date:
        """Дата, на которую фиксируется ставка, действующая в момент t."""
        past = [d for d in self.repricing if d <= t]
        last = max(past) if past else self.deal_date
        return max(last, self.deal_date)

    def spread_at(self, t: dt.date) -> float:
        past = [d for d in self.spread_schedule if d <= t]
        return self.spread_schedule[max(past)] if past else self.spread

    def at(self, t: dt.date, ind: Indicator) -> tuple[float, float | None]:
        """(ставка, значение индикатора) в момент t."""
        if self.kind == "FIXED":
            return self.spread_at(t), None
        base = ind.value(shift_by_lag(self.fixing_date(t), self.lag))
        return clamp(self.coeff * base + self.spread_at(t), self.floor, self.cap), base


# --- Сделка --------------------------------------------------------------------------------


def schedule_dates(deal: dict[str, str], prefix: str, end: dt.date) -> list[dt.date]:
    return rule_dates(
        parse_date(deal[f"{prefix}_start_dttm"]),
        deal[f"{prefix}_period_cd"],
        parse_int(deal[f"{prefix}_day_num"]),
        end,
    )


def interest_dates_all(deal: dict[str, str], maturity: dt.date) -> list[dt.date]:
    """Все даты выплаты процентов (прошлые и будущие) + дата погашения.

    Источник по приоритету: явный список → правило процентов → правило тела →
    даты явного графика тела → только дата погашения.
    """
    if deal["interest_payment"]:
        dates = explicit_dates(deal["interest_payment"])
    elif deal["interest_payment_rule_period_cd"]:
        dates = schedule_dates(deal, "interest_payment_rule", maturity)
    elif deal["regular_payment_rule_period_cd"] not in ("", "ENDDATE"):
        dates = schedule_dates(deal, "regular_payment_rule", maturity)
    elif deal["principal_payment"]:
        dates = list(explicit_amounts(deal["principal_payment"]))
    else:
        dates = []
    return sorted({d for d in dates if d <= maturity} | {maturity})


def principal_dates_all(deal: dict[str, str], maturity: dt.date, interest_dates: list[dt.date]) -> list[dt.date]:
    """Даты погашения тела для ANN/RGM: правило тела, иначе даты процентов; + дата погашения."""
    if deal["regular_payment_rule_period_cd"] not in ("", "ENDDATE"):
        dates = schedule_dates(deal, "regular_payment_rule", maturity)
    elif deal["regular_payment_rule_period_cd"] == "ENDDATE":
        dates = []
    else:
        dates = interest_dates
    return sorted({d for d in dates if d <= maturity} | {maturity})


def is_future(d: dt.date, report: dt.date) -> bool:
    return d > report if H6_REPORT_DATE_IS_PAST else d >= report


def make_client_leg(deal: dict[str, str], deal_date: dt.date, maturity: dt.date) -> RateLeg:
    kind = deal["rate_type_cd"]
    repricing = explicit_dates(deal["repricing_dttm_schedule"]) if deal["repricing_dttm_schedule"] else []
    if deal["repricing_rule_period_cd"]:
        repricing = sorted(set(repricing) | set(schedule_dates(deal, "repricing_rule", maturity)))
    spread_schedule = {}
    if deal["repricing_dttm_rate_schedule_fixed"]:
        spread_schedule = explicit_amounts(deal["repricing_dttm_rate_schedule_fixed"])
    return RateLeg(
        kind=kind,
        spread=parse_float(deal["interest_rate"]) or 0.0,
        coeff=1.0,
        floor=parse_float(deal["floor_limit_interest_rate"]),
        cap=parse_float(deal["cap_limit_interest_rate"]),
        lag=deal["indicator_lag_cd"],
        deal_date=deal_date,
        repricing=repricing,
        spread_schedule=spread_schedule,
    )


def make_sub_leg(deal: dict[str, str], deal_date: dt.date, maturity: dt.date) -> RateLeg | None:
    kind = deal["sub_rate_type_cd"]
    if not kind:
        return None
    coeff = parse_float(deal["sub_coeff_index"])
    return RateLeg(
        kind=kind,
        spread=parse_float(deal["sub_interest_rate"]) or 0.0,
        coeff=H4_SUB_COEFF_DEFAULT if coeff is None else coeff,
        floor=parse_float(deal["sub_floor_limit_interest_rate"]),
        cap=parse_float(deal["sub_cap_limit_interest_rate"]),
        lag=deal["sub_indicator_lag_cd"],
        deal_date=deal_date,
        repricing=schedule_dates(deal, "sub_repricing_rule", maturity) if kind == "FLOAT" else [],
    )


def annuity(balance: float, annual_rate: float, months: float, n: int) -> float:
    """$A = B \\cdot i / (1 - (1+i)^{-n})$, ставка периода $i = r \\cdot m / 12$ (H7)."""
    i = annual_rate * months / 12
    if n <= 0:
        return balance
    if i == 0:
        return balance / n
    return balance * i / (1 - (1 + i) ** -n)


# --- Расчёт --------------------------------------------------------------------------------


def generate_with_warnings(
    deal: dict[str, str], key_rate: list[tuple[dt.date, float]], report_date: dt.date
) -> tuple[list[dict], list[tuple[str, str, str]]]:
    """События сделки в формате 3 и предупреждения [(code, field, message)]."""
    warnings: list[tuple[str, str, str]] = []

    # 0. Проверки, при которых сделку не считаем.
    if deal["currency_id"] != "RUB":
        raise DealError("UNSUPPORTED_CURRENCY", "currency_id", f"валюта {deal['currency_id']}, считаем только RUB")
    disc = deal["discriminator"]
    if disc not in ("ANN", "RGM", "SIMPLE"):
        raise DealError("UNKNOWN_DISCRIMINATOR", "discriminator", f"неизвестный discriminator {disc!r}")
    if deal["rate_type_cd"] == "FLOAT" and not deal["base_indicator"]:
        raise DealError("FLOAT_NO_INDICATOR", "base_indicator", "FLOAT без индикатора")

    report = report_date
    maturity = parse_date(deal["maturity_dttm"])
    deal_date = parse_date(deal["deal_dttm"]) or report
    balance = float(deal["current_principal_amt"])
    convention = deal["day_convention_cd"]
    ind = Indicator(key_rate, report)
    tau = lambda a, b: year_fraction(convention, a, b)  # noqa: E731

    if not is_future(maturity, report):
        raise DealError("MATURITY_BEFORE_REPORT", "maturity_dttm", f"погашение {maturity} не позже даты расчёта")
    if balance == 0:
        return [], warnings

    # 1. Даты.
    interest_all = interest_dates_all(deal, maturity)
    interest_future = [d for d in interest_all if is_future(d, report)]
    past_interest = [d for d in interest_all if not is_future(d, report)]
    accrual_start = max(past_interest) if past_interest else deal_date

    leg = make_client_leg(deal, deal_date, maturity)
    if deal["rate_type_cd"] == "FLOAT" and not leg.repricing and Q14_FLOAT_WITHOUT_REPRICING_FIX_AT_DEAL:
        warnings.append(("FLOAT_NO_REPRICING", "repricing_rule_period_cd", "ставка зафиксирована на дату сделки"))
    repricing_future = [d for d in leg.repricing if is_future(d, report) and d < maturity]
    if leg.kind == "FIXED":
        repricing_future = [d for d in leg.spread_schedule if is_future(d, report) and d < maturity]

    # Тело.
    if disc == "SIMPLE":
        planned = {}
        if deal["principal_payment"]:
            planned = {d: a for d, a in explicit_amounts(deal["principal_payment"]).items() if is_future(d, report)}
            total = sum(planned.values())
            if abs(total - balance) > 0.005:
                warnings.append(
                    ("SCHEDULE_SUM_MISMATCH", "principal_payment", f"график {total:.2f} ≠ остатку {balance:.2f} (H9)")
                )
        principal_future = sorted({d for d in planned if d <= maturity} | {maturity})
    else:
        principal_future = [d for d in principal_dates_all(deal, maturity, interest_all) if is_future(d, report)]
        planned = {}

    sub = make_sub_leg(deal, deal_date, maturity)
    sub_future: list[dt.date] = []
    sub_repricing_future: list[dt.date] = []
    sub_start = None
    if sub:
        sub_rule_start = parse_date(deal["sub_payment_rule_start_dttm"]) or deal_date
        sub_all = sorted(set(schedule_dates(deal, "sub_payment_rule", maturity)) | {maturity})
        sub_past = [d for d in sub_all if not is_future(d, report)]
        sub_start = max(sub_past) if sub_past else max(sub_rule_start, deal_date)
        sub_future = [d for d in sub_all if is_future(d, report)]
        sub_repricing_future = [d for d in sub.repricing if is_future(d, report) and d < maturity]

    # 2. Состояние на дату расчёта.
    rate, base = leg.at(report, ind)
    accrued = 0.0
    accrued_field = parse_float(deal["accrued_interest"])
    if Q13_ACCRUED_FROM_FIELD and accrued_field is not None:
        accrued = abs(accrued_field) * (1 if balance > 0 else -1)  # со знаком сделки
        cursor = max(report, accrual_start)
    else:
        cursor = accrual_start  # досчитаем проценты с начала периода по ставкам того времени
    sub_rate, sub_base = (sub.at(report, ind) if sub else (0.0, None))
    sub_accrued = 0.0
    sub_cursor = sub_start or report  # субсидия может начаться и после даты расчёта (сделка 50)
    last_interest = accrual_start
    last_sub = sub_start

    # Проценты от начала периода до даты расчёта (в периоде могли быть пересмотры ставки).
    def accrue_past(start: dt.date, leg_: RateLeg, bal: float) -> float:
        points = sorted({start, report} | {d for d in leg_.repricing if start < d < report})
        total = 0.0
        for a, b in zip(points, points[1:]):
            total += bal * leg_.at(a, ind)[0] * tau(a, b)
        return total

    if cursor < report:
        accrued += accrue_past(cursor, leg, balance)
        cursor = report
    if sub and sub_cursor < report:
        sub_accrued += accrue_past(sub_cursor, sub, balance)
    if sub:
        sub_cursor = max(report, sub_cursor)

    # Аннуитет (H7): от остатка на дату расчёта.
    months = period_months(deal["regular_payment_rule_period_cd"])
    n_left = len(principal_future)
    ann_payment = annuity(balance, rate, months, n_left) if disc == "ANN" else 0.0
    rgm_share = balance / n_left if disc == "RGM" and n_left else 0.0

    # 3. Проход по будущим датам событий.
    points = sorted(
        set(interest_future) | set(principal_future) | set(repricing_future) | set(sub_future) | set(sub_repricing_future)
    )
    events: list[dict] = []
    rate_used, base_used = rate, base
    sub_rate_used, sub_base_used = sub_rate, sub_base

    def emit(day: dt.date, ptype: str, stream: str, amount: float, before: float, after: float,
             r: float, b: float | None, acc: tuple[dt.date, dt.date] | None = None) -> None:
        event = {
            "payment_dttm": day, "payment_type": ptype, "stream": stream, "amount": amount,
            "principal_before": before, "principal_after": after, "rate": r, "base_rate": b,
            "accrual_start_dttm": None, "accrual_end_dttm": None, "accrual_days": None, "year_frac": None,
        }
        if acc:
            event.update(accrual_start_dttm=acc[0], accrual_end_dttm=acc[1],
                         accrual_days=(acc[1] - acc[0]).days, year_frac=tau(acc[0], acc[1]))
        events.append(event)

    for p in points:
        # 3.1 Начисление на отрезке [cursor, p) при текущих остатке и ставке: I += B · r · τ.
        accrued += balance * rate * tau(cursor, p)
        rate_used, base_used = rate, base
        cursor = p
        if sub and sub_cursor < p:
            sub_accrued += balance * sub_rate * tau(sub_cursor, p)
            sub_rate_used, sub_base_used = sub_rate, sub_base
            sub_cursor = p

        # 3.2 Пересмотр ставки: новая ставка действует с даты p.
        repriced = False
        if p in repricing_future:
            rate, base = leg.at(p, ind)
            emit(p, "repricing", "client", 0.0, balance, balance, rate, base)
            repriced = True
        if p in sub_repricing_future:
            sub_rate, sub_base = sub.at(p, ind)
            emit(p, "sub_repricing", "sub", 0.0, balance, balance, sub_rate, sub_base)

        # 3.3 Выплата процентов за период (last_interest, p].
        interest_paid = 0.0
        if p in interest_future:
            interest_paid = accrued
            emit(p, "interest_payment", "client", accrued, balance, balance, rate_used, base_used, (last_interest, p))
            accrued = 0.0
            last_interest = p

        # 3.4 Выплата субсидии.
        if p in sub_future:
            emit(p, "sub_payment", "sub", sub_accrued, balance, balance, sub_rate_used, sub_base_used, (last_sub, p))
            sub_accrued = 0.0
            last_sub = p

        # 3.5 Погашение тела.
        if p in principal_future:
            is_last = p == principal_future[-1]
            if is_last:
                amount = balance
            elif disc == "ANN":
                amount = ann_payment - interest_paid
            elif disc == "RGM":
                amount = rgm_share
            else:
                amount = planned.get(p, 0.0)
            # не гасим больше остатка и не «выдаём» тело обратно
            amount = max(0.0, min(abs(amount), abs(balance))) * (1 if balance > 0 else -1)
            if is_last or amount != 0:
                closes = is_last or abs(balance - amount) < 1e-9
                # Тело гасится раньше даты процентов — проценты за хвост платим в день закрытия.
                if closes and p not in interest_future and accrued != 0:
                    emit(p, "interest_payment", "client", accrued, balance, balance, rate_used, base_used,
                         (last_interest, p))
                    accrued = 0.0
                if closes and sub and p not in sub_future and sub_accrued != 0:
                    emit(p, "sub_payment", "sub", sub_accrued, balance, balance, sub_rate_used, sub_base_used,
                         (last_sub, p))
                    sub_accrued = 0.0
                ptype = "close_payment" if closes else "principal_payment"
                emit(p, ptype, "client", amount, balance, balance - amount if ptype != "close_payment" else 0.0,
                     rate, base)
                balance = 0.0 if ptype == "close_payment" else balance - amount
            if balance == 0:
                break

        # 3.6 ANN: пересчёт платежа после пересмотра ставки (H7).
        if disc == "ANN" and repriced and H7_ANN_RECALC_ON_REPRICING:
            n_left = len([d for d in principal_future if d > p])
            ann_payment = annuity(balance, rate, months, n_left)

    # 4. Нумерация и общие поля.
    events.sort(key=lambda e: (e["payment_dttm"], DAY_ORDER[e["payment_type"]]))
    for k, e in enumerate(events, start=1):
        e.update(report_dttm=report, deal_id=deal["deal_id"], event_order=k, discriminator=disc)
    return events, warnings


def generate(deal: dict[str, str], key_rate: list[tuple[dt.date, float]], report_date: dt.date) -> list[dict]:
    return generate_with_warnings(deal, key_rate, report_date)[0]


# --- Вывод ---------------------------------------------------------------------------------


def fmt_cell(column: str, value) -> str:
    if value is None:
        return ""
    if isinstance(value, dt.date):
        return value.isoformat()
    if column in ("amount", "principal_before", "principal_after"):
        text = f"{value:.2f}"
        return "0.00" if text == "-0.00" else text
    if column == "year_frac":
        return f"{value:.10f}"
    if isinstance(value, float):
        text = repr(round(value, 10))
        return text[:-2] if text.endswith(".0") else text
    return str(value)


def write_events(path: Path, events: list[dict]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", newline="", encoding="utf-8") as f:
        w = csv.writer(f, lineterminator="\n")
        w.writerow(OUTPUT_COLUMNS)
        for e in events:
            w.writerow([fmt_cell(c, e[c]) for c in OUTPUT_COLUMNS])
