# Ozon Rust Suite

Rust-first MVP for a self-owned Ozon seller automation product. It follows a
mixed architecture: Docker VPS cloud control plane, Windows Tauri local node,
and an OpenClaw-compatible local skill bridge.

## What is implemented

- Cloud API: Nebula identity sync through the `/auth/skybridge` compatibility
  bridge, plus
  local-only `local_dev` auth fallback when explicitly enabled; manual orders,
  admin confirmation, card keys, redeem, device activation, entitlement leases,
  downloads, audit.
- Commercial flow hardening: order confirmation works by order UUID or payment
  reference, and card-key `max_devices` is enforced during device activation and
  lease issuance.
- Local node API: `127.0.0.1:8790` skill endpoints and `127.0.0.1:17870`
  status/event endpoints.
- Local diagnostics: `GET /config/status` reports connector mode, OS keyring
  availability, redacted Ozon credential status, and local endpoint URLs.
- Task engine: dry-run first, write tasks require local approval before mock
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

## Run locally

Fastest local demo with explicit mock data:

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

# Terminal 3, explicit local mock demo mode
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
error. The OpenClaw token can call read tools and propose tasks only.

Release builds default to the real connector. Mock products are allowed only in
debug/local demo mode through `OZON_CONNECTOR_MODE=mock`.

Local development uses a Rust Nebula issuer on <http://127.0.0.1:8788> by
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
user completes the same Cloudflare Turnstile challenge Nebula uses. Without that key the
password path reports the captcha requirement and fails closed instead of
bypassing the challenge.

If `https://nebula.skybridge.com` presents a self-signed or otherwise untrusted
certificate on a developer machine, do not disable browser certificate checks.
Use the local Rust issuer for development and switch to the real issuer only
after the Nebula backend has a trusted certificate and the
`ozon_rust_suite_portal` client plus exact redirect URI have been registered.

For local demos without Nebula, you can opt into the isolated development
fallback:

```bash
OZON_SUITE_ALLOW_LOCAL_NEBULA_REGISTRATION=true
```

Production should leave that fallback disabled so Ozon never becomes a second
Nebula account authority.

## OpenClaw bridge

Static package: [openclaw/manifest.json](openclaw/manifest.json) and
[openclaw/tools.md](openclaw/tools.md). The running local node also exposes a
machine-readable manifest at `GET /openclaw/manifest`.

OpenClaw should call only read tools, `POST /tasks/dry-run`, and task status
lookups with `x-openclaw-token`. In real Ozon mode those read tools use the
operator-saved Ozon Seller API credentials. Approval, cancellation, mock execution,
configuration, diagnostics, and event streaming require the operator-only
`x-local-token`.

## API safety notes

- Local APIs refuse non-loopback binds.
- OpenClaw bridge calls use `x-openclaw-token`; operator actions use
  `x-local-token`.
- Write operations enter `pending_approval` and cannot run without approval.
- Mock write execution never sends a real Ozon API request.
- `1688` live collection is intentionally excluded from MVP.
- Card keys are returned only at creation/confirmation time; storage keeps hashes
  and fingerprints, not plaintext card keys.
- Real Ozon mode (`OZON_CONNECTOR_MODE=real`, and release builds by default)
  requires saved Ozon credentials; mock fallback is limited to explicit debug
  demos.
- Nebula is the identity authority. Ozon stores a projection of
  `skybridge_user_id`, canonical `nebula_id`, and optional email/phone snapshots;
  it must not locally mint production Nebula IDs or verify Nebula user
  passwords.
- Public production hardening is still pending: strict CORS, rate limiting,
  refresh-token/session revocation for Ozon JWTs, non-dev JWT/admin secrets, and
  full Nebula OAuth/PKCE redirect wiring for the portal.

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

## Next implementation milestones

1. Add Postgres-backed integration tests for auth/order/card-key/device/lease flows.
2. Harden public auth for production: Nebula OAuth/PKCE redirect flow,
   rate limiting/captcha at the bridge boundary, refresh-token/session
   revocation, strict CORS, and non-dev secrets.
3. Add Ozon real read-only credential validation tests with mocked HTTP.
4. Add Playwright smoke tests for portal/admin/local UI.
5. Package installer/download artifacts with real checksums.
6. Feature-flag real Ozon write APIs after mock approval flow is battle-tested.
