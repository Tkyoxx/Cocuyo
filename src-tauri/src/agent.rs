// el modo agente: piensa en pasos y usa herramientas (las de cocuyo, tus scripts y los servidores mcp). el jarvis, pues

use std::collections::HashMap;
use std::sync::mpsc::{channel, Sender};
use std::sync::Mutex;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

#[derive(Clone, Deserialize)]
pub struct ExtensionTool {
    nombre: String,
    #[serde(default)]
    descripcion: String,
    #[serde(default)]
    parametros: Value,
}

static EXTENSION_TOOLS: Mutex<Vec<ExtensionTool>> = Mutex::new(Vec::new());
static WAITING: Mutex<Vec<(u64, Sender<Result<String, String>>)>> = Mutex::new(Vec::new());
static NEXT_CALL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

#[tauri::command]
pub fn extension_tools(tools: Vec<ExtensionTool>) {
    *EXTENSION_TOOLS.lock().unwrap() = tools.into_iter().take(60).collect();
}

#[tauri::command]
pub fn extension_tool_done(call: u64, ok: bool, result: String) {
    let mut waiting = WAITING.lock().unwrap();
    if let Some(at) = waiting.iter().position(|(id, _)| *id == call) {
        let (_, send) = waiting.remove(at);
        let _ = send.send(if ok { Ok(result) } else { Err(result) });
    }
}

fn ask_extension(app: &AppHandle, name: &str, args: &Value) -> Result<String, String> {
    let call = NEXT_CALL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let (send, wait) = channel();
    WAITING.lock().unwrap().push((call, send));
    let _ = app.emit_to(crate::overlay::LABEL, "agent-tool", json!({ "call": call, "nombre": name, "args": args }));
    let result = wait.recv_timeout(Duration::from_secs(45)).unwrap_or_else(|_| Err("la extensión no respondió".into()));
    WAITING.lock().unwrap().retain(|(id, _)| *id != call);
    result
}

const STEPS: usize = 8;
const WAKE: [&str; 6] = ["jarvis", "agente", "oye jarvis", "oye agente", "hey jarvis", "ok jarvis"];

pub fn wanted(app: &AppHandle) -> bool {
    crate::settings::load(app).ai_agent && crate::ai::mode(app).is_some()
}

pub fn called(spoken: &str) -> Option<String> {
    let clean = crate::intent::normalize(spoken);
    let lead = WAKE.iter().filter(|lead| clean.starts_with(**lead)).max_by_key(|lead| lead.len())?;
    let words = lead.split_whitespace().count();
    let rest: String = spoken.split_whitespace().skip(words).collect::<Vec<_>>().join(" ");
    let rest = rest.trim_start_matches([',', '.', ':', ' ']).trim().to_string();
    Some(if rest.is_empty() { spoken.to_string() } else { rest })
}

fn safe_name(text: &str) -> String {
    let name: String = text.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect();
    name.chars().take(64).collect()
}

fn function(name: &str, description: &str, parameters: Value) -> Value {
    json!({ "type": "function", "function": { "name": name, "description": description, "parameters": parameters } })
}

fn object(properties: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": properties, "required": required })
}

enum Doer {
    Order,
    Card,
    Script,
    Clipboard,
    Screen,
    Remember,
    Extension(String),
    Mcp(String, String),
}

fn toolbox(app: &AppHandle) -> (Vec<Value>, HashMap<String, Doer>) {
    let mut tools = Vec::new();
    let mut doers = HashMap::new();
    tools.push(function(
        "cocuyo_orden",
        "Hace una orden de Cocuyo en el PC, en español y como la diría una persona. Sirve para: poner música en Spotify («pon Bad Bunny», «pausa», \
         «siguiente», «sube el volumen»), abrir y cerrar apps o webs («abre Discord», «cierra Spotify»), buscar en Google o YouTube, recordatorios y \
         horarios («recuérdame en 10 minutos sacar la pizza», «a las 8 pon música»), el clima («qué clima hace»), capturas, bloquear el equipo, \
         notas («anota comprar pan»). Devuelve lo que respondió Cocuyo.",
        object(json!({ "orden": { "type": "string", "description": "La orden, en una frase" } }), &["orden"]),
    ));
    doers.insert("cocuyo_orden".to_string(), Doer::Order);
    tools.push(function(
        "cocuyo_tarjeta",
        "Muestra una tarjeta bonita bajo la isla de Cocuyo. Úsala cuando la respuesta tenga datos que se lean mejor que se oyen: listas, cifras, \
         resultados. Las filas son pares texto/valor.",
        object(
            json!({
                "titulo": { "type": "string" },
                "texto": { "type": "string" },
                "icono": { "type": "string", "description": "Un emoji" },
                "filas": { "type": "array", "items": { "type": "object", "properties": { "texto": { "type": "string" }, "valor": { "type": "string" } } } },
                "enlace": { "type": "string", "description": "Una URL https para un botón «Abrir», si hace falta" }
            }),
            &["titulo"],
        ),
    ));
    doers.insert("cocuyo_tarjeta".to_string(), Doer::Card);
    let scripts = crate::scripts::list(app);
    if !scripts.is_empty() {
        let menu: Vec<String> = scripts.iter().take(60).map(|script| format!("«{}»: {}", script.name, script.about)).collect();
        tools.push(function(
            "cocuyo_script",
            &format!("Ejecuta uno de los scripts de la persona y devuelve lo que respondió. Los que hay:\n{}", menu.join("\n")),
            object(json!({ "nombre": { "type": "string" }, "dato": { "type": "string", "description": "Lo que el script necesita, si algo" } }), &["nombre"]),
        ));
        doers.insert("cocuyo_script".to_string(), Doer::Script);
    }
    tools.push(function("cocuyo_portapapeles", "Lee el texto que la persona tiene copiado ahora mismo.", object(json!({}), &[])));
    doers.insert("cocuyo_portapapeles".to_string(), Doer::Clipboard);
    tools.push(function("cocuyo_pantalla", "Lee el texto que se ve ahora mismo en la pantalla (reconocimiento de texto de Windows).", object(json!({}), &[])));
    doers.insert("cocuyo_pantalla".to_string(), Doer::Screen);
    tools.push(function(
        "cocuyo_recordar",
        "Guarda un dato sobre la persona en la memoria de Cocuyo, para siempre. Solo si te lo pide o si es claramente algo que querrá que recuerdes.",
        object(json!({ "dato": { "type": "string" } }), &["dato"]),
    ));
    doers.insert("cocuyo_recordar".to_string(), Doer::Remember);
    if crate::settings::load(app).extensions_on {
        for tool in EXTENSION_TOOLS.lock().unwrap().iter() {
            let name = safe_name(&format!("ext__{}", tool.nombre));
            if doers.contains_key(&name) {
                continue;
            }
            let mut schema = if tool.parametros.is_object() { tool.parametros.clone() } else { json!({ "type": "object", "properties": {} }) };
            if schema.get("type").is_none() {
                schema["type"] = json!("object");
            }
            tools.push(function(&name, &format!("[extensión] {}", tool.descripcion), schema));
            doers.insert(name, Doer::Extension(tool.nombre.clone()));
        }
    }
    for tool in crate::mcp::tools(app) {
        let name = safe_name(&format!("{}__{}", tool.server, tool.name));
        if doers.contains_key(&name) || tools.len() >= 120 {
            continue;
        }
        let mut schema = tool.schema.clone();
        if schema.get("type").is_none() {
            schema["type"] = json!("object");
        }
        tools.push(function(&name, &format!("[{}] {}", tool.server, tool.description), schema));
        doers.insert(name, Doer::Mcp(tool.server, tool.name));
    }
    (tools, doers)
}

fn text(value: &Value, key: &str) -> String {
    value.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

fn act(app: &AppHandle, doer: &Doer, args: &Value) -> Result<String, String> {
    match doer {
        Doer::Order => crate::voice::local_order(app, &text(args, "orden")),
        Doer::Card => {
            let mut card = json!({ "titulo": text(args, "titulo"), "texto": text(args, "texto"), "icono": text(args, "icono"), "segundos": 25 });
            if let Some(rows) = args.get("filas") {
                card["filas"] = rows.clone();
            }
            let link = text(args, "enlace");
            if link.starts_with("https://") {
                card["botones"] = json!([{ "texto": "Abrir", "abrir": link }]);
            }
            crate::scripts::show_card(app, "Jarvis", &card);
            Ok("Tarjeta mostrada.".into())
        }
        Doer::Script => crate::scripts::run_named(app, &text(args, "nombre"), &text(args, "dato")),
        Doer::Clipboard => Ok(crate::inject::clipboard_text().unwrap_or_default().chars().take(8000).collect()),
        Doer::Screen => crate::capture::screen_text(),
        Doer::Remember => crate::ai::memorize(app, &text(args, "dato")).map(|()| "Guardado.".into()),
        Doer::Extension(name) => ask_extension(app, name, args),
        Doer::Mcp(server, tool) => crate::mcp::call(server, tool, args.clone()),
    }
}

fn progress(app: &AppHandle, step: usize, label: &str, done: bool) {
    let _ = app.emit_to(crate::overlay::LABEL, "script-progress", json!({ "script": "Jarvis", "valor": if done { Some(1.0) } else { None::<f64> }, "texto": label }));
    let _ = step;
}

fn label(name: &str, args: &Value) -> String {
    match name {
        "cocuyo_orden" => format!("«{}»", text(args, "orden")),
        "cocuyo_script" => format!("Script «{}»", text(args, "nombre")),
        "cocuyo_tarjeta" => "Armando la tarjeta".into(),
        "cocuyo_portapapeles" => "Leyendo lo que copiaste".into(),
        "cocuyo_pantalla" => "Leyendo la pantalla".into(),
        "cocuyo_recordar" => "Guardando en la memoria".into(),
        other => other.replacen("__", " → ", 1),
    }
}

pub fn run(app: &AppHandle, said: &str) -> Result<String, String> {
    let (tools, doers) = toolbox(app);
    let now = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    let system = format!(
        "Eres Cocuyo, el asistente de voz de esta persona en su PC con Windows. Hablas como un amigo listo: directo, cálido y con algo de humor, \
         nunca de robot. Hoy es {:02}/{:02}/{} y son las {}:{:02}.\n\
         Tienes herramientas: úsalas cuando sirvan, encadénalas si hace falta (por ejemplo, leer algo y luego mostrarlo en una tarjeta), y no le preguntes \
         a la persona lo que puedes averiguar tú. Si una herramienta falla, intenta otra vía o explica qué pasó.\n\
         Al final responde con una o dos frases cortas, para decir en voz alta: sin listas, sin markdown, sin URLs. Si hay datos largos, ponlos en una \
         tarjeta y en la respuesta di solo lo esencial. Si te falta un dato de verdad, pregunta (termina en «?»). Responde en el idioma de la persona.{}",
        now.wDay,
        now.wMonth,
        now.wYear,
        now.wHour,
        now.wMinute,
        crate::ai::agent_context(app, said)
    );
    let mut messages = vec![json!({ "role": "system", "content": system }), json!({ "role": "user", "content": said })];
    let mut used = false;
    for step in 0..STEPS {
        let message = crate::ai::chat_tools(app, &messages, &tools)?;
        let calls = message["tool_calls"].as_array().cloned().unwrap_or_default();
        if calls.is_empty() {
            if used {
                progress(app, step, "Listo", true);
            }
            let answer = message["content"].as_str().unwrap_or("").split("</think>").last().unwrap_or("").trim().to_string();
            return if answer.is_empty() { Ok("Hecho.".into()) } else { Ok(answer) };
        }
        let mut assistant = message.clone();
        if assistant.get("content").is_none() {
            assistant["content"] = Value::Null;
        }
        messages.push(assistant);
        for call in calls {
            let name = call["function"]["name"].as_str().unwrap_or("").to_string();
            let args: Value = match &call["function"]["arguments"] {
                Value::String(raw) => serde_json::from_str(raw).unwrap_or_else(|_| json!({})),
                other if other.is_object() => other.clone(),
                _ => json!({}),
            };
            used = true;
            progress(app, step, &label(&name, &args), false);
            let result = match doers.get(&name) {
                Some(doer) => act(app, doer, &args),
                None => Err(format!("no existe la herramienta «{name}»")),
            };
            let content = match result {
                Ok(text) if text.trim().is_empty() => "Hecho.".to_string(),
                Ok(text) => text,
                Err(why) => format!("ERROR: {why}"),
            };
            if cfg!(debug_assertions) {
                eprintln!("[jarvis] {name} {args} -> {}", content.chars().take(160).collect::<String>());
            }
            messages.push(json!({ "role": "tool", "tool_call_id": call["id"].as_str().unwrap_or(""), "name": name, "content": content }));
        }
    }
    progress(app, STEPS, "Me enredé", true);
    Err("Me enredé con tantos pasos. Pídemelo por partes.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wake_word_is_taken_off() {
        assert_eq!(called("Jarvis, ¿qué tengo en el portapapeles?").as_deref(), Some("¿qué tengo en el portapapeles?"));
        assert_eq!(called("oye jarvis busca mis archivos").as_deref(), Some("busca mis archivos"));
        assert_eq!(called("agente resume esto").as_deref(), Some("resume esto"));
        assert_eq!(called("pon música"), None);
        assert_eq!(called("Jarvis").as_deref(), Some("Jarvis"));
    }

    #[test]
    fn tool_names_stay_inside_what_the_apis_accept() {
        assert_eq!(safe_name("mi server__leer archivo.txt"), "mi_server__leer_archivo_txt");
        assert_eq!(safe_name(&"x".repeat(100)).len(), 64);
    }
}
