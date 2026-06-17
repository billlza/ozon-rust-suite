"""Typed exceptions for ozon-excel-core.

Every failure mode the user can hit during config/mapping/processing has a
dedicated exception type so the CLI can map them to deterministic exit codes
(see cli.py).
"""

from __future__ import annotations


class OzonExcelError(Exception):
    """Base class for all errors raised by this package."""


class ConfigError(OzonExcelError):
    """Raised when the fields.yaml config is malformed, has unknown keys,
    an unsupported version, or contradictory/invalid structural settings
    (e.g. out_path == in_path, data_start <= header rows)."""


class MappingError(OzonExcelError):
    """Raised when columns cannot be resolved unambiguously:
    - a required column is missing (under policy error),
    - a single-valued field matches multiple columns,
    - two roles pin the same letter,
    - the header block cannot be found.
    """


class PreflightError(OzonExcelError):
    """Raised when preflight finds lossy content (embedded images, charts,
    pivots, macros) and policy.on_preflight_risk == 'error'."""

    def __init__(self, message: str, risks=None):
        super().__init__(message)
        self.risks = list(risks or [])


class TransformError(OzonExcelError):
    """Raised when a --transform spec cannot be resolved/imported."""
