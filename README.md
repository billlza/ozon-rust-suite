# Ozon Rust Suite

Rust-first control plane for a self-owned Ozon seller automation product. It follows a
mixed architecture: Docker VPS cloud control plane, Windows Tauri local node,
and an OpenClaw-compatible local skill bridge.

## Core Capabilities

- Cloud API: Nebula identity sync through the `/auth/skybridge` compatibility
  bridge, plus
  local-only `local_dev` auth fallback when explicitly enabled; manual orders,
  Stripe Checkout orders, signed Stripe webhook fulfillment, card keys, redeem,
  device activation, entitlement leases, downloads, audit.
- Commercial flow hardening: order confirmation works by order UUID or payment
  reference for support/manual flows; Stripe orders record provider, amount,
  currency, checkout session, payment intent, and webhook event IDs before
  entitlement activation. Card-key `max_devices` is enforced during device
  activation and lease issuance.
- Local node API: `127.0.0.1:8790` skill endpoints and `127.0.0.1:17870`
  status/event endpoints.
- Local diagnostics: `GET /config/status` reports connector mode, OS keyring
  availability, redacted Ozon credential status, and local endpoint URLs.
- Task engine: review-first, store-affecting tasks require local approval before
  execution.
- Ozon connector: production read-only mode calls the official Ozon Seller API
  with user-provided `Client-Id` and `Api-Key`, reads product list pagination
  metadata (`total` / `last_id`), and fails closed when credentials are missing.
  Mock data is limited to explicit local development mode.
- React UIs: public portal, admin console, and Tauri local configuration center.
  The portal treats Nebula as the account authority for email, phone, and
  Nebula ID login/registration; Ozon only maps the resulting Nebula session to
  a service account. It also supports token exchange, browser session restore,
  order creation, card-key redemption, device binding, lease issuance,
  entitlement status, and local-node download access.
- Cloud storage: Postgres through `sqlx` migrations; card keys are stored as
  hashes plus SHA-256 fingerprints.
- Secret storage: local Ozon credentials are stored in the OS credential store
  through Rust `keyring` (Windows Credential Manager/DPAPI-backed on Windows).
- Safety baseline: localhost-only local services, separate OpenClaw/operator
  tokens, secret redaction, no 1688 live scraping, no unapproved Ozon writes.

## Run Locally

Fastest local development path:

```bash
pnpm dev:local-node
pnpm dev:local-ui
```

Local UI: <http://127.0.0.1:5173>  
OpenClaw manifest: <http://127.0.0.1:8790/openclaw/manifest>

Full stack:

```bash
pnpm install
cargo test --workspace

# Terminal 0, or provide your own Postgres and DATABASE_URL.
docker run --rm --name ozon-rust-suite-postgres \
  -e POSTGRES_DB=ozon_rust_suite \
  -e POSTGRES_USER=ozon \
  -e POSTGRES_PASSWORD=ozon \
  -p 127.0.0.1:55432:5432 postgres:18

# Terminal 1
pnpm dev:nebula

# Terminal 2
DATABASE_URL=postgres://ozon:ozon@127.0.0.1:55432/ozon_rust_suite cargo run -p ozon-cloud-api

# Terminal 3, local development connector
OZON_CONNECTOR_MODE=mock OZON_LOCAL_TOKEN=dev-local-token cargo run -p ozon-local-node

# Terminal 4
pnpm dev:portal
pnpm dev:admin
pnpm dev:local-ui
```

Portal: <http://127.0.0.1:5171>  
Admin: <http://127.0.0.1:5172>  
Local UI: <http://127.0.0.1:5173>

Real Ozon read-only local node:

```bash
OZON_CONNECTOR_MODE=real \
OZON_LOCAL_TOKEN=<strong-local-operator-token> \
OZON_OPENCLAW_TOKEN=<strong-openclaw-read-token> \
cargo run -p ozon-local-node

VITE_LOCAL_TOKEN=<same-strong-local-operator-token> pnpm dev:local-ui
```

Then open <http://127.0.0.1:5173>, save the user's Ozon Seller `Client ID` and
`API Key`, run credential validation, and read products. In this mode the local
node never falls back to mock products; missing or invalid credentials return an
error. Poster generation is account-first: use `POST /poster/handoff` or the
local UI's "复制给龙虾/Codex" action to hand a product-grounded prompt and image
URLs to OpenClaw/Codex. The optional image API config is only for unattended
background generation; the key is written to the OS keyring and is not stored in
the repository. The OpenClaw token can call read tools and prepare reviewed
tasks only.

Release builds default to the real connector. Local sample data is available
only in explicit development mode through `OZON_CONNECTOR_MODE=mock`.

Local development can use a Rust Nebula issuer on <http://127.0.0.1:8788> by
default. It registers `ozon_rust_suite_portal` with the exact callback
`http://127.0.0.1:5171/auth/callback`, implements PKCE, and exposes the
Nebula/Supabase-compatible `/get-user-profile` bridge that `cloud-api` uses to keep
the canonical Nebula ID stable. Demo credentials are `demo` / `demo-pass`.

Production Nebula identity configuration for the portal/cloud bridge:

```bash
OZON_SUITE_SKYBRIDGE_API_BASE_URL=https://<skybridge-project>.supabase.co/functions/v1
VITE_SKYBRIDGE_SUPABASE_URL=https://<skybridge-project>.supabase.co
VITE_SKYBRIDGE_SUPABASE_ANON_KEY=<skybridge-anon-key>
VITE_NEBULA_BASE_URL=https://nebula.skybridge.com
VITE_NEBULA_CLIENT_ID=ozon_rust_suite_portal
VITE_NEBULA_SCOPE="openid profile email offline_access"
VITE_TURNSTILE_SITE_KEY=<skybridge-turnstile-site-key>
```

The preferred production path is Nebula OAuth/PKCE. Register the portal client in
Nebula with an exact redirect URI matching the running portal, for example
`http://127.0.0.1:5171/auth/callback` in local development. Direct
SkyBridge/Supabase password exchange is kept only as a compatibility path; if
Nebula/Supabase requires Turnstile, configure `VITE_TURNSTILE_SITE_KEY` so the
user completes the same Cloudflare Turnstile challenge Nebula uses.

If `https://nebula.skybridge.com` presents a self-signed or otherwise untrusted
certificate on a developer machine, do not disable browser certificate checks.
Use the local Rust issuer for development and switch to the real issuer only
after the Nebula backend has a trusted certificate and the
`ozon_rust_suite_portal` client plus exact redirect URI have been registered.

For local development without Nebula, you can opt into the isolated development
fallback:

```bash
OZON_SUITE_ALLOW_LOCAL_NEBULA_REGISTRATION=true
```

Production should leave that fallback disabled so Ozon never becomes a second
Nebula account authority.

## Payments

`OZON_SUITE_PAYMENT_PROVIDER=manual` keeps the existing support workflow:
create an order, confirm it from the admin side, and redeem the returned card
key. `OZON_SUITE_PAYMENT_PROVIDER=stripe` creates a real Stripe Checkout
Session from `POST /orders`; the browser is redirected to Stripe and
`POST /webhooks/stripe` verifies `Stripe-Signature`, checks the session amount
and currency against the stored order, then activates the entitlement server-side.
`OZON_SUITE_PAYMENT_PROVIDER=wechat_pay` creates a WeChat Pay API v3 Native
prepay order from `POST /orders`; the portal renders the returned `code_url` as
a QR code, and `POST /webhooks/wechatpay` verifies the WeChat Pay signature,
decrypts the AES-256-GCM resource with the API v3 key, checks app id, merchant
id, trade type, amount, currency and `out_trade_no`, then activates the
entitlement server-side.

Required Stripe settings:

```bash
OZON_SUITE_PAYMENT_PROVIDER=stripe
OZON_SUITE_STRIPE_SECRET_KEY=sk_live_...
OZON_SUITE_STRIPE_WEBHOOK_SECRET=whsec_...
OZON_SUITE_STRIPE_SUCCESS_URL=https://ozon66.com/?checkout=success#console
OZON_SUITE_STRIPE_CANCEL_URL=https://ozon66.com/?checkout=cancelled#console
OZON_SUITE_STRIPE_CURRENCY=cny
OZON_SUITE_STRIPE_STANDARD_30D_AMOUNT_MINOR=4000
```

If you enable additional payment methods through Stripe Checkout, keep
`OZON_SUITE_PAYMENT_PROVIDER=stripe` and manage those payment methods in Stripe.

Required WeChat Pay Native settings:

```bash
OZON_SUITE_PAYMENT_PROVIDER=wechat_pay
OZON_SUITE_WECHAT_API_BASE_URL=https://api.mch.weixin.qq.com
OZON_SUITE_WECHAT_APP_ID=wx...
OZON_SUITE_WECHAT_MCH_ID=1900000000
OZON_SUITE_WECHAT_MERCHANT_SERIAL_NO=...
OZON_SUITE_WECHAT_MERCHANT_PRIVATE_KEY_PEM="-----BEGIN PRIVATE KEY-----..."
OZON_SUITE_WECHAT_API_V3_KEY=32-byte-api-v3-key
OZON_SUITE_WECHATPAY_PUBLIC_KEY_ID=PUB_KEY_ID_...
OZON_SUITE_WECHATPAY_PUBLIC_KEY_PEM="-----BEGIN PUBLIC KEY-----..."
OZON_SUITE_WECHAT_NOTIFY_URL=https://api.ozon66.com/webhooks/wechatpay
OZON_SUITE_WECHAT_CURRENCY=CNY
OZON_SUITE_WECHAT_STANDARD_30D_AMOUNT_MINOR=4000
```

The WeChat login phone or personal WeChat ID is not a payment API credential.
Use it only to sign in to the WeChat Pay merchant platform, then copy the
merchant parameters above into the deployment environment.

## Local node release manifest

The local-node release workflow publishes Windows MSI/EXE and Apple Silicon DMG assets plus
`release-manifest.json` to `billlza/ozon-rust-suite-downloads` by default. Set
the repository variable `OZON_DOWNLOADS_REPOSITORY` to publish to another
downloads repository, and set `OZON_DOWNLOADS_REPO_TOKEN` when the workflow needs
cross-repository release permissions.

`cloud-api` treats the manifest as the single package source for `GET
/downloads`. Production deployments should point
`OZON_SUITE_LOCAL_NODE_RELEASE_MANIFEST_URL` at the release asset:

```json
{
  "version": "0.1.0",
  "commit": "0123456789abcdef0123456789abcdef01234567",
  "msi": {
    "url": "https://github.com/billlza/ozon-rust-suite-downloads/releases/download/local-node-v0.1.0/OzonRustLocal-x64.msi",
    "sha256": "64-character-lowercase-hex"
  },
  "exe": {
    "url": "https://github.com/billlza/ozon-rust-suite-downloads/releases/download/local-node-v0.1.0/OzonRustLocalSetup-x64.exe",
    "sha256": "64-character-lowercase-hex"
  },
  "macos_aarch64_dmg": {
    "url": "https://github.com/billlza/ozon-rust-suite-downloads/releases/download/local-node-v0.1.0/OzonRustLocal-aarch64.dmg",
    "sha256": "64-character-lowercase-hex"
  }
}
```

## OpenClaw bridge

Static package: [openclaw/manifest.json](openclaw/manifest.json) and
[openclaw/tools.md](openclaw/tools.md). The running local node also exposes a
machine-readable manifest at `GET /openclaw/manifest`.

OpenClaw should call only read tools, `POST /poster/handoff`,
`POST /tasks/dry-run`, and task status lookups with `x-openclaw-token`. In real
Ozon mode those read tools use the operator-saved Ozon Seller API credentials.
Approval, cancellation, execution, configuration, diagnostics, and event
streaming require the operator-only `x-local-token`.

The public portal can issue a cloud lease and deliver it to
`POST /portal/lease` on the local node after the browser has detected
`127.0.0.1:8790`. The local node validates expiry/features, persists the lease
through the OS keyring, and reports it from `GET /portal/status`.

Product detail reads use `POST /tools/ozon.products.get` with exactly one of
`offer_id`, `product_id`, or `sku`. Real mode reads Ozon `/v3/product/info/list`
for the canonical product record and image order, then enriches attributes and
backup image fields from `/v4/product/info/attributes` when available. The
response is a product fact pack suitable for downstream poster generation: text
facts and image URLs stay separate so generated artwork does not invent product
specs.

## Test CLI

`ozon-suite-qa` is a read-only Rust harness for local-node smoke, performance,
stability, and RSS growth checks. It emits JSON and is safe to run against a
real node because it does not call Ozon write endpoints or image-generation
endpoints.

```bash
cargo run -p ozon-suite-qa -- \
  --base-url http://127.0.0.1:8790 \
  --local-token "$OZON_LOCAL_TOKEN" \
  --openclaw-token "$OZON_OPENCLAW_TOKEN" \
  smoke

cargo run -p ozon-suite-qa -- \
  --base-url http://127.0.0.1:8790 \
  --openclaw-token "$OZON_OPENCLAW_TOKEN" \
  perf --scenario poster-handoff --offer-id SKU-123 --requests 100 --concurrency 8

cargo run -p ozon-suite-qa -- \
  --base-url http://127.0.0.1:8790 \
  --openclaw-token "$OZON_OPENCLAW_TOKEN" \
  stability --scenario health --duration-secs 300 --interval-ms 500

cargo run -p ozon-suite-qa -- \
  --base-url http://127.0.0.1:8790 \
  memory --pid <local-node-pid> --scenario health --duration-secs 600
```

Use `all --pid <local-node-pid>` for a short combined run. Product-detail and
poster-handoff scenarios require one lookup flag: `--offer-id`, `--product-id`,
or `--sku`.

## API safety notes

- Local APIs refuse non-loopback binds.
- OpenClaw bridge calls use `x-openclaw-token`; operator actions use
  `x-local-token`.
- Store-affecting operations enter `pending_approval` and cannot run without approval.
- Review-mode write execution never sends a real Ozon API request.
- Live 1688 collection is intentionally excluded from this product surface.
- Card keys are returned only at creation/confirmation time; storage keeps hashes
  and fingerprints, not plaintext card keys.
- Real Ozon mode (`OZON_CONNECTOR_MODE=real`, and release builds by default)
  requires saved Ozon credentials; local sample data is limited to explicit
  development runs.
- Nebula is the identity authority. Ozon stores a projection of
  `skybridge_user_id`, canonical `nebula_id`, and optional email/phone snapshots;
  it must not locally mint production Nebula IDs or verify Nebula user
  passwords.

## Production deployment sketch

The public portal can be deployed as a static Vite app. Build it with the public
API origin:

```bash
VITE_CLOUD_API=https://api.ozon66.com \
VITE_NEBULA_BASE_URL=https://nebula.skybridge.com \
VITE_NEBULA_CLIENT_ID=ozon_rust_suite_portal \
pnpm --dir apps/web-portal build
```

For the Rust cloud API on a VPS, copy `deploy/.env.ozon66.example` to
`deploy/.env.ozon66`, replace every secret, point DNS for `api.ozon66.com` at
the VPS, then run:

```bash
docker compose -f deploy/docker-compose.ozon66.yml --env-file deploy/.env.ozon66 up -d --build
```

Register `https://ozon66.com/auth/callback` in Nebula before opening production
OAuth to users. Keep the admin console behind private access controls; it still
uses an operator token and should not be exposed as a public static site.
