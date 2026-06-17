"""ozon-excel-core: pure-core surgical Ozon .xlsx content rewriter + verifier."""

from __future__ import annotations

__version__ = "0.1.0"

from .config import MappingConfig, load_config, parse_config
from .errors import (
    ConfigError,
    MappingError,
    OzonExcelError,
    PreflightError,
    TransformError,
)
from .model import (
    CellRef,
    ColumnRole,
    HeaderBlock,
    ImageCell,
    ImageForm,
    MappedColumn,
    MappedColumns,
    ProductRow,
)
from .preflight import Risk, preflight
from .transforms import Transform, get_transform
from .verifier import VerifyReport, verify
from .writer import ProcessResult, process

__all__ = [
    "__version__",
    "MappingConfig",
    "load_config",
    "parse_config",
    "OzonExcelError",
    "ConfigError",
    "MappingError",
    "PreflightError",
    "TransformError",
    "CellRef",
    "ColumnRole",
    "HeaderBlock",
    "ImageCell",
    "ImageForm",
    "MappedColumn",
    "MappedColumns",
    "ProductRow",
    "Risk",
    "preflight",
    "Transform",
    "get_transform",
    "VerifyReport",
    "verify",
    "ProcessResult",
    "process",
]
