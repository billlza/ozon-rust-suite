"""Ozon Seller API client.

Thin wrapper over the verified Seller API endpoints used by the relist demo to
pull real product data (names, descriptions, image URLs). Network-only: the
core never imports this; the demo wires it explicitly and tests substitute fake
data so nothing here is exercised offline.

Verified endpoints (base https://api-seller.ozon.ru):
  POST /v3/product/list           -> items[].product_id / offer_id
  POST /v3/product/info/list      -> items[] with name, primary_image, images[]
  POST /v1/product/info/description -> description text
Headers on every call: Client-Id, Api-Key, Content-Type: application/json.
"""

from __future__ import annotations

import os
from typing import Optional

import requests

from .errors import OzonExcelError

DEFAULT_BASE = "https://api-seller.ozon.ru"


class OzonApiError(OzonExcelError):
    """Raised on a non-200 Ozon Seller API response or missing credentials."""


class OzonClient:
    def __init__(
        self,
        client_id: str,
        api_key: str,
        *,
        base: str = DEFAULT_BASE,
        timeout: int = 60,
    ):
        if not client_id or not api_key:
            raise OzonApiError(
                "OzonClient needs both client_id and api_key. Set "
                "OZON_CLIENT_ID and OZON_API_KEY (see .env.example)."
            )
        self.client_id = client_id
        self.api_key = api_key
        self.base = base.rstrip("/")
        self.timeout = timeout

    @classmethod
    def from_env(cls, **kwargs) -> "OzonClient":
        """Build a client from OZON_CLIENT_ID / OZON_API_KEY."""
        client_id = os.environ.get("OZON_CLIENT_ID", "")
        api_key = os.environ.get("OZON_API_KEY", "")
        if not client_id or not api_key:
            raise OzonApiError(
                "OZON_CLIENT_ID and OZON_API_KEY must both be set in the "
                "environment (see .env.example) to use OzonClient.from_env()."
            )
        return cls(client_id, api_key, **kwargs)

    # ------------------------------------------------------------------ #
    def _headers(self) -> dict:
        return {
            "Client-Id": str(self.client_id),
            "Api-Key": str(self.api_key),
            "Content-Type": "application/json",
        }

    def _post(self, path: str, body: dict) -> dict:
        url = self.base + path
        resp = requests.post(
            url, headers=self._headers(), json=body, timeout=self.timeout
        )
        if resp.status_code != 200:
            raise OzonApiError(
                f"Ozon {path} returned HTTP {resp.status_code}: {resp.text[:300]!r}"
            )
        try:
            return resp.json()
        except ValueError as exc:
            raise OzonApiError(
                f"Ozon {path} returned non-JSON body: {resp.text[:300]!r}"
            ) from exc

    # ------------------------------------------------------------------ #
    def list_products(self, limit: int = 3) -> list:
        """Return up to ``limit`` items, each with product_id / offer_id."""
        payload = self._post(
            "/v3/product/list",
            {"filter": {"visibility": "ALL"}, "last_id": "", "limit": int(limit)},
        )
        result = payload.get("result") or {}
        return result.get("items") or []

    def product_info(self, product_ids: list) -> list:
        """Return info items (name, primary_image, images[]) for the given ids."""
        ids = [int(p) for p in product_ids]
        payload = self._post("/v3/product/info/list", {"product_id": ids})
        result = payload.get("result")
        if isinstance(result, dict):
            return result.get("items") or []
        # Some API variants return items at the top level.
        return payload.get("items") or []

    def product_description(
        self,
        *,
        offer_id: Optional[str] = None,
        product_id: Optional[int] = None,
    ) -> str:
        """Return the description text for one product (by offer_id or id)."""
        if offer_id is not None:
            body = {"offer_id": str(offer_id)}
        elif product_id is not None:
            body = {"product_id": int(product_id)}
        else:
            raise OzonApiError(
                "product_description requires offer_id= or product_id="
            )
        payload = self._post("/v1/product/info/description", body)
        result = payload.get("result")
        if isinstance(result, dict):
            return result.get("description") or ""
        return payload.get("description") or ""
