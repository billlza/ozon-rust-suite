# OpenClaw Local Bridge

Base URL: `http://127.0.0.1:8790`  
Auth header: `x-openclaw-token: <bridge proposal token>`

OpenClaw reads product facts through the local node and can prepare store
changes for operator review. Actions that affect store data stay behind local
operator approval. For poster generation, the preferred path is account-based:
OpenClaw/Codex uses its signed-in image capability while the local node supplies
only Ozon facts, image URLs, and safety instructions.

## Tools

| Tool | Method | Path | Purpose | Approval |
| --- | --- | --- | --- | --- |
| `health` | `GET` | `/health` | Check local node availability | No |
| `ozon.products.count` | `POST` | `/tools/ozon.products.count` | Count products through the configured connector; real mode uses saved Ozon Seller API credentials | No |
| `ozon.products.list` | `POST` | `/tools/ozon.products.list` | List product summaries with a bounded limit; real mode returns official Ozon Seller API data | No |
| `ozon.products.get` | `POST` | `/tools/ozon.products.get` | Read one product fact pack with stable details, attributes, and image URLs | No |
| `poster.handoff` | `POST` | `/poster/handoff` | Build a product-grounded poster prompt and image package for OpenClaw/Codex generation | No |
| `tasks.dry_run` | `POST` | `/tasks/dry-run` | Prepare a reviewed task with diff and risk summary | Local approval required |
| `tasks.get` | `GET` | `/tasks/{task_id}` | Read task state | No |
| `schedules.ecommerce_read.propose` | `POST` | `/schedules/ecommerce-read/propose` | Prepare official Ozon read-only polling | Operator must enable |

## Product Fact Pack

```json
{
  "offer_id": "SKU-123"
}
```

Provide exactly one of `offer_id`, `product_id`, or `sku`. The real connector
uses Ozon Seller `/v3/product/info/list` as the source of truth for the product
record and image order, then enriches attributes and backup image fields from
`/v4/product/info/attributes` when available. The response keeps image URLs in a
stable ordered `images` array with roles: `primary`, `gallery`, `color`, and
`spin360`.

## Poster Handoff Payload

```json
{
  "offer_id": "SKU-123",
  "theme": "studio",
  "locale": "zh-CN"
}
```

This returns the same product fact pack plus an operator-ready prompt. The
prompt tells OpenClaw/Codex to use the current signed-in account for image
generation, preserve the real product appearance, and avoid unsupported claims.
It deliberately does not include the bridge token or any OpenAI/API secret.

## Dry-Run Payload

```json
{
  "operation": "ozon_update_price_review",
  "source": "open_claw",
  "shop_id": "default-shop",
  "risk": "high",
  "idempotency_key": "openclaw-review-001"
}
```

Review operations:

- `ozon_update_price_review`
- `ozon_update_inventory_review`
- `ozon_join_promotion_review`
- `draft_upload_review`

## Read-Only Schedule Proposal

```json
{
  "shop_id": "default-shop",
  "interval_secs": 900,
  "limit": 20,
  "idempotency_key": "openclaw-schedule-001"
}
```

This prepares a low-risk schedule request. The local operator enables, disables,
or runs the scheduler through operator-token endpoints.

## Safety Contract

- Do not call approval or execution endpoints from OpenClaw; those endpoints
  require the local operator token.
- Do not submit captcha bypass, anti-bot bypass, or live 1688 scraping tasks.
- Keep batch sizes small and include an `idempotency_key`.
- Treat all Ozon writes as reviewed work until the local UI approves them.
- Scheduled e-commerce reads must use official Ozon seller APIs with saved
  credentials, bounded intervals, and small limits.
- Do not schedule live 1688 scraping, captcha bypass, anti-bot bypass, or
  unattended write operations.
