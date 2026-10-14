// cliente mcp: cocuyo se conecta a los mismos servidores que claude desktop y usa sus herramientas por voz

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const PROTOCOL: &str = "2025-06-18";
const START_WAIT: Duration = Duration::from_secs(45);
const CALL_WAIT: Duration = Duration::from_secs(90);
const RETRY_AFTER: Duration = Duration::from_secs(60);

#[derive(Clone, Deserialize)]
struct Spec {
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: HashMap<String, String>,
    #[serde(default)]
    disabled: bool,
}

#[derive(Deserialize)]
struct Config {
    #[serde(rename = "mcpServers", default)]
    servers: HashMap<String, Spec>,
}

#[derive(Clone, Serialize)]
pub struct Tool {
    pub server: String,
    pub name: String,
    pub description: String,
    pub schema: Value,
}

struct Server {
    child: Child,
    stdin: Arc<Mutex<ChildStdin>>,
    replies: Receiver<Value>,
    next: u64,
    tools: Vec<Tool>,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

#[derive(Default)]
struct State {
    servers: HashMap<String, Server>,
    failed: HashMap<String, (Instant, String)>,
    loaded: Option<Instant>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

pub fn path(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_config_dir().ok()?.join("mcp.json"))
}

fn template(app: &AppHandle) -> String {
    let documents = app.path().document_dir().map(|dir| dir.to_string_lossy().into_owned()).unwrap_or_else(|_| "C:\\\\Users\\\\tu-usuario\\\\Documents".into());
    let config = json!({
        "mcpServers": {
            "archivos": { "command": "npx", "args": ["-y", "@modelcontextprotocol/server-filesystem", documents], "disabled": true },
            "memoria": { "command": "npx", "args": ["-y", "@modelcontextprotocol/server-memory"], "disabled": true },
            "navegador": { "command": "npx", "args": ["-y", "@playwright/mcp@latest"], "disabled": true },
            "tiempo": { "command": "uvx", "args": ["mcp-server-time", "--local-timezone=America/Bogota"], "disabled": true }
        }
    });
    serde_json::to_string_pretty(&config).unwrap_or_default()
}

fn read_config(app: &AppHandle) -> HashMap<String, Spec> {
    let Some(path) = path(app) else { return HashMap::new() };
    if !path.is_file() {
        let _ = std::fs::write(&path, template(app));
    }
    std::fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Config>(&bytes).ok())
        .map(|config| config.servers.into_iter().filter(|(_, spec)| !spec.disabled && !spec.command.trim().is_empty()).collect())
        .unwrap_or_default()
}

fn command_for(spec: &Spec) -> Command {
    let lower = spec.command.to_lowercase();
    let mut command = if lower.ends_with(".exe") {
        Command::new(&spec.command)
    } else {
        let mut via = Command::new("cmd.exe");
        via.args(["/d", "/s", "/c", &spec.command]);
        via
    };
    command.args(&spec.args).envs(&spec.env).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).creation_flags(CREATE_NO_WINDOW);
    command
}

fn write(stdin: &Arc<Mutex<ChildStdin>>, message: &Value) -> Result<(), String> {
    let mut stdin = stdin.lock().map_err(|_| "el servidor quedó trabado".to_string())?;
    writeln!(stdin, "{message}").and_then(|()| stdin.flush()).map_err(|e| format!("el servidor se cerró: {e}"))
}

impl Server {
    fn start(spec: &Spec) -> Result<Server, String> {
        let mut child = command_for(spec).spawn().map_err(|e| format!("no arrancó «{}»: {e}", spec.command))?;
        let stdin = Arc::new(Mutex::new(child.stdin.take().ok_or("sin entrada")?));
        let stdout = child.stdout.take().ok_or("sin salida")?;
        let (send, replies) = channel();
        let answer = stdin.clone();
        std::thread::Builder::new()
            .name("mcp".into())
            .spawn(move || {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    let Ok(message) = serde_json::from_str::<Value>(line.trim()) else { continue };
                    if message.get("method").is_some() {
                        if let Some(id) = message.get("id") {
                            let _ = write(&answer, &json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": "Cocuyo no sabe hacer eso" } }));
                        }
                        continue;
                    }
                    if message.get("id").is_some() && send.send(message).is_err() {
                        return;
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        let mut server = Server { child, stdin, replies, next: 1, tools: Vec::new() };
        server.request(
            "initialize",
            json!({ "protocolVersion": PROTOCOL, "capabilities": {}, "clientInfo": { "name": "cocuyo", "version": env!("CARGO_PKG_VERSION") } }),
            START_WAIT,
        )?;
        write(&server.stdin, &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))?;
        Ok(server)
    }

    fn request(&mut self, method: &str, params: Value, wait: Duration) -> Result<Value, String> {
        let id = self.next;
        self.next += 1;
        write(&self.stdin, &json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))?;
        let until = Instant::now() + wait;
        loop {
            let left = until.saturating_duration_since(Instant::now());
            let reply = self.replies.recv_timeout(left).map_err(|_| format!("el servidor no respondió a «{method}»"))?;
            if reply["id"].as_u64() != Some(id) {
                continue;
            }
            if let Some(error) = reply.get("error") {
                return Err(error["message"].as_str().unwrap_or("error del servidor").chars().take(200).collect());
            }
            return Ok(reply.get("result").cloned().unwrap_or(Value::Null));
        }
    }

    fn list_tools(&mut self, server: &str) -> Result<Vec<Tool>, String> {
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..20 {
            let params = cursor.as_ref().map_or(json!({}), |cursor| json!({ "cursor": cursor }));
            let page = self.request("tools/list", params, START_WAIT)?;
            for tool in page["tools"].as_array().into_iter().flatten() {
                let Some(name) = tool["name"].as_str() else { continue };
                tools.push(Tool {
                    server: server.to_string(),
                    name: name.to_string(),
                    description: tool["description"].as_str().unwrap_or("").chars().take(900).collect(),
                    schema: tool.get("inputSchema").cloned().unwrap_or_else(|| json!({ "type": "object", "properties": {} })),
                });
            }
            cursor = page["nextCursor"].as_str().map(str::to_owned);
            if cursor.is_none() {
                break;
            }
        }
        Ok(tools)
    }
}

fn with_state<T>(work: impl FnOnce(&mut State) -> T) -> T {
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    work(guard.get_or_insert_with(State::default))
}

pub fn ensure(app: &AppHandle) {
    let wanted = read_config(app);
    let pending: Vec<(String, Spec)> = with_state(|state| {
        state.servers.retain(|name, server| wanted.contains_key(name) && server.child.try_wait().ok().flatten().is_none());
        state.loaded = Some(Instant::now());
        wanted
            .iter()
            .filter(|(name, _)| !state.servers.contains_key(*name))
            .filter(|(name, _)| state.failed.get(*name).is_none_or(|(when, _)| when.elapsed() > RETRY_AFTER))
            .map(|(name, spec)| (name.clone(), spec.clone()))
            .collect()
    });
    let started: Vec<(String, Result<Server, String>)> = std::thread::scope(|scope| {
        let jobs: Vec<_> = pending
            .into_iter()
            .map(|(name, spec)| {
                scope.spawn(move || {
                    let server = Server::start(&spec).and_then(|mut server| {
                        server.tools = server.list_tools(&name)?;
                        Ok(server)
                    });
                    (name, server)
                })
            })
            .collect();
        jobs.into_iter().filter_map(|job| job.join().ok()).collect()
    });
    with_state(|state| {
        for (name, server) in started {
            match server {
                Ok(server) => {
                    state.failed.remove(&name);
                    state.servers.insert(name, server);
                }
                Err(why) => {
                    state.failed.insert(name, (Instant::now(), why));
                }
            }
        }
    });
}

pub fn tools(app: &AppHandle) -> Vec<Tool> {
    ensure(app);
    with_state(|state| state.servers.values().flat_map(|server| server.tools.clone()).collect())
}

fn text_of(result: &Value) -> String {
    let mut parts = Vec::new();
    for item in result["content"].as_array().into_iter().flatten() {
        match item["type"].as_str() {
            Some("text") => parts.push(item["text"].as_str().unwrap_or("").to_string()),
            Some("image") => parts.push("(una imagen)".into()),
            Some("resource") => parts.push(item["resource"]["text"].as_str().unwrap_or("(un recurso)").to_string()),
            _ => {}
        }
    }
    if parts.is_empty() {
        if let Some(structured) = result.get("structuredContent") {
            parts.push(structured.to_string());
        }
    }
    let text = parts.join("\n");
    text.chars().take(12_000).collect()
}

pub fn call(server: &str, tool: &str, arguments: Value) -> Result<String, String> {
    let result = with_state(|state| {
        let server = state.servers.get_mut(server).ok_or_else(|| format!("el servidor «{server}» no está conectado"))?;
        server.request("tools/call", json!({ "name": tool, "arguments": arguments }), CALL_WAIT)
    })?;
    let text = text_of(&result);
    if result["isError"].as_bool().unwrap_or(false) {
        Err(if text.is_empty() { "la herramienta falló".into() } else { text })
    } else {
        Ok(text)
    }
}

pub fn shutdown() {
    with_state(|state| state.servers.clear());
}

#[derive(Serialize)]
pub struct Status {
    name: String,
    ok: bool,
    tools: usize,
    error: Option<String>,
}

#[tauri::command]
pub async fn mcp_status(app: AppHandle) -> Vec<Status> {
    ensure(&app);
    let wanted = read_config(&app);
    with_state(|state| {
        let mut all: Vec<Status> = wanted
            .keys()
            .map(|name| match state.servers.get(name) {
                Some(server) => Status { name: name.clone(), ok: true, tools: server.tools.len(), error: None },
                None => Status { name: name.clone(), ok: false, tools: 0, error: state.failed.get(name).map(|(_, why)| why.clone()) },
            })
            .collect();
        all.sort_by(|a, b| a.name.cmp(&b.name));
        all
    })
}

#[tauri::command]
pub fn mcp_open(app: AppHandle) -> Result<(), String> {
    let path = path(&app).ok_or("No se encontró la carpeta de configuración.")?;
    if !path.is_file() {
        std::fs::write(&path, template(&app)).map_err(|e| e.to_string())?;
    }
    crate::apps::open(&path.to_string_lossy())
}

#[tauri::command]
pub async fn mcp_reload(app: AppHandle) -> Vec<Status> {
    with_state(|state| {
        state.servers.clear();
        state.failed.clear();
    });
    mcp_status(app).await
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAKE: &str = r#"
import json, sys
for line in sys.stdin:
    msg = json.loads(line)
    if "id" not in msg:
        continue
    method = msg.get("method")
    if method == "initialize":
        result = {"protocolVersion": "2025-06-18", "capabilities": {"tools": {}}, "serverInfo": {"name": "falso", "version": "1"}}
    elif method == "tools/list":
        if msg.get("params", {}).get("cursor"):
            result = {"tools": [{"name": "eco", "description": "Repite", "inputSchema": {"type": "object", "properties": {"texto": {"type": "string"}}}}]}
        else:
            result = {"tools": [{"name": "suma", "description": "Suma dos números", "inputSchema": {"type": "object", "properties": {"a": {"type": "number"}, "b": {"type": "number"}}, "required": ["a", "b"]}}], "nextCursor": "2"}
    elif method == "tools/call":
        args = msg["params"]["arguments"]
        if msg["params"]["name"] == "suma":
            result = {"content": [{"type": "text", "text": str(args["a"] + args["b"])}]}
        else:
            result = {"content": [{"type": "text", "text": "no sé"}], "isError": True}
    else:
        print(json.dumps({"jsonrpc": "2.0", "id": msg["id"], "error": {"code": -32601, "message": "nope"}}), flush=True)
        continue
    print(json.dumps({"jsonrpc": "2.0", "id": msg["id"], "result": result}), flush=True)
"#;

    #[test]
    fn talks_to_a_real_stdio_server() {
        let dir = std::env::temp_dir().join(format!("cocuyo-mcp-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let script = dir.join("falso.py");
        std::fs::write(&script, FAKE).unwrap();
        let spec = Spec { command: "python.exe".into(), args: vec![script.to_string_lossy().into_owned()], env: HashMap::new(), disabled: false };
        let Ok(mut server) = Server::start(&spec) else {
            eprintln!("sin python: se salta");
            return;
        };
        let tools = server.list_tools("prueba").unwrap();
        assert_eq!(tools.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["suma", "eco"], "lee todas las páginas");
        assert_eq!(tools[0].schema["required"], json!(["a", "b"]));
        with_state(|state| state.servers.insert("prueba".into(), server));
        assert_eq!(call("prueba", "suma", json!({ "a": 21, "b": 21 })).unwrap(), "42");
        assert!(call("prueba", "eco", json!({})).is_err());
        assert!(call("otro", "suma", json!({})).unwrap_err().contains("no está conectado"));
        with_state(|state| state.servers.remove("prueba"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
