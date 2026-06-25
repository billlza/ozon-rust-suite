"""extract subcommand: read-only JSON projection of product rows to stdout."""

from __future__ import annotations

import json

from ozon_excel_core.cli import main


def test_extract_emits_json_rows(tmp_path, config_path, capsys):
    sample = tmp_path / "sample.xlsx"
    assert main(["gen-sample", "--out", str(sample)]) == 0
    capsys.readouterr()  # drop gen-sample's stdout

    rc = main(["extract", "--in", str(sample), "--config", config_path])
    assert rc == 0

    out = capsys.readouterr().out
    payload = json.loads(out)
    assert "rows" in payload
    rows = payload["rows"]
    assert isinstance(rows, list)
    assert len(rows) >= 1

    first = rows[0]
    # Every projected key is present and shaped as documented.
    for key in ("sheet", "row", "sku", "title", "listing",
                "images_main", "images_additional"):
        assert key in first
    assert isinstance(first["row"], int)
    assert isinstance(first["images_main"], list)
    assert isinstance(first["images_additional"], list)
    # Flattened image URLs are plain strings.
    for url in first["images_main"] + first["images_additional"]:
        assert isinstance(url, str)


def test_extract_sheet_filter_excludes_other_sheets(tmp_path, config_path, capsys):
    sample = tmp_path / "sample.xlsx"
    assert main(["gen-sample", "--out", str(sample)]) == 0
    capsys.readouterr()  # drop gen-sample's stdout

    rc = main(["extract", "--in", str(sample), "--config", config_path,
               "--sheet", "no-such-sheet"])
    assert rc == 0
    payload = json.loads(capsys.readouterr().out)
    assert payload["rows"] == []


def test_extract_bad_config_returns_2(tmp_path, capsys):
    sample = tmp_path / "sample.xlsx"
    assert main(["gen-sample", "--out", str(sample)]) == 0

    # A present-but-invalid config maps to the 0/2/3 contract's exit 2.
    bad_config = tmp_path / "bad.yaml"
    bad_config.write_text("- this is a list not a mapping\n", encoding="utf-8")
    rc = main(["extract", "--in", str(sample), "--config", str(bad_config)])
    assert rc == 2
