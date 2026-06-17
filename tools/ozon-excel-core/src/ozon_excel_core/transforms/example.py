"""Realistic but self-contained example transforms.

All are pure, deterministic, and network-free. They demonstrate the seam the
"real" rewriters plug into.
"""

from __future__ import annotations

import re
from typing import Optional

from . import BaseTransform

TITLE_SUFFIX = " — Оригинал"
TITLE_MAX = 200
LISTING_MAX = 4000
LISTING_LEAD = "Описание товара: "
# A placeholder CDN host the example image transform rehosts to (pure rule).
CDN_PREFIX = "https://cdn.example-ozon-mirror.test/"


def _collapse_ws(s: str) -> str:
    return re.sub(r"\s+", " ", s).strip()


def _truncate_word_boundary(s: str, limit: int) -> str:
    if len(s) <= limit:
        return s
    cut = s[:limit]
    if " " in cut:
        cut = cut[: cut.rfind(" ")]
    return cut.rstrip()


class ExampleTitleTransform(BaseTransform):
    """Trim, collapse whitespace, title-case the first segment, append a fixed
    marketing suffix, enforce Ozon's title length cap on a word boundary."""

    name = "example-title"

    def transform_title(self, title: Optional[str]) -> Optional[str]:
        if title is None:
            return None
        s = _collapse_ws(str(title))
        if not s:
            return title
        # Title-case only the first segment (up to first comma), Unicode-aware.
        head, sep, tail = s.partition(",")
        head = head[:1].upper() + head[1:] if head else head
        s = head + sep + tail
        # Reserve room for the suffix within the cap.
        budget = TITLE_MAX - len(TITLE_SUFFIX)
        s = _truncate_word_boundary(s, max(0, budget))
        return s + TITLE_SUFFIX


class ExampleListingTransform(BaseTransform):
    """Strip boilerplate, normalize line breaks, prepend a fixed lead sentence,
    clamp length."""

    name = "example-listing"

    def transform_listing(self, listing: Optional[str]) -> Optional[str]:
        if listing is None:
            return None
        s = str(listing).replace("\r\n", "\n").replace("\r", "\n")
        s = re.sub(r"[ \t]+", " ", s)
        s = re.sub(r"\n{3,}", "\n\n", s).strip()
        if not s:
            return listing
        s = LISTING_LEAD + s
        return _truncate_word_boundary(s, LISTING_MAX)


class ExampleImageTransform(BaseTransform):
    """Placeholder-rehost passthrough: rewrite each URL's host to a configured
    CDN prefix, preserving the path. Order preserved, list-in/list-out, no
    network. Non-http inputs pass through unchanged."""

    name = "example-image"

    def transform_images(self, urls: list) -> list:
        out = []
        for u in urls:
            out.append(self._rehost(u))
        return out

    @staticmethod
    def _rehost(url: str) -> str:
        m = re.match(r"^https?://([^/]+)(/.*)?$", url, re.IGNORECASE)
        if not m:
            return url  # not a plain http(s) url; leave untouched
        path = m.group(2) or "/"
        return CDN_PREFIX.rstrip("/") + path


class ExampleAllTransform(BaseTransform):
    """Composes all three example transforms. Used by test_transform.py."""

    name = "example"

    def __init__(self):
        self._title = ExampleTitleTransform()
        self._listing = ExampleListingTransform()
        self._image = ExampleImageTransform()

    def transform_title(self, title: Optional[str]) -> Optional[str]:
        return self._title.transform_title(title)

    def transform_listing(self, listing: Optional[str]) -> Optional[str]:
        return self._listing.transform_listing(listing)

    def transform_images(self, urls: list) -> list:
        return self._image.transform_images(urls)
