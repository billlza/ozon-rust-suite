use std::{env, net::SocketAddr};

use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{
        HeaderMap, HeaderValue, Method, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE, HeaderName},
    },
    response::{IntoResponse, Response},
    routing::{get, post},
};
use chrono::{DateTime, Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use ozon_domain::{
    AuditEvent, AuditEventId, CardKey, CardKeyId, CardKeyStatus, Device, DeviceId, DeviceStatus,
    Email, Entitlement, EntitlementId, EntitlementLease, Feature, NebulaId, NebulaSource, Order,
    OrderId, OrderStatus, PhoneNumber, PlanCode, TenantId, User, UserId, UserRole,
};
use rand::{Rng, distr::Alphanumeric};
use rand_core::OsRng;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{
    PgPool, Postgres, Row, Transaction,
    postgres::{PgPoolOptions, PgRow},
};
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    trace::TraceLayer,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;

const DEFAULT_DEV_JWT_SECRET: &str = "dev-secret-change-before-production-32";
const DEFAULT_DEV_ADMIN_TOKEN: &str = "dev-admin-token";
const DEFAULT_DEV_SKYBRIDGE_API_BASE_URLS: &[&str] = &[
    "http://127.0.0.1:8788",
    "https://hloqytmhjludmuhwyyzb.supabase.co/functions/v1",
];
const DEV_CORS_ORIGINS: &[&str] = &[
    "http://127.0.0.1:5171",
    "http://localhost:5171",
    "http://127.0.0.1:5172",
    "http://localhost:5172",
];

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ozon_cloud_api=info,tower_http=info,axum=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = AppConfig::from_env();
    let bind: SocketAddr = config.bind.parse()?;
    config.validate(bind)?;
    let db = PgPoolOptions::new()
        .max_connections(config.database_max_connections)
        .connect(&config.database_url)
        .await?;
    sqlx::migrate!("./migrations").run(&db).await?;
    let app = app_router(AppState::new(config, db));
    tracing::info!(%bind, "starting Ozon Rust Suite cloud API");
    let listener = tokio::net::TcpListener::bind(bind).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

fn app_router(state: AppState) -> Router {
    let cors = cloud_cors(&state.config);
    Router::new()
        .route("/health", get(health))
        .route("/auth/register", post(register))
        .route("/auth/login", post(login))
        .route("/auth/skybridge", post(auth_skybridge))
        .route("/me", get(me))
        .route("/orders", post(create_order))
        .route("/orders/{id}", get(get_order))
        .route("/admin/orders/{id}/confirm", post(confirm_order))
        .route(
            "/admin/orders/by-reference/{payment_reference}/confirm",
            post(confirm_order_by_reference),
        )
        .route("/admin/card-keys", post(create_card_keys))
        .route("/card-keys/redeem", post(redeem_card_key))
        .route("/devices/activate", post(activate_device))
        .route("/entitlements/lease", post(issue_lease))
        .route("/entitlements/revoke", post(revoke_entitlement))
        .route("/downloads", get(downloads))
        .route("/audit", get(audit_log))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

#[derive(Clone)]
struct AppConfig {
    bind: String,
    database_url: String,
    database_max_connections: u32,
    environment: String,
    jwt_secret: String,
    admin_token: String,
    download_url: String,
    skybridge_api_base_urls: Vec<String>,
    allow_local_nebula_registration: bool,
    cors_allowed_origins: Vec<String>,
    cors_allowed_origins_configured: bool,
}

impl AppConfig {
    fn from_env() -> Self {
        let bind = env::var("OZON_SUITE_BIND")
            .or_else(|_| env::var("PORT").map(|port| format!("0.0.0.0:{port}")))
            .unwrap_or_else(|_| "127.0.0.1:8080".to_string());
        Self {
            bind,
            database_url: env::var("DATABASE_URL").unwrap_or_else(|_| {
                "postgres://ozon:ozon@127.0.0.1:5432/ozon_rust_suite".to_string()
            }),
            database_max_connections: env::var("OZON_SUITE_DB_MAX_CONNECTIONS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(10),
            environment: env::var("OZON_SUITE_ENV").unwrap_or_else(|_| "development".to_string()),
            jwt_secret: env::var("OZON_SUITE_JWT_SECRET")
                .unwrap_or_else(|_| DEFAULT_DEV_JWT_SECRET.to_string()),
            admin_token: env::var("OZON_SUITE_ADMIN_TOKEN")
                .unwrap_or_else(|_| DEFAULT_DEV_ADMIN_TOKEN.to_string()),
            download_url: env::var("OZON_SUITE_PORTAL_DOWNLOAD_URL").unwrap_or_else(|_| {
                "https://downloads.example.com/ozon-local-node.msi".to_string()
            }),
            skybridge_api_base_urls: skybridge_api_base_urls_from_env(),
            allow_local_nebula_registration: env::var("OZON_SUITE_ALLOW_LOCAL_NEBULA_REGISTRATION")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(false),
            cors_allowed_origins: cors_allowed_origins_from_env().unwrap_or_else(|| {
                DEV_CORS_ORIGINS
                    .iter()
                    .map(|origin| (*origin).to_string())
                    .collect()
            }),
            cors_allowed_origins_configured: env::var("OZON_SUITE_CORS_ALLOWED_ORIGINS").is_ok(),
        }
    }

    fn validate(&self, bind: SocketAddr) -> anyhow::Result<()> {
        let production_like =
            self.environment.eq_ignore_ascii_case("production") || !bind.ip().is_loopback();
        let dev_override = env::var("OZON_SUITE_DEV_ALLOW_INSECURE_DEFAULTS").as_deref() == Ok("1");

        if production_like && self.jwt_secret == DEFAULT_DEV_JWT_SECRET && !dev_override {
            anyhow::bail!(
                "OZON_SUITE_JWT_SECRET must be set before running cloud-api in production or on a non-loopback bind"
            );
        }
        if production_like && self.admin_token == DEFAULT_DEV_ADMIN_TOKEN && !dev_override {
            anyhow::bail!(
                "OZON_SUITE_ADMIN_TOKEN must be set before running cloud-api in production or on a non-loopback bind"
            );
        }
        if production_like && self.jwt_secret.len() < 32 {
            anyhow::bail!("OZON_SUITE_JWT_SECRET must be at least 32 bytes in production");
        }
        if production_like && self.admin_token.len() < 24 {
            anyhow::bail!("OZON_SUITE_ADMIN_TOKEN must be at least 24 bytes in production");
        }
        if production_like && !self.cors_allowed_origins_configured && !dev_override {
            anyhow::bail!(
                "OZON_SUITE_CORS_ALLOWED_ORIGINS must be set before running cloud-api in production or on a non-loopback bind"
            );
        }
        if self.cors_allowed_origins.is_empty() {
            anyhow::bail!("at least one CORS origin must be configured");
        }
        for origin in &self.cors_allowed_origins {
            origin
                .parse::<HeaderValue>()
                .map_err(|_| anyhow::anyhow!("invalid CORS origin: {origin}"))?;
        }
        Ok(())
    }
}

fn cors_allowed_origins_from_env() -> Option<Vec<String>> {
    env::var("OZON_SUITE_CORS_ALLOWED_ORIGINS")
        .ok()
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect()
        })
}

fn skybridge_api_base_urls_from_env() -> Vec<String> {
    let raw_value = env::var("OZON_SUITE_SKYBRIDGE_API_BASE_URLS")
        .or_else(|_| env::var("OZON_SUITE_SKYBRIDGE_API_BASE_URL"))
        .unwrap_or_else(|_| DEFAULT_DEV_SKYBRIDGE_API_BASE_URLS.join(","));
    raw_value
        .split(',')
        .map(|value| value.trim().trim_end_matches('/'))
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

fn cloud_cors(config: &AppConfig) -> CorsLayer {
    let origins = config
        .cors_allowed_origins
        .iter()
        .map(|origin| {
            origin
                .parse::<HeaderValue>()
                .expect("validated CORS origin")
        })
        .collect::<Vec<_>>();
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([
            AUTHORIZATION,
            CONTENT_TYPE,
            HeaderName::from_static("x-admin-token"),
        ])
}

#[derive(Clone)]
struct AppState {
    config: AppConfig,
    db: PgPool,
}

impl AppState {
    fn new(config: AppConfig, db: PgPool) -> Self {
        Self { config, db }
    }
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        service: "ozon-cloud-api",
        status: "ok",
    })
}

async fn register(
    State(state): State<AppState>,
    Json(input): Json<RegisterRequest>,
) -> Result<Json<AuthResponse>, ApiError> {
    let identity = RegistrationIdentity::from_request(&input)?;
    if input.password.len() < 8 {
        return Err(ApiError::bad_request(
            "password must be at least 8 characters",
        ));
    }

    let tenant_id = TenantId::new();
    if !state.config.allow_local_nebula_registration {
        return Err(ApiError::forbidden(
            "local account registration is disabled; use SkyBridge identity",
        ));
    }
    let nebula_id = create_unique_nebula_id(&state.db).await?;
    let user = User {
        id: UserId::new(),
        tenant_id,
        nebula_id,
        nebula_source: NebulaSource::LocalDev,
        skybridge_user_id: None,
        email: identity.email,
        phone: identity.phone,
        name: input.name,
        password_hash: hash_password(&SecretString::from(input.password))?,
        role: UserRole::User,
        email_verified_at: None,
        phone_verified_at: None,
        created_at: Utc::now(),
    };

    let mut tx = state.db.begin().await.map_err(db_internal)?;
    insert_tenant(&mut tx, tenant_id).await?;
    insert_user(&mut tx, &user)
        .await
        .map_err(|error| map_identity_unique_conflict(error))?;
    insert_audit(
        &mut tx,
        &audit(
            Some(tenant_id),
            "anonymous",
            "auth.register",
            user.nebula_id.as_str(),
            "user registered",
        ),
    )
    .await?;
    tx.commit().await.map_err(db_internal)?;

    let token = issue_jwt(&state.config, user.id, user.tenant_id, user.role)?;
    Ok(Json(AuthResponse {
        token,
        user: UserResponse::from_user(&user),
    }))
}

async fn auth_skybridge(
    State(state): State<AppState>,
    Json(input): Json<SkybridgeAuthRequest>,
) -> Result<Json<AuthResponse>, ApiError> {
    let profile = resolve_skybridge_profile(&state.config, &input.access_token).await?;
    let user = upsert_skybridge_user(&state.db, profile).await?;
    insert_audit_pool(
        &state.db,
        &audit(
            Some(user.tenant_id),
            user.nebula_id.as_str(),
            "auth.skybridge",
            "user",
            "skybridge identity synced",
        ),
    )
    .await?;
    let token = issue_jwt(&state.config, user.id, user.tenant_id, user.role)?;
    Ok(Json(AuthResponse {
        token,
        user: UserResponse::from_user(&user),
    }))
}

async fn login(
    State(state): State<AppState>,
    Json(input): Json<LoginRequest>,
) -> Result<Json<AuthResponse>, ApiError> {
    let identity = LoginIdentity::from_request(&input)?;
    let user = find_user_by_login_identity(&state.db, &identity)
        .await?
        .ok_or_else(|| ApiError::unauthorized("invalid credentials"))?;
    ensure_local_password_login_allowed(&user)?;
    verify_password(&user.password_hash, &SecretString::from(input.password))?;
    insert_audit_pool(
        &state.db,
        &audit(
            Some(user.tenant_id),
            user.nebula_id.as_str(),
            "auth.login",
            "user",
            "user logged in",
        ),
    )
    .await?;

    let token = issue_jwt(&state.config, user.id, user.tenant_id, user.role)?;
    Ok(Json(AuthResponse {
        token,
        user: UserResponse::from_user(&user),
    }))
}

async fn me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<MeResponse>, ApiError> {
    let claims = require_user(&state, &headers)?;
    let user = find_user_by_id(&state.db, claims.sub)
        .await?
        .ok_or_else(|| ApiError::unauthorized("user not found"))?;
    let entitlements = list_entitlements_for_user(&state.db, user.id).await?;
    Ok(Json(MeResponse {
        user: UserResponse::from_user(&user),
        entitlements,
    }))
}

async fn create_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<CreateOrderRequest>,
) -> Result<Json<OrderResponse>, ApiError> {
    let claims = require_user(&state, &headers)?;
    let order = Order {
        id: OrderId::new(),
        tenant_id: claims.tenant_id,
        user_id: claims.sub,
        plan_code: PlanCode(
            input
                .plan_code
                .unwrap_or_else(|| "standard_30d".to_string()),
        ),
        status: OrderStatus::PendingManualPayment,
        payment_reference: format!("OZON-{}", Uuid::new_v4().simple()),
        created_at: Utc::now(),
        confirmed_at: None,
    };
    let mut tx = state.db.begin().await.map_err(db_internal)?;
    insert_order(&mut tx, &order).await?;
    insert_audit(
        &mut tx,
        &audit(
            Some(order.tenant_id),
            &format!("{:?}", order.user_id),
            "order.created",
            &format!("{:?}", order.id),
            "manual payment order created",
        ),
    )
    .await?;
    tx.commit().await.map_err(db_internal)?;
    Ok(Json(OrderResponse { order }))
}

async fn get_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<OrderResponse>, ApiError> {
    let claims = require_user(&state, &headers)?;
    let order = find_order_by_id(&state.db, OrderId(id))
        .await?
        .filter(|order| order.user_id == claims.sub || claims.role == UserRole::Admin)
        .ok_or_else(|| ApiError::not_found("order not found"))?;
    Ok(Json(OrderResponse { order }))
}

async fn confirm_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<ConfirmOrderResponse>, ApiError> {
    require_admin(&state, &headers)?;
    let mut tx = state.db.begin().await.map_err(db_internal)?;
    let order = confirm_pending_order(&mut tx, OrderId(id)).await?;
    let response = confirm_order_card_key(&mut tx, order).await?;
    tx.commit().await.map_err(db_internal)?;
    Ok(Json(response))
}

async fn confirm_order_by_reference(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(payment_reference): Path<String>,
) -> Result<Json<ConfirmOrderResponse>, ApiError> {
    require_admin(&state, &headers)?;
    let mut tx = state.db.begin().await.map_err(db_internal)?;
    let order = confirm_pending_order_by_reference(&mut tx, &payment_reference).await?;
    let response = confirm_order_card_key(&mut tx, order).await?;
    tx.commit().await.map_err(db_internal)?;
    Ok(Json(response))
}

async fn confirm_order_card_key(
    tx: &mut Transaction<'_, Postgres>,
    order: Order,
) -> Result<ConfirmOrderResponse, ApiError> {
    let duration_days = 30;
    let max_devices = 1;
    validate_card_key_limits(duration_days, max_devices)?;
    let generated = generate_card_key(
        &order.plan_code,
        order.tenant_id,
        duration_days,
        max_devices,
    )?;
    insert_card_key(tx, &generated.card_key, Some(order.id))
        .await
        .map_err(|error| {
            map_unique_conflict(
                error,
                "uq_card_keys_order_id",
                "order already has a card key",
            )
        })?;
    insert_audit(
        tx,
        &audit(
            Some(order.tenant_id),
            "admin",
            "order.confirmed",
            &format!("{:?}", order.id),
            "manual payment confirmed and card key generated",
        ),
    )
    .await?;
    Ok(ConfirmOrderResponse {
        order,
        card_key: generated.plain_code,
    })
}

async fn create_card_keys(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<CreateCardKeysRequest>,
) -> Result<Json<CreateCardKeysResponse>, ApiError> {
    require_admin(&state, &headers)?;
    let count = input.count.clamp(1, 50);
    let plan_code = PlanCode(
        input
            .plan_code
            .unwrap_or_else(|| "standard_30d".to_string()),
    );
    let duration_days = input.duration_days.unwrap_or(30);
    let max_devices = input.max_devices.unwrap_or(1);
    validate_card_key_limits(duration_days, max_devices)?;
    let tenant_id = TenantId::new();
    let mut plain_codes = Vec::with_capacity(count as usize);

    let mut tx = state.db.begin().await.map_err(db_internal)?;
    insert_tenant(&mut tx, tenant_id).await?;
    for _ in 0..count {
        let generated = generate_card_key(&plan_code, tenant_id, duration_days, max_devices)?;
        plain_codes.push(generated.plain_code.clone());
        insert_card_key(&mut tx, &generated.card_key, None)
            .await
            .map_err(db_internal)?;
    }
    insert_audit(
        &mut tx,
        &audit(
            Some(tenant_id),
            "admin",
            "card_keys.created",
            "card_keys",
            &format!("{count} card keys created"),
        ),
    )
    .await?;
    tx.commit().await.map_err(db_internal)?;
    Ok(Json(CreateCardKeysResponse {
        card_keys: plain_codes,
    }))
}

async fn redeem_card_key(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<RedeemCardKeyRequest>,
) -> Result<Json<RedeemCardKeyResponse>, ApiError> {
    let claims = require_user(&state, &headers)?;
    let code = SecretString::from(input.card_key);
    let fingerprint = card_key_fingerprint(code.expose_secret());

    let mut tx = state.db.begin().await.map_err(db_internal)?;
    let card = find_card_key_by_fingerprint_for_update(&mut tx, &fingerprint)
        .await?
        .ok_or_else(|| ApiError::not_found("card key not found"))?;
    if card.status != CardKeyStatus::Available {
        return Err(ApiError::conflict("card key is not available"));
    }
    verify_password(&card.code_hash, &code)?;

    let redeemed_at = Utc::now();
    let entitlement = Entitlement {
        id: EntitlementId::new(),
        tenant_id: claims.tenant_id,
        user_id: claims.sub,
        plan_code: card.plan_code.clone(),
        source_card_key_id: card.id,
        features: default_features(),
        expires_at: redeemed_at + Duration::days(card.duration_days as i64),
        revoked_at: None,
    };
    mark_card_key_redeemed(&mut tx, card.id, claims.sub, redeemed_at).await?;
    insert_entitlement(&mut tx, &entitlement)
        .await
        .map_err(|error| {
            map_unique_conflict(
                error,
                "uq_entitlements_source_card_key",
                "card key is already redeemed",
            )
        })?;
    insert_audit(
        &mut tx,
        &audit(
            Some(claims.tenant_id),
            &format!("{:?}", claims.sub),
            "card_key.redeemed",
            &format!("{:?}", card.id),
            "card key redeemed",
        ),
    )
    .await?;
    tx.commit().await.map_err(db_internal)?;

    Ok(Json(RedeemCardKeyResponse { entitlement }))
}

async fn activate_device(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<ActivateDeviceRequest>,
) -> Result<Json<DeviceResponse>, ApiError> {
    let claims = require_user(&state, &headers)?;
    if input.name.trim().is_empty() {
        return Err(ApiError::bad_request("device name is required"));
    }
    if input.fingerprint.trim().is_empty() {
        return Err(ApiError::bad_request("device fingerprint is required"));
    }
    let fingerprint_hash = hash_fingerprint(&input.fingerprint);
    let now = Utc::now();

    let mut tx = state.db.begin().await.map_err(db_internal)?;
    lock_user_for_update(&mut tx, claims.sub).await?;
    let entitlement = find_active_entitlement_with_card_limit_for_user(&mut tx, claims.sub)
        .await?
        .ok_or_else(|| ApiError::forbidden("no active entitlement"))?;
    let existing = find_device_by_fingerprint(&mut tx, claims.sub, &fingerprint_hash).await?;
    if !existing
        .as_ref()
        .is_some_and(|device| device.status == DeviceStatus::Active)
    {
        let active_count = count_active_devices_for_user(&mut tx, claims.sub).await?;
        if !device_limit_allows_new_activation(active_count, entitlement.max_devices) {
            return Err(ApiError::forbidden(format!(
                "device limit reached for this entitlement ({}/{})",
                active_count, entitlement.max_devices
            )));
        }
    }
    let device = upsert_device(
        &mut tx,
        Device {
            id: DeviceId::new(),
            tenant_id: claims.tenant_id,
            user_id: claims.sub,
            name: input.name,
            fingerprint_hash,
            status: DeviceStatus::Active,
            activated_at: now,
            last_seen_at: Some(now),
        },
    )
    .await?;
    insert_audit(
        &mut tx,
        &audit(
            Some(device.tenant_id),
            &format!("{:?}", device.user_id),
            "device.activated",
            &format!("{:?}", device.id),
            "device activated",
        ),
    )
    .await?;
    tx.commit().await.map_err(db_internal)?;
    Ok(Json(DeviceResponse { device }))
}

async fn issue_lease(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<IssueLeaseRequest>,
) -> Result<Json<LeaseResponse>, ApiError> {
    let claims = require_user(&state, &headers)?;
    let mut tx = state.db.begin().await.map_err(db_internal)?;
    let device = find_active_device(&mut tx, DeviceId(input.device_id), claims.sub)
        .await?
        .ok_or_else(|| ApiError::not_found("active device not found"))?;
    let entitlement = find_active_entitlement_for_user(&mut tx, claims.sub)
        .await?
        .ok_or_else(|| ApiError::forbidden("no active entitlement"))?;
    enforce_device_limit_for_lease(&mut tx, claims.sub).await?;
    touch_device_last_seen(&mut tx, device.id, Utc::now()).await?;
    tx.commit().await.map_err(db_internal)?;
    let lease = EntitlementLease::new(claims.tenant_id, claims.sub, device.id, &entitlement);
    Ok(Json(LeaseResponse { lease }))
}

async fn revoke_entitlement(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<RevokeEntitlementRequest>,
) -> Result<Json<EntitlementResponse>, ApiError> {
    require_admin(&state, &headers)?;
    let mut tx = state.db.begin().await.map_err(db_internal)?;
    let entitlement = revoke_entitlement_by_id(&mut tx, EntitlementId(input.entitlement_id))
        .await?
        .ok_or_else(|| ApiError::not_found("entitlement not found"))?;
    insert_audit(
        &mut tx,
        &audit(
            Some(entitlement.tenant_id),
            "admin",
            "entitlement.revoked",
            &format!("{:?}", entitlement.id),
            "entitlement revoked",
        ),
    )
    .await?;
    tx.commit().await.map_err(db_internal)?;
    Ok(Json(EntitlementResponse { entitlement }))
}

async fn downloads(State(state): State<AppState>) -> Json<DownloadsResponse> {
    Json(DownloadsResponse {
        local_node: state.config.download_url,
        checksum: "publish-checksum-after-building-installer".to_string(),
    })
}

async fn audit_log(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<AuditEvent>>, ApiError> {
    require_admin(&state, &headers)?;
    Ok(Json(list_audit_events(&state.db).await?))
}

async fn insert_tenant(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: TenantId,
) -> Result<(), ApiError> {
    sqlx::query("INSERT INTO tenants (id, created_at) VALUES ($1, $2) ON CONFLICT (id) DO NOTHING")
        .bind(tenant_id.0)
        .bind(Utc::now())
        .execute(&mut **tx)
        .await
        .map_err(db_internal)?;
    Ok(())
}

async fn insert_user(tx: &mut Transaction<'_, Postgres>, user: &User) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO users (
            id, tenant_id, nebula_id, nebula_source, skybridge_user_id, email, phone,
            name, password_hash, role, email_verified_at, phone_verified_at, created_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
        "#,
    )
    .bind(user.id.0)
    .bind(user.tenant_id.0)
    .bind(user.nebula_id.as_str())
    .bind(nebula_source_to_db(user.nebula_source))
    .bind(user.skybridge_user_id)
    .bind(user.email.as_ref().map(Email::as_str))
    .bind(user.phone.as_ref().map(PhoneNumber::as_str))
    .bind(&user.name)
    .bind(&user.password_hash)
    .bind(user_role_to_db(user.role))
    .bind(user.email_verified_at)
    .bind(user.phone_verified_at)
    .bind(user.created_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn upsert_skybridge_user(db: &PgPool, profile: SkybridgeProfile) -> Result<User, ApiError> {
    let mut tx = db.begin().await.map_err(db_internal)?;
    let existing = find_user_by_skybridge_user_id_for_update(&mut tx, profile.user_id).await?;
    let user = if let Some(existing) = existing {
        update_skybridge_user(&mut tx, existing.id, &profile).await?
    } else {
        let tenant_id = TenantId::new();
        let user = User {
            id: UserId::new(),
            tenant_id,
            nebula_id: profile.nebula_id,
            nebula_source: NebulaSource::Skybridge,
            skybridge_user_id: Some(profile.user_id),
            email: profile.email,
            phone: profile.phone,
            name: profile.display_name,
            password_hash: skybridge_password_sentinel(),
            role: UserRole::User,
            email_verified_at: Some(Utc::now()),
            phone_verified_at: None,
            created_at: Utc::now(),
        };
        insert_tenant(&mut tx, tenant_id).await?;
        insert_user(&mut tx, &user)
            .await
            .map_err(|error| map_identity_unique_conflict(error))?;
        user
    };
    tx.commit().await.map_err(db_internal)?;
    Ok(user)
}

async fn find_user_by_skybridge_user_id_for_update(
    tx: &mut Transaction<'_, Postgres>,
    skybridge_user_id: Uuid,
) -> Result<Option<User>, ApiError> {
    sqlx::query(
        r#"
        SELECT id, tenant_id, nebula_id, nebula_source, skybridge_user_id,
               email, phone, name, password_hash, role,
               email_verified_at, phone_verified_at, created_at
        FROM users
        WHERE skybridge_user_id = $1
        FOR UPDATE
        "#,
    )
    .bind(skybridge_user_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(db_internal)?
    .map(row_to_user)
    .transpose()
}

async fn update_skybridge_user(
    tx: &mut Transaction<'_, Postgres>,
    user_id: UserId,
    profile: &SkybridgeProfile,
) -> Result<User, ApiError> {
    sqlx::query(
        r#"
        UPDATE users
        SET nebula_id = $2,
            nebula_source = 'skybridge',
            skybridge_user_id = $3,
            email = $4,
            phone = $5,
            name = COALESCE($6, name),
            email_verified_at = COALESCE(email_verified_at, $7)
        WHERE id = $1
        RETURNING id, tenant_id, nebula_id, nebula_source, skybridge_user_id,
                  email, phone, name, password_hash, role,
                  email_verified_at, phone_verified_at, created_at
        "#,
    )
    .bind(user_id.0)
    .bind(profile.nebula_id.as_str())
    .bind(profile.user_id)
    .bind(profile.email.as_ref().map(Email::as_str))
    .bind(profile.phone.as_ref().map(PhoneNumber::as_str))
    .bind(&profile.display_name)
    .bind(Utc::now())
    .fetch_one(&mut **tx)
    .await
    .map_err(db_internal)
    .and_then(row_to_user)
}

async fn find_user_by_login_identity(
    db: &PgPool,
    identity: &LoginIdentity,
) -> Result<Option<User>, ApiError> {
    let (field, value) = match identity {
        LoginIdentity::Email(email) => ("email", email.as_str()),
        LoginIdentity::Phone(phone) => ("phone", phone.as_str()),
        LoginIdentity::NebulaId(nebula_id) => ("nebula_id", nebula_id.as_str()),
    };
    let query = format!(
        r#"
        SELECT id, tenant_id, nebula_id, nebula_source, skybridge_user_id,
               email, phone, name, password_hash, role,
               email_verified_at, phone_verified_at, created_at
        FROM users
        WHERE {field} = $1
        "#
    );
    sqlx::query(&query)
        .bind(value)
        .fetch_optional(db)
        .await
        .map_err(db_internal)?
        .map(row_to_user)
        .transpose()
}

async fn nebula_id_exists(db: &PgPool, nebula_id: &NebulaId) -> Result<bool, ApiError> {
    sqlx::query(
        r#"
        SELECT id
        FROM users
        WHERE nebula_id = $1
        "#,
    )
    .bind(nebula_id.as_str())
    .fetch_optional(db)
    .await
    .map_err(db_internal)
    .map(|row| row.is_some())
}

async fn find_user_by_id(db: &PgPool, user_id: UserId) -> Result<Option<User>, ApiError> {
    sqlx::query(
        r#"
        SELECT id, tenant_id, nebula_id, nebula_source, skybridge_user_id,
               email, phone, name, password_hash, role,
               email_verified_at, phone_verified_at, created_at
        FROM users
        WHERE id = $1
        "#,
    )
    .bind(user_id.0)
    .fetch_optional(db)
    .await
    .map_err(db_internal)?
    .map(row_to_user)
    .transpose()
}

async fn insert_order(tx: &mut Transaction<'_, Postgres>, order: &Order) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        INSERT INTO orders (id, tenant_id, user_id, plan_code, status, payment_reference, created_at, confirmed_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
    )
    .bind(order.id.0)
    .bind(order.tenant_id.0)
    .bind(order.user_id.0)
    .bind(&order.plan_code.0)
    .bind(order_status_to_db(order.status))
    .bind(&order.payment_reference)
    .bind(order.created_at)
    .bind(order.confirmed_at)
    .execute(&mut **tx)
    .await
    .map_err(db_internal)?;
    Ok(())
}

async fn find_order_by_id(db: &PgPool, order_id: OrderId) -> Result<Option<Order>, ApiError> {
    sqlx::query(
        r#"
        SELECT id, tenant_id, user_id, plan_code, status, payment_reference, created_at, confirmed_at
        FROM orders
        WHERE id = $1
        "#,
    )
    .bind(order_id.0)
    .fetch_optional(db)
    .await
    .map_err(db_internal)?
    .map(row_to_order)
    .transpose()
}

async fn confirm_pending_order(
    tx: &mut Transaction<'_, Postgres>,
    order_id: OrderId,
) -> Result<Order, ApiError> {
    let row = sqlx::query(
        r#"
        UPDATE orders
        SET status = 'confirmed', confirmed_at = $2
        WHERE id = $1 AND status = 'pending_manual_payment'
        RETURNING id, tenant_id, user_id, plan_code, status, payment_reference, created_at, confirmed_at
        "#,
    )
    .bind(order_id.0)
    .bind(Utc::now())
    .fetch_optional(&mut **tx)
    .await
    .map_err(db_internal)?;
    if let Some(row) = row {
        return row_to_order(row);
    }

    let existing = sqlx::query(
        r#"
        SELECT id, tenant_id, user_id, plan_code, status, payment_reference, created_at, confirmed_at
        FROM orders
        WHERE id = $1
        "#,
    )
    .bind(order_id.0)
    .fetch_optional(&mut **tx)
    .await
    .map_err(db_internal)?;
    match existing {
        Some(_) => Err(ApiError::conflict("order is not pending manual payment")),
        None => Err(ApiError::not_found("order not found")),
    }
}

async fn confirm_pending_order_by_reference(
    tx: &mut Transaction<'_, Postgres>,
    payment_reference: &str,
) -> Result<Order, ApiError> {
    let row = sqlx::query(
        r#"
        UPDATE orders
        SET status = 'confirmed', confirmed_at = $2
        WHERE payment_reference = $1 AND status = 'pending_manual_payment'
        RETURNING id, tenant_id, user_id, plan_code, status, payment_reference, created_at, confirmed_at
        "#,
    )
    .bind(payment_reference)
    .bind(Utc::now())
    .fetch_optional(&mut **tx)
    .await
    .map_err(db_internal)?;
    if let Some(row) = row {
        return row_to_order(row);
    }

    let existing = sqlx::query(
        r#"
        SELECT id, tenant_id, user_id, plan_code, status, payment_reference, created_at, confirmed_at
        FROM orders
        WHERE payment_reference = $1
        "#,
    )
    .bind(payment_reference)
    .fetch_optional(&mut **tx)
    .await
    .map_err(db_internal)?;
    match existing {
        Some(_) => Err(ApiError::conflict("order is not pending manual payment")),
        None => Err(ApiError::not_found("order not found")),
    }
}

async fn insert_card_key(
    tx: &mut Transaction<'_, Postgres>,
    card_key: &CardKey,
    order_id: Option<OrderId>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO card_keys (
            id, tenant_id, order_id, plan_code, code_hash, code_fingerprint,
            duration_days, max_devices, status, redeemed_by, redeemed_at, created_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        "#,
    )
    .bind(card_key.id.0)
    .bind(card_key.tenant_id.0)
    .bind(order_id.map(|id| id.0))
    .bind(&card_key.plan_code.0)
    .bind(&card_key.code_hash)
    .bind(&card_key.code_fingerprint)
    .bind(i32::from(card_key.duration_days))
    .bind(i32::from(card_key.max_devices))
    .bind(card_key_status_to_db(card_key.status))
    .bind(card_key.redeemed_by.map(|id| id.0))
    .bind(card_key.redeemed_at)
    .bind(card_key.created_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn find_card_key_by_fingerprint_for_update(
    tx: &mut Transaction<'_, Postgres>,
    fingerprint: &str,
) -> Result<Option<CardKey>, ApiError> {
    sqlx::query(
        r#"
        SELECT id, tenant_id, plan_code, code_hash, code_fingerprint,
               duration_days, max_devices, status, redeemed_by, redeemed_at, created_at
        FROM card_keys
        WHERE code_fingerprint = $1
        FOR UPDATE
        "#,
    )
    .bind(fingerprint)
    .fetch_optional(&mut **tx)
    .await
    .map_err(db_internal)?
    .map(row_to_card_key)
    .transpose()
}

async fn mark_card_key_redeemed(
    tx: &mut Transaction<'_, Postgres>,
    card_key_id: CardKeyId,
    user_id: UserId,
    redeemed_at: DateTime<Utc>,
) -> Result<(), ApiError> {
    let result = sqlx::query(
        r#"
        UPDATE card_keys
        SET status = 'redeemed', redeemed_by = $2, redeemed_at = $3
        WHERE id = $1 AND status = 'available'
        "#,
    )
    .bind(card_key_id.0)
    .bind(user_id.0)
    .bind(redeemed_at)
    .execute(&mut **tx)
    .await
    .map_err(db_internal)?;
    if result.rows_affected() == 0 {
        return Err(ApiError::conflict("card key is not available"));
    }
    Ok(())
}

async fn insert_entitlement(
    tx: &mut Transaction<'_, Postgres>,
    entitlement: &Entitlement,
) -> Result<(), sqlx::Error> {
    let features: Vec<String> = entitlement
        .features
        .iter()
        .copied()
        .map(feature_to_db)
        .map(str::to_string)
        .collect();
    sqlx::query(
        r#"
        INSERT INTO entitlements (
            id, tenant_id, user_id, plan_code, source_card_key_id,
            features, expires_at, revoked_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
    )
    .bind(entitlement.id.0)
    .bind(entitlement.tenant_id.0)
    .bind(entitlement.user_id.0)
    .bind(&entitlement.plan_code.0)
    .bind(entitlement.source_card_key_id.0)
    .bind(features)
    .bind(entitlement.expires_at)
    .bind(entitlement.revoked_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn list_entitlements_for_user(
    db: &PgPool,
    user_id: UserId,
) -> Result<Vec<Entitlement>, ApiError> {
    let rows = sqlx::query(
        r#"
        SELECT id, tenant_id, user_id, plan_code, source_card_key_id, features, expires_at, revoked_at
        FROM entitlements
        WHERE user_id = $1
        ORDER BY expires_at DESC
        "#,
    )
    .bind(user_id.0)
    .fetch_all(db)
    .await
    .map_err(db_internal)?;
    rows.into_iter().map(row_to_entitlement).collect()
}

async fn lock_user_for_update(
    tx: &mut Transaction<'_, Postgres>,
    user_id: UserId,
) -> Result<(), ApiError> {
    let exists = sqlx::query("SELECT id FROM users WHERE id = $1 FOR UPDATE")
        .bind(user_id.0)
        .fetch_optional(&mut **tx)
        .await
        .map_err(db_internal)?
        .is_some();
    if exists {
        Ok(())
    } else {
        Err(ApiError::unauthorized("user not found"))
    }
}

async fn upsert_device(
    tx: &mut Transaction<'_, Postgres>,
    device: Device,
) -> Result<Device, ApiError> {
    sqlx::query(
        r#"
        INSERT INTO devices (
            id, tenant_id, user_id, name, fingerprint_hash, status, activated_at, last_seen_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT ON CONSTRAINT uq_devices_user_fingerprint
        DO UPDATE SET
            name = EXCLUDED.name,
            status = 'active',
            last_seen_at = EXCLUDED.last_seen_at
        RETURNING id, tenant_id, user_id, name, fingerprint_hash, status, activated_at, last_seen_at
        "#,
    )
    .bind(device.id.0)
    .bind(device.tenant_id.0)
    .bind(device.user_id.0)
    .bind(&device.name)
    .bind(&device.fingerprint_hash)
    .bind(device_status_to_db(device.status))
    .bind(device.activated_at)
    .bind(device.last_seen_at)
    .fetch_one(&mut **tx)
    .await
    .map_err(db_internal)
    .and_then(row_to_device)
}

async fn find_device_by_fingerprint(
    tx: &mut Transaction<'_, Postgres>,
    user_id: UserId,
    fingerprint_hash: &str,
) -> Result<Option<Device>, ApiError> {
    sqlx::query(
        r#"
        SELECT id, tenant_id, user_id, name, fingerprint_hash, status, activated_at, last_seen_at
        FROM devices
        WHERE user_id = $1 AND fingerprint_hash = $2
        "#,
    )
    .bind(user_id.0)
    .bind(fingerprint_hash)
    .fetch_optional(&mut **tx)
    .await
    .map_err(db_internal)?
    .map(row_to_device)
    .transpose()
}

async fn find_active_device(
    tx: &mut Transaction<'_, Postgres>,
    device_id: DeviceId,
    user_id: UserId,
) -> Result<Option<Device>, ApiError> {
    sqlx::query(
        r#"
        SELECT id, tenant_id, user_id, name, fingerprint_hash, status, activated_at, last_seen_at
        FROM devices
        WHERE id = $1 AND user_id = $2 AND status = 'active'
        "#,
    )
    .bind(device_id.0)
    .bind(user_id.0)
    .fetch_optional(&mut **tx)
    .await
    .map_err(db_internal)?
    .map(row_to_device)
    .transpose()
}

async fn count_active_devices_for_user(
    tx: &mut Transaction<'_, Postgres>,
    user_id: UserId,
) -> Result<i64, ApiError> {
    let count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)::bigint
        FROM devices
        WHERE user_id = $1 AND status = 'active'
        "#,
    )
    .bind(user_id.0)
    .fetch_one(&mut **tx)
    .await
    .map_err(db_internal)?;
    Ok(count)
}

async fn touch_device_last_seen(
    tx: &mut Transaction<'_, Postgres>,
    device_id: DeviceId,
    seen_at: DateTime<Utc>,
) -> Result<(), ApiError> {
    sqlx::query("UPDATE devices SET last_seen_at = $2 WHERE id = $1")
        .bind(device_id.0)
        .bind(seen_at)
        .execute(&mut **tx)
        .await
        .map_err(db_internal)?;
    Ok(())
}

async fn find_active_entitlement_for_user(
    tx: &mut Transaction<'_, Postgres>,
    user_id: UserId,
) -> Result<Option<Entitlement>, ApiError> {
    sqlx::query(
        r#"
        SELECT id, tenant_id, user_id, plan_code, source_card_key_id, features, expires_at, revoked_at
        FROM entitlements
        WHERE user_id = $1 AND revoked_at IS NULL AND expires_at > $2
        ORDER BY expires_at DESC
        LIMIT 1
        "#,
    )
    .bind(user_id.0)
    .bind(Utc::now())
    .fetch_optional(&mut **tx)
    .await
    .map_err(db_internal)?
    .map(row_to_entitlement)
    .transpose()
}

async fn find_active_entitlement_with_card_limit_for_user(
    tx: &mut Transaction<'_, Postgres>,
    user_id: UserId,
) -> Result<Option<EntitlementDeviceLimit>, ApiError> {
    sqlx::query(
        r#"
        SELECT c.max_devices
        FROM entitlements e
        JOIN card_keys c ON c.id = e.source_card_key_id
        WHERE e.user_id = $1 AND e.revoked_at IS NULL AND e.expires_at > $2
        ORDER BY e.expires_at DESC
        LIMIT 1
        "#,
    )
    .bind(user_id.0)
    .bind(Utc::now())
    .fetch_optional(&mut **tx)
    .await
    .map_err(db_internal)?
    .map(row_to_entitlement_device_limit)
    .transpose()
}

async fn enforce_device_limit_for_lease(
    tx: &mut Transaction<'_, Postgres>,
    user_id: UserId,
) -> Result<(), ApiError> {
    let entitlement = find_active_entitlement_with_card_limit_for_user(tx, user_id)
        .await?
        .ok_or_else(|| ApiError::forbidden("no active entitlement"))?;
    let active_count = count_active_devices_for_user(tx, user_id).await?;
    if device_limit_allows_active_fleet(active_count, entitlement.max_devices) {
        Ok(())
    } else {
        Err(ApiError::forbidden(format!(
            "device limit exceeded for this entitlement ({}/{})",
            active_count, entitlement.max_devices
        )))
    }
}

async fn revoke_entitlement_by_id(
    tx: &mut Transaction<'_, Postgres>,
    entitlement_id: EntitlementId,
) -> Result<Option<Entitlement>, ApiError> {
    sqlx::query(
        r#"
        UPDATE entitlements
        SET revoked_at = $2
        WHERE id = $1
        RETURNING id, tenant_id, user_id, plan_code, source_card_key_id, features, expires_at, revoked_at
        "#,
    )
    .bind(entitlement_id.0)
    .bind(Utc::now())
    .fetch_optional(&mut **tx)
    .await
    .map_err(db_internal)?
    .map(row_to_entitlement)
    .transpose()
}

async fn insert_audit(
    tx: &mut Transaction<'_, Postgres>,
    event: &AuditEvent,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        INSERT INTO audit_events (id, tenant_id, actor, action, target, summary, created_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
    )
    .bind(event.id.0)
    .bind(event.tenant_id.map(|id| id.0))
    .bind(&event.actor)
    .bind(&event.action)
    .bind(&event.target)
    .bind(&event.summary)
    .bind(event.created_at)
    .execute(&mut **tx)
    .await
    .map_err(db_internal)?;
    Ok(())
}

async fn insert_audit_pool(db: &PgPool, event: &AuditEvent) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        INSERT INTO audit_events (id, tenant_id, actor, action, target, summary, created_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
    )
    .bind(event.id.0)
    .bind(event.tenant_id.map(|id| id.0))
    .bind(&event.actor)
    .bind(&event.action)
    .bind(&event.target)
    .bind(&event.summary)
    .bind(event.created_at)
    .execute(db)
    .await
    .map_err(db_internal)?;
    Ok(())
}

async fn list_audit_events(db: &PgPool) -> Result<Vec<AuditEvent>, ApiError> {
    let rows = sqlx::query(
        r#"
        SELECT id, tenant_id, actor, action, target, summary, created_at
        FROM audit_events
        ORDER BY created_at ASC
        "#,
    )
    .fetch_all(db)
    .await
    .map_err(db_internal)?;
    rows.into_iter().map(row_to_audit).collect()
}

fn row_to_user(row: PgRow) -> Result<User, ApiError> {
    let nebula_id: String = row.get("nebula_id");
    let email: Option<String> = row.get("email");
    let phone: Option<String> = row.get("phone");
    Ok(User {
        id: UserId(row.get("id")),
        tenant_id: TenantId(row.get("tenant_id")),
        nebula_id: NebulaId::parse(nebula_id)
            .map_err(|_| ApiError::internal("invalid nebula id in database"))?,
        nebula_source: nebula_source_from_db(row.get("nebula_source"))?,
        skybridge_user_id: row.get("skybridge_user_id"),
        email: email
            .map(Email::parse)
            .transpose()
            .map_err(|_| ApiError::internal("invalid email in database"))?,
        phone: phone
            .map(PhoneNumber::parse)
            .transpose()
            .map_err(|_| ApiError::internal("invalid phone number in database"))?,
        name: row.get("name"),
        password_hash: row.get("password_hash"),
        role: user_role_from_db(row.get("role"))?,
        email_verified_at: row.get("email_verified_at"),
        phone_verified_at: row.get("phone_verified_at"),
        created_at: row.get("created_at"),
    })
}

fn row_to_order(row: PgRow) -> Result<Order, ApiError> {
    Ok(Order {
        id: OrderId(row.get("id")),
        tenant_id: TenantId(row.get("tenant_id")),
        user_id: UserId(row.get("user_id")),
        plan_code: PlanCode(row.get("plan_code")),
        status: order_status_from_db(row.get("status"))?,
        payment_reference: row.get("payment_reference"),
        created_at: row.get("created_at"),
        confirmed_at: row.get("confirmed_at"),
    })
}

fn row_to_card_key(row: PgRow) -> Result<CardKey, ApiError> {
    Ok(CardKey {
        id: CardKeyId(row.get("id")),
        tenant_id: TenantId(row.get("tenant_id")),
        plan_code: PlanCode(row.get("plan_code")),
        code_hash: row.get("code_hash"),
        code_fingerprint: row.get("code_fingerprint"),
        duration_days: i32_to_u16(row.get("duration_days"), "duration_days")?,
        max_devices: i32_to_u8(row.get("max_devices"), "max_devices")?,
        status: card_key_status_from_db(row.get("status"))?,
        redeemed_by: row.get::<Option<Uuid>, _>("redeemed_by").map(UserId),
        redeemed_at: row.get("redeemed_at"),
        created_at: row.get("created_at"),
    })
}

fn row_to_device(row: PgRow) -> Result<Device, ApiError> {
    Ok(Device {
        id: DeviceId(row.get("id")),
        tenant_id: TenantId(row.get("tenant_id")),
        user_id: UserId(row.get("user_id")),
        name: row.get("name"),
        fingerprint_hash: row.get("fingerprint_hash"),
        status: device_status_from_db(row.get("status"))?,
        activated_at: row.get("activated_at"),
        last_seen_at: row.get("last_seen_at"),
    })
}

fn row_to_entitlement(row: PgRow) -> Result<Entitlement, ApiError> {
    let raw_features: Vec<String> = row.get("features");
    let features = raw_features
        .into_iter()
        .map(|feature| feature_from_db(&feature))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Entitlement {
        id: EntitlementId(row.get("id")),
        tenant_id: TenantId(row.get("tenant_id")),
        user_id: UserId(row.get("user_id")),
        plan_code: PlanCode(row.get("plan_code")),
        source_card_key_id: CardKeyId(row.get("source_card_key_id")),
        features,
        expires_at: row.get("expires_at"),
        revoked_at: row.get("revoked_at"),
    })
}

fn row_to_entitlement_device_limit(row: PgRow) -> Result<EntitlementDeviceLimit, ApiError> {
    Ok(EntitlementDeviceLimit {
        max_devices: i32_to_u8(row.get("max_devices"), "max_devices")?,
    })
}

fn row_to_audit(row: PgRow) -> Result<AuditEvent, ApiError> {
    Ok(AuditEvent {
        id: AuditEventId(row.get("id")),
        tenant_id: row.get::<Option<Uuid>, _>("tenant_id").map(TenantId),
        actor: row.get("actor"),
        action: row.get("action"),
        target: row.get("target"),
        summary: row.get("summary"),
        created_at: row.get("created_at"),
    })
}

fn i32_to_u16(value: i32, field: &'static str) -> Result<u16, ApiError> {
    u16::try_from(value).map_err(|_| ApiError::internal(format!("invalid {field} in database")))
}

fn i32_to_u8(value: i32, field: &'static str) -> Result<u8, ApiError> {
    u8::try_from(value).map_err(|_| ApiError::internal(format!("invalid {field} in database")))
}

fn require_user(state: &AppState, headers: &HeaderMap) -> Result<AuthClaims, ApiError> {
    let value = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| ApiError::unauthorized("missing bearer token"))?;
    let token = value
        .strip_prefix("Bearer ")
        .ok_or_else(|| ApiError::unauthorized("missing bearer token"))?;
    let data = decode::<AuthClaims>(
        token,
        &DecodingKey::from_secret(state.config.jwt_secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|_| ApiError::unauthorized("invalid bearer token"))?;
    Ok(data.claims)
}

fn require_admin(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let admin_header = headers
        .get("x-admin-token")
        .and_then(|value| value.to_str().ok());
    if admin_header.is_some_and(|token| constant_time_eq(token, &state.config.admin_token)) {
        return Ok(());
    }
    let claims = require_user(state, headers)?;
    if claims.role == UserRole::Admin {
        return Ok(());
    }
    Err(ApiError::forbidden("admin access required"))
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

fn issue_jwt(
    config: &AppConfig,
    user_id: UserId,
    tenant_id: TenantId,
    role: UserRole,
) -> Result<String, ApiError> {
    let claims = AuthClaims {
        sub: user_id,
        tenant_id,
        role,
        exp: (Utc::now() + Duration::hours(12)).timestamp() as usize,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
    )
    .map_err(|_| ApiError::internal("failed to issue token"))
}

fn hash_password(secret: &SecretString) -> Result<String, ApiError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(secret.expose_secret().as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| ApiError::internal("failed to hash secret"))
}

fn verify_password(hash: &str, secret: &SecretString) -> Result<(), ApiError> {
    let parsed =
        PasswordHash::new(hash).map_err(|_| ApiError::unauthorized("invalid credentials"))?;
    Argon2::default()
        .verify_password(secret.expose_secret().as_bytes(), &parsed)
        .map_err(|_| ApiError::unauthorized("invalid credentials"))
}

fn generate_card_key(
    plan_code: &PlanCode,
    tenant_id: TenantId,
    duration_days: u16,
    max_devices: u8,
) -> Result<GeneratedCardKey, ApiError> {
    let random: String = rand::rng()
        .sample_iter(&Alphanumeric)
        .take(24)
        .map(char::from)
        .collect();
    let plain_code = format!("ORS-{}-{}", &plan_code.0.to_uppercase(), random);
    let secret = SecretString::from(plain_code.clone());
    let card_key = CardKey {
        id: CardKeyId::new(),
        tenant_id,
        plan_code: plan_code.clone(),
        code_hash: hash_password(&secret)?,
        code_fingerprint: card_key_fingerprint(&plain_code),
        duration_days,
        max_devices,
        status: CardKeyStatus::Available,
        redeemed_by: None,
        redeemed_at: None,
        created_at: Utc::now(),
    };
    Ok(GeneratedCardKey {
        card_key,
        plain_code,
    })
}

fn validate_card_key_limits(duration_days: u16, max_devices: u8) -> Result<(), ApiError> {
    if duration_days == 0 {
        return Err(ApiError::bad_request(
            "duration_days must be greater than 0",
        ));
    }
    if max_devices == 0 {
        return Err(ApiError::bad_request("max_devices must be greater than 0"));
    }
    Ok(())
}

fn device_limit_allows_new_activation(active_count: i64, max_devices: u8) -> bool {
    active_count < i64::from(max_devices)
}

fn device_limit_allows_active_fleet(active_count: i64, max_devices: u8) -> bool {
    active_count <= i64::from(max_devices)
}

fn card_key_fingerprint(code: &str) -> String {
    format!("{:x}", Sha256::digest(code.as_bytes()))
}

fn hash_fingerprint(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

async fn create_unique_nebula_id(db: &PgPool) -> Result<NebulaId, ApiError> {
    for _ in 0..8 {
        let nebula_id = generate_nebula_id()?;
        if !nebula_id_exists(db, &nebula_id).await? {
            return Ok(nebula_id);
        }
    }
    Err(ApiError::internal("failed to allocate nebula id"))
}

fn generate_nebula_id() -> Result<NebulaId, ApiError> {
    let random: String = rand::rng()
        .sample_iter(&Alphanumeric)
        .take(12)
        .map(char::from)
        .map(|ch| ch.to_ascii_uppercase())
        .collect();
    NebulaId::parse(format!("NEBULA-{}-{}", Utc::now().format("%Y"), random))
        .map_err(|_| ApiError::internal("failed to generate nebula id"))
}

async fn resolve_skybridge_profile(
    config: &AppConfig,
    access_token: &SecretString,
) -> Result<SkybridgeProfile, ApiError> {
    if config.skybridge_api_base_urls.is_empty() {
        return Err(ApiError::service_unavailable(
            "SkyBridge identity provider is not configured",
        ));
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(12))
        .build()
        .map_err(|_| ApiError::internal("failed to create identity provider client"))?;
    let mut last_error: Option<ApiError> = None;
    for base_url in &config.skybridge_api_base_urls {
        match resolve_skybridge_profile_from_provider(&client, base_url, access_token).await {
            Ok(profile) => return Ok(profile),
            Err(error) => {
                if error.status != StatusCode::UNAUTHORIZED {
                    tracing::warn!(
                        provider = %redacted_identity_provider(base_url),
                        error = %error.message,
                        "SkyBridge identity provider failed"
                    );
                }
                last_error = Some(error);
            }
        }
    }
    Err(last_error.unwrap_or_else(|| {
        ApiError::service_unavailable("SkyBridge identity provider is not configured")
    }))
}

async fn resolve_skybridge_profile_from_provider(
    client: &reqwest::Client,
    base_url: &str,
    access_token: &SecretString,
) -> Result<SkybridgeProfile, ApiError> {
    let mut profile = fetch_skybridge_profile(&client, base_url, access_token).await?;
    if profile.nebula_id.is_none() {
        generate_skybridge_nebula_id(&client, base_url, access_token).await?;
        profile = fetch_skybridge_profile(&client, base_url, access_token).await?;
    }
    SkybridgeProfile::try_from_response(profile)
}

async fn fetch_skybridge_profile(
    client: &reqwest::Client,
    base_url: &str,
    access_token: &SecretString,
) -> Result<SkybridgeProfileResponse, ApiError> {
    let response = client
        .get(format!("{base_url}/get-user-profile"))
        .bearer_auth(access_token.expose_secret())
        .send()
        .await
        .map_err(|_| ApiError::bad_gateway("SkyBridge identity provider is unavailable"))?;
    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(ApiError::unauthorized("invalid SkyBridge session"));
    }
    if !response.status().is_success() {
        return Err(ApiError::bad_gateway("SkyBridge profile lookup failed"));
    }
    response
        .json::<SkybridgeDataResponse<SkybridgeProfileResponse>>()
        .await
        .map(|response| response.data)
        .map_err(|_| ApiError::bad_gateway("invalid SkyBridge profile response"))
}

async fn generate_skybridge_nebula_id(
    client: &reqwest::Client,
    base_url: &str,
    access_token: &SecretString,
) -> Result<(), ApiError> {
    let response = client
        .post(format!("{base_url}/generate-nebula-id"))
        .bearer_auth(access_token.expose_secret())
        .send()
        .await
        .map_err(|_| ApiError::bad_gateway("SkyBridge identity provider is unavailable"))?;
    if response.status() == reqwest::StatusCode::BAD_REQUEST {
        return Ok(());
    }
    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(ApiError::unauthorized("invalid SkyBridge session"));
    }
    if !response.status().is_success() {
        return Err(ApiError::bad_gateway(
            "SkyBridge nebula id generation failed",
        ));
    }
    Ok(())
}

fn redacted_identity_provider(base_url: &str) -> String {
    match reqwest::Url::parse(base_url) {
        Ok(url) => url
            .host_str()
            .map(|host| format!("{}://{}", url.scheme(), host))
            .unwrap_or_else(|| "configured-provider".to_string()),
        Err(_) => "configured-provider".to_string(),
    }
}

fn parse_skybridge_email(value: Option<String>) -> Result<Option<Email>, ApiError> {
    value
        .filter(|value| !value.trim().is_empty())
        .map(Email::parse)
        .transpose()
        .map_err(|_| ApiError::bad_gateway("invalid SkyBridge email"))
}

fn parse_skybridge_phone(value: Option<String>) -> Result<Option<PhoneNumber>, ApiError> {
    value
        .filter(|value| !value.trim().is_empty())
        .map(PhoneNumber::parse)
        .transpose()
        .map_err(|_| ApiError::bad_gateway("invalid SkyBridge phone"))
}

fn skybridge_password_sentinel() -> String {
    "skybridge-session-only".to_string()
}

fn ensure_local_password_login_allowed(user: &User) -> Result<(), ApiError> {
    if user.nebula_source == NebulaSource::LocalDev {
        return Ok(());
    }
    Err(ApiError::unauthorized("use SkyBridge identity"))
}

fn default_features() -> Vec<Feature> {
    vec![
        Feature::OzonRead,
        Feature::OzonWriteMock,
        Feature::DraftImport1688Mock,
        Feature::OpenClawBridge,
        Feature::LocalApproval,
    ]
}

fn user_role_to_db(role: UserRole) -> &'static str {
    match role {
        UserRole::User => "user",
        UserRole::Admin => "admin",
    }
}

fn user_role_from_db(value: &str) -> Result<UserRole, ApiError> {
    match value {
        "user" => Ok(UserRole::User),
        "admin" => Ok(UserRole::Admin),
        _ => Err(ApiError::internal("invalid user role in database")),
    }
}

fn nebula_source_to_db(source: NebulaSource) -> &'static str {
    match source {
        NebulaSource::Skybridge => "skybridge",
        NebulaSource::LocalDev => "local_dev",
    }
}

fn nebula_source_from_db(value: &str) -> Result<NebulaSource, ApiError> {
    match value {
        "skybridge" => Ok(NebulaSource::Skybridge),
        "local_dev" => Ok(NebulaSource::LocalDev),
        _ => Err(ApiError::internal("invalid nebula source in database")),
    }
}

fn order_status_to_db(status: OrderStatus) -> &'static str {
    match status {
        OrderStatus::PendingManualPayment => "pending_manual_payment",
        OrderStatus::Confirmed => "confirmed",
        OrderStatus::Cancelled => "cancelled",
    }
}

fn order_status_from_db(value: &str) -> Result<OrderStatus, ApiError> {
    match value {
        "pending_manual_payment" => Ok(OrderStatus::PendingManualPayment),
        "confirmed" => Ok(OrderStatus::Confirmed),
        "cancelled" => Ok(OrderStatus::Cancelled),
        _ => Err(ApiError::internal("invalid order status in database")),
    }
}

fn card_key_status_to_db(status: CardKeyStatus) -> &'static str {
    match status {
        CardKeyStatus::Available => "available",
        CardKeyStatus::Redeemed => "redeemed",
        CardKeyStatus::Revoked => "revoked",
    }
}

fn card_key_status_from_db(value: &str) -> Result<CardKeyStatus, ApiError> {
    match value {
        "available" => Ok(CardKeyStatus::Available),
        "redeemed" => Ok(CardKeyStatus::Redeemed),
        "revoked" => Ok(CardKeyStatus::Revoked),
        _ => Err(ApiError::internal("invalid card key status in database")),
    }
}

fn device_status_to_db(status: DeviceStatus) -> &'static str {
    match status {
        DeviceStatus::Active => "active",
        DeviceStatus::Revoked => "revoked",
    }
}

fn device_status_from_db(value: &str) -> Result<DeviceStatus, ApiError> {
    match value {
        "active" => Ok(DeviceStatus::Active),
        "revoked" => Ok(DeviceStatus::Revoked),
        _ => Err(ApiError::internal("invalid device status in database")),
    }
}

fn feature_to_db(feature: Feature) -> &'static str {
    match feature {
        Feature::OzonRead => "ozon_read",
        Feature::OzonWriteMock => "ozon_write_mock",
        Feature::DraftImport1688Mock => "draft_import1688_mock",
        Feature::OpenClawBridge => "open_claw_bridge",
        Feature::LocalApproval => "local_approval",
    }
}

fn feature_from_db(value: &str) -> Result<Feature, ApiError> {
    match value {
        "ozon_read" => Ok(Feature::OzonRead),
        "ozon_write_mock" => Ok(Feature::OzonWriteMock),
        "draft_import1688_mock" => Ok(Feature::DraftImport1688Mock),
        "open_claw_bridge" => Ok(Feature::OpenClawBridge),
        "local_approval" => Ok(Feature::LocalApproval),
        _ => Err(ApiError::internal("invalid feature in database")),
    }
}

fn audit(
    tenant_id: Option<TenantId>,
    actor: &str,
    action: &str,
    target: &str,
    summary: &str,
) -> AuditEvent {
    AuditEvent {
        id: AuditEventId::new(),
        tenant_id,
        actor: actor.to_string(),
        action: action.to_string(),
        target: target.to_string(),
        summary: summary.to_string(),
        created_at: Utc::now(),
    }
}

fn db_internal(error: sqlx::Error) -> ApiError {
    tracing::error!(error = %error, "database operation failed");
    ApiError::internal("database operation failed")
}

fn map_unique_conflict(error: sqlx::Error, constraint: &str, message: &'static str) -> ApiError {
    if let Some(db_error) = error.as_database_error() {
        if db_error.constraint() == Some(constraint) {
            return ApiError::conflict(message);
        }
    }
    db_internal(error)
}

fn map_identity_unique_conflict(error: sqlx::Error) -> ApiError {
    if let Some(db_error) = error.as_database_error() {
        return match db_error.constraint() {
            Some("uq_users_email") => ApiError::conflict("email already registered"),
            Some("uq_users_phone") => ApiError::conflict("phone already registered"),
            Some("uq_users_nebula_id") => ApiError::conflict("nebula id already allocated"),
            Some("uq_users_skybridge_user_id") => {
                ApiError::conflict("SkyBridge user already linked")
            }
            _ => db_internal(error),
        };
    }
    db_internal(error)
}

struct GeneratedCardKey {
    card_key: CardKey,
    plain_code: String,
}

#[derive(Debug)]
struct EntitlementDeviceLimit {
    max_devices: u8,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    service: &'static str,
    status: &'static str,
}

#[derive(Debug, Deserialize)]
struct RegisterRequest {
    email: Option<String>,
    phone: Option<String>,
    login_method: Option<LoginMethod>,
    identifier: Option<String>,
    password: String,
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LoginRequest {
    email: Option<String>,
    phone: Option<String>,
    nebula_id: Option<String>,
    login_method: Option<LoginMethod>,
    identifier: Option<String>,
    password: String,
}

#[derive(Debug, Deserialize)]
struct SkybridgeAuthRequest {
    access_token: SecretString,
}

#[derive(Debug)]
struct SkybridgeProfile {
    user_id: Uuid,
    nebula_id: NebulaId,
    email: Option<Email>,
    phone: Option<PhoneNumber>,
    display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SkybridgeDataResponse<T> {
    data: T,
}

#[derive(Debug, Deserialize)]
struct SkybridgeProfileResponse {
    id: Uuid,
    email: Option<String>,
    phone: Option<String>,
    nebula_id: Option<String>,
    full_name: Option<String>,
}

impl SkybridgeProfile {
    fn try_from_response(response: SkybridgeProfileResponse) -> Result<Self, ApiError> {
        let nebula_id = response
            .nebula_id
            .ok_or_else(|| ApiError::bad_gateway("SkyBridge profile has no nebula id"))
            .and_then(|value| {
                NebulaId::parse(value)
                    .map_err(|_| ApiError::bad_gateway("invalid SkyBridge nebula id"))
            })?;
        Ok(Self {
            user_id: response.id,
            nebula_id,
            email: parse_skybridge_email(response.email)?,
            phone: parse_skybridge_phone(response.phone)?,
            display_name: response.full_name.filter(|value| !value.trim().is_empty()),
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum LoginMethod {
    Email,
    Phone,
    Nebula,
}

#[derive(Debug)]
struct RegistrationIdentity {
    email: Option<Email>,
    phone: Option<PhoneNumber>,
}

impl RegistrationIdentity {
    fn from_request(input: &RegisterRequest) -> Result<Self, ApiError> {
        match requested_login_method(
            input.login_method,
            input.email.as_ref(),
            input.phone.as_ref(),
        ) {
            LoginMethod::Email => {
                let value = input
                    .identifier
                    .as_ref()
                    .or(input.email.as_ref())
                    .ok_or_else(|| ApiError::bad_request("email is required"))?;
                Ok(Self {
                    email: Some(
                        Email::parse(value).map_err(|_| ApiError::bad_request("invalid email"))?,
                    ),
                    phone: None,
                })
            }
            LoginMethod::Phone => {
                let value = input
                    .identifier
                    .as_ref()
                    .or(input.phone.as_ref())
                    .ok_or_else(|| ApiError::bad_request("phone is required"))?;
                Ok(Self {
                    email: None,
                    phone: Some(
                        PhoneNumber::parse(value)
                            .map_err(|_| ApiError::bad_request("invalid phone"))?,
                    ),
                })
            }
            LoginMethod::Nebula => Err(ApiError::bad_request(
                "nebula id is generated during registration",
            )),
        }
    }
}

#[derive(Debug)]
enum LoginIdentity {
    Email(Email),
    Phone(PhoneNumber),
    NebulaId(NebulaId),
}

impl LoginIdentity {
    fn from_request(input: &LoginRequest) -> Result<Self, ApiError> {
        if let Some(method) = input.login_method {
            let value = input
                .identifier
                .as_ref()
                .or(match method {
                    LoginMethod::Email => input.email.as_ref(),
                    LoginMethod::Phone => input.phone.as_ref(),
                    LoginMethod::Nebula => input.nebula_id.as_ref(),
                })
                .ok_or_else(|| ApiError::bad_request("login identifier is required"))?;
            return parse_login_identity(method, value);
        }
        if let Some(email) = input.email.as_ref() {
            return parse_login_identity(LoginMethod::Email, email);
        }
        if let Some(phone) = input.phone.as_ref() {
            return parse_login_identity(LoginMethod::Phone, phone);
        }
        if let Some(nebula_id) = input.nebula_id.as_ref().or(input.identifier.as_ref()) {
            return parse_login_identity(LoginMethod::Nebula, nebula_id);
        }
        Err(ApiError::bad_request("login identifier is required"))
    }
}

fn requested_login_method(
    method: Option<LoginMethod>,
    email: Option<&String>,
    phone: Option<&String>,
) -> LoginMethod {
    method.unwrap_or_else(|| {
        if phone.is_some() && email.is_none() {
            LoginMethod::Phone
        } else {
            LoginMethod::Email
        }
    })
}

fn parse_login_identity(method: LoginMethod, value: &str) -> Result<LoginIdentity, ApiError> {
    match method {
        LoginMethod::Email => Email::parse(value)
            .map(LoginIdentity::Email)
            .map_err(|_| ApiError::bad_request("invalid email")),
        LoginMethod::Phone => PhoneNumber::parse(value)
            .map(LoginIdentity::Phone)
            .map_err(|_| ApiError::bad_request("invalid phone")),
        LoginMethod::Nebula => NebulaId::parse(value)
            .map(LoginIdentity::NebulaId)
            .map_err(|_| ApiError::bad_request("invalid nebula id")),
    }
}

#[derive(Debug, Serialize)]
struct AuthResponse {
    token: String,
    user: UserResponse,
}

#[derive(Debug, Serialize)]
struct UserResponse {
    id: UserId,
    tenant_id: TenantId,
    nebula_id: String,
    nebula_source: NebulaSource,
    skybridge_user_id: Option<Uuid>,
    email: Option<String>,
    phone: Option<String>,
    name: Option<String>,
    role: UserRole,
    email_verified: bool,
    phone_verified: bool,
}

impl UserResponse {
    fn from_user(user: &User) -> Self {
        Self {
            id: user.id,
            tenant_id: user.tenant_id,
            nebula_id: user.nebula_id.as_str().to_string(),
            nebula_source: user.nebula_source,
            skybridge_user_id: user.skybridge_user_id,
            email: user.email.as_ref().map(|email| email.as_str().to_string()),
            phone: user.phone.as_ref().map(|phone| phone.as_str().to_string()),
            name: user.name.clone(),
            role: user.role,
            email_verified: user.email_verified_at.is_some(),
            phone_verified: user.phone_verified_at.is_some(),
        }
    }
}

#[derive(Debug, Serialize)]
struct MeResponse {
    user: UserResponse,
    entitlements: Vec<Entitlement>,
}

#[derive(Debug, Deserialize)]
struct CreateOrderRequest {
    plan_code: Option<String>,
}

#[derive(Debug, Serialize)]
struct OrderResponse {
    order: Order,
}

#[derive(Debug, Serialize)]
struct ConfirmOrderResponse {
    order: Order,
    card_key: String,
}

#[derive(Debug, Deserialize)]
struct CreateCardKeysRequest {
    count: u16,
    plan_code: Option<String>,
    duration_days: Option<u16>,
    max_devices: Option<u8>,
}

#[derive(Debug, Serialize)]
struct CreateCardKeysResponse {
    card_keys: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RedeemCardKeyRequest {
    card_key: String,
}

#[derive(Debug, Serialize)]
struct RedeemCardKeyResponse {
    entitlement: Entitlement,
}

#[derive(Debug, Deserialize)]
struct ActivateDeviceRequest {
    name: String,
    fingerprint: String,
}

#[derive(Debug, Serialize)]
struct DeviceResponse {
    device: Device,
}

#[derive(Debug, Deserialize)]
struct IssueLeaseRequest {
    device_id: Uuid,
}

#[derive(Debug, Serialize)]
struct LeaseResponse {
    lease: EntitlementLease,
}

#[derive(Debug, Deserialize)]
struct RevokeEntitlementRequest {
    entitlement_id: Uuid,
}

#[derive(Debug, Serialize)]
struct EntitlementResponse {
    entitlement: Entitlement,
}

#[derive(Debug, Serialize)]
struct DownloadsResponse {
    local_node: String,
    checksum: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct AuthClaims {
    sub: UserId,
    tenant_id: TenantId,
    role: UserRole,
    exp: usize,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: message.into(),
        }
    }

    fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }

    fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: message.into(),
        }
    }

    fn bad_gateway(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            message: message.into(),
        }
    }

    fn service_unavailable(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
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
    use std::net::SocketAddr;

    use secrecy::{ExposeSecret, SecretString};

    use super::*;

    #[test]
    fn password_hash_does_not_store_plaintext() {
        let password = SecretString::from("correct horse battery staple");
        let hash = hash_password(&password).unwrap();
        assert!(!hash.contains(password.expose_secret()));
        verify_password(&hash, &password).unwrap();
    }

    #[test]
    fn card_key_fingerprint_is_stable_and_full_sha256() {
        let a = card_key_fingerprint("ORS-KEY");
        let b = card_key_fingerprint("ORS-KEY");
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn registration_identity_accepts_legacy_email_contract() {
        let request = RegisterRequest {
            email: Some(" USER@Example.COM ".to_string()),
            phone: None,
            login_method: None,
            identifier: None,
            password: "password-123".to_string(),
            name: None,
        };
        let identity = RegistrationIdentity::from_request(&request).unwrap();
        assert_eq!(identity.email.unwrap().as_str(), "user@example.com");
        assert!(identity.phone.is_none());
    }

    #[test]
    fn registration_identity_accepts_phone_contract() {
        let request = RegisterRequest {
            email: None,
            phone: None,
            login_method: Some(LoginMethod::Phone),
            identifier: Some(" +86 138-0013-8000 ".to_string()),
            password: "password-123".to_string(),
            name: None,
        };
        let identity = RegistrationIdentity::from_request(&request).unwrap();
        assert!(identity.email.is_none());
        assert_eq!(identity.phone.unwrap().as_str(), "+8613800138000");
    }

    #[test]
    fn login_identity_accepts_nebula_id() {
        let request = LoginRequest {
            email: None,
            phone: None,
            nebula_id: Some("nebula-2026-a1b2c3d4e5f6".to_string()),
            login_method: None,
            identifier: None,
            password: "password-123".to_string(),
        };
        let identity = LoginIdentity::from_request(&request).unwrap();
        match identity {
            LoginIdentity::NebulaId(nebula_id) => {
                assert_eq!(nebula_id.as_str(), "NEBULA-2026-A1B2C3D4E5F6");
            }
            _ => panic!("expected nebula identity"),
        }
    }

    #[test]
    fn db_enum_values_match_json_contract() {
        assert_eq!(user_role_to_db(UserRole::User), "user");
        assert_eq!(nebula_source_to_db(NebulaSource::Skybridge), "skybridge");
        assert_eq!(nebula_source_to_db(NebulaSource::LocalDev), "local_dev");
        assert_eq!(
            nebula_source_from_db("skybridge").unwrap(),
            NebulaSource::Skybridge
        );
        assert_eq!(
            order_status_to_db(OrderStatus::PendingManualPayment),
            "pending_manual_payment"
        );
        assert_eq!(feature_to_db(Feature::OpenClawBridge), "open_claw_bridge");
    }

    #[test]
    fn skybridge_profile_requires_canonical_nebula_id() {
        let response = SkybridgeProfileResponse {
            id: Uuid::new_v4(),
            email: Some("USER@example.COM".to_string()),
            phone: None,
            nebula_id: Some("nebula-2026-a1b2c3d4e5f6".to_string()),
            full_name: Some("Operator".to_string()),
        };
        let profile = SkybridgeProfile::try_from_response(response).unwrap();
        assert_eq!(profile.nebula_id.as_str(), "NEBULA-2026-A1B2C3D4E5F6");
        assert_eq!(profile.email.unwrap().as_str(), "user@example.com");
    }

    #[test]
    fn skybridge_profile_rejects_missing_nebula_id() {
        let response = SkybridgeProfileResponse {
            id: Uuid::new_v4(),
            email: Some("user@example.com".to_string()),
            phone: None,
            nebula_id: None,
            full_name: None,
        };
        assert!(SkybridgeProfile::try_from_response(response).is_err());
    }

    #[test]
    fn skybridge_projection_cannot_use_local_password_login() {
        let user = User {
            id: UserId::new(),
            tenant_id: TenantId::new(),
            nebula_id: NebulaId::parse("NEBULA-2026-A1B2C3D4E5F6").unwrap(),
            nebula_source: NebulaSource::Skybridge,
            skybridge_user_id: Some(Uuid::new_v4()),
            email: Some(Email::parse("user@example.com").unwrap()),
            phone: None,
            name: None,
            password_hash: skybridge_password_sentinel(),
            role: UserRole::User,
            email_verified_at: Some(Utc::now()),
            phone_verified_at: None,
            created_at: Utc::now(),
        };
        assert!(ensure_local_password_login_allowed(&user).is_err());
    }

    #[test]
    fn device_limit_blocks_new_activation_at_capacity() {
        assert!(device_limit_allows_new_activation(0, 1));
        assert!(!device_limit_allows_new_activation(1, 1));
        assert!(device_limit_allows_new_activation(1, 2));
    }

    #[test]
    fn device_limit_blocks_over_capacity_lease() {
        assert!(device_limit_allows_active_fleet(1, 1));
        assert!(!device_limit_allows_active_fleet(2, 1));
    }

    #[test]
    fn card_key_limits_reject_zero_values() {
        assert!(validate_card_key_limits(30, 1).is_ok());
        assert!(validate_card_key_limits(0, 1).is_err());
        assert!(validate_card_key_limits(30, 0).is_err());
    }

    fn config_for_security_tests() -> AppConfig {
        AppConfig {
            bind: "127.0.0.1:8080".to_string(),
            database_url: "postgres://ozon:ozon@127.0.0.1:5432/ozon_rust_suite".to_string(),
            database_max_connections: 1,
            environment: "development".to_string(),
            jwt_secret: DEFAULT_DEV_JWT_SECRET.to_string(),
            admin_token: DEFAULT_DEV_ADMIN_TOKEN.to_string(),
            download_url: "https://downloads.example.com/ozon-local-node.msi".to_string(),
            skybridge_api_base_urls: vec![],
            allow_local_nebula_registration: false,
            cors_allowed_origins: DEV_CORS_ORIGINS
                .iter()
                .map(|origin| (*origin).to_string())
                .collect(),
            cors_allowed_origins_configured: false,
        }
    }

    #[test]
    fn production_config_rejects_dev_secrets() {
        let mut config = config_for_security_tests();
        config.environment = "production".to_string();
        let bind: SocketAddr = "127.0.0.1:8080".parse().unwrap();

        assert!(config.validate(bind).is_err());
    }

    #[test]
    fn non_loopback_config_rejects_missing_cors_env() {
        let mut config = config_for_security_tests();
        config.jwt_secret = "01234567890123456789012345678901".to_string();
        config.admin_token = "012345678901234567890123".to_string();
        let bind: SocketAddr = "0.0.0.0:8080".parse().unwrap();

        assert!(config.validate(bind).is_err());
    }

    #[test]
    fn constant_time_comparison_checks_full_secret() {
        assert!(constant_time_eq("same-secret", "same-secret"));
        assert!(!constant_time_eq("same-secret", "same-secret-extra"));
        assert!(!constant_time_eq("same-secret", "same-secreu"));
    }
}
