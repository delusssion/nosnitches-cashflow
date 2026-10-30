import datetime as dt

import pytest

from cf_reference import DealError, generate, generate_with_warnings, load_deals, load_key_rate
from common import DEALS_CSV, GOLDEN_DIR, KEY_RATE_CSV
from make_golden import GOLDEN_DEALS

KEY_RATE = load_key_rate(KEY_RATE_CSV)
DEALS = {int(d["deal_id"]): d for d in load_deals(DEALS_CSV)}
RUB_DEALS = [i for i, d in DEALS.items() if d["currency_id"] == "RUB"]
REPORT = dt.date(2022, 8, 31)
BODY = ("principal_payment", "close_payment")
ORDER = ["repricing", "sub_repricing", "interest_payment", "sub_payment", "principal_payment", "close_payment"]


def run(i):
    return generate(DEALS[i], KEY_RATE, REPORT)


@pytest.mark.parametrize("i", RUB_DEALS)
def test_invariants(i):
    events = run(i)
    balance = float(DEALS[i]["current_principal_amt"])
    body = [e for e in events if e["payment_type"] in BODY]
    # остаток после последнего события тела = 0, последнее событие тела — close_payment
    assert body[-1]["payment_type"] == "close_payment"
    assert body[-1]["principal_after"] == 0
    assert [e["payment_type"] for e in body].count("close_payment") == 1
    # сумма тела = остатку на дату расчёта
    assert sum(e["amount"] for e in body) == pytest.approx(balance, abs=1e-6)
    # нет событий в дату расчёта и раньше (H6), нет событий после погашения
    maturity = dt.date.fromisoformat(DEALS[i]["maturity_dttm"])
    assert all(REPORT < e["payment_dttm"] <= maturity for e in events)
    # порядок: дата, затем тип события в день; event_order с 1 подряд
    keys = [(e["payment_dttm"], ORDER.index(e["payment_type"].replace("sub_repricing", "repricing"))) for e in events]
    assert keys == sorted(keys)
    assert [e["event_order"] for e in events] == list(range(1, len(events) + 1))
    # суммы со знаком сделки, остаток монотонно идёт к нулю
    sign = 1 if balance > 0 else -1
    assert all(e["amount"] * sign >= -1e-9 for e in events)
    afters = [abs(e["principal_after"]) for e in events]
    assert afters == sorted(afters, reverse=True)


def test_usd_rejected():
    with pytest.raises(DealError) as e:
        run(32)
    assert e.value.code == "CURRENCY_NOT_RUB"


def test_deal_1_control_isda():
    events = run(1)
    interest = sum(e["amount"] for e in events if e["payment_type"] == "interest_payment")
    assert interest == pytest.approx(5051623.47, abs=0.005)
    first = events[0]
    assert (first["accrual_start_dttm"], first["accrual_days"]) == (dt.date(2022, 8, 22), 31)
    assert first["amount"] == pytest.approx(23224.76, abs=0.005)
    assert len([e for e in events if e["payment_type"] in BODY]) == 334


def test_rgm_equal_shares():
    events = run(2)
    body = [e["amount"] for e in events if e["payment_type"] == "principal_payment"]
    assert body[0] == pytest.approx(3646037.89 / 334)
    assert max(body) - min(body) < 1e-6


def test_float_cap_floor():
    assert {round(e["rate"], 10) for e in run(25)} == {0.1}  # 8 % + 5 % = 13 % → cap 10 %
    assert {round(e["rate"], 10) for e in run(22)} == {0.19}  # 8 % + 11 %, cap 1


def test_period_45d_and_last_day():
    dates = [e["payment_dttm"].isoformat() for e in run(30) if e["payment_type"] == "interest_payment"]
    assert dates == ["2022-09-15", "2022-10-30", "2022-12-14", "2023-01-01"]
    rep = [e["payment_dttm"].isoformat() for e in run(35) if e["payment_type"] == "repricing"]
    assert rep == ["2022-10-31", "2022-12-31"]  # 2M, day_num -1 от 2022-06-30; 2022-08-31 = дата расчёта


def test_report_date_event_is_past():
    events = run(44)
    assert events[0]["accrual_start_dttm"] == REPORT
    assert events[0]["amount"] == pytest.approx(224204.19 * 0.09 * 30 / 365, abs=0.005)


def test_accrued_interest_field():
    first = run(36)[0]
    assert first["amount"] == pytest.approx(1000 + 500000 * 0.13 * 123 / 365, abs=0.005)


def test_subsidy():
    sub = [e for e in run(47) if e["stream"] == "sub" and e["payment_type"] == "sub_payment"]
    assert {round(e["rate"], 10) for e in sub} == {0.155}
    sub53 = [e for e in run(53) if e["payment_type"] == "sub_payment"]
    assert {round(e["rate"], 10) for e in sub53} == {round(0.9 * 0.08 + 0.031, 10)}


def test_schedule_mismatch_warning():
    _, warnings = generate_with_warnings(DEALS[20], KEY_RATE, REPORT)
    assert [w[0] for w in warnings] == ["PRINCIPAL_SUM_MISMATCH"]
    events = run(54)
    close = [e for e in events if e["payment_type"] == "close_payment"][0]
    assert close["payment_dttm"] == dt.date(2029, 10, 11)
    assert close["amount"] == pytest.approx(9020721.21 - 850085.26, abs=0.005)


def test_deposit_signs():
    events = run(15)
    assert all(e["amount"] <= 0 for e in events)


@pytest.mark.parametrize("i", sorted(GOLDEN_DEALS))
def test_golden_files_up_to_date(i, tmp_path):
    from cf_reference import write_events

    path = tmp_path / f"deal_{i}.csv"
    write_events(path, run(i))
    assert path.read_text(encoding="utf-8") == (GOLDEN_DIR / f"deal_{i}.csv").read_text(encoding="utf-8")
