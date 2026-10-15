// hablar en vivo con gemini: te va escuchando mientras hablas y contesta con voz mientras piensa. como hitodama, pero gratis

use std::collections::VecDeque;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};
use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Networking::WinHttp::*;

const HOST: &str = "generativelanguage.googleapis.com";
const PATH: &str = "/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent";
const IN_RATE: u32 = 16_000;
const OUT_RATE: f64 = 24_000.0;
const CHUNK: usize = 1_600;
const PATIENCE: Duration = Duration::from_secs(90);
const STALE_AFTER: Duration = Duration::from_secs(8 * 60);
const LOOK: &str = "cocuyo_ver_pantalla";

pub(crate) struct Socket {
    session: *mut c_void,
    connect: *mut c_void,
    socket: *mut c_void,
    sending: Mutex<()>,
}

unsafe impl Send for Socket {}
unsafe impl Sync for Socket {}

pub(crate) enum Refused {
    Session,
    Unreachable,
    Offline,
    Status(u32, String),
    Upgrade,
}

impl Socket {
    pub(crate) fn open(key: &str) -> Result<Socket, String> {
        Socket::connect(HOST, 443, PATH, &format!("x-goog-api-key: {key}\r\n"), true).map_err(|refused| match refused {
            Refused::Session => "No pude abrir la conexión.".to_string(),
            Refused::Unreachable => "No llegué a Google.".to_string(),
            Refused::Offline => "No hay conexión con Google.".to_string(),
            Refused::Status(status, body) => {
                let said = serde_json::from_str::<Value>(&body).ok().and_then(|v| v["error"]["message"].as_str().map(str::to_string)).unwrap_or_default();
                format!("Gemini Live no aceptó la conexión ({status}). {said}").trim().to_string()
            }
            Refused::Upgrade => "Google no completó la conexión en vivo.".to_string(),
        })
    }

    pub(crate) fn connect(host: &str, port: u16, path: &str, headers: &str, secure: bool) -> Result<Socket, Refused> {
        unsafe {
            let session = WinHttpOpen(w!("Cocuyo"), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, PCWSTR::null(), PCWSTR::null(), 0);
            if session.is_null() {
                return Err(Refused::Session);
            }
            let connect = WinHttpConnect(session, &HSTRING::from(host), port, 0);
            let flags = if secure { WINHTTP_FLAG_SECURE } else { WINHTTP_OPEN_REQUEST_FLAGS(0) };
            let request = if connect.is_null() { std::ptr::null_mut() } else { WinHttpOpenRequest(connect, w!("GET"), &HSTRING::from(path), PCWSTR::null(), PCWSTR::null(), std::ptr::null(), flags) };
            let fail = |why: Refused| {
                if !request.is_null() {
                    let _ = WinHttpCloseHandle(request);
                }
                if !connect.is_null() {
                    let _ = WinHttpCloseHandle(connect);
                }
                let _ = WinHttpCloseHandle(session);
                Err(why)
            };
            if request.is_null() {
                return fail(Refused::Unreachable);
            }
            let _ = WinHttpSetOption(Some(request), WINHTTP_OPTION_UPGRADE_TO_WEB_SOCKET, None);
            let headers: Vec<u16> = headers.encode_utf16().collect();
            let sent = if headers.is_empty() { WinHttpSendRequest(request, None, None, 0, 0, 0) } else { WinHttpSendRequest(request, Some(&headers), None, 0, 0, 0) };
            if sent.is_err() || WinHttpReceiveResponse(request, std::ptr::null_mut()).is_err() {
                return fail(Refused::Offline);
            }
            let mut status = 0u32;
            let mut size = 4u32;
            let _ = WinHttpQueryHeaders(request, WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER, PCWSTR::null(), Some((&mut status as *mut u32).cast()), &mut size, std::ptr::null_mut());
            if status != 101 {
                let mut body = Vec::new();
                let mut chunk = [0u8; 4096];
                let mut read = 0u32;
                while WinHttpReadData(request, chunk.as_mut_ptr().cast(), chunk.len() as u32, &mut read).is_ok() && read > 0 && body.len() < 16_384 {
                    body.extend_from_slice(&chunk[..read as usize]);
                }
                return fail(Refused::Status(status, String::from_utf8_lossy(&body).into_owned()));
            }
            let socket = WinHttpWebSocketCompleteUpgrade(request, None);
            let _ = WinHttpCloseHandle(request);
            if socket.is_null() {
                let _ = WinHttpCloseHandle(connect);
                let _ = WinHttpCloseHandle(session);
                return Err(Refused::Upgrade);
            }
            Ok(Socket { session, connect, socket, sending: Mutex::new(()) })
        }
    }

    pub(crate) fn send_bytes(&self, data: &[u8], binary: bool) -> Result<(), String> {
        let _turn = self.sending.lock().unwrap();
        let kind = if binary { WINHTTP_WEB_SOCKET_BINARY_MESSAGE_BUFFER_TYPE } else { WINHTTP_WEB_SOCKET_UTF8_MESSAGE_BUFFER_TYPE };
        let code = unsafe { WinHttpWebSocketSend(self.socket, kind, Some(data)) };
        if code == 0 { Ok(()) } else { Err(format!("se cortó la conexión ({code})")) }
    }

    pub(crate) fn send(&self, message: &Value) -> Result<(), String> {
        let text = message.to_string();
        let _turn = self.sending.lock().unwrap();
        let code = unsafe { WinHttpWebSocketSend(self.socket, WINHTTP_WEB_SOCKET_UTF8_MESSAGE_BUFFER_TYPE, Some(text.as_bytes())) };
        if code == 0 { Ok(()) } else { Err(format!("se cortó la conexión ({code})")) }
    }

    pub(crate) fn receive(&self) -> Result<Value, String> {
        let (whole, _) = self.receive_bytes()?;
        serde_json::from_slice(&whole).map_err(|e| e.to_string())
    }

    pub(crate) fn receive_bytes(&self) -> Result<(Vec<u8>, bool), String> {
        let mut whole = Vec::new();
        let mut chunk = vec![0u8; 64 * 1024];
        loop {
            let mut read = 0u32;
            let mut kind = WINHTTP_WEB_SOCKET_BUFFER_TYPE(0);
            let code = unsafe { WinHttpWebSocketReceive(self.socket, chunk.as_mut_ptr().cast(), chunk.len() as u32, &mut read, &mut kind) };
            if code != 0 {
                return Err(format!("se cortó la conexión ({code})"));
            }
            if kind == WINHTTP_WEB_SOCKET_CLOSE_BUFFER_TYPE {
                let mut status = 0u16;
                let mut why = [0u8; 256];
                let mut used = 0u32;
                unsafe {
                    let _ = WinHttpWebSocketQueryCloseStatus(self.socket, &mut status, Some(why.as_mut_ptr().cast()), why.len() as u32, &mut used);
                }
                let reason = format!("{status} {}", String::from_utf8_lossy(&why[..used as usize]));
                return Err(if reason.trim().is_empty() { "Google cerró la conversación".into() } else { format!("Google cerró la conversación: {}", reason.trim()) });
            }
            whole.extend_from_slice(&chunk[..read as usize]);
            if whole.len() > 4_000_000 {
                return Err("mensaje demasiado grande".into());
            }
            if kind == WINHTTP_WEB_SOCKET_UTF8_MESSAGE_BUFFER_TYPE || kind == WINHTTP_WEB_SOCKET_BINARY_MESSAGE_BUFFER_TYPE {
                return Ok((whole, kind == WINHTTP_WEB_SOCKET_BINARY_MESSAGE_BUFFER_TYPE));
            }
        }
    }
}

impl Socket {
    pub(crate) fn shut(&self) {
        unsafe {
            let _ = WinHttpWebSocketShutdown(self.socket, 1000, None, 0);
        }
    }
}

impl Drop for Socket {
    fn drop(&mut self) {
        unsafe {
            let _ = WinHttpWebSocketClose(self.socket, 1000, None, 0);
            let _ = WinHttpCloseHandle(self.socket);
            let _ = WinHttpCloseHandle(self.connect);
            let _ = WinHttpCloseHandle(self.session);
        }
    }
}

static QUEUE: Mutex<VecDeque<f32>> = Mutex::new(VecDeque::new());
static SPEAKER_RATE: AtomicU32 = AtomicU32::new(0);
static SPEAKER_ON: AtomicBool = AtomicBool::new(false);

fn open_speaker() -> Result<(cpal::Stream, u32), String> {
    let device = cpal::default_host().default_output_device().ok_or("No hay altavoces.")?;
    let config = device.default_output_config().map_err(|e| e.to_string())?;
    let rate = config.sample_rate();
    let channels = config.channels() as usize;
    let format = config.sample_format();
    let config: cpal::StreamConfig = config.into();
    let next = || QUEUE.lock().unwrap().pop_front().unwrap_or(0.0);
    let stream = match format {
        cpal::SampleFormat::F32 => device.build_output_stream(
            config,
            move |data: &mut [f32], _: &_| {
                for frame in data.chunks_mut(channels.max(1)) {
                    frame.fill(next());
                }
            },
            |_| {},
            None,
        ),
        cpal::SampleFormat::I16 => device.build_output_stream(
            config,
            move |data: &mut [i16], _: &_| {
                for frame in data.chunks_mut(channels.max(1)) {
                    frame.fill((next().clamp(-1.0, 1.0) * 32767.0) as i16);
                }
            },
            |_| {},
            None,
        ),
        other => return Err(format!("formato de altavoz no soportado: {other:?}")),
    }
    .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok((stream, rate))
}

fn wake_speaker() {
    if SPEAKER_ON.swap(true, Ordering::Relaxed) {
        return;
    }
    let (ready, wait) = channel();
    std::thread::spawn(move || {
        let opened = open_speaker();
        let _ = ready.send(opened.as_ref().map(|(_, rate)| *rate).map_err(Clone::clone));
        let Ok((stream, _)) = opened else {
            SPEAKER_ON.store(false, Ordering::Relaxed);
            return;
        };
        let mut quiet_since = Instant::now();
        loop {
            std::thread::sleep(Duration::from_millis(250));
            if !QUEUE.lock().unwrap().is_empty() {
                quiet_since = Instant::now();
            } else if quiet_since.elapsed() > Duration::from_secs(20) {
                break;
            }
        }
        drop(stream);
        SPEAKER_ON.store(false, Ordering::Relaxed);
    });
    if let Ok(Ok(rate)) = wait.recv_timeout(Duration::from_secs(3)) {
        SPEAKER_RATE.store(rate, Ordering::Relaxed);
    }
}

pub fn hush() {
    QUEUE.lock().unwrap().clear();
}

pub(crate) struct Resampler {
    step: f64,
    pos: f64,
    pending: Vec<f32>,
}

impl Resampler {
    pub(crate) fn new(from: f64, to: f64) -> Self {
        Resampler { step: from / to, pos: 0.0, pending: Vec::new() }
    }

    pub(crate) fn feed(&mut self, samples: &[f32]) -> Vec<f32> {
        self.pending.extend_from_slice(samples);
        let mut out = Vec::with_capacity((samples.len() as f64 / self.step) as usize + 2);
        while self.pos + 1.0 < self.pending.len() as f64 {
            let at = self.pos as usize;
            let fraction = (self.pos - at as f64) as f32;
            out.push(self.pending[at] * (1.0 - fraction) + self.pending[at + 1] * fraction);
            self.pos += self.step;
        }
        let used = (self.pos as usize).min(self.pending.len());
        self.pending.drain(..used);
        self.pos -= used as f64;
        out
    }
}

fn play(pcm: &[u8], to_speaker: &mut Option<Resampler>) {
    wake_speaker();
    let rate = SPEAKER_RATE.load(Ordering::Relaxed);
    if rate == 0 {
        return;
    }
    let samples: Vec<f32> = pcm.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect();
    let resampler = to_speaker.get_or_insert_with(|| Resampler::new(OUT_RATE, rate as f64));
    let out = resampler.feed(&samples);
    QUEUE.lock().unwrap().extend(out);
}

pub(crate) fn pcm16(samples: &[f32]) -> Vec<u8> {
    samples.iter().flat_map(|s| ((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes()).collect()
}

struct Live {
    socket: Arc<Socket>,
    events: Mutex<Receiver<Value>>,
    since: Instant,
    dead: Arc<AtomicBool>,
}

static LIVE: Mutex<Option<Arc<Live>>> = Mutex::new(None);
static MODEL: Mutex<String> = Mutex::new(String::new());
static THINK: Mutex<String> = Mutex::new(String::new());

#[cfg(debug_assertions)]
pub fn try_model(model: &str, think: &str) {
    *MODEL.lock().unwrap() = model.to_string();
    *THINK.lock().unwrap() = think.to_string();
    forget();
}
static ANSWERING: AtomicBool = AtomicBool::new(false);
static GENERATION: AtomicU32 = AtomicU32::new(0);
static LEFTOVER: AtomicBool = AtomicBool::new(false);
static CIRCLED: Mutex<Option<Vec<u8>>> = Mutex::new(None);
static RESTING: Mutex<Option<Instant>> = Mutex::new(None);
const REST: Duration = Duration::from_secs(5 * 60);

fn out_of_quota(why: &str) -> bool {
    let why = why.to_lowercase();
    ["429", "quota", "exhausted", "cuota", "rate limit"].iter().any(|sign| why.contains(sign))
}

pub fn wanted(app: &AppHandle) -> bool {
    let config = crate::settings::load(app);
    let resting = RESTING.lock().unwrap().is_some_and(|since| since.elapsed() < REST);
    !resting && config.ai_live && crate::ai::mode(app).is_some() && config.ai_url.contains("googleapis.com") && config.ai_format != "anthropic"
}

fn model(app: &AppHandle) -> Result<String, String> {
    let chosen = crate::settings::load(app).live_model.trim().to_string();
    if !chosen.is_empty() {
        return Ok(chosen);
    }
    let mut known = MODEL.lock().unwrap();
    if known.is_empty() {
        let mut models = crate::ai::live_models(app)?;
        models.sort_by_key(|name| {
            let thinking = name.contains("thinking") as u8;
            let preview = name.contains("preview") as u8;
            let version: Vec<u32> = name.split(|c: char| !c.is_ascii_digit()).filter_map(|n| n.parse().ok()).take(2).collect();
            (thinking, std::cmp::Reverse(version), preview)
        });
        *known = models.into_iter().next().ok_or("Tu clave no tiene ningún modelo de voz en vivo.")?;
    }
    Ok(known.clone())
}

fn connect(app: &AppHandle) -> Result<Arc<Live>, String> {
    let opened = open_live(app);
    if let Err(why) = &opened {
        if out_of_quota(why) {
            *RESTING.lock().unwrap() = Some(Instant::now());
        }
    }
    opened
}

const READER: &str = "Eres un lector en voz alta. Lee exactamente el texto que te mandan, palabra por palabra y en su mismo idioma, con naturalidad.                       No respondas, no saludes, no añadas ni quites nada: solo léelo.";

pub(crate) fn reader(app: &AppHandle, voice: &str) -> Result<Socket, String> {
    let key = crate::ai::key(app).ok_or("Falta la clave de Gemini.")?;
    let model = model(app)?;
    let socket = Socket::open(&key)?;
    let mut setup = json!({ "setup": {
        "model": format!("models/{model}"),
        "generationConfig": { "responseModalities": ["AUDIO"], "speechConfig": { "voiceConfig": { "prebuiltVoiceConfig": { "voiceName": voice } } } },
        "systemInstruction": { "parts": [{ "text": READER }] },
    } });
    if cfg!(debug_assertions) {
        setup["setup"]["outputAudioTranscription"] = json!({});
    }
    socket.send(&setup)?;
    if socket.receive()?.get("setupComplete").is_none() {
        return Err("Gemini Live no arrancó.".into());
    }
    crate::ai::count(app, &model);
    Ok(socket)
}

pub(crate) fn read_aloud(socket: Socket, text: &str, mut chunk: impl FnMut(&[u8])) -> Result<(), String> {
    let socket = Arc::new(socket);
    let done = Arc::new(AtomicBool::new(false));
    let watch = (socket.clone(), done.clone());
    std::thread::spawn(move || {
        let started = Instant::now();
        while !watch.1.load(Ordering::Relaxed) && started.elapsed() < Duration::from_secs(45) {
            std::thread::sleep(Duration::from_millis(250));
        }
        if !watch.1.load(Ordering::Relaxed) {
            watch.0.shut();
        }
    });
    let read = (|| {
        socket.send(&json!({ "clientContent": { "turns": [{ "role": "user", "parts": [{ "text": text }] }], "turnComplete": true } }))?;
        let mut any = false;
        loop {
            let message = socket.receive()?;
            let content = &message["serverContent"];
            if let Some(text) = content["outputTranscription"]["text"].as_str().filter(|_| cfg!(debug_assertions)) {
                eprintln!("[lector] {text}");
            }
            for part in content["modelTurn"]["parts"].as_array().into_iter().flatten() {
                if let Some(data) = part["inlineData"]["data"].as_str() {
                    any = true;
                    chunk(&crate::ai::unbase64(data));
                }
            }
            if content["turnComplete"].as_bool() == Some(true) || content["interrupted"].as_bool() == Some(true) || message.get("goAway").is_some() {
                return if any { Ok(()) } else { Err("Gemini Live no devolvió voz.".to_string()) };
            }
        }
    })();
    done.store(true, Ordering::Relaxed);
    read
}

fn open_live(app: &AppHandle) -> Result<Arc<Live>, String> {
    {
        let mut live = LIVE.lock().unwrap();
        if let Some(open) = live.as_ref().filter(|live| !live.dead.load(Ordering::Relaxed) && live.since.elapsed() < STALE_AFTER) {
            return Ok(open.clone());
        }
        if let Some(old) = live.take() {
            old.socket.shut();
        }
    }
    let key = crate::ai::key(app).ok_or("Falta la clave de Gemini.")?;
    let model = model(app)?;
    let started = Instant::now();
    let socket = Arc::new(Socket::open(&key)?);
    let voice = crate::settings::load(app).live_voice;
    let voice = if voice.trim().is_empty() { "Charon".to_string() } else { voice };
    let mut tools = crate::agent::declarations(app);
    tools.push(json!({
        "name": LOOK,
        "description": "Mira la pantalla de la persona (una captura de lo que ve ahora). Úsala cuando pregunte por algo que está viendo: «¿qué hago aquí?», \
                        «¿qué dice esto?», «¿cómo paso este nivel?», «¿qué error es este?».",
    }));
    let mut setup = json!({ "setup": {
        "model": format!("models/{model}"),
        "generationConfig": { "responseModalities": ["AUDIO"], "speechConfig": { "voiceConfig": { "prebuiltVoiceConfig": { "voiceName": voice } } } },
        "systemInstruction": { "parts": [{ "text": crate::agent::voice_prompt(app) }] },
        "realtimeInputConfig": { "automaticActivityDetection": { "disabled": true } },
        "inputAudioTranscription": {},
        "outputAudioTranscription": {}
    } });
    match THINK.lock().unwrap().as_str() {
        "" => {}
        "0" => setup["setup"]["generationConfig"]["thinkingConfig"] = json!({ "thinkingBudget": 0 }),
        level => setup["setup"]["generationConfig"]["thinkingConfig"] = json!({ "thinkingLevel": level }),
    }
    if !tools.is_empty() {
        setup["setup"]["tools"] = json!([{ "functionDeclarations": tools }]);
    }
    socket.send(&setup)?;
    let first = socket.receive()?;
    if first.get("setupComplete").is_none() {
        return Err(format!("Gemini Live no arrancó: {}", first.to_string().chars().take(300).collect::<String>()));
    }
    crate::ai::count(app, &model);
    note(app, &format!("conectado en {} ms", started.elapsed().as_millis()));
    if cfg!(debug_assertions) {
        eprintln!("[vivo] conectado a {model} en {:?}", started.elapsed());
    }
    let (send, events) = channel();
    let dead = Arc::new(AtomicBool::new(false));
    let reader = (socket.clone(), dead.clone());
    std::thread::Builder::new()
        .name("vivo".into())
        .spawn(move || {
            let (socket, dead) = reader;
            loop {
                match socket.receive() {
                    Ok(message) => {
                        if message.get("goAway").is_some() {
                            dead.store(true, Ordering::Relaxed);
                        }
                        if send.send(message).is_err() {
                            break;
                        }
                    }
                    Err(why) => {
                        dead.store(true, Ordering::Relaxed);
                        let _ = send.send(json!({ "cerrada": why }));
                        break;
                    }
                }
            }
        })
        .map_err(|e| e.to_string())?;
    let live = Arc::new(Live { socket, events: Mutex::new(events), since: Instant::now(), dead });
    *LIVE.lock().unwrap() = Some(live.clone());
    Ok(live)
}

fn note(app: &AppHandle, line: &str) {
    let Ok(dir) = app.path().app_config_dir() else { return };
    let path = dir.join("vivo.log");
    let mut lines: Vec<String> = std::fs::read_to_string(&path).unwrap_or_default().lines().map(str::to_string).collect();
    let now = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    let model = MODEL.lock().unwrap().clone();
    lines.push(format!("{:02}:{:02}:{:02} {model} · {line}", now.wHour, now.wMinute, now.wSecond));
    let keep = lines.len().saturating_sub(200);
    let _ = std::fs::write(path, lines[keep..].join("\n"));
}

pub fn forget() {
    if let Some(old) = LIVE.lock().unwrap().take() {
        old.socket.shut();
    }
}

fn forget_this(live: &Arc<Live>) {
    let mut open = LIVE.lock().unwrap();
    if open.as_ref().is_some_and(|open| Arc::ptr_eq(open, live)) {
        open.take();
    }
    live.socket.shut();
}

struct Turn {
    ended: Arc<AtomicBool>,
    cancelled: Arc<AtomicBool>,
    picture: Option<std::sync::mpsc::Sender<Option<Vec<u8>>>>,
    result: Receiver<Result<(String, String), String>>,
}

static TURN: Mutex<Option<Turn>> = Mutex::new(None);

pub fn begin(app: &AppHandle) {
    start(app, false);
}

pub fn begin_lasso(app: &AppHandle) -> bool {
    start(app, true)
}

fn start(app: &AppHandle, with_picture: bool) -> bool {
    if !wanted(app) {
        return false;
    }
    CIRCLED.lock().unwrap().take();
    hush();
    GENERATION.fetch_add(1, Ordering::Relaxed);
    let taps = crate::mic::tap();
    let ended = Arc::new(AtomicBool::new(false));
    let cancelled = Arc::new(AtomicBool::new(false));
    let (done, result) = channel();
    let (picture, framed) = if with_picture {
        let (send, wait) = channel();
        (Some(send), Some(wait))
    } else {
        (None, None)
    };
    *TURN.lock().unwrap() = Some(Turn { ended: ended.clone(), cancelled: cancelled.clone(), picture, result });
    let app = app.clone();
    std::thread::spawn(wake_speaker);
    crate::ai::maybe_distill(&app);
    std::thread::spawn(move || {
        let outcome = connect(&app).and_then(|live| {
            let outcome = talk(&app, &live, taps, &ended, &cancelled, framed);
            if outcome.is_err() {
                forget_this(&live);
            }
            outcome
        });
        if let Err(why) = &outcome {
            note(&app, &format!("falló: {why}"));
        }
        let _ = done.send(outcome);
    });
    true
}

fn picture(png: &[u8]) -> Value {
    json!({ "realtimeInput": { "video": { "data": crate::ai::base64(png), "mimeType": "image/png" } } })
}

fn talk(
    app: &AppHandle,
    live: &Arc<Live>,
    taps: Receiver<(u32, Vec<f32>)>,
    ended: &AtomicBool,
    cancelled: &AtomicBool,
    framed: Option<Receiver<Option<Vec<u8>>>>,
) -> Result<(String, String), String> {
    live.socket.send(&json!({ "realtimeInput": { "activityStart": {} } }))?;
    let mut to_model: Option<Resampler> = None;
    let mut ready = Vec::new();
    loop {
        match taps.recv_timeout(Duration::from_millis(80)) {
            Ok((rate, samples)) => {
                let rate = rate.max(8_000);
                let resampler = to_model.get_or_insert_with(|| Resampler::new(rate as f64, IN_RATE as f64));
                ready.extend(resampler.feed(&samples));
                while ready.len() >= CHUNK {
                    let piece: Vec<f32> = ready.drain(..CHUNK).collect();
                    live.socket.send(&json!({ "realtimeInput": { "audio": { "data": crate::ai::base64(&pcm16(&piece)), "mimeType": "audio/pcm;rate=16000" } } }))?;
                }
            }
            Err(RecvTimeoutError::Timeout) if !ended.load(Ordering::Relaxed) => {}
            Err(_) => break,
        }
    }
    if cancelled.load(Ordering::Relaxed) {
        return Err("cancelado".into());
    }
    if !ready.is_empty() {
        live.socket.send(&json!({ "realtimeInput": { "audio": { "data": crate::ai::base64(&pcm16(&ready)), "mimeType": "audio/pcm;rate=16000" } } }))?;
    }
    if let Some(framed) = framed {
        if let Ok(Some(png)) = framed.recv_timeout(Duration::from_secs(6)) {
            live.socket.send(&picture(&png))?;
            *CIRCLED.lock().unwrap() = Some(png);
            live.socket.send(&json!({ "realtimeInput": { "text": "(La persona rodeó una parte de su pantalla mientras hablaba: mírala con cocuyo_ver_pantalla antes de responder.)" } }))?;
        }
    }
    let events = live.events.lock().unwrap();
    while events.try_recv().is_ok() {}
    live.socket.send(&json!({ "realtimeInput": { "activityEnd": {} } }))?;
    answer(app, live, &events, Instant::now())
}

pub(crate) enum Said<'a> {
    You(&'a str),
    Me(&'a str),
    Voice(&'a [u8]),
}

pub(crate) fn phone_turn(app: &AppHandle, audio: Receiver<Vec<f32>>, cancelled: &AtomicBool, phone: &mut dyn FnMut(Said)) -> Result<(String, String), String> {
    let live = connect(app)?;
    GENERATION.fetch_add(1, Ordering::Relaxed);
    let outcome = (|| {
        live.socket.send(&json!({ "realtimeInput": { "activityStart": {} } }))?;
        let mut ready = Vec::new();
        while let Ok(samples) = audio.recv_timeout(Duration::from_secs(70)) {
            ready.extend(samples);
            while ready.len() >= CHUNK {
                let piece: Vec<f32> = ready.drain(..CHUNK).collect();
                live.socket.send(&json!({ "realtimeInput": { "audio": { "data": crate::ai::base64(&pcm16(&piece)), "mimeType": "audio/pcm;rate=16000" } } }))?;
            }
        }
        if cancelled.load(Ordering::Relaxed) {
            return Err("cancelado".to_string());
        }
        if !ready.is_empty() {
            live.socket.send(&json!({ "realtimeInput": { "audio": { "data": crate::ai::base64(&pcm16(&ready)), "mimeType": "audio/pcm;rate=16000" } } }))?;
        }
        let events = live.events.lock().unwrap();
        while events.try_recv().is_ok() {}
        live.socket.send(&json!({ "realtimeInput": { "activityEnd": {} } }))?;
        ANSWERING.store(true, Ordering::Relaxed);
        let heard = listen(app, &live, &events, Instant::now(), Some(phone));
        ANSWERING.store(false, Ordering::Relaxed);
        heard
    })();
    if outcome.is_err() {
        forget_this(&live);
    }
    outcome
}

fn answer(app: &AppHandle, live: &Live, events: &Receiver<Value>, asked: Instant) -> Result<(String, String), String> {
    ANSWERING.store(true, Ordering::Relaxed);
    crate::cue::hold_voice(true);
    let heard = listen(app, live, events, asked, None);
    crate::cue::hold_voice(false);
    ANSWERING.store(false, Ordering::Relaxed);
    heard
}

fn listen(app: &AppHandle, live: &Live, events: &Receiver<Value>, asked: Instant, mut phone: Option<&mut dyn FnMut(Said)>) -> Result<(String, String), String> {
    let mine = GENERATION.load(Ordering::Relaxed);
    let mut said = String::new();
    let mut reply = String::new();
    let mut to_speaker = None;
    let mut first_sound = None;
    let mut used = Vec::new();
    let mut owed = false;
    let mut heard_anything = false;
    loop {
        let cut = GENERATION.load(Ordering::Relaxed) != mine;
        let message = match events.recv_timeout(if cut { Duration::from_millis(1500) } else { PATIENCE }) {
            Ok(message) => message,
            Err(_) if cut => {
                LEFTOVER.store(true, Ordering::Relaxed);
                return Ok((said.trim().to_string(), reply.trim().to_string()));
            }
            Err(_) => return Err("Gemini se quedó callado.".to_string()),
        };
        if let Some(why) = message.get("cerrada").and_then(Value::as_str) {
            return Err(why.to_string());
        }
        if let Some(content) = message.get("serverContent") {
            if content["turnComplete"].as_bool() == Some(true) && !heard_anything && LEFTOVER.swap(false, Ordering::Relaxed) {
                continue;
            }
            if let Some(text) = content["inputTranscription"]["text"].as_str() {
                heard_anything = true;
                said.push_str(text);
                if let Some(phone) = phone.as_mut() {
                    phone(Said::You(said.trim()));
                } else if reply.is_empty() && !cut {
                    let _ = app.emit_to(crate::overlay::LABEL, "live-text", json!({ "who": "you", "text": said.trim() }));
                }
            }
            if let Some(text) = content["outputTranscription"]["text"].as_str() {
                heard_anything = true;
                reply.push_str(text);
                if let Some(phone) = phone.as_mut() {
                    phone(Said::Me(reply.trim()));
                } else if !cut {
                    let _ = app.emit_to(crate::overlay::LABEL, "live-text", json!({ "who": "app", "text": reply.trim() }));
                }
            }
            for part in content["modelTurn"]["parts"].as_array().into_iter().flatten() {
                heard_anything = true;
                owed = false;
                if let Some(data) = part["inlineData"]["data"].as_str() {
                    if let Some(phone) = phone.as_mut() {
                        first_sound.get_or_insert_with(|| asked.elapsed());
                        phone(Said::Voice(&crate::ai::unbase64(data)));
                    } else if !cut {
                        first_sound.get_or_insert_with(|| asked.elapsed());
                        play(&crate::ai::unbase64(data), &mut to_speaker);
                    }
                }
            }
            if content["interrupted"].as_bool() == Some(true) {
                hush();
                if cut {
                    owed = false;
                }
            }
            if content["turnComplete"].as_bool() == Some(true) && (!owed || cut) {
                if cfg!(debug_assertions) {
                    eprintln!("[vivo] primera voz a los {first_sound:?}, todo en {:?}", asked.elapsed());
                }
                let ms = |at: Option<Duration>| at.map_or("-".to_string(), |at| at.as_millis().to_string());
                note(app, &format!("voz {} ms · fin {} ms · herramientas: {}", ms(first_sound), asked.elapsed().as_millis(), if used.is_empty() { "ninguna".to_string() } else { used.join(", ") }));
                return Ok((said.trim().to_string(), reply.trim().to_string()));
            }
        }
        if let Some(calls) = message["toolCall"]["functionCalls"].as_array() {
            heard_anything = true;
            let mut responses = Vec::new();
            for call in calls {
                let name = call["name"].as_str().unwrap_or("");
                let args = call.get("args").cloned().unwrap_or_else(|| json!({}));
                let started = Instant::now();
                let shot = if name == LOOK { CIRCLED.lock().unwrap().take().or_else(crate::capture::screen_png) } else { None };
                let result = if name == LOOK {
                    if shot.is_some() { "Aquí está la captura de su pantalla tal como la ve ahora: mírala y respóndele.".to_string() } else { "ERROR: no pude capturar la pantalla.".to_string() }
                } else {
                    crate::agent::call_tool(app, name, &args)
                };
                used.push(format!("{name} ({} ms)", started.elapsed().as_millis()));
                if cfg!(debug_assertions) {
                    eprintln!("[vivo] {name} {args} -> {}", result.chars().take(200).collect::<String>().replace('\n', " | "));
                }
                let mut response = json!({ "id": call["id"], "name": name, "response": { "result": result } });
                if let Some(png) = &shot {
                    response["parts"] = json!([{ "inlineData": { "mimeType": "image/png", "data": crate::ai::base64(png) } }]);
                }
                responses.push(response);
            }
            live.socket.send(&json!({ "toolResponse": { "functionResponses": responses } }))?;
            owed = true;
        }
    }
}

pub fn lasso_end(cancelled: bool) {
    let turn = TURN.lock().unwrap();
    if let Some(turn) = turn.as_ref() {
        if cancelled {
            turn.cancelled.store(true, Ordering::Relaxed);
            if let Some(picture) = &turn.picture {
                let _ = picture.send(None);
            }
        }
        turn.ended.store(true, Ordering::Relaxed);
    }
    drop(turn);
    crate::mic::untap();
}

pub fn lasso_answer(png: Option<Vec<u8>>) -> Ended {
    let Some(turn) = TURN.lock().unwrap().take() else { return Ended::NotLive };
    if let Some(picture) = &turn.picture {
        let _ = picture.send(png);
    }
    wait_for(turn)
}

fn wait_for(turn: Turn) -> Ended {
    match turn.result.recv_timeout(PATIENCE + Duration::from_secs(10)) {
        Ok(Ok((said, reply))) => Ended::Spoke(said, reply),
        Ok(Err(why)) => Ended::Failed(why),
        Err(_) => {
            forget();
            Ended::Failed("Gemini se quedó callado.".into())
        }
    }
}

pub enum Ended {
    Spoke(String, String),
    Failed(String),
    NotLive,
}

pub fn finish(cancelled: bool) -> Ended {
    let Some(turn) = TURN.lock().unwrap().take() else {
        crate::mic::untap();
        return Ended::NotLive;
    };
    if cancelled {
        turn.cancelled.store(true, Ordering::Relaxed);
    }
    turn.ended.store(true, Ordering::Relaxed);
    crate::mic::untap();
    if cancelled {
        hush();
        return Ended::Failed("cancelado".into());
    }
    wait_for(turn)
}

#[cfg(debug_assertions)]
pub fn probe_barge(app: &AppHandle, wav: &std::path::Path) -> Result<(String, String), String> {
    let bytes = std::fs::read(wav).map_err(|e| e.to_string())?;
    let rate = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]);
    let samples: Vec<f32> = bytes[44..].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect();
    let live = connect(app)?;
    wake_speaker();
    let first = {
        let (app, live) = (app.clone(), live.clone());
        std::thread::spawn(move || {
            let events = live.events.lock().unwrap();
            live.socket.send(&json!({ "clientContent": { "turns": [{ "role": "user", "parts": [{ "text": "Cuéntame con calma una historia larguita, de un minuto, sobre un cocuyo que quería ser estrella." }] }], "turnComplete": true } }))?;
            answer(&app, &live, &events, Instant::now())
        })
    };
    std::thread::sleep(Duration::from_secs(7));
    let cut_at = Instant::now();
    GENERATION.fetch_add(1, Ordering::Relaxed);
    hush();
    let (feed, taps) = channel();
    std::thread::spawn(move || {
        for piece in samples.chunks(rate as usize / 50) {
            let _ = feed.send((rate, piece.to_vec()));
            std::thread::sleep(Duration::from_millis(20));
        }
    });
    let second = talk(app, &live, taps, &AtomicBool::new(false), &AtomicBool::new(false), None);
    eprintln!("[vivo] la historia quedó así: {:?}", first.join().map_err(|_| "se cayó".to_string()));
    eprintln!("[vivo] la segunda tardó {:?} desde que la interrumpí", cut_at.elapsed());
    while !QUEUE.lock().unwrap().is_empty() {
        std::thread::sleep(Duration::from_millis(100));
    }
    second
}

#[cfg(debug_assertions)]
pub fn probe_lasso(app: &AppHandle, wav: &std::path::Path) -> Result<(String, String), String> {
    let bytes = std::fs::read(wav).map_err(|e| e.to_string())?;
    let rate = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]);
    let samples: Vec<f32> = bytes[44..].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect();
    let png = match std::env::var("COCUYO_DEMO_LAZO_PNG") {
        Ok(file) => std::fs::read(file).map_err(|e| e.to_string())?,
        Err(_) => crate::capture::screen_png().ok_or("sin captura")?,
    };
    let (feed, taps) = channel();
    let (framed_send, framed) = channel();
    let _ = framed_send.send(Some(png));
    let live = connect(app)?;
    wake_speaker();
    std::thread::spawn(move || {
        for piece in samples.chunks(rate as usize / 50) {
            let _ = feed.send((rate, piece.to_vec()));
            std::thread::sleep(Duration::from_millis(20));
        }
    });
    let heard = talk(app, &live, taps, &AtomicBool::new(false), &AtomicBool::new(false), Some(framed));
    while !QUEUE.lock().unwrap().is_empty() {
        std::thread::sleep(Duration::from_millis(100));
    }
    heard
}

#[cfg(debug_assertions)]
pub fn probe_audio(app: &AppHandle, wav: &std::path::Path) -> Result<(String, String), String> {
    let bytes = std::fs::read(wav).map_err(|e| e.to_string())?;
    let rate = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]);
    let samples: Vec<f32> = bytes[44..].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect();
    let (feed, taps) = channel();
    let ended = AtomicBool::new(false);
    let started = Instant::now();
    let live = connect(app)?;
    std::thread::spawn(move || {
        for piece in samples.chunks(rate as usize / 50) {
            let _ = feed.send((rate, piece.to_vec()));
            std::thread::sleep(Duration::from_millis(20));
        }
    });
    let heard = talk(app, &live, taps, &ended, &AtomicBool::new(false), None);
    if cfg!(debug_assertions) {
        eprintln!("[vivo] desde que empezó a hablar: {:?}", started.elapsed());
    }
    while !QUEUE.lock().unwrap().is_empty() {
        std::thread::sleep(Duration::from_millis(100));
    }
    heard
}

#[cfg(debug_assertions)]
pub fn probe(app: &AppHandle, text: &str) -> Result<(String, String), String> {
    let live = connect(app)?;
    wake_speaker();
    let events = live.events.lock().unwrap();
    let started = Instant::now();
    live.socket.send(&json!({ "clientContent": { "turns": [{ "role": "user", "parts": [{ "text": text }] }], "turnComplete": true } }))?;
    let heard = answer(app, &live, &events, started);
    while !QUEUE.lock().unwrap().is_empty() {
        std::thread::sleep(Duration::from_millis(100));
    }
    heard
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resampling_keeps_the_length_across_chunks() {
        let mut down = Resampler::new(48_000.0, 16_000.0);
        let total: usize = (0..10).map(|_| down.feed(&vec![0.2; 4_800]).len()).sum();
        assert!((15_990..=16_000).contains(&total), "{total}");
        let mut up = Resampler::new(24_000.0, 48_000.0);
        let total: usize = (0..4).map(|_| up.feed(&vec![0.2; 6_000]).len()).sum();
        assert!((47_990..=48_000).contains(&total), "{total}");
    }

    #[test]
    fn an_empty_quota_is_recognized() {
        assert!(out_of_quota("Gemini Live no aceptó la conexión (429). Resource has been exhausted"));
        assert!(out_of_quota("Google cerró la conversación: You exceeded your current quota"));
        assert!(!out_of_quota("No hay conexión con Google."));
    }

    #[test]
    fn audio_goes_out_as_little_endian_pcm() {
        assert_eq!(pcm16(&[0.0, 1.0, -1.0]), vec![0, 0, 0xFF, 0x7F, 0x01, 0x80]);
    }
}
