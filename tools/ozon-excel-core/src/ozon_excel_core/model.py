"""Data model: dataclasses + enums describing resolved columns, header blocks,
extracted product rows, and decoded image cells.

Pure descriptors are ``frozen``. Nothing here does I/O.
"""

from __future__ import annotations

import enum
from dataclasses import dataclass, field
from typing import Optional

from openpyxl.utils import get_column_letter


class ColumnRole(enum.Enum):
    TITLE = "title"
    LISTING = "listing"
    IMAGE_MAIN = "image_main"
    IMAGE_ADDITIONAL = "image_additional"
    FROZEN = "frozen"


class ImageForm(enum.Enum):
    PLAIN_URL = "plain_url"
    HYPERLINK_FORMULA = "hyperlink_formula"
    REAL_HYPERLINK = "real_hyperlink"
    MULTI_URL = "multi_url"
    EMBEDDED_IMAGE = "embedded_image"
    EMPTY = "empty"


@dataclass(frozen=True)
class CellRef:
    """An addressable cell. Derived attributes (coordinate, column_letter) are
    computed in __post_init__ since the dataclass is frozen."""

    sheet: str
    row: int  # 1-based
    col: int  # 1-based
    coordinate: str = field(default="", compare=False)
    column_letter: str = field(default="", compare=False)

    def __post_init__(self) -> None:
        letter = get_column_letter(self.col)
        object.__setattr__(self, "column_letter", letter)
        object.__setattr__(self, "coordinate", f"{letter}{self.row}")


@dataclass(frozen=True)
class HeaderBlock:
    """Multi-row header description for a single sheet."""

    header_rows: list  # list[int], 1-based
    match_row: int  # row the mapper matched against
    data_start_row: int  # first product-data row


@dataclass(frozen=True)
class MappedColumn:
    """One resolved target column."""

    role: ColumnRole
    sheet: str
    col_index: int  # 1-based
    column_letter: str
    matched_by: str  # "keyword" | "letter" | "index"
    matched_token: Optional[str] = None
    image_form_hint: Optional[ImageForm] = None


@dataclass(frozen=True)
class MappedColumns:
    """Full resolution result for a single sheet."""

    sheet: str
    header_block: HeaderBlock
    title: Optional[MappedColumn]
    listing: Optional[MappedColumn]
    images_main: list  # list[MappedColumn]
    images_additional: list  # list[MappedColumn]
    target_col_indices: frozenset  # frozenset[int]
    key_column: Optional[MappedColumn] = None

    def is_target(self, col_index: int) -> bool:
        return col_index in self.target_col_indices

    def all_targets(self) -> list:
        out: list = []
        if self.title is not None:
            out.append(self.title)
        if self.listing is not None:
            out.append(self.listing)
        out.extend(self.images_main)
        out.extend(self.images_additional)
        return out


@dataclass(frozen=True)
class ImageCell:
    """One image-bearing cell, decoded.

    For MULTI_URL cells, ``parts`` records every delimited token in order
    (URLs *and* any interleaved non-URL labels), and ``url_positions`` lists the
    indices within ``parts`` that are URLs. ``urls`` is the URL-only projection
    fed to transforms; on serialize the transformed URLs are mapped back into
    their original slots so interleaved labels are preserved. Both are ``None``
    for non-MULTI_URL forms.
    """

    ref: CellRef
    form: ImageForm
    urls: list  # list[str]
    display_text: Optional[str] = None
    delimiter: Optional[str] = None
    original_value: object = None
    parts: Optional[list] = None  # list[str] full ordered tokens (MULTI_URL)
    url_positions: Optional[list] = None  # list[int] indices in parts that are urls


@dataclass
class ProductRow:
    """Content-only read-side view of one product row. The writer does NOT
    rebuild rows from this; it is for diffing/logging/extraction only."""

    sheet: str
    row: int
    sku: Optional[str] = None
    title: Optional[str] = None
    listing: Optional[str] = None
    images_main: list = field(default_factory=list)  # list[ImageCell]
    images_additional: list = field(default_factory=list)  # list[ImageCell]
    raw_targets: dict = field(default_factory=dict)  # col_index -> original value
