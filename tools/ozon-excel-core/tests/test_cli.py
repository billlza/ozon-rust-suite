"""End-to-end CLI: gen-sample -> process -> verify, with exit codes."""

from __future__ import annotations

import json

from openpyxl import load_workbook

from ozon_excel_core.cli import main


def test_end_to_end(tmp_path, config_path, capsys):
    sample = tmp_path / "sample.xlsx"
    out = tmp_path / "out.xlsx"

    assert main(["gen-sample", "--out", str(sample)]) == 0
    assert sample.exists()

    rc = main(["process", "--in", str(sample), "--out", str(out),
               "--config", config_path, "--transform", "example"])
    assert rc == 0
    assert out.exists()
    capsys.readouterr()  # drop process summary output

    rc = main(["verify", "--in", str(sample), "--out", str(out),
               "--config", config_path, "--report", "json"])
    assert rc == 0
    captured = capsys.readouterr()
    payload = json.loads(captured.out)
    assert payload["ok"] is True
    assert payload["unexpected_changes"] == []
    assert len(payload["expected_changes"]) > 0
    # every expected change is a content role
    for d in payload["expected_changes"]:
        assert d["role"] in {"title", "listing", "image"}


def test_process_with_verify_flag(tmp_path, config_path):
    sample = tmp_path / "sample.xlsx"
    out = tmp_path / "out.xlsx"
    main(["gen-sample", "--out", str(sample)])
    rc = main(["process", "--in", str(sample), "--out", str(out),
               "--config", config_path, "--transform", "example", "--verify", "--quiet"])
    assert rc == 0


def test_in_equals_out_rejected(tmp_path, config_path):
    sample = tmp_path / "sample.xlsx"
    main(["gen-sample", "--out", str(sample)])
    rc = main(["process", "--in", str(sample), "--out", str(sample),
               "--config", config_path])
    assert rc == 2


def test_tampered_verify_nonzero(tmp_path, config_path):
    sample = tmp_path / "sample.xlsx"
    out = tmp_path / "out.xlsx"
    main(["gen-sample", "--out", str(sample)])
    main(["process", "--in", str(sample), "--out", str(out),
          "--config", config_path, "--transform", "example", "--quiet"])
    wb = load_workbook(str(out))
    wb["Шаблон для поставщика"]["G4"] = 0
    wb.save(str(out))
    wb.close()
    rc = main(["verify", "--in", str(sample), "--out", str(out), "--config", config_path])
    assert rc == 1


def test_identity_default_transform(tmp_path, config_path):
    sample = tmp_path / "sample.xlsx"
    out = tmp_path / "out.xlsx"
    main(["gen-sample", "--out", str(sample)])
    # no --transform => identity => zero changes => verify ok
    rc = main(["process", "--in", str(sample), "--out", str(out),
               "--config", config_path, "--quiet"])
    assert rc == 0
    rc = main(["verify", "--in", str(sample), "--out", str(out), "--config", config_path])
    assert rc == 0
