use std::{
    collections::HashMap,
    env, fs,
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use async_stream::stream;
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{get, post},
};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64_STANDARD};
use chrono::Utc;
use ozon_connector::{
    MockOzonConnector, OzonCategoryAttribute, OzonCategoryAttributeValue, OzonCredentials,
    OzonProductCopyUpdate, OzonProductListPage, OzonProductLookup, OzonReadConnector,
    OzonResolvedAttribute, OzonResolvedValue, OzonWriteConnector,
};
use ozon_domain::{
    DryRunDiff, EntitlementLease, ExecutionReceipt, Feature, FieldChange, OperationKind, RiskLevel,
    Task, TaskId, TaskSource, TenantId,
};
use ozon_secret_store::{
    FileSecretStore, LayeredSecretStore, SecretName, SecretStore, SystemSecretStore,
    fingerprint_secret, redact,
};
use ozon_task_engine::{CreateTask, TaskEvent, TaskStore};
use rsa::{
    RsaPublicKey,
    pkcs1v15::{Signature as RsaPkcs1v15Signature, VerifyingKey},
    pkcs8::DecodePublicKey,
    signature::Verifier,
};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use tokio::{
    net::TcpListener,
    sync::{RwLock, broadcast},
    task::JoinHandle,
};
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    trace::TraceLayer,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;

mod model_router;
mod video_dialect;
use model_router::{
    Capability, CapabilityStatus, ProviderEntry, ProviderKind, ResolvedProvider,
    StoredModelRegistry, VideoDialect, apply_auth, endpoint_for, inspect_capabilities,
    resolve_capability,
};

const DEFAULT_DEV_LOCAL_TOKEN: &str = "dev-local-token";
const DEFAULT_DEV_OPENCLAW_TOKEN: &str = "dev-openclaw-token";
/// Loopback (skill, agent) port pairs tried in order when no explicit bind is
/// configured, so a port already taken by another program doesn't leave the
/// helper permanently offline. The first pair where BOTH ports bind wins, and
/// the chosen pair is reported on stdout. The Tauri supervisor and the web
/// portal probe this SAME list to find the node, so it must stay in sync across
/// `apps/local-node/src-tauri/src/main.rs` and `apps/web-portal/src/main.tsx`.
const PORT_CANDIDATES: &[(u16, u16)] = &[
    (8790, 17870),
    (8791, 17871),
    (8890, 17970),
    (18790, 27870),
];
const DEFAULT_OPENCLAW_BIND_URL: &str = "http://127.0.0.1:18789/openclaw/import";
const OPENCLAW_PAIRING_TTL: Duration = Duration::from_secs(5 * 60);
const DEFAULT_OPENAI_IMAGE_MODEL: &str = "gpt-image-1";
const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com";
// Re-listing workbench: Russian-market Ozon promo restyle + temp public host.
// The product name (+ a few attributes) is injected per-product so the model
// writes correct, relevant Russian copy. Rules: keep the product & its layout
// untouched, restyle ONLY the background to fit the product, add a Russian
// headline + selling-point badges (may overlap the product), and never draw an
// Ozon logo or a QR code. Validated on real test-shop products 2026-06.
const RELIST_OZON_RULES: &str = "\nRULE 1 - Keep the product unchanged: reproduce the product from the source photo EXACTLY, like a fixed cut-out pasted onto a new background - identical shape, colours, materials, proportions, fine details and markings, and the SAME camera angle, pose, size and position. Do NOT re-pose, re-angle, rotate, shift, rescale, redraw or restyle the product, and do NOT crop or hide its main body. Keep the ENTIRE product visible; build everything else around it.\nRULE 2 - New background + Russian sales copy: replace ONLY the background with an attractive, modern, themed promotional background whose colours and mood fit this specific product. Lay it out cleanly: keep the product fully visible (usually lower-centre), put ONE large bold Russian HEADLINE in the empty background space ABOVE the product, and 2-3 short Russian selling-point captions in rounded pill badges down one side; badges may overlap only the product's outer edges, never crop its centre. Use clean modern e-commerce typography and write correct, natural, persuasive Russian that matches the product named above.\nRULE 3 - Strictly forbidden: no Ozon logo, no \"Ozon\" text, no marketplace logos or branding, no QR codes, no barcodes, no watermarks.\nOutput one clean, high-contrast, ready-to-publish vertical Russian listing image.";
// Portrait 2:3 — room for the headline + badges above/over the product.
const RELIST_IMAGE_SIZE: &str = "1024x1536";
const RELIST_MAX_BATCH: usize = 12;
/// Number of restyle candidates generated per product in the relist workbench.
/// The operator picks one in the UI; a partial failure returns the successful
/// subset rather than failing the whole item.
const RELIST_MAX_CANDIDATES: usize = 3;
const SECRET_OZON_CONFIG: &str = "ozon_config";
const SECRET_OPENAI_CONFIG: &str = "openai_config";
const SECRET_MODEL_REGISTRY: &str = "model_registry";
const SECRET_CLOUD_LEASE: &str = "cloud_lease";
const SECRET_DEVICE_FINGERPRINT: &str = "device_fingerprint";
const PROTOCOL_VERSION: &str = "2026-05-13.local-node.v1";
const DEFAULT_LEASE_PUBLIC_KEY_PEM: &str = r#"-----BEGIN PUBLIC KEY-----
MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAvZkEYHN2VhaoCxw2kNSU
hIET4BU1k0ffjB6BRIBrvf73Uo3gX14swZ6TuuLUFvm6ovUDYsv3qYJEOUmwnaXK
xE/QFwhKlny3vhC+g7LI3Pd6zRSTb9x0BwDH1yo6vctBU25o5L24FZ4qG/to/ga0
p6Jla1IjK6kATX7ixsozQExIVaijs6tGW4WVUpizRWMmQL0VI4BpBZHLegvdDUNP
k+s+IPC7WP3o7rl8UCU1LApyKAaQRdFxIym+mgTuKUEAR0/AJ9tPE1ez2XNCjmuN
bJOrzcpKwBnpZOzbu4bIUanfNeCkGySqJeAIT7L/zj1j9j2Wh48mExLa0A77jxBS
lwIDAQAB
-----END PUBLIC KEY-----"#;
const BUILD_COMMIT: &str = match option_env!("GITHUB_SHA") {
    Some(value) => value,
    None => "local-build",
};

fn package_version() -> &'static str {
    option_env!("OZON_LOCAL_NODE_RELEASE_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))
}

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        // Print the real cause as a plain, flushed stderr line so the Tauri
        // supervisor always captures it. The default `Result` Termination impl
        // can race the Terminated event and lose the message on Windows, which
        // is exactly how "sidecar exits code 1" became invisible in the logs.
        eprintln!("local-node fatal: {err:#}");
        let _ = std::io::Write::flush(&mut std::io::stderr());
        std::process::exit(1);
    }
}

async fn run() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ozon_local_node=info,tower_http=info,axum=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let mut config = LocalConfig::from_env();
    config.validate()?;

    // Bind FIRST (with port fallback), then reflect the actual chosen addresses
    // into the config so every manifest/status URL points at the real port.
    let (skill_listener, agent_listener) = bind_local_servers(&config).await?;
    let skill_addr = skill_listener.local_addr()?;
    let agent_addr = agent_listener.local_addr()?;
    config.skill_bind = skill_addr.to_string();
    config.agent_bind = agent_addr.to_string();

    let state = LocalState::new(config.clone())?;

    // Tell the supervisor (and the log) which pair we actually took, so it can
    // attach/display the right port even when the primary was unavailable.
    println!(
        "LOCAL_NODE_PORTS skill={} agent={}",
        skill_addr.port(),
        agent_addr.port()
    );
    tracing::info!(%skill_addr, %agent_addr, "starting local node services");

    let skill = serve_on(skill_listener, skill_router(state.clone()));
    let agent = serve_on(agent_listener, agent_router(state.clone()));
    tokio::try_join!(skill, agent)?;
    Ok(())
}

/// Bind the skill + agent loopback listeners. With an explicit bind override
/// (env-set) it binds exactly those; otherwise it tries PORT_CANDIDATES in order
/// and returns the first pair where BOTH ports are free, so a single busy port
/// no longer leaves the helper permanently offline.
async fn bind_local_servers(config: &LocalConfig) -> anyhow::Result<(TcpListener, TcpListener)> {
    if config.bind_override {
        let skill_addr: SocketAddr = config.skill_bind.parse()?;
        let agent_addr: SocketAddr = config.agent_bind.parse()?;
        let skill = bind_loopback(skill_addr).await?;
        let agent = bind_loopback(agent_addr).await?;
        return Ok((skill, agent));
    }

    let mut last_err: Option<String> = None;
    for (skill_port, agent_port) in PORT_CANDIDATES {
        let skill_addr = SocketAddr::from(([127, 0, 0, 1], *skill_port));
        let agent_addr = SocketAddr::from(([127, 0, 0, 1], *agent_port));
        match bind_loopback(skill_addr).await {
            Ok(skill) => match bind_loopback(agent_addr).await {
                Ok(agent) => return Ok((skill, agent)),
                Err(err) => {
                    // Release the skill port before trying the next pair.
                    drop(skill);
                    last_err = Some(err.to_string());
                }
            },
            Err(err) => last_err = Some(err.to_string()),
        }
    }
    anyhow::bail!(
        "no free loopback port pair (tried {} candidates): {}",
        PORT_CANDIDATES.len(),
        last_err.unwrap_or_else(|| "unknown".to_string())
    )
}

async fn bind_loopback(addr: SocketAddr) -> anyhow::Result<TcpListener> {
    if !addr.ip().is_loopback() {
        anyhow::bail!("local-node refuses to bind non-loopback address: {addr}");
    }
    // Name the address + raw OS error so a port conflict (Windows WSAEADDRINUSE
    // 10048 / Unix 48) is obvious in the log instead of a bare "exit code 1".
    TcpListener::bind(addr).await.map_err(|e| {
        anyhow::anyhow!("failed to bind {addr}: {e} (os error {:?})", e.raw_os_error())
    })
}

async fn serve_on(listener: TcpListener, router: Router) -> anyhow::Result<()> {
    axum::serve(listener, router).await?;
    Ok(())
}

fn skill_router(state: LocalState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/attest", get(attest))
        .route("/diagnostics", get(diagnostics))
        .route("/portal/status", get(portal_status))
        .route("/portal/lease", post(save_portal_lease))
        .route("/openclaw/manifest", get(openclaw_manifest))
        .route("/openclaw/pairing/start", post(start_openclaw_pairing))
        .route("/openclaw/pairing/claim", post(claim_openclaw_pairing))
        .route("/config/status", get(config_status))
        .route("/config/ozon", post(save_ozon_config))
        .route("/config/ozon/validate", post(validate_ozon_config))
        .route("/config/openai", post(save_openai_config))
        .route(
            "/config/registry",
            get(get_model_registry).post(save_model_registry),
        )
        .route("/config/secret", post(save_secret))
        .route("/tools/ozon.products.count", post(ozon_products_count))
        .route("/tools/ozon.products.list", post(ozon_products_list))
        .route("/tools/ozon.products.get", post(ozon_products_get))
        .route("/tools/ozon.relist.generate", post(relist_generate))
        .route("/tools/ozon.relist.push", post(relist_push))
        .route("/tools/ozon.relist.export", post(relist_export))
        .route("/tools/ozon.relist.extract", post(relist_extract))
        .route(
            "/tools/ozon.relist.import-image",
            post(relist_import_image),
        )
        .route(
            "/tools/ozon.module3.recognize",
            post(module3_recognize),
        )
        .route("/tools/ozon.module3.push", post(module3_push))
        .route("/tools/ozon.video.create", post(video_create))
        .route("/tools/ozon.video.get/{id}", get(video_get))
        .route("/poster/brief", post(poster_brief))
        .route("/poster/handoff", post(poster_handoff))
        .route("/poster/generate", post(poster_generate))
        .route("/poster/verify", post(poster_verify))
        .route("/tasks/dry-run", post(create_dry_run))
        .route("/tasks", get(list_tasks))
        .route("/tasks/{id}", get(get_task))
        .route("/tasks/{id}/approve", post(approve_task))
        .route("/tasks/{id}/cancel", post(cancel_task))
        .route("/tasks/{id}/execute-mock", post(execute_mock_task))
        .route("/schedules/ecommerce-read", get(get_ecommerce_schedule))
        .route(
            "/schedules/ecommerce-read",
            post(configure_ecommerce_schedule),
        )
        .route(
            "/schedules/ecommerce-read/run-now",
            post(run_ecommerce_schedule_now),
        )
        .route(
            "/schedules/ecommerce-read/propose",
            post(propose_ecommerce_schedule),
        )
        .layer(local_cors())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

fn agent_router(state: LocalState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/events", get(events))
        .layer(local_cors())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

fn local_cors() -> CorsLayer {
    let extra_origins = configured_openclaw_allowed_origins();
    CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(move |origin: &HeaderValue, _| {
            origin
                .to_str()
                .map(|origin| {
                    origin == "http://localhost:5173"
                        || origin == "http://127.0.0.1:5173"
                        || origin == "http://localhost:5171"
                        || origin == "http://127.0.0.1:5171"
                        || origin == "https://ozon66.com"
                        || origin == "https://www.ozon66.com"
                        || origin == "https://cn.ozon66.com"
                        || origin == "https://ozonclaw.jl696.cn"
                        || origin == "https://www.ozonclaw.jl696.cn"
                        || origin == "http://127.0.0.1:18789"
                        || origin == "http://localhost:18789"
                        || extra_origins.iter().any(|allowed| allowed == origin)
                        || origin.starts_with("tauri://")
                })
                .unwrap_or(false)
        }))
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(tower_http::cors::Any)
        .allow_private_network(true)
}

#[derive(Clone)]
struct LocalConfig {
    /// Effective skill/agent bind. After dynamic binding these hold the actual
    /// chosen address (all manifest/status URLs read them), so a fallback port
    /// is reflected everywhere automatically.
    skill_bind: String,
    agent_bind: String,
    /// True when BOTH bind env vars were set explicitly (tests/dev): bind those
    /// exact ports and do NOT fall back. False => try PORT_CANDIDATES in order.
    bind_override: bool,
    operator_token: String,
    openclaw_token: String,
    use_real_ozon: bool,
    openai_base_url: String,
    openai_image_model: String,
    default_ecommerce_interval_secs: u64,
    default_ecommerce_limit: u16,
    lease_public_key_pem: Option<String>,
    lease_issuer: String,
    lease_audience: String,
    allow_unsigned_lease: bool,
    openclaw_bind_url: String,
}

impl LocalConfig {
    fn from_env() -> Self {
        let use_real_ozon = match env::var("OZON_CONNECTOR_MODE")
            .ok()
            .map(|value| value.to_lowercase())
            .as_deref()
        {
            Some("real") => true,
            Some("mock") => false,
            _ => match env::var("OZON_USE_REAL_API").as_deref() {
                Ok("1") => true,
                Ok("0") => false,
                _ => !cfg!(debug_assertions),
            },
        };
        let skill_override = env::var("OZON_LOCAL_SKILL_BIND").ok();
        let agent_override = env::var("OZON_LOCAL_AGENT_BIND").ok();
        let bind_override = skill_override.is_some() && agent_override.is_some();
        let (default_skill, default_agent) = PORT_CANDIDATES[0];
        Self {
            skill_bind: skill_override
                .unwrap_or_else(|| format!("127.0.0.1:{default_skill}")),
            agent_bind: agent_override
                .unwrap_or_else(|| format!("127.0.0.1:{default_agent}")),
            bind_override,
            operator_token: env::var("OZON_LOCAL_TOKEN")
                .unwrap_or_else(|_| DEFAULT_DEV_LOCAL_TOKEN.to_string()),
            openclaw_token: env::var("OZON_OPENCLAW_TOKEN")
                .unwrap_or_else(|_| DEFAULT_DEV_OPENCLAW_TOKEN.to_string()),
            use_real_ozon,
            openai_base_url: env::var("OPENAI_BASE_URL")
                .or_else(|_| env::var("OPENAI_API_BASE_URL"))
                .unwrap_or_else(|_| DEFAULT_OPENAI_BASE_URL.to_string()),
            openai_image_model: env::var("OPENAI_IMAGE_MODEL")
                .unwrap_or_else(|_| DEFAULT_OPENAI_IMAGE_MODEL.to_string()),
            default_ecommerce_interval_secs: env_u64("OZON_ECOMMERCE_READ_INTERVAL_SECS", 15 * 60),
            default_ecommerce_limit: env_u16("OZON_ECOMMERCE_READ_LIMIT", 20),
            lease_public_key_pem: optional_env("OZON_SUITE_LEASE_PUBLIC_KEY_PEM")
                .or_else(|| read_optional_file_env("OZON_SUITE_LEASE_PUBLIC_KEY_PATH"))
                .or_else(|| option_env!("OZON_SUITE_LEASE_PUBLIC_KEY_PEM").map(str::to_string))
                .or_else(|| Some(DEFAULT_LEASE_PUBLIC_KEY_PEM.to_string())),
            lease_issuer: env::var("OZON_SUITE_LEASE_ISSUER")
                .unwrap_or_else(|_| "ozon66-cloud".to_string()),
            lease_audience: env::var("OZON_SUITE_LEASE_AUDIENCE")
                .unwrap_or_else(|_| "ozon-rust-local-node".to_string()),
            allow_unsigned_lease: env::var("OZON_LOCAL_ALLOW_UNSIGNED_LEASE")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(cfg!(debug_assertions)),
            openclaw_bind_url: env::var("OZON_OPENCLAW_BIND_URL")
                .unwrap_or_else(|_| DEFAULT_OPENCLAW_BIND_URL.to_string()),
        }
    }

    fn validate(&self) -> anyhow::Result<()> {
        validate_openclaw_bind_url(&self.openclaw_bind_url)?;
        if self.use_real_ozon
            && (self.operator_token == DEFAULT_DEV_LOCAL_TOKEN
                || self.openclaw_token == DEFAULT_DEV_OPENCLAW_TOKEN)
        {
            anyhow::bail!(
                "OZON_LOCAL_TOKEN and OZON_OPENCLAW_TOKEN must be explicitly set when the real Ozon connector is enabled"
            );
        }
        if self.use_real_ozon && !self.allow_unsigned_lease {
            let Some(public_key_pem) = self.lease_public_key_pem.as_deref() else {
                anyhow::bail!("lease public key must be configured when real Ozon mode is enabled");
            };
            RsaPublicKey::from_public_key_pem(public_key_pem).map_err(|_| {
                anyhow::anyhow!("lease public key must be a valid RSA public key PEM")
            })?;
        }
        Ok(())
    }
}

#[derive(Clone)]
struct LocalState {
    config: LocalConfig,
    tasks: TaskStore,
    secrets: Arc<dyn SecretStore>,
    ozon_config_cache: Arc<RwLock<Option<StoredOzonConfig>>>,
    openai_config_cache: Arc<RwLock<Option<StoredOpenAiConfig>>>,
    model_registry_cache: Arc<RwLock<Option<StoredModelRegistry>>>,
    cloud_lease_cache: Arc<RwLock<Option<EntitlementLease>>>,
    ozon_connector: Arc<dyn OzonReadConnector>,
    ozon_writer: Arc<dyn OzonWriteConnector>,
    http_client: reqwest::Client,
    schedules: ScheduleStore,
    openclaw_pairings: OpenClawPairingStore,
    video_jobs: VideoJobStore,
}

impl LocalState {
    fn new(config: LocalConfig) -> anyhow::Result<Self> {
        Self::new_with_secret_store(config, default_secret_store()?)
    }

    fn new_with_secret_store(
        config: LocalConfig,
        secrets: Arc<dyn SecretStore>,
    ) -> anyhow::Result<Self> {
        // Build the read + write connectors from one concrete instance so the
        // relist workbench can push images through the same client the read
        // tools use (real Ozon HTTP client, or the in-process mock in debug).
        let (ozon_connector, ozon_writer): (
            Arc<dyn OzonReadConnector>,
            Arc<dyn OzonWriteConnector>,
        ) = if config.use_real_ozon {
            let client = Arc::new(ozon_connector::OzonHttpClient::new());
            (client.clone(), client)
        } else {
            if !cfg!(debug_assertions) {
                anyhow::bail!(
                    "mock Ozon connector is disabled in non-debug builds; set OZON_CONNECTOR_MODE=real"
                );
            }
            let mock = Arc::new(MockOzonConnector);
            (mock.clone(), mock)
        };
        let schedules = Arc::new(RwLock::new(EcommerceReadSchedule {
            interval_secs: config
                .default_ecommerce_interval_secs
                .clamp(60, 24 * 60 * 60),
            limit: config.default_ecommerce_limit.clamp(1, 100),
            ..EcommerceReadSchedule::default()
        }));
        Ok(Self {
            config,
            tasks: TaskStore::new(),
            secrets,
            ozon_config_cache: Arc::new(RwLock::new(None)),
            openai_config_cache: Arc::new(RwLock::new(None)),
            model_registry_cache: Arc::new(RwLock::new(None)),
            cloud_lease_cache: Arc::new(RwLock::new(None)),
            ozon_connector,
            ozon_writer,
            http_client: reqwest::Client::builder()
                .user_agent("ozon-rust-suite-local/0.1")
                // Image-edit (gpt-image-2) can take a couple of minutes per image,
                // so allow a generous ceiling; normal calls still return fast.
                .timeout(Duration::from_secs(300))
                .build()
                .map_err(|error| anyhow::anyhow!("failed to build HTTP client: {error}"))?,
            schedules,
            openclaw_pairings: Arc::new(RwLock::new(HashMap::new())),
            video_jobs: Arc::new(RwLock::new(HashMap::new())),
        })
    }
}

fn default_secret_store() -> anyhow::Result<Arc<dyn SecretStore>> {
    let file_store: Arc<dyn SecretStore> =
        Arc::new(FileSecretStore::new(default_secret_file_path()));
    let system_store = match SystemSecretStore::new("ozon-rust-suite-local", "default") {
        Ok(store) => store,
        Err(_) => return Ok(file_store),
    };
    Ok(Arc::new(LayeredSecretStore::new(
        Arc::new(system_store),
        file_store,
    )))
}

fn default_secret_file_path() -> PathBuf {
    if let Ok(path) = env::var("OZON_LOCAL_SECRET_FILE") {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = env::var("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("com.ozonrustsuite.local")
                .join("local-node-private-secrets.json");
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = env::var("APPDATA") {
            return PathBuf::from(appdata)
                .join("Ozon Rust Suite")
                .join("local-node-private-secrets.json");
        }
    }

    if let Ok(config_home) = env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(config_home)
            .join("ozon-rust-suite")
            .join("local-node-private-secrets.json");
    }

    if let Ok(home) = env::var("HOME") {
        return PathBuf::from(home)
            .join(".config")
            .join("ozon-rust-suite")
            .join("local-node-private-secrets.json");
    }

    PathBuf::from("local-node-private-secrets.json")
}

type ScheduleStore = Arc<RwLock<EcommerceReadSchedule>>;
type OpenClawPairingStore = Arc<RwLock<HashMap<String, OpenClawPairing>>>;

#[derive(Debug)]
struct OpenClawPairing {
    expires_at: Instant,
    expires_at_rfc3339: String,
}

#[derive(Debug)]
struct EcommerceReadSchedule {
    enabled: bool,
    interval_secs: u64,
    limit: u16,
    last_run: Option<EcommerceReadRun>,
    last_error: Option<String>,
    audit: Vec<ScheduleAuditEvent>,
    worker: Option<JoinHandle<()>>,
}

impl Default for EcommerceReadSchedule {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_secs: 15 * 60,
            limit: 20,
            last_run: None,
            last_error: None,
            audit: Vec::new(),
            worker: None,
        }
    }
}

async fn health(State(state): State<LocalState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        service: "ozon-local-node",
        status: "ok",
        skill_port: local_port(&state.config.skill_bind),
        agent_port: local_port(&state.config.agent_bind),
        protocol_version: PROTOCOL_VERSION,
        build_commit: BUILD_COMMIT,
        package_version: package_version(),
        supervisor: "tauri-sidecar",
        features: vec![
            Feature::OzonRead,
            Feature::OzonWriteMock,
            Feature::DraftImport1688Mock,
            Feature::OpenClawBridge,
            Feature::LocalApproval,
        ],
        real_ozon_enabled: state.config.use_real_ozon,
    })
}

async fn attest(
    State(state): State<LocalState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Proof-of-possession: returns HMAC-SHA256(operator_token, nonce) so a caller can verify this
    // node holds the shared operator token WITHOUT ever transmitting the token. The Tauri shell
    // challenges a (possibly pre-existing) node with a fresh nonce and only trusts — and only then
    // sends the token to — a node that returns the correct proof, so a port squatter that lacks
    // the token cannot impersonate the node and harvest it. HMAC is a PRF, so serving proofs does
    // not reveal the token.
    let nonce = headers
        .get("x-attest-nonce")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::bad_request("missing x-attest-nonce"))?;
    if nonce.len() > 256 {
        return Err(ApiError::bad_request("attestation nonce is too long"));
    }
    Ok(Json(serde_json::json!({
        "proof": attest_proof(&state.config.operator_token, nonce),
    })))
}

/// HMAC-SHA256(operator_token, nonce), lowercase hex. Must stay byte-for-byte identical to the
/// verifier in the Tauri shell (`attest_proof`).
fn attest_proof(token: &str, nonce: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    use std::fmt::Write as _;

    let mut mac =
        Hmac::<Sha256>::new_from_slice(token.as_bytes()).expect("HMAC accepts a key of any size");
    mac.update(nonce.as_bytes());
    let bytes = mac.finalize().into_bytes();
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

async fn diagnostics(State(state): State<LocalState>) -> Json<DiagnosticsResponse> {
    let ozon = inspect_ozon_credentials(&state).await;
    let openai = inspect_openai_config(&state).await;
    let lease = inspect_cloud_lease(&state).await;
    let manifest_url = format!(
        "{}/openclaw/manifest",
        local_http_url(&state.config.skill_bind)
    );
    Json(DiagnosticsResponse {
        service: "ozon-local-node",
        status: "ok",
        checked_at: Utc::now().to_rfc3339(),
        protocol_version: PROTOCOL_VERSION,
        build_commit: BUILD_COMMIT,
        package_version: package_version(),
        skill_api: local_http_url(&state.config.skill_bind),
        agent_api: local_http_url(&state.config.agent_bind),
        connector_mode: connector_mode(&state),
        real_ozon_enabled: state.config.use_real_ozon,
        secret_store: SecretStoreStatus {
            backend: "system_keyring+local_file",
            available: ozon.secret_store_available,
        },
        ozon: OzonCredentialStatus {
            configured: ozon.configured,
            source: ozon.source,
            client_id: ozon.client_id,
            api_key_fingerprint: ozon.api_key_fingerprint,
            issue: ozon.issue,
        },
        poster_generation: poster_generation_status(&openai, manifest_url),
        openai,
        lease,
        capabilities: inspect_capabilities(&state).await,
    })
}

async fn portal_status(
    State(state): State<LocalState>,
) -> Result<Json<PortalStatusResponse>, ApiError> {
    let device_fingerprint = load_or_create_device_fingerprint(&state).await?;
    let ozon = inspect_ozon_credentials(&state).await;
    let openai = inspect_openai_config(&state).await;
    let lease = inspect_cloud_lease(&state).await;
    let manifest_url = format!(
        "{}/openclaw/manifest",
        local_http_url(&state.config.skill_bind)
    );
    let poster_generation = poster_generation_status(&openai, manifest_url.clone());
    Ok(Json(PortalStatusResponse {
        service: "ozon-local-node",
        status: "online",
        checked_at: Utc::now().to_rfc3339(),
        skill_api: local_http_url(&state.config.skill_bind),
        agent_api: local_http_url(&state.config.agent_bind),
        manifest_url: manifest_url.clone(),
        bridge_auth_header: "x-openclaw-token",
        protocol_version: PROTOCOL_VERSION,
        build_commit: BUILD_COMMIT,
        package_version: package_version(),
        real_ozon_enabled: state.config.use_real_ozon,
        device_fingerprint,
        ozon: PortalCredentialStatus {
            configured: ozon.configured,
            issue: ozon.issue,
        },
        openai: PortalOpenAiStatus {
            configured: openai.configured,
            image_model: openai.image_model,
            issue: openai.issue,
        },
        poster_generation,
        lease,
        features: vec![
            Feature::OzonRead,
            Feature::OzonWriteMock,
            Feature::DraftImport1688Mock,
            Feature::OpenClawBridge,
            Feature::LocalApproval,
        ],
    }))
}

async fn openclaw_manifest(State(state): State<LocalState>) -> Json<OpenClawManifest> {
    Json(OpenClawManifest {
        name: "ozon-rust-suite-local",
        version: package_version(),
        description: "Local Ozon seller automation bridge with dry-run and approval enforcement",
        base_url: local_http_url(&state.config.skill_bind),
        auth: OpenClawAuth {
            header: "x-openclaw-token",
            source: "operator_configured_bridge_secret",
        },
        tools: vec![
            OpenClawTool {
                name: "ozon.products.count",
                method: "POST",
                path: "/tools/ozon.products.count",
                risk: "read_only",
                approval_required: false,
                description: "Count Ozon products through the configured connector; real mode uses saved Ozon Seller API credentials",
            },
            OpenClawTool {
                name: "ozon.products.list",
                method: "POST",
                path: "/tools/ozon.products.list",
                risk: "read_only",
                approval_required: false,
                description: "List Ozon product summaries with a bounded limit",
            },
            OpenClawTool {
                name: "ozon.products.get",
                method: "POST",
                path: "/tools/ozon.products.get",
                risk: "read_only",
                approval_required: false,
                description: "Read one Ozon product fact pack with stable details and image URLs",
            },
            OpenClawTool {
                name: "ozon.module3.recognize",
                method: "POST",
                path: "/tools/ozon.module3.recognize",
                risk: "read_only",
                approval_required: false,
                description: "Read one Ozon product (title/description/attributes/category) plus its images and return a Russian, redistributed copy proposal; nothing is written back to Ozon",
            },
            OpenClawTool {
                name: "ozon.module3.push",
                method: "POST",
                path: "/tools/ozon.module3.push",
                risk: "write",
                approval_required: true,
                description: "Write a reviewed module-3 copy proposal (title/description/attributes) back to the LIVE Ozon store. Always returns a dry-run preview (before->after, matched + dropped attributes); only writes when called with confirm=true after operator review. Attribute names/values are re-matched against the Ozon category dictionary and any unmatched item is dropped (never written with a guessed id)",
            },
            OpenClawTool {
                name: "ozon.video.create",
                method: "POST",
                path: "/tools/ozon.video.create",
                risk: "read_only",
                approval_required: false,
                description: "Module 6 cloud image-to-video: create an async generation job from a first-frame image URL (and optional last-frame) plus a prompt. Read-only — nothing is written to Ozon in v1; returns our local job id immediately while a bounded background poller tracks the provider job. Requires a video provider configured in the model registry",
            },
            OpenClawTool {
                name: "ozon.video.get",
                method: "GET",
                path: "/tools/ozon.video.get/{job_id}",
                risk: "read_only",
                approval_required: false,
                description: "Module 6: read one cloud image-to-video job status (Queued/Running/Succeeded/Failed) and, when succeeded, the hosted video URL for operator review. No Ozon write",
            },
            OpenClawTool {
                name: "ozon.relist.export",
                method: "POST",
                path: "/tools/ozon.relist.export",
                risk: "read_only",
                approval_required: false,
                description: "Module 4 export/delivery: inject reviewed per-row title/listing/image URLs into an Ozon template .xlsx and run the engine process --verify to write a deliverable workbook locally (NO Ozon push). Returns the deliverable file path + verify summary; a verification failure (frozen-cell change) is a hard error and the file is never returned. Needs the local Python engine (dev uses the repo .venv)",
            },
            OpenClawTool {
                name: "ozon.relist.extract",
                method: "POST",
                path: "/tools/ozon.relist.extract",
                risk: "read_only",
                approval_required: false,
                description: "Module 1 intake: read a supplier .xlsx with the local Python engine and return per-row {sheet,row,sku,title,listing,images_main,images_additional} JSON for review/merge into the relist list. READ-ONLY — never writes a workbook or pushes to Ozon",
            },
            OpenClawTool {
                name: "ozon.relist.import-image",
                method: "POST",
                path: "/tools/ozon.relist.import-image",
                risk: "read_only",
                approval_required: false,
                description: "Module 1 intake: accept an operator-dragged PNG image as base64-in-JSON, host it on a public image host, and return the hosted URL so it can be set as a relist candidate. READ-ONLY — nothing is pushed to Ozon",
            },
            OpenClawTool {
                name: "poster.handoff",
                method: "POST",
                path: "/poster/handoff",
                risk: "read_only",
                approval_required: false,
                description: "Return a product-grounded poster package for OpenClaw/Codex generation; no OpenAI API key required",
            },
            OpenClawTool {
                name: "tasks.dry_run",
                method: "POST",
                path: "/tasks/dry-run",
                risk: "proposal_only",
                approval_required: true,
                description: "Create a proposed task; write operations remain pending until local approval",
            },
            OpenClawTool {
                name: "tasks.get",
                method: "GET",
                path: "/tasks/{task_id}",
                risk: "read_only",
                approval_required: false,
                description: "Read task status after a proposal has been created",
            },
            OpenClawTool {
                name: "schedules.ecommerce_read.propose",
                method: "POST",
                path: "/schedules/ecommerce-read/propose",
                risk: "proposal_only",
                approval_required: true,
                description: "Propose a bounded read-only Ozon product polling schedule; operator token must enable it",
            },
        ],
        safety_rules: vec![
            "Bind only to 127.0.0.1",
            "Require x-openclaw-token for bridge read and proposal calls",
            "AI/OpenClaw may propose tasks but cannot approve or execute writes",
            "Local approval, cancellation, execution, config, and diagnostics require x-local-token",
            "Write operations default to dry-run and require explicit local approval",
            "Mock executor never sends real Ozon write requests",
            "OpenClaw may propose read-only schedules but cannot enable, disable, or run schedules",
            "Scheduled e-commerce reads use official Ozon seller APIs only; no 1688 live scraping or captcha bypass",
        ],
    })
}

async fn start_openclaw_pairing(
    State(state): State<LocalState>,
    headers: HeaderMap,
) -> Result<Json<OpenClawPairingStartResponse>, ApiError> {
    require_operator_token(&state, &headers)?;
    let now = Instant::now();
    let code = Uuid::new_v4().simple().to_string();
    let expires_at_rfc3339 = (Utc::now()
        + chrono::Duration::seconds(OPENCLAW_PAIRING_TTL.as_secs() as i64))
    .to_rfc3339();
    let claim_url = format!(
        "{}/openclaw/pairing/claim",
        local_http_url(&state.config.skill_bind)
    );
    let manifest_url = format!(
        "{}/openclaw/manifest",
        local_http_url(&state.config.skill_bind)
    );
    let bind_url = build_openclaw_bind_url(
        &state.config.openclaw_bind_url,
        &code,
        &claim_url,
        &manifest_url,
    )?;

    let mut pairings = state.openclaw_pairings.write().await;
    pairings.retain(|_, pairing| pairing.expires_at > now);
    pairings.insert(
        code.clone(),
        OpenClawPairing {
            expires_at: now + OPENCLAW_PAIRING_TTL,
            expires_at_rfc3339: expires_at_rfc3339.clone(),
        },
    );

    Ok(Json(OpenClawPairingStartResponse {
        status: "pairing_started",
        bind_url,
        pairing_code: code,
        claim_url,
        manifest_url,
        auth_header: "x-openclaw-token",
        expires_at: expires_at_rfc3339,
        instructions: vec![
            "Open the bind URL in Longxia/OpenClaw.".to_string(),
            "Longxia should read the URL fragment and POST the pairing code to claim_url."
                .to_string(),
            "The long-lived bridge token is never embedded in the bind URL; OpenClaw stores it only after a trusted localhost claim.".to_string(),
        ],
    }))
}

async fn claim_openclaw_pairing(
    State(state): State<LocalState>,
    headers: HeaderMap,
    Json(input): Json<OpenClawPairingClaimRequest>,
) -> Result<Json<OpenClawPairingClaimResponse>, ApiError> {
    require_openclaw_pairing_origin(&headers)?;
    let code = input.code.trim();
    if code.is_empty() {
        return Err(ApiError::bad_request("pairing code is required"));
    }
    let now = Instant::now();
    let mut pairings = state.openclaw_pairings.write().await;
    pairings.retain(|_, pairing| pairing.expires_at > now);
    let pairing = pairings
        .remove(code)
        .ok_or_else(|| ApiError::unauthorized("pairing code is invalid or expired"))?;
    let manifest_url = format!(
        "{}/openclaw/manifest",
        local_http_url(&state.config.skill_bind)
    );

    Ok(Json(OpenClawPairingClaimResponse {
        status: "paired",
        manifest_url,
        base_url: local_http_url(&state.config.skill_bind),
        auth_header: "x-openclaw-token",
        auth_token: state.config.openclaw_token.clone(),
        auth_token_fingerprint: fingerprint_secret(&SecretString::from(
            state.config.openclaw_token.clone(),
        )),
        expires_at: pairing.expires_at_rfc3339,
        safety_rules: vec![
            "Use the token only inside Longxia/OpenClaw connector settings.".to_string(),
            "Do not paste the token into chats, prompts, logs, or public documents.".to_string(),
            "Bridge calls are read/proposal only; local approval is required for writes."
                .to_string(),
        ],
    }))
}

async fn save_ozon_config(
    State(state): State<LocalState>,
    headers: HeaderMap,
    Json(input): Json<OzonConfigRequest>,
) -> Result<Json<OzonConfigResponse>, ApiError> {
    require_operator_token(&state, &headers)?;
    let client_id = input.client_id.trim();
    let api_key = input.api_key.trim();
    if client_id.is_empty() || api_key.is_empty() {
        return Err(ApiError::bad_request(
            "Ozon Client ID and API Key are required",
        ));
    }
    if client_id.eq_ignore_ascii_case("mock-client-id")
        || api_key.eq_ignore_ascii_case("mock-api-key")
        || client_id.eq_ignore_ascii_case("debug-local-client-id")
        || api_key.eq_ignore_ascii_case("debug-local-api-key")
    {
        return Err(ApiError::bad_request(
            "debug mock Ozon credentials cannot be saved",
        ));
    }
    let bundle = StoredOzonConfig {
        client_id: client_id.to_string(),
        api_key: api_key.to_string(),
    };
    let bundle_json = serde_json::to_string(&bundle)
        .map_err(|_| ApiError::internal("failed to serialize Ozon config"))?;
    state
        .secrets
        .put(
            SecretName::new(SECRET_OZON_CONFIG),
            SecretString::from(bundle_json),
        )
        .await
        .map_err(|_| ApiError::internal("failed to save Ozon config"))?;
    *state.ozon_config_cache.write().await = Some(bundle);
    Ok(Json(OzonConfigResponse {
        client_id: redact(client_id),
        api_key: redact(api_key),
        saved_at: Utc::now().to_rfc3339(),
    }))
}

async fn save_openai_config(
    State(state): State<LocalState>,
    headers: HeaderMap,
    Json(input): Json<OpenAiConfigRequest>,
) -> Result<Json<OpenAiConfigResponse>, ApiError> {
    require_operator_token(&state, &headers)?;
    let api_key_input = input.api_key.trim();
    let stored_config = if api_key_input.is_empty() {
        Some(load_persisted_openai_config(&state).await.map_err(|_| {
            ApiError::bad_request("OpenAI API key is required the first time you save this config")
        })?)
    } else {
        None
    };
    let api_key = stored_config
        .as_ref()
        .map(|config| config.api_key.as_str())
        .unwrap_or(api_key_input);
    let base_url = normalize_openai_base_url(
        input
            .base_url
            .as_deref()
            .unwrap_or(&state.config.openai_base_url),
    )?;
    let image_model = input
        .image_model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&state.config.openai_image_model)
        .to_string();
    let bundle = StoredOpenAiConfig {
        api_key: api_key.to_string(),
        base_url,
        image_model,
    };
    let bundle_json = serde_json::to_string(&bundle)
        .map_err(|_| ApiError::internal("failed to serialize OpenAI config"))?;
    state
        .secrets
        .put(
            SecretName::new(SECRET_OPENAI_CONFIG),
            SecretString::from(bundle_json),
        )
        .await
        .map_err(|_| ApiError::internal("failed to save OpenAI config"))?;
    *state.openai_config_cache.write().await = Some(bundle.clone());
    Ok(Json(OpenAiConfigResponse {
        base_url: bundle.base_url,
        image_model: bundle.image_model,
        api_key_fingerprint: fingerprint_secret(&SecretString::from(api_key.to_string())),
        saved_at: Utc::now().to_rfc3339(),
    }))
}

async fn save_model_registry(
    State(state): State<LocalState>,
    headers: HeaderMap,
    Json(input): Json<StoredModelRegistry>,
) -> Result<Json<ModelRegistryResponse>, ApiError> {
    require_operator_token(&state, &headers)?;

    // Validate + normalize every entry across all capabilities. The same
    // loopback-only-http rule applies regardless of auth style, since
    // Header/Query auth also transmits the key on the wire.
    let mut validated = StoredModelRegistry::default();
    for (entries, target) in [
        (&input.image_gen, &mut validated.image_gen),
        (&input.text_gen, &mut validated.text_gen),
        (&input.video_gen, &mut validated.video_gen),
    ] {
        for entry in entries {
            let secret_ref = entry.secret_ref.trim();
            if secret_ref.is_empty() {
                return Err(ApiError::bad_request("provider secret_ref must not be empty"));
            }
            // Reject a ref that does not resolve to a key right now.
            resolve_secret_ref(&state, secret_ref).await?;
            let base_url = normalize_provider_base_url(&entry.base_url)?;
            target.push(ProviderEntry {
                kind: entry.kind,
                base_url,
                model: entry.model.clone(),
                secret_ref: secret_ref.to_string(),
                auth: entry.auth.clone(),
                enabled: entry.enabled,
                video_dialect: entry.video_dialect,
                extra: entry.extra.clone(),
            });
        }
    }

    let bundle_json = serde_json::to_string(&validated)
        .map_err(|_| ApiError::internal("failed to serialize model registry"))?;
    state
        .secrets
        .put(
            SecretName::new(SECRET_MODEL_REGISTRY),
            SecretString::from(bundle_json),
        )
        .await
        .map_err(|_| ApiError::internal("failed to save model registry"))?;
    *state.model_registry_cache.write().await = Some(validated.clone());

    Ok(Json(ModelRegistryResponse {
        ok: true,
        image_gen_entries: validated.image_gen.len(),
        text_gen_entries: validated.text_gen.len(),
        video_gen_entries: validated.video_gen.len(),
        saved_at: Utc::now().to_rfc3339(),
    }))
}

/// Reserved secret names that back the node's own config blobs. An operator may
/// NOT overwrite these through the generic /config/secret endpoint.
const RESERVED_SECRET_NAMES: &[&str] = &[
    SECRET_OZON_CONFIG,
    SECRET_OPENAI_CONFIG,
    SECRET_MODEL_REGISTRY,
    SECRET_CLOUD_LEASE,
    SECRET_DEVICE_FINGERPRINT,
];

async fn save_secret(
    State(state): State<LocalState>,
    headers: HeaderMap,
    Json(input): Json<SecretSaveRequest>,
) -> Result<Json<SecretSaveResponse>, ApiError> {
    require_operator_token(&state, &headers)?;

    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(ApiError::bad_request("secret name is required"));
    }
    // Policy: lowercase ascii, digits, underscore only — the same shape registry
    // secret_refs use. Rejecting anything else keeps refs predictable + safe.
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return Err(ApiError::bad_request(
            "secret name must match [a-z0-9_]+ (lowercase letters, digits, underscore)",
        ));
    }
    if RESERVED_SECRET_NAMES.contains(&name.as_str()) {
        return Err(ApiError::bad_request(format!(
            "'{name}' is a reserved secret name and cannot be set here"
        )));
    }

    let value = input.value;
    if value.is_empty() {
        return Err(ApiError::bad_request("secret value is required"));
    }
    let secret = SecretString::from(value);
    let fingerprint = fingerprint_secret(&secret);
    state
        .secrets
        .put(SecretName::new(name.clone()), secret)
        .await
        .map_err(|_| ApiError::internal("failed to save secret"))?;

    Ok(Json(SecretSaveResponse {
        name,
        fingerprint,
        saved_at: Utc::now().to_rfc3339(),
    }))
}

/// Read-only view of the persisted model registry so the provider UI can fetch,
/// merge one entry, and re-POST the full registry. Includes secret_ref + auth but
/// NEVER a raw key (the registry blob never stores one — only secret_ref).
async fn get_model_registry(
    State(state): State<LocalState>,
    headers: HeaderMap,
) -> Result<Json<StoredModelRegistry>, ApiError> {
    require_operator_token(&state, &headers)?;
    let registry = load_persisted_model_registry(&state).await;
    Ok(Json(registry))
}

async fn save_portal_lease(
    State(state): State<LocalState>,
    Json(input): Json<PortalLeaseRequest>,
) -> Result<Json<PortalLeaseResponse>, ApiError> {
    validate_cloud_lease_with_feature(&state, &input.lease, Feature::OzonRead)?;
    let device_fingerprint = load_or_create_device_fingerprint(&state).await?;
    ensure_lease_bound_to_device(&input.lease, &device_fingerprint)?;
    let lease_json = serde_json::to_string(&input.lease)
        .map_err(|_| ApiError::internal("failed to serialize lease"))?;
    state
        .secrets
        .put(
            SecretName::new(SECRET_CLOUD_LEASE),
            SecretString::from(lease_json),
        )
        .await
        .map_err(|_| ApiError::internal("failed to save cloud lease"))?;
    *state.cloud_lease_cache.write().await = Some(input.lease.clone());
    Ok(Json(PortalLeaseResponse {
        accepted: true,
        lease: lease_status(&state, &input.lease),
        saved_at: Utc::now().to_rfc3339(),
    }))
}

async fn config_status(
    State(state): State<LocalState>,
    headers: HeaderMap,
) -> Result<Json<ConfigStatusResponse>, ApiError> {
    require_operator_token(&state, &headers)?;
    let ozon = inspect_ozon_credentials(&state).await;
    let openai = inspect_openai_config(&state).await;
    let manifest_url = format!(
        "{}/openclaw/manifest",
        local_http_url(&state.config.skill_bind)
    );
    let poster_generation = poster_generation_status(&openai, manifest_url.clone());
    Ok(Json(ConfigStatusResponse {
        service: "ozon-local-node",
        checked_at: Utc::now().to_rfc3339(),
        real_ozon_enabled: state.config.use_real_ozon,
        connector_mode: connector_mode(&state),
        secret_store: SecretStoreStatus {
            backend: "system_keyring+local_file",
            available: ozon.secret_store_available,
        },
        ozon: OzonCredentialStatus {
            configured: ozon.configured,
            source: ozon.source,
            client_id: ozon.client_id,
            api_key_fingerprint: ozon.api_key_fingerprint,
            issue: ozon.issue,
        },
        poster_generation,
        openai,
        lease: inspect_cloud_lease(&state).await,
        endpoints: LocalEndpointStatus {
            skill_api: local_http_url(&state.config.skill_bind),
            agent_api: local_http_url(&state.config.agent_bind),
            manifest_url,
        },
        capabilities: inspect_capabilities(&state).await,
    }))
}

async fn validate_ozon_config(
    State(state): State<LocalState>,
    headers: HeaderMap,
) -> Result<Json<OzonCredentialValidationResponse>, ApiError> {
    require_operator_token(&state, &headers)?;
    let credentials = load_ozon_credentials(&state).await?;
    state
        .ozon_connector
        .validate_credentials(&credentials)
        .await
        .map_err(|error| {
            ApiError::bad_gateway(format!("ozon credential validation failed: {error}"))
        })?;
    Ok(Json(OzonCredentialValidationResponse {
        ok: true,
        checked_at: Utc::now().to_rfc3339(),
        connector_mode: connector_mode(&state),
        message: if state.config.use_real_ozon {
            "real Ozon read-only credential validation succeeded"
        } else {
            "mock connector validation succeeded; set OZON_CONNECTOR_MODE=real for real Ozon validation"
        },
    }))
}

async fn ozon_products_count(
    State(state): State<LocalState>,
    headers: HeaderMap,
) -> Result<Json<ProductCountResponse>, ApiError> {
    require_bridge_or_operator_token(&state, &headers)?;
    require_valid_lease_with_feature(&state, Feature::OzonRead).await?;
    let credentials = load_ozon_credentials(&state).await?;
    let mut count = state
        .ozon_connector
        .product_count(&credentials)
        .await
        .map_err(|error| ApiError::bad_gateway(format!("ozon connector failed: {error}")))?;
    let mut visibility = "ALL".to_string();
    let mut archived_fallback = false;
    if count == 0 {
        let archived_page = state
            .ozon_connector
            .product_list_page_with_visibility(&credentials, 1, None, Some("ARCHIVED".into()))
            .await
            .map_err(|error| {
                ApiError::bad_gateway(format!("ozon archived connector failed: {error}"))
            })?;
        if archived_page.total > 0 {
            count = archived_page.total;
            visibility = "ARCHIVED".to_string();
            archived_fallback = true;
        }
    }
    Ok(Json(ProductCountResponse {
        count,
        visibility,
        archived_fallback,
    }))
}

async fn ozon_products_list(
    State(state): State<LocalState>,
    headers: HeaderMap,
    Json(input): Json<ProductListRequest>,
) -> Result<Json<ProductListResponse>, ApiError> {
    require_bridge_or_operator_token(&state, &headers)?;
    require_valid_lease_with_feature(&state, Feature::OzonRead).await?;
    let credentials = load_ozon_credentials(&state).await?;
    let limit = input.limit.unwrap_or(20);
    let requested_visibility = normalize_product_list_visibility(input.visibility)?;
    let mut resolved_visibility = requested_visibility
        .clone()
        .unwrap_or_else(|| "ALL".to_string());
    let mut archived_fallback = false;
    let mut products = state
        .ozon_connector
        .product_list_page_with_visibility(
            &credentials,
            limit,
            input.last_id,
            requested_visibility.clone(),
        )
        .await
        .map_err(|error| ApiError::bad_gateway(format!("ozon connector failed: {error}")))?;
    if requested_visibility.is_none()
        && input.include_archived_if_empty.unwrap_or(true)
        && products.total == 0
        && products.products.is_empty()
    {
        products = state
            .ozon_connector
            .product_list_page_with_visibility(&credentials, limit, None, Some("ARCHIVED".into()))
            .await
            .map_err(|error| {
                ApiError::bad_gateway(format!("ozon archived connector failed: {error}"))
            })?;
        if products.total > 0 || !products.products.is_empty() {
            resolved_visibility = "ARCHIVED".to_string();
            archived_fallback = true;
        }
    }
    Ok(Json(ProductListResponse {
        connector_mode: connector_mode(&state),
        products: products.products,
        total: products.total,
        last_id: products.last_id,
        visibility: resolved_visibility,
        archived_fallback,
    }))
}

async fn ozon_products_get(
    State(state): State<LocalState>,
    headers: HeaderMap,
    Json(input): Json<ProductGetRequest>,
) -> Result<Json<ProductGetResponse>, ApiError> {
    require_bridge_or_operator_token(&state, &headers)?;
    require_valid_lease_with_feature(&state, Feature::OzonRead).await?;
    let credentials = load_ozon_credentials(&state).await?;
    let product = state
        .ozon_connector
        .product_get(&credentials, input.into_lookup())
        .await
        .map_err(|error| map_product_get_error("ozon connector failed", error))?;
    Ok(Json(ProductGetResponse {
        connector_mode: connector_mode(&state),
        product,
    }))
}

async fn poster_brief(
    State(state): State<LocalState>,
    headers: HeaderMap,
    Json(input): Json<PosterBriefRequest>,
) -> Result<Json<PosterBriefResponse>, ApiError> {
    require_bridge_or_operator_token(&state, &headers)?;
    let PosterBriefRequest {
        lookup,
        theme,
        locale,
    } = input;
    let brief = build_poster_brief(
        &state,
        load_product_for_lookup(&state, lookup.into_lookup()).await?,
        theme.as_deref().unwrap_or("studio"),
        locale.as_deref().unwrap_or("zh-CN"),
    )?;
    Ok(Json(PosterBriefResponse {
        connector_mode: connector_mode(&state),
        product: brief.product.clone(),
        brief: brief.brief,
    }))
}

async fn poster_handoff(
    State(state): State<LocalState>,
    headers: HeaderMap,
    Json(input): Json<PosterBriefRequest>,
) -> Result<Json<PosterHandoffResponse>, ApiError> {
    require_bridge_or_operator_token(&state, &headers)?;
    let PosterBriefRequest {
        lookup,
        theme,
        locale,
    } = input;
    let locale = locale.as_deref().unwrap_or("zh-CN");
    let poster = build_poster_brief(
        &state,
        load_product_for_lookup(&state, lookup.into_lookup()).await?,
        theme.as_deref().unwrap_or("studio"),
        locale,
    )?;
    let source_images = poster_source_images(&poster.product, locale);
    let prompt =
        build_openclaw_poster_prompt(&poster.product, &poster.brief, &source_images, locale);
    Ok(Json(PosterHandoffResponse {
        connector_mode: connector_mode(&state),
        generated_at: Utc::now().to_rfc3339(),
        mode: "openclaw_codex",
        product: poster.product,
        brief: poster.brief,
        source_images,
        openclaw: PosterOpenClawHandoff {
            manifest_url: format!(
                "{}/openclaw/manifest",
                local_http_url(&state.config.skill_bind)
            ),
            auth_header: "x-openclaw-token",
            token_policy: "Do not paste the bridge token into public prompts. Configure it only inside OpenClaw/Codex connector settings.",
            recommended_tools: vec!["ozon.products.get", "poster.handoff"],
        },
        instructions: poster_handoff_instructions(locale),
        prompt,
    }))
}

async fn poster_generate(
    State(state): State<LocalState>,
    headers: HeaderMap,
    Json(input): Json<PosterBriefRequest>,
) -> Result<Json<PosterGenerateResponse>, ApiError> {
    require_operator_token(&state, &headers)?;
    let PosterBriefRequest {
        lookup,
        theme,
        locale,
    } = input;
    let poster = build_poster_brief(
        &state,
        load_product_for_lookup(&state, lookup.into_lookup()).await?,
        theme.as_deref().unwrap_or("studio"),
        locale.as_deref().unwrap_or("zh-CN"),
    )?;
    let generated = generate_poster_background(&state, &poster.brief).await?;
    Ok(Json(PosterGenerateResponse {
        connector_mode: connector_mode(&state),
        product: poster.product,
        brief: poster.brief,
        image_model: generated.image_model,
        prompt: generated.prompt,
        revised_prompt: generated.revised_prompt,
        background_data_url: generated.background_data_url,
    }))
}

async fn poster_verify(
    State(state): State<LocalState>,
    headers: HeaderMap,
    Json(input): Json<PosterVerifyRequest>,
) -> Result<Json<PosterVerifyResponse>, ApiError> {
    require_operator_token(&state, &headers)?;
    let PosterVerifyRequest {
        lookup,
        theme,
        locale,
        headline,
        subheadline,
        selling_points,
        cta_line,
        compliance_note,
    } = input;
    let locale = locale.as_deref().unwrap_or("zh-CN");
    let poster = build_poster_brief(
        &state,
        load_product_for_lookup(&state, lookup.into_lookup()).await?,
        theme.as_deref().unwrap_or("studio"),
        locale,
    )?;
    let approved_copy = PosterCopy {
        headline: poster.brief.headline.clone(),
        subheadline: poster.brief.subheadline.clone(),
        selling_points: poster.brief.selling_points.clone(),
        cta_line: poster.brief.cta_line.clone(),
        compliance_note: poster.brief.compliance_note.clone(),
    };
    let submitted_copy = PosterCopy {
        headline: headline.trim().to_string(),
        subheadline: subheadline.trim().to_string(),
        selling_points: selling_points
            .into_iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .collect(),
        cta_line: cta_line.trim().to_string(),
        compliance_note: compliance_note.trim().to_string(),
    };
    let mismatches = compare_poster_copy(&approved_copy, &submitted_copy);
    let warnings = poster_verify_warnings(locale, mismatches.is_empty());
    Ok(Json(PosterVerifyResponse {
        ok: mismatches.is_empty(),
        checked_at: Utc::now().to_rfc3339(),
        approved_copy,
        mismatches,
        warnings,
    }))
}

async fn create_dry_run(
    State(state): State<LocalState>,
    headers: HeaderMap,
    Json(input): Json<DryRunRequest>,
) -> Result<Json<TaskResponse>, ApiError> {
    require_bridge_or_operator_token(&state, &headers)?;
    let operation = input
        .operation
        .unwrap_or(OperationKind::OzonUpdatePriceMock);
    let dry_run = input.dry_run.unwrap_or_else(|| sample_dry_run(operation));
    let task = state
        .tasks
        .create_dry_run(CreateTask {
            tenant_id: input.tenant_id.map(TenantId).unwrap_or_default(),
            shop_id: input.shop_id.unwrap_or_else(|| "default-shop".to_string()),
            source: input.source.unwrap_or(TaskSource::OpenClaw),
            operation,
            dry_run,
            risk: input.risk.unwrap_or(RiskLevel::High),
            idempotency_key: input
                .idempotency_key
                .unwrap_or_else(|| format!("idem-{}", Uuid::new_v4())),
        })
        .await
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    Ok(Json(TaskResponse { task }))
}

async fn list_tasks(
    State(state): State<LocalState>,
    headers: HeaderMap,
) -> Result<Json<Vec<Task>>, ApiError> {
    require_operator_token(&state, &headers)?;
    Ok(Json(state.tasks.list().await))
}

async fn get_task(
    State(state): State<LocalState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<TaskResponse>, ApiError> {
    require_bridge_or_operator_token(&state, &headers)?;
    let task = state
        .tasks
        .get(TaskId(id))
        .await
        .ok_or_else(|| ApiError::not_found("task not found"))?;
    Ok(Json(TaskResponse { task }))
}

async fn approve_task(
    State(state): State<LocalState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<ApproveTaskRequest>,
) -> Result<Json<TaskResponse>, ApiError> {
    require_operator_token(&state, &headers)?;
    let task = state
        .tasks
        .approve(
            TaskId(id),
            input
                .approved_by
                .unwrap_or_else(|| "local-operator".to_string()),
            input.note,
        )
        .await
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    Ok(Json(TaskResponse { task }))
}

async fn cancel_task(
    State(state): State<LocalState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<TaskResponse>, ApiError> {
    require_operator_token(&state, &headers)?;
    let task = state
        .tasks
        .cancel(TaskId(id), "local-operator")
        .await
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    Ok(Json(TaskResponse { task }))
}

async fn execute_mock_task(
    State(state): State<LocalState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<TaskResponse>, ApiError> {
    require_operator_token(&state, &headers)?;
    let id = TaskId(id);
    state
        .tasks
        .mark_running(id)
        .await
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    tokio::time::sleep(Duration::from_millis(150)).await;
    let task = state
        .tasks
        .mark_succeeded(
            id,
            ExecutionReceipt {
                external_request_id: Some(format!("dry-run-receipt-{}", Uuid::new_v4())),
                executed_at: Utc::now(),
                result_summary: "dry-run execution completed; no Ozon write was sent".to_string(),
            },
        )
        .await
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    Ok(Json(TaskResponse { task }))
}

async fn get_ecommerce_schedule(
    State(state): State<LocalState>,
    headers: HeaderMap,
) -> Result<Json<EcommerceScheduleResponse>, ApiError> {
    require_operator_token(&state, &headers)?;
    Ok(Json(schedule_response(&state).await))
}

async fn configure_ecommerce_schedule(
    State(state): State<LocalState>,
    headers: HeaderMap,
    Json(input): Json<ConfigureEcommerceScheduleRequest>,
) -> Result<Json<EcommerceScheduleResponse>, ApiError> {
    require_operator_token(&state, &headers)?;
    let interval_secs = input
        .interval_secs
        .unwrap_or(state.config.default_ecommerce_interval_secs)
        .clamp(60, 24 * 60 * 60);
    let limit = input
        .limit
        .unwrap_or(state.config.default_ecommerce_limit)
        .clamp(1, 100);

    if input.enabled {
        let _ = execute_ecommerce_read_once(&state, limit).await?;
        start_ecommerce_schedule(state.clone(), interval_secs, limit).await;
    } else {
        stop_ecommerce_schedule(&state, "disabled by local operator").await;
    }
    Ok(Json(schedule_response(&state).await))
}

async fn run_ecommerce_schedule_now(
    State(state): State<LocalState>,
    headers: HeaderMap,
) -> Result<Json<EcommerceScheduleRunResponse>, ApiError> {
    require_operator_token(&state, &headers)?;
    let limit = state.schedules.read().await.limit;
    let run = execute_ecommerce_read_once(&state, limit).await?;
    Ok(Json(EcommerceScheduleRunResponse { run }))
}

async fn propose_ecommerce_schedule(
    State(state): State<LocalState>,
    headers: HeaderMap,
    Json(input): Json<ProposeEcommerceScheduleRequest>,
) -> Result<Json<TaskResponse>, ApiError> {
    require_bridge_or_operator_token(&state, &headers)?;
    require_valid_lease_with_feature(&state, Feature::OzonRead).await?;
    let interval_secs = input
        .interval_secs
        .unwrap_or(state.config.default_ecommerce_interval_secs)
        .clamp(60, 24 * 60 * 60);
    let limit = input
        .limit
        .unwrap_or(state.config.default_ecommerce_limit)
        .clamp(1, 100);
    let task = state
        .tasks
        .create_dry_run(CreateTask {
            tenant_id: input.tenant_id.map(TenantId).unwrap_or_default(),
            shop_id: input.shop_id.unwrap_or_else(|| "default-shop".to_string()),
            source: input.source.unwrap_or(TaskSource::OpenClaw),
            operation: OperationKind::OzonProductsList,
            dry_run: DryRunDiff {
                summary: format!(
                    "Propose official Ozon read-only product polling every {interval_secs}s with limit {limit}"
                ),
                target_count: u32::from(limit),
                changes: vec![],
            warnings: vec![
                "This proposal does not enable a scheduler; local operator token is required"
                    .to_string(),
                "Real connector mode uses official Ozon Seller API credentials only; no live 1688 scraping"
                    .to_string(),
            ],
            },
            risk: RiskLevel::Low,
            idempotency_key: input
                .idempotency_key
                .unwrap_or_else(|| format!("schedule-proposal-{}", Uuid::new_v4())),
        })
        .await
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    Ok(Json(TaskResponse { task }))
}

async fn events(
    State(state): State<LocalState>,
    headers: HeaderMap,
) -> Result<Sse<impl futures_core::Stream<Item = Result<Event, std::convert::Infallible>>>, ApiError>
{
    require_operator_token(&state, &headers)?;
    let mut receiver = state.tasks.subscribe();
    let stream = stream! {
        yield Ok(Event::default().event("status").data("{\"status\":\"connected\"}"));
        loop {
            match receiver.recv().await {
                Ok(TaskEvent::Changed(task)) => {
                    let payload = serde_json::to_string(&task).unwrap_or_else(|_| "{}".to_string());
                    yield Ok(Event::default().event("task.changed").data(payload));
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    yield Ok(Event::default().event("warning").data("{\"warning\":\"event lagged\"}"));
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    };
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

async fn start_ecommerce_schedule(state: LocalState, interval_secs: u64, limit: u16) {
    stop_existing_schedule_worker(&state).await;
    {
        let mut schedule = state.schedules.write().await;
        schedule.enabled = true;
        schedule.interval_secs = interval_secs;
        schedule.limit = limit;
        schedule.last_error = None;
        schedule.audit.push(ScheduleAuditEvent {
            at: Utc::now().to_rfc3339(),
            actor: "local-operator".to_string(),
            action: "schedule.enabled".to_string(),
            summary: format!(
                "official Ozon read-only polling enabled every {interval_secs}s with limit {limit}"
            ),
        });
    }

    let worker_state = state.clone();
    let handle = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            if !worker_state.schedules.read().await.enabled {
                break;
            }
            if let Err(error) = execute_ecommerce_read_once(&worker_state, limit).await {
                let mut schedule = worker_state.schedules.write().await;
                schedule.last_error = Some(error.message.clone());
                schedule.audit.push(ScheduleAuditEvent {
                    at: Utc::now().to_rfc3339(),
                    actor: "scheduler".to_string(),
                    action: "schedule.read_failed".to_string(),
                    summary: error.message,
                });
            }
        }
    });
    state.schedules.write().await.worker = Some(handle);
}

async fn stop_ecommerce_schedule(state: &LocalState, reason: &str) {
    stop_existing_schedule_worker(state).await;
    let mut schedule = state.schedules.write().await;
    schedule.enabled = false;
    schedule.audit.push(ScheduleAuditEvent {
        at: Utc::now().to_rfc3339(),
        actor: "local-operator".to_string(),
        action: "schedule.disabled".to_string(),
        summary: reason.to_string(),
    });
}

async fn stop_existing_schedule_worker(state: &LocalState) {
    if let Some(handle) = state.schedules.write().await.worker.take() {
        handle.abort();
    }
}

async fn execute_ecommerce_read_once(
    state: &LocalState,
    limit: u16,
) -> Result<EcommerceReadRun, ApiError> {
    require_valid_lease_with_feature(state, Feature::OzonRead).await?;
    let started_at = Utc::now();
    let start = Instant::now();
    let credentials = load_ozon_credentials(state).await?;
    let mut page = state
        .ozon_connector
        .product_list_page_with_visibility(&credentials, limit.clamp(1, 100), None, None)
        .await
        .map_err(|error| ApiError::bad_gateway(format!("scheduled Ozon read failed: {error}")))?;
    if page.total == 0 && page.products.is_empty() {
        let archived_page = state
            .ozon_connector
            .product_list_page_with_visibility(
                &credentials,
                limit.clamp(1, 100),
                None,
                Some("ARCHIVED".into()),
            )
            .await
            .map_err(|error| {
                ApiError::bad_gateway(format!("scheduled archived Ozon read failed: {error}"))
            })?;
        if archived_page.total > 0 || !archived_page.products.is_empty() {
            page = archived_page;
        }
    }
    let OzonProductListPage {
        products,
        total,
        last_id,
    } = page;
    let run = EcommerceReadRun {
        started_at: started_at.to_rfc3339(),
        completed_at: Utc::now().to_rfc3339(),
        duration_ms: start.elapsed().as_millis() as u64,
        connector_mode: connector_mode(state),
        product_count: total,
        sample_size: products.len() as u16,
        next_last_id: last_id,
        products,
        total,
    };

    let mut schedule = state.schedules.write().await;
    schedule.last_error = None;
    schedule.last_run = Some(run.clone());
    schedule.audit.push(ScheduleAuditEvent {
        at: Utc::now().to_rfc3339(),
        actor: "scheduler".to_string(),
        action: "schedule.read_succeeded".to_string(),
        summary: format!(
            "read {} products in {}ms through {} connector",
            run.sample_size, run.duration_ms, run.connector_mode
        ),
    });
    if schedule.audit.len() > 100 {
        let keep_from = schedule.audit.len() - 100;
        schedule.audit.drain(0..keep_from);
    }
    Ok(run)
}

async fn schedule_response(state: &LocalState) -> EcommerceScheduleResponse {
    let schedule = state.schedules.read().await;
    EcommerceScheduleResponse {
        enabled: schedule.enabled,
        interval_secs: schedule.interval_secs,
        limit: schedule.limit,
        connector_mode: connector_mode(state),
        last_run: schedule.last_run.clone(),
        last_error: schedule.last_error.clone(),
        audit: schedule.audit.clone(),
        safety: vec![
            "official_ozon_api_only",
            "read_only",
            "operator_token_required_to_enable",
            "openclaw_proposal_only",
            "no_1688_live_scraping",
        ],
    }
}

fn require_operator_token(state: &LocalState, headers: &HeaderMap) -> Result<(), ApiError> {
    let token = headers
        .get("x-local-token")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| ApiError::unauthorized("missing x-local-token"))?;
    if !constant_time_eq(token, &state.config.operator_token) {
        return Err(ApiError::unauthorized("invalid x-local-token"));
    }
    Ok(())
}

fn require_bridge_or_operator_token(
    state: &LocalState,
    headers: &HeaderMap,
) -> Result<(), ApiError> {
    if headers
        .get("x-local-token")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|token| constant_time_eq(token, &state.config.operator_token))
    {
        return Ok(());
    }
    if headers
        .get("x-openclaw-token")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|token| constant_time_eq(token, &state.config.openclaw_token))
    {
        return Ok(());
    }
    Err(ApiError::unauthorized(
        "missing or invalid x-openclaw-token / x-local-token",
    ))
}

fn require_openclaw_pairing_origin(headers: &HeaderMap) -> Result<(), ApiError> {
    let origin = headers
        .get("origin")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| ApiError::forbidden("missing OpenClaw pairing origin"))?;
    if is_allowed_openclaw_pairing_origin(origin) {
        return Ok(());
    }
    Err(ApiError::forbidden(
        "OpenClaw pairing origin is not allowed",
    ))
}

fn is_allowed_openclaw_pairing_origin(origin: &str) -> bool {
    origin == "https://ozonclaw.jl696.cn"
        || origin == "https://www.ozonclaw.jl696.cn"
        || origin == "http://127.0.0.1:18789"
        || origin == "http://localhost:18789"
        || configured_openclaw_allowed_origins()
            .iter()
            .any(|allowed| allowed == origin)
}

async fn require_valid_lease_with_feature(
    state: &LocalState,
    feature: Feature,
) -> Result<(), ApiError> {
    if !state.config.use_real_ozon && state.config.allow_unsigned_lease {
        return Ok(());
    }
    let lease = load_cloud_lease(state).await?;
    validate_cloud_lease_with_feature(state, &lease, feature)?;
    let device_fingerprint = load_or_create_device_fingerprint(state).await?;
    ensure_lease_bound_to_device(&lease, &device_fingerprint)
}

async fn load_cloud_lease(state: &LocalState) -> Result<EntitlementLease, ApiError> {
    if let Some(lease) = state.cloud_lease_cache.read().await.clone() {
        return Ok(lease);
    }
    let bundle = state
        .secrets
        .get(&SecretName::new(SECRET_CLOUD_LEASE))
        .await
        .map_err(|_| ApiError::internal("secret store unavailable"))?
        .ok_or_else(|| {
            ApiError::forbidden(
                "cloud lease is not installed; sign in on ozon66.com, bind this device, then issue a lease",
            )
        })?;
    let lease: EntitlementLease = serde_json::from_str(bundle.expose_secret())
        .map_err(|_| ApiError::bad_request("stored cloud lease is invalid"))?;
    *state.cloud_lease_cache.write().await = Some(lease.clone());
    Ok(lease)
}

fn constant_time_eq(left: &str, right: &str) -> bool {
    let left = left.as_bytes();
    let right = right.as_bytes();
    let max = left.len().max(right.len());
    let mut diff = left.len() ^ right.len();
    for index in 0..max {
        let left_byte = *left.get(index).unwrap_or(&0);
        let right_byte = *right.get(index).unwrap_or(&0);
        diff |= usize::from(left_byte ^ right_byte);
    }
    diff == 0
}

async fn inspect_ozon_credentials(state: &LocalState) -> InspectedOzonCredentials {
    if let Some(stored) = state.ozon_config_cache.read().await.clone() {
        return InspectedOzonCredentials {
            configured: true,
            source: "ozon_config_cache",
            client_id: Some(redact(&stored.client_id)),
            api_key_fingerprint: Some(fingerprint_secret(&SecretString::from(stored.api_key))),
            secret_store_available: true,
            issue: None,
        };
    }
    if let Some(env_credentials) = inspect_ozon_env_credentials() {
        return env_credentials;
    }
    match get_secret_for_status(state, SECRET_OZON_CONFIG).await {
        Ok(Some(bundle)) => {
            let stored: Result<StoredOzonConfig, _> = serde_json::from_str(bundle.expose_secret());
            match stored {
                Ok(stored) => InspectedOzonCredentials {
                    configured: true,
                    source: "ozon_config",
                    client_id: Some(redact(&stored.client_id)),
                    api_key_fingerprint: Some(fingerprint_secret(&SecretString::from(
                        stored.api_key,
                    ))),
                    secret_store_available: true,
                    issue: None,
                },
                Err(_) => InspectedOzonCredentials {
                    configured: false,
                    source: "ozon_config",
                    client_id: None,
                    api_key_fingerprint: None,
                    secret_store_available: true,
                    issue: Some("stored Ozon config is invalid".to_string()),
                },
            }
        }
        Ok(None) => inspect_legacy_ozon_credentials(state).await,
        Err(_) => InspectedOzonCredentials {
            configured: false,
            source: "unavailable",
            client_id: None,
            api_key_fingerprint: None,
            secret_store_available: false,
            issue: Some("secret store unavailable".to_string()),
        },
    }
}

async fn inspect_legacy_ozon_credentials(state: &LocalState) -> InspectedOzonCredentials {
    let client_id = match get_secret_for_status(state, "ozon_client_id").await {
        Ok(value) => value,
        Err(_) => {
            return InspectedOzonCredentials {
                configured: false,
                source: "legacy_split_keys",
                client_id: None,
                api_key_fingerprint: None,
                secret_store_available: false,
                issue: Some("secret store unavailable".to_string()),
            };
        }
    };
    let api_key = match get_secret_for_status(state, "ozon_api_key").await {
        Ok(value) => value,
        Err(_) => {
            return InspectedOzonCredentials {
                configured: false,
                source: "legacy_split_keys",
                client_id: None,
                api_key_fingerprint: None,
                secret_store_available: false,
                issue: Some("secret store unavailable".to_string()),
            };
        }
    };

    match (client_id, api_key) {
        (Some(client_id), Some(api_key)) => InspectedOzonCredentials {
            configured: true,
            source: "legacy_split_keys",
            client_id: Some(redact(client_id.expose_secret())),
            api_key_fingerprint: Some(fingerprint_secret(&api_key)),
            secret_store_available: true,
            issue: None,
        },
        (None, None) => InspectedOzonCredentials {
            configured: false,
            source: if state.config.use_real_ozon {
                "missing"
            } else {
                "debug_mock_connector"
            },
            client_id: None,
            api_key_fingerprint: None,
            secret_store_available: true,
            issue: if state.config.use_real_ozon {
                Some("Ozon credentials are not configured".to_string())
            } else {
                None
            },
        },
        _ => InspectedOzonCredentials {
            configured: false,
            source: "legacy_split_keys",
            client_id: None,
            api_key_fingerprint: None,
            secret_store_available: true,
            issue: Some("legacy Ozon credentials are incomplete".to_string()),
        },
    }
}

fn inspect_ozon_env_credentials() -> Option<InspectedOzonCredentials> {
    let client_id =
        optional_env("OZON_CLIENT_ID").or_else(|| optional_env("OZON_SELLER_CLIENT_ID"));
    let api_key = optional_env("OZON_API_KEY").or_else(|| optional_env("OZON_SELLER_API_KEY"));
    match (client_id, api_key) {
        (Some(client_id), Some(api_key)) => Some(InspectedOzonCredentials {
            configured: true,
            source: "env",
            client_id: Some(redact(&client_id)),
            api_key_fingerprint: Some(fingerprint_secret(&SecretString::from(api_key))),
            secret_store_available: true,
            issue: None,
        }),
        (Some(_), None) | (None, Some(_)) => Some(InspectedOzonCredentials {
            configured: false,
            source: "env",
            client_id: None,
            api_key_fingerprint: None,
            secret_store_available: true,
            issue: Some(
                "OZON_CLIENT_ID/OZON_API_KEY environment credentials are incomplete".to_string(),
            ),
        }),
        (None, None) => None,
    }
}

async fn load_ozon_credentials(state: &LocalState) -> Result<OzonCredentials, ApiError> {
    if !state.config.use_real_ozon {
        return Ok(debug_mock_ozon_credentials());
    }

    if let Some(stored) = state.ozon_config_cache.read().await.clone() {
        return Ok(OzonCredentials {
            client_id: stored.client_id,
            api_key: SecretString::from(stored.api_key),
        });
    }

    if let Some(credentials) = load_ozon_env_credentials()? {
        return Ok(credentials);
    }

    if let Some(bundle) = state
        .secrets
        .get(&SecretName::new(SECRET_OZON_CONFIG))
        .await
        .map_err(|_| ApiError::internal("secret store unavailable"))?
    {
        let stored: StoredOzonConfig = serde_json::from_str(bundle.expose_secret())
            .map_err(|_| ApiError::internal("stored Ozon config is invalid"))?;
        return Ok(OzonCredentials {
            client_id: stored.client_id,
            api_key: SecretString::from(stored.api_key),
        });
    }

    let client_id = state
        .secrets
        .get(&SecretName::new("ozon_client_id"))
        .await
        .map_err(|_| ApiError::internal("secret store unavailable"))?;
    let api_key = state
        .secrets
        .get(&SecretName::new("ozon_api_key"))
        .await
        .map_err(|_| ApiError::internal("secret store unavailable"))?;
    let Some(client_id) = client_id else {
        return Err(ApiError::bad_request("Ozon credentials are not configured"));
    };
    let Some(api_key) = api_key else {
        return Err(ApiError::bad_request("Ozon credentials are not configured"));
    };
    Ok(OzonCredentials {
        client_id: client_id.expose_secret().to_string(),
        api_key,
    })
}

fn load_ozon_env_credentials() -> Result<Option<OzonCredentials>, ApiError> {
    let client_id =
        optional_env("OZON_CLIENT_ID").or_else(|| optional_env("OZON_SELLER_CLIENT_ID"));
    let api_key = optional_env("OZON_API_KEY").or_else(|| optional_env("OZON_SELLER_API_KEY"));
    match (client_id, api_key) {
        (Some(client_id), Some(api_key)) => Ok(Some(OzonCredentials {
            client_id,
            api_key: SecretString::from(api_key),
        })),
        (Some(_), None) | (None, Some(_)) => Err(ApiError::bad_request(
            "OZON_CLIENT_ID/OZON_API_KEY environment credentials are incomplete",
        )),
        (None, None) => Ok(None),
    }
}

/// A STABLE, machine-derived device fingerprint, computed deterministically from
/// the macOS hardware UUID and cached for the process lifetime. This is immune to
/// the secret-store/keyring flakiness that previously re-minted a random
/// fingerprint whenever the keyring was momentarily unavailable at sidecar boot —
/// which silently broke the lease's device binding (the lease is bound to
/// `device_id_for(user, fingerprint)`, so a changed fingerprint => "cloud lease is
/// not bound to this device"). Returns `None` on non-macOS or if the hardware id
/// cannot be read, in which case the caller falls back to the stored/random value.
fn stable_machine_fingerprint() -> Option<String> {
    static CACHE: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    CACHE.get_or_init(compute_stable_machine_fingerprint).clone()
}

fn compute_stable_machine_fingerprint() -> Option<String> {
    use sha2::Digest;
    // `ioreg` exposes IOPlatformUUID, a per-machine identifier stable across
    // reboots and reinstalls. Hash it (with a domain-separating prefix) so the
    // raw hardware id never leaves the device.
    let output = std::process::Command::new("/usr/sbin/ioreg")
        .args(["-rd1", "-c", "IOPlatformExpertDevice"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let uuid = text
        .lines()
        .find(|line| line.contains("IOPlatformUUID"))
        .and_then(|line| line.split('"').nth(3))
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let mut hasher = Sha256::new();
    hasher.update(b"ozon-rust-local-device-v1:");
    hasher.update(uuid.as_bytes());
    let digest = hasher.finalize();
    let hex: String = digest
        .iter()
        .take(16)
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Some(format!("ors-local-{hex}"))
}

async fn load_or_create_device_fingerprint(state: &LocalState) -> Result<String, ApiError> {
    // Prefer the stable machine-derived fingerprint so restarts and keyring
    // flakiness can never change it. It is deterministic, so no storage is needed.
    if let Some(stable) = stable_machine_fingerprint() {
        return Ok(stable);
    }

    // Fallback (non-macOS / hardware probe failure): the original
    // stored-or-create-random behavior.
    if let Some(existing) = state
        .secrets
        .get(&SecretName::new(SECRET_DEVICE_FINGERPRINT))
        .await
        .map_err(|_| ApiError::internal("secret store unavailable"))?
    {
        let value = existing.expose_secret().trim();
        if value.starts_with("ors-local-") && value.len() >= 20 {
            return Ok(value.to_string());
        }
    }

    let value = format!("ors-local-{}", Uuid::new_v4());
    state
        .secrets
        .put(
            SecretName::new(SECRET_DEVICE_FINGERPRINT),
            SecretString::from(value.clone()),
        )
        .await
        .map_err(|_| ApiError::internal("failed to persist device fingerprint"))?;
    Ok(value)
}

async fn inspect_openai_config(state: &LocalState) -> OpenAiCredentialStatus {
    if let Some(stored) = state.openai_config_cache.read().await.clone() {
        return OpenAiCredentialStatus {
            configured: true,
            source: "openai_config_cache",
            base_url: stored.base_url,
            image_model: stored.image_model,
            api_key_fingerprint: Some(fingerprint_secret(&SecretString::from(stored.api_key))),
            issue: None,
        };
    }
    if let Ok(env_config) = load_openai_env_config() {
        return OpenAiCredentialStatus {
            configured: true,
            source: "env",
            base_url: env_config.base_url,
            image_model: env_config.image_model,
            api_key_fingerprint: Some(fingerprint_secret(&SecretString::from(env_config.api_key))),
            issue: None,
        };
    }
    match get_secret_for_status(state, SECRET_OPENAI_CONFIG).await {
        Ok(Some(bundle)) => {
            match serde_json::from_str::<StoredOpenAiConfig>(bundle.expose_secret()) {
                Ok(stored) => OpenAiCredentialStatus {
                    configured: true,
                    source: "openai_config",
                    base_url: stored.base_url,
                    image_model: stored.image_model,
                    api_key_fingerprint: Some(fingerprint_secret(&SecretString::from(
                        stored.api_key,
                    ))),
                    issue: None,
                },
                Err(_) => OpenAiCredentialStatus {
                    configured: false,
                    source: "openai_config",
                    base_url: state.config.openai_base_url.clone(),
                    image_model: state.config.openai_image_model.clone(),
                    api_key_fingerprint: None,
                    issue: Some("stored OpenAI config is invalid".to_string()),
                },
            }
        }
        Ok(None) => OpenAiCredentialStatus {
            configured: false,
            source: "missing",
            base_url: state.config.openai_base_url.clone(),
            image_model: state.config.openai_image_model.clone(),
            api_key_fingerprint: None,
            issue: Some("OpenAI API key is not configured".to_string()),
        },
        Err(_) => match load_openai_env_config() {
            Ok(env_config) => OpenAiCredentialStatus {
                configured: true,
                source: "env",
                base_url: env_config.base_url,
                image_model: env_config.image_model,
                api_key_fingerprint: Some(fingerprint_secret(&SecretString::from(
                    env_config.api_key,
                ))),
                issue: None,
            },
            Err(_) => OpenAiCredentialStatus {
                configured: false,
                source: "unavailable",
                base_url: state.config.openai_base_url.clone(),
                image_model: state.config.openai_image_model.clone(),
                api_key_fingerprint: None,
                issue: Some("secret store unavailable".to_string()),
            },
        },
    }
}

async fn load_openai_config(state: &LocalState) -> Result<StoredOpenAiConfig, ApiError> {
    if let Some(stored) = state.openai_config_cache.read().await.clone() {
        return Ok(stored);
    }

    if let Ok(env_config) = load_openai_env_config() {
        return Ok(env_config);
    }

    if let Some(bundle) = state
        .secrets
        .get(&SecretName::new(SECRET_OPENAI_CONFIG))
        .await
        .map_err(|_| ApiError::internal("secret store unavailable"))?
    {
        let stored: StoredOpenAiConfig = serde_json::from_str(bundle.expose_secret())
            .map_err(|_| ApiError::internal("stored OpenAI config is invalid"))?;
        return Ok(stored);
    }

    let api_key = env::var("OPENAI_API_KEY").map_err(|_| {
        ApiError::bad_request("OpenAI API key is not configured for poster generation")
    })?;
    Ok(StoredOpenAiConfig {
        api_key,
        base_url: normalize_openai_base_url(&state.config.openai_base_url)?,
        image_model: state.config.openai_image_model.clone(),
    })
}

async fn load_persisted_openai_config(state: &LocalState) -> Result<StoredOpenAiConfig, ApiError> {
    if let Some(stored) = state.openai_config_cache.read().await.clone() {
        return Ok(stored);
    }

    let Some(bundle) = state
        .secrets
        .get(&SecretName::new(SECRET_OPENAI_CONFIG))
        .await
        .map_err(|_| ApiError::internal("secret store unavailable"))?
    else {
        return Err(ApiError::bad_request(
            "OpenAI API key is not stored in this app",
        ));
    };

    serde_json::from_str(bundle.expose_secret())
        .map_err(|_| ApiError::internal("stored OpenAI config is invalid"))
}

/// Load the persisted model registry, mirroring `load_persisted_openai_config`:
/// return the cached value, else read + deserialize `SECRET_MODEL_REGISTRY`, else
/// the default (empty) registry. The cache is populated on every path so repeated
/// resolves do not hit the secret store.
async fn load_persisted_model_registry(state: &LocalState) -> StoredModelRegistry {
    if let Some(cached) = state.model_registry_cache.read().await.clone() {
        return cached;
    }

    let registry = match get_secret_for_status(state, SECRET_MODEL_REGISTRY).await {
        Ok(Some(bundle)) => serde_json::from_str::<StoredModelRegistry>(bundle.expose_secret())
            .unwrap_or_default(),
        _ => StoredModelRegistry::default(),
    };

    *state.model_registry_cache.write().await = Some(registry.clone());
    registry
}

/// Resolve a `secret_ref` from a registry entry to a raw API key. The registry
/// blob never contains a key — only this reference. `"openai_config"` reuses the
/// existing OpenAI key (with its env precedence) via `load_openai_config`; any
/// other ref reads that exact key from the secret store and errors if absent.
async fn resolve_secret_ref(state: &LocalState, secret_ref: &str) -> Result<String, ApiError> {
    if secret_ref == SECRET_OPENAI_CONFIG {
        return Ok(load_openai_config(state).await?.api_key);
    }
    let bundle = state
        .secrets
        .get(&SecretName::new(secret_ref))
        .await
        .map_err(|_| ApiError::internal("secret store unavailable"))?
        .ok_or_else(|| {
            ApiError::bad_request(format!("secret_ref '{secret_ref}' is not configured"))
        })?;
    Ok(bundle.expose_secret().to_string())
}

/// Validate and normalize a provider base URL for any auth style. Reuses the
/// loopback-only-http rule from `normalize_openai_base_url`: non-loopback hosts
/// MUST use https, since Header/Query auth also puts the key on the wire.
fn normalize_provider_base_url(value: &str) -> Result<String, ApiError> {
    normalize_openai_base_url(value)
}

fn load_openai_env_config() -> Result<StoredOpenAiConfig, ApiError> {
    let api_key = optional_env("OPENAI_API_KEY").ok_or_else(|| {
        ApiError::bad_request("OpenAI API key is not configured for poster generation")
    })?;
    Ok(StoredOpenAiConfig {
        api_key,
        base_url: normalize_openai_base_url(
            optional_env("OPENAI_BASE_URL")
                .or_else(|| optional_env("OPENAI_API_BASE_URL"))
                .as_deref()
                .unwrap_or(DEFAULT_OPENAI_BASE_URL),
        )?,
        image_model: optional_env("OPENAI_IMAGE_MODEL")
            .unwrap_or_else(|| DEFAULT_OPENAI_IMAGE_MODEL.to_string()),
    })
}

async fn inspect_cloud_lease(state: &LocalState) -> LeaseStatus {
    if let Some(lease) = state.cloud_lease_cache.read().await.clone() {
        return lease_status(state, &lease);
    }
    match get_secret_for_status(state, SECRET_CLOUD_LEASE).await {
        Ok(Some(bundle)) => {
            match serde_json::from_str::<EntitlementLease>(bundle.expose_secret()) {
                Ok(lease) => lease_status(state, &lease),
                Err(_) => LeaseStatus {
                    configured: false,
                    valid: false,
                    lease_id: None,
                    device_id: None,
                    features: Vec::new(),
                    expires_at: None,
                    issue: Some("stored cloud lease is invalid".to_string()),
                },
            }
        }
        Ok(None) => LeaseStatus {
            configured: false,
            valid: false,
            lease_id: None,
            device_id: None,
            features: Vec::new(),
            expires_at: None,
            issue: Some("cloud lease is not installed".to_string()),
        },
        Err(_) => LeaseStatus {
            configured: false,
            valid: false,
            lease_id: None,
            device_id: None,
            features: Vec::new(),
            expires_at: None,
            issue: Some("secret store unavailable".to_string()),
        },
    }
}

async fn get_secret_for_status(
    state: &LocalState,
    name: &'static str,
) -> Result<Option<SecretString>, ()> {
    tokio::time::timeout(
        Duration::from_secs(2),
        state.secrets.get(&SecretName::new(name)),
    )
    .await
    .map_err(|_| ())?
    .map_err(|_| ())
}

fn validate_cloud_lease(state: &LocalState, lease: &EntitlementLease) -> Result<(), ApiError> {
    if lease.expires_at <= Utc::now() {
        return Err(ApiError::bad_request("lease is expired"));
    }
    verify_cloud_lease_signature(state, lease)?;
    Ok(())
}

fn validate_cloud_lease_with_feature(
    state: &LocalState,
    lease: &EntitlementLease,
    feature: Feature,
) -> Result<(), ApiError> {
    validate_cloud_lease(state, lease)?;
    if !lease.features.contains(&feature) {
        return Err(ApiError::forbidden(format!(
            "cloud lease does not include {}",
            feature_name(feature)
        )));
    }
    Ok(())
}

/// Reject a cloud lease that was not issued for THIS device. The signed lease carries the
/// cloud-assigned device id, which the cloud derives deterministically from (user_id, device
/// fingerprint); recomputing it locally and comparing prevents a validly-signed lease from being
/// replayed or shared onto a different machine.
fn ensure_lease_bound_to_device(
    lease: &EntitlementLease,
    device_fingerprint: &str,
) -> Result<(), ApiError> {
    let expected = ozon_domain::device_id_for(lease.user_id, device_fingerprint);
    if lease.device_id != expected {
        return Err(ApiError::forbidden(
            "cloud lease is not bound to this device",
        ));
    }
    Ok(())
}

fn feature_name(feature: Feature) -> &'static str {
    match feature {
        Feature::OzonRead => "Ozon read access",
        Feature::OzonWriteMock => "Ozon dry-run write access",
        Feature::DraftImport1688Mock => "1688 draft import dry-run access",
        Feature::OpenClawBridge => "OpenClaw bridge access",
        Feature::LocalApproval => "local approval access",
    }
}

fn lease_status(state: &LocalState, lease: &EntitlementLease) -> LeaseStatus {
    let validation = validate_cloud_lease_with_feature(state, lease, Feature::OzonRead);
    let valid = validation.is_ok();
    LeaseStatus {
        configured: true,
        valid,
        lease_id: Some(lease.lease_id.to_string()),
        device_id: Some(lease.device_id.0.to_string()),
        features: lease.features.clone(),
        expires_at: Some(lease.expires_at.to_rfc3339()),
        issue: if valid {
            None
        } else {
            Some(
                validation
                    .err()
                    .map(|error| error.message)
                    .unwrap_or_else(|| "lease is invalid".to_string()),
            )
        },
    }
}

fn verify_cloud_lease_signature(
    state: &LocalState,
    lease: &EntitlementLease,
) -> Result<(), ApiError> {
    let Some(signature) = &lease.signature else {
        if state.config.allow_unsigned_lease {
            return Ok(());
        }
        return Err(ApiError::bad_request("lease is missing a cloud signature"));
    };
    if signature.alg != "RS256" {
        return Err(ApiError::bad_request(
            "lease signature algorithm is not supported",
        ));
    }
    if signature.issuer != state.config.lease_issuer {
        return Err(ApiError::bad_request("lease signature issuer mismatch"));
    }
    if signature.audience != state.config.lease_audience {
        return Err(ApiError::bad_request("lease signature audience mismatch"));
    }
    let public_key_pem = state
        .config
        .lease_public_key_pem
        .as_deref()
        .ok_or_else(|| {
            ApiError::bad_request("lease public key is not configured on this local node")
        })?;
    let public_key = RsaPublicKey::from_public_key_pem(public_key_pem)
        .map_err(|_| ApiError::bad_request("lease public key is invalid"))?;
    let signature_bytes = BASE64_STANDARD
        .decode(&signature.value)
        .map_err(|_| ApiError::bad_request("lease signature encoding is invalid"))?;
    let decoded_signature = RsaPkcs1v15Signature::try_from(signature_bytes.as_slice())
        .map_err(|_| ApiError::bad_request("lease signature is invalid"))?;
    let verifying_key = VerifyingKey::<Sha256>::new(public_key);
    let payload = lease_signing_payload(lease, &signature.issuer, &signature.audience)?;
    verifying_key
        .verify(payload.as_bytes(), &decoded_signature)
        .map_err(|_| ApiError::bad_request("lease signature verification failed"))
}

fn lease_signing_payload(
    lease: &EntitlementLease,
    issuer: &str,
    audience: &str,
) -> Result<String, ApiError> {
    #[derive(Serialize)]
    struct SignedLeasePayload<'a> {
        issuer: &'a str,
        audience: &'a str,
        claims: ozon_domain::EntitlementLeaseClaims,
    }
    serde_json::to_string(&SignedLeasePayload {
        issuer,
        audience,
        claims: lease.claims(),
    })
    .map_err(|_| ApiError::internal("failed to serialize lease verification payload"))
}

fn normalize_openai_base_url(value: &str) -> Result<String, ApiError> {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Ok(DEFAULT_OPENAI_BASE_URL.to_string());
    }
    let url = reqwest::Url::parse(trimmed)
        .map_err(|_| ApiError::bad_request("OpenAI base URL must be a valid URL"))?;
    match url.scheme() {
        "https" => {}
        "http" => {
            // The configured base URL is used to send the OpenAI/relay request with the
            // bearer API key. Permit plaintext http only for loopback hosts so the key is
            // never transmitted in cleartext to a remote host.
            let host = url.host_str().unwrap_or("");
            let host_addr = host.trim_start_matches('[').trim_end_matches(']');
            let is_loopback = host.eq_ignore_ascii_case("localhost")
                || host_addr
                    .parse::<std::net::IpAddr>()
                    .map(|ip| ip.is_loopback())
                    .unwrap_or(false);
            if !is_loopback {
                return Err(ApiError::bad_request(
                    "OpenAI base URL must use https (http is only allowed for loopback hosts)",
                ));
            }
        }
        _ => {
            return Err(ApiError::bad_request(
                "OpenAI base URL must use http or https",
            ));
        }
    }
    Ok(trimmed.to_string())
}

fn openai_images_endpoint(base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    if base.ends_with("/v1") {
        format!("{base}/images/generations")
    } else {
        format!("{base}/v1/images/generations")
    }
}

fn openai_images_edit_endpoint(base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    if base.ends_with("/v1") {
        format!("{base}/images/edits")
    } else {
        format!("{base}/v1/images/edits")
    }
}

fn poster_generation_status(
    openai: &OpenAiCredentialStatus,
    manifest_url: String,
) -> PosterGenerationStatus {
    PosterGenerationStatus {
        preferred: "openclaw_codex",
        openclaw_bridge_ready: true,
        handoff_path: "/poster/handoff",
        manifest_url,
        api_fallback_configured: openai.configured,
        api_fallback_model: openai.configured.then(|| openai.image_model.clone()),
        api_fallback_issue: openai.issue.clone(),
        message: if openai.configured {
            "openclaw_codex_preferred_with_api_fallback"
        } else {
            "openclaw_codex_preferred_without_api_fallback"
        },
    }
}

async fn load_product_for_lookup(
    state: &LocalState,
    lookup: OzonProductLookup,
) -> Result<ozon_connector::OzonProductDetail, ApiError> {
    require_valid_lease_with_feature(state, Feature::OzonRead).await?;
    let credentials = load_ozon_credentials(state).await?;
    state
        .ozon_connector
        .product_get(&credentials, lookup)
        .await
        .map_err(|error| map_product_get_error("ozon connector failed", error))
}

fn build_poster_brief(
    state: &LocalState,
    product: ozon_connector::OzonProductDetail,
    theme: &str,
    locale: &str,
) -> Result<PosterContext, ApiError> {
    let headline = preferred_headline(&product);
    let attribute_points = product
        .attributes
        .iter()
        .filter_map(attribute_selling_point)
        .take(3)
        .collect::<Vec<_>>();
    let selling_points = if attribute_points.is_empty() {
        default_selling_points(locale)
    } else {
        attribute_points
    };
    let image_count = product.images.len();
    let subheadline = poster_subheadline(locale, image_count > 0);
    let compliance_note = poster_compliance_note(locale, state.config.use_real_ozon);
    let cta_line = match locale {
        "zh-CN" => "先出一版能给运营看的海报，再微调标题和卖点".to_string(),
        _ => "Lock the hero image, then tune the selling points in one pass.".to_string(),
    };
    let image_stage = if product.primary_image.is_some() {
        "Reserve a clean stage in the lower-right area for compositing the real product cutout."
    } else {
        "Leave the center clean for a product to be placed later."
    };
    let background_prompt = format!(
        "Create a premium e-commerce poster background only, with no product, no packaging, no text, no logo, and no watermark. Theme: {}. Mood: confident, commercial, polished. Use light, shadow, reflections, and spatial depth to support a seller campaign poster. {} Palette should feel modern and readable behind {} text overlays. Keep the composition suitable for a 4:5 portrait poster.",
        normalize_theme(theme),
        image_stage,
        poster_overlay_language(locale)
    );
    Ok(PosterContext {
        product,
        brief: PosterBrief {
            theme: normalize_theme(theme).to_string(),
            headline,
            subheadline,
            selling_points,
            cta_line,
            compliance_note,
            background_prompt,
        },
    })
}

fn default_selling_points(locale: &str) -> Vec<String> {
    match locale {
        "zh-CN" => vec![
            "商品图来自当前 Ozon 店铺".to_string(),
            "保留包装、颜色和标签细节".to_string(),
            "适合先做首图和活动海报".to_string(),
        ],
        _ => vec![
            "Product images come from the current Ozon shop".to_string(),
            "Preserve packaging, color, and label details".to_string(),
            "Suitable for hero images and campaign posters".to_string(),
        ],
    }
}

fn poster_subheadline(locale: &str, has_image: bool) -> String {
    match (locale, has_image) {
        ("zh-CN", true) => "用真实商品图打底，背景只负责把质感和场景感补上。".to_string(),
        ("zh-CN", false) => "这件商品还没带主图，先补图再出海报会更稳。".to_string(),
        (_, true) => "Start from the real product image; use the background only to add texture and scene depth.".to_string(),
        (_, false) => "This product has no main image yet. Add a clear product image before poster generation.".to_string(),
    }
}

fn poster_compliance_note(locale: &str, real_ozon: bool) -> String {
    match (locale, real_ozon) {
        ("zh-CN", true) => "不改商品外观；文案和图片按 Ozon 实时数据校验。".to_string(),
        ("zh-CN", false) => "当前是本地 mock 模式，正式出图前请切到真实 Ozon API 再校验一次。".to_string(),
        (_, true) => "Do not alter the product appearance; copy and images are checked against live Ozon data.".to_string(),
        (_, false) => "This is local mock mode. Switch to the real Ozon API and verify again before production poster work.".to_string(),
    }
}

fn poster_overlay_language(locale: &str) -> &'static str {
    match locale {
        "zh-CN" => "Chinese",
        _ => "English",
    }
}

fn poster_source_images(
    product: &ozon_connector::OzonProductDetail,
    locale: &str,
) -> Vec<PosterSourceImage> {
    let mut images = Vec::new();
    if let Some(primary) = product.primary_image.as_deref() {
        images.push(PosterSourceImage {
            role: "primary".to_string(),
            url: primary.to_string(),
            note: match locale {
                "zh-CN" => "主图，优先作为商品外观参考".to_string(),
                _ => "Primary image; use it as the first product appearance reference".to_string(),
            },
        });
    }
    for image in product.images.iter().take(8) {
        if images.iter().any(|existing| existing.url == image.url) {
            continue;
        }
        images.push(PosterSourceImage {
            role: format!("{:?}", image.role).to_ascii_lowercase(),
            url: image.url.clone(),
            note: match locale {
                "zh-CN" => format!("Ozon 图片序号 {}", image.position),
                _ => format!("Ozon image position {}", image.position),
            },
        });
    }
    images
}

fn build_openclaw_poster_prompt(
    product: &ozon_connector::OzonProductDetail,
    brief: &PosterBrief,
    source_images: &[PosterSourceImage],
    locale: &str,
) -> String {
    let product_name = product.name.as_deref().unwrap_or_else(|| {
        if locale == "zh-CN" {
            "未命名商品"
        } else {
            "Unnamed product"
        }
    });
    let image_lines = if source_images.is_empty() {
        match locale {
            "zh-CN" => "当前商品没有可用图片 URL。先提醒运营补充商品图，不要凭空生成商品外观。".to_string(),
            _ => "This product has no available image URL. Ask the operator to add product images before generating appearance.".to_string(),
        }
    } else {
        source_images
            .iter()
            .enumerate()
            .map(|(index, image)| format!("{}. [{}] {}", index + 1, image.role, image.url))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let selling_points = brief
        .selling_points
        .iter()
        .filter(|point| !point.trim().is_empty())
        .map(|point| format!("- {point}"))
        .collect::<Vec<_>>()
        .join("\n");
    let sku = product
        .sku
        .as_deref()
        .unwrap_or_else(|| if locale == "zh-CN" { "无" } else { "none" });
    let archived = product
        .archived
        .map(|value| match (locale, value) {
            ("zh-CN", true) => "已归档",
            ("zh-CN", false) => "未归档",
            (_, true) => "archived",
            (_, false) => "not archived",
        })
        .unwrap_or_else(|| {
            if locale == "zh-CN" {
                "未知"
            } else {
                "unknown"
            }
        });
    if locale == "zh-CN" {
        format!(
            "你现在接手一个 Ozon 商品海报任务。请使用你当前登录的 OpenClaw/Codex 图片能力完成，不要要求用户额外提供 OpenAI API Key。\n\n商品事实：\n- 商品名：{}\n- offer_id：{}\n- product_id：{}\n- sku：{}\n- 归档状态：{}\n\n商品图片 URL：\n{}\n\n海报文案草稿：\n标题：{}\n副标题：{}\n卖点：\n{}\n收尾句：{}\n校验说明：{}\n\n设计要求：\n1. 输出 4:5 竖版电商宣传海报，适合 Ozon 店铺运营先看第一版。\n2. 商品外观必须以提供的图片为准，保留包装、颜色、标签、比例和可见文字，不要把商品改成其他款式。\n3. 可以补背景、灯光、陈列和氛围，但不要生成不存在的 Logo、认证、折扣、功效或品牌合作。\n4. 海报上只放上面给出的标题、卖点和收尾句；中文不要写错字，俄文/英文商品名不要擅自翻译成另一个意思。\n5. 完成后请顺手列出 3 条自检：商品是否一致、文案是否越界、图片是否有错字。\n\n背景方向：{}",
            truncate_text(product_name, 120),
            product.offer_id,
            product.product_id,
            sku,
            archived,
            image_lines,
            brief.headline,
            brief.subheadline,
            selling_points,
            brief.cta_line,
            brief.compliance_note,
            brief.background_prompt
        )
    } else {
        format!(
            "You are taking over an Ozon product poster task. Use the currently signed-in OpenClaw/Codex image capability; do not ask the user for an additional OpenAI API key.\n\nProduct facts:\n- Product name: {}\n- offer_id: {}\n- product_id: {}\n- sku: {}\n- Archive status: {}\n\nProduct image URLs:\n{}\n\nPoster copy draft:\nHeadline: {}\nSubheadline: {}\nSelling points:\n{}\nClosing line: {}\nVerification note: {}\n\nDesign requirements:\n1. Produce a finished 4:5 portrait e-commerce poster suitable for an Ozon shop operator to review as a first draft.\n2. Product appearance must follow the supplied images. Preserve packaging, color, labels, proportions, and visible text. Do not turn it into another product variant.\n3. You may add background, lighting, display setting, and mood, but do not invent logos, certifications, discounts, benefits, or brand partnerships.\n4. Put only the headline, selling points, and closing line above on the poster. Do not mistranslate Russian or English product names into a different meaning.\n5. After generation, list 3 self-checks: product consistency, copy overclaiming, and text/image errors.\n\nBackground direction: {}",
            truncate_text(product_name, 120),
            product.offer_id,
            product.product_id,
            sku,
            archived,
            image_lines,
            brief.headline,
            brief.subheadline,
            selling_points,
            brief.cta_line,
            brief.compliance_note,
            brief.background_prompt
        )
    }
}

fn poster_handoff_instructions(locale: &str) -> Vec<&'static str> {
    match locale {
        "zh-CN" => vec![
            "优先使用用户已登录的 OpenClaw/Codex 图片能力；这条路径不要求 Ozon Local 保存 OpenAI API Key。",
            "使用提供的 Ozon 图片 URL 作为商品参考，保留包装、颜色、标签、比例和可见文字。",
            "生成 4:5 竖版电商海报；不要编造来源事实里没有的认证、折扣、品牌合作或产品功能。",
            "如果商品信息不明确，文案保持保守，并在加强卖点前先询问操作员。",
        ],
        _ => vec![
            "Prefer the user's signed-in OpenClaw/Codex image capability; this path does not require Ozon Local to save an OpenAI API key.",
            "Use the supplied Ozon image URLs as product references and preserve packaging, color, labels, proportions, and visible text.",
            "Generate a finished 4:5 marketplace poster; do not invent certifications, discounts, brand partnerships, or product functions that are not present in the source facts.",
            "If any product detail is ambiguous, keep the claim conservative and ask the operator before strengthening copy.",
        ],
    }
}

fn poster_verify_warnings(locale: &str, ok: bool) -> Vec<String> {
    match (locale, ok) {
        ("zh-CN", true) => vec![
            "校验通过：当前文案与系统生成稿一致。".to_string(),
            "商品主体应继续使用真实主图合成，避免让图片模型重画包装和文字。".to_string(),
        ],
        ("zh-CN", false) => vec![
            "当前文案和系统生成稿不一致，建议回到商品属性再确认改写是否安全。".to_string(),
            "这一步只做逐字段比对，不会帮你猜测哪些自由改写仍然安全。".to_string(),
        ],
        (_, true) => vec![
            "Check passed: the current copy matches the system brief.".to_string(),
            "Keep the product body composited from the real main image; do not let the image model redraw packaging or text.".to_string(),
        ],
        (_, false) => vec![
            "The current copy does not match the system brief. Recheck the product attributes before using this rewrite.".to_string(),
            "This check only compares fields. It does not guess whether freeform copy changes are still safe.".to_string(),
        ],
    }
}

fn attribute_selling_point(attribute: &ozon_connector::OzonProductAttribute) -> Option<String> {
    let name = attribute.name.as_deref()?.trim();
    let value = attribute.values.first()?.trim();
    if name.is_empty() || value.is_empty() || is_low_value_attribute(name) {
        return None;
    }
    Some(format!("{name}: {}", truncate_text(value, 22)))
}

fn is_low_value_attribute(name: &str) -> bool {
    let normalized = name.trim().to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "id" | "sku" | "barcode" | "barcodes" | "offer id" | "offer_id"
    )
}

async fn generate_poster_background(
    state: &LocalState,
    brief: &PosterBrief,
) -> Result<PosterGeneratedBackground, ApiError> {
    let openai = resolve_capability(state, Capability::ImageGen)
        .await?
        .expect_openai_image()?;
    let request = OpenAiImageGenerationRequest {
        model: openai.image_model.clone(),
        prompt: brief.background_prompt.clone(),
        size: "1024x1536".to_string(),
    };
    let response = state
        .http_client
        .post(openai_images_endpoint(&openai.base_url))
        .bearer_auth(openai.api_key)
        .json(&request)
        .send()
        .await
        .map_err(|error| {
            ApiError::bad_gateway(format!("OpenAI image generation failed: {error}"))
        })?;
    if !response.status().is_success() {
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "unknown error".to_string());
        return Err(ApiError::bad_gateway(format!(
            "OpenAI image generation failed: {}",
            summarize_openai_error(&body)
        )));
    }
    let payload: OpenAiImageGenerationResponse = response.json().await.map_err(|error| {
        ApiError::bad_gateway(format!("invalid OpenAI image response: {error}"))
    })?;
    let image = payload
        .data
        .into_iter()
        .next()
        .ok_or_else(|| ApiError::bad_gateway("OpenAI image response returned no images"))?;
    let b64 = image.b64_json.ok_or_else(|| {
        ApiError::bad_gateway("OpenAI image response did not include base64 image data")
    })?;
    let bytes = BASE64_STANDARD
        .decode(b64.as_bytes())
        .map_err(|_| ApiError::bad_gateway("OpenAI image response returned invalid base64 data"))?;
    Ok(PosterGeneratedBackground {
        image_model: openai.image_model,
        prompt: request.prompt,
        revised_prompt: image.revised_prompt,
        background_data_url: format!("data:image/png;base64,{}", BASE64_STANDARD.encode(bytes)),
    })
}

// ------------------------------------------------------------------------- //
// Module 4 — export / delivery (LOCAL ONLY, no Ozon push).                    //
//                                                                             //
// Inject reviewed per-row {title, listing, primary_image, additional_images}  //
// into an Ozon template .xlsx via the packaged Python injector, then run the  //
// engine `process --verify` (which writes the deliverable AND proves only the //
// mapped title/listing/image cells changed). On exit code 1 the verifier      //
// found unexpected/frozen-cell changes — the deliverable is contaminated and  //
// is NEVER returned as usable.                                                //
//                                                                             //
// NOTE: this needs the Python engine. In dev we use the repo .venv            //
// (tools/ozon-excel-core/.venv); a shipped installer must bundle Python       //
// (follow-up, out of scope here). tokio is built WITHOUT the `process`        //
// feature, so the blocking child runs inside tokio::task::spawn_blocking with //
// std::process::Command (explicit args slice + absolute interpreter, never a  //
// joined shell string — paths may contain spaces).                            //
// ------------------------------------------------------------------------- //

/// Default dev location of the pure-openpyxl engine (repo `tools/ozon-excel-core`).
const ENGINE_ROOT_DEV_DEFAULT: &str = "/Users/bill/ozon-rust-suite/tools/ozon-excel-core";
const RELIST_EXPORT_MAX_ROWS: usize = 200;

/// Resolve the Python interpreter for the export engine:
///   OZON_EXCEL_CORE_PYTHON / OZON_PYTHON env -> repo .venv -> `python3` on PATH.
fn python_interpreter() -> PathBuf {
    for key in ["OZON_EXCEL_CORE_PYTHON", "OZON_PYTHON"] {
        if let Ok(value) = env::var(key) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return PathBuf::from(trimmed);
            }
        }
    }
    let venv = engine_root().join(".venv").join("bin").join("python");
    if venv.exists() {
        return venv;
    }
    PathBuf::from("python3")
}

/// Resolve the engine root (where `src/ozon_excel_core` + `fields.example.yaml`
/// live). Overridable via OZON_EXCEL_CORE_ROOT; dev default is the repo path.
fn engine_root() -> PathBuf {
    if let Ok(value) = env::var("OZON_EXCEL_CORE_ROOT") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    PathBuf::from(ENGINE_ROOT_DEV_DEFAULT)
}

/// Default config (`fields.example.yaml`) shipped alongside the engine.
fn default_export_config_path() -> PathBuf {
    engine_root().join("fields.example.yaml")
}

/// `<data-dir>/exports/` — created on demand. Mirrors `default_secret_file_path`'s
/// per-OS data-dir resolution so deliverables land next to the local node's state.
fn exports_dir() -> PathBuf {
    let base = default_secret_file_path()
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("exports")
}

#[derive(Debug, Deserialize)]
struct RelistExportRequest {
    template_path: String,
    #[serde(default)]
    config_path: Option<String>,
    #[serde(default)]
    rows: Vec<ExportRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExportRow {
    #[serde(default)]
    title: String,
    #[serde(default)]
    listing: String,
    #[serde(default)]
    primary_image_url: Option<String>,
    #[serde(default)]
    additional_image_urls: Vec<String>,
}

/// The shape the Python injector consumes (one object per row in rows.json).
#[derive(Debug, Serialize)]
struct InjectRow {
    title: String,
    listing: String,
    primary_image: Option<String>,
    additional_images: Vec<String>,
}

#[derive(Debug, Serialize)]
struct RelistExportResponse {
    ok: bool,
    out_path: String,
    file_url: String,
    verify: Option<VerifySummary>,
    warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
struct VerifySummary {
    ok: bool,
    expected_changes: u64,
    unexpected_changes: u64,
    frozen_cells_compared: u64,
    sheets_compared: u64,
}

/// Outcome of the blocking engine pipeline (runs inside spawn_blocking).
struct ExportPipelineOutput {
    out_path: PathBuf,
    verify: Option<VerifySummary>,
    warnings: Vec<String>,
}

async fn relist_export(
    State(state): State<LocalState>,
    headers: HeaderMap,
    Json(input): Json<RelistExportRequest>,
) -> Result<Json<RelistExportResponse>, ApiError> {
    require_bridge_or_operator_token(&state, &headers)?;
    require_valid_lease_with_feature(&state, Feature::OzonRead).await?;

    let template_path = input.template_path.trim().to_string();
    if template_path.is_empty() {
        return Err(ApiError::bad_request("template_path is required"));
    }
    let template = PathBuf::from(&template_path);
    if !template.is_file() {
        return Err(ApiError::bad_request(format!(
            "template .xlsx not found: {template_path}"
        )));
    }
    if input.rows.is_empty() {
        return Err(ApiError::bad_request("provide at least one export row"));
    }
    if input.rows.len() > RELIST_EXPORT_MAX_ROWS {
        return Err(ApiError::bad_request(format!(
            "export at most {RELIST_EXPORT_MAX_ROWS} rows per call"
        )));
    }

    let config_path = input
        .config_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(default_export_config_path);
    if !config_path.is_file() {
        return Err(ApiError::bad_request(format!(
            "config not found: {}",
            config_path.display()
        )));
    }

    let inject_rows: Vec<InjectRow> = input
        .rows
        .into_iter()
        .map(|row| InjectRow {
            title: row.title,
            listing: row.listing,
            primary_image: row
                .primary_image_url
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            additional_images: row
                .additional_image_urls
                .into_iter()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .collect(),
        })
        .collect();

    let exports = exports_dir();
    let out_path = exports.join(format!(
        "ozon-deliverable-{}.xlsx",
        Uuid::new_v4().simple()
    ));
    let python = python_interpreter();
    let root = engine_root();

    let output = tokio::task::spawn_blocking(move || {
        run_export_pipeline(python, root, template, config_path, exports, out_path, inject_rows)
    })
    .await
    .map_err(|_| ApiError::internal("export task panicked"))??;

    let file_url = format!("file://{}", output.out_path.display());
    Ok(Json(RelistExportResponse {
        ok: true,
        out_path: output.out_path.display().to_string(),
        file_url,
        verify: output.verify,
        warnings: output.warnings,
    }))
}

/// Blocking engine pipeline: inject rows into the template, then run
/// `process --verify` to write + prove the deliverable. Maps the engine's
/// deterministic exit codes (0/1/2/3) onto ApiError. MUST run inside
/// tokio::task::spawn_blocking — it uses std::process::Command (no tokio
/// `process` feature) and synchronous fs.
fn run_export_pipeline(
    python: PathBuf,
    root: PathBuf,
    template: PathBuf,
    config_path: PathBuf,
    exports: PathBuf,
    out_path: PathBuf,
    rows: Vec<InjectRow>,
) -> Result<ExportPipelineOutput, ApiError> {
    use std::process::Command;

    fs::create_dir_all(&exports)
        .map_err(|err| ApiError::internal(format!("cannot create exports dir: {err}")))?;

    // Intermediate populated workbook + rows.json live next to the deliverable.
    let stem = Uuid::new_v4().simple().to_string();
    let rows_json = exports.join(format!("export-rows-{stem}.json"));
    let populated = exports.join(format!("export-populated-{stem}.xlsx"));

    let rows_body = serde_json::to_vec(&rows)
        .map_err(|err| ApiError::internal(format!("cannot serialize export rows: {err}")))?;
    fs::write(&rows_json, rows_body)
        .map_err(|err| ApiError::internal(format!("cannot write rows.json: {err}")))?;

    // PYTHONPATH points at the engine's src so `-m ozon_excel_core.cli` resolves
    // in the dev tree even without an editable install. We deliberately do NOT
    // forward any secrets into the child env.
    let pythonpath = root.join("src");
    let cleanup = |extra: &[&PathBuf]| {
        let _ = fs::remove_file(&rows_json);
        for path in extra {
            let _ = fs::remove_file(path);
        }
    };

    let template_arg = template.to_string_lossy().to_string();
    let rows_arg = rows_json.to_string_lossy().to_string();
    let populated_arg = populated.to_string_lossy().to_string();
    let config_arg = config_path.to_string_lossy().to_string();

    // 1) inject -> populated.xlsx
    let inject = Command::new(&python)
        .current_dir(&root)
        .env("PYTHONPATH", &pythonpath)
        .args([
            "-m",
            "ozon_excel_core.cli",
            "inject",
            "--in",
            &template_arg,
            "--rows",
            &rows_arg,
            "--out",
            &populated_arg,
            "--config",
            &config_arg,
            "--quiet",
        ])
        .output();
    let inject = match inject {
        Ok(out) => out,
        Err(err) => {
            cleanup(&[]);
            return Err(ApiError::internal(format!(
                "failed to launch Python engine ({}): {err}",
                python.display()
            )));
        }
    };
    if !inject.status.success() {
        let code = inject.status.code().unwrap_or(-1);
        let stderr = summarize_stderr(&inject.stderr);
        cleanup(&[&populated]);
        return Err(map_engine_exit(code, &stderr, "inject"));
    }

    // 2) process --verify -> deliverable (writes AND proves the frozen set).
    let out_arg = out_path.to_string_lossy().to_string();
    let process = Command::new(&python)
        .current_dir(&root)
        .env("PYTHONPATH", &pythonpath)
        .args([
            "-m",
            "ozon_excel_core.cli",
            "process",
            "--in",
            &populated_arg,
            "--out",
            &out_arg,
            "--config",
            &config_arg,
            "--transform",
            "identity",
            "--verify",
            "--quiet",
        ])
        .output();
    let process = match process {
        Ok(out) => out,
        Err(err) => {
            cleanup(&[&populated]);
            return Err(ApiError::internal(format!(
                "failed to launch Python engine ({}): {err}",
                python.display()
            )));
        }
    };
    if !process.status.success() {
        let code = process.status.code().unwrap_or(-1);
        let stderr = summarize_stderr(&process.stderr);
        // Exit 1 = the verifier found unexpected/frozen-cell changes: the
        // deliverable is contaminated. Never return it — delete it.
        let _ = fs::remove_file(&out_path);
        cleanup(&[&populated]);
        return Err(map_engine_exit(code, &stderr, "process --verify"));
    }

    // 3) verify --report json (best-effort) to fill the structured summary.
    let mut warnings = Vec::new();
    let verify = match Command::new(&python)
        .current_dir(&root)
        .env("PYTHONPATH", &pythonpath)
        .args([
            "-m",
            "ozon_excel_core.cli",
            "verify",
            "--in",
            &populated_arg,
            "--out",
            &out_arg,
            "--config",
            &config_arg,
            "--report",
            "json",
        ])
        .output()
    {
        Ok(out) if out.status.success() => parse_verify_summary(&out.stdout),
        Ok(_) => {
            warnings.push("verify --report json did not return a parseable summary".to_string());
            None
        }
        Err(err) => {
            warnings.push(format!("verify --report json could not run: {err}"));
            None
        }
    };

    cleanup(&[&populated]);
    Ok(ExportPipelineOutput {
        out_path,
        verify,
        warnings,
    })
}

/// Map the engine's deterministic exit codes onto ApiError.
///   0 -> ok (callers never reach here with 0)
///   1 -> verification found unexpected changes (HARD FAIL; deliverable unsafe)
///   2 -> config / mapping error
///   3 -> preflight risk
fn map_engine_exit(code: i32, stderr: &str, stage: &str) -> ApiError {
    let detail = if stderr.is_empty() {
        String::new()
    } else {
        format!(" — {stderr}")
    };
    match code {
        1 => ApiError::bad_gateway(format!(
            "verification found unexpected changes — deliverable is unsafe and was discarded{detail}"
        )),
        2 => ApiError::bad_request(format!("config/mapping error ({stage}){detail}")),
        3 => ApiError::bad_request(format!("preflight risk ({stage}){detail}")),
        other => ApiError::internal(format!(
            "export engine failed at {stage} (exit {other}){detail}"
        )),
    }
}

/// Summarize child stderr to a single bounded line for the API error message.
fn summarize_stderr(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let last = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .next_back()
        .unwrap_or("");
    if last.len() > 280 {
        format!("{}…", &last[..277])
    } else {
        last.to_string()
    }
}

/// Parse `verify --report json` stdout into the structured VerifySummary.
fn parse_verify_summary(stdout: &[u8]) -> Option<VerifySummary> {
    let value: serde_json::Value = serde_json::from_slice(stdout).ok()?;
    let summary = value.get("summary")?;
    let as_u64 = |key: &str| summary.get(key).and_then(serde_json::Value::as_u64).unwrap_or(0);
    Some(VerifySummary {
        ok: value.get("ok").and_then(serde_json::Value::as_bool).unwrap_or(false),
        expected_changes: as_u64("expected_changes"),
        unexpected_changes: as_u64("unexpected_changes"),
        frozen_cells_compared: as_u64("frozen_cells_compared"),
        sheets_compared: as_u64("sheets_compared"),
    })
}

// ------------------------------------------------------------------------- //
// Module 1 intake (READ-ONLY): read a supplier .xlsx with the Python engine and
// project each product row to JSON so the workbench can merge imported rows into
// the relist list. NO inject / process / push — this never mutates a workbook or
// touches Ozon.
// ------------------------------------------------------------------------- //

#[derive(Debug, Deserialize)]
struct RelistExtractRequest {
    template_path: String,
    #[serde(default)]
    config_path: Option<String>,
    #[serde(default)]
    sheet: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ExtractedRow {
    #[serde(default)]
    sheet: String,
    #[serde(default)]
    row: u64,
    #[serde(default)]
    sku: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    listing: Option<String>,
    #[serde(default)]
    images_main: Vec<String>,
    #[serde(default)]
    images_additional: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ExtractStdout {
    #[serde(default)]
    rows: Vec<ExtractedRow>,
}

#[derive(Debug, Serialize)]
struct RelistExtractResponse {
    rows: Vec<ExtractedRow>,
}

async fn relist_extract(
    State(state): State<LocalState>,
    headers: HeaderMap,
    Json(input): Json<RelistExtractRequest>,
) -> Result<Json<RelistExtractResponse>, ApiError> {
    require_bridge_or_operator_token(&state, &headers)?;
    require_valid_lease_with_feature(&state, Feature::OzonRead).await?;

    let template_path = input.template_path.trim().to_string();
    if template_path.is_empty() {
        return Err(ApiError::bad_request("template_path is required"));
    }
    let template = PathBuf::from(&template_path);
    if !template.is_file() {
        return Err(ApiError::bad_request(format!(
            "template .xlsx not found: {template_path}"
        )));
    }

    let config_path = input
        .config_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(default_export_config_path);
    if !config_path.is_file() {
        return Err(ApiError::bad_request(format!(
            "config not found: {}",
            config_path.display()
        )));
    }

    let sheet = input
        .sheet
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let python = python_interpreter();
    let root = engine_root();

    let rows = tokio::task::spawn_blocking(move || {
        run_extract_pipeline(python, root, template, config_path, sheet)
    })
    .await
    .map_err(|_| ApiError::internal("extract task panicked"))??;

    Ok(Json(RelistExtractResponse { rows }))
}

/// Blocking read-only engine call: `-m ozon_excel_core.cli extract` and parse the
/// JSON object it prints to stdout. Maps the engine's deterministic exit codes
/// (0/2/3) onto ApiError. MUST run inside tokio::task::spawn_blocking — it uses
/// std::process::Command (no tokio `process` feature). No secrets are forwarded.
fn run_extract_pipeline(
    python: PathBuf,
    root: PathBuf,
    template: PathBuf,
    config_path: PathBuf,
    sheet: Option<String>,
) -> Result<Vec<ExtractedRow>, ApiError> {
    use std::process::Command;

    let pythonpath = root.join("src");
    let template_arg = template.to_string_lossy().to_string();
    let config_arg = config_path.to_string_lossy().to_string();

    let mut args: Vec<String> = vec![
        "-m".into(),
        "ozon_excel_core.cli".into(),
        "extract".into(),
        "--in".into(),
        template_arg,
        "--config".into(),
        config_arg,
    ];
    if let Some(sheet) = sheet {
        args.push("--sheet".into());
        args.push(sheet);
    }

    let output = Command::new(&python)
        .current_dir(&root)
        .env("PYTHONPATH", &pythonpath)
        .args(&args)
        .output();
    let output = match output {
        Ok(out) => out,
        Err(err) => {
            return Err(ApiError::internal(format!(
                "failed to launch Python engine ({}): {err}",
                python.display()
            )));
        }
    };
    if !output.status.success() {
        let code = output.status.code().unwrap_or(-1);
        let stderr = summarize_stderr(&output.stderr);
        return Err(map_engine_exit(code, &stderr, "extract"));
    }

    let parsed: ExtractStdout = serde_json::from_slice(&output.stdout).map_err(|err| {
        ApiError::bad_gateway(format!("extract did not return parseable JSON: {err}"))
    })?;
    Ok(parsed.rows)
}

// ------------------------------------------------------------------------- //
// Module 1 intake (READ-ONLY): accept an operator-dragged image as base64-in-JSON,
// host it publicly, and return the hosted URL so the workbench can set it as a
// candidate on a relist row. PNG is required: the image-host upload part is hard
// PNG (relist_upload_to_host), and we deliberately do NOT pull in a heavy image
// transcoder crate just for this — non-PNG drops are rejected with a clear error.
// ------------------------------------------------------------------------- //

#[derive(Debug, Deserialize)]
struct RelistImportImageRequest {
    #[serde(default)]
    filename: Option<String>,
    data_base64: String,
}

#[derive(Debug, Serialize)]
struct RelistImportImageResponse {
    new_url: String,
}

/// The 8-byte PNG magic number. We only accept PNG bytes here (see module note).
const PNG_MAGIC: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

async fn relist_import_image(
    State(state): State<LocalState>,
    headers: HeaderMap,
    Json(input): Json<RelistImportImageRequest>,
) -> Result<Json<RelistImportImageResponse>, ApiError> {
    require_bridge_or_operator_token(&state, &headers)?;
    require_valid_lease_with_feature(&state, Feature::OzonRead).await?;

    // Tolerate a `data:image/png;base64,...` data-URL prefix from the browser.
    let raw = input.data_base64.trim();
    let raw = raw.rsplit_once(',').map(|(_, b)| b).unwrap_or(raw);
    if raw.is_empty() {
        return Err(ApiError::bad_request("data_base64 is required"));
    }
    let bytes = BASE64_STANDARD
        .decode(raw.as_bytes())
        .map_err(|_| ApiError::bad_request("data_base64 is not valid base64"))?;
    if bytes.len() < PNG_MAGIC.len() || &bytes[..PNG_MAGIC.len()] != PNG_MAGIC {
        return Err(ApiError::bad_request(
            "only PNG images are accepted — please drop a .png file (other formats are not supported by this local helper)",
        ));
    }

    let filename = input
        .filename
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|name| {
            if name.to_ascii_lowercase().ends_with(".png") {
                name.to_string()
            } else {
                format!("{name}.png")
            }
        })
        .unwrap_or_else(|| format!("import-{}.png", Uuid::new_v4().simple()));

    let new_url = relist_host_image(&state, &filename, bytes).await?;
    Ok(Json(RelistImportImageResponse { new_url }))
}

// ------------------------------------------------------------------------- //
// Re-listing workbench: restyle a product's primary image (GPT image-edit),
// host it publicly, and push it back to Ozon as the new primary image.
// Images go through the API (proven-reliable). Title/listing are intentionally
// NOT pushed here — they belong on the Excel-upload route per the category
// title-template gotcha.
// ------------------------------------------------------------------------- //

#[derive(Debug, Deserialize)]
struct RelistGenerateRequest {
    #[serde(default)]
    targets: Vec<RelistTarget>,
    #[serde(default)]
    prompt: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RelistTarget {
    #[serde(default)]
    product_id: Option<String>,
    #[serde(default)]
    offer_id: Option<String>,
}

impl RelistTarget {
    fn label(&self) -> String {
        self.offer_id
            .clone()
            .or_else(|| self.product_id.clone())
            .unwrap_or_else(|| "(unknown)".to_string())
    }

    fn into_lookup(self) -> OzonProductLookup {
        let normalized = OzonProductLookup {
            product_id: self.product_id,
            offer_id: self.offer_id,
            sku: None,
        }
        .normalized();
        // The connector requires EXACTLY one identifier, but the workbench sends
        // both product_id + offer_id for every selected row (the product list
        // carries both). Prefer the canonical numeric product_id; fall back to
        // offer_id only when product_id is absent.
        if normalized.product_id.is_some() {
            OzonProductLookup {
                product_id: normalized.product_id,
                offer_id: None,
                sku: None,
            }
        } else {
            normalized
        }
    }
}

#[derive(Debug, Serialize)]
struct RelistGenerateResponse {
    connector_mode: &'static str,
    prompt: String,
    items: Vec<RelistItem>,
}

#[derive(Debug, Serialize)]
struct RelistItem {
    product_id: String,
    offer_id: String,
    name: Option<String>,
    original_url: Option<String>,
    /// Up to RELIST_MAX_CANDIDATES hosted restyle URLs; the operator selects one
    /// in the UI. Empty when generation failed (see `error`).
    candidates: Vec<String>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RelistPushRequest {
    #[serde(default)]
    items: Vec<RelistPushTarget>,
}

#[derive(Debug, Deserialize, Clone)]
struct RelistPushTarget {
    product_id: String,
    new_primary_url: String,
}

#[derive(Debug, Serialize)]
struct RelistPushResponse {
    connector_mode: &'static str,
    items: Vec<RelistPushResult>,
}

#[derive(Debug, Serialize)]
struct RelistPushResult {
    product_id: String,
    primary_url: String,
    image_count: u32,
    ok: bool,
    error: Option<String>,
}

async fn relist_generate(
    State(state): State<LocalState>,
    headers: HeaderMap,
    Json(input): Json<RelistGenerateRequest>,
) -> Result<Json<RelistGenerateResponse>, ApiError> {
    require_bridge_or_operator_token(&state, &headers)?;
    require_valid_lease_with_feature(&state, Feature::OzonRead).await?;
    if input.targets.is_empty() {
        return Err(ApiError::bad_request(
            "select at least one product to restyle",
        ));
    }
    if input.targets.len() > RELIST_MAX_BATCH {
        return Err(ApiError::bad_request(format!(
            "restyle at most {RELIST_MAX_BATCH} products per batch"
        )));
    }
    let credentials = load_ozon_credentials(&state).await?;
    // No override -> compose a per-product Russian-Ozon prompt (injects the
    // product name so the headline/selling points are accurate).
    let prompt_override = input
        .prompt
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let mut items = Vec::with_capacity(input.targets.len());
    for target in input.targets {
        let label = target.label();
        match relist_generate_one(
            &state,
            &credentials,
            target.into_lookup(),
            prompt_override.as_deref(),
        )
        .await
        {
            Ok(item) => items.push(item),
            Err(error) => items.push(RelistItem {
                product_id: String::new(),
                offer_id: label,
                name: None,
                original_url: None,
                candidates: Vec::new(),
                error: Some(error),
            }),
        }
    }

    Ok(Json(RelistGenerateResponse {
        connector_mode: connector_mode(&state),
        prompt: prompt_override
            .unwrap_or_else(|| "auto (per-product Russian Ozon template)".to_string()),
        items,
    }))
}

// --------------------------------------------------------------------------- //
// Module 3 ("copy") — recognize half (M3-PR1).                                //
//                                                                             //
// READ-ONLY: per product, read the 4 source fields, send them WITH the        //
// product images to an OpenAI-compatible vision chat model, and return the    //
// 4 REDISTRIBUTED fields as a reviewable Russian proposal. There is NO        //
// write-back to Ozon and NO attribute-id remapping in this PR (M3-PR2).       //
// --------------------------------------------------------------------------- //

/// Max product images attached to the multimodal request, to bound tokens/cost.
const MODULE3_MAX_IMAGES: usize = 6;
const MODULE3_TITLE_MAX: usize = 200;
const MODULE3_DESCRIPTION_MAX: usize = 1800;
const MODULE3_MAX_BATCH: usize = RELIST_MAX_BATCH;
const MODULE3_DEFAULT_LANGUAGE: &str = "ru";

#[derive(Debug, Deserialize)]
struct Module3RecognizeRequest {
    #[serde(default)]
    targets: Vec<RelistTarget>,
    #[serde(default)]
    target_language: Option<String>,
}

#[derive(Debug, Serialize)]
struct Module3RecognizeResponse {
    connector_mode: &'static str,
    items: Vec<Module3Item>,
}

#[derive(Debug, Serialize)]
struct Module3Item {
    product_id: String,
    offer_id: String,
    source: Module3Fields,
    proposal: Option<Module3Fields>,
    error: Option<String>,
}

/// The 4 module-3 fields, in both the source (read from Ozon) and proposal
/// (redistributed by the model) directions.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Module3Fields {
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    attributes: Vec<Module3Attribute>,
    #[serde(default)]
    type_category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Module3Attribute {
    #[serde(default)]
    name: String,
    #[serde(default)]
    values: Vec<String>,
}

async fn module3_recognize(
    State(state): State<LocalState>,
    headers: HeaderMap,
    Json(input): Json<Module3RecognizeRequest>,
) -> Result<Json<Module3RecognizeResponse>, ApiError> {
    require_bridge_or_operator_token(&state, &headers)?;
    require_valid_lease_with_feature(&state, Feature::OzonRead).await?;
    if input.targets.is_empty() {
        return Err(ApiError::bad_request(
            "select at least one product to recognize",
        ));
    }
    if input.targets.len() > MODULE3_MAX_BATCH {
        return Err(ApiError::bad_request(format!(
            "recognize at most {MODULE3_MAX_BATCH} products per batch"
        )));
    }
    let language = input
        .target_language
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| MODULE3_DEFAULT_LANGUAGE.to_string());
    let credentials = load_ozon_credentials(&state).await?;

    let mut items = Vec::with_capacity(input.targets.len());
    for target in input.targets {
        let label = target.label();
        match module3_recognize_one(&state, &credentials, target.into_lookup(), &language).await {
            Ok(item) => items.push(item),
            Err(error) => items.push(Module3Item {
                product_id: String::new(),
                offer_id: label,
                source: Module3Fields::default(),
                proposal: None,
                error: Some(error),
            }),
        }
    }

    Ok(Json(Module3RecognizeResponse {
        connector_mode: connector_mode(&state),
        items,
    }))
}

impl Default for Module3Fields {
    fn default() -> Self {
        Self {
            title: String::new(),
            description: String::new(),
            attributes: Vec::new(),
            type_category: String::new(),
        }
    }
}

async fn module3_recognize_one(
    state: &LocalState,
    credentials: &OzonCredentials,
    lookup: OzonProductLookup,
    language: &str,
) -> Result<Module3Item, String> {
    let product = state
        .ozon_connector
        .product_get(credentials, lookup)
        .await
        .map_err(|error| format!("read product failed: {error}"))?;

    let source = module3_source_fields(&product);
    let document = module3_labeled_document(&source);
    let images = module3_image_urls(&product);

    // On any transport/parse failure, surface a per-item error and echo the raw
    // source fields as the proposal so the operator still sees something.
    let (proposal, error) =
        match module3_request_proposal(state, language, &document, &images).await {
            Ok(content) => match parse_module3_result(&content) {
                Ok(mut fields) => {
                    fields.title = word_boundary_truncate(&fields.title, MODULE3_TITLE_MAX);
                    fields.description =
                        word_boundary_truncate(&fields.description, MODULE3_DESCRIPTION_MAX);
                    (Some(fields), None)
                }
                Err(parse_error) => (Some(source.clone()), Some(parse_error)),
            },
            Err(api_error) => (Some(source.clone()), Some(api_error.message)),
        };

    Ok(Module3Item {
        product_id: product.product_id,
        offer_id: product.offer_id,
        source,
        proposal,
        error,
    })
}

/// Read the 4 module-3 source fields off a product detail.
fn module3_source_fields(product: &ozon_connector::OzonProductDetail) -> Module3Fields {
    let title = product
        .name
        .as_deref()
        .map(str::trim)
        .unwrap_or("")
        .to_string();
    let description = product
        .description
        .as_deref()
        .map(str::trim)
        .unwrap_or("")
        .to_string();
    let attributes = product
        .attributes
        .iter()
        .filter_map(|attribute| {
            let name = attribute.name.as_deref().map(str::trim).unwrap_or("");
            let values: Vec<String> = attribute
                .values
                .iter()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .collect();
            if name.is_empty() && values.is_empty() {
                return None;
            }
            Some(Module3Attribute {
                name: name.to_string(),
                values,
            })
        })
        .collect();
    let type_category = module3_type_category(product);
    Module3Fields {
        title,
        description,
        attributes,
        type_category,
    }
}

fn module3_type_category(product: &ozon_connector::OzonProductDetail) -> String {
    let mut parts = Vec::new();
    if let Some(type_id) = product.type_id {
        parts.push(format!("type_id={type_id}"));
    }
    if let Some(category_id) = product.description_category_id {
        parts.push(format!("description_category_id={category_id}"));
    }
    parts.join(", ")
}

/// Concatenate the 4 source fields into one labeled document for the model.
fn module3_labeled_document(fields: &Module3Fields) -> String {
    let mut attributes = String::new();
    for attribute in &fields.attributes {
        let name = if attribute.name.is_empty() {
            "(без названия)"
        } else {
            attribute.name.as_str()
        };
        attributes.push_str(&format!("{name}: {}\n", attribute.values.join("; ")));
    }
    format!(
        "===TITLE===\n{title}\n\n===DESCRIPTION===\n{description}\n\n===ATTRIBUTES===\n{attributes}\n===TYPE/CATEGORY===\n{type_category}",
        title = fields.title,
        description = fields.description,
        attributes = attributes,
        type_category = fields.type_category,
    )
}

/// Ordered, deduped product image URLs (primary first), capped for cost.
fn module3_image_urls(product: &ozon_connector::OzonProductDetail) -> Vec<String> {
    let mut urls: Vec<String> = Vec::new();
    let mut push = |url: &str| {
        let url = url.trim();
        if url.is_empty() || urls.iter().any(|existing| existing == url) {
            return;
        }
        urls.push(url.to_string());
    };
    if let Some(primary) = product.primary_image.as_deref() {
        push(primary);
    }
    for image in &product.images {
        push(&image.url);
    }
    for url in &product.gallery_images {
        push(url);
    }
    urls.truncate(MODULE3_MAX_IMAGES);
    urls
}

/// Build the system + user (multimodal) messages and call the chat codec.
async fn module3_request_proposal(
    state: &LocalState,
    language: &str,
    document: &str,
    image_urls: &[String],
) -> Result<String, ApiError> {
    let system = format!(
        "Ты — помощник по карточкам товаров маркетплейса Ozon. На основе исходных полей товара и его фотографий перераспредели и очисти контент по четырём полям. Выводи текст на языке с кодом \"{language}\" (по умолчанию русский). НИЧЕГО НЕ ВЫДУМЫВАЙ: используй только факты из исходного текста и изображений. Верни СТРОГО валидный JSON ровно с такими ключами: {{\"title\": строка, \"description\": строка, \"attributes\": [{{\"name\": строка, \"values\": [строка, ...]}}], \"type_category\": строка}}. Без markdown, без пояснений, только JSON."
    );

    let mut parts = vec![Module3ContentPart::Text {
        text: format!(
            "Исходные поля товара (перераспредели их по четырём полям):\n\n{document}"
        ),
    }];
    for url in image_urls {
        parts.push(Module3ContentPart::ImageUrl {
            image_url: Module3ImageUrl { url: url.clone() },
        });
    }

    let messages = vec![
        ChatMessage {
            role: "system",
            content: ChatContent::Text(system),
        },
        ChatMessage {
            role: "user",
            content: ChatContent::Parts(parts),
        },
    ];

    chat_completion(state, messages).await
}

// --- OpenAI-compatible chat-completion codec (multimodal) ------------------ //

#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    max_tokens: u32,
}

#[derive(Debug, Serialize)]
struct ChatMessage {
    role: &'static str,
    content: ChatContent,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum ChatContent {
    Text(String),
    Parts(Vec<Module3ContentPart>),
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Module3ContentPart {
    Text { text: String },
    ImageUrl { image_url: Module3ImageUrl },
}

#[derive(Debug, Serialize)]
struct Module3ImageUrl {
    url: String,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    #[serde(default)]
    choices: Vec<ChatCompletionChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChoice {
    #[serde(default)]
    message: ChatCompletionMessage,
}

#[derive(Debug, Default, Deserialize)]
struct ChatCompletionMessage {
    #[serde(default)]
    content: Option<String>,
}

/// POST an OpenAI-compatible chat completion via the PR2 model-router seam and
/// return `choices[0].message.content`. Uses `apply_auth` (not `bearer_auth`) so
/// Header/Query-keyed providers also work.
async fn chat_completion(
    state: &LocalState,
    messages: Vec<ChatMessage>,
) -> Result<String, ApiError> {
    let (base_url, model, api_key, auth) = match resolve_capability(state, Capability::TextGen)
        .await?
    {
        ResolvedProvider::Generic {
            kind: ProviderKind::OpenAiCompatChat,
            base_url,
            model,
            api_key,
            auth,
            ..
        } => (base_url, model, api_key, auth),
        ResolvedProvider::Generic { .. } => {
            return Err(ApiError::bad_request(
                "configured text/vision provider is not an OpenAI-compatible chat provider",
            ));
        }
        ResolvedProvider::NotConfigured { reason, .. } => {
            return Err(ApiError::bad_request(format!(
                "text/vision provider is not configured: {reason}"
            )));
        }
        ResolvedProvider::OpenAiImage(_) => {
            return Err(ApiError::bad_request(
                "text/vision provider is not configured",
            ));
        }
    };

    // No response_format: many OpenAI-compatible vision endpoints reject
    // json_object mode (especially alongside image_url content). The recognizer
    // system prompt demands strict JSON and the parser tolerantly strips
    // fences/quotes before deserializing.
    let request = ChatCompletionRequest {
        model,
        messages,
        temperature: 0.2,
        max_tokens: 2048,
    };

    let endpoint = endpoint_for(ProviderKind::OpenAiCompatChat, &base_url);
    let builder = state.http_client.post(endpoint).json(&request);
    let response = apply_auth(builder, &auth, &api_key)
        .send()
        .await
        .map_err(|error| ApiError::bad_gateway(format!("chat request failed: {error}")))?;

    if !response.status().is_success() {
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "unknown error".to_string());
        return Err(ApiError::bad_gateway(format!(
            "chat completion failed: {}",
            summarize_openai_error(&body)
        )));
    }

    let payload: ChatCompletionResponse = response
        .json()
        .await
        .map_err(|error| ApiError::bad_gateway(format!("invalid chat response: {error}")))?;
    payload
        .choices
        .into_iter()
        .next()
        .and_then(|choice| choice.message.content)
        .map(|content| content.trim().to_string())
        .filter(|content| !content.is_empty())
        .ok_or_else(|| ApiError::bad_gateway("chat completion returned no content"))
}

// --------------------------------------------------------------------------- //
// Module 6 ("video") — cloud image-to-video (M6-PR1).                         //
//                                                                             //
// SECOND consumer of the PR2 model-router seam (module 3 chat was the first). //
// Image-to-video is ASYNC: create-job -> poll -> hosted video URL (seconds to //
// minutes). We model the lifecycle explicitly with a job store + a BOUNDED    //
// background poller (explicit deadline + max attempts; never leaks/loops      //
// forever). First/last frame are PUBLIC image URLs from module 2 (new images) //
// passed BY REFERENCE — we never download or re-host them here.               //
//                                                                             //
// v1 SCOPE: generation only. We do NOT push the video to Ozon; we return the  //
// hosted video URL for operator review (a gated push is a future follow-up).  //
//                                                                             //
// Provider JSON shape is UNKNOWN: the structs below are OpenAI-compatible      //
// DEFAULTS with tolerant (#[serde(default)]) parsing. Adapting to a chosen     //
// vendor = tweak these structs + the JSON field paths; the operator configures //
// the provider (kind=cloud_video) in the model registry.                      //
// --------------------------------------------------------------------------- //

/// Maximum clip length we will request, in seconds. Operator input is clamped to
/// this ceiling (default is `VIDEO_DEFAULT_DURATION_SECS`).
const VIDEO_MAX_DURATION_SECS: u32 = 15;
/// Default clip length when the caller omits `duration_seconds`.
const VIDEO_DEFAULT_DURATION_SECS: u32 = 8;
/// Poller cadence: how often the background task asks the provider for status.
const VIDEO_POLL_INTERVAL_SECS: u64 = 5;
/// Poller ceiling: the background task gives up after this many attempts and
/// marks the job `Failed("timed out")`. With a 5s cadence this is ~10 minutes.
const VIDEO_POLL_MAX_ATTEMPTS: u32 = 120;

/// Lifecycle state of a cloud image-to-video job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum VideoStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
}

impl VideoStatus {
    fn is_terminal(self) -> bool {
        matches!(self, VideoStatus::Succeeded | VideoStatus::Failed)
    }
}

/// One tracked cloud image-to-video job. Stored in `LocalState.video_jobs` keyed
/// by our own `id` (NOT the provider's job id).
#[derive(Debug, Clone, Serialize)]
struct VideoJob {
    id: Uuid,
    status: VideoStatus,
    /// The provider's own job id, learned from the create response.
    provider_job_id: Option<String>,
    /// The hosted video URL, populated once the job succeeds.
    video_url: Option<String>,
    error: Option<String>,
    created_at: String,
    updated_at: String,
    first_frame_url: String,
    last_frame_url: Option<String>,
    prompt: String,
    duration_seconds: u32,
}

/// In-memory job store. Sibling to the other `LocalState` caches; jobs are
/// ephemeral (process-lifetime) — review the hosted URL, then it can be dropped.
type VideoJobStore = Arc<RwLock<HashMap<Uuid, VideoJob>>>;

// --- OpenAI-compatible cloud-video codec (async create + poll) ------------- //

/// A frame reference (public image URL passed by reference, never re-hosted).
#[derive(Debug, Serialize)]
struct VideoFrameRef {
    url: String,
}

/// Create-job request body. OpenAI-compatible DEFAULT shape; adapt per vendor.
#[derive(Debug, Serialize)]
struct VideoCreateRequest {
    model: String,
    prompt: String,
    first_frame: VideoFrameRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_frame: Option<VideoFrameRef>,
    duration_seconds: u32,
}

/// Create-job response. Tolerant: a provider job id is read from a few common
/// default JSON paths (`id`, or `job_id`). Adapt the paths per vendor.
#[derive(Debug, Default, Deserialize)]
struct VideoCreateResponse {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    job_id: Option<String>,
}

impl VideoCreateResponse {
    fn provider_job_id(&self) -> Option<String> {
        self.id
            .clone()
            .or_else(|| self.job_id.clone())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    }
}

/// Status-poll response. Tolerant: `status` is mapped through `map_video_status`,
/// and the hosted URL is read from a few common default paths.
#[derive(Debug, Default, Deserialize)]
struct VideoStatusResponse {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    video_url: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    output_url: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

impl VideoStatusResponse {
    fn hosted_url(&self) -> Option<String> {
        self.video_url
            .clone()
            .or_else(|| self.output_url.clone())
            .or_else(|| self.url.clone())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    }
}

/// Map a vendor status string to our lifecycle enum. Tolerant of the common
/// vendor vocabularies; an unknown string is treated as still `Running` so the
/// poller keeps going until a terminal status or the deadline.
fn map_video_status(raw: &str) -> VideoStatus {
    match raw.trim().to_ascii_lowercase().as_str() {
        "queued" | "pending" | "created" | "accepted" => VideoStatus::Queued,
        "running" | "processing" | "in_progress" | "in-progress" | "started" => {
            VideoStatus::Running
        }
        "succeeded" | "success" | "completed" | "complete" | "done" | "finished" => {
            VideoStatus::Succeeded
        }
        "failed" | "failure" | "error" | "errored" | "canceled" | "cancelled" => {
            VideoStatus::Failed
        }
        _ => VideoStatus::Running,
    }
}

/// Clamp a caller-supplied duration to `[1, VIDEO_MAX_DURATION_SECS]`, defaulting
/// to `VIDEO_DEFAULT_DURATION_SECS` when absent or zero.
fn clamp_video_duration(requested: Option<u32>) -> u32 {
    match requested {
        Some(value) if value > 0 => value.min(VIDEO_MAX_DURATION_SECS),
        _ => VIDEO_DEFAULT_DURATION_SECS,
    }
}

/// A fully-resolved cloud-video provider: connection + the dispatch dialect and
/// its `extra` knobs. NO gating here (the handler gates first); mirrors
/// `chat_completion`'s NotConfigured / wrong-kind errors so the UI can surface a
/// clear "configure a video provider" message.
struct ResolvedVideoProvider {
    base_url: String,
    model: String,
    api_key: String,
    auth: model_router::AuthStyle,
    dialect: VideoDialect,
    extra: std::collections::BTreeMap<String, String>,
}

/// Resolve the `cloud_video` provider via the PR2 seam, threading the dialect +
/// extra so the codec can dispatch.
async fn resolve_cloud_video(state: &LocalState) -> Result<ResolvedVideoProvider, ApiError> {
    match resolve_capability(state, Capability::VideoGen).await? {
        ResolvedProvider::Generic {
            kind: ProviderKind::CloudVideo,
            base_url,
            model,
            api_key,
            auth,
            video_dialect,
            extra,
        } => Ok(ResolvedVideoProvider {
            base_url,
            model,
            api_key,
            auth,
            dialect: video_dialect,
            extra,
        }),
        ResolvedProvider::Generic { .. } => Err(ApiError::bad_request(
            "configured video provider is not a cloud_video provider",
        )),
        ResolvedProvider::NotConfigured { reason, .. } => Err(ApiError::bad_request(format!(
            "video provider is not configured: {reason}"
        ))),
        ResolvedProvider::OpenAiImage(_) => {
            Err(ApiError::bad_request("video provider is not configured"))
        }
    }
}

/// Execute one dialect [`video_dialect::HttpSpec`] and return the parsed JSON.
/// Centralizes method/header/body wiring + non-2xx handling for all non-default
/// dialects. The api key (incl. `ak:sk` for signed dialects) is placed by the
/// dialect's own headers and is never logged here.
async fn execute_video_spec(
    state: &LocalState,
    spec: video_dialect::HttpSpec,
    phase: &str,
) -> Result<serde_json::Value, ApiError> {
    let mut builder = match spec.method {
        video_dialect::HttpMethod::Get => state.http_client.get(&spec.url),
        video_dialect::HttpMethod::Post => state.http_client.post(&spec.url),
    };
    for (name, value) in &spec.headers {
        builder = builder.header(name, value);
    }
    if let Some(body) = &spec.body {
        builder = builder.json(body);
    }
    let response = builder
        .send()
        .await
        .map_err(|error| ApiError::bad_gateway(format!("video {phase} request failed: {error}")))?;

    if !response.status().is_success() {
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "unknown error".to_string());
        return Err(ApiError::bad_gateway(format!(
            "video {phase} failed: {}",
            summarize_openai_error(&body)
        )));
    }

    response
        .json::<serde_json::Value>()
        .await
        .map_err(|error| ApiError::bad_gateway(format!("invalid video {phase} response: {error}")))
}

/// Create an async generation job at the provider and return its job id.
///
/// Dispatches on the configured [`VideoDialect`]. `OpenAiCompat` is the DEFAULT
/// and runs the original module-6 code path VERBATIM (same `VideoCreateRequest`
/// body via `apply_auth` to `endpoint_for(CloudVideo, base)`); the other dialects
/// build their request through `video_dialect::build_create` + execute it.
async fn video_create_job(
    state: &LocalState,
    prompt: &str,
    first_frame_url: &str,
    last_frame_url: Option<&str>,
    duration_seconds: u32,
) -> Result<String, ApiError> {
    let provider = resolve_cloud_video(state).await?;

    if provider.dialect == VideoDialect::OpenAiCompat {
        // --- DEFAULT path: byte-for-byte the original module-6 behavior. ---
        let request = VideoCreateRequest {
            model: provider.model.clone(),
            prompt: prompt.to_string(),
            first_frame: VideoFrameRef {
                url: first_frame_url.to_string(),
            },
            last_frame: last_frame_url.map(|url| VideoFrameRef {
                url: url.to_string(),
            }),
            duration_seconds,
        };

        let endpoint = endpoint_for(ProviderKind::CloudVideo, &provider.base_url);
        let builder = state.http_client.post(endpoint).json(&request);
        let response = apply_auth(builder, &provider.auth, &provider.api_key)
            .send()
            .await
            .map_err(|error| {
                ApiError::bad_gateway(format!("video create request failed: {error}"))
            })?;

        if !response.status().is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            return Err(ApiError::bad_gateway(format!(
                "video create failed: {}",
                summarize_openai_error(&body)
            )));
        }

        let payload: VideoCreateResponse = response.json().await.map_err(|error| {
            ApiError::bad_gateway(format!("invalid video create response: {error}"))
        })?;

        return payload
            .provider_job_id()
            .ok_or_else(|| ApiError::bad_gateway("video create returned no job id"));
    }

    // --- Per-dialect path. ---
    let now = Utc::now().timestamp();
    let inputs = video_dialect::CreateInputs {
        base_url: &provider.base_url,
        model: &provider.model,
        api_key: &provider.api_key,
        prompt,
        first_frame_url,
        last_frame_url,
        duration_seconds,
        extra: &provider.extra,
    };
    let spec = video_dialect::build_create(provider.dialect, &inputs, now);
    let payload = execute_video_spec(state, spec, "create").await?;
    let outcome = video_dialect::parse_create(provider.dialect, &payload)
        .map_err(|message| ApiError::bad_gateway(format!("video create: {message}")))?;
    Ok(outcome.provider_job_id)
}

/// Poll one job's status at the provider, returning `(status, video_url?, error?)`.
///
/// Dispatches on the configured [`VideoDialect`]. `OpenAiCompat` (DEFAULT) runs
/// the original module-6 GET path VERBATIM. Other dialects build + execute their
/// poll request via `video_dialect`; MiniMax's two-step `file_id -> retrieve` is
/// handled by issuing the follow-up fetch the parser requests.
async fn video_poll_status(
    state: &LocalState,
    provider_job_id: &str,
) -> Result<(VideoStatus, Option<String>, Option<String>), ApiError> {
    let provider = resolve_cloud_video(state).await?;

    if provider.dialect == VideoDialect::OpenAiCompat {
        // --- DEFAULT path: byte-for-byte the original module-6 behavior. ---
        let endpoint = format!(
            "{}/{}",
            endpoint_for(ProviderKind::CloudVideo, &provider.base_url),
            provider_job_id
        );
        let builder = state.http_client.get(endpoint);
        let response = apply_auth(builder, &provider.auth, &provider.api_key)
            .send()
            .await
            .map_err(|error| {
                ApiError::bad_gateway(format!("video poll request failed: {error}"))
            })?;

        if !response.status().is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            return Err(ApiError::bad_gateway(format!(
                "video poll failed: {}",
                summarize_openai_error(&body)
            )));
        }

        let payload: VideoStatusResponse = response.json().await.map_err(|error| {
            ApiError::bad_gateway(format!("invalid video status response: {error}"))
        })?;

        let status = payload
            .status
            .as_deref()
            .map(map_video_status)
            // No status field but a URL came back -> treat as succeeded.
            .unwrap_or(if payload.hosted_url().is_some() {
                VideoStatus::Succeeded
            } else {
                VideoStatus::Running
            });

        return Ok((status, payload.hosted_url(), payload.error.clone()));
    }

    // --- Per-dialect path. ---
    let now = Utc::now().timestamp();
    let poll_inputs = video_dialect::PollInputs {
        base_url: &provider.base_url,
        api_key: &provider.api_key,
        provider_job_id,
        extra: &provider.extra,
    };
    let spec = video_dialect::build_poll(provider.dialect, &poll_inputs, now);
    let payload = execute_video_spec(state, spec, "poll").await?;

    match video_dialect::parse_poll(provider.dialect, &payload, &poll_inputs) {
        video_dialect::PollOutcome::Status {
            status,
            video_url,
            error,
        } => Ok((status, video_url, error)),
        video_dialect::PollOutcome::NeedsFetch { fetch } => {
            // MiniMax success: one more GET to resolve file_id -> download_url.
            let file_payload = execute_video_spec(state, fetch, "file retrieve").await?;
            let url = video_dialect::parse_minimax_file_retrieve(&file_payload);
            Ok((VideoStatus::Succeeded, url, None))
        }
    }
}

// --- Module 6 handlers ----------------------------------------------------- //

#[derive(Debug, Deserialize)]
struct VideoCreateRequestBody {
    first_frame_url: String,
    #[serde(default)]
    last_frame_url: Option<String>,
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    duration_seconds: Option<u32>,
}

/// POST /tools/ozon.video.create — create a cloud image-to-video job.
///
/// Gated (bridge/operator token + a valid lease with `OzonRead`; generation is
/// read-only). Caps duration, calls `video_create_job`, inserts a `Queued`
/// `VideoJob`, and SPAWNS a BOUNDED background poller that updates the job and
/// stops on a terminal status OR the deadline. Returns our job id immediately.
async fn video_create(
    State(state): State<LocalState>,
    headers: HeaderMap,
    Json(input): Json<VideoCreateRequestBody>,
) -> Result<Json<VideoJob>, ApiError> {
    require_bridge_or_operator_token(&state, &headers)?;
    require_valid_lease_with_feature(&state, Feature::OzonRead).await?;

    let first_frame_url = input.first_frame_url.trim().to_string();
    if first_frame_url.is_empty() {
        return Err(ApiError::bad_request("first_frame_url is required"));
    }
    let last_frame_url = input
        .last_frame_url
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let prompt = input
        .prompt
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_default();
    let duration_seconds = clamp_video_duration(input.duration_seconds);

    // Create the provider job FIRST so a misconfigured provider surfaces a clear
    // error synchronously (NotConfigured -> bad_request, like chat_completion).
    let provider_job_id = video_create_job(
        &state,
        &prompt,
        &first_frame_url,
        last_frame_url.as_deref(),
        duration_seconds,
    )
    .await?;

    let now = Utc::now().to_rfc3339();
    let job = VideoJob {
        id: Uuid::new_v4(),
        status: VideoStatus::Queued,
        provider_job_id: Some(provider_job_id.clone()),
        video_url: None,
        error: None,
        created_at: now.clone(),
        updated_at: now,
        first_frame_url,
        last_frame_url,
        prompt,
        duration_seconds,
    };
    let job_id = job.id;
    state.video_jobs.write().await.insert(job_id, job.clone());

    spawn_video_poller(state.clone(), job_id, provider_job_id);

    Ok(Json(job))
}

/// Spawn the BOUNDED background poller for one job. It polls every
/// `VIDEO_POLL_INTERVAL_SECS` up to `VIDEO_POLL_MAX_ATTEMPTS` attempts, updates
/// the stored job on each tick, and STOPS on a terminal status OR when the
/// attempt budget is exhausted (writing `Failed("timed out")`). It cannot loop
/// forever or leak: the attempt counter is the explicit deadline, and it also
/// exits early if the job disappears from the store. Mirrors the scheduler poll
/// loop (`tokio::spawn` + `tokio::time::interval`); `reqwest` is async so the
/// poller uses `tokio::time`, NOT `spawn_blocking`.
fn spawn_video_poller(state: LocalState, job_id: Uuid, provider_job_id: String) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(VIDEO_POLL_INTERVAL_SECS));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        // Burn the immediate first tick so we wait one interval before polling
        // (the create call just returned; the job is unlikely to be ready).
        ticker.tick().await;

        for _ in 0..VIDEO_POLL_MAX_ATTEMPTS {
            ticker.tick().await;

            // If the job is gone (process state reset), stop — never leak.
            if !state.video_jobs.read().await.contains_key(&job_id) {
                return;
            }

            match video_poll_status(&state, &provider_job_id).await {
                Ok((status, video_url, provider_error)) => {
                    let mut jobs = state.video_jobs.write().await;
                    let Some(job) = jobs.get_mut(&job_id) else {
                        return;
                    };
                    job.status = status;
                    job.updated_at = Utc::now().to_rfc3339();
                    if let Some(url) = video_url {
                        job.video_url = Some(url);
                    }
                    if status == VideoStatus::Failed {
                        job.error = Some(
                            provider_error
                                .unwrap_or_else(|| "provider reported failure".to_string()),
                        );
                    }
                    if status.is_terminal() {
                        return;
                    }
                }
                Err(error) => {
                    // A transient poll error does not kill the job; record it and
                    // keep trying until the attempt budget is exhausted.
                    let mut jobs = state.video_jobs.write().await;
                    if let Some(job) = jobs.get_mut(&job_id) {
                        job.error = Some(error.message.clone());
                        job.updated_at = Utc::now().to_rfc3339();
                    }
                }
            }
        }

        // Deadline reached without a terminal status -> mark timed out.
        let mut jobs = state.video_jobs.write().await;
        if let Some(job) = jobs.get_mut(&job_id) {
            if !job.status.is_terminal() {
                job.status = VideoStatus::Failed;
                job.error = Some("timed out waiting for the video provider".to_string());
                job.updated_at = Utc::now().to_rfc3339();
            }
        }
    });
}

/// GET /tools/ozon.video.get/{id} — read one video job (status + hosted URL when
/// succeeded).
async fn video_get(
    State(state): State<LocalState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<VideoJob>, ApiError> {
    require_bridge_or_operator_token(&state, &headers)?;
    require_valid_lease_with_feature(&state, Feature::OzonRead).await?;
    let job = state
        .video_jobs
        .read()
        .await
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("video job not found"))?;
    Ok(Json(job))
}

/// Strip one enclosing markdown code fence and/or one enclosing quote pair, then
/// parse the JSON into a `Module3Fields`. Mirrors `textgen.py:53-56` quote
/// stripping plus tolerant fenced-block handling.
fn parse_module3_result(content: &str) -> Result<Module3Fields, String> {
    let cleaned = strip_json_wrappers(content);
    serde_json::from_str::<Module3Fields>(&cleaned)
        .map_err(|error| format!("could not parse model JSON: {error}"))
}

// --------------------------------------------------------------------------- //
// Module 3 ("copy") — push half (M3-PR2).                                     //
//                                                                             //
// WRITE-BACK to the LIVE store. A push is gated: it ALWAYS runs a dry-run     //
// preview (before -> after for title/description + the matched/dropped        //
// attributes) and only writes when the caller passes an explicit confirm.     //
//                                                                             //
// SAFETY: attribute names + value strings from the (human-reviewed) proposal  //
// are re-matched against the Ozon category dictionary by NORMALIZED EXACT     //
// match only. Anything that does not match is DROPPED + reported — never      //
// written with a guessed numeric id.                                          //
// --------------------------------------------------------------------------- //

#[derive(Debug, Deserialize)]
struct Module3PushRequest {
    #[serde(default)]
    items: Vec<Module3PushTarget>,
    /// Must be `true` to actually write. Absent/false = dry-run preview only.
    #[serde(default)]
    confirm: bool,
}

#[derive(Debug, Deserialize, Clone)]
struct Module3PushTarget {
    /// Numeric Ozon product id (used to read the live product + its category).
    product_id: String,
    /// The reviewed/edited proposal to write back.
    proposal: Module3Fields,
}

#[derive(Debug, Serialize)]
struct Module3PushResponse {
    connector_mode: &'static str,
    /// Echoes whether this was an executed write (`true`) or a dry-run (`false`).
    confirmed: bool,
    items: Vec<Module3PushItem>,
}

#[derive(Debug, Serialize)]
struct Module3PushItem {
    product_id: String,
    offer_id: String,
    /// The dry-run preview is ALWAYS present, even on a confirmed write.
    preview: Option<Module3PushPreview>,
    /// Only set on a confirmed, executed write.
    written: bool,
    task_id: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct Module3FieldChange {
    before: String,
    after: String,
}

#[derive(Debug, Clone, Serialize)]
struct Module3PushPreview {
    title: Module3FieldChange,
    description: Module3FieldChange,
    attributes_to_write: Vec<Module3MatchedAttribute>,
    dropped: Vec<Module3DroppedItem>,
}

#[derive(Debug, Clone, Serialize)]
struct Module3MatchedAttribute {
    attribute_id: u64,
    name: String,
    /// Human-readable values that WILL be written (display strings).
    values: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct Module3DroppedItem {
    /// The proposal attribute name this drop is about.
    name: String,
    /// The specific value dropped (None when the whole attribute name is unmatched).
    value: Option<String>,
    reason: String,
}

/// Internal result of the pure re-match step: resolved (numeric-id) attributes
/// plus the human-readable preview rows and dropped report.
struct Module3MatchOutcome {
    resolved: Vec<OzonResolvedAttribute>,
    matched: Vec<Module3MatchedAttribute>,
    dropped: Vec<Module3DroppedItem>,
}

/// Normalize a name/value for conservative exact matching: trim, case-fold, and
/// collapse internal whitespace. No fuzzy/semantic transforms.
fn module3_normalize(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// Pure re-match: map proposal attributes [{name, values}] against the fetched
/// category dictionary. Dictionary-typed attributes require a value_id match
/// (unmatched values dropped); free-text attributes pass the string through.
/// Unmatched attribute NAMES are dropped wholesale. NOTHING is ever assigned a
/// guessed id.
fn module3_match_attributes(
    proposal: &[Module3Attribute],
    dictionary: &[OzonCategoryAttribute],
    values_by_attribute: &HashMap<u64, Vec<OzonCategoryAttributeValue>>,
) -> Module3MatchOutcome {
    let mut resolved: Vec<OzonResolvedAttribute> = Vec::new();
    let mut matched: Vec<Module3MatchedAttribute> = Vec::new();
    let mut dropped: Vec<Module3DroppedItem> = Vec::new();

    for attribute in proposal {
        let raw_name = attribute.name.trim();
        if raw_name.is_empty() {
            dropped.push(Module3DroppedItem {
                name: String::new(),
                value: None,
                reason: "empty attribute name".to_string(),
            });
            continue;
        }
        let normalized_name = module3_normalize(raw_name);
        let Some(definition) = dictionary
            .iter()
            .find(|candidate| module3_normalize(&candidate.name) == normalized_name)
        else {
            dropped.push(Module3DroppedItem {
                name: raw_name.to_string(),
                value: None,
                reason: "attribute name not in category dictionary".to_string(),
            });
            continue;
        };

        let mut resolved_values: Vec<OzonResolvedValue> = Vec::new();
        let mut matched_values: Vec<String> = Vec::new();

        for raw_value in &attribute.values {
            let value = raw_value.trim();
            if value.is_empty() {
                continue;
            }
            if definition.is_dictionary() {
                let normalized_value = module3_normalize(value);
                let candidate = values_by_attribute
                    .get(&definition.id)
                    .and_then(|values| {
                        values
                            .iter()
                            .find(|entry| module3_normalize(&entry.value) == normalized_value)
                    });
                match candidate {
                    Some(entry) => {
                        resolved_values.push(OzonResolvedValue::Dictionary {
                            dictionary_value_id: entry.value_id,
                        });
                        matched_values.push(entry.value.clone());
                    }
                    None => dropped.push(Module3DroppedItem {
                        name: raw_name.to_string(),
                        value: Some(value.to_string()),
                        reason: "value not in attribute dictionary".to_string(),
                    }),
                }
            } else {
                resolved_values.push(OzonResolvedValue::FreeText {
                    value: value.to_string(),
                });
                matched_values.push(value.to_string());
            }
        }

        if resolved_values.is_empty() {
            // Name matched but no value survived: report it (no empty write).
            if !definition.is_dictionary() {
                dropped.push(Module3DroppedItem {
                    name: raw_name.to_string(),
                    value: None,
                    reason: "no non-empty values to write".to_string(),
                });
            }
            continue;
        }

        resolved.push(OzonResolvedAttribute {
            attribute_id: definition.id,
            values: resolved_values,
        });
        matched.push(Module3MatchedAttribute {
            attribute_id: definition.id,
            name: definition.name.clone(),
            values: matched_values,
        });
    }

    Module3MatchOutcome {
        resolved,
        matched,
        dropped,
    }
}

async fn module3_push(
    State(state): State<LocalState>,
    headers: HeaderMap,
    Json(input): Json<Module3PushRequest>,
) -> Result<Json<Module3PushResponse>, ApiError> {
    require_bridge_or_operator_token(&state, &headers)?;
    // Mirrors the relist push gate (same feature): a valid lease with write-
    // capable Ozon access is required before any preview or write.
    require_valid_lease_with_feature(&state, Feature::OzonRead).await?;
    if input.items.is_empty() {
        return Err(ApiError::bad_request("nothing to push"));
    }
    if input.items.len() > MODULE3_MAX_BATCH {
        return Err(ApiError::bad_request(format!(
            "push at most {MODULE3_MAX_BATCH} products per batch"
        )));
    }
    let credentials = load_ozon_credentials(&state).await?;

    let mut items = Vec::with_capacity(input.items.len());
    for target in input.items {
        match module3_push_one(&state, &credentials, &target, input.confirm).await {
            Ok(item) => items.push(item),
            Err(error) => items.push(Module3PushItem {
                product_id: target.product_id.clone(),
                offer_id: String::new(),
                preview: None,
                written: false,
                task_id: None,
                error: Some(error),
            }),
        }
    }

    Ok(Json(Module3PushResponse {
        connector_mode: connector_mode(&state),
        confirmed: input.confirm,
        items,
    }))
}

async fn module3_push_one(
    state: &LocalState,
    credentials: &OzonCredentials,
    target: &Module3PushTarget,
    confirm: bool,
) -> Result<Module3PushItem, String> {
    // 1) Read the LIVE product to get the before-values + the category/type that
    //    scopes the attribute dictionary. Always re-read at push time so the
    //    "before" and the dictionary reflect the current live listing.
    let lookup = OzonProductLookup {
        product_id: Some(target.product_id.clone()),
        offer_id: None,
        sku: None,
    }
    .normalized();
    let product = state
        .ozon_connector
        .product_get(credentials, lookup)
        .await
        .map_err(|error| format!("read product failed: {error}"))?;

    let description_category_id = product
        .description_category_id
        .ok_or_else(|| "product has no description_category_id; cannot resolve attributes".to_string())?;
    let type_id = product
        .type_id
        .ok_or_else(|| "product has no type_id; cannot resolve attributes".to_string())?;

    // 2) Fetch the category attribute dictionary, then the value dictionaries for
    //    each dictionary-typed attribute the proposal actually references.
    let dictionary = state
        .ozon_writer
        .description_category_attributes(credentials, description_category_id, type_id)
        .await
        .map_err(|error| format!("read category attributes failed: {error}"))?;

    let referenced_ids = module3_referenced_dictionary_ids(&target.proposal.attributes, &dictionary);
    let mut values_by_attribute: HashMap<u64, Vec<OzonCategoryAttributeValue>> = HashMap::new();
    for attribute_id in referenced_ids {
        let values = state
            .ozon_writer
            .description_category_attribute_values(
                credentials,
                description_category_id,
                type_id,
                attribute_id,
            )
            .await
            .map_err(|error| format!("read attribute values failed: {error}"))?;
        values_by_attribute.insert(attribute_id, values);
    }

    // 3) Pure re-match (no I/O). Drops + reports anything unmatched.
    let outcome =
        module3_match_attributes(&target.proposal.attributes, &dictionary, &values_by_attribute);

    let before_title = product.name.clone().unwrap_or_default();
    let before_description = product.description.clone().unwrap_or_default();
    let after_title = word_boundary_truncate(target.proposal.title.trim(), MODULE3_TITLE_MAX);
    let after_description =
        word_boundary_truncate(target.proposal.description.trim(), MODULE3_DESCRIPTION_MAX);

    let preview = Module3PushPreview {
        title: Module3FieldChange {
            before: before_title,
            after: after_title.clone(),
        },
        description: Module3FieldChange {
            before: before_description,
            after: after_description.clone(),
        },
        attributes_to_write: outcome.matched.clone(),
        dropped: outcome.dropped.clone(),
    };

    // 4) DRY-RUN: without an explicit confirm, return the preview and write
    //    NOTHING. This is the only path AI/bridge callers can reach by default.
    if !confirm {
        return Ok(Module3PushItem {
            product_id: product.product_id,
            offer_id: product.offer_id,
            preview: Some(preview),
            written: false,
            task_id: None,
            error: None,
        });
    }

    // 5) Confirmed write: send only the matched (resolved-id) attributes plus the
    //    title/description. `name`/`description` are only sent when non-empty.
    let update = OzonProductCopyUpdate {
        offer_id: product.offer_id.clone(),
        name: Some(after_title).filter(|value| !value.is_empty()),
        description: Some(after_description).filter(|value| !value.is_empty()),
        attributes: outcome.resolved,
    };
    let result = state
        .ozon_writer
        .product_update_copy(credentials, update)
        .await
        .map_err(|error| format!("product update failed: {error}"))?;

    Ok(Module3PushItem {
        product_id: product.product_id,
        offer_id: product.offer_id,
        preview: Some(preview),
        written: result.accepted,
        task_id: result.task_id,
        error: None,
    })
}

/// Collect the numeric attribute ids of the dictionary-typed attributes that the
/// proposal references by name (so we only fetch the value dictionaries we need).
fn module3_referenced_dictionary_ids(
    proposal: &[Module3Attribute],
    dictionary: &[OzonCategoryAttribute],
) -> Vec<u64> {
    let mut ids: Vec<u64> = Vec::new();
    for attribute in proposal {
        let normalized_name = module3_normalize(attribute.name.trim());
        if normalized_name.is_empty() {
            continue;
        }
        if let Some(definition) = dictionary
            .iter()
            .find(|candidate| module3_normalize(&candidate.name) == normalized_name)
        {
            if definition.is_dictionary() && !ids.contains(&definition.id) {
                ids.push(definition.id);
            }
        }
    }
    ids
}

fn strip_json_wrappers(content: &str) -> String {
    let mut out = content.trim().to_string();
    // Strip one enclosing markdown code fence (```json ... ``` or ``` ... ```).
    if out.starts_with("```") {
        if let Some(rest) = out.strip_prefix("```") {
            let rest = rest.strip_prefix("json").unwrap_or(rest);
            let rest = rest.strip_prefix("JSON").unwrap_or(rest);
            let rest = rest.trim_start_matches('\n');
            if let Some(inner) = rest.strip_suffix("```") {
                out = inner.trim().to_string();
            } else {
                out = rest.trim().to_string();
            }
        }
    }
    // Strip a single enclosing quote pair (mirrors textgen.py:53-56).
    let chars: Vec<char> = out.chars().collect();
    if chars.len() >= 2 {
        let first = chars[0];
        let last = chars[chars.len() - 1];
        if matches!(first, '"' | '\'' | '«') && matches!(last, '"' | '\'' | '»') {
            out = chars[1..chars.len() - 1].iter().collect::<String>();
            out = out.trim().to_string();
        }
    }
    out
}

/// Hard cap on character count, cut on a word boundary (mirrors relist.py:125-152).
fn word_boundary_truncate(value: &str, max_chars: usize) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let cut: String = trimmed.chars().take(max_chars).collect();
    let cut = match cut.rfind(' ') {
        Some(index) => &cut[..index],
        None => cut.as_str(),
    };
    cut.trim_end().to_string()
}

/// Build the per-product Russian-Ozon prompt: the product name (+ a few real
/// attributes) up front so the model's headline & selling points are accurate,
/// followed by the fixed rules.
fn compose_relist_prompt(product: &ozon_connector::OzonProductDetail) -> String {
    let name = product
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("the product shown in the photo");
    let mut attrs: Vec<String> = Vec::new();
    for attr in &product.attributes {
        if attrs.len() >= 6 {
            break;
        }
        if let (Some(attr_name), Some(value)) = (attr.name.as_deref(), attr.values.first()) {
            let attr_name = attr_name.trim();
            let value = value.trim();
            if !attr_name.is_empty() && !value.is_empty() {
                attrs.push(format!("{attr_name}: {value}"));
            }
        }
    }
    let facts = if attrs.is_empty() {
        String::new()
    } else {
        format!(" Key facts: {}.", attrs.join("; "))
    };
    format!(
        "You are creating a professional Russian-language e-commerce product image for the Ozon marketplace.\nSOURCE PRODUCT (this is what the photo shows, keep it exactly): \"{name}\".{facts}{RELIST_OZON_RULES}"
    )
}

async fn relist_generate_one(
    state: &LocalState,
    credentials: &OzonCredentials,
    lookup: OzonProductLookup,
    prompt_override: Option<&str>,
) -> Result<RelistItem, String> {
    let product = state
        .ozon_connector
        .product_get(credentials, lookup)
        .await
        .map_err(|error| format!("read product failed: {error}"))?;
    let original = product
        .primary_image
        .clone()
        .or_else(|| product.images.first().map(|image| image.url.clone()))
        .ok_or_else(|| "product has no image to restyle".to_string())?;

    let prompt = match prompt_override {
        Some(value) => value.to_string(),
        None => compose_relist_prompt(&product),
    };
    // The source photo is downloaded once and reused for every candidate. Each
    // candidate re-runs the image edit (the model varies) + a fresh host upload.
    let source = relist_download(state, &original)
        .await
        .map_err(|error| error.message)?;

    let mut candidates = Vec::with_capacity(RELIST_MAX_CANDIDATES);
    let mut last_error: Option<String> = None;
    for _ in 0..RELIST_MAX_CANDIDATES {
        let edited = match relist_edit_image(state, source.clone(), &prompt).await {
            Ok(bytes) => bytes,
            Err(error) => {
                last_error = Some(error.message);
                continue;
            }
        };
        let filename = format!("relist-{}-{}.png", product.product_id, Uuid::new_v4().simple());
        match relist_host_image(state, &filename, edited).await {
            Ok(url) => candidates.push(url),
            Err(error) => last_error = Some(error.message),
        }
    }

    // Partial failure returns the successful subset; only a total failure (no
    // candidate hosted) is reported as an error for this item.
    if candidates.is_empty() {
        return Err(last_error.unwrap_or_else(|| "failed to generate any candidate".to_string()));
    }

    Ok(RelistItem {
        product_id: product.product_id,
        offer_id: product.offer_id,
        name: product.name,
        original_url: Some(original),
        candidates,
        error: None,
    })
}

async fn relist_push(
    State(state): State<LocalState>,
    headers: HeaderMap,
    Json(input): Json<RelistPushRequest>,
) -> Result<Json<RelistPushResponse>, ApiError> {
    require_bridge_or_operator_token(&state, &headers)?;
    require_valid_lease_with_feature(&state, Feature::OzonRead).await?;
    if input.items.is_empty() {
        return Err(ApiError::bad_request("nothing to push"));
    }
    if input.items.len() > RELIST_MAX_BATCH {
        return Err(ApiError::bad_request(format!(
            "push at most {RELIST_MAX_BATCH} products per batch"
        )));
    }
    let credentials = load_ozon_credentials(&state).await?;

    let mut results = Vec::with_capacity(input.items.len());
    for target in input.items {
        match relist_push_one(&state, &credentials, &target).await {
            Ok(result) => results.push(result),
            Err(error) => results.push(RelistPushResult {
                product_id: target.product_id.clone(),
                primary_url: target.new_primary_url.clone(),
                image_count: 0,
                ok: false,
                error: Some(error),
            }),
        }
    }

    Ok(Json(RelistPushResponse {
        connector_mode: connector_mode(&state),
        items: results,
    }))
}

async fn relist_push_one(
    state: &LocalState,
    credentials: &OzonCredentials,
    target: &RelistPushTarget,
) -> Result<RelistPushResult, String> {
    let new_primary = target.new_primary_url.trim().to_string();
    if !new_primary.starts_with("http") {
        return Err("new primary URL is not a valid http(s) URL".to_string());
    }

    // Recompute the live image list at push time so we never set a stale or
    // wrong gallery: AI primary first, then the product's current images as
    // gallery (skipping any duplicate of the new primary).
    let lookup = OzonProductLookup {
        product_id: Some(target.product_id.clone()),
        offer_id: None,
        sku: None,
    }
    .normalized();
    let product = state
        .ozon_connector
        .product_get(credentials, lookup)
        .await
        .map_err(|error| format!("read product failed: {error}"))?;

    let mut images = vec![new_primary.clone()];
    for image in &product.images {
        if image.url != new_primary && !images.contains(&image.url) {
            images.push(image.url.clone());
        }
    }

    let result = state
        .ozon_writer
        .pictures_import(credentials, &product.product_id, images)
        .await
        .map_err(|error| format!("pictures import failed: {error}"))?;

    Ok(RelistPushResult {
        product_id: product.product_id,
        primary_url: new_primary,
        image_count: result.pictures.len() as u32,
        ok: true,
        error: None,
    })
}

/// Download an image URL into memory (used for the restyle source and the
/// optional image-edit URL fallback).
async fn relist_download(state: &LocalState, url: &str) -> Result<Vec<u8>, ApiError> {
    let response = state
        .http_client
        .get(url)
        .send()
        .await
        .map_err(|error| ApiError::bad_gateway(format!("failed to download image: {error}")))?;
    if !response.status().is_success() {
        return Err(ApiError::bad_gateway(format!(
            "image download returned HTTP {}",
            response.status().as_u16()
        )));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|error| ApiError::bad_gateway(format!("failed to read image bytes: {error}")))?;
    Ok(bytes.to_vec())
}

/// Run one image through the GPT image-edit endpoint and return PNG bytes.
async fn relist_edit_image(
    state: &LocalState,
    source_bytes: Vec<u8>,
    prompt: &str,
) -> Result<Vec<u8>, ApiError> {
    let openai = resolve_capability(state, Capability::ImageGen)
        .await?
        .expect_openai_image()?;
    let part = reqwest::multipart::Part::bytes(source_bytes)
        .file_name("source.png")
        .mime_str("image/png")
        .map_err(|error| ApiError::internal(format!("failed to build image part: {error}")))?;
    let form = reqwest::multipart::Form::new()
        .text("model", openai.image_model.clone())
        .text("prompt", prompt.to_string())
        .text("size", RELIST_IMAGE_SIZE.to_string())
        .text("n", "1")
        .part("image", part);

    let response = state
        .http_client
        .post(openai_images_edit_endpoint(&openai.base_url))
        .bearer_auth(openai.api_key)
        .multipart(form)
        .send()
        .await
        .map_err(|error| ApiError::bad_gateway(format!("image edit request failed: {error}")))?;
    if !response.status().is_success() {
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "unknown error".to_string());
        return Err(ApiError::bad_gateway(format!(
            "image edit failed: {}",
            summarize_openai_error(&body)
        )));
    }
    let payload: OpenAiImageGenerationResponse = response
        .json()
        .await
        .map_err(|error| ApiError::bad_gateway(format!("invalid image edit response: {error}")))?;
    let image = payload
        .data
        .into_iter()
        .next()
        .ok_or_else(|| ApiError::bad_gateway("image edit response returned no images"))?;
    if let Some(b64) = image.b64_json {
        return BASE64_STANDARD.decode(b64.as_bytes()).map_err(|_| {
            ApiError::bad_gateway("image edit response returned invalid base64 data")
        });
    }
    if let Some(url) = image.url {
        return relist_download(state, &url).await;
    }
    Err(ApiError::bad_gateway(
        "image edit response did not include image data",
    ))
}

/// Public image hosts, tried in order. Ozon fetches the URL on pictures/import
/// and re-hosts the image on its own CDN, so a temporary host is enough. We try
/// several because any single one can be down or blocked by a local proxy
/// (observed: litterbox/catbox time out behind some proxies, uguu works).
struct ImageHost {
    name: &'static str,
    url: &'static str,
    fields: &'static [(&'static str, &'static str)],
    file_field: &'static str,
    json_url: bool,
}

const IMAGE_HOSTS: &[ImageHost] = &[
    ImageHost {
        name: "litterbox",
        url: "https://litterbox.catbox.moe/resources/internals/api.php",
        fields: &[("reqtype", "fileupload"), ("time", "72h")],
        file_field: "fileToUpload",
        json_url: false,
    },
    ImageHost {
        name: "catbox",
        url: "https://catbox.moe/user/api.php",
        fields: &[("reqtype", "fileupload")],
        file_field: "fileToUpload",
        json_url: false,
    },
    ImageHost {
        name: "uguu",
        url: "https://uguu.se/upload.php",
        fields: &[],
        file_field: "files[]",
        json_url: true,
    },
];

/// Upload PNG bytes to the first reachable public host and return its URL.
async fn relist_host_image(
    state: &LocalState,
    filename: &str,
    bytes: Vec<u8>,
) -> Result<String, ApiError> {
    let mut last_error = "no image host was reachable".to_string();
    for host in IMAGE_HOSTS {
        match relist_upload_to_host(state, host, filename, &bytes).await {
            Ok(url) => return Ok(url),
            Err(error) => last_error = format!("{}: {}", host.name, error.message),
        }
    }
    Err(ApiError::bad_gateway(format!(
        "all image hosts failed (last {last_error})"
    )))
}

async fn relist_upload_to_host(
    state: &LocalState,
    host: &ImageHost,
    filename: &str,
    bytes: &[u8],
) -> Result<String, ApiError> {
    let part = reqwest::multipart::Part::bytes(bytes.to_vec())
        .file_name(filename.to_string())
        .mime_str("image/png")
        .map_err(|error| ApiError::internal(format!("failed to build upload part: {error}")))?;
    let mut form = reqwest::multipart::Form::new();
    for (key, value) in host.fields {
        form = form.text(*key, *value);
    }
    form = form.part(host.file_field, part);

    let response = state
        .http_client
        .post(host.url)
        .multipart(form)
        .send()
        .await
        .map_err(|error| ApiError::bad_gateway(format!("upload failed: {error}")))?;
    if !response.status().is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(ApiError::bad_gateway(format!(
            "HTTP error: {}",
            truncate_text(body.trim(), 160)
        )));
    }
    let body = response
        .text()
        .await
        .map_err(|error| ApiError::bad_gateway(format!("response unreadable: {error}")))?;
    let url = if host.json_url {
        serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|value| {
                value
                    .get("files")
                    .and_then(|files| files.get(0))
                    .and_then(|first| first.get("url"))
                    .and_then(|url| url.as_str())
                    .map(str::to_string)
            })
            .unwrap_or_default()
    } else {
        body.trim().to_string()
    };
    if !url.starts_with("http") {
        return Err(ApiError::bad_gateway(format!(
            "no URL in response: {}",
            truncate_text(body.trim(), 160)
        )));
    }
    Ok(url)
}

fn summarize_openai_error(body: &str) -> String {
    if let Ok(envelope) = serde_json::from_str::<OpenAiErrorEnvelope>(body) {
        let message = envelope
            .error
            .message
            .unwrap_or_else(|| "upstream returned an error without a message".to_string());
        if envelope.error.code.as_deref() == Some("model_not_found") {
            return format!(
                "image model is not available for this API key: {message}. Use an API key or proxy with gpt-image-1 / gpt-image-2 enabled."
            );
        }
        if let Some(code) = envelope.error.code {
            return format!("{code}: {message}");
        }
        if let Some(error_type) = envelope.error.error_type {
            return format!("{error_type}: {message}");
        }
        return message;
    }
    truncate_text(body.trim(), 600)
}

fn preferred_headline(product: &ozon_connector::OzonProductDetail) -> String {
    let raw = product
        .name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&product.offer_id);
    truncate_text(raw.trim(), 28)
}

fn truncate_text(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let truncated = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

fn normalize_theme(theme: &str) -> &str {
    let normalized = theme.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "spotlight" => "spotlight studio",
        "launch" => "product launch stage",
        "lifestyle" => "editorial lifestyle display",
        _ => "clean studio",
    }
}

fn compare_poster_copy(expected: &PosterCopy, actual: &PosterCopy) -> Vec<PosterCopyMismatch> {
    let mut mismatches = Vec::new();
    push_copy_mismatch(
        &mut mismatches,
        "headline",
        &expected.headline,
        &actual.headline,
    );
    push_copy_mismatch(
        &mut mismatches,
        "subheadline",
        &expected.subheadline,
        &actual.subheadline,
    );
    push_copy_mismatch(
        &mut mismatches,
        "cta_line",
        &expected.cta_line,
        &actual.cta_line,
    );
    push_copy_mismatch(
        &mut mismatches,
        "compliance_note",
        &expected.compliance_note,
        &actual.compliance_note,
    );
    let expected_points = expected.selling_points.join(" | ");
    let actual_points = actual.selling_points.join(" | ");
    push_copy_mismatch(
        &mut mismatches,
        "selling_points",
        &expected_points,
        &actual_points,
    );
    mismatches
}

fn push_copy_mismatch(
    mismatches: &mut Vec<PosterCopyMismatch>,
    field: &'static str,
    expected: &str,
    actual: &str,
) {
    if normalize_copy(expected) != normalize_copy(actual) {
        mismatches.push(PosterCopyMismatch {
            field,
            expected: expected.to_string(),
            actual: actual.to_string(),
        });
    }
}

fn normalize_copy(value: &str) -> String {
    value.split_whitespace().collect::<String>().to_lowercase()
}

fn debug_mock_ozon_credentials() -> OzonCredentials {
    OzonCredentials {
        client_id: "debug-local-client-id".to_string(),
        api_key: SecretString::from("debug-local-api-key"),
    }
}

fn connector_mode(state: &LocalState) -> &'static str {
    if state.config.use_real_ozon {
        "real"
    } else {
        "mock"
    }
}

fn map_product_get_error(context: &str, error: ozon_connector::OzonConnectorError) -> ApiError {
    match error {
        ozon_connector::OzonConnectorError::InvalidProductLookup(message) => {
            ApiError::bad_request(format!("{context}: {message}"))
        }
        ozon_connector::OzonConnectorError::ProductNotFound(label) => {
            ApiError::not_found(format!("{context}: product not found for {label}"))
        }
        error => ApiError::bad_gateway(format!("{context}: {error}")),
    }
}

fn normalize_product_list_visibility(
    visibility: Option<String>,
) -> Result<Option<String>, ApiError> {
    let Some(visibility) = visibility else {
        return Ok(None);
    };
    let visibility = visibility.trim().to_ascii_uppercase();
    if visibility.is_empty() {
        return Ok(None);
    }
    if !visibility
        .chars()
        .all(|value| value.is_ascii_alphanumeric() || value == '_')
    {
        return Err(ApiError::bad_request(format!(
            "invalid product list visibility: {visibility}"
        )));
    }
    Ok(Some(visibility))
}

fn local_http_url(bind: &str) -> String {
    format!("http://{bind}")
}

fn build_openclaw_bind_url(
    base_url: &str,
    pairing_code: &str,
    claim_url: &str,
    manifest_url: &str,
) -> Result<String, ApiError> {
    let mut url = reqwest::Url::parse(base_url)
        .map_err(|_| ApiError::bad_request("OpenClaw bind URL must be a valid URL"))?;
    let fragment = format!(
        "ozon66_pairing_code={}&claim_url={}&manifest_url={}",
        percent_encode_url_component(pairing_code),
        percent_encode_url_component(claim_url),
        percent_encode_url_component(manifest_url)
    );
    url.set_fragment(Some(&fragment));
    Ok(url.to_string())
}

fn percent_encode_url_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => {
                encoded.push('%');
                encoded.push(nibble_to_hex(byte >> 4));
                encoded.push(nibble_to_hex(byte & 0x0f));
            }
        }
    }
    encoded
}

fn nibble_to_hex(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        10..=15 => (b'A' + (value - 10)) as char,
        _ => '0',
    }
}

fn validate_openclaw_bind_url(value: &str) -> anyhow::Result<()> {
    let url = reqwest::Url::parse(value)
        .map_err(|_| anyhow::anyhow!("OZON_OPENCLAW_BIND_URL must be a valid URL"))?;
    if url.path() != "/openclaw/import" {
        anyhow::bail!("OZON_OPENCLAW_BIND_URL must point to /openclaw/import")
    }
    let host = url.host_str().unwrap_or_default();
    let allowed = matches!(
        (url.scheme(), host, url.port_or_known_default()),
        (
            "https",
            "ozonclaw.jl696.cn" | "www.ozonclaw.jl696.cn",
            Some(443)
        ) | ("http", "127.0.0.1" | "localhost", Some(18789))
    );
    if allowed {
        return Ok(());
    }
    anyhow::bail!(
        "OZON_OPENCLAW_BIND_URL must be https://ozonclaw.jl696.cn/openclaw/import or http://127.0.0.1:18789/openclaw/import"
    )
}

fn configured_openclaw_allowed_origins() -> Vec<String> {
    env::var("OZON_OPENCLAW_ALLOWED_ORIGINS")
        .ok()
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn local_port(bind: &str) -> u16 {
    bind.rsplit_once(':')
        .and_then(|(_, port)| port.parse().ok())
        .unwrap_or(0)
}

fn env_u64(name: &str, fallback: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(fallback)
}

fn optional_env(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
}

fn read_optional_file_env(name: &str) -> Option<String> {
    let path = optional_env(name)?;
    fs::read_to_string(path)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn env_u16(name: &str, fallback: u16) -> u16 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(fallback)
}

fn sample_dry_run(operation: OperationKind) -> DryRunDiff {
    match operation {
        OperationKind::Import1688Mock => DryRunDiff {
            summary: "Import 3 authorized mock 1688 source items into local draft catalog"
                .to_string(),
            target_count: 3,
            changes: vec![FieldChange {
                object_id: "draft-1688-001".to_string(),
                field: "source".to_string(),
                before: None,
                after: Some("mock-import".to_string()),
            }],
            warnings: vec!["Live 1688 collection is disabled in MVP".to_string()],
        },
        OperationKind::DraftUploadMock => DryRunDiff {
            summary: "Prepare 2 local product drafts for Ozon upload preview".to_string(),
            target_count: 2,
            changes: vec![],
            warnings: vec!["No real Ozon write will be sent by mock executor".to_string()],
        },
        _ => DryRunDiff {
            summary: "Mock Ozon write proposal; approval required before execution".to_string(),
            target_count: 1,
            changes: vec![FieldChange {
                object_id: "SKU-MOCK-1".to_string(),
                field: "price".to_string(),
                before: Some("1299 RUB".to_string()),
                after: Some("1199 RUB".to_string()),
            }],
            warnings: vec![
                "AI/OpenClaw can propose this task, but local approval is mandatory".to_string(),
            ],
        },
    }
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    service: &'static str,
    status: &'static str,
    skill_port: u16,
    agent_port: u16,
    protocol_version: &'static str,
    build_commit: &'static str,
    package_version: &'static str,
    supervisor: &'static str,
    features: Vec<Feature>,
    real_ozon_enabled: bool,
}

#[derive(Debug, Serialize)]
struct PortalStatusResponse {
    service: &'static str,
    status: &'static str,
    checked_at: String,
    skill_api: String,
    agent_api: String,
    manifest_url: String,
    bridge_auth_header: &'static str,
    protocol_version: &'static str,
    build_commit: &'static str,
    package_version: &'static str,
    real_ozon_enabled: bool,
    device_fingerprint: String,
    ozon: PortalCredentialStatus,
    openai: PortalOpenAiStatus,
    poster_generation: PosterGenerationStatus,
    lease: LeaseStatus,
    features: Vec<Feature>,
}

#[derive(Debug, Serialize)]
struct PortalCredentialStatus {
    configured: bool,
    issue: Option<String>,
}

#[derive(Debug, Serialize)]
struct PortalOpenAiStatus {
    configured: bool,
    image_model: String,
    issue: Option<String>,
}

#[derive(Debug, Serialize)]
struct DiagnosticsResponse {
    service: &'static str,
    status: &'static str,
    checked_at: String,
    protocol_version: &'static str,
    build_commit: &'static str,
    package_version: &'static str,
    skill_api: String,
    agent_api: String,
    connector_mode: &'static str,
    real_ozon_enabled: bool,
    secret_store: SecretStoreStatus,
    ozon: OzonCredentialStatus,
    poster_generation: PosterGenerationStatus,
    openai: OpenAiCredentialStatus,
    lease: LeaseStatus,
    capabilities: Vec<CapabilityStatus>,
}

#[derive(Debug, Serialize)]
struct OpenClawManifest {
    name: &'static str,
    version: &'static str,
    description: &'static str,
    base_url: String,
    auth: OpenClawAuth,
    tools: Vec<OpenClawTool>,
    safety_rules: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
struct OpenClawAuth {
    header: &'static str,
    source: &'static str,
}

#[derive(Debug, Serialize)]
struct OpenClawTool {
    name: &'static str,
    method: &'static str,
    path: &'static str,
    risk: &'static str,
    approval_required: bool,
    description: &'static str,
}

#[derive(Debug, Serialize)]
struct OpenClawPairingStartResponse {
    status: &'static str,
    bind_url: String,
    pairing_code: String,
    claim_url: String,
    manifest_url: String,
    auth_header: &'static str,
    expires_at: String,
    instructions: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct OpenClawPairingClaimRequest {
    code: String,
}

#[derive(Debug, Serialize)]
struct OpenClawPairingClaimResponse {
    status: &'static str,
    manifest_url: String,
    base_url: String,
    auth_header: &'static str,
    auth_token: String,
    auth_token_fingerprint: String,
    expires_at: String,
    safety_rules: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct OzonConfigRequest {
    client_id: String,
    api_key: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiConfigRequest {
    api_key: String,
    base_url: Option<String>,
    image_model: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct StoredOzonConfig {
    client_id: String,
    api_key: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct StoredOpenAiConfig {
    api_key: String,
    base_url: String,
    image_model: String,
}

impl StoredOpenAiConfig {
    /// Build an image config from a registry entry. The image call sites consume
    /// this unchanged: they pick generations-vs-edits endpoint from `base_url`
    /// themselves, send `image_model` as the model, and use `api_key` as a bearer
    /// token — identical to a config produced by `load_openai_config`.
    pub(crate) fn for_registry(api_key: String, base_url: String, image_model: String) -> Self {
        Self {
            api_key,
            base_url,
            image_model,
        }
    }
}

#[derive(Debug, Serialize)]
struct OzonConfigResponse {
    client_id: String,
    api_key: String,
    saved_at: String,
}

#[derive(Debug, Serialize)]
struct OpenAiConfigResponse {
    base_url: String,
    image_model: String,
    api_key_fingerprint: String,
    saved_at: String,
}

#[derive(Debug, Serialize)]
struct ModelRegistryResponse {
    ok: bool,
    image_gen_entries: usize,
    text_gen_entries: usize,
    video_gen_entries: usize,
    saved_at: String,
}

#[derive(Debug, Deserialize)]
struct SecretSaveRequest {
    name: String,
    value: String,
}

#[derive(Debug, Serialize)]
struct SecretSaveResponse {
    name: String,
    /// Stable, non-reversible fingerprint of the saved value. NEVER the key.
    fingerprint: String,
    saved_at: String,
}

#[derive(Debug, Deserialize)]
struct PortalLeaseRequest {
    lease: EntitlementLease,
}

#[derive(Debug, Serialize)]
struct PortalLeaseResponse {
    accepted: bool,
    lease: LeaseStatus,
    saved_at: String,
}

#[derive(Debug)]
struct InspectedOzonCredentials {
    configured: bool,
    source: &'static str,
    client_id: Option<String>,
    api_key_fingerprint: Option<String>,
    secret_store_available: bool,
    issue: Option<String>,
}

#[derive(Debug, Serialize)]
struct ConfigStatusResponse {
    service: &'static str,
    checked_at: String,
    real_ozon_enabled: bool,
    connector_mode: &'static str,
    secret_store: SecretStoreStatus,
    ozon: OzonCredentialStatus,
    poster_generation: PosterGenerationStatus,
    openai: OpenAiCredentialStatus,
    lease: LeaseStatus,
    endpoints: LocalEndpointStatus,
    capabilities: Vec<CapabilityStatus>,
}

#[derive(Debug, Serialize)]
struct PosterGenerationStatus {
    preferred: &'static str,
    openclaw_bridge_ready: bool,
    handoff_path: &'static str,
    manifest_url: String,
    api_fallback_configured: bool,
    api_fallback_model: Option<String>,
    api_fallback_issue: Option<String>,
    message: &'static str,
}

#[derive(Debug, Serialize)]
struct SecretStoreStatus {
    backend: &'static str,
    available: bool,
}

#[derive(Debug, Serialize)]
struct OzonCredentialStatus {
    configured: bool,
    source: &'static str,
    client_id: Option<String>,
    api_key_fingerprint: Option<String>,
    issue: Option<String>,
}

#[derive(Debug, Serialize)]
struct OpenAiCredentialStatus {
    configured: bool,
    source: &'static str,
    base_url: String,
    image_model: String,
    api_key_fingerprint: Option<String>,
    issue: Option<String>,
}

#[derive(Debug, Serialize)]
struct LeaseStatus {
    configured: bool,
    valid: bool,
    lease_id: Option<String>,
    device_id: Option<String>,
    features: Vec<Feature>,
    expires_at: Option<String>,
    issue: Option<String>,
}

#[derive(Debug, Serialize)]
struct LocalEndpointStatus {
    skill_api: String,
    agent_api: String,
    manifest_url: String,
}

#[derive(Debug, Serialize)]
struct OzonCredentialValidationResponse {
    ok: bool,
    checked_at: String,
    connector_mode: &'static str,
    message: &'static str,
}

#[derive(Debug, Deserialize)]
struct ProductListRequest {
    limit: Option<u16>,
    last_id: Option<String>,
    visibility: Option<String>,
    include_archived_if_empty: Option<bool>,
}

#[derive(Debug, Serialize)]
struct ProductCountResponse {
    count: u32,
    visibility: String,
    archived_fallback: bool,
}

#[derive(Debug, Serialize)]
struct ProductListResponse {
    connector_mode: &'static str,
    products: Vec<ozon_connector::OzonProductSummary>,
    total: u32,
    last_id: Option<String>,
    visibility: String,
    archived_fallback: bool,
}

#[derive(Debug, Deserialize)]
struct ProductGetRequest {
    product_id: Option<String>,
    offer_id: Option<String>,
    sku: Option<String>,
}

impl ProductGetRequest {
    fn into_lookup(self) -> OzonProductLookup {
        OzonProductLookup {
            product_id: self.product_id,
            offer_id: self.offer_id,
            sku: self.sku,
        }
    }
}

#[derive(Debug, Serialize)]
struct ProductGetResponse {
    connector_mode: &'static str,
    product: ozon_connector::OzonProductDetail,
}

#[derive(Debug, Deserialize)]
struct PosterBriefRequest {
    #[serde(flatten)]
    lookup: ProductGetRequest,
    theme: Option<String>,
    locale: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct PosterBrief {
    theme: String,
    headline: String,
    subheadline: String,
    selling_points: Vec<String>,
    cta_line: String,
    compliance_note: String,
    background_prompt: String,
}

#[derive(Debug, Clone, Serialize)]
struct PosterContext {
    product: ozon_connector::OzonProductDetail,
    brief: PosterBrief,
}

#[derive(Debug, Serialize)]
struct PosterBriefResponse {
    connector_mode: &'static str,
    product: ozon_connector::OzonProductDetail,
    brief: PosterBrief,
}

#[derive(Debug, Serialize)]
struct PosterSourceImage {
    role: String,
    url: String,
    note: String,
}

#[derive(Debug, Serialize)]
struct PosterOpenClawHandoff {
    manifest_url: String,
    auth_header: &'static str,
    token_policy: &'static str,
    recommended_tools: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
struct PosterHandoffResponse {
    connector_mode: &'static str,
    generated_at: String,
    mode: &'static str,
    product: ozon_connector::OzonProductDetail,
    brief: PosterBrief,
    source_images: Vec<PosterSourceImage>,
    openclaw: PosterOpenClawHandoff,
    instructions: Vec<&'static str>,
    prompt: String,
}

#[derive(Debug, Serialize)]
struct PosterGenerateResponse {
    connector_mode: &'static str,
    product: ozon_connector::OzonProductDetail,
    brief: PosterBrief,
    image_model: String,
    prompt: String,
    revised_prompt: Option<String>,
    background_data_url: String,
}

#[derive(Debug, Deserialize)]
struct PosterVerifyRequest {
    #[serde(flatten)]
    lookup: ProductGetRequest,
    theme: Option<String>,
    locale: Option<String>,
    headline: String,
    subheadline: String,
    selling_points: Vec<String>,
    cta_line: String,
    compliance_note: String,
}

#[derive(Debug, Clone, Serialize)]
struct PosterCopy {
    headline: String,
    subheadline: String,
    selling_points: Vec<String>,
    cta_line: String,
    compliance_note: String,
}

#[derive(Debug, Serialize)]
struct PosterCopyMismatch {
    field: &'static str,
    expected: String,
    actual: String,
}

#[derive(Debug, Serialize)]
struct PosterVerifyResponse {
    ok: bool,
    checked_at: String,
    approved_copy: PosterCopy,
    mismatches: Vec<PosterCopyMismatch>,
    warnings: Vec<String>,
}

#[derive(Debug)]
struct PosterGeneratedBackground {
    image_model: String,
    prompt: String,
    revised_prompt: Option<String>,
    background_data_url: String,
}

#[derive(Debug, Serialize)]
struct OpenAiImageGenerationRequest {
    model: String,
    prompt: String,
    size: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiImageGenerationResponse {
    data: Vec<OpenAiImageData>,
}

#[derive(Debug, Deserialize)]
struct OpenAiImageData {
    b64_json: Option<String>,
    #[serde(default)]
    url: Option<String>,
    revised_prompt: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiErrorEnvelope {
    error: OpenAiErrorBody,
}

#[derive(Debug, Deserialize)]
struct OpenAiErrorBody {
    code: Option<String>,
    message: Option<String>,
    #[serde(rename = "type")]
    error_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DryRunRequest {
    tenant_id: Option<Uuid>,
    shop_id: Option<String>,
    source: Option<TaskSource>,
    operation: Option<OperationKind>,
    dry_run: Option<DryRunDiff>,
    risk: Option<RiskLevel>,
    idempotency_key: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApproveTaskRequest {
    approved_by: Option<String>,
    note: Option<String>,
}

#[derive(Debug, Serialize)]
struct TaskResponse {
    task: Task,
}

#[derive(Debug, Deserialize)]
struct ConfigureEcommerceScheduleRequest {
    enabled: bool,
    interval_secs: Option<u64>,
    limit: Option<u16>,
}

#[derive(Debug, Deserialize)]
struct ProposeEcommerceScheduleRequest {
    tenant_id: Option<Uuid>,
    shop_id: Option<String>,
    source: Option<TaskSource>,
    interval_secs: Option<u64>,
    limit: Option<u16>,
    idempotency_key: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
struct EcommerceScheduleResponse {
    enabled: bool,
    interval_secs: u64,
    limit: u16,
    connector_mode: &'static str,
    last_run: Option<EcommerceReadRun>,
    last_error: Option<String>,
    audit: Vec<ScheduleAuditEvent>,
    safety: Vec<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
struct EcommerceScheduleRunResponse {
    run: EcommerceReadRun,
}

#[derive(Clone, Debug, Serialize)]
struct EcommerceReadRun {
    started_at: String,
    completed_at: String,
    duration_ms: u64,
    connector_mode: &'static str,
    product_count: u32,
    sample_size: u16,
    next_last_id: Option<String>,
    products: Vec<ozon_connector::OzonProductSummary>,
    total: u32,
}

#[derive(Clone, Debug, Serialize)]
struct ScheduleAuditEvent {
    at: String,
    actor: String,
    action: String,
    summary: String,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }

    fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: message.into(),
        }
    }

    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn bad_gateway(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(serde_json::json!({
                "error": self.message,
            })),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderMap;
    use ozon_secret_store::MemorySecretStore;
    use reqwest::header::HeaderValue as ReqwestHeaderValue;

    use super::*;

    #[test]
    fn map_video_status_handles_common_vendor_strings() {
        assert_eq!(map_video_status("queued"), VideoStatus::Queued);
        assert_eq!(map_video_status("PENDING"), VideoStatus::Queued);
        assert_eq!(map_video_status(" processing "), VideoStatus::Running);
        assert_eq!(map_video_status("running"), VideoStatus::Running);
        assert_eq!(map_video_status("succeeded"), VideoStatus::Succeeded);
        assert_eq!(map_video_status("Completed"), VideoStatus::Succeeded);
        assert_eq!(map_video_status("done"), VideoStatus::Succeeded);
        assert_eq!(map_video_status("failed"), VideoStatus::Failed);
        assert_eq!(map_video_status("error"), VideoStatus::Failed);
        // Unknown -> keep polling (Running), not a false terminal.
        assert_eq!(map_video_status("weird-vendor-state"), VideoStatus::Running);
        assert!(!VideoStatus::Running.is_terminal());
        assert!(VideoStatus::Succeeded.is_terminal());
        assert!(VideoStatus::Failed.is_terminal());
    }

    #[test]
    fn clamp_video_duration_caps_and_defaults() {
        assert_eq!(clamp_video_duration(None), VIDEO_DEFAULT_DURATION_SECS);
        assert_eq!(clamp_video_duration(Some(0)), VIDEO_DEFAULT_DURATION_SECS);
        assert_eq!(clamp_video_duration(Some(5)), 5);
        assert_eq!(
            clamp_video_duration(Some(999)),
            VIDEO_MAX_DURATION_SECS,
            "duration must be clamped to the ceiling"
        );
        assert_eq!(
            clamp_video_duration(Some(VIDEO_MAX_DURATION_SECS)),
            VIDEO_MAX_DURATION_SECS
        );
    }

    #[test]
    fn video_create_response_reads_tolerant_job_id_paths() {
        let from_id: VideoCreateResponse =
            serde_json::from_str(r#"{"id":"job_123"}"#).expect("parse id");
        assert_eq!(from_id.provider_job_id().as_deref(), Some("job_123"));
        let from_job_id: VideoCreateResponse =
            serde_json::from_str(r#"{"job_id":"jb_9"}"#).expect("parse job_id");
        assert_eq!(from_job_id.provider_job_id().as_deref(), Some("jb_9"));
        let empty: VideoCreateResponse = serde_json::from_str(r#"{}"#).expect("parse empty");
        assert_eq!(empty.provider_job_id(), None);
    }

    #[test]
    fn video_status_response_reads_tolerant_url_paths() {
        let a: VideoStatusResponse =
            serde_json::from_str(r#"{"status":"succeeded","video_url":"https://v/a.mp4"}"#)
                .expect("parse a");
        assert_eq!(a.hosted_url().as_deref(), Some("https://v/a.mp4"));
        let b: VideoStatusResponse =
            serde_json::from_str(r#"{"status":"done","output_url":"https://v/b.mp4"}"#)
                .expect("parse b");
        assert_eq!(b.hosted_url().as_deref(), Some("https://v/b.mp4"));
        let c: VideoStatusResponse =
            serde_json::from_str(r#"{"url":"https://v/c.mp4"}"#).expect("parse c");
        assert_eq!(c.hosted_url().as_deref(), Some("https://v/c.mp4"));
    }

    fn dict_attr(id: u64, name: &str, dictionary_id: u64) -> OzonCategoryAttribute {
        OzonCategoryAttribute {
            id,
            name: name.to_string(),
            is_collection: false,
            dictionary_id,
            attribute_type: None,
        }
    }

    fn proposal_attr(name: &str, values: &[&str]) -> Module3Attribute {
        Module3Attribute {
            name: name.to_string(),
            values: values.iter().map(|value| value.to_string()).collect(),
        }
    }

    #[test]
    fn module3_match_exact_dictionary_and_freetext() {
        let dictionary = vec![dict_attr(85, "Brand", 0), dict_attr(10096, "Color", 901)];
        let mut values = HashMap::new();
        values.insert(
            10096,
            vec![OzonCategoryAttributeValue {
                value_id: 5001,
                value: "Graphite".to_string(),
            }],
        );
        let proposal = vec![
            proposal_attr("Brand", &["Acme"]),
            proposal_attr("Color", &["Graphite"]),
        ];

        let outcome = module3_match_attributes(&proposal, &dictionary, &values);

        assert_eq!(outcome.dropped.len(), 0);
        assert_eq!(outcome.resolved.len(), 2);
        // Free-text brand passes the raw string through.
        let brand = outcome.resolved.iter().find(|a| a.attribute_id == 85).unwrap();
        assert!(matches!(&brand.values[0], OzonResolvedValue::FreeText { value } if value == "Acme"));
        // Dictionary color resolves to a numeric value_id.
        let color = outcome.resolved.iter().find(|a| a.attribute_id == 10096).unwrap();
        assert!(matches!(
            &color.values[0],
            OzonResolvedValue::Dictionary { dictionary_value_id: 5001 }
        ));
    }

    #[test]
    fn module3_match_is_case_and_whitespace_insensitive() {
        let dictionary = vec![dict_attr(10096, "Color", 901)];
        let mut values = HashMap::new();
        values.insert(
            10096,
            vec![OzonCategoryAttributeValue {
                value_id: 5001,
                value: "Graphite Grey".to_string(),
            }],
        );
        // Mixed case + extra/internal whitespace on both name and value.
        let proposal = vec![proposal_attr("  cOLOR ", &["  graphite   grey "])];

        let outcome = module3_match_attributes(&proposal, &dictionary, &values);

        assert_eq!(outcome.dropped.len(), 0);
        assert_eq!(outcome.resolved.len(), 1);
        assert!(matches!(
            &outcome.resolved[0].values[0],
            OzonResolvedValue::Dictionary { dictionary_value_id: 5001 }
        ));
    }

    #[test]
    fn module3_match_drops_unmatched_attribute_name() {
        let dictionary = vec![dict_attr(85, "Brand", 0)];
        let values = HashMap::new();
        let proposal = vec![proposal_attr("Totally Unknown", &["whatever"])];

        let outcome = module3_match_attributes(&proposal, &dictionary, &values);

        assert!(outcome.resolved.is_empty());
        assert_eq!(outcome.matched.len(), 0);
        assert_eq!(outcome.dropped.len(), 1);
        assert_eq!(outcome.dropped[0].name, "Totally Unknown");
        assert!(outcome.dropped[0].value.is_none());
        assert!(outcome.dropped[0].reason.contains("not in category dictionary"));
    }

    #[test]
    fn module3_match_drops_unmatched_dictionary_value() {
        let dictionary = vec![dict_attr(10096, "Color", 901)];
        let mut values = HashMap::new();
        values.insert(
            10096,
            vec![OzonCategoryAttributeValue {
                value_id: 5001,
                value: "Graphite".to_string(),
            }],
        );
        // Name matches, value does NOT exist in the dictionary -> value dropped,
        // and since no value survives the attribute is not written at all.
        let proposal = vec![proposal_attr("Color", &["Neon Pink"])];

        let outcome = module3_match_attributes(&proposal, &dictionary, &values);

        assert!(outcome.resolved.is_empty());
        assert_eq!(outcome.dropped.len(), 1);
        assert_eq!(outcome.dropped[0].value.as_deref(), Some("Neon Pink"));
        assert!(outcome.dropped[0].reason.contains("not in attribute dictionary"));
    }

    #[test]
    fn module3_match_freetext_passthrough_without_dictionary_fetch() {
        let dictionary = vec![dict_attr(85, "Brand", 0)];
        // No value dictionary provided at all — free-text must still pass.
        let values = HashMap::new();
        let proposal = vec![proposal_attr("Brand", &["Made-Up Co"])];

        let outcome = module3_match_attributes(&proposal, &dictionary, &values);

        assert_eq!(outcome.dropped.len(), 0);
        assert_eq!(outcome.resolved.len(), 1);
        assert_eq!(outcome.resolved[0].attribute_id, 85);
        assert!(matches!(
            &outcome.resolved[0].values[0],
            OzonResolvedValue::FreeText { value } if value == "Made-Up Co"
        ));
    }

    #[test]
    fn module3_referenced_ids_only_includes_dictionary_typed_matches() {
        let dictionary = vec![dict_attr(85, "Brand", 0), dict_attr(10096, "Color", 901)];
        let proposal = vec![
            proposal_attr("Brand", &["Acme"]),
            proposal_attr("Color", &["Graphite"]),
            proposal_attr("Unknown", &["x"]),
        ];
        let ids = module3_referenced_dictionary_ids(&proposal, &dictionary);
        assert_eq!(ids, vec![10096]);
    }

    fn test_state() -> LocalState {
        LocalState::new_with_secret_store(test_config(), Arc::new(MemorySecretStore::default()))
            .expect("local state")
    }

    fn test_config() -> LocalConfig {
        LocalConfig {
            skill_bind: "127.0.0.1:8790".to_string(),
            agent_bind: "127.0.0.1:17870".to_string(),
            bind_override: true,
            operator_token: "operator-token".to_string(),
            openclaw_token: "bridge-token".to_string(),
            use_real_ozon: false,
            openai_base_url: DEFAULT_OPENAI_BASE_URL.to_string(),
            openai_image_model: DEFAULT_OPENAI_IMAGE_MODEL.to_string(),
            default_ecommerce_interval_secs: 900,
            default_ecommerce_limit: 20,
            lease_public_key_pem: None,
            lease_issuer: "ozon66-cloud".to_string(),
            lease_audience: "ozon-rust-local-node".to_string(),
            allow_unsigned_lease: true,
            openclaw_bind_url: DEFAULT_OPENCLAW_BIND_URL.to_string(),
        }
    }

    #[test]
    fn bridge_token_cannot_satisfy_operator_auth() {
        let state = test_state();
        let mut headers = HeaderMap::new();
        headers.insert("x-openclaw-token", "bridge-token".parse().unwrap());

        let error = require_operator_token(&state, &headers).expect_err("must reject bridge token");
        assert_eq!(error.status, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn bridge_or_operator_auth_accepts_either_limited_token() {
        let state = test_state();
        let mut bridge_headers = HeaderMap::new();
        bridge_headers.insert("x-openclaw-token", "bridge-token".parse().unwrap());
        require_bridge_or_operator_token(&state, &bridge_headers).expect("bridge token");

        let mut operator_headers = HeaderMap::new();
        operator_headers.insert("x-local-token", "operator-token".parse().unwrap());
        require_bridge_or_operator_token(&state, &operator_headers).expect("operator token");
    }

    #[tokio::test]
    async fn openclaw_pairing_uses_one_time_code_without_token_in_url() {
        let state = test_state();
        let mut headers = HeaderMap::new();
        headers.insert("x-local-token", "operator-token".parse().unwrap());

        let start = start_openclaw_pairing(State(state.clone()), headers)
            .await
            .expect("start pairing")
            .0;

        assert!(start.bind_url.starts_with(DEFAULT_OPENCLAW_BIND_URL));
        assert!(start.bind_url.contains("ozon66_pairing_code="));
        assert!(start.bind_url.contains("claim_url="));
        assert!(!start.bind_url.contains("bridge-token"));
        assert!(!start.bind_url.contains("x-openclaw-token"));

        let mut origin_headers = HeaderMap::new();
        origin_headers.insert("origin", "https://ozonclaw.jl696.cn".parse().unwrap());
        let claim = claim_openclaw_pairing(
            State(state.clone()),
            origin_headers.clone(),
            Json(OpenClawPairingClaimRequest {
                code: start.pairing_code.clone(),
            }),
        )
        .await
        .expect("claim pairing")
        .0;

        assert_eq!(claim.status, "paired");
        assert_eq!(claim.auth_header, "x-openclaw-token");
        assert_eq!(claim.auth_token, "bridge-token");
        assert_eq!(
            claim.auth_token_fingerprint,
            fingerprint_secret(&SecretString::from("bridge-token".to_string()))
        );

        let replay = claim_openclaw_pairing(
            State(state),
            origin_headers,
            Json(OpenClawPairingClaimRequest {
                code: start.pairing_code,
            }),
        )
        .await
        .expect_err("pairing code must be one-time");
        assert_eq!(replay.status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn openclaw_pairing_claim_rejects_disallowed_origin() {
        let state = test_state();
        let mut headers = HeaderMap::new();
        headers.insert("x-local-token", "operator-token".parse().unwrap());
        let start = start_openclaw_pairing(State(state.clone()), headers)
            .await
            .expect("start pairing")
            .0;
        let mut bad_origin_headers = HeaderMap::new();
        bad_origin_headers.insert("origin", "https://evil.example.com".parse().unwrap());

        let error = claim_openclaw_pairing(
            State(state),
            bad_origin_headers,
            Json(OpenClawPairingClaimRequest {
                code: start.pairing_code,
            }),
        )
        .await
        .expect_err("claim must reject untrusted origin");

        assert_eq!(error.status, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn openclaw_pairing_claim_rejects_expired_code() {
        let state = test_state();
        let code = "expired-code".to_string();
        state.openclaw_pairings.write().await.insert(
            code.clone(),
            OpenClawPairing {
                expires_at: Instant::now() - Duration::from_secs(1),
                expires_at_rfc3339: Utc::now().to_rfc3339(),
            },
        );
        let mut origin_headers = HeaderMap::new();
        origin_headers.insert("origin", "https://ozonclaw.jl696.cn".parse().unwrap());

        let error = claim_openclaw_pairing(
            State(state),
            origin_headers,
            Json(OpenClawPairingClaimRequest { code }),
        )
        .await
        .expect_err("expired code must be rejected");

        assert_eq!(error.status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn openclaw_pairing_start_requires_operator_token() {
        let state = test_state();
        let mut headers = HeaderMap::new();
        headers.insert("x-openclaw-token", "bridge-token".parse().unwrap());

        let error = start_openclaw_pairing(State(state), headers)
            .await
            .expect_err("bridge token cannot create pairing");

        assert_eq!(error.status, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn openclaw_bind_url_fragment_encodes_pairing_without_secret() {
        let bind_url = build_openclaw_bind_url(
            "https://ozonclaw.jl696.cn/openclaw/import",
            "pairing-123",
            "http://127.0.0.1:8790/openclaw/pairing/claim",
            "http://127.0.0.1:8790/openclaw/manifest",
        )
        .expect("bind url");

        assert!(bind_url.starts_with("https://ozonclaw.jl696.cn/openclaw/import#"));
        assert!(bind_url.contains("pairing-123"));
        assert!(bind_url.contains("http%3A%2F%2F127.0.0.1%3A8790"));
        assert!(!bind_url.contains("bridge-token"));
    }

    #[tokio::test]
    async fn openai_config_save_preserves_existing_key_when_key_is_blank() {
        let state = test_state();
        let mut headers = HeaderMap::new();
        headers.insert("x-local-token", "operator-token".parse().unwrap());

        let _ = save_openai_config(
            State(state.clone()),
            headers.clone(),
            Json(OpenAiConfigRequest {
                api_key: "sk-test-openai-key".to_string(),
                base_url: Some("https://relay.example.com".to_string()),
                image_model: Some("gpt-image-2".to_string()),
            }),
        )
        .await
        .expect("initial OpenAI config save");

        let response = save_openai_config(
            State(state.clone()),
            headers,
            Json(OpenAiConfigRequest {
                api_key: "".to_string(),
                base_url: Some("https://relay.example.com".to_string()),
                image_model: Some("gpt-image-1".to_string()),
            }),
        )
        .await
        .expect("OpenAI config update without retyping key");

        assert_eq!(response.image_model, "gpt-image-1");
        let stored = load_openai_config(&state).await.expect("stored config");
        assert_eq!(stored.api_key, "sk-test-openai-key");
        assert_eq!(stored.image_model, "gpt-image-1");
    }

    async fn save_registry(state: &LocalState, registry: StoredModelRegistry) {
        let mut headers = HeaderMap::new();
        headers.insert("x-local-token", "operator-token".parse().unwrap());
        let _ = save_model_registry(State(state.clone()), headers, Json(registry))
            .await
            .expect("save registry");
    }

    #[tokio::test]
    async fn resolve_image_with_no_registry_matches_load_openai_config() {
        let state = test_state();
        let mut headers = HeaderMap::new();
        headers.insert("x-local-token", "operator-token".parse().unwrap());
        let _ = save_openai_config(
            State(state.clone()),
            headers,
            Json(OpenAiConfigRequest {
                api_key: "sk-legacy".to_string(),
                base_url: Some("https://relay.example.com".to_string()),
                image_model: Some("gpt-image-1".to_string()),
            }),
        )
        .await
        .expect("save openai config");

        let expected = load_openai_config(&state).await.expect("legacy config");
        let resolved = resolve_capability(&state, Capability::ImageGen)
            .await
            .expect("resolve image")
            .expect_openai_image()
            .expect("openai image");
        assert_eq!(resolved.api_key, expected.api_key);
        assert_eq!(resolved.base_url, expected.base_url);
        assert_eq!(resolved.image_model, expected.image_model);
        assert_eq!(resolved.api_key, "sk-legacy");
    }

    #[tokio::test]
    async fn resolve_image_with_enabled_entry_uses_entry_base_url_and_model() {
        let state = test_state();
        let mut headers = HeaderMap::new();
        headers.insert("x-local-token", "operator-token".parse().unwrap());
        let _ = save_openai_config(
            State(state.clone()),
            headers,
            Json(OpenAiConfigRequest {
                api_key: "sk-registry-key".to_string(),
                base_url: Some("https://default.example.com".to_string()),
                image_model: Some("gpt-image-1".to_string()),
            }),
        )
        .await
        .expect("save openai config");

        save_registry(
            &state,
            StoredModelRegistry {
                image_gen: vec![ProviderEntry {
                    kind: model_router::ProviderKind::OpenAiImages,
                    base_url: "https://images.example.com/v1".to_string(),
                    model: "gpt-image-2".to_string(),
                    secret_ref: SECRET_OPENAI_CONFIG.to_string(),
                    auth: model_router::AuthStyle::Bearer,
                    enabled: true,
                    video_dialect: model_router::VideoDialect::default(),
                    extra: Default::default(),
                }],
                ..Default::default()
            },
        )
        .await;

        let resolved = resolve_capability(&state, Capability::ImageGen)
            .await
            .expect("resolve image")
            .expect_openai_image()
            .expect("openai image");
        assert_eq!(resolved.base_url, "https://images.example.com/v1");
        assert_eq!(resolved.image_model, "gpt-image-2");
        assert_eq!(resolved.api_key, "sk-registry-key");
    }

    #[tokio::test]
    async fn resolve_text_gen_returns_generic_or_not_configured() {
        let state = test_state();
        let mut headers = HeaderMap::new();
        headers.insert("x-local-token", "operator-token".parse().unwrap());
        let _ = save_openai_config(
            State(state.clone()),
            headers,
            Json(OpenAiConfigRequest {
                api_key: "sk-text-key".to_string(),
                base_url: Some("https://default.example.com".to_string()),
                image_model: Some("gpt-image-1".to_string()),
            }),
        )
        .await
        .expect("save openai config");

        // No registry yet -> NotConfigured.
        match resolve_capability(&state, Capability::TextGen)
            .await
            .expect("resolve text")
        {
            model_router::ResolvedProvider::NotConfigured { capability, .. } => {
                assert_eq!(capability, "text_gen");
            }
            _ => panic!("expected NotConfigured for text_gen"),
        }

        save_registry(
            &state,
            StoredModelRegistry {
                text_gen: vec![ProviderEntry {
                    kind: model_router::ProviderKind::OpenAiCompatChat,
                    base_url: "https://chat.example.com".to_string(),
                    model: "qwen-max".to_string(),
                    secret_ref: SECRET_OPENAI_CONFIG.to_string(),
                    auth: model_router::AuthStyle::Header {
                        name: "x-api-key".to_string(),
                    },
                    enabled: true,
                    video_dialect: model_router::VideoDialect::default(),
                    extra: Default::default(),
                }],
                ..Default::default()
            },
        )
        .await;

        match resolve_capability(&state, Capability::TextGen)
            .await
            .expect("resolve text")
        {
            model_router::ResolvedProvider::Generic {
                kind,
                base_url,
                model,
                api_key,
                auth,
                ..
            } => {
                assert_eq!(kind, model_router::ProviderKind::OpenAiCompatChat);
                assert_eq!(base_url, "https://chat.example.com");
                assert_eq!(model, "qwen-max");
                assert_eq!(api_key, "sk-text-key");
                assert_eq!(
                    auth,
                    model_router::AuthStyle::Header {
                        name: "x-api-key".to_string()
                    }
                );
            }
            _ => panic!("expected Generic for text_gen"),
        }
    }

    #[tokio::test]
    async fn save_model_registry_rejects_non_loopback_http() {
        let state = test_state();
        let mut headers = HeaderMap::new();
        headers.insert("x-local-token", "operator-token".parse().unwrap());
        let _ = save_openai_config(
            State(state.clone()),
            headers.clone(),
            Json(OpenAiConfigRequest {
                api_key: "sk-key".to_string(),
                base_url: Some("https://default.example.com".to_string()),
                image_model: Some("gpt-image-1".to_string()),
            }),
        )
        .await
        .expect("save openai config");

        let error = save_model_registry(
            State(state.clone()),
            headers,
            Json(StoredModelRegistry {
                image_gen: vec![ProviderEntry {
                    kind: model_router::ProviderKind::OpenAiImages,
                    base_url: "http://images.example.com".to_string(),
                    model: "gpt-image-1".to_string(),
                    secret_ref: SECRET_OPENAI_CONFIG.to_string(),
                    auth: model_router::AuthStyle::Bearer,
                    enabled: true,
                    video_dialect: model_router::VideoDialect::default(),
                    extra: Default::default(),
                }],
                ..Default::default()
            }),
        )
        .await
        .expect_err("non-loopback http must be rejected");
        assert_eq!(error.status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn portal_status_exposes_safe_readiness_without_secrets() {
        let state = test_state();
        let mut headers = HeaderMap::new();
        headers.insert("x-local-token", "operator-token".parse().unwrap());

        let _ = save_ozon_config(
            State(state.clone()),
            headers.clone(),
            Json(OzonConfigRequest {
                client_id: "3169219".to_string(),
                api_key: "ozon-secret".to_string(),
            }),
        )
        .await
        .expect("save Ozon config");
        let _ = save_openai_config(
            State(state.clone()),
            headers,
            Json(OpenAiConfigRequest {
                api_key: "sk-test-openai-key".to_string(),
                base_url: Some("https://relay.example.com".to_string()),
                image_model: Some("gpt-image-1".to_string()),
            }),
        )
        .await
        .expect("save OpenAI config");

        let response = portal_status(State(state)).await.expect("portal status").0;

        assert!(response.ozon.configured);
        assert!(response.openai.configured);
        assert_eq!(response.openai.image_model, "gpt-image-1");
        assert_eq!(response.poster_generation.preferred, "openclaw_codex");
        assert!(response.poster_generation.openclaw_bridge_ready);
        assert!(response.poster_generation.api_fallback_configured);
        assert_eq!(response.ozon.issue, None);
        assert_eq!(response.openai.issue, None);
    }

    #[test]
    fn poster_brief_fallback_copy_is_operator_friendly() {
        let state = test_state();
        let product = ozon_connector::OzonProductDetail {
            lookup: ozon_connector::OzonProductLookup {
                offer_id: Some("offer-1".to_string()),
                ..Default::default()
            },
            product_id: "product-1".to_string(),
            offer_id: "offer-1".to_string(),
            sku: None,
            name: Some("Pocket lighter".to_string()),
            description_category_id: None,
            type_id: None,
            description: None,
            barcodes: vec![],
            primary_image: Some("https://cdn.example.test/product.jpg".to_string()),
            images: vec![ozon_connector::OzonProductImage {
                url: "https://cdn.example.test/product.jpg".to_string(),
                role: ozon_connector::OzonProductImageRole::Primary,
                position: 0,
            }],
            gallery_images: vec![],
            images360: vec![],
            color_image: None,
            attributes: vec![],
            visibility: None,
            archived: None,
            autoarchived: None,
            created_at: None,
            updated_at: None,
            statuses: None,
            source_endpoints: vec![],
            warnings: vec![],
        };

        let context = build_poster_brief(&state, product, "studio", "zh-CN").expect("poster brief");

        assert_eq!(
            context.brief.selling_points,
            vec![
                "商品图来自当前 Ozon 店铺".to_string(),
                "保留包装、颜色和标签细节".to_string(),
                "适合先做首图和活动海报".to_string(),
            ]
        );
        assert!(!context.brief.subheadline.contains("已读取"));
        assert!(!context.brief.cta_line.contains("接口"));
    }

    #[test]
    fn openclaw_poster_handoff_is_account_first_and_secret_free() {
        let state = test_state();
        let product = ozon_connector::OzonProductDetail {
            lookup: ozon_connector::OzonProductLookup {
                offer_id: Some("offer-1".to_string()),
                ..Default::default()
            },
            product_id: "product-1".to_string(),
            offer_id: "offer-1".to_string(),
            sku: Some("sku-1".to_string()),
            name: Some("Pocket lighter".to_string()),
            description_category_id: None,
            type_id: None,
            description: None,
            barcodes: vec![],
            primary_image: Some("https://cdn.example.test/product.jpg".to_string()),
            images: vec![ozon_connector::OzonProductImage {
                url: "https://cdn.example.test/product.jpg".to_string(),
                role: ozon_connector::OzonProductImageRole::Primary,
                position: 0,
            }],
            gallery_images: vec![],
            images360: vec![],
            color_image: None,
            attributes: vec![],
            visibility: None,
            archived: Some(false),
            autoarchived: None,
            created_at: None,
            updated_at: None,
            statuses: None,
            source_endpoints: vec![],
            warnings: vec![],
        };
        let context = build_poster_brief(&state, product, "studio", "zh-CN").expect("poster brief");
        let images = poster_source_images(&context.product, "zh-CN");
        let prompt =
            build_openclaw_poster_prompt(&context.product, &context.brief, &images, "zh-CN");

        assert!(prompt.contains("OpenClaw/Codex"));
        assert!(prompt.contains("不要要求用户额外提供 OpenAI API Key"));
        assert!(prompt.contains("https://cdn.example.test/product.jpg"));
        assert!(!prompt.contains("bridge-token"));
        assert!(!prompt.contains("operator-token"));
    }

    #[test]
    fn token_comparison_checks_full_secret() {
        assert!(constant_time_eq("operator-token", "operator-token"));
        assert!(!constant_time_eq("operator-token", "operator-token-extra"));
        assert!(!constant_time_eq("operator-token", "operator-tokem"));
    }

    #[test]
    fn bundled_lease_public_key_is_valid_rsa_pem() {
        RsaPublicKey::from_public_key_pem(DEFAULT_LEASE_PUBLIC_KEY_PEM)
            .expect("bundled lease public key must parse");
    }

    #[tokio::test]
    async fn openclaw_proposal_does_not_enable_schedule() {
        let state = test_state();
        let mut headers = HeaderMap::new();
        headers.insert("x-openclaw-token", "bridge-token".parse().unwrap());

        let response = propose_ecommerce_schedule(
            State(state.clone()),
            headers,
            Json(ProposeEcommerceScheduleRequest {
                tenant_id: None,
                shop_id: None,
                source: None,
                interval_secs: Some(60),
                limit: Some(3),
                idempotency_key: Some("schedule-test".to_string()),
            }),
        )
        .await
        .expect("schedule proposal");

        assert_eq!(response.task.operation, OperationKind::OzonProductsList);
        assert_eq!(response.task.state, ozon_domain::TaskState::Queued);
        assert!(!state.schedules.read().await.enabled);
    }

    #[tokio::test]
    async fn bridge_token_cannot_enable_schedule() {
        let state = test_state();
        let mut headers = HeaderMap::new();
        headers.insert("x-openclaw-token", "bridge-token".parse().unwrap());

        let error = configure_ecommerce_schedule(
            State(state),
            headers,
            Json(ConfigureEcommerceScheduleRequest {
                enabled: true,
                interval_secs: Some(60),
                limit: Some(3),
            }),
        )
        .await
        .expect_err("bridge token must not enable schedule");

        assert_eq!(error.status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn bridge_token_can_read_product_detail() {
        let state = test_state();
        let mut headers = HeaderMap::new();
        headers.insert("x-openclaw-token", "bridge-token".parse().unwrap());

        let response = ozon_products_get(
            State(state),
            headers,
            Json(ProductGetRequest {
                product_id: Some("mock-product-1".to_string()),
                offer_id: None,
                sku: None,
            }),
        )
        .await
        .expect("product detail");

        assert_eq!(response.connector_mode, "mock");
        assert_eq!(response.product.product_id, "mock-product-1");
        assert_eq!(response.product.images.len(), 2);
    }

    #[tokio::test]
    async fn real_mode_requires_cloud_lease_before_product_read() {
        let mut config = test_config();
        config.use_real_ozon = true;
        let state =
            LocalState::new_with_secret_store(config, Arc::new(MemorySecretStore::default()))
                .expect("local state");
        let mut headers = HeaderMap::new();
        headers.insert("x-openclaw-token", "bridge-token".parse().unwrap());

        let error = ozon_products_get(
            State(state),
            headers,
            Json(ProductGetRequest {
                product_id: Some("mock-product-1".to_string()),
                offer_id: None,
                sku: None,
            }),
        )
        .await
        .expect_err("real reads require a cloud lease");

        assert_eq!(error.status, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn local_cors_allows_ozon66_private_network_preflight() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test local node");
        let addr = listener.local_addr().expect("local addr");
        let server = tokio::spawn(async move {
            axum::serve(listener, skill_router(test_state()))
                .await
                .expect("serve test local node");
        });

        let client = reqwest::Client::new();
        for origin in [
            "https://ozon66.com",
            "https://cn.ozon66.com",
            "https://ozonclaw.jl696.cn",
        ] {
            let response = client
                .request(reqwest::Method::OPTIONS, format!("http://{addr}/health"))
                .header("Origin", origin)
                .header("Access-Control-Request-Method", "GET")
                .header("Access-Control-Request-Private-Network", "true")
                .send()
                .await
                .expect("preflight response");
            let expected_origin = ReqwestHeaderValue::from_str(origin).expect("test origin header");

            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(
                response.headers().get("access-control-allow-origin"),
                Some(&expected_origin)
            );
            assert_eq!(
                response
                    .headers()
                    .get("access-control-allow-private-network"),
                Some(&ReqwestHeaderValue::from_static("true"))
            );
        }

        server.abort();
    }

    #[tokio::test]
    async fn product_get_rejects_ambiguous_lookup() {
        let state = test_state();
        let mut headers = HeaderMap::new();
        headers.insert("x-openclaw-token", "bridge-token".parse().unwrap());

        let error = ozon_products_get(
            State(state),
            headers,
            Json(ProductGetRequest {
                product_id: Some("mock-product-1".to_string()),
                offer_id: Some("SKU-MOCK-1".to_string()),
                sku: None,
            }),
        )
        .await
        .expect_err("ambiguous lookup");

        assert_eq!(error.status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn operator_can_run_read_schedule_once_with_mock_connector() {
        let state = test_state();
        let mut headers = HeaderMap::new();
        headers.insert("x-local-token", "operator-token".parse().unwrap());

        let response = run_ecommerce_schedule_now(State(state), headers)
            .await
            .expect("manual read run");

        assert_eq!(response.run.product_count, 3);
        assert_eq!(response.run.sample_size, 3);
    }

    #[test]
    fn real_ozon_mode_rejects_default_local_tokens() {
        let mut config = test_config();
        config.use_real_ozon = true;
        config.operator_token = DEFAULT_DEV_LOCAL_TOKEN.to_string();
        config.openclaw_token = DEFAULT_DEV_OPENCLAW_TOKEN.to_string();

        assert!(config.validate().is_err());
    }

    #[test]
    fn module3_parses_fenced_and_quoted_json() {
        let fenced = "```json\n{\"title\":\"T\",\"description\":\"D\",\"attributes\":[{\"name\":\"Цвет\",\"values\":[\"красный\"]}],\"type_category\":\"type_id=1\"}\n```";
        let parsed = parse_module3_result(fenced).expect("fenced json");
        assert_eq!(parsed.title, "T");
        assert_eq!(parsed.description, "D");
        assert_eq!(parsed.attributes.len(), 1);
        assert_eq!(parsed.attributes[0].name, "Цвет");
        assert_eq!(parsed.type_category, "type_id=1");

        let plain = "{\"title\":\"X\",\"description\":\"\",\"attributes\":[],\"type_category\":\"\"}";
        assert_eq!(parse_module3_result(plain).expect("plain").title, "X");

        assert!(parse_module3_result("not json").is_err());
    }

    #[test]
    fn module3_word_boundary_truncation_caps_on_space() {
        let value = "alpha beta gamma delta";
        // 12 chars lands mid-"gamma"; we cut back to the last full word.
        assert_eq!(word_boundary_truncate(value, 12), "alpha beta");
        assert_eq!(word_boundary_truncate("short", 200), "short");
    }

    #[test]
    fn module3_labeled_document_has_all_four_sections() {
        let fields = Module3Fields {
            title: "Title".to_string(),
            description: "Desc".to_string(),
            attributes: vec![Module3Attribute {
                name: "Brand".to_string(),
                values: vec!["Acme".to_string(), "Co".to_string()],
            }],
            type_category: "type_id=20001, description_category_id=10001".to_string(),
        };
        let doc = module3_labeled_document(&fields);
        assert!(doc.contains("===TITLE===\nTitle"));
        assert!(doc.contains("===DESCRIPTION===\nDesc"));
        assert!(doc.contains("Brand: Acme; Co"));
        assert!(doc.contains("===TYPE/CATEGORY===\ntype_id=20001"));
    }

    #[tokio::test]
    async fn module3_source_fields_cap_images_at_six() {
        let connector = MockOzonConnector;
        let credentials = OzonCredentials {
            client_id: "mock".to_string(),
            api_key: SecretString::from("mock"),
        };
        let detail = connector
            .product_get(
                &credentials,
                OzonProductLookup {
                    product_id: Some("mock-product-1".to_string()),
                    offer_id: None,
                    sku: None,
                },
            )
            .await
            .expect("detail");
        let fields = module3_source_fields(&detail);
        assert_eq!(fields.title, "Mock Ozon product 2");
        assert_eq!(fields.description, "Mock Ozon description for product 2");
        assert!(fields.type_category.contains("type_id="));
        assert!(module3_image_urls(&detail).len() <= MODULE3_MAX_IMAGES);
    }
}
