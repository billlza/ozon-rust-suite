# Mainland production line

This line is separate from the Vercel overseas deployment. It serves the same
portal code as static files, but it must not rely on Cloudflare Turnstile,
direct Supabase browser auth, or Vercel edge routing.

## Build

```bash
cp deploy/.env.portal-mainland.example deploy/.env.portal-mainland
# Edit deploy/.env.portal-mainland with the real mainland/HK API and Nebula origins.
pnpm build:portal:mainland -- --env-file deploy/.env.portal-mainland
```

The build script fails if the portal bundle contains the Cloudflare Turnstile
script, direct SkyBridge/Supabase browser auth markers, or the compatibility
email/phone login panel.

## Static hosting requirements

- Serve `apps/web-portal/dist` as immutable static assets.
- Rewrite `/auth/callback` to `/index.html`.
- Rewrite other SPA paths to `/index.html`.
- Do not inject third-party challenge scripts into the portal page.
- Register every public callback URL in Nebula, for example
  `https://cn.ozon66.com/auth/callback`.

`deploy/nginx.portal-mainland.conf` is a minimal Nginx reference for a mainland
or Hong Kong server/CDN origin.

## Identity requirements

The portal is only an OAuth relying party. Human verification, SMS, MFA, and
risk checks must happen inside Nebula/SkyBridge before it issues the
authorization code. For mainland users, Nebula/SkyBridge needs a
mainland-accessible verification provider and server-side verification.

This repository currently contains only the local development issuer under
`apps/nebula-dev-issuer`; the production Nebula/SkyBridge identity service must
be deployed and configured separately.
