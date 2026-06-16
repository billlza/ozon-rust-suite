# Mainland production line

This line is separate from the Vercel overseas deployment. It serves the same
portal code as static files, but it must not rely on Cloudflare Turnstile,
direct Supabase browser auth, phone/SMS auth flags, or Vercel edge routing.

## Chosen target

Use Tencent Cloud for the mainland/HK line:

- Static portal/CDN: Tencent EdgeOne Pages project
  `ozon66-mainland-portal`, serving `apps/web-portal/dist` on
  `https://cn.ozon66.com`.
- API: Tencent Cloud Lighthouse or CVM in Hong Kong, running
  `cloud-api + Caddy + Postgres` for `https://api-cn.ozon66.com`.
- Downloads mirror: `https://downloads-cn.ozon66.com` for the local-node MSI,
  EXE, release manifest, and OpenClaw plugin ZIP.
- Identity: `https://nebula-cn.ozon66.com`, with
  `https://cn.ozon66.com/auth/callback` registered as a callback URL.

If `cn.ozon66.com` uses EdgeOne's Chinese mainland availability zone or a
global zone that includes Chinese mainland, the domain must complete ICP filing
before it can be attached. Before ICP is ready, use the Hong Kong server as the
origin or temporary static host and keep the same portal build settings.

## Build

```bash
cp deploy/.env.portal-mainland.example deploy/.env.portal-mainland
# Edit deploy/.env.portal-mainland with the real mainland/HK API and Nebula origins.
pnpm build:portal:mainland -- --env-file deploy/.env.portal-mainland
```

The build script fails if the portal bundle contains the Cloudflare Turnstile
script, direct SkyBridge/Supabase browser auth markers, phone/SMS auth enablement,
or the compatibility email/phone login panel.

## Static hosting requirements

- Serve `apps/web-portal/dist` as immutable static assets.
- Rewrite `/auth/callback` to `/index.html`.
- Rewrite other SPA paths to `/index.html`.
- Do not inject third-party challenge scripts into the portal page.
- Register every public callback URL in Nebula, for example
  `https://cn.ozon66.com/auth/callback`.

`deploy/nginx.portal-mainland.conf` is a minimal Nginx reference for a mainland
or Hong Kong server/CDN origin.

## EdgeOne Pages deployment

Create an EdgeOne Pages project named `ozon66-mainland-portal`, then set:

```text
Build command: pnpm build:portal:mainland
Output directory: apps/web-portal/dist
Custom domain: cn.ozon66.com
```

The repository also includes `edgeone.json` and a CLI deployment script:

```bash
EDGEONE_API_TOKEN=... pnpm deploy:portal:edgeone
```

The CLI path is for release automation. Console deployment is fine as long as
the project uses the same build command and environment variables.

## API deployment

Provision a Tencent Cloud Lighthouse or CVM instance in Hong Kong with Docker
and ports `80` and `443` open. Point `api-cn.ozon66.com` to the instance public
IP, then deploy:

```bash
cp deploy/.env.api-cn.example deploy/.env.api-cn
# Fill every secret and production URL in deploy/.env.api-cn.
docker compose -f deploy/docker-compose.api-cn.yml --env-file deploy/.env.api-cn up -d --build
curl -fsSL https://api-cn.ozon66.com/health
```

`deploy/Caddyfile.api-cn` terminates HTTPS and proxies to `cloud-api:8080`.
The API CORS allowlist must include `https://cn.ozon66.com`; the local-node CORS
allowlist also includes this domain so browser-to-`127.0.0.1:8790` checks work
from the mainland portal.

## DNS records

```text
cn.ozon66.com          CNAME  <EdgeOne Pages assigned target>
api-cn.ozon66.com      A      <Hong Kong Lighthouse/CVM public IP>
downloads-cn.ozon66.com CNAME/A <release mirror target>
nebula-cn.ozon66.com   CNAME/A <Nebula/SkyBridge CN identity target>
```

## Identity requirements

The portal is only an OAuth relying party. Human verification, SMS, MFA, and
risk checks must happen inside Nebula/SkyBridge before it issues the
authorization code. For mainland users, Nebula/SkyBridge needs a
mainland-accessible verification provider and server-side verification; do not
publish a portal-side "send SMS code" entry until that path has been tested end
to end.

This repository currently contains only the local development issuer under
`apps/nebula-dev-issuer`; the production Nebula/SkyBridge identity service must
be deployed and configured separately.
