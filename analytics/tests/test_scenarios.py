"""Сценарные эталоны спринта 3: досрочки (H10) и стресс-сдвиг +200 б.п. (H3)."""

import csv
import datetime as dt

import pytest

from cf_reference import Scenario, generate, load_key_rate
from common import GOLDEN_DIR, KEY_RATE_CSV
from make_golden import PREPAY, SCENARIOS

KEY_RATE = load_key_rate(KEY_RATE_CSV)
REPORT = dt.date(2022, 8, 31)
BODY = ("principal_payment", "prepayment", "close_payment")


def input_deals(name):
    with (GOLDEN_DIR / "scenarios" / name / "input" / "deals.csv").open(encoding="utf-8") as f:
        return {int(d["deal_id"]): d for d in csv.DictReader(f)}


@pytest.mark.parametrize("i", sorted(PREPAY))
def test_prepay_invariants(i):
    deal = input_deals("prepay")[i]
    events = generate(deal, KEY_RATE, REPORT)
    body = [e for e in events if e["payment_type"] in BODY]
    assert [e["payment_type"] for e in body].count("prepayment") == 1
    # тело вместе с досрочкой = остатку; закрытие — в дату погашения (H10: срок прежний)
    assert sum(e["amount"] for e in body) == pytest.approx(float(deal["current_principal_amt"]), abs=1e-6)
    assert body[-1]["payment_type"] == "close_payment"
    assert body[-1]["payment_dttm"] == dt.date.fromisoformat(deal["maturity_dttm"])
    assert body[-1]["principal_after"] == 0
    # досрочка в один день с плановым телом — раньше него
    order = [(e["payment_dttm"], e["payment_type"]) for e in events]
    for k, (d, t) in enumerate(order):
        if t == "prepayment" and k + 1 < len(order) and order[k + 1][0] == d:
            assert order[k + 1][1] in ("principal_payment", "close_payment")


def test_prepay_lowers_interest_and_subsidy_base():
    base = generate(input_deals("prepay")[47] | {"prepayment_schedule": ""}, KEY_RATE, REPORT)
    pre = generate(input_deals("prepay")[47], KEY_RATE, REPORT)
    total = lambda ev, t: sum(e["amount"] for e in ev if e["payment_type"] == t)  # noqa: E731
    assert total(pre, "interest_payment") < total(base, "interest_payment")
    # субсидия начисляется на остаток тела — после досрочки она тоже меньше
    assert total(pre, "sub_payment") < total(base, "sub_payment")


def test_shift_applies_only_after_report_date():
    deal = input_deals("shift200")[35]  # пересмотр 2M, последний день месяца: 31.08 = дата расчёта
    base = generate(deal, KEY_RATE, REPORT)
    stress = generate(deal, KEY_RATE, REPORT, Scenario(shift_bp=200))
    # ставка, зафиксированная 31.08, известна — сдвига нет; после следующего пересмотра — +2 п.п.
    assert base[0]["rate"] == stress[0]["rate"]
    rep = [e for e in stress if e["payment_type"] == "repricing"]
    assert rep and all(e["base_rate"] == pytest.approx(0.10) for e in rep)


def test_curve_replaces_h3():
    deal = input_deals("shift200")[22]
    curve = Scenario(curves={"CBKEY1M": [(dt.date(2023, 1, 1), 0.12)]})
    ev = generate(deal, KEY_RATE, REPORT, curve)
    rates = {e["payment_dttm"]: e["base_rate"] for e in ev if e["payment_type"] == "repricing"}
    assert rates[dt.date(2022, 12, 15)] == pytest.approx(0.08)  # до начала кривой — H3
    assert rates[dt.date(2023, 1, 15)] == pytest.approx(0.12)


@pytest.mark.parametrize("name", sorted(SCENARIOS))
def test_scenario_golden_up_to_date(name, tmp_path):
    from cf_reference import write_events

    spec = SCENARIOS[name]
    for i, deal in input_deals(name).items():
        path = tmp_path / f"deal_{i}.csv"
        write_events(path, generate(deal, KEY_RATE, REPORT, spec["scenario"]))
        golden = GOLDEN_DIR / "scenarios" / name / f"deal_{i}.csv"
        assert path.read_text(encoding="utf-8") == golden.read_text(encoding="utf-8"), (name, i)
