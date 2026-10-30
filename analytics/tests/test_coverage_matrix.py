from common import DEALS_CSV
from profile_deals import load_deals
from coverage_matrix import FEATURES


def hits(name):
    test = dict(FEATURES)[name]
    return sorted(int(d["deal_id"]) for d in load_deals(DEALS_CSV) if test(d))


def test_feature_counts():
    assert len(hits("ANN")) == 13 and len(hits("RGM")) == 17 and len(hits("FLOAT")) == 21
    assert hits("депозит (остаток < 0)") == [15, 41, 48]
    assert hits("USD — отказ") == [32]
    assert hits("лаг индикатора") == [51]
