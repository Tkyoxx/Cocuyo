// whatsapp sin abrir whatsapp: gowa (github.com/aldinokemal/go-whatsapp-web-multidevice, mit) corre aca en tu pc, solo en 127.0.0.1
// y con una clave nueva en cada arranque, y le cuenta a cocuyo cada mensaje por un webhook local firmado. nada se baja si no lo conectas

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
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
const KEEP: usize = 150;
const REPLY_WINDOW: Duration = Duration::from_secs(30 * 60);
const MEDIA_DAYS: u64 = 3;
const QR_WINDOW: Duration = Duration::from_secs(160);
const SIGN_LEAST: u64 = 85;
const SIGN_MARGIN: u64 = 45;
const MAX_TRIES: usize = 3;
const TRIES_WINDOW: Duration = Duration::from_secs(30 * 60);

struct Gateway {
    child: Child,
    port: u16,
    password: String,
    _job: Job,
}

pub(crate) struct Job(windows::Win32::Foundation::HANDLE);

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

#[derive(Clone, Serialize, PartialEq, Debug)]
pub struct Media {
    tipo: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    ruta: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    nombre: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    enlace: Option<String>,
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
    media: Option<Media>,
    #[serde(skip)]
    id: String,
    #[serde(skip)]
    tick: u8,
}

const SENT: u8 = 1;
const DELIVERED: u8 = 2;
const SEEN: u8 = 3;

#[derive(Clone, Serialize)]
pub struct Line {
    yo: bool,
    texto: String,
    hora: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    media: Option<Media>,
    #[serde(skip_serializing_if = "Option::is_none")]
    estado: Option<&'static str>,
}

static GATEWAY: Mutex<Option<Gateway>> = Mutex::new(None);
static SECRET: Mutex<String> = Mutex::new(String::new());
static HOOK: Mutex<u16> = Mutex::new(0);
static INBOX: Mutex<Vec<Message>> = Mutex::new(Vec::new());
static STARTING: Mutex<()> = Mutex::new(());
static AVATARS: Mutex<Vec<(String, Option<String>)>> = Mutex::new(Vec::new());
static QR_AT: Mutex<Option<Instant>> = Mutex::new(None);
static FAILED: Mutex<&'static str> = Mutex::new("");
static TRIES: Mutex<Vec<(Instant, String)>> = Mutex::new(Vec::new());

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
        return Err(crate::i18n::tr("No pude bajar la pasarela de WhatsApp. Revisa tu internet y toca Conectar otra vez."));
    }
    let hash: String = crate::candado::sha256(&bytes).iter().map(|b| format!("{b:02x}")).collect();
    if hash != ZIP_SHA256 {
        return Err(crate::i18n::tr("La pasarela de WhatsApp llegó distinta a la publicada y no la instalé. Prueba en otra red."));
    }
    let exe = crate::bandeja::zip_files(&bytes, |name| name == ZIP_EXE, 128 << 20).into_iter().next().ok_or("El zip de la pasarela no trae el programa.")?;
    std::fs::write(dir.join(EXE), exe.1).map_err(|e| e.to_string())
}

pub(crate) fn job_for(child: &Child) -> Option<Job> {
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

fn call_form(path: &str, fields: &[(&str, String)], file: (&str, &Path)) -> Result<Value, String> {
    let (port, password) = GATEWAY.lock().unwrap().as_ref().map(|g| (g.port, g.password.clone())).ok_or_else(|| crate::i18n::tr("WhatsApp no está conectado."))?;
    let mut command = Command::new("curl.exe");
    command.args(["-s", "--max-time", "180", "-X", "POST", "-H", &format!("X-Device-Id: {DEVICE}"), "-u"]).arg(format!("cocuyo:{password}"));
    for (key, value) in fields {
        command.arg("--form-string").arg(format!("{key}={value}"));
    }
    command.arg("-F").arg(format!("{}=@{}", file.0, file.1.display()));
    let out = command.arg(format!("http://127.0.0.1:{port}{path}")).stdin(Stdio::null()).stderr(Stdio::null()).creation_flags(CREATE_NO_WINDOW).output().map_err(|e| e.to_string())?;
    serde_json::from_slice(&out.stdout).map_err(|_| crate::i18n::tr("La pasarela de WhatsApp no respondió."))
}

pub fn file_kind(name: &str) -> (&'static str, &'static str, &'static str) {
    let ext = Path::new(name).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "jpg" | "jpeg" | "png" | "webp" => ("/send/image", "image", "foto"),
        "mp4" | "mov" | "3gp" | "mkv" => ("/send/video", "video", "video"),
        "mp3" | "ogg" | "opus" | "m4a" | "aac" | "wav" => ("/send/audio", "audio", "nota"),
        _ => ("/send/file", "file", "documento"),
    }
}

#[tauri::command]
pub async fn whatsapp_enviar_archivo(app: AppHandle, chat: String, path: String, texto: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let source = PathBuf::from(&path);
        let meta = std::fs::metadata(&source).map_err(|_| "Ese archivo ya no está.".to_string())?;
        if !meta.is_file() || meta.len() > 64 << 20 {
            return Err("WhatsApp no deja mandar archivos de más de 64 MB.".into());
        }
        let name = source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let (endpoint, field, tipo) = file_kind(&name);
        let ext: String = source.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_else(|| "bin".into()).chars().filter(|c| c.is_ascii_alphanumeric()).take(8).collect();
        let outbox = dir(&app).ok_or("No encontré la carpeta de Cocuyo.")?.join("enviando");
        std::fs::create_dir_all(&outbox).map_err(|e| e.to_string())?;
        let temp = outbox.join(format!("envio-{}.{ext}", now_ms()));
        std::fs::copy(&source, &temp).map_err(|e| format!("No pude leer el archivo: {e}"))?;
        let caption = texto.trim().chars().take(1000).collect::<String>();
        let mut fields = vec![("phone", chat.clone())];
        if !caption.is_empty() && field != "audio" {
            fields.push(("caption", caption.clone()));
        }
        let said = call_form(endpoint, &fields, (field, &temp));
        let _ = std::fs::remove_file(&temp);
        let said = said?;
        if said["code"] != "SUCCESS" {
            return Err(format!("WhatsApp no lo mandó: {}", said["message"].as_str().unwrap_or("sin explicación")));
        }
        let id = said["results"]["message_id"].as_str().unwrap_or("").chars().take(80).collect();
        let media = Some(Media { tipo, ruta: None, nombre: Some(name), enlace: None });
        keep(Message { chat: chat.clone(), name: String::new(), text: caption, at: Instant::now(), read: true, mine: true, stamp: now_ms(), media, id, tick: SENT });
        mark_seen(&chat);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

fn call(method: &str, path: &str, body: Option<&Value>) -> Result<Value, String> {
    let (port, password) = GATEWAY.lock().unwrap().as_ref().map(|g| (g.port, g.password.clone())).ok_or_else(|| crate::i18n::tr("WhatsApp no está conectado."))?;
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
    serde_json::from_slice(&out.stdout).map_err(|_| crate::i18n::tr("La pasarela de WhatsApp no respondió."))
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
    let port = free_port().ok_or_else(|| crate::i18n::tr("No encontré un puerto libre."))?;
    let (password, secret) = (token(), token());
    *SECRET.lock().unwrap() = secret.clone();
    let groups = crate::settings::load(app).whatsapp_groups;
    let (output, writer) = std::io::pipe().map_err(|e| e.to_string())?;
    let child = Command::new(dir.join(EXE))
        .args(["rest", "--os=Cocuyo"])
        .current_dir(&dir)
        .env("APP_HOST", "127.0.0.1")
        .env("APP_PORT", port.to_string())
        .env("APP_BASIC_AUTH", format!("cocuyo:{password}"))
        .env("WHATSAPP_WEBHOOK", format!("http://127.0.0.1:{hook}/wa"))
        .env("WHATSAPP_WEBHOOK_SECRET", &secret)
        .env("WHATSAPP_WEBHOOK_EVENTS", "message,message.ack")
        .env("WHATSAPP_AUTO_DOWNLOAD_MEDIA", "true")
        .env("WHATSAPP_WEBHOOK_IGNORE_JIDS", if groups { "status@broadcast" } else { "@g.us,status@broadcast" })
        .stdin(Stdio::null())
        .stdout(writer.try_clone().map_err(|e| e.to_string())?)
        .stderr(writer)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|why| {
            crate::errores::note("whatsapp", &format!("gowa no arranco: {why}"));
            crate::i18n::tr("Windows no dejó abrir la pasarela de WhatsApp. Mira si tu antivirus bloqueó gowa.exe y toca Conectar otra vez.")
        })?;
    watch_output(output);
    let job = job_for(&child).ok_or_else(|| crate::i18n::tr("No pude atar la pasarela de WhatsApp a Cocuyo."))?;
    let media = dir.join("statics").join("media");
    let _ = std::fs::create_dir_all(&media);
    let _ = app.asset_protocol_scope().allow_directory(&media, true);
    std::thread::spawn(move || tidy_media(&media, Duration::from_secs(MEDIA_DAYS * 86_400)));
    *GATEWAY.lock().unwrap() = Some(Gateway { child, port, password, _job: job });
    let since = Instant::now();
    while call("GET", "/devices", None).is_err() {
        if since.elapsed() > Duration::from_secs(15) || !running() {
            GATEWAY.lock().unwrap().take();
            return Err(crate::i18n::tr("La pasarela de WhatsApp abrió pero no respondió. Toca Conectar otra vez; si se repite, reinicia Cocuyo."));
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    let _ = call("POST", "/devices", Some(&json!({ "device_id": DEVICE })));
    Ok(())
}

pub fn stop() {
    GATEWAY.lock().unwrap().take();
    *QR_AT.lock().unwrap() = None;
    *FAILED.lock().unwrap() = "";
}

fn fail_of(line: &str) -> Option<&'static str> {
    if line.contains("[PASSKEY]") && line.contains("pairing error") {
        return Some("rechazo");
    }
    if line.contains("[PASSKEY]") && line.contains("failed to send") {
        return Some("envio");
    }
    if line.contains("Client outdated (405)") {
        return Some("vieja");
    }
    if line.contains("Temporary ban connect failure") {
        return Some("cortada");
    }
    if !line.contains("error when get qrCode") {
        return None;
    }
    [("qrCode timeout", "vencio"), ("qrCode err-client-outdated", "vieja"), ("qrCode err-unexpected-state", "cortada"), ("qrCode err-scanned-without-multidevice", "multi"), ("qrCode error", "rechazo")]
        .iter()
        .find(|(said, _)| line.contains(said))
        .map(|(_, kind)| *kind)
}

fn without_numbers(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut digits = String::new();
    for c in line.chars().chain(std::iter::once(' ')) {
        if c.is_ascii_digit() {
            digits.push(c);
            continue;
        }
        out.push_str(if digits.len() >= 7 { "…" } else { &digits });
        digits.clear();
        out.push(c);
    }
    out.pop();
    out
}

fn watch_output(output: std::io::PipeReader) {
    std::thread::spawn(move || {
        for line in std::io::BufRead::split(std::io::BufReader::new(output), b'\n') {
            let Ok(line) = line else { return };
            let line = String::from_utf8_lossy(&line);
            if let Some(kind) = fail_of(&line) {
                *FAILED.lock().unwrap() = kind;
                crate::errores::note("whatsapp", &without_numbers(&line));
            }
        }
    });
}

fn qr_left() -> u64 {
    QR_AT.lock().unwrap().map_or(0, |at| QR_WINDOW.saturating_sub(at.elapsed()).as_secs())
}

fn pause_left() -> Option<u64> {
    let mut tries = TRIES.lock().unwrap();
    tries.retain(|(at, _)| at.elapsed() < TRIES_WINDOW);
    if tries.len() < MAX_TRIES {
        return None;
    }
    Some(TRIES_WINDOW.saturating_sub(tries[0].0.elapsed()).as_secs() / 60 + 1)
}

fn count_try(challenge: &str) {
    let mut tries = TRIES.lock().unwrap();
    if tries.last().is_none_or(|(_, last)| last != challenge) {
        tries.push((Instant::now(), challenge.to_string()));
    }
}

fn online() -> bool {
    Command::new("curl.exe").args(["-s", "-o", "NUL", "--max-time", "6", "https://web.whatsapp.com/"]).stdin(Stdio::null()).stderr(Stdio::null()).creation_flags(CREATE_NO_WINDOW).status().is_ok_and(|status| status.success())
}

fn no_code() -> String {
    crate::i18n::tr(if online() { "WhatsApp no dio el código. Espera un minuto y vuelve a pedirlo." } else { "No hay internet o tu red bloquea WhatsApp. Revisa la conexión y vuelve a pedirlo." })
}

fn saved_session() -> bool {
    let Ok(said) = call("GET", "/devices", None) else { return true };
    said["results"].as_array().is_some_and(|list| list.iter().any(|device| device["id"] == DEVICE && device["jid"].as_str().is_some_and(|jid| !jid.is_empty())))
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
    llave: String,
    codigo: String,
    segundos: u64,
    fallo: String,
    sesion: bool,
}

fn key_state() -> (&'static str, String) {
    let said = call("GET", "/app/passkey", None).unwrap_or_default();
    let results = &said["results"];
    match results["status"].as_str() {
        Some("awaiting_response") => ("firmar", String::new()),
        Some("awaiting_confirmation") if results["skip_handoff_ux"] == true => ("terminando", String::new()),
        Some("awaiting_confirmation") => ("confirmar", results["code"].as_str().unwrap_or("").to_string()),
        _ => ("", String::new()),
    }
}

#[tauri::command]
pub async fn whatsapp_estado(app: AppHandle) -> State {
    tauri::async_runtime::spawn_blocking(move || {
        let corriendo = running();
        let (conectado, numero) = if corriendo { status() } else { (false, String::new()) };
        if conectado {
            TRIES.lock().unwrap().clear();
            remember(&app, true);
        }
        let linking = corriendo && !conectado;
        let (llave, codigo) = if linking { key_state() } else { ("", String::new()) };
        let segundos = qr_left();
        let fallo = match *FAILED.lock().unwrap() {
            _ if !linking => "",
            "vencio" if segundos > 5 => "caida",
            kind => kind,
        };
        let sesion = linking && llave.is_empty() && segundos == 0 && saved_session();
        State { instalado: installed(&app), corriendo, conectado, numero, llave: llave.to_string(), codigo, segundos, fallo: fallo.to_string(), sesion }
    })
    .await
    .unwrap_or(State { instalado: false, corriendo: false, conectado: false, numero: String::new(), llave: String::new(), codigo: String::new(), segundos: 0, fallo: String::new(), sesion: false })
}

fn checked(said: Value) -> Result<(), String> {
    if said["code"] == "SUCCESS" {
        return Ok(());
    }
    let why = said["message"].as_str().unwrap_or("");
    crate::errores::note("whatsapp", &format!("llave rechazada: {}", without_numbers(why)));
    Err(crate::i18n::tr(if why.contains("not connected") || why.contains("no pending") {
        "Se venció la conexión con WhatsApp antes de mandar la firma."
    } else {
        "WhatsApp no aceptó la firma. Revisa que la llave sea de esta cuenta y que tengas internet."
    }))
}

#[tauri::command]
pub async fn whatsapp_llave_firmar() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| {
        let said = call("GET", "/app/passkey", None)?;
        let options = &said["results"]["challenge"];
        if said["results"]["status"] != "awaiting_response" || !options.is_object() {
            return Err(crate::i18n::tr("WhatsApp ya no está pidiendo la llave."));
        }
        let left = qr_left();
        if left < SIGN_LEAST {
            return Err(crate::i18n::tr("Se acabó el tiempo de este código."));
        }
        let challenge = options["challenge"].as_str().unwrap_or("");
        count_try(challenge);
        let assertion = crate::llave::sign(options, Duration::from_secs(left - SIGN_MARGIN))?;
        let now = call("GET", "/app/passkey", None)?;
        if now["results"]["challenge"]["challenge"].as_str() != Some(challenge) {
            return Err(crate::i18n::tr("WhatsApp ya no está pidiendo la llave."));
        }
        checked(call("POST", "/app/passkey/response", Some(&assertion))?)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn whatsapp_llave_confirmar() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| checked(call("POST", "/app/passkey/confirm", None)?)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn whatsapp_llave_cancelar() {
    crate::llave::cancel();
}

#[tauri::command]
pub fn whatsapp_abrir_web() -> Result<(), String> {
    const PAGE: &str = "https://web.whatsapp.com";
    match crate::llave::browser() {
        Some(browser) => Command::new(browser).arg(PAGE).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map(|_| ()).map_err(|e| e.to_string()),
        None => crate::apps::open(PAGE),
    }
}

fn qr_image() -> Result<String, String> {
    if let Some(minutes) = pause_left() {
        return Err(crate::i18n::tr(&format!("Ya van {MAX_TRIES} intentos con la llave. Para un rato y vuelve en {minutes} min: si insistes, WhatsApp puede bloquear la vinculación.")));
    }
    crate::llave::cancel();
    *FAILED.lock().unwrap() = "";
    *QR_AT.lock().unwrap() = None;
    let said = call("GET", "/app/login", None).map_err(|_| no_code())?;
    let Some(link) = said["results"]["qr_link"].as_str() else {
        let why = said["message"].as_str().unwrap_or("");
        if why.contains("already logged in") {
            return Ok(String::new());
        }
        crate::errores::note("whatsapp", &format!("sin codigo: {}", without_numbers(why)));
        if why.contains("session have been saved") {
            return Err(crate::i18n::tr("Tu sesión está guardada y está conectando. Si tarda, revisa tu internet; si sigue igual, toca Desconectar y vincúlalo otra vez."));
        }
        return Err(match *FAILED.lock().unwrap() {
            "vieja" => crate::i18n::tr("WhatsApp dice que esta pasarela ya está vieja. Actualiza Cocuyo y vuelve a intentarlo."),
            "cortada" => crate::i18n::tr("WhatsApp cortó la conexión mientras vinculabas. Puede ser un bloqueo temporal: espera un rato antes de probar otra vez."),
            _ => no_code(),
        });
    };
    if !link.starts_with("http://127.0.0.1:") {
        return Err(crate::i18n::tr("El código QR vino de un lugar raro."));
    }
    let out = Command::new("curl.exe").args(["-s", "--max-time", "15", link]).creation_flags(CREATE_NO_WINDOW).output().map_err(|e| e.to_string())?;
    if !out.stdout.starts_with(&[0x89, b'P', b'N', b'G']) {
        return Err(crate::i18n::tr("No pude traer el código QR."));
    }
    *QR_AT.lock().unwrap() = Some(Instant::now());
    Ok(format!("data:image/png;base64,{}", crate::ai::base64(&out.stdout)))
}

#[tauri::command]
pub async fn whatsapp_conectar(app: AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        start(&app)?;
        if saved_session() {
            let since = Instant::now();
            while since.elapsed() < Duration::from_secs(8) {
                if status().0 {
                    remember(&app, true);
                    return Ok(String::new());
                }
                std::thread::sleep(Duration::from_millis(300));
            }
            return Err(crate::i18n::tr("Tu sesión está guardada y está conectando. Si tarda, revisa tu internet; si sigue igual, toca Desconectar y vincúlalo otra vez."));
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
        crate::llave::cancel();
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
        if on != mark.is_file() {
            let _ = if on { std::fs::write(mark, b"") } else { std::fs::remove_file(mark) };
        }
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
    let twin = !message.id.is_empty() && inbox.iter().any(|old| old.id == message.id);
    let echo = twin || (message.mine && inbox.iter().rev().take(6).any(|old| old.mine && old.chat == message.chat && old.text == message.text && old.at.elapsed() < Duration::from_secs(20)));
    if !echo {
        inbox.push(message);
    }
    let extra = inbox.len().saturating_sub(KEEP);
    inbox.drain(..extra);
}

fn thread(chat: &str) -> Vec<Line> {
    let inbox = INBOX.lock().unwrap();
    let ours: Vec<&Message> = inbox.iter().filter(|m| m.chat == chat).collect();
    ours[ours.len().saturating_sub(12)..]
        .iter()
        .map(|m| Line {
            yo: m.mine,
            texto: m.text.clone(),
            hora: m.stamp,
            media: m.media.clone(),
            estado: m.mine.then_some(match m.tick {
                SEEN => "leido",
                DELIVERED => "entregado",
                _ => "listo",
            }),
        })
        .collect()
}

pub fn receipt(inbox: &mut [Message], ids: &[String], kind: &str) -> Vec<String> {
    let mut touched = Vec::new();
    for message in inbox.iter_mut().filter(|m| !m.id.is_empty() && ids.contains(&m.id)) {
        let changed = if message.mine {
            let tick = if kind.starts_with("read") || kind == "played" { SEEN } else if kind == "delivered" || kind.is_empty() { DELIVERED } else { 0 };
            let up = tick > message.tick;
            message.tick = message.tick.max(tick);
            up
        } else if kind.starts_with("read") {
            let fresh = !message.read || message.tick < SEEN;
            message.read = true;
            message.tick = SEEN;
            fresh
        } else {
            false
        };
        if changed && !touched.contains(&message.chat) {
            touched.push(message.chat.clone());
        }
    }
    touched
}

fn mark_seen(chat: &str) {
    let pending: Vec<String> = {
        let mut inbox = INBOX.lock().unwrap();
        inbox
            .iter_mut()
            .filter(|m| m.chat == chat && !m.mine && !m.id.is_empty() && m.tick < SEEN)
            .map(|m| {
                m.tick = SEEN;
                m.id.clone()
            })
            .collect()
    };
    if pending.is_empty() || !running() {
        return;
    }
    let chat = chat.to_string();
    std::thread::spawn(move || {
        for id in pending {
            let _ = call("POST", &format!("/message/{id}/read"), Some(&json!({ "phone": chat })));
        }
    });
}

fn tidy_media(media: &Path, older: Duration) {
    let Ok(files) = std::fs::read_dir(media) else { return };
    for file in files.flatten() {
        let old = file.metadata().ok().and_then(|m| m.modified().ok()).and_then(|t| t.elapsed().ok()).is_some_and(|age| age > older);
        if old && file.path().is_file() {
            let _ = std::fs::remove_file(file.path());
        }
    }
}

fn local(dir: &Path, relative: &str) -> Option<String> {
    let relative = relative.replace('\\', "/");
    let inside = relative.starts_with("statics/media/") && !relative.contains("..") && !relative.contains(':') && relative.len() < 300;
    let path = dir.join(&relative);
    (inside && path.is_file()).then(|| path.to_string_lossy().into_owned())
}

pub fn media_of(dir: &Path, said: &Value) -> Option<Media> {
    let file = |field: &Value| field.as_str().or(field["path"].as_str()).and_then(|path| local(dir, path));
    let text = |field: &Value, key: &str| field[key].as_str().map(str::trim).filter(|t| !t.is_empty()).map(|t| t.chars().take(120).collect::<String>());
    let plain = |tipo: &'static str, ruta: Option<String>, nombre: Option<String>| Some(Media { tipo, ruta, nombre, enlace: None });
    for (key, tipo) in [("sticker", "sticker"), ("image", "foto"), ("video_note", "video"), ("video", "video"), ("audio", "nota"), ("ptt", "nota")] {
        let field = &said[key];
        if !field.is_null() {
            return plain(tipo, file(field), None);
        }
    }
    let document = &said["document"];
    if !document.is_null() {
        let named = text(document, "filename").or_else(|| file(document).and_then(|p| Path::new(&p).file_name().map(|n| n.to_string_lossy().into_owned())));
        return plain("documento", file(document), named);
    }
    for key in ["location", "live_location"] {
        let place = &said[key];
        if let (Some(lat), Some(lon)) = (place["degreesLatitude"].as_f64(), place["degreesLongitude"].as_f64()) {
            let nombre = text(place, "name").or_else(|| text(place, "address"));
            return Some(Media { tipo: "ubicacion", ruta: None, nombre, enlace: Some(format!("https://www.google.com/maps?q={lat:.6},{lon:.6}")) });
        }
    }
    let contact = if said["contact"].is_null() { &said["contacts_array"][0] } else { &said["contact"] };
    if !contact.is_null() {
        return plain("contacto", None, text(contact, "displayName").or_else(|| text(contact, "phone_number")));
    }
    None
}

pub fn label(media: &Media) -> String {
    let name = media.nombre.clone().unwrap_or_default();
    let named = |icon: &str, fallback: &str| format!("{icon} {}", if name.is_empty() { crate::i18n::tr(fallback) } else { name.clone() });
    match media.tipo {
        "sticker" => crate::i18n::tr("Sticker"),
        "foto" => named("📷", "Foto"),
        "nota" => named("🎤", "Nota de voz"),
        "video" => named("🎥", "Video"),
        "documento" => named("📄", "Documento"),
        "ubicacion" => named("📍", "Ubicación"),
        _ => named("👤", "Contacto"),
    }
}

fn spoken_of(message: &Message) -> String {
    let Some(media) = &message.media else { return message.text.clone() };
    let what = crate::i18n::tr(match media.tipo {
        "sticker" => "te mandó un sticker",
        "foto" => "te mandó una foto",
        "nota" => "te mandó una nota de voz",
        "video" => "te mandó un video",
        "documento" => "te mandó un documento",
        "ubicacion" => "te mandó una ubicación",
        _ => "te mandó un contacto",
    });
    if message.text.is_empty() {
        what
    } else {
        format!("{what}: {}", message.text)
    }
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
    if event["event"] == "message.ack" {
        let ids: Vec<String> = said["ids"].as_array().map(|all| all.iter().filter_map(|id| id.as_str().map(str::to_string)).collect()).unwrap_or_default();
        let touched = receipt(&mut INBOX.lock().unwrap(), &ids, said["receipt_type"].as_str().unwrap_or(""));
        for chat in touched {
            let _ = app.emit_to(crate::overlay::LABEL, "whatsapp-hilo", json!({ "chat": chat, "hilo": thread(&chat) }));
        }
        return;
    }
    let text: String = said["body"].as_str().unwrap_or("").trim().chars().take(4000).collect();
    let media = dir(app).and_then(|dir| media_of(&dir, said));
    if event["event"] != "message" || (text.is_empty() && media.is_none()) {
        return;
    }
    let chat = said["chat_id"].as_str().unwrap_or("").to_string();
    let mine = said["is_from_me"].as_bool().unwrap_or(false);
    let name = said["sender_display_name"].as_str().or(said["from_name"].as_str()).unwrap_or("Alguien").to_string();
    let shown = match (&media, text.is_empty()) {
        (Some(media), true) => label(media),
        _ => text.clone(),
    };
    let id = said["id"].as_str().unwrap_or("").chars().take(80).collect();
    keep(Message { chat: chat.clone(), name: name.clone(), text: text.clone(), at: Instant::now(), read: mine, mine, stamp: now_ms(), media, id, tick: if mine { SENT } else { 0 } });
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
        json!({ "chat": chat, "nombre": name, "texto": shown, "hora": now_ms(), "hilo": thread(&chat), "sonar": sonar, "avatar": face.clone().flatten() }),
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
        let id = send(&chat, &text)?;
        keep(Message { chat: chat.clone(), name: String::new(), text, at: Instant::now(), read: true, mine: true, stamp: now_ms(), media: None, id, tick: SENT });
        INBOX.lock().unwrap().iter_mut().filter(|m| m.chat == chat).for_each(|m| m.read = true);
        mark_seen(&chat);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn whatsapp_hilo(chat: String) -> Vec<Line> {
    tauri::async_runtime::spawn_blocking(move || {
        INBOX.lock().unwrap().iter_mut().filter(|m| m.chat == chat).for_each(|m| m.read = true);
        mark_seen(&chat);
        let mine = thread(&chat);
        let older = if running() { stored(&chat, 30) } else { Vec::new() };
        merge(older, mine, 30)
    })
    .await
    .unwrap_or_default()
}

pub fn unix_ms(stamp: &str) -> Option<u64> {
    let (date, rest) = stamp.split_once('T')?;
    let mut parts = date.split('-').map(|n| n.parse::<i64>().ok());
    let (y, m, d) = (parts.next()??, parts.next()??, parts.next()??);
    let clock: String = rest.chars().take(8).collect();
    let mut hms = clock.split(':').map(|n| n.parse::<i64>().ok());
    let (h, min, s) = (hms.next()??, hms.next()??, hms.next()??);
    let tail = &rest[8.min(rest.len())..];
    let tail = tail.trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
    let offset = match tail.chars().next() {
        Some(sign @ ('+' | '-')) => {
            let mut zone = tail[1..].split(':').map(|n| n.parse::<i64>().ok());
            let minutes = zone.next()?? * 60 + zone.next().flatten().unwrap_or(0);
            if sign == '+' { minutes } else { -minutes }
        }
        _ => 0,
    };
    let (yy, mm) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let era = yy.div_euclid(400);
    let yoe = yy - era * 400;
    let doy = (153 * mm + 2) / 5 + d - 1;
    let days = era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468;
    let secs = days * 86_400 + h * 3600 + min * 60 + s - offset * 60;
    u64::try_from(secs).ok().map(|s| s * 1000)
}

fn stored(chat: &str, most: usize) -> Vec<(String, Line)> {
    let said = call("GET", &format!("/chat/{chat}/messages?limit={most}"), None).unwrap_or_default();
    said["results"]["data"]
        .as_array()
        .map(|all| {
            all.iter()
                .filter_map(|m| {
                    let yo = m["is_from_me"].as_bool().unwrap_or(false);
                    let texto = m["content"].as_str().map(str::trim).unwrap_or("").chars().take(4000).collect::<String>();
                    let kind = m["media_type"].as_str().unwrap_or("");
                    let media = match kind {
                        "" | "call" => None,
                        "image" => Some("foto"),
                        "sticker" => Some("sticker"),
                        "video" => Some("video"),
                        "audio" | "ptt" => Some("nota"),
                        "location" | "live_location" => Some("ubicacion"),
                        "contact" | "contacts" => Some("contacto"),
                        _ => Some("documento"),
                    }
                    .map(|tipo| Media { tipo, ruta: None, nombre: m["filename"].as_str().filter(|n| !n.is_empty()).map(str::to_string), enlace: None });
                    if texto.is_empty() && media.is_none() {
                        return None;
                    }
                    let hora = m["timestamp"].as_str().and_then(unix_ms).unwrap_or(0);
                    Some((m["id"].as_str().unwrap_or("").to_string(), Line { yo, texto, hora, media, estado: yo.then_some("listo") }))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn merge(older: Vec<(String, Line)>, fresh: Vec<Line>, most: usize) -> Vec<Line> {
    let mut all: Vec<Line> = older.into_iter().map(|(_, line)| line).filter(|old| !fresh.iter().any(|new| new.yo == old.yo && new.texto == old.texto && new.hora.abs_diff(old.hora) < 120_000)).collect();
    all.extend(fresh);
    all.sort_by_key(|line| line.hora);
    let extra = all.len().saturating_sub(most);
    all.drain(..extra);
    all
}

#[tauri::command]
pub fn whatsapp_abrir(app: AppHandle, que: String) -> Result<(), String> {
    if que.starts_with("https://www.google.com/maps?q=") && !que.contains(|c: char| c.is_whitespace() || c == '"') {
        return crate::apps::open(&que);
    }
    let media = dir(&app).ok_or("No encontré la carpeta de Cocuyo.")?.join("statics").join("media");
    let path = PathBuf::from(&que);
    if path.parent() != Some(media.as_path()) || !path.is_file() {
        return Err("Ese archivo no llegó por WhatsApp.".into());
    }
    const SAFE: [&str; 22] = ["pdf", "txt", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "odp", "csv", "jpg", "jpeg", "png", "webp", "gif", "mp4", "mov", "ogg", "mp3", "m4a"];
    let kind = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if SAFE.contains(&kind.as_str()) {
        crate::apps::open(&que)
    } else {
        Command::new("explorer.exe").arg(format!("/select,{que}")).spawn().map(|_| ()).map_err(|e| e.to_string())
    }
}

fn last_chat() -> Option<(String, String)> {
    INBOX.lock().unwrap().iter().rev().find(|message| !message.mine && message.at.elapsed() < REPLY_WINDOW).map(|message| (message.chat.clone(), message.name.clone()))
}

pub fn send(chat: &str, text: &str) -> Result<String, String> {
    let said = call("POST", "/send/message", Some(&json!({ "phone": chat, "message": text })))?;
    if said["code"] == "SUCCESS" {
        Ok(said["results"]["message_id"].as_str().unwrap_or("").chars().take(80).collect())
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

fn about_whatsapp(clean: &str) -> bool {
    ["whatsapp", "wasap", "guasap", "watsap", "wasa", "wpp", "whats", "mensaje", "message", "chat"].iter().any(|w| clean.contains(w))
}

fn reading(spoken: &str) -> bool {
    let clean = crate::intent::normalize(spoken);
    const SENDING: [&str; 16] = ["mandale", "mandarle", "enviale", "enviarle", "envia ", "manda ", "escribele", "escribirle", "dile ", "decile", "avisale", "preguntale", "respondele", "contestale", "send ", "text "];
    if SENDING.iter().any(|lead| clean.starts_with(lead)) || replying(spoken).is_some() {
        return false;
    }
    const ALWAYS: [&str; 12] = ["me escribio", "me escribieron", "me ha escrito", "me han escrito", "me hablaron", "me han hablado", "alguien me hablo", "quien me hablo", "read my messages", "any messages", "who texted me", "did anyone text"];
    const ASKING: [&str; 40] = [
        "me llego", "me llegaron", "me ha llegado", "me han llegado", "me mando", "me mandaron", "me dijo", "me dijeron", "me respondio", "me respondieron", "me contesto", "me contestaron",
        "tengo mensajes", "tengo algun", "tengo algo", "tengo whatsapp", "tengo nuevos", "hay mensajes", "hay algo", "hay algun", "hay whatsapp", "mensajes nuevos", "mensaje nuevo", "nuevos mensajes",
        "nuevo mensaje", "algo nuevo", "algun mensaje", "lee mis", "lee los", "leeme", "lee el whatsapp", "revisa", "mira si", "mira mi", "mira el", "chequea", "checa", "que hay en", "que dice", "check my",
    ];
    ALWAYS.iter().any(|ask| clean.contains(ask)) || (about_whatsapp(&clean) && ASKING.iter().any(|ask| clean.contains(ask)))
}

fn ago(stamp: u64) -> String {
    let minutes = now_ms().saturating_sub(stamp) / 60_000;
    match minutes {
        0 => crate::i18n::tr("hace un momento"),
        1 => crate::i18n::tr("hace un minuto"),
        2..=59 => crate::i18n::tr("hace {} minutos").replace("{}", &minutes.to_string()),
        60..=119 => crate::i18n::tr("hace una hora"),
        _ => crate::i18n::tr("hace {} horas").replace("{}", &(minutes / 60).to_string()),
    }
}

pub fn summary(inbox: &mut [Message]) -> String {
    let mut people: Vec<(String, Vec<String>)> = Vec::new();
    for message in inbox.iter().filter(|m| !m.read && !m.mine) {
        let said = spoken_of(message);
        match people.iter_mut().find(|(name, _)| *name == message.name) {
            Some((_, all)) => all.push(said),
            None => people.push((message.name.clone(), vec![said])),
        }
    }
    let last = inbox.iter().rev().find(|m| !m.mine && m.stamp + 6 * 3_600_000 > now_ms()).map(|m| (m.name.clone(), spoken_of(m), m.stamp));
    inbox.iter_mut().for_each(|m| m.read = true);
    if people.is_empty() {
        return match last {
            Some((name, said, stamp)) => format!("{} {name}, {}: {said}", crate::i18n::tr("Nada nuevo. Lo último fue de"), ago(stamp)),
            None => crate::i18n::tr("No tienes mensajes nuevos de WhatsApp."),
        };
    }
    let total: usize = people.iter().map(|(_, all)| all.len()).sum();
    let parts: Vec<String> = people
        .iter()
        .take(5)
        .map(|(name, all)| {
            let shown = all[all.len().saturating_sub(3)..].join(". ");
            if all.len() == 1 {
                format!("{name}: {shown}")
            } else {
                format!("{name} ({}): {shown}", all.len())
            }
        })
        .collect();
    let head = match (total, people.len()) {
        (1, _) => crate::i18n::tr("Un mensaje."),
        (n, 1) => crate::i18n::tr("{} mensajes.").replace("{}", &n.to_string()),
        (n, p) => crate::i18n::tr("{} mensajes de {} personas.").replacen("{}", &n.to_string(), 1).replacen("{}", &p.to_string(), 1),
    };
    let rest = people.len().saturating_sub(5);
    let more = if rest > 0 { format!(" {}", crate::i18n::tr("Y {} más.").replace("{}", &rest.to_string())) } else { String::new() };
    format!("{head} {}.{more}", parts.join(". ").trim_end_matches('.'))
}

pub fn unread_people() -> Vec<(String, usize)> {
    let mut people: Vec<(String, usize)> = Vec::new();
    for message in INBOX.lock().unwrap().iter().filter(|m| !m.read && !m.mine) {
        match people.iter_mut().find(|(name, _)| *name == message.name) {
            Some((_, n)) => *n += 1,
            None => people.push((message.name.clone(), 1)),
        }
    }
    people
}

pub fn connected() -> bool {
    running()
}

pub fn read_out(app: &AppHandle) -> Result<String, String> {
    if !running() {
        let hint = if installed(app) {
            "WhatsApp está desconectado de Cocuyo. Conéctalo en Ajustes → WhatsApp y te leo los mensajes sin abrir nada."
        } else {
            "Todavía no conectas WhatsApp a Cocuyo. Hazlo en Ajustes → WhatsApp y te leo los mensajes sin abrir nada."
        };
        return Err(crate::i18n::tr(hint));
    }
    if !status().0 {
        return Err(crate::i18n::tr("WhatsApp no tiene la sesión abierta: escanea el código en Ajustes → WhatsApp."));
    }
    let fresh: Vec<String> = INBOX.lock().unwrap().iter().filter(|m| !m.read && !m.mine).map(|m| m.chat.clone()).fold(Vec::new(), |mut all, chat| {
        if !all.contains(&chat) {
            all.push(chat);
        }
        all
    });
    let said = summary(&mut INBOX.lock().unwrap());
    fresh.iter().for_each(|chat| mark_seen(chat));
    let _ = app.emit_to(crate::overlay::LABEL, "whatsapp-leidos", ());
    Ok(said)
}

fn people_file(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_config_dir().ok()?.join("personas.json"))
}

fn people(app: &AppHandle) -> serde_json::Map<String, Value> {
    people_file(app).and_then(|file| std::fs::read(file).ok()).and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok()).and_then(|v| v.as_object().cloned()).unwrap_or_default()
}

fn keep_people(app: &AppHandle, all: &serde_json::Map<String, Value>) -> Result<(), String> {
    let file = people_file(app).ok_or("No encontré la carpeta de Cocuyo.")?;
    if let Some(dir) = file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(file, serde_json::to_vec_pretty(all).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct Persona {
    chat: String,
    nombre: String,
    contexto: String,
}

#[tauri::command]
pub fn whatsapp_personas(app: AppHandle) -> Vec<Persona> {
    people(&app)
        .into_iter()
        .map(|(chat, v)| Persona { chat, nombre: v["nombre"].as_str().unwrap_or("").to_string(), contexto: v["contexto"].as_str().unwrap_or("").to_string() })
        .collect()
}

#[tauri::command]
pub fn whatsapp_persona_borrar(app: AppHandle, chat: String) -> Result<(), String> {
    let mut all = people(&app);
    all.remove(&chat);
    keep_people(&app, &all)
}

pub fn who_is(who: &str) -> Option<(String, String)> {
    let mut known: Vec<(String, String)> = Vec::new();
    for m in INBOX.lock().unwrap().iter().filter(|m| !m.mine && m.chat.ends_with("@s.whatsapp.net")) {
        if !known.iter().any(|(chat, _)| *chat == m.chat) {
            known.push((m.chat.clone(), m.name.clone()));
        }
    }
    if let Some(found) = best_match(who, &known) {
        return Some(found);
    }
    let query: String = who.trim().bytes().take(60).map(|b| if b.is_ascii_alphanumeric() { (b as char).to_string() } else { format!("%{b:02X}") }).collect();
    let listed: Vec<(String, String)> = call("GET", &format!("/chats?limit=25&search={query}"), None)
        .ok()
        .and_then(|said| said["results"]["data"].as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|c| Some((c["jid"].as_str()?.to_string(), c["name"].as_str().filter(|n| !n.trim().is_empty())?.to_string())))
        .collect();
    best_match(who, &listed).or_else(|| contact(who))
}

fn history(chat: &str, name: &str, most: usize) -> Vec<String> {
    merge(stored(chat, most), thread(chat), most)
        .into_iter()
        .map(|line| {
            let said = match (&line.media, line.texto.is_empty()) {
                (Some(media), true) => format!("[{}]", label(media)),
                (Some(media), false) => format!("[{}] {}", label(media), line.texto.chars().take(600).collect::<String>()),
                _ => line.texto.chars().take(600).collect(),
            };
            format!("{}: {said}", if line.yo { "Yo" } else { name })
        })
        .collect()
}

pub fn chat_for_ai(app: &AppHandle, who: &str, most: usize) -> Result<String, String> {
    if !running() || !status().0 {
        return Err("WhatsApp no está conectado a Cocuyo.".into());
    }
    let most = most.clamp(5, 50);
    let (chat, name) = who_is(who).ok_or_else(|| format!("No encontré a «{who}» en tus chats de WhatsApp."))?;
    if !crate::agent::ask_first(app, &format!("Dejar que la IA lea tus últimos {most} mensajes con {name} para tener contexto"), &format!("leer chat {name}")) {
        return Ok(format!("La persona no quiso que leyeras el chat con {name}."));
    }
    let lines = history(&chat, &name, most);
    let card = people(app).get(&chat).and_then(|v| v["contexto"].as_str().map(str::to_string));
    let mut out = format!("Chat de WhatsApp con {name}.");
    if let Some(card) = card {
        out.push_str(&format!("\nLo que ya sabes de {name}: {card}"));
    }
    if lines.is_empty() {
        out.push_str("\nNo hay mensajes guardados con esta persona.");
    } else {
        out.push_str(&format!("\nÚltimos mensajes, del más viejo al más nuevo:\n{}", lines.join("\n")));
    }
    Ok(out)
}

pub fn save_person(app: &AppHandle, who: &str, context: &str) -> Result<String, String> {
    let context: String = context.trim().chars().take(700).collect();
    if context.is_empty() {
        return Err("La ficha está vacía.".into());
    }
    let (chat, name) = who_is(who).ok_or_else(|| format!("No encontré a «{who}» en tus chats de WhatsApp."))?;
    if !crate::agent::ask_first(app, &format!("Guardar esto sobre {name} para la próxima vez:\n«{context}»"), "") {
        return Ok(format!("La persona no quiso guardar la ficha de {name}."));
    }
    let mut all = people(app);
    all.insert(chat, json!({ "nombre": name, "contexto": context, "hora": now_ms() }));
    keep_people(app, &all)?;
    Ok(format!("Guardé lo que sé de {name}."))
}

pub fn asked(app: &AppHandle, spoken: &str) -> Option<Result<String, String>> {
    (reading(spoken) && (running() || about_whatsapp(&crate::intent::normalize(spoken)))).then(|| read_out(app))
}

pub fn send_to(app: &AppHandle, who: &str, text: &str) -> Result<String, String> {
    if !running() || !status().0 {
        return Err("WhatsApp no está conectado a Cocuyo.".into());
    }
    if who_is(who).is_none() {
        return Err(format!("No encontré a «{who}» en tus chats ni en tus contactos de WhatsApp."));
    }
    crate::agent::send_message(app, "WhatsApp", who, text)
}

pub fn send_scheduled(chat: &str, text: &str) -> Result<(), String> {
    for wait in [0u64, 20, 60, 180] {
        std::thread::sleep(Duration::from_secs(wait));
        if running() && status().0 {
            let id = send(chat, text)?;
            keep(Message { chat: chat.to_string(), name: String::new(), text: text.to_string(), at: Instant::now(), read: true, mine: true, stamp: now_ms(), media: None, id, tick: SENT });
            return Ok(());
        }
    }
    Err("WhatsApp no estaba conectado".into())
}

fn said_inside(spoken: &str, rest: &str) -> bool {
    let (full, rest) = (crate::intent::normalize(spoken), crate::intent::normalize(rest));
    full.trim().starts_with(rest.trim()) && {
        let tail = full.trim()[rest.trim().len()..].trim();
        tail.starts_with("en ") || tail.starts_with("dentro de")
    }
}

fn scheduling(app: &AppHandle, spoken: &str) -> Option<Result<String, String>> {
    let (when, rest) = crate::agenda::timed(spoken)?;
    if said_inside(spoken, &rest) {
        return None;
    }
    let (chat, who, text) = crate::agent::loose_message(&rest)?;
    if chat != "WhatsApp" {
        return None;
    }
    let (jid, name) = who_is(&who)?;
    let what = format!("Programar un WhatsApp a {name} para {}: «{text}»", crate::agenda::when_said(&when));
    if crate::settings::load(app).agent_confirm && !crate::agent::ask_first(app, &what, "") {
        return Some(Ok("Quedó sin programar.".into()));
    }
    Some(Ok(crate::agenda::add_whatsapp(app, when, &jid, &name, &text)))
}

fn sending(app: &AppHandle, spoken: &str) -> Option<Result<String, String>> {
    if !running() {
        return None;
    }
    if let Some(done) = scheduling(app, spoken) {
        return Some(done);
    }
    let (chat, who, text) = crate::agent::loose_message(spoken)?;
    if chat != "WhatsApp" || who_is(&who).is_none() {
        return None;
    }
    Some(crate::agent::send_message(app, &chat, &who, &text))
}

pub fn early(app: &AppHandle, spoken: &str) -> Option<Result<String, String>> {
    asked(app, spoken).or_else(|| sending(app, spoken))
}

pub fn spoken(app: &AppHandle, spoken: &str) -> Option<Result<String, String>> {
    if let Some(done) = early(app, spoken) {
        return Some(done);
    }
    if !running() {
        return None;
    }
    let text = replying(spoken)?;
    let Some((chat, name)) = last_chat() else {
        return Some(Err("No tengo a quién responderle: no te ha llegado ningún WhatsApp en la última media hora.".into()));
    };
    let what = format!("Mandarle a {name} por WhatsApp: «{text}»");
    if !crate::agent::ask_first(app, &what, &format!("WhatsApp {name}")) {
        return Some(Ok("Quedó sin enviar.".into()));
    }
    Some(send(&chat, &text).map(|id| {
        keep(Message { chat: chat.clone(), name: String::new(), text: text.clone(), at: Instant::now(), read: true, mine: true, stamp: now_ms(), media: None, id, tick: SENT });
        mark_seen(&chat);
        format!("Listo, le respondí a {name}.")
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_gowa_says_went_wrong() {
        assert_eq!(fail_of(r#"time="x" level=error msg="[LOGIN][cocuyo] error when get qrCode timeout <nil>""#), Some("vencio"));
        assert_eq!(fail_of(r#"msg="[LOGIN][cocuyo] error when get qrCode err-client-outdated <nil>""#), Some("vieja"));
        assert_eq!(fail_of(r#"msg="[LOGIN][cocuyo] error when get qrCode err-unexpected-state <nil>""#), Some("cortada"));
        assert_eq!(fail_of(r#"msg="[LOGIN][cocuyo] error when get qrCode err-scanned-without-multidevice <nil>""#), Some("multi"));
        assert_eq!(fail_of(r#"msg="[LOGIN][cocuyo] error when get qrCode error bad signature""#), Some("rechazo"));
        assert_eq!(fail_of(r#"msg="[PASSKEY][cocuyo] pairing error (continuation=true): nope""#), Some("rechazo"));
        assert_eq!(fail_of(r#"msg="[PASSKEY][cocuyo] failed to send passkey response: deadline""#), Some("envio"));
        assert_eq!(fail_of(r#"[Client ERROR] Client outdated (405) connect failure (client version: 2.3000.1)"#), Some("vieja"));
        assert_eq!(fail_of(r#"[Client WARN] Temporary ban connect failure: <failure reason="402"/>"#), Some("cortada"));
        assert_eq!(fail_of(r#"msg="[LOGIN][cocuyo] error when get qrCode success <nil>""#), None);
        assert_eq!(fail_of(r#"msg="[LOGIN][cocuyo] passkey pairing event: passkey-request""#), None);
        assert_eq!(fail_of("cualquier otra cosa"), None);
    }

    #[test]
    fn numbers_stay_out_of_the_log() {
        assert_eq!(without_numbers("error 428 de 573001234567@s.whatsapp.net"), "error 428 de …@s.whatsapp.net");
        assert_eq!(without_numbers("nada raro"), "nada raro");
        assert_eq!(without_numbers("573001234567"), "…");
    }

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
        assert!(reading("¿alguien me escribió al WhatsApp?"));
        assert!(reading("mira si alguien me escribió"));
        assert!(reading("revisa mi wasap"));
        assert!(reading("¿tengo mensajes nuevos?"));
        assert!(reading("¿me llegó algo al WhatsApp?"));
        assert!(reading("qué me dijo Angela por WhatsApp"));
        assert!(!reading("abre WhatsApp"));
        assert!(!reading("entra a whatsapp"));
        assert!(!reading("mándale un WhatsApp a Ana que hay comida"));
        assert!(!reading("escríbele a mamá por WhatsApp que ya me llegó el paquete"));
        assert!(!reading("respóndele que ya revisa el correo"));
    }

    fn message(name: &str, text: &str, media: Option<Media>) -> Message {
        Message { chat: name.into(), name: name.into(), text: text.into(), at: Instant::now(), read: false, mine: false, stamp: now_ms(), media, id: String::new(), tick: 0 }
    }

    #[test]
    fn whatsapp_times_turn_into_milliseconds() {
        assert_eq!(unix_ms("1970-01-01T00:00:01Z"), Some(1000));
        assert_eq!(unix_ms("2023-10-15T10:30:00Z"), Some(1_697_365_800_000));
        assert_eq!(unix_ms("2023-10-15T05:30:00-05:00"), Some(1_697_365_800_000));
        assert_eq!(unix_ms("2023-10-15T10:30:00.123456Z"), Some(1_697_365_800_000));
        assert_eq!(unix_ms("basura"), None);
    }

    #[test]
    fn stored_history_and_fresh_lines_merge_without_twins() {
        let line = |yo: bool, texto: &str, hora: u64| Line { yo, texto: texto.into(), hora, media: None, estado: None };
        let older = vec![("a".to_string(), line(false, "hola", 1_000)), ("b".to_string(), line(true, "qué más", 2_000)), ("c".to_string(), line(false, "bien", 3_000))];
        let fresh = vec![line(false, "bien", 3_500), line(false, "¿vienes?", 9_000)];
        let all = merge(older, fresh, 3);
        assert_eq!(all.iter().map(|l| l.texto.as_str()).collect::<Vec<_>>(), vec!["qué más", "bien", "¿vienes?"]);
    }

    #[test]
    fn a_time_inside_the_message_is_not_a_schedule() {
        assert!(said_inside("dile a Pedro que llego en 10 minutos", "dile a Pedro que llego"));
        assert!(!said_inside("mándale a Angela que buenos días mañana a las 7", "mándale a Angela que buenos días"));
        assert!(!said_inside("mañana a las 7 mándale a Angela que buenos días", "mándale a Angela que buenos días"));
    }

    #[test]
    fn ticks_only_go_up_and_my_own_reads_clear_the_count() {
        let mut inbox = vec![
            Message { id: "mio".into(), mine: true, tick: SENT, ..message("Ana", "voy", None) },
            Message { id: "suyo".into(), ..message("Ana", "¿vienes?", None) },
        ];
        assert_eq!(receipt(&mut inbox, &["mio".into()], "delivered"), vec!["Ana".to_string()]);
        assert_eq!(inbox[0].tick, DELIVERED);
        assert_eq!(receipt(&mut inbox, &["mio".into()], "read"), vec!["Ana".to_string()]);
        assert!(receipt(&mut inbox, &["mio".into()], "delivered").is_empty());
        assert_eq!(inbox[0].tick, SEEN);
        assert!(receipt(&mut inbox, &["suyo".into()], "delivered").is_empty());
        assert!(!inbox[1].read);
        assert_eq!(receipt(&mut inbox, &["suyo".into()], "read-self"), vec!["Ana".to_string()]);
        assert!(inbox[1].read);
        assert!(receipt(&mut inbox, &["otro".into()], "read").is_empty());
    }

    #[test]
    fn reads_who_wrote_grouped_and_says_stickers() {
        let sticker = Media { tipo: "sticker", ruta: None, nombre: None, enlace: None };
        let mut inbox = vec![message("Angela", "hola", None), message("Pedro", "", Some(sticker)), message("Angela", "¿vienes?", None)];
        let said = summary(&mut inbox);
        assert!(said.starts_with("3 mensajes de 2 personas."), "{said}");
        assert!(said.contains("Angela (2): hola. ¿vienes?"), "{said}");
        assert!(said.contains("Pedro: te mandó un sticker"), "{said}");
        assert!(inbox.iter().all(|m| m.read));
        let again = summary(&mut inbox);
        assert!(again.starts_with("Nada nuevo. Lo último fue de Angela"), "{again}");
    }

    #[test]
    fn media_stays_inside_the_gateway_folder() {
        let dir = std::env::temp_dir().join(format!("cocuyo-wa-{}", std::process::id()));
        let media = dir.join("statics").join("media");
        std::fs::create_dir_all(&media).unwrap();
        std::fs::write(media.join("a.webp"), b"RIFF").unwrap();
        let sticker = media_of(&dir, &json!({ "sticker": "statics/media/a.webp" })).unwrap();
        assert_eq!(sticker.tipo, "sticker");
        assert!(sticker.ruta.as_deref().is_some_and(|r| r.ends_with("a.webp")));
        assert_eq!(media_of(&dir, &json!({ "sticker": "statics/media/../../secreto.txt" })).unwrap().ruta, None);
        assert_eq!(media_of(&dir, &json!({ "image": { "path": "C:/Windows/win.ini", "caption": "hola" } })).unwrap().ruta, None);
        let place = media_of(&dir, &json!({ "location": { "degreesLatitude": 4.6, "degreesLongitude": -74.08, "name": "Parque" } })).unwrap();
        assert_eq!(place.nombre.as_deref(), Some("Parque"));
        assert!(place.enlace.unwrap().starts_with("https://www.google.com/maps?q=4.600000,-74.080000"));
        assert_eq!(media_of(&dir, &json!({ "body": "solo texto" })), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
