// la IA. no manda en nada: convierte lo que dijiste en una orden de las de siempre y ya

use std::io::Write;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::scripts::Script;
use crate::settings;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const PATIENCE_SECONDS: &str = "60";
const RESTS: [Duration; 3] = [Duration::from_secs(60), Duration::from_secs(600), Duration::from_secs(3600)];
const MAX_TOKENS: u32 = 400;
const REMEMBERED: usize = 300;
const THRIFTY_FROM: f32 = 0.6;
const VOICE_DAILY: u32 = 9;
const VOICE_MODELS: [&str; 4] = ["gemini-3.8-flash-lite-tts", "gemini-3.8-flash-tts", "gemini-3.1-flash-tts-preview", "gemini-2.5-flash-preview-tts"];

static NO_EFFORT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static PLAIN_ONLY: Mutex<Vec<String>> = Mutex::new(Vec::new());
static TURN: AtomicUsize = AtomicUsize::new(0);
static RESTING: Mutex<Vec<(String, Instant, usize)>> = Mutex::new(Vec::new());
static USAGE: Mutex<(String, Vec<(String, u32)>)> = Mutex::new((String::new(), Vec::new()));
static TALK: Mutex<Vec<(Instant, String, String)>> = Mutex::new(Vec::new());
const TALK_FOR: Duration = Duration::from_secs(30 * 60);
const TALK_TURNS: usize = 16;
const ANSWER_WITHIN: Duration = Duration::from_secs(3 * 60);
static ANSWERS: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

fn key_path(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_config_dir().ok()?.join("ia.bin"))
}

pub(crate) fn key(app: &AppHandle) -> Option<String> {
    let sealed = std::fs::read(key_path(app)?).ok()?;
    String::from_utf8(crate::spotify::protect(&sealed, true)?).ok().filter(|k| !k.is_empty())
}

#[tauri::command]
pub fn ai_key_set(app: AppHandle, key: String) -> Result<(), String> {
    let path = key_path(&app).ok_or("No se encontró la carpeta de configuración.")?;
    let key = key.trim();
    if key.is_empty() {
        let _ = std::fs::remove_file(path);
        return Ok(());
    }
    let sealed = crate::spotify::protect(key.as_bytes(), false).ok_or("Windows no pudo cifrar la clave.")?;
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(path, sealed).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn ai_key_saved(app: AppHandle) -> bool {
    key(&app).is_some()
}

pub fn mode(app: &AppHandle) -> Option<bool> {
    let config = settings::load(app);
    if !config.ai_on || config.ai_url.trim().is_empty() || models(&config.ai_model).is_empty() {
        return None;
    }
    let (used, budget) = budget(app);
    Some(config.ai_always && (budget == 0 || (used as f32) < budget as f32 * THRIFTY_FROM))
}

pub fn budget(app: &AppHandle) -> (u32, u32) {
    let config = settings::load(app);
    let chosen = models(&config.ai_model);
    let used = with_usage(app, |counts| counts.iter().filter(|(name, _)| chosen.contains(name)).map(|(_, used)| *used).sum());
    (used, config.ai_daily * chosen.len() as u32)
}

#[tauri::command]
pub fn ai_budget(app: AppHandle) -> (u32, u32) {
    budget(&app)
}

fn spent(app: &AppHandle, model: &str) -> bool {
    let cap = settings::load(app).ai_daily;
    cap > 0 && with_usage(app, |counts| counts.iter().any(|(name, used)| name == model && *used >= cap))
}

const FORMS: &str = "pon <canción> de <artista>
pon <canción o artista>
pon música <género o ambiente>
pon mi lista <nombre>
pon algo al azar
pausa
sigue
siguiente
anterior
otra vez
volumen al <número de 0 a 100>
sube el volumen
baja el volumen
silencia
abre <app o página web>
busca <algo> en youtube
busca <algo> en google
anota <texto>
toma una captura
bloquea el equipo
recuerda que <un dato sobre la persona que pide que no olvides>
recuérdame en <N> minutos <qué>
recuérdame a las <hora:minutos> <qué>
a las <hora:minutos> <cualquiera de las órdenes de arriba>
todos los días a las <hora:minutos> <orden>
al encender la pc <orden>
responde <respuesta corta, de una o dos frases, en el idioma de la persona>";

pub fn talked(said: &str, answer: &str) {
    let mut talk = TALK.lock().unwrap();
    talk.retain(|(when, _, _)| when.elapsed() < TALK_FOR);
    if talk.len() >= TALK_TURNS {
        talk.remove(0);
    }
    talk.push((Instant::now(), said.trim().to_string(), answer.trim().chars().take(400).collect()));
}

pub fn asking() -> bool {
    let talk = TALK.lock().unwrap();
    talk.last().is_some_and(|(when, _, answer)| when.elapsed() < ANSWER_WITHIN && answer.trim_end().ends_with('?'))
}

pub fn conversation() -> String {
    let talk = TALK.lock().unwrap();
    talk.iter().filter(|(when, _, _)| when.elapsed() < TALK_FOR).map(|(_, said, answer)| format!("Persona: {said}\nCocuyo: {answer}")).collect::<Vec<_>>().join("\n")
}

fn recent() -> String {
    let talk = TALK.lock().unwrap();
    let lines: Vec<String> = talk.iter().filter(|(when, _, _)| when.elapsed() < TALK_FOR).map(|(_, said, answer)| format!("Persona: {said}\nCocuyo: {answer}")).collect();
    if lines.is_empty() {
        return String::new();
    }
    let mut text = format!(
        "\n\nLo que llevan hablando (lo más viejo arriba). Úsalo: la frase nueva casi siempre sigue esta conversación, \
         y si habla de «eso», «lo de antes» o «la otra», se refiere a algo de aquí:\n{}",
        lines.join("\n")
    );
    if talk.last().is_some_and(|(when, _, answer)| when.elapsed() < ANSWER_WITHIN && answer.trim_end().ends_with('?')) {
        text.push_str(
            "\n\nCocuyo acaba de hacerle una pregunta. La frase nueva es la RESPUESTA a esa pregunta: júntala con lo que la persona \
             quería antes y devuelve la orden completa (o «responde …» si toca seguir conversando). No la trates como una orden suelta.",
        );
    }
    text
}

fn memory_path(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_config_dir().ok()?.join("memoria.txt"))
}

pub fn memory(app: &AppHandle) -> Vec<String> {
    let text = memory_path(app).and_then(|path| std::fs::read_to_string(path).ok()).unwrap_or_default();
    text.lines().map(str::trim).filter(|line| !line.is_empty()).map(str::to_string).collect()
}

fn save_memory(app: &AppHandle, facts: &[String]) -> Result<(), String> {
    let path = memory_path(app).ok_or("No se encontró la carpeta de configuración.")?;
    std::fs::write(path, facts.join("\r\n")).map_err(|e| e.to_string())
}

pub fn memorize(app: &AppHandle, fact: &str) -> Result<(), String> {
    let mut facts = memory(app);
    facts.push(fact.trim().chars().take(240).collect());
    let extra = facts.len().saturating_sub(REMEMBERS);
    save_memory(app, &facts[extra..])
}

pub fn forget(app: &AppHandle, about: Option<&str>) -> Result<usize, String> {
    let facts = memory(app);
    let kept: Vec<String> = match about {
        None => Vec::new(),
        Some(about) => facts.iter().filter(|fact| crate::intent::covers(about, fact) < 0.6).cloned().collect(),
    };
    save_memory(app, &kept)?;
    Ok(facts.len() - kept.len())
}

const REMEMBERS: usize = 300;
const RECENT_FACTS: usize = 10;
const RELATED_FACTS: usize = 14;

fn facts_for<'a>(facts: &'a [String], said: &str) -> Vec<&'a String> {
    let recent_from = facts.len().saturating_sub(if said.is_empty() { RECENT_FACTS + RELATED_FACTS } else { RECENT_FACTS });
    let said = crate::intent::normalize(said);
    let keys: Vec<&str> = said.split_whitespace().filter(|word| word.chars().count() >= 4).map(|word| &word[..word.char_indices().nth(5).map_or(word.len(), |(at, _)| at)]).collect();
    let related = facts[..recent_from].iter().rev().filter(|fact| {
        let fact = crate::intent::normalize(fact);
        keys.iter().any(|key| fact.contains(key))
    });
    related.take(RELATED_FACTS).chain(facts[recent_from..].iter()).collect()
}

#[tauri::command]
pub fn memory_list(app: AppHandle) -> Vec<String> {
    memory(&app)
}

#[tauri::command]
pub fn memory_add(app: AppHandle, fact: String) -> Result<(), String> {
    if fact.trim().is_empty() {
        return Ok(());
    }
    memorize(&app, &fact)
}

#[tauri::command]
pub fn memory_remove(app: AppHandle, index: usize) -> Result<(), String> {
    let mut facts = memory(&app);
    if index < facts.len() {
        facts.remove(index);
    }
    save_memory(&app, &facts)
}

fn about_the_person(app: &AppHandle, said: &str) -> String {
    let config = settings::load(app);
    let mut text = String::new();
    if !config.name.trim().is_empty() {
        text.push_str(&format!("\n\nLa persona se llama {}. Puedes llamarla por su nombre de vez en cuando.", config.name.trim().chars().take(40).collect::<String>()));
    }
    let facts = memory(app);
    let chosen = facts_for(&facts, said);
    if !chosen.is_empty() {
        text.push_str("\n\nDatos que la persona te pidió recordar (úsalos al responder):\n");
        for fact in chosen {
            text.push_str(&format!("- {fact}\n"));
        }
    }
    if !config.ai_persona.trim().is_empty() {
        text.push_str(&format!("\n\nAl usar «responde», habla así: {}", config.ai_persona.trim().chars().take(300).collect::<String>()));
    }
    text
}

fn instructions(scripts: &[Script], agent: bool) -> String {
    let mut text = String::from(
        "Eres el intérprete de órdenes de un asistente de voz para Windows. Recibes lo que la persona dijo, transcrito por una máquina: \
         puede traer errores de oído, palabras pegadas, nombres mal escritos, o venir en otro idioma.\n\
         Responde con UNA sola línea y nada más: la orden que quiso dar, escrita con una de las formas de abajo. \
         Escribe los nombres de canciones, artistas y apps como se escriben de verdad. Sin explicaciones y sin comillas.\n\n",
    );
    text.push_str(FORMS);
    if !scripts.is_empty() {
        text.push_str("\nscript <nombre> <dato opcional>\n\nScripts de la persona (elige uno cuando lo dicho encaje con lo que hace):\n");
        for script in scripts.iter().take(40) {
            text.push_str(&format!("- {}: {}\n", script.name, if script.about.is_empty() { "sin descripción" } else { &script.about }));
        }
    }
    if agent {
        text.push_str(
            "\nagente <la tarea completa, con todos los datos que dio la persona>\n\n\
             Usa «agente» solo cuando lo pedido no cabe en una de las formas de arriba: necesita varios pasos seguidos, manejar una app con el \
             mouse y el teclado (mandar un mensaje por WhatsApp, Discord o Telegram, llenar algo, cambiar un ajuste de una app), leer lo que hay \
             en pantalla, o buscar y combinar información. Si cabe en una forma, usa la forma: es mucho más rápido.\n",
        );
    }
    text.push_str(
        "\nUsa «responde» solo cuando sea una pregunta o un comentario y no una orden (¿cuánto es 15 por 12?, ¿cómo se dice gato en inglés?). \
         Si no es ninguna de esas formas, escribe solo: nada",
    );
    text
}

pub(crate) fn base64(bytes: &[u8]) -> String {
    const LETTERS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let group = u32::from(chunk[0]) << 16 | u32::from(*chunk.get(1).unwrap_or(&0)) << 8 | u32::from(*chunk.get(2).unwrap_or(&0));
        for (index, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            out.push(if index <= chunk.len() { LETTERS[(group >> shift & 63) as usize] as char } else { '=' });
        }
    }
    out
}

fn send(url: &str, headers: &[String], body: Option<&Value>) -> Result<(u16, Value), String> {
    let quote = |text: &str| text.replace('\\', "\\\\").replace('"', "\\\"");
    let mut config = format!("url = \"{}\"\nwrite-out = \"\\n%{{http_code}}\"\n", quote(url));
    for header in headers {
        config += &format!("header = \"{}\"\n", quote(header));
    }
    let mut parked = None;
    if let Some(body) = body {
        let body = body.to_string();
        config += "request = \"POST\"\nheader = \"Content-Type: application/json\"\n";
        if body.len() > 200_000 {
            let path = std::env::temp_dir().join(format!("cocuyo-ia-{}.json", std::process::id()));
            std::fs::write(&path, &body).map_err(|e| e.to_string())?;
            config += &format!("data-binary = \"@{}\"\n", quote(&path.to_string_lossy()));
            parked = Some(path);
        } else {
            config += &format!("data = \"{}\"\n", quote(&body));
        }
    }

    let mut child = Command::new("curl.exe")
        .args(["--silent", "--show-error", "--max-time", PATIENCE_SECONDS, "--connect-timeout", "6", "--config", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| e.to_string())?;
    child.stdin.take().ok_or("sin entrada")?.write_all(config.as_bytes()).map_err(|e| e.to_string())?;
    let output = child.wait_with_output();
    if let Some(path) = parked {
        let _ = std::fs::remove_file(path);
    }
    let output = output.map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&output.stdout);
    let status = text.rsplit_once('\n').and_then(|(json, status)| Some((status.trim().parse::<u16>().ok()?, json)));
    match status {
        Some((status, json)) if status > 0 => Ok((status, serde_json::from_str(json).unwrap_or(Value::Null))),
        _ => Err("No hay respuesta en esa dirección. ¿Está encendido el servicio?".into()),
    }
}

fn endpoint(base: &str, tail: &str) -> String {
    let base = base.trim().trim_end_matches('/');
    if base.ends_with(tail) {
        base.to_string()
    } else {
        format!("{base}{tail}")
    }
}

fn headers(anthropic: bool, key: Option<&str>) -> Vec<String> {
    match (anthropic, key) {
        (true, Some(key)) => vec!["anthropic-version: 2023-06-01".into(), format!("x-api-key: {key}")],
        (true, None) => vec!["anthropic-version: 2023-06-01".into()],
        (false, Some(key)) => vec![format!("Authorization: Bearer {key}")],
        (false, None) => Vec::new(),
    }
}

fn refusal(status: u16, reply: &Value, model: &str) -> String {
    let why = reply["error"]["message"].as_str().or(reply["error"].as_str()).or(reply["message"].as_str()).unwrap_or("");
    match status {
        401 | 403 => "El servicio rechazó la clave.".to_string(),
        404 => format!("El servicio no conoce el modelo «{model}» o esa dirección."),
        429 => format!("«{model}» llegó al límite de su plan. Lo dejo descansar."),
        500..=599 => format!("El servicio de «{model}» está saturado ahora mismo. Prueba en un rato."),
        _ if why.is_empty() => format!("El servicio respondió con el error {status}."),
        _ => format!("El servicio respondió: {}", why.chars().take(160).collect::<String>()),
    }
}

fn models(field: &str) -> Vec<String> {
    field.split([',', ';', '\n']).map(str::trim).filter(|model| !model.is_empty()).map(str::to_string).collect()
}

fn today() -> String {
    let now = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    format!("{:04}-{:02}-{:02}", now.wYear, now.wMonth, now.wDay)
}

fn usage_path(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_config_dir().ok()?.join("ia-uso.json"))
}

fn with_usage<T>(app: &AppHandle, change: impl FnOnce(&mut Vec<(String, u32)>) -> T) -> T {
    let mut usage = USAGE.lock().unwrap();
    let today = today();
    if usage.0 != today {
        let saved: Option<(String, Vec<(String, u32)>)> = usage_path(app).and_then(|path| std::fs::read(path).ok()).and_then(|bytes| serde_json::from_slice(&bytes).ok());
        *usage = match saved {
            Some(saved) if saved.0 == today => saved,
            _ => (today, Vec::new()),
        };
    }
    change(&mut usage.1)
}

fn count(app: &AppHandle, model: &str) {
    with_usage(app, |counts| match counts.iter_mut().find(|(name, _)| name == model) {
        Some((_, used)) => *used += 1,
        None => counts.push((model.to_string(), 1)),
    });
    if let (Some(path), Ok(json)) = (usage_path(app), serde_json::to_vec(&*USAGE.lock().unwrap())) {
        let _ = std::fs::write(path, json);
    }
}

#[tauri::command]
pub fn ai_usage(app: AppHandle) -> Vec<(String, u32)> {
    with_usage(&app, |counts| counts.clone())
}

fn resting(model: &str) -> bool {
    RESTING.lock().unwrap().iter().any(|(name, until, _)| name == model && Instant::now() < *until)
}

fn rest(model: &str) {
    let mut resting = RESTING.lock().unwrap();
    let strikes = resting.iter().find(|(name, _, _)| name == model).map_or(0, |(_, _, strikes)| *strikes + 1);
    resting.retain(|(name, _, _)| name != model);
    resting.push((model.to_string(), Instant::now() + RESTS[strikes.min(RESTS.len() - 1)], strikes));
}

enum Extra<'a> {
    None,
    Image(&'a [u8]),
    Audio(&'a [u8]),
}

fn chat(app: &AppHandle, system: &str, said: &str, extra: Extra) -> Result<String, String> {
    chat_up_to(app, system, said, extra, MAX_TOKENS)
}

fn chat_up_to(app: &AppHandle, system: &str, said: &str, extra: Extra, room: u32) -> Result<String, String> {
    let config = settings::load(app);
    let key = key(app);
    let anthropic = config.ai_format == "anthropic";
    let headers = headers(anthropic, key.as_deref());
    let url = endpoint(&config.ai_url, if anthropic { "/messages" } else { "/chat/completions" });
    let user = match (&extra, anthropic) {
        (Extra::None, _) => json!(said),
        (Extra::Image(png), true) => json!([{ "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": base64(png) } }, { "type": "text", "text": said }]),
        (Extra::Image(png), false) => json!([{ "type": "text", "text": said }, { "type": "image_url", "image_url": { "url": format!("data:image/png;base64,{}", base64(png)) } }]),
        (Extra::Audio(_), true) => return Err("Ese servicio no escucha audio.".into()),
        (Extra::Audio(wav), false) => json!([{ "type": "text", "text": said }, { "type": "input_audio", "input_audio": { "data": base64(wav), "format": "wav" } }]),
    };
    let brisk = url.contains("googleapis.com") && !NO_EFFORT.load(Ordering::Relaxed);
    let messages = json!([{ "role": "system", "content": system }, { "role": "user", "content": user }]);
    let text_of = |reply: &Value| {
        let blocks = reply["content"].as_array().and_then(|blocks| blocks.iter().find_map(|block| block["text"].as_str()));
        reply["choices"][0]["message"]["content"].as_str().or(blocks).unwrap_or("").to_string()
    };
    let cut_short = |reply: &Value| reply["choices"][0]["finish_reason"] == "length" || reply["stop_reason"] == "max_tokens";

    let all = models(&config.ai_model);
    let first = TURN.fetch_add(1, Ordering::Relaxed);
    let mut last_error = None;
    for model in (0..all.len()).map(|step| &all[(first + step) % all.len()]).filter(|model| !resting(model) && !spent(app, model)) {
        let ask = |limit: bool| {
            let body = match (anthropic, limit) {
                (true, _) => json!({ "model": model, "max_tokens": if limit { room } else { 4 * room }, "system": system, "messages": [{ "role": "user", "content": user }] }),
                (false, true) if brisk => json!({ "model": model, "messages": messages, "temperature": 0, "max_tokens": room, "stream": false, "reasoning_effort": "minimal" }),
            (false, true) => json!({ "model": model, "messages": messages, "temperature": 0, "max_tokens": room, "stream": false }),
                (false, false) => json!({ "model": model, "messages": messages }),
            };
            send(&url, &headers, Some(&body))
        };
        let plain = PLAIN_ONLY.lock().unwrap().contains(model);
        let (mut status, mut reply) = ask(!plain)?;
        count(app, model);
        if status == 400 && brisk {
            NO_EFFORT.store(true, Ordering::Relaxed);
        }
        if cfg!(debug_assertions) {
            eprintln!("[ia] {model}: {status}");
        }
        if status == 400 && !plain {
            PLAIN_ONLY.lock().unwrap().push(model.clone());
        }
        if (status == 400 && !plain) || ((200..300).contains(&status) && (cut_short(&reply) || text_of(&reply).trim().is_empty())) {
            (status, reply) = ask(false)?;
            count(app, model);
        }
        if (200..300).contains(&status) {
            RESTING.lock().unwrap().retain(|(name, _, _)| name != model);
            return Ok(text_of(&reply));
        }
        last_error = Some(refusal(status, &reply, model));
        if status >= 500 {
            continue;
        }
        if status != 429 {
            break;
        }
        rest(model);
    }
    Err(last_error.unwrap_or_else(|| "Hoy ya se usó el cupo de los modelos; hasta mañana sigo con lo que entiendo solo.".into()))
}

pub fn chat_tools(app: &AppHandle, messages: &[Value], tools: &[Value]) -> Result<Value, String> {
    let config = settings::load(app);
    if config.ai_format == "anthropic" {
        return Err("El modo agente necesita un servicio con formato OpenAI: Gemini, Groq, Ollama, LM Studio, OpenAI o DeepSeek.".into());
    }
    let key = key(app);
    let headers = headers(false, key.as_deref());
    let url = endpoint(&config.ai_url, "/chat/completions");
    let brisk = url.contains("googleapis.com") && !NO_EFFORT.load(Ordering::Relaxed);
    let all = models(&config.ai_model);
    let first = TURN.fetch_add(1, Ordering::Relaxed);
    let mut last_error = None;
    for model in (0..all.len()).map(|step| &all[(first + step) % all.len()]).filter(|model| !resting(model) && !spent(app, model)) {
        let mut body = json!({ "model": model, "messages": messages, "temperature": 0.2, "max_tokens": 1500, "stream": false });
        if !tools.is_empty() {
            body["tools"] = json!(tools);
            body["tool_choice"] = json!("auto");
        }
        if brisk {
            body["reasoning_effort"] = json!("minimal");
        }
        let (mut status, mut reply) = send(&url, &headers, Some(&body))?;
        count(app, model);
        if status == 400 && brisk {
            NO_EFFORT.store(true, Ordering::Relaxed);
            body.as_object_mut().map(|body| body.remove("reasoning_effort"));
            (status, reply) = send(&url, &headers, Some(&body))?;
            count(app, model);
        }
        if (200..300).contains(&status) {
            RESTING.lock().unwrap().retain(|(name, _, _)| name != model);
            let message = reply["choices"][0]["message"].clone();
            if message.is_null() {
                last_error = Some("El modelo respondió algo vacío.".to_string());
                continue;
            }
            return Ok(message);
        }
        last_error = Some(refusal(status, &reply, model));
        if status >= 500 {
            std::thread::sleep(Duration::from_millis(1500));
            let (again, retried) = send(&url, &headers, Some(&body))?;
            count(app, model);
            if (200..300).contains(&again) && !retried["choices"][0]["message"].is_null() {
                return Ok(retried["choices"][0]["message"].clone());
            }
            continue;
        }
        if status != 429 {
            break;
        }
        rest(model);
    }
    Err(last_error.unwrap_or_else(|| "Hoy ya se usó el cupo de los modelos; hasta mañana sigo con lo que entiendo solo.".into()))
}

pub fn agent_context(app: &AppHandle, said: &str) -> String {
    format!("{}{}", about_the_person(app, said).replace("Al usar «responde», habla así", "Habla así"), recent())
}

pub fn hear(app: &AppHandle, wav: &[u8], dictation: bool, scripts: &[Script]) -> Result<(String, String), String> {
    let language = match crate::cue::language().0 {
        "en" => "inglés",
        "pt" => "portugués",
        _ => "español",
    };
    if dictation {
        let system = format!(
            "Transcribe lo que la persona dicta en el audio y devuelve solo ese texto, listo para pegar: con su puntuación y mayúsculas, \
             sin muletillas ni arranques en falso y, si se corrige a mitad de frase, solo con la versión final. No añadas nada, no \
             contestes a lo que dice y no lo traduzcas. Suele hablar en {language}."
        );
        let text = chat(app, &system, "(el dictado está en el audio)", Extra::Audio(wav))?;
        let text = text.split("</think>").last().unwrap_or("").trim().to_string();
        return if text.is_empty() { Err("El modelo no oyó nada.".into()) } else { Ok((text.clone(), text)) };
    }
    let system = format!(
        "{}{}{}\n\nEsta vez lo que dijo la persona no viene escrito: está en el audio. Suele hablar en {language}. Responde con DOS líneas exactas y nada más:\n\
         dicho: <lo que dijo, transcrito tal cual, en su idioma>\norden: <la orden, con una de las formas de arriba>",
        instructions(scripts, crate::agent::wanted(app)),
        about_the_person(app, ""),
        recent()
    );
    let reply = chat(app, &system, "(la orden está en el audio)", Extra::Audio(wav))?;
    let (said, order) = heard_in(&reply);
    if said.is_empty() && order.is_empty() {
        return Err("El modelo no oyó nada.".into());
    }
    Ok((said, order))
}

fn heard_in(reply: &str) -> (String, String) {
    let reply = reply.split("</think>").last().unwrap_or("");
    let field = |name: &str| {
        reply.lines().map(str::trim).find_map(|line| {
            let lower = line.to_lowercase();
            lower.starts_with(name).then(|| line[name.len()..].trim().to_string())
        })
    };
    let said = field("dicho:").unwrap_or_default();
    let order = field("orden:").unwrap_or_else(|| reply.lines().map(str::trim).filter(|line| !line.is_empty() && !line.to_lowercase().starts_with("dicho:")).last().unwrap_or("").to_string());
    (said.trim_matches(|c: char| matches!(c, '"' | '«' | '»')).to_string(), order_in(&order))
}

pub(crate) fn unbase64(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let (mut group, mut bits) = (0u32, 0);
    for value in text.bytes().filter_map(|c| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' | b'-' => Some(62),
        b'/' | b'_' => Some(63),
        _ => None,
    }) {
        group = group << 6 | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((group >> bits) as u8);
        }
    }
    out
}

fn audio_in(reply: &Value) -> Option<&str> {
    match reply {
        Value::Object(fields) => fields.iter().filter_map(|(name, value)| if name == "data" { value.as_str() } else { audio_in(value) }).max_by_key(|data| data.len()),
        Value::Array(items) => items.iter().filter_map(audio_in).max_by_key(|data| data.len()),
        _ => None,
    }
}

pub fn voice_wav(app: &AppHandle, text: &str, voice: &str) -> Result<PathBuf, String> {
    use std::hash::{Hash, Hasher};
    let dir = app.path().app_local_data_dir().map_err(|e| e.to_string())?.join("voz").join("dichos");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (voice, text.trim().to_lowercase()).hash(&mut hasher);
    let file = dir.join(format!("{:016x}.wav", hasher.finish()));
    if file.is_file() {
        return Ok(file);
    }
    if !settings::load(app).ai_url.contains("googleapis.com") {
        return Err("La voz de Gemini necesita que el servicio de IA elegido sea Gemini.".into());
    }
    let key = key(app).ok_or("Falta la clave de Gemini.")?;
    let asked_today = |model: &str| with_usage(app, |counts| counts.iter().find(|(name, _)| name == model).map_or(0, |(_, used)| *used));
    for model in VOICE_MODELS.iter().filter(|model| !resting(model) && asked_today(model) < VOICE_DAILY) {
        let body = json!({
            "model": model,
            "input": [{ "type": "user_input", "content": [{ "type": "text", "text": text }] }],
            "response_format": { "type": "audio" },
            "generation_config": { "speech_config": [{ "voice": voice }] }
        });
        let (status, reply) = send("https://generativelanguage.googleapis.com/v1beta/interactions", &[format!("x-goog-api-key: {key}")], Some(&body))?;
        count(app, model);
        if cfg!(debug_assertions) {
            eprintln!("[voz] {model}: {status} {}", reply.to_string().chars().take(260).collect::<String>());
        }
        if status == 429 || status == 404 {
            rest(model);
            continue;
        }
        if !(200..300).contains(&status) {
            return Err(refusal(status, &reply, model));
        }
        let pcm = unbase64(audio_in(&reply).ok_or("El servicio no devolvió audio.")?);
        let wav = if pcm.starts_with(b"RIFF") {
            pcm
        } else {
            let mut wav = Vec::with_capacity(44 + pcm.len());
            wav.extend_from_slice(b"RIFF");
            wav.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
            wav.extend_from_slice(b"WAVEfmt ");
            for field in [16u32, 0x0001_0001, 24_000, 48_000, 0x0010_0002] {
                wav.extend_from_slice(&field.to_le_bytes());
            }
            wav.extend_from_slice(b"data");
            wav.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
            wav.extend_from_slice(&pcm);
            wav
        };
        std::fs::write(&file, wav).map_err(|e| e.to_string())?;
        let mut kept: Vec<_> = std::fs::read_dir(&dir).into_iter().flatten().flatten().filter_map(|entry| Some((entry.metadata().ok()?.modified().ok()?, entry.path()))).collect();
        kept.sort();
        for (_, old) in kept.iter().rev().skip(400) {
            let _ = std::fs::remove_file(old);
        }
        return Ok(file);
    }
    Err("La voz de Gemini llegó a su límite de hoy.".into())
}

fn chats(id: &str) -> bool {
    let id = id.to_lowercase();
    const OTHER_JOBS: [&str; 24] = [
        "embed", "whisper", "tts", "audio", "imagen", "image", "veo", "guard", "moderation", "dall-e", "rerank", "live", "aqa", "lyria", "banana",
        "robotics", "deep-research", "computer-use", "transcribe", "antigravity", "customtools", "realtime", "omni", "search",
    ];
    !OTHER_JOBS.iter().any(|kind| id.contains(kind))
}

pub fn live_models(app: &AppHandle) -> Result<Vec<String>, String> {
    let key = key(app).ok_or("Falta la clave de Gemini.")?;
    let (status, reply) = send("https://generativelanguage.googleapis.com/v1beta/models?pageSize=1000", &[format!("x-goog-api-key: {key}")], None)?;
    if !(200..300).contains(&status) {
        return Err(refusal(status, &reply, ""));
    }
    Ok(reply["models"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|model| model["supportedGenerationMethods"].as_array().is_some_and(|ways| ways.iter().any(|way| way == "bidiGenerateContent")))
        .filter_map(|model| model["name"].as_str())
        .map(|name| name.trim_start_matches("models/").to_string())
        .filter(|name| !["tts", "transcribe", "translate", "robotics"].iter().any(|job| name.contains(job)))
        .collect())
}

#[tauri::command]
pub async fn ai_models(app: AppHandle) -> Result<Vec<String>, String> {
    let config = settings::load(&app);
    let anthropic = config.ai_format == "anthropic";
    let base = config.ai_url.trim().trim_end_matches('/').trim_end_matches("/chat/completions").trim_end_matches("/messages");
    let (status, reply) = send(&format!("{base}/models"), &headers(anthropic, key(&app).as_deref()), None)?;
    if !(200..300).contains(&status) {
        return Err(refusal(status, &reply, ""));
    }
    let listed = reply["data"].as_array().or(reply["models"].as_array()).cloned().unwrap_or_default();
    let mut ids: Vec<String> = listed
        .iter()
        .filter_map(|model| model["id"].as_str().or(model["name"].as_str()))
        .map(|id| id.trim_start_matches("models/").to_string())
        .filter(|id| chats(id))
        .collect();
    ids.sort();
    ids.dedup();
    if ids.is_empty() {
        return Err("El servicio no tiene ningún modelo listo. Si es local, descarga uno primero.".into());
    }
    Ok(ids)
}

#[tauri::command]
pub fn ai_key_page(app: AppHandle) -> Result<(), String> {
    let url = settings::load(&app).ai_url.to_lowercase();
    let page = [
        ("googleapis.com", "https://aistudio.google.com/apikey"),
        ("groq.com", "https://console.groq.com/keys"),
        ("openai.com", "https://platform.openai.com/api-keys"),
        ("deepseek.com", "https://platform.deepseek.com/api_keys"),
        ("anthropic.com", "https://console.anthropic.com/settings/keys"),
        ("openrouter.ai", "https://openrouter.ai/keys"),
    ]
    .iter()
    .find(|(host, _)| url.contains(host))
    .map(|(_, page)| *page)
    .ok_or("Ese servicio no necesita clave, o no sé dónde se crea la suya.")?;
    crate::apps::open(page)
}

fn order_in(reply: &str) -> String {
    let mut text = reply.to_string();
    while let (Some(start), Some(end)) = (text.find("<think>"), text.find("</think>")) {
        if end < start {
            break;
        }
        text.replace_range(start..end + "</think>".len(), "");
    }
    let line = text.lines().map(str::trim).find(|line| !line.is_empty()).unwrap_or("");
    let bare = |text: &'_ str| -> String { text.trim_matches(|c: char| matches!(c, '"' | '\'' | '`' | '«' | '»' | '“' | '”' | '*' | '-' | ' ')).to_string() };
    let line = bare(line);
    let line = bare(line.strip_prefix("Orden:").or(line.strip_prefix("orden:")).unwrap_or(&line));
    if line.to_lowercase().starts_with("responde") {
        return line;
    }
    line.trim_end_matches('.').to_string()
}

fn answers_path(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_config_dir().ok()?.join("ia-ordenes.json"))
}

pub fn rewrite(app: &AppHandle, said: &str, scripts: &[Script]) -> Result<String, String> {
    let asked = format!("{}|{}", crate::intent::normalize(said), scripts.len());
    let talking = recent();
    if talking.is_empty() {
        let mut answers = ANSWERS.lock().unwrap();
        if answers.is_empty() {
            *answers = answers_path(app).and_then(|path| std::fs::read(path).ok()).and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
        }
        if let Some((_, order)) = answers.iter().find(|(question, _)| *question == asked) {
            return Ok(order.clone());
        }
    }
    let system = format!("{}{}{talking}", instructions(scripts, crate::agent::wanted(app)), about_the_person(app, said));
    let order = chat(app, &system, said, Extra::None).map(|reply| order_in(&reply))?;
    if !order.is_empty() && !order.to_lowercase().starts_with("responde") {
        let mut answers = ANSWERS.lock().unwrap();
        if answers.len() >= REMEMBERED {
            answers.remove(0);
        }
        answers.push((asked, order.clone()));
        if let (Some(path), Ok(json)) = (answers_path(app), serde_json::to_vec(&*answers)) {
            let _ = std::fs::write(path, json);
        }
    }
    Ok(order)
}

pub fn see(app: &AppHandle, question: &str, png: &[u8]) -> Result<String, String> {
    let system = "Eres Cocuyo, un asistente de voz para Windows. La persona rodeó una parte de su pantalla (la imagen) y \
                  preguntó algo en voz alta; la pregunta llega transcrita por una máquina y puede traer errores de oído. \
                  Responde en el idioma de la pregunta, directo y breve: tres frases como mucho, en texto corrido, sin listas ni formato.";
    let system = format!("{system}{}{}", about_the_person(app, question).replace("Al usar «responde», habla así", "Habla así"), recent());
    let answer = chat(app, &system, question, Extra::Image(png))?;
    let answer = answer.split("</think>").last().unwrap_or("").trim().to_string();
    if answer.is_empty() {
        return Err("El modelo no respondió nada. Puede que no sepa mirar imágenes: prueba con otro.".into());
    }
    Ok(answer)
}

pub fn polish(app: &AppHandle, dictated: &str) -> Option<String> {
    let system = "Recibes un dictado transcrito por una máquina. Devuélvelo limpio y nada más: pon la puntuación y las mayúsculas, \
                  corrige las palabras claramente mal oídas, quita las muletillas y los arranques en falso (eh, este, o sea) y, si la \
                  persona se corrige a mitad de frase, deja solo la versión final. No añadas nada, no contestes a lo que dice, \
                  no lo resumas y no cambies el idioma ni el sentido.";
    let clean = chat(app, system, dictated, Extra::None).ok()?;
    let clean = clean.split("</think>").last()?.trim().trim_matches(|c: char| matches!(c, '"' | '«' | '»' | '“' | '”' | '`')).trim().to_string();
    let (before, after) = (dictated.chars().count() as f32, clean.chars().count() as f32);
    (after > 0.0 && after <= before * 1.5 + 20.0 && after >= before * 0.35).then_some(clean)
}

pub fn edit(app: &AppHandle, instruction: &str, text: &str) -> Result<String, String> {
    let system = "La persona seleccionó un texto y te pide cambiarlo. Aplica su instrucción y devuelve SOLO el texto resultante, listo para \
                  pegar en el mismo sitio: sin comentarios, sin comillas alrededor y sin explicar lo que hiciste. Conserva el idioma, el \
                  tono y los saltos de línea salvo que la instrucción pida otra cosa.";
    let room = (text.chars().count() as u32 / 2 + 300).clamp(MAX_TOKENS, 4000);
    let changed = chat_up_to(app, system, &format!("Instrucción: {instruction}\n\nTexto:\n{text}"), Extra::None, room)?;
    let changed = changed.split("</think>").last().unwrap_or("").trim().to_string();
    if changed.is_empty() {
        return Err("El modelo no devolvió nada.".into());
    }
    Ok(changed)
}

#[tauri::command]
pub async fn ai_test(app: AppHandle) -> Result<String, String> {
    let started = std::time::Instant::now();
    let order = rewrite(&app, "oye ponme la de beibi dol de dominic faik porfa", &[])?;
    if order.is_empty() {
        return Err("El modelo respondió, pero con un texto vacío.".into());
    }
    Ok(format!("Funciona: entendió «{order}» en {:.1} s.", started.elapsed().as_secs_f32()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_order_is_the_first_clean_line() {
        assert_eq!(order_in("pon Baby Doll de Dominic Fike"), "pon Baby Doll de Dominic Fike");
        assert_eq!(order_in("\n\"sube el volumen.\"\n"), "sube el volumen");
        assert_eq!(order_in("<think>\nquiere música\n</think>\n\npon música rock\nporque lo pidió"), "pon música rock");
        assert_eq!(order_in("Orden: `script copia de seguridad`"), "script copia de seguridad");
        assert_eq!(order_in("responde Son 180."), "responde Son 180.");
        assert_eq!(order_in(""), "");
    }

    #[test]
    fn the_address_takes_a_base_or_the_whole_thing() {
        assert_eq!(endpoint("http://127.0.0.1:11434/v1/", "/chat/completions"), "http://127.0.0.1:11434/v1/chat/completions");
        assert_eq!(endpoint("https://api.deepseek.com/v1/chat/completions", "/chat/completions"), "https://api.deepseek.com/v1/chat/completions");
        assert_eq!(endpoint("https://api.anthropic.com/v1", "/messages"), "https://api.anthropic.com/v1/messages");
        assert_eq!(
            endpoint("https://generativelanguage.googleapis.com/v1beta/openai/", "/chat/completions"),
            "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions"
        );
    }

    #[test]
    fn several_models_take_turns_and_rest_longer_each_time() {
        assert_eq!(models("gemini-3.5-flash-lite, gemini-3.1-flash-lite"), ["gemini-3.5-flash-lite", "gemini-3.1-flash-lite"]);
        assert_eq!(models(" llama3.2 "), ["llama3.2"]);
        assert!(models(" , ").is_empty());

        let model = "modelo-de-prueba-de-descanso";
        assert!(!resting(model));
        rest(model);
        rest(model);
        let resting_now = RESTING.lock().unwrap().iter().find(|(name, _, _)| name == model).map(|(_, until, strikes)| (*until, *strikes)).unwrap();
        assert!(resting(model) && resting_now.1 == 1 && resting_now.0 > Instant::now() + Duration::from_secs(300));
    }

    #[test]
    fn a_conversation_is_kept_for_a_while() {
        talked("cómo se dice gato en inglés", "Se dice cat.");
        let recent = recent();
        assert!(recent.contains("Persona: cómo se dice gato en inglés") && recent.contains("Cocuyo: Se dice cat."));
        assert!(!asking());

        talked("quiero armar una lista de mercado", "¿Para cuántos días la quieres?");
        assert!(asking(), "acaba de preguntar algo: lo siguiente es la respuesta");
        let now = super::recent();
        assert!(now.contains("RESPUESTA") && now.contains("lista de mercado"));
        assert!(conversation().contains("Cocuyo: ¿Para cuántos días la quieres?"));
        talked("para una semana", "Listo, lista de mercado para una semana.");
        assert!(!asking());
        for n in 0..30 {
            talked(&format!("frase {n}"), "vale");
        }
        assert_eq!(TALK.lock().unwrap().len(), TALK_TURNS);
    }

    #[test]
    fn what_was_heard_and_the_order_come_apart() {
        assert_eq!(heard_in("dicho: ponme la de Baby Doll\norden: pon Baby Doll de Dominic Fike."), ("ponme la de Baby Doll".into(), "pon Baby Doll de Dominic Fike".into()));
        assert_eq!(heard_in("Dicho: «qué hora es»\nOrden: responde Son las tres."), ("qué hora es".into(), "responde Son las tres.".into()));
        assert_eq!(heard_in("pausa"), (String::new(), "pausa".into()));
    }

    #[test]
    fn audio_comes_back_from_text() {
        assert_eq!(unbase64(&base64(b"RIFF sonido de prueba \x00\xff")), b"RIFF sonido de prueba \x00\xff");
        let reply = json!({ "steps": [{ "type": "model_output", "content": [{ "type": "audio", "mime_type": "audio/wav", "data": "UklGRg==" }] }], "id": "x" });
        assert_eq!(audio_in(&reply), Some("UklGRg=="));
        assert_eq!(audio_in(&json!({ "error": { "message": "no" } })), None);
    }

    #[test]
    fn only_the_facts_that_matter_go_along() {
        let mut facts: Vec<String> = (0..60).map(|n| format!("dato suelto número {n}")).collect();
        facts.insert(3, "mi talla de camisa es la M".into());
        facts.insert(7, "el cumpleaños de mi mamá es el 4 de mayo".into());
        let chosen = facts_for(&facts, "cuándo cumple años mi mamá");
        assert_eq!(chosen.len(), RECENT_FACTS + 1);
        assert!(chosen.iter().any(|fact| fact.contains("mamá")) && !chosen.iter().any(|fact| fact.contains("talla")));
        assert!(facts_for(&facts, "qué talla de camisa uso").iter().any(|fact| fact.contains("talla")));
        assert_eq!(facts_for(&facts, "").len(), RECENT_FACTS + RELATED_FACTS);
        assert_eq!(facts_for(&facts[..4], "hola").len(), 4);
    }

    #[test]
    fn images_travel_as_text() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(&[0xff, 0xfe, 0xfd, 0xfc]), "//79/A==");
    }

    #[test]
    fn only_models_that_talk_are_listed() {
        assert!(chats("gemini-3.5-flash-lite") && chats("llama-3.3-70b-versatile") && chats("llama3.2:latest") && chats("claude-haiku-5-5"));
        assert!(!chats("lyria-3.5") && !chats("gemini-nano-banana-2.1") && !chats("gemini-3.5-transcribe") && !chats("deep-research-preview-04-2026"));
        assert!(!chats("text-embedding-004") && !chats("whisper-large-v3") && !chats("meta-llama/llama-prompt-guard-2-22m") && !chats("imagen-4"));
    }

    #[test]
    fn a_refusal_is_explained_in_plain_words() {
        assert!(refusal(429, &Value::Null, "x").contains("llegó al límite"));
        assert_eq!(refusal(401, &Value::Null, "x"), "El servicio rechazó la clave.");
        assert_eq!(refusal(418, &json!({ "error": { "message": "boom" } }), "x"), "El servicio respondió: boom");
        assert!(refusal(503, &Value::Null, "x").contains("saturado"));
    }

    #[test]
    fn scripts_are_offered_to_the_model() {
        let scripts = [Script { name: "copia de seguridad".into(), about: "Copia mis documentos al disco externo.".into(), path: PathBuf::new() }];
        let text = instructions(&scripts, false);
        assert!(text.contains("script <nombre>") && text.contains("- copia de seguridad: Copia mis documentos"));
        assert!(!instructions(&[], false).contains("script <nombre>"));
        assert!(!instructions(&[], false).contains("agente <") && instructions(&[], true).contains("agente <"));
    }
}
