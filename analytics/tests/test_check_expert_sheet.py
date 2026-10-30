import pytest

from check_expert_sheet import act_365, act_act_isda, portfolio_refs, read_sheet, schedule


@pytest.fixture(scope="module")
def sheet():
    return read_sheet()


def test_residual_debt_15420(sheet):
    assert sheet.balance_after_last == pytest.approx(15420.45, abs=0.005)
    _, balance = schedule(sheet, act_365, close=False)
    assert balance == pytest.approx(15420.45, abs=0.005)


def test_interest_365_vs_isda(sheet):
    t365, _ = schedule(sheet, act_365, close=True)
    tisda, balance = schedule(sheet, act_act_isda, close=True)
    assert t365 == pytest.approx(5065848.11, abs=0.005)
    assert tisda == pytest.approx(5051623.47, abs=0.005)
    assert t365 - tisda == pytest.approx(14224.63, abs=0.005)
    assert balance == 0


def test_hardcoded_inputs(sheet):
    assert sheet.n == 334 and sheet.formulas["B15"] == 334
    assert all(not str(v).startswith("=") for v in sheet.formulas["B"])


def test_portfolio_ref_errors():
    assert portfolio_refs() == (4, 5)


def test_first_payment_includes_accrued(sheet):
    assert sheet.interest[0] == pytest.approx(3646037.89 * 0.075 * 31 / 365)
