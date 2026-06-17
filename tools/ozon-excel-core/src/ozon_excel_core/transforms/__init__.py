"""Transform protocol + registry/loader.

A Transform is a pure mapping of content fields. External "real" rewriters can
register by dotted path (``pkg.module:ClassOrFactory``) without importing core
internals — this package only defines the seam.
"""

from __future__ import annotations

import importlib
from typing import Optional, Protocol, runtime_checkable

from ..errors import TransformError


@runtime_checkable
class Transform(Protocol):
    name: str

    def transform_title(self, title: Optional[str]) -> Optional[str]: ...

    def transform_listing(self, listing: Optional[str]) -> Optional[str]: ...

    def transform_images(self, urls: list) -> list: ...


class BaseTransform:
    """Convenience ABC-like base: identity by default; override what you need."""

    name = "base"

    def transform_title(self, title: Optional[str]) -> Optional[str]:
        return title

    def transform_listing(self, listing: Optional[str]) -> Optional[str]:
        return listing

    def transform_images(self, urls: list) -> list:
        return list(urls)


def get_transform(spec: Optional[str]):
    """Resolve a transform spec to a Transform instance.

    spec:
      - None or "identity" -> IdentityTransform
      - "example"          -> ExampleAllTransform
      - "pkg.module:Name"  -> import and instantiate (or call if a factory)
    """
    if spec is None or spec == "identity":
        from .identity import IdentityTransform

        return IdentityTransform()

    if spec == "example":
        from .example import ExampleAllTransform

        return ExampleAllTransform()

    if ":" not in spec:
        raise TransformError(
            f"unknown transform {spec!r}; use 'identity', 'example', or "
            f"'package.module:ClassOrFactory'"
        )

    module_path, attr = spec.split(":", 1)
    try:
        module = importlib.import_module(module_path)
    except ImportError as exc:
        raise TransformError(f"cannot import module {module_path!r}: {exc}") from exc
    try:
        obj = getattr(module, attr)
    except AttributeError as exc:
        raise TransformError(
            f"{module_path!r} has no attribute {attr!r}"
        ) from exc

    instance = obj() if callable(obj) else obj
    if not isinstance(instance, Transform):
        raise TransformError(
            f"{spec!r} did not produce a valid Transform (needs transform_title / "
            f"transform_listing / transform_images)"
        )
    return instance


__all__ = ["Transform", "BaseTransform", "get_transform"]
