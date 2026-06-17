"""Preflight: detect content openpyxl drops silently on save.

openpyxl raises no exception when it discards embedded images, charts, pivots,
etc. — we must surface their presence *before* writing so the caller can refuse
or accept the loss explicitly.
"""

from __future__ import annotations

from dataclasses import dataclass

from openpyxl import load_workbook


@dataclass(frozen=True)
class Risk:
    sheet: str  # "" for workbook-level
    kind: str  # images | charts | pivots | macros | drawings
    detail: str
    severity: str  # "high" | "medium"


def preflight(path) -> list:
    """Return a list of Risk for lossy content found in the workbook."""
    path = str(path)
    keep_vba = path.lower().endswith(".xlsm")
    wb = load_workbook(path, data_only=False, keep_links=True, keep_vba=keep_vba)
    risks: list = []
    try:
        for ws in wb.worksheets:
            imgs = getattr(ws, "_images", None) or []
            if imgs:
                risks.append(
                    Risk(
                        sheet=ws.title,
                        kind="images",
                        detail=f"{len(imgs)} embedded image(s) WILL BE DROPPED by openpyxl on save",
                        severity="high",
                    )
                )
            charts = getattr(ws, "_charts", None) or []
            if charts:
                risks.append(
                    Risk(
                        sheet=ws.title,
                        kind="charts",
                        detail=f"{len(charts)} chart(s) WILL BE DROPPED by openpyxl on save",
                        severity="high",
                    )
                )
            pivots = getattr(ws, "_pivots", None) or []
            if pivots:
                risks.append(
                    Risk(
                        sheet=ws.title,
                        kind="pivots",
                        detail=f"{len(pivots)} pivot table(s) may break on save",
                        severity="medium",
                    )
                )

        if path.lower().endswith(".xlsm") and not keep_vba:
            risks.append(
                Risk(
                    sheet="",
                    kind="macros",
                    detail=".xlsm workbook loaded without keep_vba; VBA project would be lost",
                    severity="high",
                )
            )
    finally:
        wb.close()
    return risks
