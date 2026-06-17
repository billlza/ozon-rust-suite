"""Identity transform: returns inputs verbatim.

Used as the safe default and as the regression anchor for the no-op test: with
this transform, output must equal input at the cell-signature level (zero
changes, not even a rewrite of target cells).
"""

from __future__ import annotations

from typing import Optional

from . import BaseTransform


class IdentityTransform(BaseTransform):
    name = "identity"

    def transform_title(self, title: Optional[str]) -> Optional[str]:
        return title

    def transform_listing(self, listing: Optional[str]) -> Optional[str]:
        return listing

    def transform_images(self, urls: list) -> list:
        return list(urls)
