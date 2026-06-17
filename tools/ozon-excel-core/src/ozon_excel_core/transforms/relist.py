"""RelistTransform: the real (network-backed) re-listing transform.

It plugs into the same ``Transform`` seam every other transform uses, so the
surgical writer/verifier guarantees are unchanged — only the *content* of
mapped cells differs. Concretely:

  - transform_images(urls): for each http(s) URL, download the source image,
    run it through GPT Image 2 image-edit (a clean white-studio restyle), host
    the result, and return the new public URL. Results are cached on disk keyed
    by sha256(model + prompt + source-url) so re-running (or a second column
    pointing at the same image) does NOT re-spend on generation. Non-URL values
    pass through unchanged.
  - transform_title / transform_listing: currently reuse the deterministic
    Example* rewriters (network-free). SEAM: swap these for a real GPT-text
    rewrite later — see the TODO below.

Image generation and hosting are INJECTED (``imagegen_fn`` / ``host``) so tests
exercise the full mapping/caching logic with fakes — no network, no spend. The
``make_relist()`` factory wires the real ``gptimage_edit`` + ``get_host()`` from
environment variables, so the CLI can resolve
``ozon_excel_core.transforms.relist:make_relist``.
"""

from __future__ import annotations

import hashlib
import os
import re
import sys
from pathlib import Path
from typing import Callable, Optional

import requests

from . import BaseTransform
from .example import ExampleListingTransform, ExampleTitleTransform

_URL_RE = re.compile(r"^\s*https?://", re.IGNORECASE)

# A clean white-studio e-commerce restyle: keep the exact product, remove the
# original background and any overlay clutter (text/badges/watermarks), square.
DEFAULT_PROMPT = (
    "Create a clean professional e-commerce MAIN listing image. Show ONLY a single "
    "primary product: pick the one most prominent item from this image and present "
    "just that one, centered and filling most of the frame. Remove any duplicate "
    "items, colour variants, extra angles, thumbnails or collage panels, and remove "
    "all overlay text, badges, logos, watermarks and borders. Pure seamless white "
    "studio background, soft even lighting, subtle floor reflection, sharp focus, "
    "square 1:1. Keep the chosen product's exact shape, colours, materials and any "
    "text printed on the product itself."
)

# Russian-market (Ozon) rewrite instructions for the title / listing GPT-text pass.
TITLE_INSTRUCTION = (
    "Ты — копирайтер карточек товаров маркетплейса Ozon (русский рынок). Перепиши "
    "НАЗВАНИЕ товара на русском языке: сохрани все фактические атрибуты (бренд, "
    "модель, масштаб, цвет, размер, материал), сделай его понятным и SEO-дружелюбным "
    "с релевантными ключевыми словами, естественным, без эмодзи и кавычек, не длиннее "
    "200 символов. Верни ТОЛЬКО переписанное название, без пояснений."
)
LISTING_INSTRUCTION = (
    "Ты — копирайтер Ozon. Перепиши ОПИСАНИЕ товара на русском языке: сохрани все "
    "факты, улучши структуру и читабельность, по делу, без ложных обещаний и воды, "
    "не длиннее 1800 символов. Верни ТОЛЬКО переписанное описание, без пояснений."
)
TITLE_MAX = 200
LISTING_MAX = 1800


def _is_url(value) -> bool:
    return isinstance(value, str) and bool(_URL_RE.match(value))


class RelistTransform(BaseTransform):
    """GPT-Image-backed image restyle + (placeholder) text rewrite.

    Constructor takes injectable ``imagegen_fn`` and ``host`` (for tests) plus
    the image-edit settings and an on-disk ``cache_dir``.
    """

    name = "relist"

    def __init__(
        self,
        *,
        imagegen_fn: Callable[..., bytes],
        host,
        image_api_base: str,
        image_api_key: str,
        image_model: str = "gpt-image-2-vip",
        prompt: str = DEFAULT_PROMPT,
        size: str = "1024x1024",
        cache_dir: Optional[str] = None,
        textgen_fn: Optional[Callable[..., str]] = None,
        text_api_base: Optional[str] = None,
        text_api_key: Optional[str] = None,
        text_model: Optional[str] = None,
        title_instruction: str = TITLE_INSTRUCTION,
        listing_instruction: str = LISTING_INSTRUCTION,
    ):
        self._imagegen_fn = imagegen_fn
        self._host = host
        self.image_api_base = image_api_base
        self.image_api_key = image_api_key
        self.image_model = image_model
        self.prompt = prompt
        self.size = size
        self.cache_dir = Path(cache_dir) if cache_dir else None
        if self.cache_dir is not None:
            self.cache_dir.mkdir(parents=True, exist_ok=True)

        # Real GPT-text rewrite when a textgen_fn + key are wired; otherwise fall
        # back to the deterministic Example rewriters (network-free). Same method
        # signatures either way, so the writer/verifier guarantees are unchanged.
        self._textgen_fn = textgen_fn
        self.text_api_base = text_api_base or image_api_base
        self.text_api_key = text_api_key or image_api_key
        self.text_model = text_model
        self.title_instruction = title_instruction
        self.listing_instruction = listing_instruction
        self._title = ExampleTitleTransform()
        self._listing = ExampleListingTransform()

    # --- text (real GPT rewrite, with deterministic fallback) ------------- #
    def _rewrite_text(self, text, instruction, max_len, fallback):
        if text is None:
            return None
        if not self._textgen_fn or not self.text_api_key:
            return fallback(text)
        try:
            kwargs = dict(
                instruction=instruction,
                api_base=self.text_api_base,
                api_key=self.text_api_key,
            )
            if self.text_model:
                kwargs["model"] = self.text_model
            out = self._textgen_fn(str(text), **kwargs)
            out = (out or "").strip()
            if not out:
                return fallback(text)
            if len(out) > max_len:  # hard cap (Ozon limits); cut on a word boundary
                cut = out[:max_len]
                if " " in cut:
                    cut = cut[: cut.rfind(" ")]
                out = cut.rstrip()
            return out
        except Exception as exc:  # noqa: BLE001 - resilience: never sink a run on text
            sys.stderr.write(
                f"[relist] WARN: text rewrite fell back ({type(exc).__name__}: {exc})\n"
            )
            return fallback(text)

    def transform_title(self, title: Optional[str]) -> Optional[str]:
        return self._rewrite_text(
            title, self.title_instruction, TITLE_MAX, self._title.transform_title
        )

    def transform_listing(self, listing: Optional[str]) -> Optional[str]:
        return self._rewrite_text(
            listing, self.listing_instruction, LISTING_MAX, self._listing.transform_listing
        )

    # --- images ----------------------------------------------------------- #
    def transform_images(self, urls: list) -> list:
        out = []
        for u in urls:
            if _is_url(u):
                out.append(self._restyle_one(u.strip()))
            else:
                out.append(u)  # non-url values pass through untouched
        return out

    def _cache_key(self, url: str) -> str:
        h = hashlib.sha256()
        h.update(self.image_model.encode("utf-8"))
        h.update(b"\0")
        h.update(self.prompt.encode("utf-8"))
        h.update(b"\0")
        h.update(url.encode("utf-8"))
        return h.hexdigest()

    def _cache_path(self, key: str) -> Optional[Path]:
        if self.cache_dir is None:
            return None
        return self.cache_dir / f"{key}.url"

    def _restyle_one(self, url: str) -> str:
        key = self._cache_key(url)

        # 1. cache lookup (key = sha256 of model+prompt+url).
        cache_path = self._cache_path(key)
        if cache_path is not None and cache_path.exists():
            cached = cache_path.read_text(encoding="utf-8").strip()
            if cached:
                return cached

        # 2-5: download -> generate -> host -> cache. A failure on a single image
        # must NOT abort the whole batch: log and keep the ORIGINAL url so the cell
        # stays a valid (old) image rather than crashing the run. (Production-grade:
        # real Ozon CDN urls occasionally hiccup; one bad image shouldn't sink a sheet.)
        try:
            src = requests.get(url, timeout=120)
            if getattr(src, "status_code", 200) != 200:
                raise RuntimeError(f"source image returned HTTP {src.status_code}")

            new_bytes = self._imagegen_fn(
                src.content,
                self.prompt,
                api_base=self.image_api_base,
                api_key=self.image_api_key,
                model=self.image_model,
                size=self.size,
            )

            filename = f"relist_{key[:16]}.png"
            new_url = self._host.put(filename, new_bytes)

            if cache_path is not None:
                cache_path.write_text(new_url, encoding="utf-8")
            return new_url
        except Exception as exc:  # noqa: BLE001 - resilience is the whole point
            sys.stderr.write(
                f"[relist] WARN: kept original image {url!r} "
                f"({type(exc).__name__}: {exc})\n"
            )
            return url


def make_relist() -> RelistTransform:
    """Factory wired to the real gptimage_edit + get_host() from environment.

    Reads:
      OZON_RELIST_IMAGE_API_BASE  (default https://api.apiyi.com)
      OZON_RELIST_IMAGE_API_KEY
      OZON_RELIST_IMAGE_MODEL     (default gpt-image-2-vip)
      OZON_RELIST_HOST            (default litterbox; via get_host())
      OZON_RELIST_CACHE           (default ./relist_cache)
      OZON_RELIST_PROMPT          (default DEFAULT_PROMPT)

    So ``--transform ozon_excel_core.transforms.relist:make_relist`` works.
    """
    from ..hosting import get_host
    from ..imagegen import DEFAULT_API_BASE, DEFAULT_MODEL, gptimage_edit
    from ..textgen import DEFAULT_TEXT_MODEL, chat_rewrite

    api_base = os.environ.get("OZON_RELIST_IMAGE_API_BASE") or DEFAULT_API_BASE
    api_key = os.environ.get("OZON_RELIST_IMAGE_API_KEY", "")
    model = os.environ.get("OZON_RELIST_IMAGE_MODEL") or DEFAULT_MODEL
    cache_dir = os.environ.get("OZON_RELIST_CACHE") or "relist_cache"
    prompt = os.environ.get("OZON_RELIST_PROMPT") or DEFAULT_PROMPT

    # Text rewrite (title/listing): on by default unless OZON_RELIST_TEXT=off.
    # Reuses the same apiyi key/base as images; model defaults to a cheap chat model.
    text_on = os.environ.get("OZON_RELIST_TEXT", "on").lower() not in ("off", "0", "false", "no")
    text_api_base = os.environ.get("OZON_RELIST_TEXT_API_BASE") or api_base
    text_api_key = os.environ.get("OZON_RELIST_TEXT_API_KEY") or api_key
    text_model = os.environ.get("OZON_RELIST_TEXT_MODEL") or DEFAULT_TEXT_MODEL

    return RelistTransform(
        imagegen_fn=gptimage_edit,
        host=get_host(),  # reads OZON_RELIST_HOST internally
        image_api_base=api_base,
        image_api_key=api_key,
        image_model=model,
        prompt=prompt,
        cache_dir=cache_dir,
        textgen_fn=(chat_rewrite if text_on else None),
        text_api_base=text_api_base,
        text_api_key=text_api_key,
        text_model=text_model,
    )
