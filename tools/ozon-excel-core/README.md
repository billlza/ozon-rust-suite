# ozon-excel-core

Pure-core Python CLI for the Ozon re-listing pipeline. It loads an Ozon product
`.xlsx`, transforms **only** the content fields (title / listing / image links),
keeps every other cell semantically identical, writes a new `.xlsx` in the same
format, and ships a **verifier** that independently proves only the mapped
target columns changed.

No network. No pandas. The whole preservation contract is built on a *surgical*
openpyxl edit: load once, mutate only mapped cells, save to a new path.

---

## Why this exists (the contract)

Re-listing tools that rebuild a sheet cell-by-cell silently destroy styles,
number formats, merges, data validations, hidden sheets, and category-specific
characteristic columns. Ozon templates have **~50 columns**, multi-row headers,
and image links in several different encodings. This tool guarantees:

- **Load once, mutate only mapped target cells, save to a new path.** Never
  rebuild sheets, never copy cell-by-cell, never write the input path.
- **Everything not mapped is frozen** — all header rows, the SKU/key column,
  every physical/logistics column, and every non-processed sheet.
- **Images round-trip in their original form** (plain URL / `=HYPERLINK(...)`
  formula / real hyperlink / multi-URL with its exact delimiter preserved).
  Embedded raster images are detected and escalated, never silently dropped.
- **The verifier independently re-derives the allowed set** from the same config
  and fails (non-zero exit) on any frozen-cell, header-cell, or structural
  change.
- **The no-op (identity) transform is exactly identity at the cell-signature
  level** — zero cells rewritten. That is the regression anchor for everything
  above.

### What is preserved vs only semantically equal

- **Preserved (semantic equality, verified):** every cell value, number format,
  and hyperlink target on untouched cells; merged ranges; column widths; row
  heights; data-validation `sqref` sets; defined names; the sheet set.
- **NOT promised:** byte-for-byte ZIP equality. openpyxl regenerates the XML on
  save, so the output bytes differ even for an identity run. The verifier proves
  **semantic** equivalence, which is the meaningful guarantee.
- **Lossy content (openpyxl drops these silently on save):** embedded images,
  charts, pivot tables, VBA macros (`.xlsm` without `keep_vba`). These are
  caught by **preflight** *before* writing — not by the verifier, because
  openpyxl cannot round-trip them to compare.

---

## Install & run

```bash
cd /Users/bill/ozon-rust-suite/tools/ozon-excel-core
python3 -m venv .venv
.venv/bin/pip install -r requirements.txt
.venv/bin/pip install -e .          # installs the `ozon-excel-core` console script
# dev/test extra:
.venv/bin/pip install pytest
```

End-to-end:

```bash
# 1. build a representative Ozon-style workbook (no proprietary template needed)
.venv/bin/ozon-excel-core gen-sample --out sample.xlsx

# 2. rewrite content columns into a NEW file using the example transforms
.venv/bin/ozon-excel-core process \
    --in sample.xlsx --out out.xlsx \
    --config fields.example.yaml --transform example

# 3. prove only title/listing/image cells changed
.venv/bin/ozon-excel-core verify \
    --in sample.xlsx --out out.xlsx --config fields.example.yaml
```

You can also invoke it as a module: `python -m ozon_excel_core ...`.

### CLI reference

```
ozon-excel-core process  --in IN.xlsx --out OUT.xlsx --config fields.yaml
                         [--transform identity|example|pkg.mod:Factory]
                         [--on-risk error|warn] [--verify]
ozon-excel-core verify   --in IN.xlsx --out OUT.xlsx --config fields.yaml
                         [--report text|json]
ozon-excel-core gen-sample --out sample.xlsx
```

**Exit codes** (designed for CI / pipeline gating):

| code | meaning |
|------|---------|
| 0 | success (`process` wrote OK; `verify` saw only mapped-column changes) |
| 1 | `verify` found unexpected frozen/header/structural changes |
| 2 | config/mapping error, or `--in` == `--out` |
| 3 | preflight risk under `on_preflight_risk: error` |

`process --verify` runs the verifier after writing and adopts its exit code.

---

## Config schema (`fields.example.yaml`)

Anything not mapped is **frozen**. Columns resolve by keyword (RU/EN/ZH) or by an
explicit `letter`/`index` pin. See `fields.example.yaml` for the full annotated
reference. Resolution precedence per field:

1. explicit `index` (1-based) → 2. explicit `letter` → 3. `keywords` against the
header match row.

Single-valued fields (`title`, `listing`) that match multiple columns raise a
`MappingError` listing the candidates (never a silent pick). `images_main` /
`images_additional` may legitimately resolve to multiple columns.

### Multi-row header keys

```yaml
header:
  scan_rows: 3        # inspect rows 1..3 as the header block
  match_row: auto     # "auto" = row with the most keyword hits, or a 1-based int
  data_start: auto    # "auto" = first data row after the header, or a 1-based int
```

The mapper does **not** assume row 1 = header / row 2 = data. The sample
workbook has a 3-row header (human labels / technical keys / hints) with product
data starting at **row 4**, and detection finds that automatically.

### Keyword tables (example)

| field | RU | EN | ZH |
|-------|----|----|----|
| title | Название товара, Наименование | Title, Product name | 商品名称, 标题 |
| listing | Описание, Аннотация | Description, Listing | 商品描述, 描述 |
| images_main | Ссылка на главное фото, Главное фото | Main photo, Primary image | 主图 |
| images_additional | Ссылки на дополнительные фото | Additional photos, Gallery | 附加图片 |
| key (frozen) | Артикул | SKU, Article, Offer ID | 货号 |

Matching is case-insensitive, trimmed, ё→е normalized, whitespace-collapsed, and
treats `Артикул*` == `Артикул` (trailing required marker dropped).

---

## Image-cell forms and how each round-trips

`images.parse(cell)` classifies a cell; `images.serialize(cell, ic, new_urls)`
writes the new URLs back **in the same form**. If `new_urls` equals the original
and the form is unchanged, serialization is a byte-for-byte no-op.

| form | read | write |
|------|------|-------|
| `PLAIN_URL` | single `http(s)://…` string | `cell.value = url` |
| `HYPERLINK_FORMULA` | `=HYPERLINK("u")` / `=HYPERLINK("u","text")` | rebuilt formula, quote-escaping (`"`→`""`) and display text preserved |
| `REAL_HYPERLINK` | `cell.hyperlink.target` | updates the hyperlink target; display text (`cell.value`) untouched unless the transform changes it |
| `MULTI_URL` | several URLs in one cell, split on `\n` / `,` / whitespace | re-joined with the **exact original delimiter** |
| `EMPTY` | blank, no hyperlink, no image | left frozen unless a `form:` is forced |
| `EMBEDDED_IMAGE` | a raster anchored to the cell | **flagged + skipped**; escalated by preflight, never rewritten |

---

## Transform plug-in API

A transform is a pure mapping of content fields (no I/O, no global state,
deterministic). The "real" rehosting/rewriting modules live elsewhere; this
package only defines the seam.

```python
from typing import Optional, Protocol

class Transform(Protocol):
    name: str
    def transform_title(self, title: Optional[str]) -> Optional[str]: ...
    def transform_listing(self, listing: Optional[str]) -> Optional[str]: ...
    def transform_images(self, urls: list) -> list: ...
```

- `None`/empty inputs pass through unless the transform populates them.
- `transform_images` receives the decoded URL list for a cell and returns a
  same-or-different-length list; the writer re-serializes in the cell's original
  form (so order and delimiter are preserved).

### Selecting a transform

```bash
--transform identity                       # default: verbatim passthrough
--transform example                        # the shipped ExampleAllTransform
--transform my_pkg.rewriters:CdnRehost     # any dotted path -> Class or factory
```

Subclass `ozon_excel_core.transforms.BaseTransform` (identity by default) and
override only the methods you need, or implement the `Transform` protocol
directly. External modules register purely by import path — no core internals to
touch.

Shipped examples (`transforms/example.py`), all deterministic and network-free:

- `ExampleTitleTransform` — trims, collapses whitespace, title-cases the first
  segment, appends a marketing suffix, clamps to Ozon's 200-char title cap on a
  word boundary.
- `ExampleListingTransform` — normalizes line breaks, prepends a lead sentence,
  clamps length.
- `ExampleImageTransform` — placeholder CDN rehost: rewrites each URL's host to a
  fixed prefix, preserving path and order. No network.
- `ExampleAllTransform` — composes all three.

---

## How the verifier proves the guarantee

`verify(in, out, config)`:

1. Re-resolves the mapped columns from the **same config** (a real check, not a
   replay of what the writer did).
2. Walks the union of used cells across every sheet and compares the signature
   `(value, number_format, hyperlink_key)` of each cell.
3. Partitions diffs into **expected** (title/listing/image target cells in data
   rows) and **unexpected** (any frozen cell, header cell, or structural diff).
4. Also compares merged ranges, column widths, row heights, data-validation
   `sqref` sets, defined names, and the sheet set.
5. `ok` is true only when there are zero unexpected changes. The CLI exits
   non-zero otherwise — this is the gate other pipeline stages call.

Sample report (`example` transform on the generated workbook): 25 expected
changes (5 rows × {title, listing, main photo, 2 additional-photo columns}),
**0 unexpected**, ~235 frozen cells compared identical across 3 sheets.

---

## Limits (explicit)

- Embedded images, charts, pivot tables, and macros are **escalated by
  preflight**, not silently handled. Under `on_preflight_risk: error` (the
  default) the run aborts with exit 3; under `warn` it continues and image cells
  backed by an embedded raster are left frozen.
- No byte-for-byte ZIP equality is promised (openpyxl regenerates XML).
- This is the **pure core**: network rehosting, scraping, and the real
  title/listing rewriters plug in via the transform seam; they are out of scope
  here.

---

## File layout

```
ozon-excel-core/
├── pyproject.toml / requirements.txt / fields.example.yaml / README.md
├── src/ozon_excel_core/
│   ├── cli.py        config.py   model.py    header.py    mapper.py
│   ├── images.py     extract.py  writer.py   verifier.py  preflight.py
│   ├── sample.py     errors.py   __init__.py __main__.py
│   └── transforms/   __init__.py identity.py example.py
└── tests/            (pytest; run `.venv/bin/python -m pytest`)
```
