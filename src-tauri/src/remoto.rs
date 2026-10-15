// tu celular le habla a cocuyo desde donde estes. cifrado de punta a punta: el punto de cita solo pasa paquetes que no puede abrir

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Write;
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::candado::{self, Pair};
use crate::talk::Socket;

const LETTERS: &[u8] = b"23456789ABCDEFGHJKMNPQRSTUVWXYZ";
const AAD: &[u8] = b"cocuyo-remoto-1";
const PAIRING_TIME: Duration = Duration::from_secs(120);
const MOST_PHONES: usize = 5;
const JSON: u8 = 1;
const AUDIO: u8 = 2;
const VOICE: u8 = 3;
const VOICE_PIECE: u8 = 4;
const AUDIO_PIECE: u8 = 5;
const CAN: [&str; 2] = ["trozos", "oido-vivo"];
const PIECE_BYTES: usize = 9_600;
const MOST_VOICE: usize = 1_800_000;
const CLOUDFLARE: &str = "https://api.cloudflare.com/client/v4";
const WORKER: &str = include_str!("../../remoto/worker.js");
const WORKER_NAME: &str = "cocuyo-remoto";
const WORKER_DATE: &str = "2026-09-01";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Serialize, Deserialize)]
struct Phone {
    id: String,
    nombre: String,
    cel: String,
    sala: String,
    desde: i64,
}

#[derive(Default, Serialize, Deserialize)]
struct Book {
    #[serde(default)]
    pc: String,
    #[serde(default)]
    celulares: Vec<Phone>,
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

pub fn b64(data: &[u8]) -> String {
    crate::ai::base64(data).trim_end_matches('=').replace('+', "-").replace('/', "_")
}

pub fn unb64(text: &str) -> Option<Vec<u8>> {
    let mut bits = 0u32;
    let mut count = 0;
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    for c in text.bytes() {
        let value = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            b'=' => continue,
            _ => return None,
        };
        bits = (bits << 6) | u32::from(value);
        count += 6;
        if count >= 8 {
            count -= 8;
            out.push((bits >> count) as u8);
        }
    }
    Some(out)
}

fn hex(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02x}")).collect()
}

fn book_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    Some(app.path().app_config_dir().ok()?.join("remoto.json"))
}

fn load(app: &AppHandle) -> Book {
    book_path(app).and_then(|path| std::fs::read(path).ok()).and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
}

fn save(app: &AppHandle, book: &Book) -> Result<(), String> {
    let path = book_path(app).ok_or("No se encontró la carpeta de configuración.")?;
    std::fs::write(path, serde_json::to_vec_pretty(book).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

fn pc_key(app: &AppHandle) -> Result<Pair, String> {
    let mut book = load(app);
    if let Some(secret) = unb64(&book.pc).and_then(|sealed| crate::spotify::protect(&sealed, true)) {
        if let Ok(pair) = Pair::from_secret(&secret) {
            return Ok(pair);
        }
    }
    let pair = Pair::generate()?;
    let sealed = crate::spotify::protect(pair.secret(), false).ok_or("Windows no pudo cifrar la llave.")?;
    book.pc = b64(&sealed);
    save(app, &book)?;
    Ok(pair)
}

fn pc_name() -> String {
    std::env::var("COMPUTERNAME").unwrap_or_else(|_| "tu PC".into())
}

struct Relay {
    secure: bool,
    host: String,
    port: u16,
    base: String,
}

fn relay(app: &AppHandle) -> Result<Relay, String> {
    let url = crate::settings::load(app).remote_relay.trim().trim_end_matches('/').to_string();
    let (secure, rest) = if let Some(rest) = url.strip_prefix("wss://") {
        (true, rest)
    } else if let Some(rest) = url.strip_prefix("ws://") {
        (false, rest)
    } else {
        return Err("Falta el punto de cita: pon su dirección en Ajustes, sección Remoto.".into());
    };
    let (place, base) = rest.split_once('/').map_or((rest, String::new()), |(place, base)| (place, format!("/{base}")));
    let (host, port) = match place.rsplit_once(':') {
        Some((host, port)) => (host.to_string(), port.parse().map_err(|_| "La dirección del punto de cita está mal escrita.")?),
        None => (place.to_string(), if secure { 443 } else { 80 }),
    };
    if host.is_empty() {
        return Err("La dirección del punto de cita está mal escrita.".into());
    }
    Ok(Relay { secure, host, port, base })
}

fn join(relay: &Relay, room: &str) -> Result<Socket, String> {
    Socket::connect(&relay.host, relay.port, &format!("{}/sala/{room}?rol=pc", relay.base), "", relay.secure).map_err(|_| "No llegué al punto de cita.".to_string())
}

fn send_json(socket: &Socket, value: &Value) -> Result<(), String> {
    socket.send_bytes(value.to_string().as_bytes(), false)
}

fn new_code() -> String {
    let mut code = String::new();
    while code.len() < 9 {
        for byte in candado::random(16) {
            if usize::from(byte) < 256 - 256 % LETTERS.len() && code.len() < 9 {
                code.push(char::from(LETTERS[usize::from(byte) % LETTERS.len()]));
            }
        }
    }
    code
}

fn from_code(code: &str) -> Result<(String, Vec<u8>), String> {
    let base = candado::pbkdf2(code.as_bytes(), b"cocuyo-emparejar-1", 150_000, 64)?;
    Ok((hex(&base[..16]), base[32..64].to_vec()))
}

static PAIRING: Mutex<Option<(String, Arc<Socket>, Instant)>> = Mutex::new(None);

fn shown(code: &str) -> String {
    format!("{}-{}-{}", &code[..3], &code[3..6], &code[6..])
}

fn changed(app: &AppHandle) {
    let _ = app.emit_to(crate::overlay::LABEL, "remoto", json!({ "tipo": "estado" }));
}

#[tauri::command]
pub fn remoto_emparejar(app: AppHandle) -> Result<String, String> {
    if !crate::settings::load(&app).remote_on {
        return Err("Enciende Remoto primero.".into());
    }
    let relay = relay(&app)?;
    if load(&app).celulares.len() >= MOST_PHONES {
        return Err("Ya hay cinco celulares. Quita uno antes de emparejar otro.".into());
    }
    remoto_cancelar();
    let code = new_code();
    let (room, key) = from_code(&code)?;
    let socket = Arc::new(join(&relay, &room)?);
    *PAIRING.lock().unwrap() = Some((code.clone(), socket.clone(), Instant::now()));
    let watch = socket.clone();
    std::thread::spawn(move || {
        std::thread::sleep(PAIRING_TIME);
        if PAIRING.lock().unwrap().as_ref().is_some_and(|(_, open, _)| Arc::ptr_eq(open, &watch)) {
            remoto_cancelar();
        }
    });
    let code_shown = shown(&code);
    std::thread::spawn(move || {
        let done = pair_with(&app, &socket, &key);
        let mut pairing = PAIRING.lock().unwrap();
        let mine = pairing.as_ref().is_some_and(|(_, open, _)| Arc::ptr_eq(open, &socket));
        if mine {
            *pairing = None;
        }
        drop(pairing);
        if !mine && done.is_err() {
            return;
        }
        let _ = app.emit_to(
            crate::overlay::LABEL,
            "remoto",
            match &done {
                Ok(name) => json!({ "tipo": "emparejado", "nombre": name }),
                Err(why) => json!({ "tipo": "fallo", "texto": crate::i18n::tr(why) }),
            },
        );
        if done.is_ok() {
            sync(&app);
        }
    });
    Ok(code_shown)
}

fn pair_with(app: &AppHandle, socket: &Socket, key: &[u8]) -> Result<String, String> {
    let started = Instant::now();
    loop {
        let (frame, binary) = socket.receive_bytes().map_err(|_| "Se acabó el tiempo para emparejar.".to_string())?;
        if started.elapsed() > PAIRING_TIME {
            return Err("Se acabó el tiempo para emparejar.".into());
        }
        if binary {
            continue;
        }
        let Ok(said) = serde_json::from_slice::<Value>(&frame) else { continue };
        if said["t"] != "hola" {
            continue;
        }
        let cel = unb64(said["cel"].as_str().unwrap_or("")).filter(|cel| cel.len() == candado::PUBLIC_LEN).ok_or("El celular mandó una llave rara.")?;
        let name: String = said["nombre"].as_str().unwrap_or("Celular").chars().filter(|c| !c.is_control()).take(40).collect();
        let mac = unb64(said["mac"].as_str().unwrap_or("")).unwrap_or_default();
        let mut signed = b"hola".to_vec();
        signed.extend_from_slice(&cel);
        signed.extend_from_slice(name.as_bytes());
        if !candado::same(&candado::hmac(key, &signed), &mac) {
            return Err("El código no coincide. Empieza otra vez.".into());
        }
        let pc = pc_key(app)?;
        let room = hex(&candado::random(16));
        let me = pc_name();
        let mut answer = b"listo".to_vec();
        answer.extend_from_slice(&pc.public());
        answer.extend_from_slice(&cel);
        answer.extend_from_slice(room.as_bytes());
        answer.extend_from_slice(me.as_bytes());
        send_json(socket, &json!({ "t": "listo", "pc": b64(&pc.public()), "sala": room, "nombre": me, "mac": b64(&candado::hmac(key, &answer)) }))?;
        let mut book = load(app);
        let id = hex(&candado::sha256(&cel)[..4]);
        book.celulares.retain(|phone| phone.id != id);
        book.celulares.push(Phone { id, nombre: name.clone(), cel: b64(&cel), sala: room, desde: now() });
        save(app, &book)?;
        return Ok(name);
    }
}

fn quote(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

fn curl(config: &str, seconds: u32) -> Result<(u16, String), String> {
    let mut child = Command::new("curl.exe")
        .args(["--silent", "--show-error", "--max-time", &seconds.to_string(), "--connect-timeout", "10", "--config", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|_| "No pude abrir curl.exe.".to_string())?;
    child.stdin.take().ok_or("sin entrada")?.write_all(config.as_bytes()).map_err(|e| e.to_string())?;
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    let (body, status) = text.rsplit_once('\n').ok_or("No hubo respuesta de Cloudflare. Revisa tu conexión.")?;
    let status = status.trim().parse::<u16>().unwrap_or(0);
    if status == 0 {
        return Err("No hubo respuesta de Cloudflare. Revisa tu conexión.".into());
    }
    Ok((status, body.to_string()))
}

fn cloudflare(token: &str, method: &str, path: &str, json: Option<&Value>, form: &[String]) -> Result<Value, String> {
    let mut config = format!(
        "url = \"{CLOUDFLARE}{}\"\nrequest = \"{method}\"\nheader = \"Authorization: Bearer {}\"\nwrite-out = \"\\n%{{http_code}}\"\n",
        quote(path),
        quote(token)
    );
    if let Some(json) = json {
        config += &format!("header = \"Content-Type: application/json\"\ndata = \"{}\"\n", quote(&json.to_string()));
    }
    for part in form {
        config += &format!("form = \"{}\"\n", quote(part));
    }
    let (status, body) = curl(&config, 90)?;
    let answer: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    if answer["success"] == true {
        return Ok(answer["result"].clone());
    }
    let first = &answer["errors"][0];
    Err(match (status, first["code"].as_i64()) {
        (401 | 403, _) | (_, Some(10000 | 9109 | 6003)) => "Cloudflare no aceptó la llave. Créala con la plantilla «Edit Cloudflare Workers» y pégala completa.".into(),
        _ => format!("Cloudflare respondió: {}", first["message"].as_str().filter(|m| !m.is_empty()).unwrap_or("un error sin explicación")),
    })
}

fn worker_parts(dir: &std::path::Path, with_migration: bool) -> Result<Vec<String>, String> {
    let mut metadata = json!({
        "main_module": "worker.js",
        "compatibility_date": WORKER_DATE,
        "bindings": [{ "type": "durable_object_namespace", "name": "SALAS", "class_name": "Sala" }],
    });
    if with_migration {
        metadata["migrations"] = json!({ "new_tag": "v1", "new_sqlite_classes": ["Sala"] });
    }
    let meta = dir.join("metadata.json");
    let code = dir.join("worker.js");
    std::fs::write(&meta, metadata.to_string()).map_err(|e| e.to_string())?;
    std::fs::write(&code, WORKER).map_err(|e| e.to_string())?;
    Ok(vec![
        format!("metadata=@{};type=application/json", meta.display()),
        format!("worker.js=@{};type=application/javascript+module;filename=worker.js", code.display()),
    ])
}

fn cloudflare_key_looks_wrong(token: &str) -> bool {
    token.len() < 30 || token.chars().any(|c| c.is_whitespace() || c.is_control())
}

fn create_relay(token: &str) -> Result<(String, bool), String> {
    let token = token.trim();
    if cloudflare_key_looks_wrong(token) {
        return Err("Eso no parece una llave de Cloudflare. Cópiala completa desde la página donde la creaste.".into());
    }
    let accounts = cloudflare(token, "GET", "/accounts?per_page=5", None, &[])?;
    let account = accounts[0]["id"].as_str().ok_or("La llave no ve ninguna cuenta. Créala con la plantilla «Edit Cloudflare Workers».")?.to_string();
    let base = format!("/accounts/{account}/workers");
    let subdomain = match cloudflare(token, "GET", &format!("{base}/subdomain"), None, &[]) {
        Ok(found) if found["subdomain"].as_str().is_some_and(|name| !name.is_empty()) => found["subdomain"].as_str().unwrap_or_default().to_string(),
        _ => {
            let name = format!("cocuyo-{}", hex(&candado::random(4)));
            cloudflare(token, "PUT", &format!("{base}/subdomain"), Some(&json!({ "subdomain": name })), &[])?["subdomain"].as_str().unwrap_or(&name).to_string()
        }
    };
    let dir = std::env::temp_dir().join(format!("cocuyo-punto-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let script = format!("{base}/scripts/{WORKER_NAME}");
    let uploaded = cloudflare(token, "PUT", &script, None, &worker_parts(&dir, true)?)
        .or_else(|why| if why.contains("migration") || why.contains("Durable Object") { cloudflare(token, "PUT", &script, None, &worker_parts(&dir, false)?) } else { Err(why) });
    let _ = std::fs::remove_dir_all(&dir);
    uploaded?;
    cloudflare(token, "POST", &format!("{script}/subdomain"), Some(&json!({ "enabled": true, "previews_enabled": false })), &[])?;
    let host = format!("{WORKER_NAME}.{subdomain}.workers.dev");
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(40) {
        if curl(&format!("url = \"https://{host}/\"\nwrite-out = \"\\n%{{http_code}}\"\n"), 8).is_ok_and(|(_, body)| body.contains("punto de cita")) {
            return Ok((host, true));
        }
        std::thread::sleep(Duration::from_secs(4));
    }
    Ok((host, false))
}

#[tauri::command]
pub async fn remoto_crear_punto(llave: String) -> Result<Value, String> {
    let (host, answers) = tauri::async_runtime::spawn_blocking(move || create_relay(&llave)).await.map_err(|e| e.to_string())?.map_err(|why| crate::i18n::tr(&why))?;
    Ok(json!({ "punto": format!("wss://{host}"), "responde": answers }))
}

#[tauri::command]
pub fn remoto_cancelar() {
    if let Some((_, socket, _)) = PAIRING.lock().unwrap().take() {
        socket.shut();
    }
}

#[derive(Serialize)]
pub struct PhoneState {
    id: String,
    nombre: String,
    desde: i64,
    conectado: bool,
}

#[derive(Serialize)]
pub struct State {
    activo: bool,
    punto: String,
    celulares: Vec<PhoneState>,
    emparejando: Option<String>,
    quedan: u64,
}

#[tauri::command]
pub fn remoto_estado(app: AppHandle) -> State {
    let links = LINKS.lock().unwrap();
    let pairing = PAIRING.lock().unwrap();
    let settings = crate::settings::load(&app);
    State {
        activo: settings.remote_on,
        punto: settings.remote_relay,
        celulares: load(&app)
            .celulares
            .into_iter()
            .map(|phone| PhoneState { conectado: links.get(&phone.id).is_some_and(|link| link.talking.load(Ordering::Relaxed)), id: phone.id, nombre: phone.nombre, desde: phone.desde })
            .collect(),
        emparejando: pairing.as_ref().map(|(code, _, _)| shown(code)),
        quedan: pairing.as_ref().map_or(0, |(_, _, since)| PAIRING_TIME.saturating_sub(since.elapsed()).as_secs()),
    }
}

#[tauri::command]
pub fn remoto_quitar(app: AppHandle, id: String) -> Result<(), String> {
    let mut book = load(&app);
    book.celulares.retain(|phone| phone.id != id);
    save(&app, &book)?;
    sync(&app);
    Ok(())
}

struct Session {
    incoming: Vec<u8>,
    outgoing: Vec<u8>,
    last_in: u64,
    next_out: u64,
}

struct Link {
    stop: AtomicBool,
    talking: AtomicBool,
    voice: AtomicBool,
    pieces: AtomicBool,
    warm: Mutex<Option<std::thread::JoinHandle<Result<Socket, String>>>>,
    hearing: Mutex<Option<(std::sync::mpsc::Sender<Vec<f32>>, Arc<AtomicBool>)>>,
    socket: Mutex<Option<Arc<Socket>>>,
    session: Mutex<Option<Session>>,
}

static LINKS: Mutex<Option<HashMap<String, Arc<Link>>>> = Mutex::new(None);

trait Links {
    fn get(&self, id: &str) -> Option<&Arc<Link>>;
}

impl Links for std::sync::MutexGuard<'_, Option<HashMap<String, Arc<Link>>>> {
    fn get(&self, id: &str) -> Option<&Arc<Link>> {
        self.as_ref().and_then(|links| links.get(id))
    }
}

impl Link {
    fn halt(&self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(socket) = self.socket.lock().unwrap().as_ref() {
            socket.shut();
        }
    }

    fn send(&self, kind: u8, data: &[u8]) -> Result<(), String> {
        let socket = self.socket.lock().unwrap().clone().ok_or("sin conexión")?;
        let mut session = self.session.lock().unwrap();
        let session = session.as_mut().ok_or("sin sesión")?;
        let counter = session.next_out;
        session.next_out += 1;
        let mut plain = Vec::with_capacity(data.len() + 1);
        plain.push(kind);
        plain.extend_from_slice(data);
        let sealed = candado::seal(&session.outgoing, &nonce(counter), AAD, &plain)?;
        let mut frame = counter.to_be_bytes().to_vec();
        frame.extend_from_slice(&sealed);
        socket.send_bytes(&frame, true)
    }
}

fn nonce(counter: u64) -> [u8; 12] {
    let mut out = [0u8; 12];
    out[4..].copy_from_slice(&counter.to_be_bytes());
    out
}

pub fn sync(app: &AppHandle) {
    let on = crate::settings::load(app).remote_on && relay(app).is_ok();
    let phones = if on { load(app).celulares } else { Vec::new() };
    let mut guard = LINKS.lock().unwrap();
    let links = guard.get_or_insert_with(HashMap::new);
    links.retain(|id, link| {
        let keep = phones.iter().any(|phone| phone.id == *id);
        if !keep {
            link.halt();
        }
        keep
    });
    for phone in phones {
        if links.contains_key(&phone.id) {
            continue;
        }
        let link = Arc::new(Link { stop: AtomicBool::new(false), talking: AtomicBool::new(false), voice: AtomicBool::new(true), pieces: AtomicBool::new(false), warm: Mutex::new(None), hearing: Mutex::new(None), socket: Mutex::new(None), session: Mutex::new(None) });
        links.insert(phone.id.clone(), link.clone());
        let app = app.clone();
        std::thread::Builder::new().name(format!("remoto-{}", phone.id)).spawn(move || keep_up(&app, &phone, &link)).ok();
    }
}

pub fn shutdown() {
    if let Some(links) = LINKS.lock().unwrap().take() {
        for link in links.values() {
            link.halt();
        }
    }
    remoto_cancelar();
}

fn keep_up(app: &AppHandle, phone: &Phone, link: &Arc<Link>) {
    let mut wait = Duration::from_secs(3);
    while !link.stop.load(Ordering::Relaxed) {
        let socket = match relay(app).and_then(|relay| join(&relay, &phone.sala)) {
            Ok(socket) => Arc::new(socket),
            Err(_) => {
                let until = Instant::now() + wait;
                while Instant::now() < until && !link.stop.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(500));
                }
                wait = (wait * 2).min(Duration::from_secs(60));
                continue;
            }
        };
        wait = Duration::from_secs(3);
        *link.socket.lock().unwrap() = Some(socket.clone());
        if link.stop.load(Ordering::Relaxed) {
            socket.shut();
        }
        listen(app, phone, link, &socket);
        if link.talking.swap(false, Ordering::Relaxed) {
            changed(app);
        }
        *link.session.lock().unwrap() = None;
        *link.socket.lock().unwrap() = None;
    }
}

fn listen(app: &AppHandle, phone: &Phone, link: &Arc<Link>, socket: &Socket) {
    let Some(cel) = unb64(&phone.cel) else { return };
    let Ok(pc) = pc_key(app) else { return };
    while let Ok((frame, binary)) = socket.receive_bytes() {
        if link.stop.load(Ordering::Relaxed) {
            return;
        }
        if !binary {
            let Ok(said) = serde_json::from_slice::<Value>(&frame) else { continue };
            if said["t"] == "inicio" {
                match greet(&pc, &cel, &said) {
                    Ok((session, answer)) => {
                        *link.session.lock().unwrap() = Some(session);
                        let greeted = send_json(socket, &answer).is_ok();
                        if greeted {
                            deliver_pending(link);
                        }
                        if greeted && !link.talking.swap(true, Ordering::Relaxed) {
                            let _ = app.emit_to(crate::overlay::LABEL, "note", crate::i18n::tr(&format!("Tu celular «{}» se conectó a Cocuyo.", phone.nombre)));
                            changed(app);
                        }
                    }
                    Err(why) if cfg!(debug_assertions) => eprintln!("[remoto] saludo rechazado: {why}"),
                    Err(_) => {}
                }
            } else if said["t"] == "cel-llego" || said["t"] == "cel-se-fue" {
                *link.session.lock().unwrap() = None;
                if said["t"] == "cel-se-fue" && link.talking.swap(false, Ordering::Relaxed) {
                    changed(app);
                }
            }
            continue;
        }
        let Some(plain) = open_frame(link, &frame) else { continue };
        let Some((&kind, body)) = plain.split_first() else { continue };
        if kind == AUDIO_PIECE {
            if let Some((pieces, _)) = link.hearing.lock().unwrap().as_ref() {
                let _ = pieces.send(samples_of(body));
            }
            continue;
        }
        if kind == JSON {
            match serde_json::from_slice::<Value>(body).ok().and_then(|said| said["tipo"].as_str().map(str::to_string)).as_deref() {
                Some("voz-empieza") => {
                    start_hearing(app, link);
                    continue;
                }
                Some(end @ ("voz-termina" | "voz-cancelada")) => {
                    if let Some((_, cancelled)) = link.hearing.lock().unwrap().take() {
                        cancelled.store(end == "voz-cancelada", Ordering::Relaxed);
                    }
                    continue;
                }
                _ => {}
            }
        }
        let (app, link, body) = (app.clone(), link.clone(), body.to_vec());
        std::thread::spawn(move || handle(&app, &link, kind, &body));
    }
}

fn greet(pc: &Pair, cel: &[u8], said: &Value) -> Result<(Session, Value), String> {
    let theirs = unb64(said["e"].as_str().unwrap_or("")).filter(|e| e.len() == candado::PUBLIC_LEN).ok_or("llave efímera rara")?;
    let mine = Pair::generate()?;
    let dh1 = mine.agree(&theirs)?;
    let dh2 = pc.agree(&theirs)?;
    let dh3 = mine.agree(cel)?;
    let mut salt_input = cel.to_vec();
    salt_input.extend_from_slice(&pc.public());
    salt_input.extend_from_slice(&theirs);
    salt_input.extend_from_slice(&mine.public());
    let salt = candado::sha256(&salt_input);
    let mut ikm = dh1.to_vec();
    ikm.extend_from_slice(&dh2);
    ikm.extend_from_slice(&dh3);
    let keys = candado::hkdf(&ikm, &salt, b"cocuyo-sesion-1", 64);
    let session = Session { incoming: keys[..32].to_vec(), outgoing: keys[32..].to_vec(), last_in: 0, next_out: 1 };
    let proof = candado::seal(&session.outgoing, &nonce(0), AAD, b"acepto")?;
    Ok((session, json!({ "t": "acepto", "e": b64(&mine.public()), "prueba": b64(&proof), "puede": CAN })))
}

fn open_frame(link: &Link, frame: &[u8]) -> Option<Vec<u8>> {
    if frame.len() < 8 + 16 {
        return None;
    }
    let counter = u64::from_be_bytes(frame[..8].try_into().ok()?);
    let mut session = link.session.lock().unwrap();
    let session = session.as_mut()?;
    if counter <= session.last_in {
        return None;
    }
    let plain = candado::open(&session.incoming, &nonce(counter), AAD, &frame[8..])?;
    session.last_in = counter;
    Some(plain)
}

fn handle(app: &AppHandle, link: &Link, kind: u8, body: &[u8]) {
    let text = match kind {
        JSON => match serde_json::from_slice::<Value>(body) {
            Ok(asked) if asked["tipo"] == "orden" => {
                warm_up(app, link);
                asked["texto"].as_str().unwrap_or("").chars().take(1000).collect::<String>()
            }
            Ok(asked) if asked["tipo"] == "prefs" => {
                link.voice.store(asked["voz"].as_bool().unwrap_or(true), Ordering::Relaxed);
                link.pieces.store(asked["trozos"].as_bool().unwrap_or(false), Ordering::Relaxed);
                return;
            }
            _ => return,
        },
        AUDIO => {
            warm_up(app, link);
            match heard(app, link, samples_of(body)) {
                Some(text) => text,
                None => return,
            }
        }
        _ => return,
    };
    order(app, link, &text);
}

fn samples_of(body: &[u8]) -> Vec<f32> {
    body.chunks_exact(2).map(|pair| f32::from(i16::from_le_bytes([pair[0], pair[1]])) / 32768.0).collect()
}

fn heard(app: &AppHandle, link: &Link, samples: Vec<f32>) -> Option<String> {
    match crate::voice::transcribe_samples(app, samples) {
        Ok(Some(text)) => {
            let _ = link.send(JSON, json!({ "tipo": "oido", "texto": text }).to_string().as_bytes());
            Some(text)
        }
        Ok(None) => {
            reply(app, link, None, Some(crate::i18n::tr("No te escuché. Mantén el botón mientras hablas.")), Vec::new());
            None
        }
        Err(why) => {
            reply(app, link, None, Some(crate::i18n::tr(&why)), Vec::new());
            None
        }
    }
}

fn start_hearing(app: &AppHandle, link: &Arc<Link>) {
    let (pieces, arriving) = std::sync::mpsc::channel();
    let cancelled = Arc::new(AtomicBool::new(false));
    *link.hearing.lock().unwrap() = Some((pieces, cancelled.clone()));
    let (app, link) = (app.clone(), link.clone());
    std::thread::spawn(move || hear_phone(&app, &link, arriving, &cancelled));
}

fn hear_phone(app: &AppHandle, link: &Link, arriving: std::sync::mpsc::Receiver<Vec<f32>>, cancelled: &AtomicBool) {
    let all = if crate::talk::wanted(app) {
        let (to_live, for_live) = std::sync::mpsc::channel();
        let (answered, kept) = std::thread::scope(|scope| {
            let tee = scope.spawn(move || {
                let mut all = Vec::new();
                while let Ok(samples) = arriving.recv_timeout(Duration::from_secs(70)) {
                    all.extend_from_slice(&samples);
                    let _ = to_live.send(samples);
                }
                all
            });
            let answered = live_turn(app, link, for_live, cancelled);
            (answered, tee.join().unwrap_or_default())
        });
        if answered {
            return;
        }
        kept
    } else {
        let mut all = Vec::new();
        while let Ok(samples) = arriving.recv_timeout(Duration::from_secs(70)) {
            all.extend_from_slice(&samples);
        }
        all
    };
    if cancelled.load(Ordering::Relaxed) {
        return;
    }
    if let Some(text) = heard(app, link, all) {
        order(app, link, &text);
    }
}

fn live_turn(app: &AppHandle, link: &Link, for_live: std::sync::mpsc::Receiver<Vec<f32>>, cancelled: &AtomicBool) -> bool {
    let speak = link.voice.load(Ordering::Relaxed);
    let mut piece = Vec::new();
    let mut voiced = false;
    let mut last_you = String::new();
    crate::cue::hold_voice(true);
    ASKED.with(|asked| *asked.borrow_mut() = Some(Vec::new()));
    let outcome = crate::talk::phone_turn(app, for_live, cancelled, &mut |said| match said {
        crate::talk::Said::You(text) => {
            if text != last_you {
                last_you = text.to_string();
                let _ = link.send(JSON, json!({ "tipo": "oido", "texto": text, "parcial": true }).to_string().as_bytes());
            }
        }
        crate::talk::Said::Me(text) => {
            let _ = link.send(JSON, json!({ "tipo": "diciendo", "texto": text }).to_string().as_bytes());
        }
        crate::talk::Said::Voice(pcm) => {
            if speak {
                piece.extend_from_slice(pcm);
                if !voiced || piece.len() >= PIECE_BYTES {
                    voiced |= link.send(VOICE_PIECE, &piece).is_ok();
                    piece.clear();
                }
            }
        }
    });
    if !piece.is_empty() {
        let _ = link.send(VOICE_PIECE, &piece);
    }
    let actions = ASKED.with(|asked| asked.borrow_mut().take()).unwrap_or_default();
    crate::cue::hold_voice(false);
    match outcome {
        Ok((said, answer)) if !said.is_empty() || !answer.is_empty() => {
            let _ = app.emit_to(crate::overlay::LABEL, "note", crate::i18n::tr("Orden desde tu celular."));
            if !said.is_empty() && said != last_you {
                let _ = link.send(JSON, json!({ "tipo": "oido", "texto": said }).to_string().as_bytes());
            }
            let answer = if answer.is_empty() { crate::i18n::tr("Hecho.") } else { answer };
            crate::ai::talked(&said, &answer);
            let _ = link.send(JSON, json!({ "tipo": "respuesta", "texto": answer, "acciones": actions, "hablada": voiced }).to_string().as_bytes());
            if voiced {
                let _ = link.send(JSON, json!({ "tipo": "voz-fin" }).to_string().as_bytes());
            }
            true
        }
        Ok(_) => {
            reply(app, link, None, Some(crate::i18n::tr("No te escuché. Mantén el botón mientras hablas.")), Vec::new());
            true
        }
        Err(why) => {
            if cfg!(debug_assertions) {
                eprintln!("[remoto] en vivo no salió: {why}");
            }
            voiced || cancelled.load(Ordering::Relaxed)
        }
    }
}

fn order(app: &AppHandle, link: &Link, text: &str) {
    if text.trim().is_empty() {
        return;
    }
    let _ = app.emit_to(crate::overlay::LABEL, "note", crate::i18n::tr("Orden desde tu celular."));
    crate::ai::maybe_distill(app);
    if let Some((said, actions)) = for_the_phone(text) {
        return reply(app, link, Some(crate::i18n::tr(&said)), None, actions);
    }
    let text = without_pc(text);
    crate::cue::hold_voice(true);
    ASKED.with(|asked| *asked.borrow_mut() = Some(Vec::new()));
    let done = crate::voice::run_order(app, &text);
    let actions = ASKED.with(|asked| asked.borrow_mut().take()).unwrap_or_default();
    crate::cue::hold_voice(false);
    if let Ok(said) = &done {
        crate::ai::talked(&text, said);
    }
    match done {
        Ok(said) => reply(app, link, Some(crate::i18n::tr(if said.trim().is_empty() { "Hecho." } else { &said })), None, actions),
        Err(why) => reply(app, link, None, Some(crate::i18n::tr(&why)), actions),
    }
}

fn live_voice(app: &AppHandle) -> Option<String> {
    let config = crate::settings::load(app);
    let voice = config.voice.trim();
    let gemini = config.ai_url.contains("googleapis.com") && config.ai_format != "anthropic";
    (gemini && !voice.is_empty() && !voice.contains(':') && voice != "loquendero").then(|| voice.to_string())
}

fn warm_up(app: &AppHandle, link: &Link) {
    if !link.voice.load(Ordering::Relaxed) || !link.pieces.load(Ordering::Relaxed) {
        return;
    }
    let Some(voice) = live_voice(app) else { return };
    let app = app.clone();
    *link.warm.lock().unwrap() = Some(std::thread::spawn(move || crate::talk::reader(&app, &voice)));
}

fn reply(app: &AppHandle, link: &Link, text: Option<String>, error: Option<String>, actions: Vec<Value>) {
    let warm = link.warm.lock().unwrap().take();
    let said = error.clone().or_else(|| text.clone()).unwrap_or_default();
    let voiced = link.voice.load(Ordering::Relaxed) && !said.trim().is_empty();
    if link.send(JSON, json!({ "tipo": "respuesta", "texto": text, "error": error, "acciones": actions, "voz": voiced }).to_string().as_bytes()).is_err() || !voiced {
        return;
    }
    if let (Some(warm), Some(voice)) = (warm, live_voice(app)) {
        let spoken = crate::i18n::tr(&said).replace(['«', '»'], "");
        let saved = crate::ai::voice_file(app, &spoken, &voice).ok().filter(|file| file.is_file());
        if saved.is_none() {
            let opened = warm.join().unwrap_or_else(|_| Err("el hilo se cayó".into()));
            if let (true, Err(why)) = (cfg!(debug_assertions), &opened) {
                eprintln!("[remoto] no abrió el lector: {why}");
            }
            if let Ok(socket) = opened {
                let (mut whole, mut piece, mut sent) = (Vec::new(), Vec::new(), true);
                let read = crate::talk::read_aloud(socket, &spoken, |pcm| {
                    whole.extend_from_slice(pcm);
                    piece.extend_from_slice(pcm);
                    if piece.len() >= PIECE_BYTES || whole.len() == piece.len() {
                        sent &= link.send(VOICE_PIECE, &piece).is_ok();
                        piece.clear();
                    }
                });
                if !piece.is_empty() {
                    sent &= link.send(VOICE_PIECE, &piece).is_ok();
                }
                if !whole.is_empty() {
                    let _ = link.send(JSON, json!({ "tipo": "voz-fin" }).to_string().as_bytes());
                    if read.is_ok() && sent {
                        let _ = crate::ai::keep_voice(app, &spoken, &voice, &whole);
                    }
                    return;
                }
                if cfg!(debug_assertions) {
                    eprintln!("[remoto] la voz en vivo no salió: {read:?}");
                }
            }
        }
    }
    match crate::cue::wav_of(&said) {
        Ok(wav) if wav.len() <= MOST_VOICE => {
            let _ = link.send(VOICE, &wav);
        }
        _ => {
            let _ = link.send(JSON, json!({ "tipo": "sin-voz" }).to_string().as_bytes());
        }
    }
}

thread_local! {
    static ASKED: RefCell<Option<Vec<Value>>> = const { RefCell::new(None) };
}

pub fn asking() -> bool {
    ASKED.with(|asked| asked.borrow().is_some())
}

pub fn ready(app: &AppHandle) -> bool {
    crate::settings::load(app).remote_on && !load(app).celulares.is_empty()
}

const KINDS: [&str; 9] = ["buscar", "mapa", "abrir", "llamar", "whatsapp", "alarma", "recordatorio", "copiar", "compartir"];

fn clean_action(args: &Value) -> Option<Value> {
    let kind = args["tipo"].as_str().filter(|kind| KINDS.contains(kind))?;
    let mut out = json!({ "tipo": kind });
    for (key, most) in [("texto", 2000), ("url", 2000), ("numero", 30), ("hora", 5), ("fecha", 10), ("titulo", 120)] {
        if let Some(value) = args[key].as_str().map(str::trim).filter(|value| !value.is_empty()) {
            out[key] = json!(value.chars().take(most).collect::<String>());
        }
    }
    let has = |key: &str| out.get(key).is_some();
    let complete = match kind {
        "abrir" => out["url"].as_str().is_some_and(|url| url.starts_with("https://") || url.starts_with("http://")),
        "llamar" => has("numero"),
        "alarma" | "recordatorio" => out["hora"].as_str().is_some_and(|hora| hora.len() == 5 && hora.as_bytes()[2] == b':'),
        "whatsapp" => has("texto") || has("numero"),
        _ => has("texto"),
    };
    complete.then_some(out)
}

static PENDING: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn deliver_pending(link: &Link) {
    let waiting = std::mem::take(&mut *PENDING.lock().unwrap());
    let mut left = Vec::new();
    for message in waiting {
        if link.send(JSON, message.as_bytes()).is_err() {
            left.push(message);
        }
    }
    PENDING.lock().unwrap().extend(left);
}

fn talking_links() -> Vec<Arc<Link>> {
    LINKS.lock().unwrap().as_ref().map(|links| links.values().filter(|link| link.talking.load(Ordering::Relaxed)).cloned().collect()).unwrap_or_default()
}

pub fn send_report(report: &Value) -> Result<(), String> {
    let message = json!({ "tipo": "informe", "informe": report }).to_string();
    if talking_links().iter().filter(|link| link.send(JSON, message.as_bytes()).is_ok()).count() > 0 {
        return Ok(());
    }
    let mut pending = PENDING.lock().unwrap();
    pending.push(message);
    if pending.len() > 5 {
        pending.remove(0);
    }
    Ok(())
}

pub fn to_phone(args: &Value) -> Result<String, String> {
    let action = clean_action(args).ok_or("Faltan datos para dejarlo en el celular.")?;
    if ASKED.with(|asked| asked.borrow_mut().as_mut().map(|list| list.push(action.clone())).is_some()) {
        return Ok("Queda en el celular, junto con tu respuesta.".into());
    }
    let links = talking_links();
    let message = json!({ "tipo": "respuesta", "acciones": [action] }).to_string();
    if links.iter().filter(|link| link.send(JSON, message.as_bytes()).is_ok()).count() == 0 {
        return Err("Tu celular no está conectado ahora. Abre Cocuyo en el celular y vuelve a pedirlo.".into());
    }
    Ok("Listo, quedó en el celular.".into())
}

const ON_PC: [&str; 6] = [" en el pc", " en mi pc", " en el computador", " en la compu", " on my pc", " no pc"];

fn without_pc(text: &str) -> String {
    let lower = text.to_lowercase();
    ON_PC.iter().find_map(|tail| lower.find(tail).map(|at| format!("{}{}", &text[..at], &text[at + tail.len()..]))).unwrap_or_else(|| text.to_string()).trim().to_string()
}

fn for_the_phone(text: &str) -> Option<(String, Vec<Value>)> {
    let lower = format!(" {}", text.to_lowercase());
    if ON_PC.iter().any(|tail| lower.contains(tail)) {
        return None;
    }
    let clean = crate::intent::normalize(text);
    for lead in ["como llego a ", "como llegar a ", "llevame a ", "directions to ", "como chego a ", "como chegar a "] {
        if let Some(place) = clean.strip_prefix(lead) {
            let place = &text[text.len() - place.len().min(text.len())..];
            return Some(("Te dejo el camino en el celular.".into(), vec![json!({ "tipo": "mapa", "texto": place.trim() })]));
        }
    }
    if let Some(found) = crate::agenda::for_phone(text) {
        let tipo = if found.alarm { "alarma" } else { "recordatorio" };
        let said = if found.alarm { format!("Alarma {}: tócala para dejarla en el celular.", found.said) } else { format!("Recordatorio {}: tócalo para dejarlo en el celular.", found.said) };
        return Some((said, vec![json!({ "tipo": tipo, "en": found.at * 1000, "titulo": found.what })]));
    }
    if let crate::intent::Intent::Google(query) = crate::intent::parse(text) {
        return Some(("Te la dejo para abrir en el celular.".into(), vec![json!({ "tipo": "buscar", "texto": as_said(text, &query) })]));
    }
    None
}

fn as_said(text: &str, query: &str) -> String {
    let folded: Vec<char> = text.chars().map(|c| crate::intent::normalize(&c.to_string()).chars().next().unwrap_or(' ')).collect();
    let wanted: Vec<char> = query.chars().collect();
    if wanted.is_empty() || wanted.len() > folded.len() {
        return query.to_string();
    }
    folded.windows(wanted.len()).position(|part| part == wanted.as_slice()).map_or_else(|| query.to_string(), |at| text.chars().skip(at).take(wanted.len()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_goes_and_comes_back() {
        for data in [&b""[..], b"a", b"ab", b"abc", &[0xff, 0xfe, 0x00, 0x10][..]] {
            assert_eq!(unb64(&b64(data)).unwrap(), data);
        }
        assert!(!b64(&[0xfb, 0xff]).contains(['+', '/', '=']));
        assert!(unb64("¡no!").is_none());
    }

    #[test]
    fn codes_use_only_clear_letters() {
        for _ in 0..50 {
            let code = new_code();
            assert_eq!(code.len(), 9);
            assert!(code.bytes().all(|c| LETTERS.contains(&c)), "{code}");
        }
    }

    #[test]
    fn some_orders_land_on_the_phone() {
        assert_eq!(for_the_phone("busca vuelos a Cartagena").unwrap().1[0]["texto"], "vuelos a Cartagena");
        assert_eq!(for_the_phone("Busca el estadio más cercano").unwrap().1[0]["texto"], "el estadio más cercano");
        assert_eq!(for_the_phone("cómo llego al estadio").map(|(_, a)| a[0]["tipo"].clone()), None);
        assert_eq!(for_the_phone("cómo llego a Bello").unwrap().1[0]["texto"], "Bello");
        assert_eq!(for_the_phone("ponme una alarma a las 6:30").unwrap().1[0]["tipo"], "alarma");
        assert!(for_the_phone("busca vuelos a Cartagena en el pc").is_none());
        assert!(for_the_phone("pon algo de salsa").is_none());
        assert_eq!(without_pc("busca gatos en el PC"), "busca gatos");
    }

    #[test]
    fn the_worker_goes_inside_cocuyo() {
        assert!(WORKER.contains("export class Sala"));
        assert!(include_str!("../../remoto/wrangler.toml").contains(WORKER_DATE));
        let dir = std::env::temp_dir().join("cocuyo-prueba-partes");
        std::fs::create_dir_all(&dir).unwrap();
        let parts = worker_parts(&dir, true).unwrap();
        assert!(parts[0].starts_with("metadata=@") && parts[1].contains("filename=worker.js"));
        let metadata: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("metadata.json")).unwrap()).unwrap();
        assert_eq!(metadata["migrations"]["new_sqlite_classes"][0], "Sala");
        assert_eq!(std::fs::read_to_string(dir.join("worker.js")).unwrap(), WORKER);
        let _ = std::fs::remove_dir_all(dir);
        assert_eq!(quote(r#"a"b\c"#), r#"a\"b\\c"#);
    }

    #[test]
    fn a_bad_key_never_leaves_the_pc() {
        assert!(cloudflare_key_looks_wrong("corta"));
        assert!(cloudflare_key_looks_wrong("una llave con espacios que es bastante larga para pasar"));
        assert!(!cloudflare_key_looks_wrong("AbCdEfGhIjKlMnOpQrStUvWxYz0123456789_-AbCd"));
    }

    #[test]
    fn phone_actions_are_checked() {
        assert_eq!(clean_action(&json!({ "tipo": "mapa", "texto": "Bello" })).unwrap()["texto"], "Bello");
        assert!(clean_action(&json!({ "tipo": "borrar", "texto": "todo" })).is_none());
        assert!(clean_action(&json!({ "tipo": "abrir", "url": "javascript:alert(1)" })).is_none());
        assert!(clean_action(&json!({ "tipo": "alarma", "hora": "seis" })).is_none());
        assert_eq!(clean_action(&json!({ "tipo": "alarma", "hora": "06:30", "titulo": "Gym", "otra": 1 })).unwrap(), json!({ "tipo": "alarma", "hora": "06:30", "titulo": "Gym" }));
        assert!(to_phone(&json!({ "tipo": "mapa", "texto": "Bello" })).is_err());
        ASKED.with(|asked| *asked.borrow_mut() = Some(Vec::new()));
        assert!(to_phone(&json!({ "tipo": "mapa", "texto": "Bello" })).is_ok());
        assert_eq!(ASKED.with(|asked| asked.borrow_mut().take()).unwrap().len(), 1);
    }

    #[test]
    fn a_tampered_or_repeated_frame_is_dropped() {
        let session = Session { incoming: vec![7; 32], outgoing: vec![9; 32], last_in: 0, next_out: 1 };
        let link = Link { stop: AtomicBool::new(false), talking: AtomicBool::new(true), voice: AtomicBool::new(true), pieces: AtomicBool::new(false), warm: Mutex::new(None), hearing: Mutex::new(None), socket: Mutex::new(None), session: Mutex::new(Some(session)) };
        let sealed = candado::seal(&[7; 32], &nonce(5), AAD, b"\x01{}").unwrap();
        let mut frame = 5u64.to_be_bytes().to_vec();
        frame.extend_from_slice(&sealed);
        assert_eq!(open_frame(&link, &frame).unwrap(), b"\x01{}");
        assert!(open_frame(&link, &frame).is_none(), "repetido");
        let mut later = 6u64.to_be_bytes().to_vec();
        let mut bad = candado::seal(&[7; 32], &nonce(6), AAD, b"\x01{}").unwrap();
        bad[0] ^= 1;
        later.extend_from_slice(&bad);
        assert!(open_frame(&link, &later).is_none(), "alterado");
        assert!(open_frame(&link, &[0u8; 10]).is_none(), "corto");
    }
}
