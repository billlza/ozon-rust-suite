"""Text rewrite via an OpenAI-compatible chat API (e.g. apiyi).

Used by RelistTransform to rewrite the product title / listing. Kept tiny and
dependency-light (just ``requests``); the model + endpoint are injected so the
relist transform can fall back to deterministic rewriters when no key is set or
a call fails.
"""

from __future__ import annotations

import requests

DEFAULT_TEXT_MODEL = "gpt-4o-mini"


def chat_rewrite(
    text: str,
    *,
    instruction: str,
    api_base: str,
    api_key: str,
    model: str = DEFAULT_TEXT_MODEL,
    timeout: int = 120,
    max_tokens: int = 900,
    temperature: float = 0.6,
) -> str:
    """Send ``text`` to a chat model under ``instruction`` and return the rewrite.

    Raises on transport/HTTP errors or an empty completion so the caller can
    decide whether to fall back.
    """
    url = api_base.rstrip("/") + "/v1/chat/completions"
    payload = {
        "model": model,
        "temperature": temperature,
        "max_tokens": max_tokens,
        "messages": [
            {"role": "system", "content": instruction},
            {"role": "user", "content": text},
        ],
    }
    r = requests.post(
        url,
        headers={
            "Authorization": f"Bearer {api_key}",
            "Content-Type": "application/json",
        },
        json=payload,
        timeout=timeout,
    )
    r.raise_for_status()
    j = r.json()
    choices = j.get("choices") or []
    content = (choices[0].get("message") or {}).get("content") if choices else None
    if not content or not content.strip():
        raise RuntimeError(f"text API returned no content: {str(j)[:300]}")
    # Models sometimes wrap the answer in quotes; strip a single enclosing pair.
    out = content.strip()
    if len(out) >= 2 and out[0] in "\"'«" and out[-1] in "\"'»":
        out = out[1:-1].strip()
    return out
