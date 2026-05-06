# Security Review

Date: 2026-05-05

## Executive Summary

This pass focused on cloud defaults, local-node bridge boundaries, OpenClaw
scheduling, Nebula compatibility, and the requested quantum-cryptography
direction. The main production blockers are no longer permissive CORS or
accidental cloud deployment with dev JWT/admin secrets: both now fail closed for
production or non-loopback binds. The new OpenClaw scheduling path is read-only
and proposal-gated.

The system is still not production-ready until rate limiting, revocable sessions,
and stronger admin identity are implemented.

## Fixed In This Pass

### S1. Cloud CORS Was Permissive

Previously, the cloud API used permissive CORS on every route. This is risky
because the portal currently stores its bearer token in browser storage, so any
untrusted origin allowed by CORS would increase the blast radius of browser-side
compromise.

Status: fixed.

Code:
- `/Users/bill/ozon-rust-suite/apps/cloud-api/src/main.rs:74`
- `/Users/bill/ozon-rust-suite/apps/cloud-api/src/main.rs:188`

Behavior:
- Development defaults allow only portal/admin localhost origins.
- Production/non-loopback binds must set `OZON_SUITE_CORS_ALLOWED_ORIGINS`.

### S2. Cloud Dev Secrets Could Be Deployed Accidentally

The cloud API had development defaults for `OZON_SUITE_JWT_SECRET` and
`OZON_SUITE_ADMIN_TOKEN`. Those are convenient locally but dangerous if the API
is bound publicly.

Status: fixed.

Code:
- `/Users/bill/ozon-rust-suite/apps/cloud-api/src/main.rs:149`

Behavior:
- `OZON_SUITE_ENV=production` or any non-loopback bind rejects default JWT/admin
  secrets.
- Production JWT secret must be at least 32 bytes.
- Production admin token must be at least 24 bytes.

### S3. Long-Lived Token Comparisons Used Plain Equality

Plain equality is not the largest risk here, but it is cheap to harden and keeps
auth checks consistent.

Status: fixed.

Code:
- `/Users/bill/ozon-rust-suite/apps/cloud-api/src/main.rs:1524`
- `/Users/bill/ozon-rust-suite/apps/local-node/src/main.rs:760`

### S4. Admin UI Embedded The Dev Admin Token

The admin frontend prefilled `dev-admin-token`, which could leak into demos or
static builds.

Status: fixed.

Code:
- `/Users/bill/ozon-rust-suite/apps/web-admin/src/main.tsx:6`

Behavior:
- Admin token starts empty and is stored locally only after operator input.

### S5. OpenClaw Scheduled Collection Needed A Safe Boundary

OpenClaw scheduled e-commerce collection is useful, but it must not become live
scraping or unattended write automation.

Status: implemented as read-only Ozon polling.

Code:
- `/Users/bill/ozon-rust-suite/apps/local-node/src/main.rs:87`
- `/Users/bill/ozon-rust-suite/apps/local-node/src/main.rs:528`
- `/Users/bill/ozon-rust-suite/apps/local-node/src/main.rs:569`
- `/Users/bill/ozon-rust-suite/apps/local-node/src/main.rs:697`
- `/Users/bill/ozon-rust-suite/apps/local-node/src/main.tsx:278`

Behavior:
- Operator endpoints can enable, disable, inspect, and run the scheduler.
- OpenClaw can only call `/schedules/ecommerce-read/propose`.
- Intervals are clamped to `60..86400` seconds.
- Per-run product sample limit is clamped to `1..100`.
- Only official Ozon read connector calls are used.
- No 1688 live scraping, captcha bypass, anti-bot bypass, or writes.

## Remaining Production Blockers

### P1. Add Rate Limiting

Routes for auth, Nebula token exchange, card-key redemption, admin token probes,
and local bridge reads still need explicit rate limiting.

Recommended scope:
- Cloud: `/auth/*`, `/card-keys/redeem`, `/admin/*`.
- Local: `/tools/ozon.products.*`, `/tasks/dry-run`,
  `/schedules/ecommerce-read/propose`.

SkyBridge reference:
- `/Users/bill/Desktop/SkyBridge Compass Pro release/Docs/ops/auth-email-and-sms-production.md:94`

### P1. Replace Ozon JWT With Revocable Sessions

Current Ozon service JWTs are short-lived but not revocable before expiration.

Recommended model:
- Add `iss`, `aud`, `iat`, `jti`.
- Add Postgres `sessions` and `refresh_tokens` tables.
- Store refresh tokens hashed.
- Rotate refresh tokens on every use.
- Revoke on logout, password reset, admin action, device risk, and Nebula
  session revocation.

SkyBridge reference:
- `/Users/bill/Desktop/SkyBridge Compass Pro release/Docs/Nebula-Public-Client-PKCE-Migration.md:62`

### P1. Move Portal Session Out Of `localStorage`

The portal still stores the Ozon bearer session in `localStorage`.

Code:
- `/Users/bill/ozon-rust-suite/apps/web-portal/src/main.tsx:265`

Recommended model:
- For production web: httpOnly, SameSite cookies with CSRF protection.
- For pure local desktop flows: memory-only token plus OS/keychain-backed
  refresh if needed.

### P1. Replace Static Admin Token With Admin Identity

The static `x-admin-token` path is now safer by default, but it is still a
shared root credential.

Recommended model:
- Prefer Nebula admin-role JWT for admin console.
- If bootstrap tokens remain, store only hashed tokens server-side, scope them,
  add expiry, rotate them, and audit by actor.

### P1. Harden Local Tokens For Real Ozon Mode

`dev-local-token` and `dev-openclaw-token` are acceptable for localhost mock
demos, but not real Ozon credentials.

Status: partially fixed.

Code:
- `/Users/bill/ozon-rust-suite/apps/local-node/src/main.rs:53`
- `/Users/bill/ozon-rust-suite/apps/local-node/src/main.rs:158`

Behavior:
- When `OZON_CONNECTOR_MODE=real`, the local node now fails if default local tokens
  are used.

Remaining recommendation:
- Generate first-run operator/bridge tokens and store them in OS keyring.
- Show one-time pairing codes in local UI.

### P2. Limit Stored Scheduled Read Data

The scheduler stores the latest product sample in memory. That is acceptable for
MVP, but real seller data should be minimized.

Recommended model:
- Field allowlist.
- Short retention.
- Optional redaction of names if the user only needs counts.
- Per-token quotas and audit entries for every bridge read.
- Exponential backoff and jitter.

## Quantum / Post-Quantum Cryptography Recommendation

Do not market this as "quantum communication" unless the product actually uses a
quantum key distribution channel or quantum hardware. For this product, the
practical direction is post-quantum cryptography (PQC), not custom quantum
transport.

Current MVP recommendation:
- Keep browser/cloud API on standard TLS.
- Do not invent custom encryption around HTTPS.
- Put PQC into a roadmap and metadata model first.

Future PQC path:
- Use NIST-standardized algorithms: ML-KEM from FIPS 203, ML-DSA from FIPS 204,
  and SLH-DSA from FIPS 205.
- Wait for stable Rust TLS ecosystem support before using hybrid TLS in
  production.
- For a future local-node pairing channel, model it after SkyBridge:
  deterministic binary transcript, supported suite negotiation, transcript-bound
  signatures, KEM-DEM envelope, and OS-backed signing key handles.

SkyBridge reference:
- `/Users/bill/Desktop/SkyBridge Compass Pro release/Docs/ProtocolAlignmentPlan.md:3`
- `/Users/bill/Desktop/SkyBridge Compass Pro release/Docs/ProtocolAlignmentPlan.md:15`

External references:
- NIST FIPS 203 ML-KEM: https://csrc.nist.gov/pubs/fips/203/final
- NIST FIPS 204 ML-DSA: https://csrc.nist.gov/pubs/fips/204/final
- NIST FIPS 205 SLH-DSA: https://csrc.nist.gov/pubs/fips/205/final
- Ozon API intro: https://docs.ozon.com/global/en/api/intro/

## Verification

Passed:
- `cargo check --workspace`
- `cargo test --workspace`
- `pnpm --dir /Users/bill/ozon-rust-suite/apps/local-node build`
- `pnpm --dir /Users/bill/ozon-rust-suite/apps/web-admin build`
- `pnpm --dir /Users/bill/ozon-rust-suite/apps/web-portal build`
