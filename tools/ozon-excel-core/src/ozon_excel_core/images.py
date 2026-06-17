"""Image-cell handling: parse (cell -> ImageCell) and serialize (ImageCell +
new urls -> written value/hyperlink).

Every form is read and rewritten *in the same form*. The idempotency contract
is the regression anchor: if new_urls == original urls and the form is
unchanged, the serialized value/hyperlink equals the original byte-for-byte.
"""

from __future__ import annotations

import re
from typing import Optional

from .model import CellRef, ImageCell, ImageForm

# A URL token. Permissive on purpose (Ozon links vary): http(s), www., mailto.
_URL_RE = re.compile(r"^\s*(https?://|www\.|mailto:)", re.IGNORECASE)
_URL_ANYWHERE_RE = re.compile(r"(https?://|www\.|mailto:)", re.IGNORECASE)

# =HYPERLINK("url") or =HYPERLINK("url","display")  (case-insensitive,
# tolerant of whitespace; quotes may be doubled "" inside the string args).
_HYPERLINK_RE = re.compile(
    r'^\s*=\s*HYPERLINK\s*\(\s*'
    r'"((?:[^"]|"")*)"'  # arg1: link, with "" escapes
    r'(?:\s*,\s*"((?:[^"]|"")*)")?'  # optional arg2: display text
    r'\s*\)\s*$',
    re.IGNORECASE,
)


def _looks_like_url(s: str) -> bool:
    return bool(_URL_RE.match(s))


def _unescape_quotes(s: str) -> str:
    return s.replace('""', '"')


def _escape_quotes(s: str) -> str:
    return s.replace('"', '""')


def _cell_ref(cell) -> CellRef:
    return CellRef(sheet=cell.parent.title, row=cell.row, col=cell.column)


def cell_has_embedded_image(ws, cell) -> bool:
    """True if a raster image in ws._images is anchored to this cell (0-based
    anchor col/row == cell.column-1 / cell.row-1)."""
    images = getattr(ws, "_images", None)
    if not images:
        return False
    target_col0 = cell.column - 1
    target_row0 = cell.row - 1
    for img in images:
        anchor = getattr(img, "anchor", None)
        frm = getattr(anchor, "_from", None) if anchor is not None else None
        if frm is None:
            continue
        if getattr(frm, "col", None) == target_col0 and getattr(frm, "row", None) == target_row0:
            return True
    return False


def parse(cell, field_spec=None, ws=None) -> ImageCell:
    """Classify a cell into an ImageCell. ``field_spec`` (optional) supplies
    read_delimiters and a form hint; ``ws`` (optional) enables embedded-image
    detection."""
    ref = _cell_ref(cell)
    value = cell.value
    read_delims = (
        list(field_spec.read_delimiters)
        if field_spec is not None and field_spec.read_delimiters
        else ["\n", ",", " "]
    )

    # 1. EMBEDDED_IMAGE (flag-only) — checked first so an empty value cell that
    #    actually has a picture anchored is not misread as EMPTY.
    if ws is not None and cell_has_embedded_image(ws, cell):
        return ImageCell(
            ref=ref,
            form=ImageForm.EMBEDDED_IMAGE,
            urls=[],
            display_text=None,
            delimiter=None,
            original_value=value,
        )

    # 2. EMPTY
    is_blank = value is None or (isinstance(value, str) and value.strip() == "")
    if is_blank and cell.hyperlink is None:
        return ImageCell(
            ref=ref,
            form=ImageForm.EMPTY,
            urls=[],
            display_text=None,
            delimiter=None,
            original_value=value,
        )

    # 3. HYPERLINK_FORMULA (string starting with =HYPERLINK and no real hyperlink)
    if isinstance(value, str):
        m = _HYPERLINK_RE.match(value)
        if m:
            url = _unescape_quotes(m.group(1))
            display = _unescape_quotes(m.group(2)) if m.group(2) is not None else None
            return ImageCell(
                ref=ref,
                form=ImageForm.HYPERLINK_FORMULA,
                urls=[url],
                display_text=display,
                delimiter=None,
                original_value=value,
            )

    # 4. REAL_HYPERLINK (cell.hyperlink is not None)
    if cell.hyperlink is not None:
        hl = cell.hyperlink
        target = getattr(hl, "target", None)
        location = getattr(hl, "location", None)
        url = target if target else (f"#{location}" if location else "")
        display = value if isinstance(value, str) else (str(value) if value is not None else None)
        return ImageCell(
            ref=ref,
            form=ImageForm.REAL_HYPERLINK,
            urls=[url],
            display_text=display,
            delimiter=None,
            original_value=value,
        )

    # 5. MULTI_URL — string with a delimiter from read_delims and >= 2 url tokens
    if isinstance(value, str):
        chosen_delim, url_parts, template_parts = _split_multi(value, read_delims)
        if chosen_delim is not None and len(url_parts) >= 2:
            url_positions = [
                i for i, p in enumerate(template_parts) if _looks_like_url(p)
            ]
            return ImageCell(
                ref=ref,
                form=ImageForm.MULTI_URL,
                urls=url_parts,
                display_text=None,
                delimiter=chosen_delim,
                original_value=value,
                parts=template_parts,
                url_positions=url_positions,
            )

    # 6. PLAIN_URL — single string url
    if isinstance(value, str) and _looks_like_url(value):
        return ImageCell(
            ref=ref,
            form=ImageForm.PLAIN_URL,
            urls=[value.strip()],
            display_text=None,
            delimiter=None,
            original_value=value,
        )

    # Fallback: not URL-shaped. Treat as EMPTY-ish frozen (no urls). The writer
    # will not change it. We classify as PLAIN_URL only if it parses as a url;
    # otherwise EMPTY keeps it frozen.
    return ImageCell(
        ref=ref,
        form=ImageForm.EMPTY,
        urls=[],
        display_text=None,
        delimiter=None,
        original_value=value,
    )


def _split_multi(value: str, read_delims: list):
    """Return (delimiter, url_parts, template_parts).

    ``template_parts`` is every non-empty delimited token in order — URLs *and*
    any interleaved non-URL labels (e.g. an Ozon seller pasting
    ``https://1.jpg, см. фото, https://2.jpg``). ``url_parts`` is the URL-only
    projection. ``delimiter`` is the exact original separator when a multi-URL
    split applies, else None.

    A split qualifies as MULTI_URL when the cell genuinely carries >= 2 URL
    tokens separated by ``delim`` (regardless of any non-URL labels between
    them). A space delimiter only counts when it actually separates >= 2 URL
    tokens, so a single ``http://a b`` with a stray space is not a multi."""
    # Count url tokens overall.
    n_urls = len(_URL_ANYWHERE_RE.findall(value))

    for delim in read_delims:
        if delim == " ":
            # Whitespace-run delimiter: split on runs of whitespace.
            if not re.search(r"\s", value):
                continue
            raw_parts = re.split(r"\s+", value.strip())
            parts = [p for p in (rp.strip() for rp in raw_parts) if p]
            url_parts = [p for p in parts if _looks_like_url(p)]
            # For the whitespace delimiter every token must itself be a URL —
            # otherwise a label with internal spaces would be shredded into
            # bogus tokens. Mixed labels are only supported for explicit
            # delimiters (comma / newline).
            if len(url_parts) >= 2 and len(url_parts) == len(parts):
                # Preserve the exact whitespace run actually used.
                run = re.search(r"\s+", value.strip())
                actual = run.group(0) if run else " "
                return actual, url_parts, parts
            continue
        if delim in value:
            raw_parts = value.split(delim)
            parts = [p.strip() for p in raw_parts]
            parts = [p for p in parts if p != ""]
            url_parts = [p for p in parts if _looks_like_url(p)]
            # Multi when the cell genuinely carries >= 2 URL tokens that this
            # delimiter separates. Non-URL labels between the URLs are allowed
            # and preserved verbatim in template_parts; only the URLs are exposed
            # to the transform and substituted back into their slots.
            if len(url_parts) >= 2 and n_urls >= 2:
                return delim, url_parts, parts
    return None, [], []


def serialize(cell, image_cell: ImageCell, new_urls: list, new_display: Optional[str] = None,
              forced_form: Optional[ImageForm] = None) -> bool:
    """Write new_urls back into ``cell`` in the cell's original form (or
    forced_form when set and the original was EMPTY/forced). Returns True if the
    cell value/hyperlink was modified.

    Idempotency: if new_urls equals image_cell.urls and form is unchanged and
    display unchanged, this is a no-op (no write, no byte change)."""
    form = image_cell.form

    # EMBEDDED_IMAGE is never written here.
    if form is ImageForm.EMBEDDED_IMAGE:
        return False

    # EMPTY: only write when a form is forced AND we actually have urls.
    if form is ImageForm.EMPTY:
        if forced_form is None or not new_urls:
            return False
        form = forced_form
        # fall through to write in the forced form

    # --- universal idempotency guard -------------------------------------- #
    # When nothing actually changed (the transform returned the same urls and
    # the display is unchanged), preserve the ORIGINAL cell value byte-for-byte
    # instead of re-emitting a canonicalized string. This keeps surrounding
    # whitespace (PLAIN_URL), internal spacing (HYPERLINK_FORMULA), and the exact
    # delimiter run incl. trailing spaces (MULTI_URL) intact for non-transformed
    # cells. Forced-form writes on EMPTY cells deliberately skip this (the
    # original value is blank and we DO want to write the new url).
    if image_cell.form is not ImageForm.EMPTY:
        display_unchanged = new_display is None or _value_equal(new_display, image_cell.display_text)
        if list(new_urls) == list(image_cell.urls) and display_unchanged:
            return False

    # Determine target display text.
    display = new_display if new_display is not None else image_cell.display_text

    if form is ImageForm.PLAIN_URL:
        new_value = new_urls[0] if new_urls else ""
        if _value_equal(cell.value, new_value) and cell.hyperlink is None:
            return False
        cell.value = new_value
        if cell.hyperlink is not None:
            cell.hyperlink = None
        return True

    if form is ImageForm.HYPERLINK_FORMULA:
        url = new_urls[0] if new_urls else ""
        if display is not None:
            new_value = f'=HYPERLINK("{_escape_quotes(url)}","{_escape_quotes(display)}")'
        else:
            new_value = f'=HYPERLINK("{_escape_quotes(url)}")'
        if _value_equal(cell.value, new_value) and cell.hyperlink is None:
            return False
        cell.value = new_value
        if cell.hyperlink is not None:
            cell.hyperlink = None
        return True

    if form is ImageForm.REAL_HYPERLINK:
        url = new_urls[0] if new_urls else ""
        changed = False
        cur_target = getattr(cell.hyperlink, "target", None) if cell.hyperlink else None
        if cur_target != url:
            # openpyxl: assigning a string to cell.hyperlink creates a Hyperlink
            cell.hyperlink = url
            changed = True
        if new_display is not None and not _value_equal(cell.value, new_display):
            cell.value = new_display
            changed = True
        return changed

    if form is ImageForm.MULTI_URL:
        # Preserve the original delimiter; if none recorded, use write_delimiter.
        delim = image_cell.delimiter
        if delim is None:
            delim = "\n"
        # Substitute the transformed URLs back into their original slots so any
        # interleaved non-URL labels (template_parts) are preserved in order.
        positions = image_cell.url_positions
        template = image_cell.parts
        if positions is not None and template is not None and len(positions) == len(new_urls):
            out_parts = list(template)
            for slot, url in zip(positions, new_urls):
                out_parts[slot] = url
            new_value = delim.join(out_parts)
        else:
            new_value = delim.join(new_urls)
        if _value_equal(cell.value, new_value):
            return False
        cell.value = new_value
        return True

    return False


def _value_equal(a, b) -> bool:
    """Compare two cell values for serialization idempotency."""
    return a == b
