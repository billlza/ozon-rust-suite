use rand::{Rng, distr::Alphanumeric};
use serde::{Deserialize, Serialize};
use std::{env, fs, path::PathBuf, sync::Mutex};
use tauri::{Manager, Runtime};
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
}

struct RuntimeState(LocalNodeRuntime);

struct SidecarState {
    child: Mutex<Option<CommandChild>>,
}

impl Drop for SidecarState {
    fn drop(&mut self) {
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
fn local_node_runtime(state: tauri::State<'_, RuntimeState>) -> LocalNodeRuntime {
    state.0.clone()
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
            let child =
                start_local_node_sidecar(app, &local_token, &openclaw_token, &connector_mode);
            let sidecar_pid = child.as_ref().map(CommandChild::pid);

            app.manage(RuntimeState(LocalNodeRuntime {
                skill_api: SKILL_API.to_string(),
                agent_api: AGENT_API.to_string(),
                local_token,
                openclaw_token,
                connector_mode,
                sidecar_pid,
            }));
            app.manage(SidecarState {
                child: Mutex::new(child),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![local_node_runtime])
        .run(tauri::generate_context!())
        .expect("failed to run Ozon Rust Suite local UI");
}

fn start_local_node_sidecar<R: Runtime>(
    app: &tauri::App<R>,
    local_token: &str,
    openclaw_token: &str,
    connector_mode: &str,
) -> Option<CommandChild> {
    match app
        .shell()
        .sidecar("ozon-local-node")
        .map(|command| {
            command
                .env("OZON_CONNECTOR_MODE", connector_mode)
                .env("OZON_LOCAL_TOKEN", local_token)
                .env("OZON_OPENCLAW_TOKEN", openclaw_token)
                .env("OZON_LOCAL_SKILL_BIND", "127.0.0.1:8790")
                .env("OZON_LOCAL_AGENT_BIND", "127.0.0.1:17870")
        })
        .and_then(|command| command.spawn())
    {
        Ok((mut receiver, child)) => {
            let pid = child.pid();
            tauri::async_runtime::spawn(async move {
                while let Some(event) = receiver.recv().await {
                    match event {
                        CommandEvent::Stdout(line) => {
                            tracing_line("local-node stdout", &line);
                        }
                        CommandEvent::Stderr(line) => {
                            tracing_line("local-node stderr", &line);
                        }
                        CommandEvent::Terminated(payload) => {
                            eprintln!("local-node sidecar terminated: {:?}", payload);
                            break;
                        }
                        CommandEvent::Error(error) => {
                            eprintln!("local-node sidecar error: {error}");
                            break;
                        }
                        _ => {}
                    }
                }
            });
            eprintln!("local-node sidecar started: pid={pid}");
            Some(child)
        }
        Err(error) => {
            eprintln!("local-node sidecar was not started: {error}");
            None
        }
    }
}

fn tracing_line(label: &str, line: &[u8]) {
    let text = String::from_utf8_lossy(line);
    let text = text.trim();
    if !text.is_empty() {
        eprintln!("{label}: {text}");
    }
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
