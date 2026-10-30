import csv

from common import DEAL_COLUMNS, DEALS_CSV, KEY_RATE_CSV


def read(path):
    with path.open(encoding="utf-8") as f:
        return list(csv.reader(f))


def test_deals_header_matches_contract():
    rows = read(DEALS_CSV)
    assert rows[0] == DEAL_COLUMNS
    assert len(DEAL_COLUMNS) == 48


def test_deals_count_and_mix():
    rows = read(DEALS_CSV)[1:]
    assert len(rows) == 55
    disc = [r[1] for r in rows]
    assert (disc.count("ANN"), disc.count("RGM"), disc.count("SIMPLE")) == (13, 17, 25)
    rate = [r[8] for r in rows]
    assert (rate.count("FIXED"), rate.count("FLOAT")) == (34, 21)


def test_no_midnight_times():
    text = DEALS_CSV.read_text(encoding="utf-8")
    assert "00:00:00" not in text
    assert not text.startswith("﻿")


def test_key_rate_format():
    rows = read(KEY_RATE_CSV)
    assert rows[0] == ["indicator", "date", "rate"]
    assert all(r[0] == "CBKEY1M" for r in rows[1:])
    dates = [r[1] for r in rows[1:]]
    assert dates == sorted(dates)
    assert ["CBKEY1M", "2020-01-03", "0.0625"] in rows


def test_export_is_reproducible(tmp_path, monkeypatch):
    """Повторная выгрузка из xlsx даёт те же файлы, что лежат в репозитории."""
    import xlsx_to_csv

    deals, rates = tmp_path / "d.csv", tmp_path / "r.csv"
    monkeypatch.setattr(xlsx_to_csv, "DEALS_CSV", deals)
    monkeypatch.setattr(xlsx_to_csv, "KEY_RATE_CSV", rates)
    xlsx_to_csv.main()
    assert deals.read_text(encoding="utf-8") == DEALS_CSV.read_text(encoding="utf-8")
    assert rates.read_text(encoding="utf-8") == KEY_RATE_CSV.read_text(encoding="utf-8")
