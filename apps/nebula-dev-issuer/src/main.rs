use std::{
    collections::HashMap,
    env,
    net::SocketAddr,
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};

use axum::{
    Form, Json, Router,
    extract::{Query, State},
    http::{HeaderMap, Method, StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use ozon_domain::NebulaId;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use url::Url;
use uuid::Uuid;

const DEFAULT_CLIENT_ID: &str = "ozon_rust_suite_portal";
const DEFAULT_REDIRECT_URI: &str = "http://127.0.0.1:5171/auth/callback";
const DEFAULT_DEMO_EMAIL: &str = "demo@nebula.local";
const DEFAULT_DEMO_NAME: &str = "Nebula Dev User";
const DEFAULT_NEUBLA_ID: &str = "NEBULA-2026-OZONLOCAL0001";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ozon_nebula_dev_issuer=info,tower_http=info,axum=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = AppConfig::from_env()?;
    let bind: SocketAddr = config.bind.parse()?;
    if !bind.ip().is_loopback() {
        anyhow::bail!("nebula-dev-issuer refuses to bind non-loopback address: {bind}");
    }

    let app = app_router(AppState::new(config.clone()));
    tracing::info!(%bind, issuer = %config.issuer, "starting Nebula dev issuer");
    let listener = tokio::net::TcpListener::bind(bind).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

fn app_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route(
            "/.well-known/openid-configuration",
            get(openid_configuration),
        )
        .route(
            "/oauth/authorize",
            get(authorize_page).post(authorize_submit),
        )
        .route("/dev/authorize", post(dev_authorize))
        .route("/oauth/token", post(token))
        .route("/oauth/userinfo", get(userinfo))
        .route("/oauth/revoke", post(revoke))
        .route("/get-user-profile", get(get_user_profile))
        .route("/generate-nebula-id", post(generate_nebula_id))
        .layer(local_cors())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

fn local_cors() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(Any)
}

#[derive(Clone)]
struct AppConfig {
    bind: String,
    issuer: String,
    access_token_ttl: Duration,
    refresh_token_ttl: Duration,
    allow_headless_authorize: bool,
    clients: HashMap<String, PublicClient>,
    demo_user: DemoUser,
}

impl AppConfig {
    fn from_env() -> anyhow::Result<Self> {
        let bind = env::var("NEBULA_DEV_BIND").unwrap_or_else(|_| "127.0.0.1:8788".to_string());
        let issuer = env::var("NEBULA_DEV_ISSUER")
            .unwrap_or_else(|_| format!("http://{bind}"))
            .trim_end_matches('/')
            .to_string();
        let access_token_ttl = Duration::from_secs(env_u64("NEBULA_DEV_ACCESS_TOKEN_TTL_SEC", 900));
        let refresh_token_ttl = Duration::from_secs(env_u64(
            "NEBULA_DEV_REFRESH_TOKEN_TTL_SEC",
            60 * 60 * 24 * 30,
        ));
        let allow_headless_authorize = env_bool("NEBULA_DEV_ALLOW_HEADLESS_AUTHORIZE", true);
        let clients = parse_clients()?;
        let demo_user = DemoUser::from_env()?;

        Ok(Self {
            bind,
            issuer,
            access_token_ttl,
            refresh_token_ttl,
            allow_headless_authorize,
            clients,
            demo_user,
        })
    }
}

#[derive(Clone)]
struct PublicClient {
    redirect_uris: Vec<String>,
    scopes: Vec<String>,
}

#[derive(Clone)]
struct DemoUser {
    id: Uuid,
    username: String,
    password: String,
    name: String,
    email: String,
    phone: Option<String>,
    nebula_id: NebulaId,
}

impl DemoUser {
    fn from_env() -> anyhow::Result<Self> {
        let id = env::var("NEBULA_DEV_USER_ID")
            .ok()
            .and_then(|value| Uuid::parse_str(&value).ok())
            .unwrap_or_else(|| Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap());
        let nebula_id = NebulaId::parse(
            env::var("NEBULA_DEV_NEBULA_ID").unwrap_or_else(|_| DEFAULT_NEUBLA_ID.to_string()),
        )
        .map_err(|_| anyhow::anyhow!("NEBULA_DEV_NEBULA_ID is not a valid Nebula ID"))?;

        Ok(Self {
            id,
            username: env::var("NEBULA_DEV_USERNAME").unwrap_or_else(|_| "demo".to_string()),
            password: env::var("NEBULA_DEV_PASSWORD").unwrap_or_else(|_| "demo-pass".to_string()),
            name: env::var("NEBULA_DEV_DISPLAY_NAME")
                .unwrap_or_else(|_| DEFAULT_DEMO_NAME.to_string()),
            email: env::var("NEBULA_DEV_EMAIL").unwrap_or_else(|_| DEFAULT_DEMO_EMAIL.to_string()),
            phone: env::var("NEBULA_DEV_PHONE")
                .ok()
                .filter(|value| !value.trim().is_empty()),
            nebula_id,
        })
    }
}

#[derive(Clone)]
struct AppState {
    config: AppConfig,
    store: SharedStore,
}

impl AppState {
    fn new(config: AppConfig) -> Self {
        Self {
            config,
            store: Arc::new(RwLock::new(Store::default())),
        }
    }
}

type SharedStore = Arc<RwLock<Store>>;

#[derive(Default)]
struct Store {
    authorize_requests: HashMap<String, AuthorizeRequest>,
    authorization_codes: HashMap<String, AuthorizationCode>,
    access_tokens: HashMap<String, TokenRecord>,
    refresh_tokens: HashMap<String, RefreshTokenRecord>,
}

#[derive(Clone)]
struct AuthorizeRequest {
    client_id: String,
    redirect_uri: String,
    scope: String,
    state: String,
    code_challenge: String,
    expires_at: Instant,
}

#[derive(Clone)]
struct AuthorizationCode {
    client_id: String,
    redirect_uri: String,
    scope: String,
    code_challenge: String,
    user: DemoUser,
    expires_at: Instant,
    used: bool,
}

#[derive(Clone)]
struct TokenRecord {
    user: DemoUser,
    expires_at: Instant,
}

#[derive(Clone)]
struct RefreshTokenRecord {
    user: DemoUser,
    client_id: String,
    scope: String,
    expires_at: Instant,
    revoked: bool,
}

async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        service: "ozon-nebula-dev-issuer",
        status: "ok",
        mode: "public_client_pkce_dev",
        issuer: state.config.issuer,
        headless_authorize_enabled: state.config.allow_headless_authorize,
        clients: state.config.clients.keys().cloned().collect(),
    })
}

async fn openid_configuration(State(state): State<AppState>) -> Json<OpenIdConfiguration> {
    let issuer = state.config.issuer;
    Json(OpenIdConfiguration {
        issuer: issuer.clone(),
        authorization_endpoint: format!("{issuer}/oauth/authorize"),
        token_endpoint: format!("{issuer}/oauth/token"),
        userinfo_endpoint: format!("{issuer}/oauth/userinfo"),
        revocation_endpoint: format!("{issuer}/oauth/revoke"),
        response_types_supported: vec!["code"],
        grant_types_supported: vec!["authorization_code", "refresh_token"],
        token_endpoint_auth_methods_supported: vec!["none"],
        code_challenge_methods_supported: vec!["S256"],
        scopes_supported: vec!["openid", "profile", "email", "offline_access"],
    })
}

async fn authorize_page(
    State(state): State<AppState>,
    Query(input): Query<AuthorizeQuery>,
) -> Response {
    purge_expired(&state.store);
    let request = match validate_authorize_request(&state.config, input) {
        Ok(request) => request,
        Err(error) => return oauth_error(StatusCode::BAD_REQUEST, error),
    };
    let request_id = random_token(16);
    {
        let mut store = state.store.write().expect("store poisoned");
        store
            .authorize_requests
            .insert(request_id.clone(), request.clone());
    }
    Html(render_authorize_page(
        &request_id,
        &request,
        &state.config.demo_user,
        None,
    ))
    .into_response()
}

async fn authorize_submit(
    State(state): State<AppState>,
    Form(input): Form<AuthorizeForm>,
) -> Response {
    purge_expired(&state.store);
    let request = {
        let store = state.store.read().expect("store poisoned");
        store.authorize_requests.get(&input.request_id).cloned()
    };
    let Some(request) = request else {
        return Html(render_authorize_page(
            "expired",
            &AuthorizeRequest::unknown(),
            &state.config.demo_user,
            Some("Authorization request expired. Please restart login."),
        ))
        .into_response();
    };

    if input.username.trim() != state.config.demo_user.username
        || input.password != state.config.demo_user.password
    {
        return (
            StatusCode::UNAUTHORIZED,
            Html(render_authorize_page(
                &input.request_id,
                &request,
                &state.config.demo_user,
                Some("Invalid username or password."),
            )),
        )
            .into_response();
    }

    let code = issue_authorization_code(&state, &request);
    {
        let mut store = state.store.write().expect("store poisoned");
        store.authorize_requests.remove(&input.request_id);
    }
    redirect_with_code(&request, &code)
}

async fn dev_authorize(
    State(state): State<AppState>,
    Json(input): Json<DevAuthorizeRequest>,
) -> Response {
    if !state.config.allow_headless_authorize {
        return oauth_error(StatusCode::NOT_FOUND, "not_found");
    }

    purge_expired(&state.store);
    let request = match validate_authorize_request(&state.config, input.clone().into()) {
        Ok(request) => request,
        Err(error) => return oauth_error(StatusCode::BAD_REQUEST, error),
    };
    if input.username.trim() != state.config.demo_user.username
        || input.password != state.config.demo_user.password
    {
        return oauth_error(StatusCode::UNAUTHORIZED, "invalid_credentials");
    }

    let code = issue_authorization_code(&state, &request);
    let redirect_to = make_redirect_url(&request, &code);
    Json(DevAuthorizeResponse {
        code,
        state: request.state,
        redirect_to,
    })
    .into_response()
}

async fn token(State(state): State<AppState>, Form(input): Form<TokenRequest>) -> Response {
    purge_expired(&state.store);
    if !state.config.clients.contains_key(input.client_id.trim()) {
        return oauth_error(StatusCode::UNAUTHORIZED, "unauthorized_client");
    }

    match input.grant_type.as_str() {
        "authorization_code" => exchange_authorization_code(&state, input),
        "refresh_token" => refresh_access_token(&state, input),
        _ => oauth_error(StatusCode::BAD_REQUEST, "unsupported_grant_type"),
    }
}

async fn userinfo(State(state): State<AppState>, headers: HeaderMap) -> Response {
    purge_expired(&state.store);
    let Some(record) = access_token_record(&state.store, &headers) else {
        return oauth_error(StatusCode::UNAUTHORIZED, "invalid_token");
    };
    Json(UserInfoResponse::from_user(&record.user)).into_response()
}

async fn revoke(
    State(state): State<AppState>,
    Form(input): Form<RevokeRequest>,
) -> Json<RevokeResponse> {
    let mut store = state.store.write().expect("store poisoned");
    store.access_tokens.remove(input.token.trim());
    if let Some(record) = store.refresh_tokens.get_mut(input.token.trim()) {
        record.revoked = true;
    }
    Json(RevokeResponse { revoked: true })
}

async fn get_user_profile(State(state): State<AppState>, headers: HeaderMap) -> Response {
    purge_expired(&state.store);
    let Some(record) = access_token_record(&state.store, &headers) else {
        return oauth_error(StatusCode::UNAUTHORIZED, "invalid_token");
    };
    Json(SkybridgeDataResponse {
        data: SkybridgeProfileResponse::from_user(&record.user),
    })
    .into_response()
}

async fn generate_nebula_id(State(state): State<AppState>, headers: HeaderMap) -> Response {
    purge_expired(&state.store);
    if access_token_record(&state.store, &headers).is_none() {
        return oauth_error(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    Json(SkybridgeDataResponse {
        data: serde_json::json!({ "nebula_id": state.config.demo_user.nebula_id.as_str() }),
    })
    .into_response()
}

fn exchange_authorization_code(state: &AppState, input: TokenRequest) -> Response {
    let code = input.code.trim();
    let mut store = state.store.write().expect("store poisoned");
    let Some(record) = store.authorization_codes.get_mut(code) else {
        return oauth_error(StatusCode::BAD_REQUEST, "invalid_grant");
    };
    if record.used || record.expires_at <= Instant::now() {
        return oauth_error(StatusCode::BAD_REQUEST, "invalid_grant");
    }
    if record.client_id != input.client_id || record.redirect_uri != input.redirect_uri {
        return oauth_error(StatusCode::BAD_REQUEST, "invalid_grant");
    }
    if input.code_verifier.trim().is_empty()
        || code_challenge(input.code_verifier.trim()) != record.code_challenge
    {
        return oauth_error(StatusCode::BAD_REQUEST, "invalid_code_verifier");
    }

    record.used = true;
    let user = record.user.clone();
    let client_id = record.client_id.clone();
    let scope = record.scope.clone();
    drop(store);
    Json(issue_token_payload(state, user, client_id, scope)).into_response()
}

fn refresh_access_token(state: &AppState, input: TokenRequest) -> Response {
    let refresh_token = input.refresh_token.trim();
    let mut store = state.store.write().expect("store poisoned");
    let Some(record) = store.refresh_tokens.get_mut(refresh_token) else {
        return oauth_error(StatusCode::BAD_REQUEST, "invalid_grant");
    };
    if record.revoked || record.expires_at <= Instant::now() || record.client_id != input.client_id
    {
        return oauth_error(StatusCode::BAD_REQUEST, "invalid_grant");
    }

    record.revoked = true;
    let user = record.user.clone();
    let client_id = record.client_id.clone();
    let scope = record.scope.clone();
    drop(store);
    Json(issue_token_payload(state, user, client_id, scope)).into_response()
}

fn issue_token_payload(
    state: &AppState,
    user: DemoUser,
    client_id: String,
    scope: String,
) -> TokenResponse {
    let access_token = random_token(32);
    let wants_refresh = scope
        .split_whitespace()
        .any(|item| item == "offline_access");
    let refresh_token = wants_refresh.then(|| random_token(32));
    let mut store = state.store.write().expect("store poisoned");
    store.access_tokens.insert(
        access_token.clone(),
        TokenRecord {
            user: user.clone(),
            expires_at: Instant::now() + state.config.access_token_ttl,
        },
    );
    if let Some(refresh_token) = refresh_token.as_ref() {
        store.refresh_tokens.insert(
            refresh_token.clone(),
            RefreshTokenRecord {
                user,
                client_id,
                scope: scope.clone(),
                expires_at: Instant::now() + state.config.refresh_token_ttl,
                revoked: false,
            },
        );
    }
    TokenResponse {
        access_token,
        token_type: "Bearer",
        expires_in: state.config.access_token_ttl.as_secs(),
        refresh_token,
        scope,
    }
}

fn issue_authorization_code(state: &AppState, request: &AuthorizeRequest) -> String {
    let code = random_token(24);
    let mut store = state.store.write().expect("store poisoned");
    store.authorization_codes.insert(
        code.clone(),
        AuthorizationCode {
            client_id: request.client_id.clone(),
            redirect_uri: request.redirect_uri.clone(),
            scope: request.scope.clone(),
            code_challenge: request.code_challenge.clone(),
            user: state.config.demo_user.clone(),
            expires_at: Instant::now() + Duration::from_secs(5 * 60),
            used: false,
        },
    );
    code
}

fn validate_authorize_request(
    config: &AppConfig,
    input: AuthorizeQuery,
) -> Result<AuthorizeRequest, &'static str> {
    if input.response_type.as_deref() != Some("code") {
        return Err("unsupported_response_type");
    }
    let client_id = input.client_id.unwrap_or_default();
    if client_id.trim().is_empty() {
        return Err("invalid_client");
    }
    let redirect_uri = input.redirect_uri.unwrap_or_default();
    if redirect_uri.trim().is_empty() {
        return Err("invalid_redirect_uri");
    }
    if input.state.as_deref().unwrap_or_default().trim().is_empty() {
        return Err("invalid_state");
    }
    let code_challenge = input.code_challenge.unwrap_or_default();
    if code_challenge.trim().is_empty() {
        return Err("invalid_code_challenge");
    }
    if input
        .code_challenge_method
        .as_deref()
        .map(str::to_ascii_uppercase)
        .as_deref()
        != Some("S256")
    {
        return Err("invalid_code_challenge_method");
    }
    let Some(client) = config.clients.get(client_id.trim()) else {
        return Err("unauthorized_client");
    };
    if !client
        .redirect_uris
        .iter()
        .any(|uri| uri == redirect_uri.trim())
    {
        return Err("redirect_uri_mismatch");
    }

    let scope = normalize_scope(
        input.scope.as_deref().unwrap_or("openid profile email"),
        &client.scopes,
    )?;
    Ok(AuthorizeRequest {
        client_id: client_id.trim().to_string(),
        redirect_uri: redirect_uri.trim().to_string(),
        scope,
        state: input.state.unwrap_or_default().trim().to_string(),
        code_challenge: code_challenge.trim().to_string(),
        expires_at: Instant::now() + Duration::from_secs(10 * 60),
    })
}

fn normalize_scope(scope: &str, allowed: &[String]) -> Result<String, &'static str> {
    let requested: Vec<_> = scope
        .split_whitespace()
        .filter(|item| !item.is_empty())
        .collect();
    if requested.is_empty() {
        return Err("invalid_scope");
    }
    if requested
        .iter()
        .any(|item| !allowed.iter().any(|allowed| allowed == item))
    {
        return Err("invalid_scope");
    }
    Ok(requested.join(" "))
}

fn access_token_record(store: &SharedStore, headers: &HeaderMap) -> Option<TokenRecord> {
    let token = bearer_token(headers)?;
    let store = store.read().ok()?;
    store
        .access_tokens
        .get(token)
        .filter(|record| record.expires_at > Instant::now())
        .cloned()
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|token| !token.is_empty())
}

fn redirect_with_code(request: &AuthorizeRequest, code: &str) -> Response {
    Redirect::to(&make_redirect_url(request, code)).into_response()
}

fn make_redirect_url(request: &AuthorizeRequest, code: &str) -> String {
    let mut redirect_url = Url::parse(&request.redirect_uri).expect("validated redirect URI");
    redirect_url.query_pairs_mut().append_pair("code", code);
    redirect_url
        .query_pairs_mut()
        .append_pair("state", &request.state);
    redirect_url.to_string()
}

fn purge_expired(store: &SharedStore) {
    let Ok(mut store) = store.write() else {
        return;
    };
    let now = Instant::now();
    store
        .authorize_requests
        .retain(|_, value| value.expires_at > now);
    store
        .authorization_codes
        .retain(|_, value| value.expires_at > now && !value.used);
    store
        .access_tokens
        .retain(|_, value| value.expires_at > now);
    store
        .refresh_tokens
        .retain(|_, value| value.expires_at > now && !value.revoked);
}

fn random_token(bytes: usize) -> String {
    let mut raw = vec![0; bytes];
    rand::rng().fill_bytes(&mut raw);
    URL_SAFE_NO_PAD.encode(raw)
}

fn code_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn oauth_error(status: StatusCode, error: &'static str) -> Response {
    (status, Json(ErrorResponse { error })).into_response()
}

fn render_authorize_page(
    request_id: &str,
    request: &AuthorizeRequest,
    demo_user: &DemoUser,
    error_message: Option<&str>,
) -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Nebula Dev Sign In</title>
    <style>
      * {{ box-sizing: border-box; }}
      body {{ margin: 0; min-height: 100vh; font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; background: #f6f8fb; color: #111827; display: grid; place-items: center; padding: 24px; }}
      .card {{ width: min(440px, 100%); background: #ffffff; border: 1px solid #d9e0ea; border-radius: 8px; padding: 24px; box-shadow: 0 20px 60px rgba(15, 23, 42, 0.12); }}
      h1 {{ margin: 0 0 8px; font-size: 24px; letter-spacing: 0; }}
      p {{ margin: 0 0 20px; color: #5b6472; line-height: 1.5; }}
      label {{ display: block; margin: 14px 0 6px; font-size: 13px; font-weight: 700; color: #374151; }}
      input {{ width: 100%; border: 1px solid #cbd5e1; border-radius: 6px; padding: 11px 12px; font: inherit; }}
      button {{ width: 100%; margin-top: 18px; border: none; border-radius: 6px; padding: 12px; font-weight: 800; cursor: pointer; color: #ffffff; background: #2563eb; }}
      .meta {{ margin-top: 18px; padding-top: 16px; border-top: 1px solid #e5e7eb; font-size: 12px; color: #64748b; line-height: 1.6; }}
      .error {{ margin: 12px 0 0; color: #b91c1c; font-size: 13px; }}
      code {{ color: #1d4ed8; overflow-wrap: anywhere; }}
    </style>
  </head>
  <body>
    <main class="card">
      <h1>Nebula Dev Sign In</h1>
      <p>Local public-client OAuth 2.1 + PKCE issuer for Ozon Rust Suite development.</p>
      <form method="post" action="/oauth/authorize">
        <input type="hidden" name="request_id" value="{request_id}" />
        <label for="username">Username</label>
        <input id="username" name="username" type="text" value="{username}" autocomplete="username" />
        <label for="password">Password</label>
        <input id="password" name="password" type="password" value="{password}" autocomplete="current-password" />
        <button type="submit">Authorize</button>
      </form>
      {error}
      <div class="meta">
        Client: <code>{client_id}</code><br />
        Redirect URI: <code>{redirect_uri}</code><br />
        Scope: <code>{scope}</code>
      </div>
    </main>
  </body>
</html>"#,
        request_id = html_escape(request_id),
        username = html_escape(&demo_user.username),
        password = html_escape(&demo_user.password),
        client_id = html_escape(&request.client_id),
        redirect_uri = html_escape(&request.redirect_uri),
        scope = html_escape(&request.scope),
        error = error_message
            .map(|message| format!(r#"<div class="error">{}</div>"#, html_escape(message)))
            .unwrap_or_default(),
    )
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn parse_clients() -> anyhow::Result<HashMap<String, PublicClient>> {
    if let Ok(raw) = env::var("NEBULA_DEV_PUBLIC_CLIENTS_JSON") {
        let parsed: HashMap<String, PublicClientEnv> = serde_json::from_str(&raw)?;
        return Ok(parsed
            .into_iter()
            .map(|(client_id, client)| {
                (
                    client_id,
                    PublicClient {
                        redirect_uris: client.redirect_uris,
                        scopes: client.scopes,
                    },
                )
            })
            .collect());
    }

    let client_id =
        env::var("NEBULA_DEV_CLIENT_ID").unwrap_or_else(|_| DEFAULT_CLIENT_ID.to_string());
    let redirect_uri =
        env::var("NEBULA_DEV_REDIRECT_URI").unwrap_or_else(|_| DEFAULT_REDIRECT_URI.to_string());
    Ok(HashMap::from([(
        client_id,
        PublicClient {
            redirect_uris: split_env_list("NEBULA_DEV_REDIRECT_URIS").unwrap_or_else(|| {
                vec![
                    redirect_uri,
                    "http://localhost:5171/auth/callback".to_string(),
                    "skybridge://auth/nebula".to_string(),
                ]
            }),
            scopes: split_env_list("NEBULA_DEV_SCOPES").unwrap_or_else(|| {
                vec![
                    "openid".to_string(),
                    "profile".to_string(),
                    "email".to_string(),
                    "offline_access".to_string(),
                ]
            }),
        },
    )]))
}

fn split_env_list(name: &str) -> Option<Vec<String>> {
    env::var(name).ok().map(|value| {
        value
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    })
}

fn env_u64(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn env_bool(name: &str, default: bool) -> bool {
    env::var(name)
        .ok()
        .map(|value| !matches!(value.to_ascii_lowercase().as_str(), "0" | "false" | "no"))
        .unwrap_or(default)
}

impl AuthorizeRequest {
    fn unknown() -> Self {
        Self {
            client_id: "unknown".to_string(),
            redirect_uri: "unknown".to_string(),
            scope: "unknown".to_string(),
            state: "unknown".to_string(),
            code_challenge: "unknown".to_string(),
            expires_at: Instant::now(),
        }
    }
}

impl From<DevAuthorizeRequest> for AuthorizeQuery {
    fn from(value: DevAuthorizeRequest) -> Self {
        Self {
            response_type: value.response_type,
            client_id: value.client_id,
            redirect_uri: value.redirect_uri,
            scope: value.scope,
            state: value.state,
            code_challenge: value.code_challenge,
            code_challenge_method: value.code_challenge_method,
            flow: value.flow,
        }
    }
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    service: &'static str,
    status: &'static str,
    mode: &'static str,
    issuer: String,
    headless_authorize_enabled: bool,
    clients: Vec<String>,
}

#[derive(Debug, Serialize)]
struct OpenIdConfiguration {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    userinfo_endpoint: String,
    revocation_endpoint: String,
    response_types_supported: Vec<&'static str>,
    grant_types_supported: Vec<&'static str>,
    token_endpoint_auth_methods_supported: Vec<&'static str>,
    code_challenge_methods_supported: Vec<&'static str>,
    scopes_supported: Vec<&'static str>,
}

#[derive(Clone, Debug, Deserialize)]
struct AuthorizeQuery {
    response_type: Option<String>,
    client_id: Option<String>,
    redirect_uri: Option<String>,
    scope: Option<String>,
    state: Option<String>,
    code_challenge: Option<String>,
    code_challenge_method: Option<String>,
    #[allow(dead_code)]
    flow: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AuthorizeForm {
    request_id: String,
    username: String,
    password: String,
}

#[derive(Clone, Debug, Deserialize)]
struct DevAuthorizeRequest {
    response_type: Option<String>,
    client_id: Option<String>,
    redirect_uri: Option<String>,
    scope: Option<String>,
    state: Option<String>,
    code_challenge: Option<String>,
    code_challenge_method: Option<String>,
    #[allow(dead_code)]
    flow: Option<String>,
    username: String,
    password: String,
}

#[derive(Debug, Deserialize)]
struct TokenRequest {
    grant_type: String,
    client_id: String,
    #[serde(default)]
    code: String,
    #[serde(default)]
    redirect_uri: String,
    #[serde(default)]
    code_verifier: String,
    #[serde(default)]
    refresh_token: String,
}

#[derive(Debug, Deserialize)]
struct RevokeRequest {
    token: String,
}

#[derive(Debug, Serialize)]
struct DevAuthorizeResponse {
    code: String,
    state: String,
    redirect_to: String,
}

#[derive(Debug, Serialize)]
struct TokenResponse {
    access_token: String,
    token_type: &'static str,
    expires_in: u64,
    refresh_token: Option<String>,
    scope: String,
}

#[derive(Debug, Serialize)]
struct UserInfoResponse {
    sub: String,
    preferred_username: String,
    name: String,
    email: String,
    nebula_id: String,
}

impl UserInfoResponse {
    fn from_user(user: &DemoUser) -> Self {
        Self {
            sub: user.id.to_string(),
            preferred_username: user.username.clone(),
            name: user.name.clone(),
            email: user.email.clone(),
            nebula_id: user.nebula_id.as_str().to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
struct SkybridgeDataResponse<T> {
    data: T,
}

#[derive(Debug, Serialize)]
struct SkybridgeProfileResponse {
    id: Uuid,
    email: String,
    phone: Option<String>,
    nebula_id: String,
    full_name: String,
}

impl SkybridgeProfileResponse {
    fn from_user(user: &DemoUser) -> Self {
        Self {
            id: user.id,
            email: user.email.clone(),
            phone: user.phone.clone(),
            nebula_id: user.nebula_id.as_str().to_string(),
            full_name: user.name.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
struct RevokeResponse {
    revoked: bool,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: &'static str,
}

#[derive(Debug, Deserialize)]
struct PublicClientEnv {
    redirect_uris: Vec<String>,
    scopes: Vec<String>,
}
