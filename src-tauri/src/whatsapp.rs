// whatsapp sin abrir whatsapp: gowa (github.com/aldinokemal/go-whatsapp-web-multidevice, mit) corre aca en tu pc, solo en 127.0.0.1
// y con una clave nueva en cada arranque, y le cuenta a cocuyo cada mensaje por un webhook local firmado. nada se baja si no lo conectas

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::apps::CREATE_NO_WINDOW;

const ZIP_URL: &str = "https://github.com/aldinokemal/go-whatsapp-web-multidevice/releases/download/v9.7.0/whatsapp_9.7.0_windows_amd64.zip";
const ZIP_SHA256: &str = "5a64e9db06fb0b5174e27b4c03c1563b0212bead674136eca06b985402752c36";
const ZIP_EXE: &str = "windows-amd64.exe";
const EXE: &str = "gowa.exe";
const DEVICE: &str = "cocuyo";
const KEEP: usize = 30;
const REPLY_WINDOW: Duration = Duration::from_secs(30 * 60);

struct Gateway {
    child: Child,
    port: u16,
    password: String,
    _job: Job,
}

struct Job(windows::Win32::Foundation::HANDLE);

unsafe impl Send for Job {}

impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

impl Drop for Gateway {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Clone, Serialize)]
pub struct Message {
    chat: String,
    name: String,
    text: String,
    #[serde(skip)]
    at: Instant,
    #[serde(skip)]
    read: bool,
    mine: bool,
    stamp: u64,
}

#[derive(Clone, Serialize)]
pub struct Line {
    yo: bool,
    texto: String,
    hora: u64,
}

static GATEWAY: Mutex<Option<Gateway>> = Mutex::new(None);
static SECRET: Mutex<String> = Mutex::new(String::new());
static HOOK: Mutex<u16> = Mutex::new(0);
static INBOX: Mutex<Vec<Message>> = Mutex::new(Vec::new());
static STARTING: Mutex<()> = Mutex::new(());
static AVATARS: Mutex<Vec<(String, Option<String>)>> = Mutex::new(Vec::new());

fn dir(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_local_data_dir().ok()?.join("whatsapp"))
}

fn installed(app: &AppHandle) -> bool {
    dir(app).is_some_and(|dir| dir.join(EXE).is_file())
}

fn install(app: &AppHandle) -> Result<(), String> {
    let dir = dir(app).ok_or("No encontré la carpeta de Cocuyo.")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let zip = dir.join("gowa.zip");
    let got = Command::new("curl.exe")
        .args(["-sL", "--fail", "--max-time", "300", "-o"])
        .arg(&zip)
        .arg(ZIP_URL)
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|e| e.to_string())?;
    let bytes = std::fs::read(&zip).unwrap_or_default();
    let _ = std::fs::remove_file(&zip);
    if !got.success() || bytes.is_empty() {
        return Err("No pude bajar la pasarela de WhatsApp. ¿Hay internet?".into());
    }
    let hash: String = crate::candado::sha256(&bytes).iter().map(|b| format!("{b:02x}")).collect();
    if hash != ZIP_SHA256 {
        return Err("La pasarela de WhatsApp llegó distinta a la publicada; no la instalé.".into());
    }
    let exe = crate::bandeja::zip_files(&bytes, |name| name == ZIP_EXE, 128 << 20).into_iter().next().ok_or("El zip de la pasarela no trae el programa.")?;
    std::fs::write(dir.join(EXE), exe.1).map_err(|e| e.to_string())
}

fn job_for(child: &Child) -> Option<Job> {
    use windows::Win32::System::JobObjects::*;
    let job = unsafe { CreateJobObjectW(None, None) }.ok()?;
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    let set = unsafe {
        SetInformationJobObject(job, JobObjectExtendedLimitInformation, &limits as *const _ as *const _, std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32)
    };
    let assigned = set.is_ok() && unsafe { AssignProcessToJobObject(job, windows::Win32::Foundation::HANDLE(child.as_raw_handle())) }.is_ok();
    let job = Job(job);
    assigned.then_some(job)
}

fn free_port() -> Option<u16> {
    TcpListener::bind("127.0.0.1:0").ok()?.local_addr().ok().map(|addr| addr.port())
}

fn token() -> String {
    crate::remoto::b64(&crate::candado::random(18))
}

fn call(method: &str, path: &str, body: Option<&Value>) -> Result<Value, String> {
    let (port, password) = GATEWAY.lock().unwrap().as_ref().map(|g| (g.port, g.password.clone())).ok_or("WhatsApp no está conectado.")?;
    let mut command = Command::new("curl.exe");
    command
        .args(["-s", "--max-time", "25", "-X", method, "-H", &format!("X-Device-Id: {DEVICE}"), "-u"])
        .arg(format!("cocuyo:{password}"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW);
    if body.is_some() {
        command.args(["-H", "Content-Type: application/json", "--data-binary", "@-"]);
    }
    let mut child = command.arg(format!("http://127.0.0.1:{port}{path}")).spawn().map_err(|e| e.to_string())?;
    if let (Some(body), Some(mut input)) = (body, child.stdin.take()) {
        input.write_all(body.to_string().as_bytes()).map_err(|e| e.to_string())?;
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    serde_json::from_slice(&out.stdout).map_err(|_| "La pasarela de WhatsApp no respondió.".to_string())
}

fn running() -> bool {
    GATEWAY.lock().unwrap().as_mut().is_some_and(|gateway| matches!(gateway.child.try_wait(), Ok(None)))
}

pub fn start(app: &AppHandle) -> Result<(), String> {
    let _once = STARTING.lock().unwrap();
    if running() {
        return Ok(());
    }
    if !installed(app) {
        install(app)?;
    }
    let dir = dir(app).ok_or("No encontré la carpeta de Cocuyo.")?;
    let hook = listen(app)?;
    let port = free_port().ok_or("No encontré un puerto libre.")?;
    let (password, secret) = (token(), token());
    *SECRET.lock().unwrap() = secret.clone();
    let groups = crate::settings::load(app).whatsapp_groups;
    let child = Command::new(dir.join(EXE))
        .args(["rest", "--os=Cocuyo"])
        .current_dir(&dir)
        .env("APP_HOST", "127.0.0.1")
        .env("APP_PORT", port.to_string())
        .env("APP_BASIC_AUTH", format!("cocuyo:{password}"))
        .env("WHATSAPP_WEBHOOK", format!("http://127.0.0.1:{hook}/wa"))
        .env("WHATSAPP_WEBHOOK_SECRET", &secret)
        .env("WHATSAPP_WEBHOOK_EVENTS", "message")
        .env("WHATSAPP_WEBHOOK_IGNORE_JIDS", if groups { "status@broadcast" } else { "@g.us,status@broadcast" })
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("La pasarela de WhatsApp no arrancó: {e}"))?;
    let job = job_for(&child).ok_or("No pude atar la pasarela de WhatsApp a Cocuyo.")?;
    *GATEWAY.lock().unwrap() = Some(Gateway { child, port, password, _job: job });
    let since = Instant::now();
    while call("GET", "/devices", None).is_err() {
        if since.elapsed() > Duration::from_secs(15) || !running() {
            GATEWAY.lock().unwrap().take();
            return Err("La pasarela de WhatsApp no respondió al arrancar.".into());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    let _ = call("POST", "/devices", Some(&json!({ "device_id": DEVICE })));
    Ok(())
}

pub fn stop() {
    GATEWAY.lock().unwrap().take();
}

fn status() -> (bool, String) {
    let said = call("GET", "/app/status", None).unwrap_or_default();
    let logged = said["results"]["is_logged_in"].as_bool().unwrap_or(false);
    let number = said["results"]["jid"].as_str().unwrap_or("").split(['@', ':']).next().unwrap_or("").to_string();
    (logged, number)
}

#[derive(Serialize)]
pub struct State {
    instalado: bool,
    corriendo: bool,
    conectado: bool,
    numero: String,
}

#[tauri::command]
pub fn whatsapp_estado(app: AppHandle) -> State {
    let corriendo = running();
    let (conectado, numero) = if corriendo { status() } else { (false, String::new()) };
    State { instalado: installed(&app), corriendo, conectado, numero }
}

fn qr_image() -> Result<String, String> {
    let said = call("GET", "/app/login", None)?;
    let link = said["results"]["qr_link"].as_str().ok_or_else(|| said["message"].as_str().unwrap_or("WhatsApp no dio el código QR.").to_string())?;
    if !link.starts_with("http://127.0.0.1:") {
        return Err("El código QR vino de un lugar raro.".into());
    }
    let out = Command::new("curl.exe").args(["-s", "--max-time", "15", link]).creation_flags(CREATE_NO_WINDOW).output().map_err(|e| e.to_string())?;
    if !out.stdout.starts_with(&[0x89, b'P', b'N', b'G']) {
        return Err("No pude traer el código QR.".into());
    }
    Ok(format!("data:image/png;base64,{}", crate::ai::base64(&out.stdout)))
}

#[tauri::command]
pub async fn whatsapp_conectar(app: AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        start(&app)?;
        remember(&app, true);
        if status().0 {
            return Ok(String::new());
        }
        qr_image()
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn whatsapp_qr() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(|| if status().0 { Ok(String::new()) } else { qr_image() }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn whatsapp_desconectar(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        if running() {
            let _ = call("GET", "/app/logout", None);
        }
        stop();
        INBOX.lock().unwrap().clear();
        remember(&app, false);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

fn remember(app: &AppHandle, on: bool) {
    if let Some(mark) = dir(app).map(|dir| dir.join("conectado")) {
        let _ = if on { std::fs::write(mark, b"") } else { std::fs::remove_file(mark) };
    }
}

pub fn resume(app: &AppHandle) {
    if dir(app).is_some_and(|dir| dir.join("conectado").is_file()) && installed(app) {
        let app = app.clone();
        std::thread::spawn(move || {
            if let Err(why) = start(&app) {
                crate::errores::tell(&app, &format!("WhatsApp: {why}"));
            }
        });
    }
}

fn listen(app: &AppHandle) -> Result<u16, String> {
    let mut port = HOOK.lock().unwrap();
    if *port != 0 {
        return Ok(*port);
    }
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    *port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let app = app.clone();
    std::thread::Builder::new()
        .name("whatsapp-avisos".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                if let Some(event) = receive(stream) {
                    arrived(&app, &event);
                }
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(*port)
}

pub fn signed(secret: &str, body: &[u8], signature: &str) -> bool {
    let expected: String = crate::candado::hmac(secret.as_bytes(), body).iter().map(|b| format!("{b:02x}")).collect();
    let given = signature.trim().strip_prefix("sha256=").unwrap_or("").to_lowercase();
    !secret.is_empty() && crate::candado::same(expected.as_bytes(), given.as_bytes())
}

fn receive(mut stream: TcpStream) -> Option<Value> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut data = Vec::new();
    let mut chunk = [0u8; 8192];
    let head_end = loop {
        let read = stream.read(&mut chunk).ok().filter(|&n| n > 0)?;
        data.extend_from_slice(&chunk[..read]);
        if let Some(at) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            break at + 4;
        }
        if data.len() > 64 * 1024 {
            return None;
        }
    };
    let head = String::from_utf8_lossy(&data[..head_end]).to_string();
    let header = |name: &str| head.lines().find_map(|line| line.split_once(':').filter(|(key, _)| key.trim().eq_ignore_ascii_case(name)).map(|(_, value)| value.trim().to_string()));
    let length: usize = header("content-length").and_then(|v| v.parse().ok()).filter(|&n| n <= 1 << 20)?;
    while data.len() < head_end + length {
        let read = stream.read(&mut chunk).ok().filter(|&n| n > 0)?;
        data.extend_from_slice(&chunk[..read]);
    }
    let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    let body = &data[head_end..head_end + length];
    let secret = SECRET.lock().unwrap().clone();
    if !head.starts_with("POST /wa ") || !signed(&secret, body, &header("x-hub-signature-256").unwrap_or_default()) {
        return None;
    }
    serde_json::from_slice(body).ok()
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

fn keep(message: Message) {
    let mut inbox = INBOX.lock().unwrap();
    let echo = message.mine && inbox.iter().rev().take(6).any(|old| old.mine && old.chat == message.chat && old.text == message.text && old.at.elapsed() < Duration::from_secs(20));
    if !echo {
        inbox.push(message);
    }
    let extra = inbox.len().saturating_sub(KEEP);
    inbox.drain(..extra);
}

fn thread(chat: &str) -> Vec<Line> {
    let inbox = INBOX.lock().unwrap();
    let ours: Vec<&Message> = inbox.iter().filter(|m| m.chat == chat).collect();
    ours[ours.len().saturating_sub(12)..].iter().map(|m| Line { yo: m.mine, texto: m.text.clone(), hora: m.stamp }).collect()
}

fn avatar_of(chat: &str) -> Option<Option<String>> {
    AVATARS.lock().unwrap().iter().find(|(known, _)| known == chat).map(|(_, face)| face.clone())
}

fn fetch_avatar(app: &AppHandle, chat: &str) {
    let face = call("GET", &format!("/user/avatar?phone={chat}&is_preview=true"), None)
        .ok()
        .and_then(|said| said["results"]["url"].as_str().filter(|url| url.starts_with("https://")).map(str::to_string))
        .and_then(|url| Command::new("curl.exe").args(["-s", "--max-time", "6", "--max-filesize", "400000", &url]).creation_flags(CREATE_NO_WINDOW).output().ok())
        .map(|out| out.stdout)
        .filter(|bytes| bytes.starts_with(&[0xFF, 0xD8]) || bytes.starts_with(&[0x89, b'P']))
        .map(|bytes| format!("data:image/{};base64,{}", if bytes[0] == 0xFF { "jpeg" } else { "png" }, crate::ai::base64(&bytes)));
    {
        let mut faces = AVATARS.lock().unwrap();
        faces.retain(|(known, _)| known != chat);
        faces.push((chat.to_string(), face.clone()));
        let extra = faces.len().saturating_sub(40);
        faces.drain(..extra);
    }
    if let Some(face) = face {
        let _ = app.emit_to(crate::overlay::LABEL, "whatsapp-avatar", json!({ "chat": chat, "avatar": face }));
    }
}

fn arrived(app: &AppHandle, event: &Value) {
    let said = &event["payload"];
    let text: String = said["body"].as_str().unwrap_or("").trim().chars().take(4000).collect();
    if event["event"] != "message" || text.is_empty() {
        return;
    }
    let chat = said["chat_id"].as_str().unwrap_or("").to_string();
    let mine = said["is_from_me"].as_bool().unwrap_or(false);
    let name = said["sender_display_name"].as_str().or(said["from_name"].as_str()).unwrap_or("Alguien").to_string();
    keep(Message { chat: chat.clone(), name: name.clone(), text: text.clone(), at: Instant::now(), read: mine, mine, stamp: now_ms() });
    if mine {
        let _ = app.emit_to(crate::overlay::LABEL, "whatsapp-hilo", json!({ "chat": chat, "hilo": thread(&chat) }));
        return;
    }
    let settings = crate::settings::load(app);
    if !settings.whatsapp_notices || (crate::placement::fullscreen() && !settings.whatsapp_gaming) {
        return;
    }
    let sonar = !settings.whatsapp_sound.is_empty() && !crate::cue::muted();
    let face = avatar_of(&chat);
    let _ = app.emit_to(
        crate::overlay::LABEL,
        "whatsapp",
        json!({ "chat": chat, "nombre": name, "texto": text, "hora": now_ms(), "hilo": thread(&chat), "sonar": sonar, "avatar": face.clone().flatten() }),
    );
    if face.is_none() {
        let app = app.clone();
        std::thread::spawn(move || fetch_avatar(&app, &chat));
    }
}

#[tauri::command]
pub async fn whatsapp_responder(chat: String, texto: String) -> Result<(), String> {
    let text = texto.trim().to_string();
    if text.is_empty() {
        return Err("Escribe algo.".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        send(&chat, &text)?;
        keep(Message { chat: chat.clone(), name: String::new(), text, at: Instant::now(), read: true, mine: true, stamp: now_ms() });
        INBOX.lock().unwrap().iter_mut().filter(|m| m.chat == chat).for_each(|m| m.read = true);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn whatsapp_hilo(chat: String) -> Vec<Line> {
    INBOX.lock().unwrap().iter_mut().filter(|m| m.chat == chat).for_each(|m| m.read = true);
    thread(&chat)
}

fn last_chat() -> Option<(String, String)> {
    INBOX.lock().unwrap().iter().rev().find(|message| !message.mine && message.at.elapsed() < REPLY_WINDOW).map(|message| (message.chat.clone(), message.name.clone()))
}

pub fn send(chat: &str, text: &str) -> Result<(), String> {
    let said = call("POST", "/send/message", Some(&json!({ "phone": chat, "message": text })))?;
    if said["code"] == "SUCCESS" {
        Ok(())
    } else {
        Err(format!("WhatsApp no lo mandó: {}", said["message"].as_str().unwrap_or("sin explicación")))
    }
}

pub fn contact(who: &str) -> Option<(String, String)> {
    let wanted = crate::intent::normalize(who);
    if wanted.trim().is_empty() {
        return None;
    }
    let said = call("GET", "/user/my/contacts", None).ok()?;
    let all: Vec<(String, String)> = said["results"]["data"]
        .as_array()?
        .iter()
        .filter_map(|c| Some((c["jid"].as_str()?.to_string(), c["name"].as_str().filter(|n| !n.trim().is_empty())?.to_string())))
        .filter(|(jid, _)| jid.ends_with("@s.whatsapp.net"))
        .collect();
    best_match(who, &all)
}

pub fn best_match(who: &str, all: &[(String, String)]) -> Option<(String, String)> {
    let mut scored: Vec<(f32, &(String, String))> = all.iter().map(|contact| (crate::intent::similarity(who, &contact.1), contact)).collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    let (best, contact) = scored.first()?;
    let runner = scored.get(1).map_or(0.0, |next| next.0);
    (*best >= 0.8 && best - runner >= 0.05).then(|| (*contact).clone())
}

fn replying(spoken: &str) -> Option<String> {
    let clean = crate::intent::normalize(spoken);
    const LEADS: [&str; 8] = ["respondele que ", "contestale que ", "respondele ", "contestale ", "reply that ", "reply ", "responda que ", "responda "];
    let lead = LEADS.iter().find(|lead| clean.starts_with(**lead))?;
    let skip = lead.split_whitespace().count();
    let text: Vec<&str> = spoken.split_whitespace().skip(skip).collect();
    let text = text.join(" ");
    let mut chars = text.chars();
    let first = chars.next()?;
    Some(first.to_uppercase().chain(chars).collect())
}

fn reading(spoken: &str) -> bool {
    let clean = crate::intent::normalize(spoken);
    ["que me escribieron", "lee mis mensajes", "leeme los mensajes", "tengo mensajes", "mensajes nuevos", "quien me escribio", "read my messages", "any messages"]
        .iter()
        .any(|ask| clean.contains(ask))
}

pub fn spoken(app: &AppHandle, spoken: &str) -> Option<Result<String, String>> {
    if !running() {
        return None;
    }
    if reading(spoken) {
        let mut inbox = INBOX.lock().unwrap();
        let fresh: Vec<String> = inbox.iter().filter(|m| !m.read && !m.mine).map(|m| format!("{}: {}", m.name, m.text)).collect();
        inbox.iter_mut().for_each(|m| m.read = true);
        return Some(Ok(match fresh.len() {
            0 => "No tienes mensajes nuevos de WhatsApp.".to_string(),
            1 => format!("Un mensaje. {}", fresh[0]),
            n => format!("{n} mensajes. {}", fresh.join(". ")),
        }));
    }
    let text = replying(spoken)?;
    let Some((chat, name)) = last_chat() else {
        return Some(Err("No tengo a quién responderle: no te ha llegado ningún WhatsApp en la última media hora.".into()));
    };
    let what = format!("Mandarle a {name} por WhatsApp: «{text}»");
    if !crate::agent::ask_first(app, &what, &format!("WhatsApp {name}")) {
        return Some(Ok("Quedó sin enviar.".into()));
    }
    Some(send(&chat, &text).map(|()| {
        keep(Message { chat: chat.clone(), name: String::new(), text: text.clone(), at: Instant::now(), read: true, mine: true, stamp: now_ms() });
        format!("Listo, le respondí a {name}.")
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_signed_notices_count() {
        let body = br#"{"event":"message"}"#;
        let good: String = crate::candado::hmac(b"clave", body).iter().map(|b| format!("{b:02x}")).collect();
        assert!(signed("clave", body, &format!("sha256={good}")));
        assert!(!signed("otra", body, &format!("sha256={good}")));
        assert!(!signed("clave", br#"{"event":"otro"}"#, &format!("sha256={good}")));
        assert!(!signed("", body, "sha256="));
    }

    #[test]
    fn finds_a_contact_even_if_misheard_but_never_guesses_between_two() {
        let book = vec![("1@s.whatsapp.net".to_string(), "Angela Ruiz".to_string()), ("2@s.whatsapp.net".to_string(), "Andrés".to_string()), ("3@s.whatsapp.net".to_string(), "Mamá".to_string())];
        assert_eq!(best_match("Angela", &book).map(|c| c.1), Some("Angela Ruiz".into()));
        assert_eq!(best_match("mama", &book).map(|c| c.1), Some("Mamá".into()));
        assert_eq!(best_match("Pedro", &book), None);
        let twins = vec![("1@s.whatsapp.net".to_string(), "Juan Pérez".to_string()), ("2@s.whatsapp.net".to_string(), "Juan Gómez".to_string())];
        assert_eq!(best_match("Juan", &twins), None);
    }

    #[test]
    fn hears_a_reply_and_a_reading() {
        assert_eq!(replying("Respóndele que ya voy saliendo").as_deref(), Some("Ya voy saliendo"));
        assert_eq!(replying("contéstale ok, nos vemos").as_deref(), Some("Ok, nos vemos"));
        assert_eq!(replying("pon música"), None);
        assert!(reading("¿Qué me escribieron?"));
        assert!(reading("léeme los mensajes"));
        assert!(!reading("escribe un mensaje"));
    }
}
