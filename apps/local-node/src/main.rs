use std::{
    env, fs,
    net::SocketAddr,
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
    MockOzonConnector, OzonCredentials, OzonProductListPage, OzonProductLookup, OzonReadConnector,
};
use ozon_domain::{
    DryRunDiff, EntitlementLease, ExecutionReceipt, Feature, FieldChange, OperationKind, RiskLevel,
    Task, TaskId, TaskSource, TenantId,
};
use ozon_secret_store::{SecretName, SecretStore, SystemSecretStore, fingerprint_secret, redact};
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

const DEFAULT_DEV_LOCAL_TOKEN: &str = "dev-local-token";
const DEFAULT_DEV_OPENCLAW_TOKEN: &str = "dev-openclaw-token";
const DEFAULT_OPENAI_IMAGE_MODEL: &str = "gpt-image-1";
const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com";
const SECRET_OZON_CONFIG: &str = "ozon_config";
const SECRET_OPENAI_CONFIG: &str = "openai_config";
const SECRET_CLOUD_LEASE: &str = "cloud_lease";
const SECRET_DEVICE_FINGERPRINT: &str = "device_fingerprint";
const PROTOCOL_VERSION: &str = "2026-05-13.local-node.v1";
const BUILD_COMMIT: &str = match option_env!("GITHUB_SHA") {
    Some(value) => value,
    None => "local-build",
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ozon_local_node=info,tower_http=info,axum=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = LocalConfig::from_env();
    config.validate()?;
    let state = LocalState::new(config.clone())?;
    let skill_addr: SocketAddr = config.skill_bind.parse()?;
    let agent_addr: SocketAddr = config.agent_bind.parse()?;

    let skill = run_server(skill_addr, skill_router(state.clone()));
    let agent = run_server(agent_addr, agent_router(state.clone()));
    tracing::info!(%skill_addr, %agent_addr, "starting local node services");
    tokio::try_join!(skill, agent)?;
    Ok(())
}

async fn run_server(addr: SocketAddr, router: Router) -> anyhow::Result<()> {
    if !addr.ip().is_loopback() {
        anyhow::bail!("local-node refuses to bind non-loopback address: {addr}");
    }
    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, router).await?;
    Ok(())
}

fn skill_router(state: LocalState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/diagnostics", get(diagnostics))
        .route("/portal/status", get(portal_status))
        .route("/portal/lease", post(save_portal_lease))
        .route("/openclaw/manifest", get(openclaw_manifest))
        .route("/config/status", get(config_status))
        .route("/config/ozon", post(save_ozon_config))
        .route("/config/ozon/validate", post(validate_ozon_config))
        .route("/config/openai", post(save_openai_config))
        .route("/tools/ozon.products.count", post(ozon_products_count))
        .route("/tools/ozon.products.list", post(ozon_products_list))
        .route("/tools/ozon.products.get", post(ozon_products_get))
        .route("/poster/brief", post(poster_brief))
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
    CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(|origin: &HeaderValue, _| {
            origin
                .to_str()
                .map(|origin| {
                    origin == "http://localhost:5173"
                        || origin == "http://127.0.0.1:5173"
                        || origin == "http://localhost:5171"
                        || origin == "http://127.0.0.1:5171"
                        || origin == "https://ozon66.com"
                        || origin == "https://www.ozon66.com"
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
    skill_bind: String,
    agent_bind: String,
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
        Self {
            skill_bind: env::var("OZON_LOCAL_SKILL_BIND")
                .unwrap_or_else(|_| "127.0.0.1:8790".to_string()),
            agent_bind: env::var("OZON_LOCAL_AGENT_BIND")
                .unwrap_or_else(|_| "127.0.0.1:17870".to_string()),
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
                .or_else(|| option_env!("OZON_SUITE_LEASE_PUBLIC_KEY_PEM").map(str::to_string)),
            lease_issuer: env::var("OZON_SUITE_LEASE_ISSUER")
                .unwrap_or_else(|_| "ozon66-cloud".to_string()),
            lease_audience: env::var("OZON_SUITE_LEASE_AUDIENCE")
                .unwrap_or_else(|_| "ozon-rust-local-node".to_string()),
            allow_unsigned_lease: env::var("OZON_LOCAL_ALLOW_UNSIGNED_LEASE")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(cfg!(debug_assertions)),
        }
    }

    fn validate(&self) -> anyhow::Result<()> {
        if self.use_real_ozon
            && (self.operator_token == DEFAULT_DEV_LOCAL_TOKEN
                || self.openclaw_token == DEFAULT_DEV_OPENCLAW_TOKEN)
        {
            anyhow::bail!(
                "OZON_LOCAL_TOKEN and OZON_OPENCLAW_TOKEN must be explicitly set when the real Ozon connector is enabled"
            );
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
    cloud_lease_cache: Arc<RwLock<Option<EntitlementLease>>>,
    ozon_connector: Arc<dyn OzonReadConnector>,
    http_client: reqwest::Client,
    schedules: ScheduleStore,
}

impl LocalState {
    fn new(config: LocalConfig) -> anyhow::Result<Self> {
        let ozon_connector: Arc<dyn OzonReadConnector> = if config.use_real_ozon {
            Arc::new(ozon_connector::OzonHttpClient::new())
        } else {
            if !cfg!(debug_assertions) {
                anyhow::bail!(
                    "mock Ozon connector is disabled in non-debug builds; set OZON_CONNECTOR_MODE=real"
                );
            }
            Arc::new(MockOzonConnector)
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
            secrets: Arc::new(SystemSecretStore::new("ozon-rust-suite-local", "default")?),
            ozon_config_cache: Arc::new(RwLock::new(None)),
            openai_config_cache: Arc::new(RwLock::new(None)),
            cloud_lease_cache: Arc::new(RwLock::new(None)),
            ozon_connector,
            http_client: reqwest::Client::builder()
                .user_agent("ozon-rust-suite-local/0.1")
                .timeout(Duration::from_secs(90))
                .build()
                .map_err(|error| anyhow::anyhow!("failed to build HTTP client: {error}"))?,
            schedules,
        })
    }
}

type ScheduleStore = Arc<RwLock<EcommerceReadSchedule>>;

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
        package_version: env!("CARGO_PKG_VERSION"),
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

async fn diagnostics(State(state): State<LocalState>) -> Json<DiagnosticsResponse> {
    let ozon = inspect_ozon_credentials(&state).await;
    let openai = inspect_openai_config(&state).await;
    let lease = inspect_cloud_lease(&state).await;
    Json(DiagnosticsResponse {
        service: "ozon-local-node",
        status: "ok",
        checked_at: Utc::now().to_rfc3339(),
        protocol_version: PROTOCOL_VERSION,
        build_commit: BUILD_COMMIT,
        package_version: env!("CARGO_PKG_VERSION"),
        skill_api: local_http_url(&state.config.skill_bind),
        agent_api: local_http_url(&state.config.agent_bind),
        connector_mode: connector_mode(&state),
        real_ozon_enabled: state.config.use_real_ozon,
        secret_store: SecretStoreStatus {
            backend: "system",
            available: ozon.secret_store_available,
        },
        ozon: OzonCredentialStatus {
            configured: ozon.configured,
            source: ozon.source,
            client_id: ozon.client_id,
            api_key_fingerprint: ozon.api_key_fingerprint,
            issue: ozon.issue,
        },
        openai,
        lease,
    })
}

async fn portal_status(
    State(state): State<LocalState>,
) -> Result<Json<PortalStatusResponse>, ApiError> {
    let device_fingerprint = load_or_create_device_fingerprint(&state).await?;
    let lease = inspect_cloud_lease(&state).await;
    Ok(Json(PortalStatusResponse {
        service: "ozon-local-node",
        status: "online",
        checked_at: Utc::now().to_rfc3339(),
        skill_api: local_http_url(&state.config.skill_bind),
        agent_api: local_http_url(&state.config.agent_bind),
        manifest_url: format!(
            "{}/openclaw/manifest",
            local_http_url(&state.config.skill_bind)
        ),
        bridge_auth_header: "x-openclaw-token",
        protocol_version: PROTOCOL_VERSION,
        build_commit: BUILD_COMMIT,
        package_version: env!("CARGO_PKG_VERSION"),
        real_ozon_enabled: state.config.use_real_ozon,
        device_fingerprint,
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
        version: env!("CARGO_PKG_VERSION"),
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
    let api_key = input.api_key.trim();
    if api_key.is_empty() {
        return Err(ApiError::bad_request("OpenAI API key is required"));
    }
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

async fn save_portal_lease(
    State(state): State<LocalState>,
    Json(input): Json<PortalLeaseRequest>,
) -> Result<Json<PortalLeaseResponse>, ApiError> {
    validate_cloud_lease(&state, &input.lease)?;
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
    Ok(Json(ConfigStatusResponse {
        service: "ozon-local-node",
        checked_at: Utc::now().to_rfc3339(),
        real_ozon_enabled: state.config.use_real_ozon,
        connector_mode: connector_mode(&state),
        secret_store: SecretStoreStatus {
            backend: "system_keyring",
            available: ozon.secret_store_available,
        },
        ozon: OzonCredentialStatus {
            configured: ozon.configured,
            source: ozon.source,
            client_id: ozon.client_id,
            api_key_fingerprint: ozon.api_key_fingerprint,
            issue: ozon.issue,
        },
        openai,
        lease: inspect_cloud_lease(&state).await,
        endpoints: LocalEndpointStatus {
            skill_api: local_http_url(&state.config.skill_bind),
            agent_api: local_http_url(&state.config.agent_bind),
            manifest_url: format!(
                "{}/openclaw/manifest",
                local_http_url(&state.config.skill_bind)
            ),
        },
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
    require_operator_token(&state, &headers)?;
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
    let poster = build_poster_brief(
        &state,
        load_product_for_lookup(&state, lookup.into_lookup()).await?,
        theme.as_deref().unwrap_or("studio"),
        locale.as_deref().unwrap_or("zh-CN"),
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
    let warnings = if mismatches.is_empty() {
        vec![
            "校验通过：当前文案与真实商品 fact pack 一致。".to_string(),
            "商品主体应继续使用真实主图合成，避免让图片模型重画包装和文字。".to_string(),
        ]
    } else {
        vec![
            "当前文案和事实包不一致，建议直接回退到系统生成稿。".to_string(),
            "这一步只做逐字段比对，不会帮你猜测哪些自由改写仍然安全。".to_string(),
        ]
    };
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

async fn load_or_create_device_fingerprint(state: &LocalState) -> Result<String, ApiError> {
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
    if !lease.features.contains(&Feature::OzonRead) {
        return Err(ApiError::bad_request(
            "lease does not include Ozon read access",
        ));
    }
    verify_cloud_lease_signature(state, lease)?;
    Ok(())
}

fn lease_status(state: &LocalState, lease: &EntitlementLease) -> LeaseStatus {
    let validation = validate_cloud_lease(state, lease);
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
    if !matches!(url.scheme(), "https" | "http") {
        return Err(ApiError::bad_request(
            "OpenAI base URL must use http or https",
        ));
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

async fn load_product_for_lookup(
    state: &LocalState,
    lookup: OzonProductLookup,
) -> Result<ozon_connector::OzonProductDetail, ApiError> {
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
        .filter_map(|attribute| {
            let name = attribute.name.as_deref()?.trim();
            let value = attribute.values.first()?.trim();
            if name.is_empty() || value.is_empty() {
                return None;
            }
            Some(format!("{name}: {}", truncate_text(value, 22)))
        })
        .take(3)
        .collect::<Vec<_>>();
    let selling_points = if attribute_points.is_empty() {
        vec![
            format!("Ozon 实时商品：{}", product.offer_id),
            format!("已读取 {} 张商品图", product.images.len()),
            format!("已整理 {} 条属性", product.attributes.len()),
        ]
    } else {
        attribute_points
    };
    let image_count = product.images.len();
    let subheadline = if image_count > 0 {
        format!("先用真实商品图站住画面，再把 Ozon 已有属性整理成能直接上图的卖点。")
    } else {
        "这件商品还没带主图，先补图再出海报会更稳。".to_string()
    };
    let compliance_note = if state.config.use_real_ozon {
        "商品主体与文案来自 Ozon 实时数据；AI 只负责背景氛围，不重画商品包装。".to_string()
    } else {
        "当前是本地 mock 模式，正式出图前请切到真实 Ozon API 再校验一次。".to_string()
    };
    let cta_line = match locale {
        "zh-CN" => "主图、卖点和活动视觉可以先在这一版里敲定".to_string(),
        _ => "Lock the hero image, then tune the selling points in one pass.".to_string(),
    };
    let image_stage = if product.primary_image.is_some() {
        "Reserve a clean stage in the lower-right area for compositing the real product cutout."
    } else {
        "Leave the center clean for a product to be placed later."
    };
    let background_prompt = format!(
        "Create a premium e-commerce poster background only, with no product, no packaging, no text, no logo, and no watermark. Theme: {}. Mood: confident, commercial, polished. Use light, shadow, reflections, and spatial depth to support a seller campaign poster. {} Palette should feel modern and readable behind Chinese text overlays. Keep the composition suitable for a 4:5 portrait poster.",
        normalize_theme(theme),
        image_stage
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

async fn generate_poster_background(
    state: &LocalState,
    brief: &PosterBrief,
) -> Result<PosterGeneratedBackground, ApiError> {
    let openai = load_openai_config(state).await?;
    let request = OpenAiImageGenerationRequest {
        model: openai.image_model.clone(),
        prompt: brief.background_prompt.clone(),
        size: "1536x1024".to_string(),
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

fn summarize_openai_error(body: &str) -> String {
    if let Ok(envelope) = serde_json::from_str::<OpenAiErrorEnvelope>(body) {
        let message = envelope
            .error
            .message
            .unwrap_or_else(|| "upstream returned an error without a message".to_string());
        if envelope.error.code.as_deref() == Some("model_not_found") {
            return format!(
                "image model is not available on this relay key: {message}. Change the Image model to one enabled by the relay, or use a key with an image generation channel."
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
    lease: LeaseStatus,
    features: Vec<Feature>,
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
    openai: OpenAiCredentialStatus,
    lease: LeaseStatus,
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
    openai: OpenAiCredentialStatus,
    lease: LeaseStatus,
    endpoints: LocalEndpointStatus,
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

    use super::*;

    fn test_state() -> LocalState {
        LocalState::new(test_config()).expect("local state")
    }

    fn test_config() -> LocalConfig {
        LocalConfig {
            skill_bind: "127.0.0.1:8790".to_string(),
            agent_bind: "127.0.0.1:17870".to_string(),
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

    #[test]
    fn token_comparison_checks_full_secret() {
        assert!(constant_time_eq("operator-token", "operator-token"));
        assert!(!constant_time_eq("operator-token", "operator-token-extra"));
        assert!(!constant_time_eq("operator-token", "operator-tokem"));
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
}
