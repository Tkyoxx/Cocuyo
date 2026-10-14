// el modo agente: piensa en pasos y usa herramientas (las de cocuyo, tus scripts, los servidores mcp y hasta el mouse)

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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
static ASKING: Mutex<Vec<(u64, Sender<bool>)>> = Mutex::new(Vec::new());
static NEXT_CALL: AtomicU64 = AtomicU64::new(1);
static ASKED_LAST: AtomicBool = AtomicBool::new(false);

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

pub fn answered_permission(call: u64, yes: bool) {
    let mut asking = ASKING.lock().unwrap();
    if let Some(at) = asking.iter().position(|(id, _)| *id == call) {
        let (_, send) = asking.remove(at);
        let _ = send.send(yes);
    }
}

fn ask_extension(app: &AppHandle, name: &str, args: &Value) -> Result<String, String> {
    let call = NEXT_CALL.fetch_add(1, Ordering::Relaxed);
    let (send, wait) = channel();
    WAITING.lock().unwrap().push((call, send));
    let _ = app.emit_to(crate::overlay::LABEL, "agent-tool", json!({ "call": call, "nombre": name, "args": args }));
    let result = wait.recv_timeout(Duration::from_secs(45)).unwrap_or_else(|_| Err("la extensión no respondió".into()));
    WAITING.lock().unwrap().retain(|(id, _)| *id != call);
    result
}

fn permission(app: &AppHandle, what: &str) -> bool {
    if crate::pc::boxed_in().is_some() {
        eprintln!("[agente] permiso dentro de la caja de prueba: {what}");
        return true;
    }
    let call = NEXT_CALL.fetch_add(1, Ordering::Relaxed);
    let (send, wait) = channel();
    ASKING.lock().unwrap().push((call, send));
    let card = json!({
        "id": 900_000 + call, "script": "Cocuyo", "titulo": "¿Lo hago?", "texto": what, "imagen": "", "icono": "✋", "color": "#f2b84b",
        "botones": [{ "texto": "Hazlo", "agente": call, "si": true }, { "texto": "No", "agente": call, "si": false }],
        "filas": [], "segundos": 60
    });
    let _ = app.emit_to(crate::overlay::LABEL, "script-card", card);
    crate::cue::answer(&format!("{what}. ¿Lo hago?"));
    let yes = wait.recv_timeout(Duration::from_secs(60)).unwrap_or(false);
    ASKING.lock().unwrap().retain(|(id, _)| *id != call);
    yes
}

const STEPS: usize = 12;
const WAKE: [&str; 9] = ["jarvis", "agente", "oye jarvis", "oye agente", "hey jarvis", "ok jarvis", "encargate de", "encargate", "hazte cargo de"];

pub fn wanted(app: &AppHandle) -> bool {
    crate::settings::load(app).ai_agent && crate::ai::mode(app).is_some()
}

pub fn called(spoken: &str) -> Option<String> {
    let mut clean = crate::intent::normalize(spoken);
    let mut skip = 0;
    if clean.starts_with("cocuyo encargate") || clean.starts_with("cocuyo hazte cargo") {
        clean = clean["cocuyo ".len()..].to_string();
        skip = 1;
    }
    let lead = WAKE.iter().filter(|lead| clean == **lead || clean.starts_with(&format!("{lead} "))).max_by_key(|lead| lead.len())?;
    skip += lead.split_whitespace().count();
    let rest: String = spoken.split_whitespace().skip(skip).collect::<Vec<_>>().join(" ");
    let rest = rest.trim_start_matches([',', '.', ':', ' ']).trim().to_string();
    Some(if rest.is_empty() { spoken.to_string() } else { rest })
}

pub fn asked() -> bool {
    ASKED_LAST.load(Ordering::Relaxed) && crate::ai::asking()
}

pub fn needs_steps(spoken: &str) -> bool {
    let clean = format!(" {} ", crate::intent::normalize(spoken));
    const TO_SOMEONE: [&str; 16] = [
        " mandale ", " mandales ", " enviale ", " envieles ", " escribele ", " escribeles ", " avisale ", " respondele ", " contestale ",
        " manda un mensaje", " envia un mensaje", " mandar un mensaje", " enviar un mensaje", " escribir un mensaje", " send a message", " message ",
    ];
    const CHATS: [&str; 12] = [" whatsapp ", " wasap ", " guasap ", " discord ", " telegram ", " instagram ", " messenger ", " slack ", " teams ", " signal ", " gmail ", " correo "];
    const HANDS: [&str; 12] = [
        " haz clic", " haz click", " dale clic", " dale click", " da clic", " pulsa el boton", " presiona el boton", " en la pantalla ", " en esta ventana",
        " rellena ", " llena el formulario", " llena este ",
    ];
    const THEN: [&str; 6] = [" y luego ", " y despues ", " despues de eso ", " y entonces ", " y al final ", " and then "];
    TO_SOMEONE.iter().any(|lead| clean.contains(lead))
        || (clean.contains(" dile a ") && clean.contains(" que "))
        || (CHATS.iter().any(|app| clean.contains(app)) && (clean.contains("mensaje") || clean.contains(" por ")) && !clean.starts_with(" abre "))
        || HANDS.iter().any(|lead| clean.contains(lead))
        || THEN.iter().any(|lead| clean.contains(lead))
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
    Window,
    Look,
    Hands,
    Extension(String),
    Mcp(String, String),
}

fn toolbox(app: &AppHandle) -> (Vec<Value>, HashMap<String, Doer>) {
    let config = crate::settings::load(app);
    let mut tools = Vec::new();
    let mut doers = HashMap::new();
    tools.push(function(
        "cocuyo_orden",
        "Una orden de Cocuyo, como la diría una persona: música en Spotify («pon Bad Bunny», «pausa», «sube el volumen»), abrir o cerrar apps y webs, \
         buscar en Google o YouTube, recordatorios y horarios («recuérdame en 10 minutos…», «a las 8 pon música»), el clima, capturas, bloquear, \
         notas («anota…»). Devuelve lo que respondió Cocuyo.",
        object(json!({ "orden": { "type": "string" } }), &["orden"]),
    ));
    doers.insert("cocuyo_orden".to_string(), Doer::Order);
    tools.push(function(
        "cocuyo_tarjeta",
        "Muestra una tarjeta bajo la isla. Para datos que se leen mejor que se oyen: listas, cifras, resultados. Las filas son pares texto/valor.",
        object(
            json!({
                "titulo": { "type": "string" },
                "texto": { "type": "string" },
                "icono": { "type": "string", "description": "Un emoji" },
                "filas": { "type": "array", "items": { "type": "object", "properties": { "texto": { "type": "string" }, "valor": { "type": "string" } } } },
                "enlace": { "type": "string", "description": "Una URL https para un botón «Abrir»" }
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
            object(json!({ "nombre": { "type": "string" }, "dato": { "type": "string" } }), &["nombre"]),
        ));
        doers.insert("cocuyo_script".to_string(), Doer::Script);
    }
    tools.push(function("cocuyo_portapapeles", "Lee el texto que la persona tiene copiado.", object(json!({}), &[])));
    doers.insert("cocuyo_portapapeles".to_string(), Doer::Clipboard);
    tools.push(function("cocuyo_pantalla", "Lee todo el texto que se ve en la pantalla.", object(json!({}), &[])));
    doers.insert("cocuyo_pantalla".to_string(), Doer::Screen);
    tools.push(function(
        "cocuyo_recordar",
        "Guarda un dato sobre la persona en la memoria, para siempre. Solo si lo pide o si es claro que querrá que lo recuerdes.",
        object(json!({ "dato": { "type": "string" } }), &["dato"]),
    ));
    doers.insert("cocuyo_recordar".to_string(), Doer::Remember);
    if config.agent_hands {
        tools.push(function(
            "cocuyo_ventana",
            "Abre una app (o trae al frente la que ya está abierta) y te muestra su ventana: cada botón, campo y texto con un número.",
            object(json!({ "app": { "type": "string", "description": "WhatsApp, Discord, Bloc de notas, Chrome…" } }), &["app"]),
        ));
        doers.insert("cocuyo_ventana".to_string(), Doer::Window);
        tools.push(function(
            "cocuyo_mirar",
            "Mira la ventana de adelante: sus elementos numerados. Con imagen=true también ves una captura; pídela solo si los elementos no bastan, es más lenta.",
            object(json!({ "imagen": { "type": "boolean" } }), &[]),
        ));
        doers.insert("cocuyo_mirar".to_string(), Doer::Look);
        tools.push(function(
            "cocuyo_hacer",
            "Usa el mouse y el teclado en la ventana que miraste, en orden, y te devuelve cómo quedó. Junta varias acciones en una sola llamada cuando \
             sepas lo que viene. Cada acción: {accion: clic | doble_clic | clic_derecho | escribir | tecla | esperar | rueda, id: número del elemento, \
             x/y: posición de 0 a 1000 dentro de la ventana si no hay número, texto: lo que se escribe (o «arriba»/«abajo» para la rueda), \
             teclas: «enter», «ctrl+f», «esc», «tab», «flecha abajo»…, segundos: para esperar}. «escribir» con id hace clic ahí antes.",
            object(
                json!({
                    "acciones": { "type": "array", "items": { "type": "object", "properties": {
                        "accion": { "type": "string", "enum": ["clic", "doble_clic", "clic_derecho", "escribir", "tecla", "esperar", "rueda"] },
                        "id": { "type": "integer" }, "x": { "type": "integer" }, "y": { "type": "integer" },
                        "texto": { "type": "string" }, "teclas": { "type": "string" }, "segundos": { "type": "number" }
                    }, "required": ["accion"] } },
                    "confirmar": { "type": "string", "description": "Si la ÚLTIMA acción envía, publica, compra, paga o borra algo, di aquí qué va a pasar, \
                        corto y concreto («Enviar 'llego tarde' a Angela por WhatsApp»). Se le pide permiso a la persona antes de esa acción." },
                    "destino": { "type": "string", "description": "Con «confirmar»: a quién o a qué chat va (por ejemplo «Angela»). Antes de la última acción \
                        compruebo que ese nombre está en la cabecera del chat abierto; si no, no lo hago." },
                    "ver": { "type": "boolean", "description": "true para recibir también una captura de cómo quedó" }
                }),
                &["acciones"],
            ),
        ));
        doers.insert("cocuyo_hacer".to_string(), Doer::Hands);
    }
    if config.extensions_on {
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

const VIEW: &str = "\n\nAsí quedó:\n";

struct Done {
    text: String,
    image: Option<Vec<u8>>,
}

impl From<String> for Done {
    fn from(text: String) -> Self {
        Done { text, image: None }
    }
}

fn hands(app: &AppHandle, args: &Value) -> Result<Done, String> {
    let mut actions = args.get("acciones").and_then(Value::as_array).cloned().unwrap_or_default();
    let what = text(args, "confirmar");
    let who = text(args, "destino");
    let mut log = String::new();
    if !what.trim().is_empty() && !actions.is_empty() {
        let last = actions.pop().unwrap();
        if !actions.is_empty() {
            log = crate::pc::act(&actions)?;
            log.push('\n');
            if log.contains('✗') {
                let (view, image) = crate::pc::look(false)?;
                return Ok(Done { text: format!("{log}No llegué a la última acción.{VIEW}{view}"), image });
            }
        }
        std::thread::sleep(Duration::from_millis(500));
        crate::pc::refresh()?;
        if !crate::pc::shows(&who) {
            let (view, image) = crate::pc::look(false)?;
            let text = format!("{log}NO lo hice: el chat abierto no parece ser de «{who}» ({}). Revisa la vista y vuelve a intentarlo, o pregunta.{VIEW}{view}", crate::pc::where_now());
            return Ok(Done { text, image });
        }
        if crate::settings::load(app).agent_confirm && !permission(app, &format!("{}\n{}", what.trim(), crate::pc::where_now())) {
            log.push_str("La persona dijo que NO. No lo hagas ni lo intentes de otra forma; dile que quedó sin hacer.");
            return Ok(log.into());
        }
        actions = vec![last];
    }
    log.push_str(&crate::pc::act(&actions)?);
    std::thread::sleep(Duration::from_millis(700));
    let (view, image) = crate::pc::look(args.get("ver").and_then(Value::as_bool).unwrap_or(false))?;
    Ok(Done { text: format!("{log}{VIEW}{view}"), image })
}

fn act(app: &AppHandle, doer: &Doer, args: &Value) -> Result<Done, String> {
    match doer {
        Doer::Order => crate::voice::local_order(app, &text(args, "orden")).map(Done::from),
        Doer::Card => {
            let mut card = json!({ "titulo": text(args, "titulo"), "texto": text(args, "texto"), "icono": text(args, "icono"), "segundos": 25 });
            if let Some(rows) = args.get("filas") {
                card["filas"] = rows.clone();
            }
            let link = text(args, "enlace");
            if link.starts_with("https://") {
                card["botones"] = json!([{ "texto": "Abrir", "abrir": link }]);
            }
            crate::scripts::show_card(app, "Cocuyo", &card);
            Ok(String::from("Tarjeta mostrada.").into())
        }
        Doer::Script => crate::scripts::run_named(app, &text(args, "nombre"), &text(args, "dato")).map(Done::from),
        Doer::Clipboard => Ok(crate::inject::clipboard_text().unwrap_or_default().chars().take(8000).collect::<String>().into()),
        Doer::Screen => crate::capture::screen_text().map(Done::from),
        Doer::Remember => crate::ai::memorize(app, &text(args, "dato")).map(|()| String::from("Guardado.").into()),
        Doer::Window => crate::pc::open(app, &text(args, "app")).map(Done::from),
        Doer::Look => crate::pc::look(args.get("imagen").and_then(Value::as_bool).unwrap_or(false)).map(|(text, image)| Done { text, image }),
        Doer::Hands => hands(app, args),
        Doer::Extension(name) => ask_extension(app, name, args).map(Done::from),
        Doer::Mcp(server, tool) => crate::mcp::call(server, tool, args.clone()).map(Done::from),
    }
}

fn progress(app: &AppHandle, label: &str, done: bool) {
    let _ = app.emit_to(crate::overlay::LABEL, "script-progress", json!({ "script": "Cocuyo", "valor": if done { Some(1.0) } else { None::<f64> }, "texto": label }));
}

fn label(name: &str, args: &Value) -> String {
    match name {
        "cocuyo_orden" => format!("«{}»", text(args, "orden")),
        "cocuyo_script" => format!("Script «{}»", text(args, "nombre")),
        "cocuyo_tarjeta" => "Armando la tarjeta".into(),
        "cocuyo_portapapeles" => "Leyendo lo que copiaste".into(),
        "cocuyo_pantalla" => "Leyendo la pantalla".into(),
        "cocuyo_recordar" => "Guardando en la memoria".into(),
        "cocuyo_ventana" => format!("Abriendo {}", text(args, "app")),
        "cocuyo_mirar" => "Mirando la ventana".into(),
        "cocuyo_hacer" => {
            let actions = args.get("acciones").and_then(Value::as_array).map_or(0, Vec::len);
            if actions == 1 { "Usando el mouse".into() } else { format!("Usando el mouse ({actions} pasos)") }
        }
        other => other.replacen("__", " → ", 1),
    }
}

fn forget_old_views(messages: &mut [Value]) {
    for message in messages.iter_mut() {
        match message["role"].as_str() {
            Some("tool") => {
                let Some(content) = message["content"].as_str() else { continue };
                if let Some(at) = content.find(VIEW) {
                    message["content"] = json!(format!("{}\n(la vista de entonces ya cambió)", &content[..at]));
                } else if content.starts_with("Ventana: ") {
                    message["content"] = json!("(la vista de entonces ya cambió)");
                }
            }
            Some("user") if message["content"].is_array() => message["content"] = json!("(captura vieja)"),
            _ => {}
        }
    }
}

const QUICK: [&str; 2] = ["cocuyo_orden", "cocuyo_script"];

fn prompt(app: &AppHandle, said: &str, with_hands: bool) -> String {
    let now = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    let mut system = format!(
        "Eres Cocuyo, el asistente de voz de esta persona en su PC con Windows. Hablas como un amigo listo: directo, cálido y con algo de humor, \
         nunca de robot. Hoy es {:02}/{:02}/{} y son las {}:{:02}.\n\
         Usa las herramientas para hacer lo que pide; encadénalas si hace falta y no preguntes lo que puedes averiguar. Si algo falla, prueba otra vía \
         o explica qué pasó. Sé eficiente: cada paso hace esperar a la persona.\n\
         Al final responde con una o dos frases cortas para decir en voz alta: sin listas, sin markdown, sin URLs. Si hay datos largos, ponlos en una \
         tarjeta. Si te falta un dato de verdad, pregunta (termina en «?»). Responde en el idioma de la persona.",
        now.wDay, now.wMonth, now.wYear, now.wHour, now.wMinute
    );
    if with_hands {
        system.push_str(
            "\n\nPuedes usar la PC como una persona. cocuyo_ventana abre o trae una app y te da sus elementos numerados; cocuyo_hacer hace clics, \
             escribe y pulsa teclas sobre esos números y te muestra cómo quedó, así no adivinas posiciones ni dependes de la resolución. \
             Mete en un solo cocuyo_hacer todo lo que ya sabes que viene. Los atajos de teclado suelen ser lo más rápido.\n\
             Si la persona nombra una app o ventana, usa esa y no otra parecida.\n\
             Mensajes: abre la app, busca el chat (WhatsApp: el buscador de arriba; Discord: ctrl+k y el nombre; Telegram: el buscador) y entra; \
             cuando la vista muestre ese chat abierto, escribe en el campo del mensaje y envía con enter en un cocuyo_hacer con «confirmar» y «destino». \
             Si hay varios contactos que encajan, pregunta cuál. Escribe el mensaje como lo diría la persona, en primera persona.\n\
             Nunca escribas contraseñas, códigos ni datos de tarjetas: eso lo hace la persona. Si te dice que la persona movió el mouse, detente y avísale.",
        );
    }
    if with_hands {
        let open = crate::pc::open_windows();
        if !open.is_empty() {
            system.push_str(&format!("\n\nVentanas abiertas ahora: {}.", open.join(", ")));
        }
    }
    system.push_str(&crate::ai::agent_context(app, said));
    system
}

pub fn run(app: &AppHandle, said: &str) -> Result<(String, bool), String> {
    let (tools, doers) = toolbox(app);
    let with_hands = doers.contains_key("cocuyo_hacer");
    crate::pc::begin();
    let mut messages = vec![json!({ "role": "system", "content": prompt(app, said, with_hands) }), json!({ "role": "user", "content": said })];
    let mut used = false;
    let finish = |answer: String, used: bool| {
        if used {
            progress(app, "Listo", true);
        }
        ASKED_LAST.store(answer.trim_end().ends_with('?'), Ordering::Relaxed);
        answer
    };
    for step in 0..STEPS {
        let message = crate::ai::chat_tools(app, &messages, &tools)?;
        let calls = message["tool_calls"].as_array().cloned().unwrap_or_default();
        if calls.is_empty() {
            let answer = message["content"].as_str().unwrap_or("").split("</think>").last().unwrap_or("").trim().to_string();
            return Ok((finish(if answer.is_empty() { "Hecho.".into() } else { answer }, used), true));
        }
        let mut assistant = message.clone();
        if assistant.get("content").is_none() {
            assistant["content"] = Value::Null;
        }
        messages.push(assistant);
        let quick = step == 0 && calls.len() == 1;
        let mut image = None;
        for call in calls {
            let name = call["function"]["name"].as_str().unwrap_or("").to_string();
            let args: Value = match &call["function"]["arguments"] {
                Value::String(raw) => serde_json::from_str(raw).unwrap_or_else(|_| json!({})),
                other if other.is_object() => other.clone(),
                _ => json!({}),
            };
            used = true;
            progress(app, &label(&name, &args), false);
            let result = match doers.get(&name) {
                Some(doer) => act(app, doer, &args),
                None => Err(format!("no existe la herramienta «{name}»")),
            };
            if quick && QUICK.contains(&name.as_str()) {
                if let Ok(done) = &result {
                    progress(app, "Listo", true);
                    ASKED_LAST.store(false, Ordering::Relaxed);
                    return Ok((done.text.clone(), false));
                }
            }
            if matches!(name.as_str(), "cocuyo_ventana" | "cocuyo_mirar" | "cocuyo_hacer") && result.is_ok() {
                forget_old_views(&mut messages);
            }
            let content = match result {
                Ok(done) => {
                    image = done.image.or(image);
                    if done.text.trim().is_empty() { "Hecho.".to_string() } else { done.text }
                }
                Err(why) => format!("ERROR: {why}"),
            };
            if cfg!(debug_assertions) {
                eprintln!("[agente] {name} {args} -> {}", content.chars().take(220).collect::<String>().replace('\n', " | "));
            }
            messages.push(json!({ "role": "tool", "tool_call_id": call["id"].as_str().unwrap_or(""), "name": name, "content": content }));
        }
        if crate::pc::let_go() {
            return Ok((finish("Listo, te devolví el mouse. Me detuve ahí.".into(), used), true));
        }
        if let Some(png) = image {
            messages.push(json!({ "role": "user", "content": [
                { "type": "text", "text": "La captura de la ventana (x,y de 0 a 1000)." },
                { "type": "image_url", "image_url": { "url": format!("data:image/png;base64,{}", crate::ai::base64(&png)) } }
            ] }));
        }
    }
    progress(app, "Me enredé", true);
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
        assert_eq!(called("Encárgate de ordenar mis descargas").as_deref(), Some("ordenar mis descargas"));
        assert_eq!(called("Cocuyo, encárgate de esto").as_deref(), Some("esto"));
        assert_eq!(called("pon música"), None);
        assert_eq!(called("agenda una cita"), None);
        assert_eq!(called("Jarvis").as_deref(), Some("Jarvis"));
    }

    #[test]
    fn it_knows_when_a_task_needs_steps() {
        for heavy in [
            "envíale un mensaje a Angela por WhatsApp",
            "mándale a mi mamá que llego tarde",
            "escríbele a Juan por Discord que ya voy",
            "dile a Angela que la quiero",
            "manda un mensaje por telegram al grupo",
            "abre el bloc de notas y luego escribe una lista",
            "haz clic en aceptar",
            "rellena el formulario con mis datos",
        ] {
            assert!(needs_steps(heavy), "{heavy}");
        }
        for light in ["abre whatsapp", "pon bad bunny", "sube el volumen", "qué clima hace", "recuérdame en 10 minutos sacar la pizza", "anota comprar pan", "abre discord"] {
            assert!(!needs_steps(light), "{light}");
        }
    }

    #[test]
    fn tool_names_stay_inside_what_the_apis_accept() {
        assert_eq!(safe_name("mi server__leer archivo.txt"), "mi_server__leer_archivo_txt");
        assert_eq!(safe_name(&"x".repeat(100)).len(), 64);
    }

    #[test]
    fn old_views_are_dropped_to_save_time() {
        let mut messages = vec![
            json!({ "role": "tool", "content": format!("✓ clic en botón «Buscar»{VIEW}Ventana: «WhatsApp»\n[1] campo") }),
            json!({ "role": "tool", "content": "Ventana: «WhatsApp»\n[1] campo" }),
            json!({ "role": "user", "content": [{ "type": "text", "text": "captura" }] }),
            json!({ "role": "user", "content": "envíale un mensaje" }),
        ];
        forget_old_views(&mut messages);
        assert!(messages[0]["content"].as_str().unwrap().starts_with("✓ clic en botón «Buscar»\n"));
        assert_eq!(messages[1]["content"], json!("(la vista de entonces ya cambió)"));
        assert_eq!(messages[2]["content"], json!("(captura vieja)"));
        assert_eq!(messages[3]["content"], json!("envíale un mensaje"));
    }
}
