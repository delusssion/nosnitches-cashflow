from profile_deals import load_deals, profile


def deals_with(code):
    return sorted({f.deal_id for f in profile(load_deals()) if f.code == code})


def test_known_anomalies():
    assert deals_with("SCHEDULE_SUM_MISMATCH") == [20, 44, 54]
    assert deals_with("INTEREST_UNSORTED") == [44, 45, 55]
    assert deals_with("INTEREST_DUP") == [55]
    assert deals_with("INTEREST_ON_REPORT_DATE") == [44, 45, 46]
    assert deals_with("UNSUPPORTED_CURRENCY") == [32]
    assert deals_with("LIABILITY") == [15, 41, 48]
    assert deals_with("START_ZERO") == [13, 22, 23, 24]
    assert deals_with("PRINCIPAL_ENDS_BEFORE_MATURITY") == [54]


def test_deal_20_numbers():
    f = next(f for f in profile(load_deals()) if f.deal_id == 20 and f.code == "SCHEDULE_SUM_MISMATCH")
    assert "1 106 179.00" in f.message and "1 047 077.30" in f.message
