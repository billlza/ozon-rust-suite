use rand::{Rng, distr::Alphanumeric};
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    fs::OpenOptions,
    io::{Read, Write},
    net::TcpStream,
    path::PathBuf,
    sync::Mutex,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Manager, Runtime, State};
use tauri_plugin_shell::{
    ShellExt,
    process::{CommandChild, CommandEvent},
};
use url::Url;

const SKILL_API: &str = "http://127.0.0.1:8790";
const AGENT_API: &str = "http://127.0.0.1:17870";

#[derive(Clone, Serialize)]
struct LocalNodeRuntime {
    skill_api: String,
    agent_api: String,
    local_token: String,
    connector_mode: String,
    sidecar_pid: Option<u32>,
    sidecar_status: String,
    sidecar_restart_count: u32,
    sidecar_last_started_at_ms: Option<u64>,
    sidecar_last_exit: Option<String>,
    sidecar_last_error: Option<String>,
    sidecar_log_path: String,
}

#[derive(Clone)]
struct RuntimeConfig {
    skill_api: String,
    agent_api: String,
    local_token: String,
    openclaw_token: String,
    connector_mode: String,
    sidecar_log_path: String,
    secret_store_path: String,
}

#[derive(Clone, Serialize)]
struct SidecarSnapshot {
    pid: Option<u32>,
    status: String,
    restart_count: u32,
    last_started_at_ms: Option<u64>,
    last_exit: Option<String>,
    last_error: Option<String>,
    desired_running: bool,
    /// Consecutive *rapid* exits (sidecar died < RAPID_FAILURE_UPTIME_MS after
    /// start). A clean long-lived run resets this to 0. Used to back off instead
    /// of hammer-restarting every 2s — the bug that produced "已自恢复 805 次"
    /// when a second app instance already held 127.0.0.1:8790.
    #[serde(default)]
    consecutive_failures: u32,
    /// The skill/agent base URLs the sidecar actually bound, once known — it may
    /// differ from the default port if PORT_CANDIDATES fell back. Set from the
    /// sidecar's reported "LOCAL_NODE_PORTS" line or from adopting an existing
    /// node, and preferred over the static defaults so the UI talks to the real
    /// port.
    #[serde(default)]
    effective_skill_api: Option<String>,
    #[serde(default)]
    effective_agent_api: Option<String>,
}

/// Loopback (skill, agent) port pairs probed in order to find the node. MUST
/// stay in sync with PORT_CANDIDATES in apps/local-node/src/main.rs and the
/// portal's candidate list in apps/web-portal/src/main.tsx.
const PORT_CANDIDATES: &[(u16, u16)] = &[
    (8790, 17870),
    (8791, 17871),
    (8890, 17970),
    (18790, 27870),
];

/// A sidecar that exits sooner than this after starting is treated as a rapid
/// failure (port already bound, immediate panic) rather than a healthy run.
const RAPID_FAILURE_UPTIME_MS: u64 = 8_000;
/// After this many consecutive rapid failures, stop auto-restarting and surface
/// a "blocked" status so the user sees a real error instead of a silent loop.
const MAX_RAPID_FAILURES: u32 = 6;
/// Base restart delay; the effective delay grows 2s → 4s → 8s → … capped at 30s.
const RESTART_BASE_DELAY_SECS: u64 = 2;
const RESTART_MAX_DELAY_SECS: u64 = 30;

#[derive(Deserialize)]
struct HealthProbe {
    service: Option<String>,
    status: Option<String>,
}

enum ExistingLocalNode {
    /// An adoptable node (healthy + same operator token) is already listening on
    /// this candidate pair. Carries its base URLs so the UI uses the right port.
    Ready {
        skill_api: String,
        agent_api: String,
    },
    Missing,
}

struct SidecarState {
    runtime: RuntimeConfig,
    child: Mutex<Option<CommandChild>>,
    snapshot: Mutex<SidecarSnapshot>,
}

impl SidecarState {
    fn new(runtime: RuntimeConfig) -> Self {
        Self {
            runtime,
            child: Mutex::new(None),
            snapshot: Mutex::new(SidecarSnapshot {
                pid: None,
                status: "starting".to_string(),
                restart_count: 0,
                last_started_at_ms: None,
                last_exit: None,
                last_error: None,
                desired_running: true,
                consecutive_failures: 0,
                effective_skill_api: None,
                effective_agent_api: None,
            }),
        }
    }

    fn runtime(&self) -> LocalNodeRuntime {
        let snapshot = self
            .snapshot
            .lock()
            .expect("sidecar snapshot lock poisoned");
        LocalNodeRuntime {
            skill_api: snapshot
                .effective_skill_api
                .clone()
                .unwrap_or_else(|| self.runtime.skill_api.clone()),
            agent_api: snapshot
                .effective_agent_api
                .clone()
                .unwrap_or_else(|| self.runtime.agent_api.clone()),
            local_token: self.runtime.local_token.clone(),
            connector_mode: self.runtime.connector_mode.clone(),
            sidecar_pid: snapshot.pid,
            sidecar_status: snapshot.status.clone(),
            sidecar_restart_count: snapshot.restart_count,
            sidecar_last_started_at_ms: snapshot.last_started_at_ms,
            sidecar_last_exit: snapshot.last_exit.clone(),
            sidecar_last_error: snapshot.last_error.clone(),
            sidecar_log_path: self.runtime.sidecar_log_path.clone(),
        }
    }
}

impl Drop for SidecarState {
    fn drop(&mut self) {
        if let Ok(mut snapshot) = self.snapshot.lock() {
            snapshot.desired_running = false;
            snapshot.status = "stopped".to_string();
        }
        if let Ok(mut child) = self.child.lock() {
            if let Some(child) = child.take() {
                let _ = child.kill();
            }
        }
    }
}

#[derive(Deserialize, Serialize)]
struct StoredSecrets {
    local_token: String,
    openclaw_token: String,
}

#[tauri::command]
fn local_node_runtime(state: State<'_, SidecarState>) -> LocalNodeRuntime {
    state.runtime()
}

#[tauri::command]
fn restart_local_node(app: AppHandle, state: State<'_, SidecarState>) -> LocalNodeRuntime {
    stop_current_sidecar(&state, "manual restart requested");
    start_managed_sidecar(&app);
    state.runtime()
}

#[tauri::command]
fn open_openclaw_binding_url(bind_url: String) -> Result<(), String> {
    validate_openclaw_binding_url(&bind_url)?;
    tauri_plugin_opener::open_url(bind_url, None::<&str>)
        .map_err(|error| format!("failed to open browser: {error}"))
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let secrets = load_or_create_secrets(app)?;
            let local_token = env::var("OZON_LOCAL_TOKEN").unwrap_or(secrets.local_token);
            let openclaw_token = env::var("OZON_OPENCLAW_TOKEN").unwrap_or(secrets.openclaw_token);
            let connector_mode = env::var("OZON_CONNECTOR_MODE").unwrap_or_else(|_| {
                if cfg!(debug_assertions) {
                    "mock".to_string()
                } else {
                    "real".to_string()
                }
            });
            let runtime = RuntimeConfig {
                skill_api: SKILL_API.to_string(),
                agent_api: AGENT_API.to_string(),
                local_token,
                openclaw_token,
                connector_mode,
                sidecar_log_path: sidecar_log_path(app).display().to_string(),
                secret_store_path: private_secrets_path(app)?.display().to_string(),
            };

            app.manage(SidecarState::new(runtime));
            let app_handle = app.handle().clone();
            start_managed_sidecar(&app_handle);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            local_node_runtime,
            restart_local_node,
            open_openclaw_binding_url
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Ozon Rust Suite local UI");
}

fn validate_openclaw_binding_url(bind_url: &str) -> Result<(), String> {
    let parsed = Url::parse(bind_url).map_err(|_| "invalid binding URL".to_string())?;
    if parsed.path() != "/openclaw/import" {
        return Err("binding URL path is not allowed".to_string());
    }
    let host = parsed.host_str().unwrap_or_default();
    let origin_allowed = matches!(
        (parsed.scheme(), host, parsed.port_or_known_default()),
        (
            "https",
            "ozonclaw.jl696.cn" | "www.ozonclaw.jl696.cn",
            Some(443)
        ) | ("http", "127.0.0.1" | "localhost", Some(18789))
    );
    if !origin_allowed {
        return Err("binding URL origin is not allowed".to_string());
    }
    let fragment = parsed
        .fragment()
        .ok_or_else(|| "binding URL is missing pairing fragment".to_string())?;
    if !fragment
        .split('&')
        .any(|part| part.starts_with("ozon66_pairing_code="))
    {
        return Err("binding URL is missing pairing code".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod openclaw_binding_url_tests {
    use super::validate_openclaw_binding_url;

    #[test]
    fn openclaw_binding_url_allows_expected_origin_and_fragment() {
        assert!(
            validate_openclaw_binding_url(
                "https://ozonclaw.jl696.cn/openclaw/import#ozon66_pairing_code=abc"
            )
            .is_ok()
        );
    }

    #[test]
    fn openclaw_binding_url_rejects_unexpected_origin() {
        assert!(
            validate_openclaw_binding_url(
                "https://example.com/openclaw/import#ozon66_pairing_code=abc"
            )
            .is_err()
        );
    }
}

fn start_managed_sidecar(app: &AppHandle) {
    let state = app.state::<SidecarState>();
    if state
        .child
        .lock()
        .expect("sidecar child lock poisoned")
        .is_some()
    {
        return;
    }

    {
        let mut snapshot = state
            .snapshot
            .lock()
            .expect("sidecar snapshot lock poisoned");
        if !snapshot.desired_running {
            return;
        }
        match probe_existing_local_node(&state.runtime.local_token) {
            ExistingLocalNode::Ready { skill_api, agent_api } => {
                snapshot.pid = None;
                snapshot.status = "external".to_string();
                snapshot.last_error = None;
                snapshot.last_exit = None;
                snapshot.effective_skill_api = Some(skill_api.clone());
                snapshot.effective_agent_api = Some(agent_api.clone());
                append_sidecar_log(
                    &PathBuf::from(&state.runtime.sidecar_log_path),
                    &format!("attached to existing ozon-local-node on {skill_api} / {agent_api}"),
                );
                return;
            }
            ExistingLocalNode::Missing => {}
        }
        snapshot.status = "starting".to_string();
        snapshot.last_error = None;
    }

    let runtime = state.runtime.clone();
    let log_path = PathBuf::from(&runtime.sidecar_log_path);
    match app
        .shell()
        .sidecar("ozon-local-node")
        .map(|command| {
            command
                .env("OZON_CONNECTOR_MODE", runtime.connector_mode)
                .env("OZON_LOCAL_TOKEN", runtime.local_token)
                .env("OZON_OPENCLAW_TOKEN", runtime.openclaw_token)
                .env("OZON_LOCAL_SECRET_FILE", runtime.secret_store_path)
                // No fixed bind: the sidecar picks the first free pair from
                // PORT_CANDIDATES and reports it via "LOCAL_NODE_PORTS", so a
                // busy 8790/17870 (orphan or a third-party app) no longer leaves
                // it permanently offline.
        })
        .and_then(|command| command.spawn())
    {
        Ok((mut receiver, child)) => {
            let pid = child.pid();
            {
                let mut child_slot = state.child.lock().expect("sidecar child lock poisoned");
                if child_slot.is_some() {
                    let _ = child.kill();
                    return;
                }
                *child_slot = Some(child);
            }
            {
                let mut snapshot = state
                    .snapshot
                    .lock()
                    .expect("sidecar snapshot lock poisoned");
                if snapshot.last_started_at_ms.is_some() {
                    snapshot.restart_count += 1;
                }
                snapshot.pid = Some(pid);
                snapshot.status = "running".to_string();
                snapshot.last_started_at_ms = Some(now_ms());
                snapshot.last_error = None;
            }
            let app_handle = app.clone();
            append_sidecar_log(&log_path, &format!("started pid={pid}"));
            tauri::async_runtime::spawn(async move {
                // Keep draining the event channel after Terminated/Error so the
                // sidecar's final stderr (e.g. the real bind error) is written to
                // the log before we react. On Windows the Terminated event can
                // arrive before the buffered stderr is delivered, which is how the
                // "exit code 1" cause stayed invisible in the user's log.
                let mut pending_exit: Option<String> = None;
                while let Some(event) = receiver.recv().await {
                    match event {
                        CommandEvent::Stdout(line) => {
                            tracing_line("local-node stdout", &line);
                            tracing_line_to_file(&log_path, "stdout", &line);
                            // The sidecar reports the actual ports it bound (it may
                            // have fallen back off a busy 8790/17870); record them so
                            // the UI talks to the real port.
                            if let Some((skill_api, agent_api)) = parse_reported_ports(&line) {
                                if let Ok(mut snapshot) = app_handle
                                    .state::<SidecarState>()
                                    .snapshot
                                    .lock()
                                {
                                    snapshot.effective_skill_api = Some(skill_api);
                                    snapshot.effective_agent_api = Some(agent_api);
                                }
                            }
                        }
                        CommandEvent::Stderr(line) => {
                            tracing_line("local-node stderr", &line);
                            tracing_line_to_file(&log_path, "stderr", &line);
                        }
                        CommandEvent::Terminated(payload) => {
                            let reason = format!("terminated: {payload:?}");
                            eprintln!("local-node sidecar {reason}");
                            append_sidecar_log(&log_path, &reason);
                            pending_exit = Some(reason);
                        }
                        CommandEvent::Error(error) => {
                            let reason = format!("error: {error}");
                            eprintln!("local-node sidecar {reason}");
                            append_sidecar_log(&log_path, &reason);
                            pending_exit = Some(reason);
                        }
                        _ => {}
                    }
                }
                if let Some(reason) = pending_exit {
                    handle_sidecar_exit(&app_handle, pid, reason);
                }
            });
            eprintln!("local-node sidecar started: pid={pid}");
        }
        Err(error) => {
            eprintln!("local-node sidecar was not started: {error}");
            append_sidecar_log(&log_path, &format!("failed to start: {error}"));
            let mut snapshot = state
                .snapshot
                .lock()
                .expect("sidecar snapshot lock poisoned");
            snapshot.pid = None;
            snapshot.status = "failed".to_string();
            snapshot.last_error = Some(error.to_string());
        }
    }
}

fn stop_current_sidecar(state: &SidecarState, reason: &str) {
    let child = state
        .child
        .lock()
        .expect("sidecar child lock poisoned")
        .take();
    {
        let mut snapshot = state
            .snapshot
            .lock()
            .expect("sidecar snapshot lock poisoned");
        snapshot.pid = None;
        snapshot.status = "restarting".to_string();
        snapshot.last_exit = Some(reason.to_string());
        snapshot.last_error = None;
        snapshot.desired_running = true;
        // A manual restart is an explicit "try again from scratch": clear the
        // backoff streak so a previously-"blocked" node will attempt to start.
        snapshot.consecutive_failures = 0;
    }
    if let Some(child) = child {
        let _ = child.kill();
    }
}

fn handle_sidecar_exit(app: &AppHandle, pid: u32, reason: String) {
    let state = app.state::<SidecarState>();
    let decision = {
        let mut snapshot = state
            .snapshot
            .lock()
            .expect("sidecar snapshot lock poisoned");
        if snapshot.pid != Some(pid) {
            return;
        }
        snapshot.pid = None;

        // A run that stayed up past the rapid-failure window counts as healthy:
        // reset the backoff so a later crash restarts promptly. A run that died
        // immediately (port already bound, panic on boot) increments the streak.
        let uptime_ms = snapshot
            .last_started_at_ms
            .map(|started| now_ms().saturating_sub(started));
        let rapid = uptime_ms.map(|ms| ms < RAPID_FAILURE_UPTIME_MS).unwrap_or(true);
        if rapid {
            snapshot.consecutive_failures = snapshot.consecutive_failures.saturating_add(1);
        } else {
            snapshot.consecutive_failures = 0;
        }

        snapshot.last_exit = Some(reason.clone());
        snapshot.last_error = None;

        if !snapshot.desired_running {
            snapshot.status = "stopped".to_string();
            SidecarExit::Stop
        } else if snapshot.consecutive_failures >= MAX_RAPID_FAILURES {
            // Give up the tight loop. Almost always: another ozon-local-node (a
            // second app copy) already owns 127.0.0.1:8790. Surface it instead of
            // restarting forever.
            snapshot.status = "blocked".to_string();
            let blocked = if is_address_in_use(&reason) {
                "另一个 Ozon 本地节点已占用本机端口（多半是上次没退干净留下的残留进程，或开了两份 App）。请在任务管理器结束所有 ozon-local-node 进程后点重启节点。".to_string()
            } else {
                format!("本地节点连续 {} 次快速退出，已停止自动重启。最后退出：{reason}", snapshot.consecutive_failures)
            };
            snapshot.last_error = Some(blocked);
            SidecarExit::GiveUp
        } else {
            snapshot.status = "restarting".to_string();
            SidecarExit::Restart(restart_delay_secs(snapshot.consecutive_failures))
        }
    };

    let _ = state
        .child
        .lock()
        .expect("sidecar child lock poisoned")
        .take();

    if let SidecarExit::Restart(delay) = decision {
        schedule_sidecar_restart(app.clone(), delay);
    }
}

enum SidecarExit {
    Restart(u64),
    GiveUp,
    Stop,
}

/// True when a sidecar exit reason is a "port already in use" error, across
/// platforms: Unix EADDRINUSE (os error 48), Windows WSAEADDRINUSE (10048),
/// Windows WSAEACCES on a reserved/excluded port (10013), and the OS strings.
/// Windows users hit this when a prior sidecar orphaned and still holds the port.
fn is_address_in_use(reason: &str) -> bool {
    reason.contains("os error 48")
        || reason.contains("os error 10048")
        || reason.contains("os error 10013")
        || reason.contains("Address already in use")
        || reason.contains("Only one usage of each socket address")
}

/// Parse the sidecar's `LOCAL_NODE_PORTS skill=8791 agent=17871` stdout line into
/// the (skill_api, agent_api) base URLs. Returns None for any other line.
fn parse_reported_ports(line: &[u8]) -> Option<(String, String)> {
    let text = String::from_utf8_lossy(line);
    let rest = text.trim().strip_prefix("LOCAL_NODE_PORTS")?;
    let mut skill: Option<u16> = None;
    let mut agent: Option<u16> = None;
    for token in rest.split_whitespace() {
        if let Some(value) = token.strip_prefix("skill=") {
            skill = value.parse().ok();
        } else if let Some(value) = token.strip_prefix("agent=") {
            agent = value.parse().ok();
        }
    }
    Some((
        format!("http://127.0.0.1:{}", skill?),
        format!("http://127.0.0.1:{}", agent?),
    ))
}

/// Exponential backoff capped at RESTART_MAX_DELAY_SECS: 2s, 4s, 8s, 16s, 30s, 30s…
/// `failures` is 1-based (first failure → index 0 → base delay).
fn restart_delay_secs(failures: u32) -> u64 {
    let shift = failures.saturating_sub(1).min(8);
    let delay = RESTART_BASE_DELAY_SECS.saturating_mul(1u64 << shift);
    delay.min(RESTART_MAX_DELAY_SECS)
}

fn schedule_sidecar_restart(app: AppHandle, delay_secs: u64) {
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(delay_secs));
        start_managed_sidecar(&app);
    });
}

fn probe_existing_local_node(local_token: &str) -> ExistingLocalNode {
    // Look across the candidate pairs for a node we can adopt (healthy on both
    // ports + proves the same operator token). A candidate that's busy with a
    // foreign/half-up process is simply skipped — the sidecar will skip it too
    // and bind the next free pair. If none is adoptable, return Missing so we
    // spawn a fresh sidecar (which picks the first free pair via PORT_CANDIDATES).
    for (skill_port, agent_port) in PORT_CANDIDATES {
        let skill = format!("127.0.0.1:{skill_port}");
        let agent = format!("127.0.0.1:{agent_port}");
        if probe_health_endpoint(&skill)
            && probe_health_endpoint(&agent)
            && probe_attest_endpoint(local_token, &skill)
        {
            return ExistingLocalNode::Ready {
                skill_api: format!("http://{skill}"),
                agent_api: format!("http://{agent}"),
            };
        }
    }
    ExistingLocalNode::Missing
}

fn probe_health_endpoint(addr: &str) -> bool {
    let Some(response) = probe_http_endpoint(addr, "/health", None) else {
        return false;
    };
    if response.status_code != 200 {
        return false;
    }
    let Ok(health) = serde_json::from_str::<HealthProbe>(response.body.trim()) else {
        return false;
    };
    health.service.as_deref() == Some("ozon-local-node") && health.status.as_deref() == Some("ok")
}

/// Verify a pre-existing node holds the same operator token WITHOUT transmitting the token:
/// challenge it with a fresh nonce and accept only a correct HMAC-SHA256(local_token, nonce)
/// proof. A port squatter that lacks the token cannot answer, so the token is never disclosed to
/// an unauthenticated peer (and is only ever sent to a peer that has proven it already holds it).
fn probe_attest_endpoint(local_token: &str, skill_addr: &str) -> bool {
    let nonce = generate_nonce();
    let Some(response) = probe_http_endpoint(
        skill_addr,
        "/attest",
        Some(("x-attest-nonce", &nonce)),
    ) else {
        return false;
    };
    if response.status_code != 200 {
        return false;
    }
    let Ok(parsed) = serde_json::from_str::<AttestProbe>(response.body.trim()) else {
        return false;
    };
    let Some(proof) = parsed.proof else {
        return false;
    };
    constant_time_eq(
        proof.as_bytes(),
        attest_proof(local_token, &nonce).as_bytes(),
    )
}

#[derive(Deserialize)]
struct AttestProbe {
    proof: Option<String>,
}

fn generate_nonce() -> String {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect()
}

/// HMAC-SHA256(local_token, nonce), lowercase hex. Must stay byte-for-byte identical to the
/// `attest_proof` served by the local-node `/attest` endpoint.
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

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in left.iter().zip(right.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}

struct HttpProbeResponse {
    status_code: u16,
    body: String,
}

fn probe_http_endpoint(
    addr: &str,
    path: &str,
    header: Option<(&str, &str)>,
) -> Option<HttpProbeResponse> {
    let Ok(mut stream) = TcpStream::connect(addr) else {
        return None;
    };
    let timeout = Some(Duration::from_millis(500));
    let _ = stream.set_read_timeout(timeout);
    let _ = stream.set_write_timeout(timeout);
    let mut request = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n");
    if let Some((name, value)) = header {
        request.push_str(name);
        request.push_str(": ");
        request.push_str(value);
        request.push_str("\r\n");
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes()).ok()?;

    let mut raw = String::new();
    stream.read_to_string(&mut raw).ok()?;
    let Some((headers, body)) = raw.split_once("\r\n\r\n") else {
        return None;
    };
    let status_code = parse_http_status(headers)?;
    Some(HttpProbeResponse {
        status_code,
        body: body.to_string(),
    })
}

fn parse_http_status(headers: &str) -> Option<u16> {
    headers
        .lines()
        .next()?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn tracing_line(label: &str, line: &[u8]) {
    let text = String::from_utf8_lossy(line);
    let text = text.trim();
    if !text.is_empty() {
        eprintln!("{label}: {text}");
    }
}

fn tracing_line_to_file(path: &PathBuf, label: &str, line: &[u8]) {
    let text = String::from_utf8_lossy(line);
    let text = text.trim();
    if !text.is_empty() {
        append_sidecar_log(path, &format!("{label}: {text}"));
    }
}

fn append_sidecar_log(path: &PathBuf, line: &str) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "[{}] {line}", now_ms());
    }
}

fn sidecar_log_path<R: Runtime>(app: &tauri::App<R>) -> PathBuf {
    let mut path = app
        .path()
        .app_log_dir()
        .or_else(|_| app.path().app_config_dir())
        .unwrap_or_else(|_| env::temp_dir());
    path.push("ozon-local-node-sidecar.log");
    path
}

fn load_or_create_secrets<R: Runtime>(
    app: &tauri::App<R>,
) -> Result<StoredSecrets, Box<dyn std::error::Error>> {
    let path = secrets_path(app)?;
    if let Ok(raw) = fs::read_to_string(&path) {
        if let Ok(secrets) = serde_json::from_str::<StoredSecrets>(&raw) {
            if valid_secret(&secrets.local_token) && valid_secret(&secrets.openclaw_token) {
                return Ok(secrets);
            }
        }
    }

    let secrets = StoredSecrets {
        local_token: generate_secret(),
        openclaw_token: generate_secret(),
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
        }
    }
    fs::write(&path, serde_json::to_vec_pretty(&secrets)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(secrets)
}

fn secrets_path<R: Runtime>(app: &tauri::App<R>) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let mut path = app.path().app_config_dir()?;
    path.push("local-node-secrets.json");
    Ok(path)
}

fn private_secrets_path<R: Runtime>(
    app: &tauri::App<R>,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let mut path = app.path().app_config_dir()?;
    path.push("local-node-private-secrets.json");
    Ok(path)
}

fn generate_secret() -> String {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .take(48)
        .map(char::from)
        .collect()
}

fn valid_secret(value: &str) -> bool {
    value.len() >= 32
}

#[cfg(test)]
mod tests {
    use super::{is_address_in_use, parse_http_status, parse_reported_ports};

    #[test]
    fn parse_reported_ports_reads_the_sidecar_line() {
        // Must match the exact line the sidecar prints in apps/local-node/src/main.rs.
        assert_eq!(
            parse_reported_ports(b"LOCAL_NODE_PORTS skill=8791 agent=17871"),
            Some((
                "http://127.0.0.1:8791".to_string(),
                "http://127.0.0.1:17871".to_string()
            ))
        );
        // Trailing newline / surrounding whitespace is tolerated.
        assert_eq!(
            parse_reported_ports(b"  LOCAL_NODE_PORTS skill=8790 agent=17870\n"),
            Some((
                "http://127.0.0.1:8790".to_string(),
                "http://127.0.0.1:17870".to_string()
            ))
        );
        assert_eq!(parse_reported_ports(b"starting local node services"), None);
        assert_eq!(parse_reported_ports(b"LOCAL_NODE_PORTS skill=8791"), None);
    }

    #[test]
    fn is_address_in_use_covers_windows_and_unix() {
        assert!(is_address_in_use("terminated: ... os error 48"));
        assert!(is_address_in_use("failed to bind 127.0.0.1:8790: ... os error 10048"));
        assert!(is_address_in_use("Only one usage of each socket address"));
        assert!(!is_address_in_use("some unrelated panic"));
    }

    #[test]
    fn parse_http_status_extracts_status_code() {
        assert_eq!(
            parse_http_status("HTTP/1.1 200 OK\r\ncontent-type: application/json"),
            Some(200)
        );
        assert_eq!(parse_http_status("HTTP/1.0 401 Unauthorized"), Some(401));
        assert_eq!(parse_http_status("not-http"), None);
    }
}
