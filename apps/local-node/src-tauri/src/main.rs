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

const SKILL_API: &str = "http://127.0.0.1:8790";
const AGENT_API: &str = "http://127.0.0.1:17870";

#[derive(Clone, Serialize)]
struct LocalNodeRuntime {
    skill_api: String,
    agent_api: String,
    local_token: String,
    openclaw_token: String,
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
}

#[derive(Deserialize)]
struct HealthProbe {
    service: Option<String>,
    status: Option<String>,
}

enum ExistingLocalNode {
    Ready,
    Blocked(String),
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
            }),
        }
    }

    fn runtime(&self) -> LocalNodeRuntime {
        let snapshot = self
            .snapshot
            .lock()
            .expect("sidecar snapshot lock poisoned");
        LocalNodeRuntime {
            skill_api: self.runtime.skill_api.clone(),
            agent_api: self.runtime.agent_api.clone(),
            local_token: self.runtime.local_token.clone(),
            openclaw_token: self.runtime.openclaw_token.clone(),
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
            restart_local_node
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Ozon Rust Suite local UI");
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
            ExistingLocalNode::Ready => {
                snapshot.pid = None;
                snapshot.status = "external".to_string();
                snapshot.last_error = None;
                snapshot.last_exit = None;
                append_sidecar_log(
                    &PathBuf::from(&state.runtime.sidecar_log_path),
                    "attached to existing ozon-local-node on 127.0.0.1:8790 / 17870",
                );
                return;
            }
            ExistingLocalNode::Blocked(reason) => {
                snapshot.pid = None;
                snapshot.status = "blocked".to_string();
                snapshot.last_error = Some(reason.clone());
                snapshot.last_exit = None;
                append_sidecar_log(&PathBuf::from(&state.runtime.sidecar_log_path), &reason);
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
                .env("OZON_LOCAL_SKILL_BIND", "127.0.0.1:8790")
                .env("OZON_LOCAL_AGENT_BIND", "127.0.0.1:17870")
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
                while let Some(event) = receiver.recv().await {
                    match event {
                        CommandEvent::Stdout(line) => {
                            tracing_line("local-node stdout", &line);
                            tracing_line_to_file(&log_path, "stdout", &line);
                        }
                        CommandEvent::Stderr(line) => {
                            tracing_line("local-node stderr", &line);
                            tracing_line_to_file(&log_path, "stderr", &line);
                        }
                        CommandEvent::Terminated(payload) => {
                            let reason = format!("terminated: {payload:?}");
                            eprintln!("local-node sidecar {reason}");
                            append_sidecar_log(&log_path, &reason);
                            handle_sidecar_exit(&app_handle, pid, reason);
                            break;
                        }
                        CommandEvent::Error(error) => {
                            let reason = format!("error: {error}");
                            eprintln!("local-node sidecar {reason}");
                            append_sidecar_log(&log_path, &reason);
                            handle_sidecar_exit(&app_handle, pid, reason);
                            break;
                        }
                        _ => {}
                    }
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
    }
    if let Some(child) = child {
        let _ = child.kill();
    }
}

fn handle_sidecar_exit(app: &AppHandle, pid: u32, reason: String) {
    let state = app.state::<SidecarState>();
    let should_restart = {
        let mut snapshot = state
            .snapshot
            .lock()
            .expect("sidecar snapshot lock poisoned");
        if snapshot.pid != Some(pid) {
            return;
        }
        snapshot.pid = None;
        snapshot.status = "restarting".to_string();
        snapshot.last_exit = Some(reason);
        snapshot.last_error = None;
        snapshot.desired_running
    };

    let _ = state
        .child
        .lock()
        .expect("sidecar child lock poisoned")
        .take();

    if should_restart {
        schedule_sidecar_restart(app.clone());
    }
}

fn schedule_sidecar_restart(app: AppHandle) {
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(2));
        start_managed_sidecar(&app);
    });
}

fn probe_existing_local_node(local_token: &str) -> ExistingLocalNode {
    if !probe_health_endpoint("127.0.0.1:8790") {
        return ExistingLocalNode::Missing;
    }
    if !probe_health_endpoint("127.0.0.1:17870") {
        return ExistingLocalNode::Blocked(
            "127.0.0.1:8790 已有 Ozon 本地节点，但 17870 agent 端口未就绪".to_string(),
        );
    }
    if !probe_config_status_endpoint(local_token) {
        return ExistingLocalNode::Blocked(
            "检测到已有 Ozon 本地节点，但当前桌面端 token 无法访问 /config/status".to_string(),
        );
    }
    ExistingLocalNode::Ready
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

fn probe_config_status_endpoint(local_token: &str) -> bool {
    probe_http_endpoint("127.0.0.1:8790", "/config/status", Some(local_token))
        .is_some_and(|response| response.status_code == 200)
}

struct HttpProbeResponse {
    status_code: u16,
    body: String,
}

fn probe_http_endpoint(
    addr: &str,
    path: &str,
    local_token: Option<&str>,
) -> Option<HttpProbeResponse> {
    let Ok(mut stream) = TcpStream::connect(addr) else {
        return None;
    };
    let timeout = Some(Duration::from_millis(500));
    let _ = stream.set_read_timeout(timeout);
    let _ = stream.set_write_timeout(timeout);
    let mut request = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n");
    if let Some(token) = local_token {
        request.push_str("x-local-token: ");
        request.push_str(token);
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
    }
    fs::write(&path, serde_json::to_vec_pretty(&secrets)?)?;
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
    use super::parse_http_status;

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
