"""GPT Image 2 image-edit client.

A single thin function over the verified ``POST {api_base}/v1/images/edits``
multipart endpoint. This is the ONLY module in the package that talks to the
image-generation provider; everything else (the relist transform, the demo)
injects this function so tests run fully offline with no network and no spend.

The core stays network-free: this lives outside ``writer``/``verifier`` and is
only reached when a caller explicitly wires the relist transform to it.
"""

from __future__ import annotations

import base64
import io

import requests

from .errors import OzonExcelError

DEFAULT_API_BASE = "https://api.apiyi.com"
DEFAULT_MODEL = "gpt-image-2-vip"
DEFAULT_SIZE = "1024x1024"


class ImageGenError(OzonExcelError):
    """Raised when the image-edit endpoint cannot produce an image."""


def gptimage_edit(
    image_bytes: bytes,
    prompt: str,
    *,
    api_base: str = DEFAULT_API_BASE,
    api_key: str,
    model: str = DEFAULT_MODEL,
    size: str = DEFAULT_SIZE,
    timeout: int = 300,
) -> bytes:
    """Edit ``image_bytes`` with GPT Image 2 and return the generated PNG bytes.

    Implements the verified call format exactly:
      POST {api_base}/v1/images/edits
      Authorization: Bearer {api_key}
      multipart form: model, image (file), prompt, size, n=1
      response JSON: {"data": [{"b64_json": "..."}]}  (may carry "url" instead)
    """
    if not api_key:
        raise ImageGenError(
            "image-edit API key is empty; set OZON_RELIST_IMAGE_API_KEY (or pass "
            "image_api_key=) before running the relist transform in real mode."
        )

    url = api_base.rstrip("/") + "/v1/images/edits"
    headers = {"Authorization": f"Bearer {api_key}"}
    files = {"image": ("image.png", io.BytesIO(image_bytes), "image/png")}
    data = {
        "model": model,
        "prompt": prompt,
        "size": size,
        "n": "1",
    }

    resp = requests.post(url, headers=headers, files=files, data=data, timeout=timeout)
    if resp.status_code != 200:
        raise ImageGenError(
            f"image-edit endpoint {url} returned HTTP {resp.status_code}: "
            f"{resp.text[:300]!r}"
        )

    try:
        payload = resp.json()
    except ValueError as exc:
        raise ImageGenError(
            f"image-edit endpoint {url} returned non-JSON body: {resp.text[:300]!r}"
        ) from exc

    items = payload.get("data") or []
    if not items:
        raise ImageGenError(
            f"image-edit response from {url} had no 'data' items: {payload!r}"
        )
    item = items[0]

    b64 = item.get("b64_json")
    if b64:
        try:
            return base64.b64decode(b64)
        except (ValueError, TypeError) as exc:
            raise ImageGenError(
                f"image-edit response b64_json from {url} was not valid base64"
            ) from exc

    img_url = item.get("url")
    if img_url:
        fetched = requests.get(img_url, timeout=timeout)
        if fetched.status_code != 200:
            raise ImageGenError(
                f"image-edit response url {img_url} returned HTTP "
                f"{fetched.status_code}"
            )
        return fetched.content

    raise ImageGenError(
        f"image-edit response from {url} carried neither 'b64_json' nor 'url': "
        f"{item!r}"
    )
