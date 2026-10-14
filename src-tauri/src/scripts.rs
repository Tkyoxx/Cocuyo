// scripts del usuario por voz y la galeria. lo que se dice llega como dato, nunca como codigo

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::io::{BufRead, BufReader, Read};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::intent;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const PATIENCE: Duration = Duration::from_secs(20);
const KINDS: [&str; 6] = ["ps1", "py", "js", "bat", "cmd", "sh"];
const ASK_FOR: Duration = Duration::from_secs(150);
const SDK_PS: &str = include_str!("../../sdk/cocuyo.ps1");
const SDK_PY: &str = include_str!("../../sdk/cocuyo.py");
const SDK_JS: &str = include_str!("../../sdk/cocuyo.js");

static ASKED: Mutex<Option<(PathBuf, Instant)>> = Mutex::new(None);
static CARDS: AtomicU64 = AtomicU64::new(1);
const RUN: [&str; 12] = ["ejecuta", "ejecutar", "corre", "correr", "lanza", "script", "run", "execute", "launch", "roda", "rodar", "executa"];

const README: &str = "Scripts de Cocuyo\r\n\
=================\r\n\r\n\
Cada archivo de esta carpeta es una orden nueva para Cocuyo.\r\n\r\n\
- Vale PowerShell (.ps1), Python (.py), JavaScript con Node (.js), por lotes\r\n  (.bat, .cmd) y Bash (.sh).\r\n\
- El nombre del archivo es el nombre de la orden: \"copia de seguridad.ps1\" se\r\n  ejecuta diciendo \"ejecuta copia de seguridad\".\r\n\
- La primera línea de comentario cuenta para qué sirve. Si conectas un modelo de\r\n  IA en Ajustes, con decir algo que encaje con esa descripción ya lo encuentra.\r\n\
- Lo último que el script escriba en pantalla es lo que Cocuyo te responde.\r\n\
- Lo que digas después del nombre le llega como primer argumento, y también en\r\n  la variable COCUYO_DATO. La frase entera va en COCUYO_FRASE.\r\n\
- Con el SDK (carpeta sdk) puedes mostrar tarjetas, barras de progreso, hacer\r\n  preguntas y mucho más. Todo explicado en:\r\n  https://github.com/Tkyoxx/Cocuyo/tree/main/sdk\r\n\r\n\
Cocuyo solo ejecuta lo que tú pongas aquí. No pongas scripts que no entiendas.\r\n";

const EXAMPLE: &str = "# Dice qué hora es.\r\n\"Son las \" + (Get-Date -Format \"h:mm\") + \".\"\r\n";

#[derive(Clone, Serialize)]
pub struct Script {
    pub name: String,
    pub about: String,
    #[serde(skip)]
    pub path: PathBuf,
}

pub fn dir(app: &AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_config_dir().ok()?.join("scripts");
    if !dir.is_dir() {
        std::fs::create_dir_all(&dir).ok()?;
        let _ = std::fs::write(dir.join("di la hora.ps1"), EXAMPLE);
    }
    if std::fs::read_to_string(dir.join("LEEME.txt")).ok().as_deref() != Some(README) {
        let _ = std::fs::write(dir.join("LEEME.txt"), README);
    }
    sdk_dir(&dir);
    Some(dir)
}

fn sdk_dir(dir: &Path) -> PathBuf {
    let sdk = dir.join("sdk");
    let _ = std::fs::create_dir_all(&sdk);
    for (file, text) in [("cocuyo.ps1", SDK_PS), ("cocuyo.py", SDK_PY), ("cocuyo.js", SDK_JS)] {
        let path = sdk.join(file);
        if std::fs::read_to_string(&path).ok().as_deref() != Some(text) {
            let _ = std::fs::write(&path, text);
        }
    }
    sdk
}

fn about(text: &str) -> String {
    for line in text.lines().take(12) {
        let line = line.trim();
        let lower = line.to_lowercase();
        if line.starts_with("#!") || lower.starts_with("@echo") || lower.starts_with("# -*-") {
            continue;
        }
        let comment = ["#", "::", "//", "\"\"\"", "'''"].iter().find_map(|mark| line.strip_prefix(mark)).or_else(|| {
            lower.starts_with("rem ").then(|| &line[4..])
        });
        match comment.map(|c| c.trim().trim_end_matches("\"\"\"").trim()) {
            Some(text) if !text.is_empty() => return text.chars().take(160).collect(),
            Some(_) => continue,
            None if line.is_empty() => continue,
            None => break,
        }
    }
    String::new()
}

fn scripts_in(dir: &Path) -> Vec<Script> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut found: Vec<Script> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let kind = path.extension()?.to_string_lossy().to_lowercase();
            if !KINDS.contains(&kind.as_str()) || !path.is_file() {
                return None;
            }
            let name = path.file_stem()?.to_string_lossy().replace(['_', '-'], " ").trim().to_string();
            let head = std::fs::read(&path).ok().map(|bytes| String::from_utf8_lossy(&bytes[..bytes.len().min(2048)]).into_owned());
            Some(Script { name, about: about(head.as_deref().unwrap_or("").trim_start_matches('\u{feff}')), path })
        })
        .collect();
    found.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    found
}

pub fn list(app: &AppHandle) -> Vec<Script> {
    dir(app).map(|dir| scripts_in(&dir)).unwrap_or_default()
}

fn pick<'a>(scripts: &'a [Script], text: &str) -> Option<(&'a Script, String)> {
    let said: Vec<&str> = text.split_whitespace().collect();
    let mut best: Option<(&Script, usize, f32)> = None;
    for script in scripts {
        let words = script.name.split_whitespace().count().max(1);
        for take in [words, words + 1, words.saturating_sub(1)] {
            if take == 0 || take > said.len() {
                continue;
            }
            let score = intent::similarity(&said[..take].join(" "), &script.name);
            if score >= 0.8 && best.is_none_or(|(_, _, top)| score > top) {
                best = Some((script, take, score));
            }
        }
    }
    best.map(|(script, take, _)| (script, said[take..].join(" ")))
}

pub fn by_name(app: &AppHandle, spoken: &str) -> Option<Result<String, String>> {
    let clean = intent::normalize(spoken);
    let words: Vec<&str> = clean.split_whitespace().collect();
    let asked = words.first().is_some_and(|first| RUN.iter().any(|verb| intent::close(first, verb)));
    let rest = if asked { words[1..].join(" ") } else { clean.clone() };
    let rest = rest.trim_start_matches("el script ").trim_start_matches("script ").trim();
    if rest.is_empty() {
        return None;
    }
    let scripts = list(app);
    let (script, data) = pick(&scripts, rest)?;
    if !asked && !(data.is_empty() && intent::similarity(rest, &script.name) >= 0.92) {
        return None;
    }
    Some(run(app, script, &data, spoken, None))
}

pub fn named(app: &AppHandle, text: &str, spoken: &str) -> Result<String, String> {
    let scripts = list(app);
    match pick(&scripts, text.trim()) {
        Some((script, data)) => run(app, script, &data, spoken, None),
        None => Err(format!("No tengo ningún script llamado «{}».", text.trim())),
    }
}

fn command_for(script: &Script, data: &str, sdk: &Path) -> Command {
    let kind = script.path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let mut command = match kind.as_str() {
        "ps1" => {
            let literal = |text: &str| format!("'{}'", text.replace('\'', "''"));
            let mut line = format!(
                "[Console]::OutputEncoding = [Text.Encoding]::UTF8; . ([scriptblock]::Create([IO.File]::ReadAllText({}))); & ([scriptblock]::Create([IO.File]::ReadAllText({}))) ",
                literal(&sdk.join("cocuyo.ps1").to_string_lossy()),
                literal(&script.path.to_string_lossy())
            );
            if !data.is_empty() {
                line += &literal(data);
            }
            let mut command = Command::new("powershell.exe");
            command.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &line]);
            return command;
        }
        "py" => {
            let mut command = Command::new("python.exe");
            command.arg(&script.path).env("PYTHONUTF8", "1").env("PYTHONIOENCODING", "utf-8").env("PYTHONPATH", sdk);
            command
        }
        "js" => {
            let mut command = Command::new("node.exe");
            command.arg(&script.path).env("NODE_PATH", sdk);
            command
        }
        "sh" => {
            let mut command = Command::new("bash.exe");
            command.arg(&script.path);
            command
        }
        _ => Command::new(&script.path),
    };
    if !data.is_empty() {
        command.arg(data);
    }
    command
}

#[derive(Debug, PartialEq)]
enum Line {
    Plain(String),
    Sdk(Value),
}

fn read_line(line: &str) -> Option<Line> {
    let line = line.trim().trim_start_matches('\u{feff}');
    if line.is_empty() {
        return None;
    }
    if let Some(json) = line.strip_prefix("@cocuyo") {
        if let Ok(value @ Value::Object(_)) = serde_json::from_str::<Value>(json.trim()) {
            return Some(Line::Sdk(value));
        }
    }
    Some(Line::Plain(line.chars().take(220).collect()))
}

pub enum Event<'a> {
    Sdk(&'a Value, bool),
    Late(Result<String, String>),
}

pub fn execute(
    script: &Script,
    data: &str,
    spoken: &str,
    env: &[(&str, String)],
    sdk: &Path,
    on: impl FnMut(Event) -> Option<String> + Send + 'static,
) -> Result<String, String> {
    let name = script.name.clone();
    let mut command = command_for(script, data, sdk);
    command
        .env("COCUYO_FRASE", spoken)
        .env("COCUYO_DATO", data)
        .env("COCUYO_SDK", "2")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW);
    for (key, value) in env {
        command.env(key, value);
    }
    if let Some(dir) = script.path.parent() {
        command.current_dir(dir);
    }
    let mut child = command.spawn().map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => format!("Para ejecutar «{name}» falta instalar el programa que lo interpreta."),
        _ => format!("No pude ejecutar «{name}»: {e}"),
    })?;
    let stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let errors = std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(stderr) = stderr.as_mut() {
            let mut bytes = Vec::new();
            let _ = stderr.read_to_end(&mut bytes);
            text = String::from_utf8_lossy(&bytes).into_owned();
        }
        text
    });

    let late = Arc::new(AtomicBool::new(false));
    let late_reader = late.clone();
    let (done, wait) = std::sync::mpsc::channel();
    let fallback = name.clone();
    std::thread::spawn(move || {
        let mut on = on;
        let (mut last, mut chosen) = (None, None);
        if let Some(stdout) = stdout {
            for raw in BufReader::new(stdout).split(b'\n').map_while(Result::ok) {
                match read_line(&String::from_utf8_lossy(&raw)) {
                    Some(Line::Sdk(value)) => {
                        if let Some(reply) = on(Event::Sdk(&value, late_reader.load(Ordering::Relaxed))) {
                            chosen = Some(reply);
                        }
                    }
                    Some(Line::Plain(text)) => last = Some(text),
                    None => {}
                }
            }
        }
        let status = child.wait();
        let errors = errors.join().unwrap_or_default();
        let last_error = errors.lines().map(str::trim).filter(|line| !line.is_empty()).last().map(|line| line.chars().take(220).collect::<String>());
        let result = match status {
            Ok(status) if status.success() => Ok(chosen.or(last).unwrap_or_else(|| format!("Hecho: {fallback}."))),
            Ok(_) => Err(last_error.or(last).map_or_else(|| format!("«{fallback}» terminó con un error."), |why| format!("«{fallback}» falló: {why}"))),
            Err(e) => Err(format!("No pude ejecutar «{fallback}»: {e}")),
        };
        if late_reader.load(Ordering::Relaxed) {
            on(Event::Late(result));
        } else {
            let _ = done.send(result);
        }
    });
    match wait.recv_timeout(PATIENCE) {
        Ok(result) => result,
        Err(_) => {
            late.store(true, Ordering::Relaxed);
            match wait.try_recv() {
                Ok(result) => result,
                Err(_) => Ok(format!("«{name}» sigue trabajando.")),
            }
        }
    }
}

#[derive(Clone, Serialize)]
struct Card {
    id: u64,
    script: String,
    titulo: String,
    texto: String,
    imagen: String,
    icono: String,
    color: String,
    botones: Vec<Value>,
    filas: Vec<Value>,
    segundos: u64,
}

fn text_of(value: &Value, key: &str, limit: usize) -> String {
    value.get(key).and_then(Value::as_str).unwrap_or("").trim().chars().take(limit).collect()
}

fn picture(raw: &str) -> String {
    let raw = raw.trim();
    if raw.starts_with("https://") || (raw.starts_with("data:image/") && raw.len() < 4_000_000) {
        return raw.to_string();
    }
    let path = Path::new(raw);
    let kind = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let mime = match kind.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        _ => return String::new(),
    };
    match std::fs::read(path) {
        Ok(bytes) if bytes.len() < 3_000_000 => format!("data:{mime};base64,{}", crate::ai::base64(&bytes)),
        _ => String::new(),
    }
}

fn card_from(script: &str, value: &Value) -> Card {
    let color = text_of(value, "color", 9);
    let valid_color = color.len() >= 4 && color.starts_with('#') && color[1..].chars().all(|c| c.is_ascii_hexdigit());
    let action = |button: &Value| {
        let mut kept = serde_json::Map::new();
        for key in ["texto", "abrir", "ejecutar", "dato", "copiar"] {
            let limit = if key == "texto" { 30 } else { 2000 };
            let text = text_of(button, key, limit);
            if !text.is_empty() {
                kept.insert(key.into(), Value::String(text));
            }
        }
        kept.contains_key("texto").then_some(Value::Object(kept))
    };
    let row = |row: &Value| {
        let text = text_of(row, "texto", 80);
        (!text.is_empty()).then(|| json!({ "texto": text, "valor": text_of(row, "valor", 40), "barra": row.get("barra").and_then(Value::as_f64).map(|v| v.clamp(0.0, 1.0)) }))
    };
    Card {
        id: CARDS.fetch_add(1, Ordering::Relaxed),
        script: script.to_string(),
        titulo: text_of(value, "titulo", 80),
        texto: text_of(value, "texto", 500),
        imagen: picture(&text_of(value, "imagen", 4_000_000)),
        icono: text_of(value, "icono", 8),
        color: if valid_color { color } else { String::new() },
        botones: value.get("botones").and_then(Value::as_array).map(|all| all.iter().filter_map(action).take(4).collect()).unwrap_or_default(),
        filas: value.get("filas").and_then(Value::as_array).map(|all| all.iter().filter_map(row).take(8).collect()).unwrap_or_default(),
        segundos: value.get("segundos").and_then(Value::as_u64).unwrap_or(10).clamp(2, 120),
    }
}

fn handle(app: &AppHandle, script: &Script, value: &Value, late: bool) -> Option<String> {
    let mut reply = None;
    let tell = |text: &str, aloud: bool| {
        if late || aloud {
            let _ = app.emit_to(crate::overlay::LABEL, "note", text.to_string());
        }
        if aloud {
            crate::cue::answer(text);
        }
    };
    let aloud = value.get("hablar").and_then(Value::as_bool).unwrap_or(false);
    let said = text_of(value, "decir", 400);
    if !said.is_empty() {
        tell(&said, aloud);
        reply = Some(said);
    }
    if let Some(card) = value.get("tarjeta").filter(|card| card.is_object()) {
        let _ = app.emit_to(crate::overlay::LABEL, "script-card", card_from(&script.name, card));
    }
    if let Some(amount) = value.get("progreso") {
        let amount = amount.as_f64().map(|v| v.clamp(0.0, 1.0));
        let _ = app.emit_to(crate::overlay::LABEL, "script-progress", json!({ "script": script.name, "valor": amount, "texto": text_of(value, "texto", 80) }));
    }
    let question = text_of(value, "preguntar", 300);
    if !question.is_empty() {
        let question = if question.ends_with('?') { question } else { format!("{question}?") };
        *ASKED.lock().unwrap() = Some((script.path.clone(), Instant::now()));
        tell(&question, true);
        crate::ai::talked(&format!("(ejecuté «{}»)", script.name), &question);
        reply = Some(question);
    }
    let copy = text_of(value, "copiar", 100_000);
    if !copy.is_empty() {
        crate::clip::ignore_own_changes();
        crate::inject::set_clipboard_text(&copy);
    }
    let open = text_of(value, "abrir", 2000);
    if !open.is_empty() {
        let _ = crate::apps::open(&open);
    }
    let write = text_of(value, "escribir", 100_000);
    if !write.is_empty() {
        let _ = crate::inject::paste(&write);
    }
    let fact = text_of(value, "recordar", 300);
    if !fact.is_empty() {
        let _ = crate::ai::memorize(app, &fact);
    }
    if let Some(plan) = value.get("programar") {
        let (when, order) = (text_of(plan, "cuando", 80), text_of(plan, "orden", 300));
        if !when.is_empty() && !order.is_empty() {
            let _ = crate::agenda::schedule(app, &format!("{when} {order}"));
        }
    }
    reply
}

pub fn run(app: &AppHandle, script: &Script, data: &str, spoken: &str, answer: Option<&str>) -> Result<String, String> {
    let dir = dir(app).ok_or("No se encontró la carpeta de scripts.")?;
    let sdk = sdk_dir(&dir);
    let stem = script.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let keep = dir.join(".datos").join(&stem);
    let _ = std::fs::create_dir_all(&keep);
    let config = crate::settings::load(app);
    let env = [
        ("COCUYO_IDIOMA", config.language.clone()),
        ("COCUYO_NOMBRE", config.name.clone()),
        ("COCUYO_CIUDAD", config.city.clone()),
        ("COCUYO_CARPETA", keep.to_string_lossy().into_owned()),
        ("COCUYO_RESPUESTA", answer.unwrap_or("").to_string()),
        ("COCUYO_CONVERSACION", crate::ai::conversation()),
        ("COCUYO_VERSION", env!("CARGO_PKG_VERSION").to_string()),
    ];
    let (handle_app, mine) = (app.clone(), script.clone());
    let progressed = Arc::new(AtomicBool::new(false));
    let shown = progressed.clone();
    let result = execute(script, data, spoken, &env, &sdk, move |event| match event {
        Event::Sdk(value, late) => {
            if value.get("progreso").is_some() {
                shown.store(true, Ordering::Relaxed);
            }
            handle(&handle_app, &mine, value, late)
        }
        Event::Late(result) => {
            let text = match result {
                Ok(text) => text,
                Err(why) => why,
            };
            let _ = handle_app.emit_to(crate::overlay::LABEL, "note", text);
            if shown.load(Ordering::Relaxed) {
                let _ = handle_app.emit_to(crate::overlay::LABEL, "script-progress", json!({ "script": mine.name, "valor": 1.0, "texto": "" }));
            }
            None
        }
    });
    if progressed.load(Ordering::Relaxed) && !result.as_ref().is_ok_and(|text| text.ends_with("sigue trabajando.")) {
        let _ = app.emit_to(crate::overlay::LABEL, "script-progress", json!({ "script": script.name, "valor": 1.0, "texto": "" }));
    }
    result
}

pub fn answered(app: &AppHandle, spoken: &str) -> Option<Result<String, String>> {
    let (path, at) = ASKED.lock().unwrap().take()?;
    if at.elapsed() > ASK_FOR {
        return None;
    }
    let script = list(app).into_iter().find(|script| script.path == path)?;
    Some(run(app, &script, "", spoken, Some(spoken)))
}

#[derive(Deserialize)]
pub struct Action {
    abrir: Option<String>,
    copiar: Option<String>,
    ejecutar: Option<String>,
    dato: Option<String>,
}

#[tauri::command]
pub async fn script_action(app: AppHandle, action: Action) -> Result<String, String> {
    if let Some(target) = action.abrir.filter(|t| !t.trim().is_empty()) {
        crate::apps::open(target.trim())?;
        return Ok(String::new());
    }
    if let Some(text) = action.copiar.filter(|t| !t.is_empty()) {
        crate::clip::ignore_own_changes();
        crate::inject::set_clipboard_text(&text);
        return Ok("Copiado.".into());
    }
    if let Some(name) = action.ejecutar.filter(|t| !t.trim().is_empty()) {
        return script_run(app, name, action.dato.unwrap_or_default()).await;
    }
    Ok(String::new())
}

#[tauri::command]
pub async fn script_run(app: AppHandle, name: String, data: String) -> Result<String, String> {
    let scripts = list(&app);
    let script = scripts
        .iter()
        .find(|script| script.name.eq_ignore_ascii_case(name.trim()))
        .or_else(|| pick(&scripts, name.trim()).map(|(script, _)| script))
        .ok_or_else(|| format!("No tengo ningún script llamado «{}».", name.trim()))?;
    run(&app, script, &data, &format!("ejecuta {} {data}", script.name), None)
}

#[tauri::command]
pub fn say_aloud(text: String) {
    crate::cue::answer(&text.chars().take(400).collect::<String>());
}

const GALLERY: &str = "https://raw.githubusercontent.com/Tkyoxx/Cocuyo/main/galeria/";

#[derive(Clone, Serialize, serde::Deserialize)]
pub struct Shared {
    name: String,
    about: String,
    file: String,
    #[serde(default)]
    author: String,
    #[serde(default, skip_deserializing)]
    have: bool,
}

const PUBLISH: &str = "https://github.com/Tkyoxx/Cocuyo/issues/new?template=script.yml";
const LONGEST_LINK: usize = 1900;

fn a_login(name: &str) -> bool {
    !name.is_empty() && name.len() <= 39 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn escaped(text: &str) -> String {
    text.bytes().map(|b| if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.') { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

fn publish_link(file: &str, about: &str, code: &str) -> (String, bool) {
    let name = file.rsplit_once('.').map_or(file, |(name, _)| name);
    let head = format!("{PUBLISH}&title={}&archivo={}&que-hace={}", escaped(&format!("Script: {name}")), escaped(file), escaped(about));
    let whole = format!("{head}&codigo={}", escaped(code));
    if whole.len() <= LONGEST_LINK {
        (whole, true)
    } else {
        (head, false)
    }
}

fn fetch(url: &str) -> Result<Vec<u8>, String> {
    let output = Command::new("curl.exe")
        .args(["--silent", "--location", "--fail", "--max-time", "15", url])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err("No se pudo llegar a la galería. Revisa tu conexión.".into())
    }
}

fn safe_file(file: &str) -> bool {
    let kind = file.rsplit('.').next().unwrap_or("").to_lowercase();
    !file.is_empty()
        && file.len() <= 80
        && KINDS.contains(&kind.as_str())
        && !file.starts_with('.')
        && file.chars().all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.'))
        && !file.contains("..")
}

#[tauri::command]
pub async fn gallery_list(app: AppHandle) -> Result<Vec<Shared>, String> {
    let listed: Vec<Shared> = serde_json::from_slice(&fetch(&format!("{GALLERY}indice.json"))?).map_err(|_| "La galería respondió algo que no entiendo.".to_string())?;
    let mine = dir(&app);
    Ok(listed
        .into_iter()
        .filter(|shared| safe_file(&shared.file))
        .map(|mut shared| {
            if !a_login(&shared.author) {
                shared.author.clear();
            }
            shared.have = mine.as_ref().is_some_and(|dir| dir.join(&shared.file).is_file());
            shared
        })
        .collect())
}

#[tauri::command]
pub async fn gallery_publish(app: AppHandle, name: String) -> Result<bool, String> {
    let script = list(&app).into_iter().find(|script| script.name == name).ok_or("Ese script ya no está en tu carpeta.")?;
    let file = script.path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
    if !safe_file(&file) {
        return Err("Para subirlo, el nombre del archivo solo puede llevar letras, números, espacios y guiones.".into());
    }
    let code = std::fs::read_to_string(&script.path).map_err(|_| "No pude leer ese script.".to_string())?;
    let code = code.trim_start_matches('\u{feff}');
    if code.trim().is_empty() || code.len() > 20_000 {
        return Err("Ese script está vacío o es demasiado largo para la galería.".into());
    }
    let (link, whole) = publish_link(&file, &script.about, code);
    if !whole {
        crate::inject::set_clipboard_text(code);
    }
    crate::apps::open(&link)?;
    Ok(whole)
}

#[tauri::command]
pub async fn gallery_install(app: AppHandle, file: String) -> Result<String, String> {
    if !safe_file(&file) {
        return Err("Ese archivo no es de la galería.".into());
    }
    let target = dir(&app).ok_or("No se encontró la carpeta de configuración.")?.join(&file);
    if target.exists() {
        return Err("Ya tienes un script con ese nombre.".into());
    }
    let content = fetch(&format!("{GALLERY}{}", escaped(&file)))?;
    if content.is_empty() || content.len() > 200_000 {
        return Err("Ese script no se pudo descargar.".into());
    }
    std::fs::write(&target, content).map_err(|e| e.to_string())?;
    Ok(file.rsplit_once('.').map_or(file.clone(), |(name, _)| name.replace(['_', '-'], " ")))
}

#[tauri::command]
pub async fn scripts_list(app: AppHandle) -> Vec<Script> {
    list(&app)
}

#[tauri::command]
pub fn scripts_open(app: AppHandle) -> Result<(), String> {
    let dir = dir(&app).ok_or("No se encontró la carpeta de configuración.")?;
    crate::apps::open(&dir.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn script(name: &str) -> Script {
        Script { name: name.into(), about: String::new(), path: PathBuf::new() }
    }

    #[test]
    fn the_gallery_only_takes_plain_script_files() {
        assert!(safe_file("di la hora.ps1") && safe_file("modo_cine-2.py") && safe_file("café.bat"));
        for bad in ["..\\x.ps1", "../x.ps1", "a/b.ps1", "x.exe", "x", ".ps1", "x.ps1;calc", "C:\\x.ps1", ""] {
            assert!(!safe_file(bad), "{bad} no debería valer");
        }
    }

    #[test]
    fn publishing_opens_the_form_already_filled_in() {
        let (link, whole) = publish_link("di la hora.ps1", "Dice qué hora es.", "\"Son las \" + (Get-Date -Format \"h:mm\")\r\n");
        assert!(whole && link.starts_with("https://github.com/Tkyoxx/Cocuyo/issues/new?template=script.yml&title=Script%3A%20di%20la%20hora&archivo=di%20la%20hora.ps1&que-hace=Dice%20qu%C3%A9%20hora%20es."));
        assert!(link.contains("&codigo=%22Son%20las%20%22%20%2B%20%28Get-Date") && !link.contains(' ') && !link.contains('"'));
        let (link, whole) = publish_link("largo.ps1", "x", &"a = 1\n".repeat(600));
        assert!(!whole && !link.contains("codigo=") && link.len() < LONGEST_LINK);
        assert!(a_login("Tkyoxx") && a_login("un-nombre-9") && !a_login("") && !a_login("dos palabras") && !a_login("a/b") && !a_login(&"x".repeat(40)));
    }

    #[test]
    fn the_first_comment_says_what_it_does() {
        assert_eq!(about("# Dice qué hora es.\n\"Son las\""), "Dice qué hora es.");
        assert_eq!(about("#!/usr/bin/env python\n# -*- coding: utf-8 -*-\n\n# Copia mis documentos.\nimport os"), "Copia mis documentos.");
        assert_eq!(about("@echo off\r\nREM Apaga las luces\r\ncurl x"), "Apaga las luces");
        assert_eq!(about(":: Sube el brillo\r\n"), "Sube el brillo");
        assert_eq!(about("\"\"\"Resume el portapapeles.\"\"\"\nimport sys"), "Resume el portapapeles.");
        assert_eq!(about("import os\n# tarde"), "");
    }

    #[test]
    fn a_script_is_found_by_its_name_and_keeps_its_data() {
        let scripts = [script("copia de seguridad"), script("di la hora"), script("luces")];
        let (found, data) = pick(&scripts, "copia de seguridad fotos del viaje").unwrap();
        assert_eq!((found.name.as_str(), data.as_str()), ("copia de seguridad", "fotos del viaje"));
        let (found, data) = pick(&scripts, "di la ora").unwrap();
        assert_eq!((found.name.as_str(), data.as_str()), ("di la hora", ""));
        let (found, data) = pick(&scripts, "luces rojo").unwrap();
        assert_eq!((found.name.as_str(), data.as_str()), ("luces", "rojo"));
        assert!(pick(&scripts, "pon rock en español").is_none());
    }

    fn quiet(_: Event) -> Option<String> {
        None
    }

    #[test]
    fn a_powershell_script_runs_and_answers() {
        let dir = std::env::temp_dir().join(format!("cocuyo-scripts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let sdk = sdk_dir(&dir);
        let path = dir.join("eco de prueba.ps1");
        std::fs::write(&path, "# Repite lo que le dicen.\r\nparam($dato)\r\n\"ruido\"\r\n\"Dijiste: $dato / $env:COCUYO_DATO / año\"\r\n").unwrap();
        let scripts = scripts_in(&dir);
        assert_eq!((scripts[0].name.as_str(), scripts[0].about.as_str()), ("eco de prueba", "Repite lo que le dicen."));

        let tricky = "it's $(Get-Date); rm x";
        assert_eq!(execute(&scripts[0], tricky, "frase", &[], &sdk, quiet).unwrap(), format!("Dijiste: {tricky} / {tricky} / año"));

        std::fs::write(dir.join("roto.ps1"), "Write-Error 'no hay disco'; exit 3").unwrap();
        let broken = scripts_in(&dir).into_iter().find(|s| s.name == "roto").unwrap();
        assert!(execute(&broken, "", "", &[], &sdk, quiet).unwrap_err().starts_with("«roto» falló"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sdk_lines_are_read_apart_from_plain_text() {
        assert_eq!(read_line("  hola  "), Some(Line::Plain("hola".into())));
        assert_eq!(read_line(""), None);
        assert_eq!(read_line("@cocuyo {\"decir\":\"hey\"}"), Some(Line::Sdk(json!({ "decir": "hey" }))));
        assert_eq!(read_line("@cocuyo esto no es json"), Some(Line::Plain("@cocuyo esto no es json".into())));
        let card = card_from("x", &json!({ "titulo": "Hola", "color": "#ff00aa", "botones": [{ "texto": "Ir", "abrir": "https://a.b" }, { "abrir": "sin texto" }], "filas": [{ "texto": "uno", "valor": "1", "barra": 3.0 }], "segundos": 999 }));
        assert_eq!((card.titulo.as_str(), card.color.as_str(), card.botones.len(), card.segundos), ("Hola", "#ff00aa", 1, 120));
        assert_eq!(card.filas[0]["barra"], json!(1.0));
        assert_eq!(card_from("x", &json!({ "color": "red; background:url(x)" })).color, "");
    }

    #[test]
    fn the_sdk_speaks_from_every_language() {
        let dir = std::env::temp_dir().join(format!("cocuyo-sdk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let sdk = sdk_dir(&dir);
        std::fs::write(dir.join("tarjeta ps.ps1"), "# Prueba.\r\nTarjeta -Titulo \"Dólar\" -Texto $Cocuyo.Nombre -Botones @(Boton -Texto 'Ver' -Abrir 'https://x.y')\r\nProgreso 0.5 'mitad'\r\nDecir 'listo ps'\r\n").unwrap();
        std::fs::write(dir.join("tarjeta py.py"), "# Prueba.\nimport cocuyo\ncocuyo.tarjeta('Clima', cocuyo.nombre, botones=[cocuyo.boton('Ver', abrir='https://x.y')])\ncocuyo.decir('listo py')\n").unwrap();
        std::fs::write(dir.join("tarjeta js.js"), "// Prueba.\nconst cocuyo = require('cocuyo');\ncocuyo.tarjeta({ titulo: 'Hora', texto: cocuyo.nombre });\ncocuyo.decir('listo js');\n").unwrap();
        for script in scripts_in(&dir) {
            let seen = Arc::new(Mutex::new(Vec::new()));
            let kept = seen.clone();
            let env = [("COCUYO_NOMBRE", "Valentina".to_string())];
            let result = execute(&script, "", "", &env, &sdk, move |event| match event {
                Event::Sdk(value, _) => {
                    kept.lock().unwrap().push(value.clone());
                    value.get("decir").and_then(Value::as_str).map(str::to_owned)
                }
                Event::Late(_) => None,
            });
            let kind = script.name.rsplit(' ').next().unwrap().to_string();
            match result {
                Err(why) if why.contains("falta instalar") => continue,
                result => assert_eq!(result.unwrap(), format!("listo {kind}"), "{}", script.name),
            }
            let seen = seen.lock().unwrap();
            let card = seen.iter().find(|v| v.get("tarjeta").is_some()).expect("sin tarjeta");
            assert_eq!(card["tarjeta"]["texto"], json!("Valentina"), "{}", script.name);
            if kind == "ps" {
                assert_eq!(card["tarjeta"]["titulo"], json!("Dólar"));
                assert_eq!(card["tarjeta"]["botones"][0]["abrir"], json!("https://x.y"));
                assert!(seen.iter().any(|v| v["progreso"] == json!(0.5)));
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
