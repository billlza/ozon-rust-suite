"""Load + validate fields.yaml into a typed MappingConfig.

Responsibilities:
- parse YAML, apply defaults, reject unknown keys, check version,
- flatten RU/EN/ZH keyword lists into normalized lookups with provenance,
- expose typed accessors, no I/O beyond reading the YAML path.
"""

from __future__ import annotations

import re
import unicodedata
from dataclasses import dataclass, field
from typing import Optional

import yaml

from .errors import ConfigError
from .model import ImageForm

SUPPORTED_VERSION = 1

# --------------------------------------------------------------------------- #
# Normalization (shared by config + header + mapper)
# --------------------------------------------------------------------------- #


def normalize_text(
    value: object,
    *,
    case_insensitive: bool = True,
    strip_required_marker: bool = True,
) -> str:
    """Trim, collapse whitespace, NFKC-normalize, fold ё->е, optionally
    lowercase and drop a trailing required marker (* or ＊)."""
    if value is None:
        return ""
    s = str(value)
    s = unicodedata.normalize("NFKC", s)
    s = s.replace("ё", "е").replace("Ё", "Е")  # ё/Ё -> е/Е
    s = re.sub(r"\s+", " ", s).strip()
    if strip_required_marker:
        s = re.sub(r"[*＊]+\s*$", "", s).strip()
    if case_insensitive:
        s = s.casefold()
    return s


# --------------------------------------------------------------------------- #
# Config dataclasses
# --------------------------------------------------------------------------- #


@dataclass(frozen=True)
class Keyword:
    """A single normalized keyword with provenance for the report."""

    lang: str
    raw: str
    norm: str


@dataclass(frozen=True)
class FieldSpec:
    """Resolution spec for one logical column (title/listing/one image col)."""

    role: str
    keywords: list = field(default_factory=list)  # list[Keyword]
    letter: Optional[str] = None
    index: Optional[int] = None  # 1-based
    required: bool = False
    image_form: Optional[ImageForm] = None  # None == auto
    mode: str = "contains"  # contains | equals | regex
    read_delimiters: list = field(default_factory=lambda: ["\n", ",", " "])
    write_delimiter: str = "\n"


@dataclass(frozen=True)
class SheetSpec:
    name: str
    aliases: list = field(default_factory=list)


@dataclass(frozen=True)
class HeaderSpec:
    scan_rows: int = 3
    match_row: object = "auto"  # "auto" | int
    data_start: object = "auto"  # "auto" | int


@dataclass(frozen=True)
class MatchSpec:
    normalize: bool = True
    case_insensitive: bool = True
    strip_required_marker: bool = True
    mode: str = "contains"


@dataclass(frozen=True)
class PolicySpec:
    on_missing_required: str = "error"  # error | warn
    on_preflight_risk: str = "error"  # error | warn
    freeze_unmapped: bool = True


@dataclass(frozen=True)
class MappingConfig:
    version: int
    sheets: list  # list[SheetSpec]; empty == all visible sheets
    header: HeaderSpec
    match: MatchSpec
    title: Optional[FieldSpec]
    listing: Optional[FieldSpec]
    images_main: list  # list[FieldSpec]
    images_additional: list  # list[FieldSpec]
    key_column: Optional[FieldSpec]
    policy: PolicySpec

    # ------------------------------------------------------------------ #
    def normalizer(self):
        m = self.match
        return lambda v: normalize_text(
            v,
            case_insensitive=m.case_insensitive,
            strip_required_marker=m.strip_required_marker,
        )

    def all_field_specs(self) -> list:
        out: list = []
        if self.title is not None:
            out.append(self.title)
        if self.listing is not None:
            out.append(self.listing)
        out.extend(self.images_main)
        out.extend(self.images_additional)
        if self.key_column is not None:
            out.append(self.key_column)
        return out

    def matches_processed_sheet(self, ws_title: str) -> bool:
        """True if the given worksheet title is named/aliased for processing.
        Empty sheets list means "process all" (caller handles visibility)."""
        if not self.sheets:
            return True
        for s in self.sheets:
            if s.name == "*":
                return True
            if s.name == ws_title or ws_title in s.aliases:
                return True
        return False


# --------------------------------------------------------------------------- #
# Parsing helpers
# --------------------------------------------------------------------------- #

_FORM_BY_NAME = {
    "auto": None,
    "plain_url": ImageForm.PLAIN_URL,
    "hyperlink_formula": ImageForm.HYPERLINK_FORMULA,
    "real_hyperlink": ImageForm.REAL_HYPERLINK,
    "multi_url": ImageForm.MULTI_URL,
}


def _require_dict(value, where: str) -> dict:
    if value is None:
        return {}
    if not isinstance(value, dict):
        raise ConfigError(f"'{where}' must be a mapping, got {type(value).__name__}")
    return value


def _check_unknown(value: dict, allowed: set, where: str) -> None:
    unknown = set(value) - allowed
    if unknown:
        raise ConfigError(
            f"unknown key(s) {sorted(unknown)} in '{where}'; allowed: {sorted(allowed)}"
        )


def _parse_image_form(value, where: str) -> Optional[ImageForm]:
    if value is None:
        return None
    key = str(value).strip().lower()
    if key not in _FORM_BY_NAME:
        raise ConfigError(
            f"'{where}.form' = {value!r} invalid; allowed: {sorted(_FORM_BY_NAME)}"
        )
    return _FORM_BY_NAME[key]


def _flatten_keywords(kw_block, match: MatchSpec) -> list:
    """kw_block is a dict lang->list[str] (or a flat list). Returns Keywords."""
    out: list = []
    if kw_block is None:
        return out

    def add(lang: str, raw: str) -> None:
        norm = normalize_text(
            raw,
            case_insensitive=match.case_insensitive,
            strip_required_marker=match.strip_required_marker,
        )
        if norm:
            out.append(Keyword(lang=lang, raw=str(raw), norm=norm))

    if isinstance(kw_block, dict):
        for lang, vals in kw_block.items():
            if vals is None:
                continue
            if not isinstance(vals, list):
                raise ConfigError(f"keywords.{lang} must be a list")
            for raw in vals:
                add(str(lang), raw)
    elif isinstance(kw_block, list):
        for raw in kw_block:
            add("any", raw)
    else:
        raise ConfigError("keywords must be a mapping lang->list or a flat list")
    return out


def _parse_field(raw: dict, role: str, match: MatchSpec, where: str) -> FieldSpec:
    raw = _require_dict(raw, where)
    _check_unknown(
        raw,
        {"keywords", "letter", "index", "required", "form", "mode", "multi"},
        where,
    )
    letter = raw.get("letter")
    if letter is not None:
        letter = str(letter).strip().upper()
        if not re.fullmatch(r"[A-Z]+", letter):
            raise ConfigError(f"'{where}.letter' = {raw.get('letter')!r} not a column letter")
    index = raw.get("index")
    if index is not None:
        if not isinstance(index, int) or index < 1:
            raise ConfigError(f"'{where}.index' must be a 1-based int")

    multi = _require_dict(raw.get("multi"), f"{where}.multi")
    _check_unknown(multi, {"read_delimiters", "write_delimiter"}, f"{where}.multi")
    read_delims = multi.get("read_delimiters", ["\n", ",", " "])
    if not isinstance(read_delims, list) or not all(isinstance(d, str) for d in read_delims):
        raise ConfigError(f"'{where}.multi.read_delimiters' must be a list of strings")
    write_delim = multi.get("write_delimiter", "\n")
    if not isinstance(write_delim, str):
        raise ConfigError(f"'{where}.multi.write_delimiter' must be a string")

    mode = raw.get("mode", match.mode)
    if mode not in ("contains", "equals", "regex"):
        raise ConfigError(f"'{where}.mode' = {mode!r} invalid")

    return FieldSpec(
        role=role,
        keywords=_flatten_keywords(raw.get("keywords"), match),
        letter=letter,
        index=index,
        required=bool(raw.get("required", False)),
        image_form=_parse_image_form(raw.get("form"), where),
        mode=mode,
        read_delimiters=list(read_delims),
        write_delimiter=write_delim,
    )


# --------------------------------------------------------------------------- #
# Public loaders
# --------------------------------------------------------------------------- #


def load_config(path) -> MappingConfig:
    with open(path, "r", encoding="utf-8") as fh:
        data = yaml.safe_load(fh)
    return parse_config(data)


def parse_config(data) -> MappingConfig:
    if not isinstance(data, dict):
        raise ConfigError("config root must be a mapping")

    _check_unknown(
        data,
        {
            "version",
            "sheets",
            "header",
            "match",
            "columns",
            "key_column",
            "policy",
        },
        "<root>",
    )

    version = data.get("version")
    if version != SUPPORTED_VERSION:
        raise ConfigError(
            f"unsupported config version {version!r}; this build supports {SUPPORTED_VERSION}"
        )

    # sheets
    sheets: list = []
    raw_sheets = data.get("sheets")
    if raw_sheets is not None:
        if not isinstance(raw_sheets, list):
            raise ConfigError("'sheets' must be a list")
        for item in raw_sheets:
            item = _require_dict(item, "sheets[]")
            _check_unknown(item, {"name", "aliases"}, "sheets[]")
            name = item.get("name")
            if not isinstance(name, str) or not name:
                raise ConfigError("each sheet entry needs a non-empty 'name'")
            aliases = item.get("aliases", []) or []
            if not isinstance(aliases, list):
                raise ConfigError("'sheets[].aliases' must be a list")
            sheets.append(SheetSpec(name=name, aliases=[str(a) for a in aliases]))

    # header
    raw_header = _require_dict(data.get("header"), "header")
    _check_unknown(raw_header, {"scan_rows", "match_row", "data_start"}, "header")
    scan_rows = raw_header.get("scan_rows", 3)
    if not isinstance(scan_rows, int) or scan_rows < 1:
        raise ConfigError("'header.scan_rows' must be a positive int")
    header = HeaderSpec(
        scan_rows=scan_rows,
        match_row=_parse_auto_int(raw_header.get("match_row", "auto"), "header.match_row"),
        data_start=_parse_auto_int(raw_header.get("data_start", "auto"), "header.data_start"),
    )

    # match
    raw_match = _require_dict(data.get("match"), "match")
    _check_unknown(
        raw_match,
        {"normalize", "case_insensitive", "strip_required_marker", "mode"},
        "match",
    )
    mode = raw_match.get("mode", "contains")
    if mode not in ("contains", "equals", "regex"):
        raise ConfigError(f"'match.mode' = {mode!r} invalid")
    match = MatchSpec(
        normalize=bool(raw_match.get("normalize", True)),
        case_insensitive=bool(raw_match.get("case_insensitive", True)),
        strip_required_marker=bool(raw_match.get("strip_required_marker", True)),
        mode=mode,
    )

    # columns
    raw_cols = _require_dict(data.get("columns"), "columns")
    _check_unknown(
        raw_cols,
        {"title", "listing", "images_main", "images_additional"},
        "columns",
    )

    title = (
        _parse_field(raw_cols["title"], "title", match, "columns.title")
        if "title" in raw_cols and raw_cols["title"] is not None
        else None
    )
    listing = (
        _parse_field(raw_cols["listing"], "listing", match, "columns.listing")
        if "listing" in raw_cols and raw_cols["listing"] is not None
        else None
    )

    images_main = _parse_image_list(
        raw_cols.get("images_main"), "image_main", match, "columns.images_main"
    )
    images_additional = _parse_image_list(
        raw_cols.get("images_additional"),
        "image_additional",
        match,
        "columns.images_additional",
    )

    # key_column
    key_column = None
    if data.get("key_column") is not None:
        key_column = _parse_field(data["key_column"], "key", match, "key_column")

    # policy
    raw_policy = _require_dict(data.get("policy"), "policy")
    _check_unknown(
        raw_policy,
        {"on_missing_required", "on_preflight_risk", "freeze_unmapped"},
        "policy",
    )
    for k in ("on_missing_required", "on_preflight_risk"):
        v = raw_policy.get(k, "error")
        if v not in ("error", "warn"):
            raise ConfigError(f"'policy.{k}' = {v!r} must be 'error' or 'warn'")
    freeze_unmapped = bool(raw_policy.get("freeze_unmapped", True))
    if not freeze_unmapped:
        raise ConfigError(
            "'policy.freeze_unmapped' must stay true; the preservation contract "
            "requires every unmapped column to be frozen."
        )
    policy = PolicySpec(
        on_missing_required=raw_policy.get("on_missing_required", "error"),
        on_preflight_risk=raw_policy.get("on_preflight_risk", "error"),
        freeze_unmapped=freeze_unmapped,
    )

    return MappingConfig(
        version=version,
        sheets=sheets,
        header=header,
        match=match,
        title=title,
        listing=listing,
        images_main=images_main,
        images_additional=images_additional,
        key_column=key_column,
        policy=policy,
    )


def _parse_auto_int(value, where: str):
    if value is None or (isinstance(value, str) and value.strip().lower() == "auto"):
        return "auto"
    if isinstance(value, int):
        if value < 1:
            raise ConfigError(f"'{where}' int must be >= 1")
        return value
    raise ConfigError(f"'{where}' must be 'auto' or a positive int, got {value!r}")


def _parse_image_list(raw, role: str, match: MatchSpec, where: str) -> list:
    if raw is None:
        return []
    items = raw if isinstance(raw, list) else [raw]
    out: list = []
    for i, item in enumerate(items):
        out.append(_parse_field(item, role, match, f"{where}[{i}]"))
    return out
