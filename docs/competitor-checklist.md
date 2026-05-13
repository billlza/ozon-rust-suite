# OzonClaw / PDF Feature Comparison

This project implements equivalent product capabilities with independent branding,
safer defaults, and explicit approval boundaries.

| Area | Competitor Observed Capability | MVP Status |
| --- | --- | --- |
| Account registration | Website register/login with captcha | Implemented SkyBridge/Nebula-backed account sync: email/phone/Nebula login belongs to SkyBridge, Ozon maps `/auth/skybridge` into service accounts and keeps `local_dev` fallback explicit; captcha/email/SMS policy stays on SkyBridge |
| Manual payment | Manual confirmation after user payment | Implemented manual order + admin confirmation by order UUID or payment reference |
| Card key | Product card key / device activation | Implemented card key generation/redeem + device activation with `max_devices` enforcement |
| Downloads | Windows package download | Implemented release-manifest-backed MSI/EXE downloads with SHA-256 checksums from the published release |
| Local config center | Browser/Tauri-like setup wizard | Implemented Tauri shell, React config center, OS keyring save, config status, and Ozon credential validation |
| Skill service | `127.0.0.1:8790` | Implemented health, read tools, dry-run, task status, and operator-only approval/execution |
| Agent service | `127.0.0.1:17870` | Implemented operator-only health + SSE task events |
| OpenClaw bridge | OpenClaw sends tasks | Implemented separate bridge token for read/proposal calls; plugin packaging still pending |
| QQ bot | Natural language commands through OpenClaw | Deferred to OpenClaw bridge; direct QQ protocol excluded |
| Ozon read | Product/query capability | Implemented official Ozon Seller API read connector with fail-closed credential handling; mock products are explicit debug/demo mode only |
| Ozon write | Price/inventory/promotion/upload | Mock write tasks implemented; real write deferred behind feature flag |
| 1688 search | Image search through browser session | Mock/import only; live scraping excluded |
| Task progress | Task ID and status | Implemented task state machine and event stream |
| Approval safety | Not clearly enforced in competitor docs | Implemented mandatory approval for all write tasks and split OpenClaw/operator tokens |
| Secret storage | Claims encrypted credentials | Implemented OS credential store via Rust keyring; Windows uses Credential Manager/DPAPI-backed storage |

## Current Gap Snapshot

- Identity is now aligned semantically: SkyBridge is the authority, Ozon is a relying service with cached `nebula_id`.
- Portal still needs production-grade OAuth/PKCE redirect wiring; current browser form can call SkyBridge Supabase Auth when env vars are set, and token sync remains the safest local demo path.
- Local `local_dev` auth is intentionally not a production account system and is disabled by default in the cloud API.
- Remaining commercial gaps versus a polished competitor site: payment automation, card-key admin UX polish, and end-to-end browser smoke tests.
