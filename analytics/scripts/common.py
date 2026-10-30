"""Общие пути и помощники для скриптов аналитики."""

from __future__ import annotations

import datetime as dt
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SOURCE_XLSX = ROOT / "testdata" / "source" / "experts_calc.xlsx"
DEALS_CSV = ROOT / "testdata" / "deals" / "test_deals.csv"
KEY_RATE_CSV = ROOT / "testdata" / "rates" / "key_rate.csv"
GOLDEN_DIR = ROOT / "testdata" / "golden"

# 48 колонок формата 1 docs/CONTRACT.md, в том же порядке.
DEAL_COLUMNS = [
    "report_dttm", "discriminator", "deal_id", "currency_id", "start_principal_amt",
    "current_principal_amt", "deal_dttm", "maturity_dttm", "rate_type_cd", "interest_rate",
    "base_indicator_rate", "base_indicator", "interest_payment_rule_start_dttm",
    "interest_payment_rule_period_cd", "interest_payment_rule_day_num", "day_convention_cd",
    "regular_payment_rule_start_dttm", "regular_payment_rule_period_cd",
    "regular_payment_rule_day_num", "repricing_rule_start_dttm", "repricing_rule_period_cd",
    "repricing_rule_day_num", "accrued_interest", "product", "behavior_model_cd",
    "interest_payment", "principal_payment", "repricing_dttm_schedule",
    "repricing_dttm_rate_schedule", "repricing_dttm_rate_schedule_fixed",
    "cap_limit_interest_rate", "floor_limit_interest_rate", "sub_rate_type_cd",
    "sub_interest_rate", "sub_float_indicator", "sub_payment_rule_period_cd",
    "sub_payment_rule_day_num", "sub_payment_rule_start_dttm", "sub_repricing_rule_start_dttm",
    "sub_repricing_rule_period_cd", "sub_repricing_rule_day_num", "sub_cap_limit_interest_rate",
    "sub_floor_limit_interest_rate", "sub_coeff_index", "deal_report_dttm",
    "sub_repricing_rule_first_event_flg", "indicator_lag_cd", "sub_indicator_lag_cd",
]


def parse_date(s: str | None) -> dt.date | None:
    return dt.date.fromisoformat(s) if s else None
