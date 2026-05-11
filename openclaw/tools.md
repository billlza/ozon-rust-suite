# OpenClaw Local Bridge

Base URL: `http://127.0.0.1:8790`  
Auth header: `x-openclaw-token: <bridge proposal token>`

OpenClaw may create proposed work, but it must not approve or execute write
tasks. Approval stays in the local operator UI.

## Tools

| Tool | Method | Path | Purpose | Approval |
| --- | --- | --- | --- | --- |
| `health` | `GET` | `/health` | Check local node availability | No |
| `ozon.products.count` | `POST` | `/tools/ozon.products.count` | Count products through the configured connector; real mode uses saved Ozon Seller API credentials | No |
| `ozon.products.list` | `POST` | `/tools/ozon.products.list` | List product summaries with a bounded limit; real mode returns official Ozon Seller API data | No |
| `ozon.products.get` | `POST` | `/tools/ozon.products.get` | Read one product fact pack with stable details, attributes, and image URLs | No |
| `tasks.dry_run` | `POST` | `/tasks/dry-run` | Create a proposed task with dry-run diff and risk | Local approval required for writes |
| `tasks.get` | `GET` | `/tasks/{task_id}` | Read task state | No |
| `schedules.ecommerce_read.propose` | `POST` | `/schedules/ecommerce-read/propose` | Propose official Ozon read-only polling | Operator must enable |

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

## Dry-Run Payload

```json
{
  "operation": "ozon_update_price_mock",
  "source": "open_claw",
  "shop_id": "default-shop",
  "risk": "high",
  "idempotency_key": "openclaw-demo-001"
}
```

Allowed MVP operations:

- `ozon_update_price_mock`
- `ozon_update_inventory_mock`
- `ozon_join_promotion_mock`
- `draft_upload_mock`
- `import1688_mock`

## Read-Only Schedule Proposal

```json
{
  "shop_id": "default-shop",
  "interval_secs": 900,
  "limit": 20,
  "idempotency_key": "openclaw-schedule-001"
}
```

This creates a low-risk task proposal only. The local operator must enable,
disable, or run the scheduler through operator-token endpoints.

## Safety Contract

- Do not call approval or execution endpoints from OpenClaw; those endpoints
  reject the bridge token and require the local operator token.
- Do not submit captcha bypass, anti-bot bypass, or live 1688 scraping tasks.
- Keep batch sizes small and include an `idempotency_key`.
- Treat all Ozon writes as proposed work until the local UI approves them.
- Scheduled e-commerce reads must use official Ozon seller APIs with saved
  credentials, bounded intervals, and small limits.
- Do not schedule live 1688 scraping, captcha bypass, anti-bot bypass, or
  unattended write operations.
