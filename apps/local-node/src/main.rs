use std::{
    env,
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
use chrono::Utc;
use ozon_connector::{MockOzonConnector, OzonCredentials, OzonProductListPage, OzonReadConnector};
use ozon_domain::{
    DryRunDiff, ExecutionReceipt, Feature, FieldChange, OperationKind, RiskLevel, Task, TaskId,
    TaskSource, TenantId,
};
use ozon_secret_store::{SecretName, SecretStore, SystemSecretStore, fingerprint_secret, redact};
use ozon_task_engine::{CreateTask, TaskEvent, TaskStore};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
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
        .route("/openclaw/manifest", get(openclaw_manifest))
        .route("/config/status", get(config_status))
        .route("/config/ozon", post(save_ozon_config))
        .route("/config/ozon/validate", post(validate_ozon_config))
        .route("/tools/ozon.products.count", post(ozon_products_count))
        .route("/tools/ozon.products.list", post(ozon_products_list))
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
                        || origin.starts_with("tauri://")
                })
                .unwrap_or(false)
        }))
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(tower_http::cors::Any)
}

#[derive(Clone)]
struct LocalConfig {
    skill_bind: String,
    agent_bind: String,
    operator_token: String,
    openclaw_token: String,
    use_real_ozon: bool,
    default_ecommerce_interval_secs: u64,
    default_ecommerce_limit: u16,
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
            default_ecommerce_interval_secs: env_u64("OZON_ECOMMERCE_READ_INTERVAL_SECS", 15 * 60),
            default_ecommerce_limit: env_u16("OZON_ECOMMERCE_READ_LIMIT", 20),
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
    ozon_connector: Arc<dyn OzonReadConnector>,
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
            ozon_connector,
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
            SecretName::new("ozon_config"),
            SecretString::from(bundle_json),
        )
        .await
        .map_err(|_| ApiError::internal("failed to save Ozon config"))?;
    Ok(Json(OzonConfigResponse {
        client_id: redact(client_id),
        api_key: redact(api_key),
        saved_at: Utc::now().to_rfc3339(),
    }))
}

async fn config_status(
    State(state): State<LocalState>,
    headers: HeaderMap,
) -> Result<Json<ConfigStatusResponse>, ApiError> {
    require_operator_token(&state, &headers)?;
    let ozon = inspect_ozon_credentials(&state).await;
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
    let count = state
        .ozon_connector
        .product_count(&credentials)
        .await
        .map_err(|error| ApiError::bad_gateway(format!("ozon connector failed: {error}")))?;
    Ok(Json(ProductCountResponse { count }))
}

async fn ozon_products_list(
    State(state): State<LocalState>,
    headers: HeaderMap,
    Json(input): Json<ProductListRequest>,
) -> Result<Json<ProductListResponse>, ApiError> {
    require_bridge_or_operator_token(&state, &headers)?;
    let credentials = load_ozon_credentials(&state).await?;
    let products = state
        .ozon_connector
        .product_list_page(&credentials, input.limit.unwrap_or(20), None)
        .await
        .map_err(|error| ApiError::bad_gateway(format!("ozon connector failed: {error}")))?;
    Ok(Json(ProductListResponse {
        connector_mode: connector_mode(&state),
        products: products.products,
        total: products.total,
        last_id: products.last_id,
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
    let page = state
        .ozon_connector
        .product_list_page(&credentials, limit.clamp(1, 100), None)
        .await
        .map_err(|error| ApiError::bad_gateway(format!("scheduled Ozon read failed: {error}")))?;
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
    match state.secrets.get(&SecretName::new("ozon_config")).await {
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
    let client_id = match state.secrets.get(&SecretName::new("ozon_client_id")).await {
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
    let api_key = match state.secrets.get(&SecretName::new("ozon_api_key")).await {
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

async fn load_ozon_credentials(state: &LocalState) -> Result<OzonCredentials, ApiError> {
    if !state.config.use_real_ozon {
        return Ok(debug_mock_ozon_credentials());
    }

    if let Some(bundle) = state
        .secrets
        .get(&SecretName::new("ozon_config"))
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
    features: Vec<Feature>,
    real_ozon_enabled: bool,
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

#[derive(Debug, Deserialize, Serialize)]
struct StoredOzonConfig {
    client_id: String,
    api_key: String,
}

#[derive(Debug, Serialize)]
struct OzonConfigResponse {
    client_id: String,
    api_key: String,
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
}

#[derive(Debug, Serialize)]
struct ProductCountResponse {
    count: u32,
}

#[derive(Debug, Serialize)]
struct ProductListResponse {
    connector_mode: &'static str,
    products: Vec<ozon_connector::OzonProductSummary>,
    total: u32,
    last_id: Option<String>,
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
            default_ecommerce_interval_secs: 900,
            default_ecommerce_limit: 20,
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
