// hablar en vivo con gemini: te va escuchando mientras hablas y contesta con voz mientras piensa. como hitodama, pero gratis

use std::collections::VecDeque;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde_json::{json, Value};
use tauri::AppHandle;
use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Networking::WinHttp::*;

const HOST: &str = "generativelanguage.googleapis.com";
const PATH: &str = "/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent";
const IN_RATE: u32 = 16_000;
const OUT_RATE: f64 = 24_000.0;
const CHUNK: usize = 1_600;
const PATIENCE: Duration = Duration::from_secs(90);
const STALE_AFTER: Duration = Duration::from_secs(8 * 60);

struct Socket {
    session: *mut c_void,
    connect: *mut c_void,
    socket: *mut c_void,
    sending: Mutex<()>,
}

unsafe impl Send for Socket {}
unsafe impl Sync for Socket {}

impl Socket {
    fn open(key: &str) -> Result<Socket, String> {
        unsafe {
            let session = WinHttpOpen(w!("Cocuyo"), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, PCWSTR::null(), PCWSTR::null(), 0);
            if session.is_null() {
                return Err("No pude abrir la conexión.".into());
            }
            let connect = WinHttpConnect(session, &HSTRING::from(HOST), 443, 0);
            let request = if connect.is_null() { std::ptr::null_mut() } else { WinHttpOpenRequest(connect, w!("GET"), &HSTRING::from(PATH), PCWSTR::null(), PCWSTR::null(), std::ptr::null(), WINHTTP_FLAG_SECURE) };
            let fail = |why: String| {
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
                return fail("No llegué a Google.".into());
            }
            let _ = WinHttpSetOption(Some(request), WINHTTP_OPTION_UPGRADE_TO_WEB_SOCKET, None);
            let headers: Vec<u16> = format!("x-goog-api-key: {key}\r\n").encode_utf16().collect();
            if WinHttpSendRequest(request, Some(&headers), None, 0, 0, 0).is_err() || WinHttpReceiveResponse(request, std::ptr::null_mut()).is_err() {
                return fail("No hay conexión con Google.".into());
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
                let said = serde_json::from_slice::<Value>(&body).ok().and_then(|v| v["error"]["message"].as_str().map(str::to_string)).unwrap_or_default();
                return fail(format!("Gemini Live no aceptó la conexión ({status}). {said}").trim().to_string());
            }
            let socket = WinHttpWebSocketCompleteUpgrade(request, None);
            let _ = WinHttpCloseHandle(request);
            if socket.is_null() {
                let _ = WinHttpCloseHandle(connect);
                let _ = WinHttpCloseHandle(session);
                return Err("Google no completó la conexión en vivo.".into());
            }
            Ok(Socket { session, connect, socket, sending: Mutex::new(()) })
        }
    }

    fn send(&self, message: &Value) -> Result<(), String> {
        let text = message.to_string();
        let _turn = self.sending.lock().unwrap();
        let code = unsafe { WinHttpWebSocketSend(self.socket, WINHTTP_WEB_SOCKET_UTF8_MESSAGE_BUFFER_TYPE, Some(text.as_bytes())) };
        if code == 0 { Ok(()) } else { Err(format!("se cortó la conexión ({code})")) }
    }

    fn receive(&self) -> Result<Value, String> {
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
                let reason = String::from_utf8_lossy(&chunk[..read as usize]).to_string();
                return Err(if reason.trim().is_empty() { "Google cerró la conversación".into() } else { format!("Google cerró la conversación: {}", reason.trim()) });
            }
            whole.extend_from_slice(&chunk[..read as usize]);
            if kind == WINHTTP_WEB_SOCKET_UTF8_MESSAGE_BUFFER_TYPE || kind == WINHTTP_WEB_SOCKET_BINARY_MESSAGE_BUFFER_TYPE {
                return serde_json::from_slice(&whole).map_err(|e| e.to_string());
            }
        }
    }
}

impl Socket {
    fn shut(&self) {
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

struct Resampler {
    step: f64,
    pos: f64,
    pending: Vec<f32>,
}

impl Resampler {
    fn new(from: f64, to: f64) -> Self {
        Resampler { step: from / to, pos: 0.0, pending: Vec::new() }
    }

    fn feed(&mut self, samples: &[f32]) -> Vec<f32> {
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

fn pcm16(samples: &[f32]) -> Vec<u8> {
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
static ANSWERING: AtomicBool = AtomicBool::new(false);
static RESTING: Mutex<Option<Instant>> = Mutex::new(None);
const REST: Duration = Duration::from_secs(60 * 60);

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
    let tools = crate::agent::declarations(app);
    let mut setup = json!({ "setup": {
        "model": format!("models/{model}"),
        "generationConfig": { "responseModalities": ["AUDIO"], "speechConfig": { "voiceConfig": { "prebuiltVoiceConfig": { "voiceName": voice } } } },
        "systemInstruction": { "parts": [{ "text": crate::agent::voice_prompt(app) }] },
        "realtimeInputConfig": { "automaticActivityDetection": { "disabled": true } },
        "inputAudioTranscription": {},
        "outputAudioTranscription": {}
    } });
    if !tools.is_empty() {
        setup["setup"]["tools"] = json!([{ "functionDeclarations": tools }]);
    }
    socket.send(&setup)?;
    let first = socket.receive()?;
    if first.get("setupComplete").is_none() {
        return Err(format!("Gemini Live no arrancó: {}", first.to_string().chars().take(300).collect::<String>()));
    }
    crate::ai::count(app, &model);
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
    result: Receiver<Result<(String, String), String>>,
}

static TURN: Mutex<Option<Turn>> = Mutex::new(None);

pub fn begin(app: &AppHandle) {
    if !wanted(app) {
        return;
    }
    hush();
    if ANSWERING.load(Ordering::Relaxed) {
        forget();
    }
    let taps = crate::mic::tap();
    let ended = Arc::new(AtomicBool::new(false));
    let cancelled = Arc::new(AtomicBool::new(false));
    let (done, result) = channel();
    *TURN.lock().unwrap() = Some(Turn { ended: ended.clone(), cancelled: cancelled.clone(), result });
    let app = app.clone();
    std::thread::spawn(move || {
        let outcome = connect(&app).and_then(|live| {
            let outcome = talk(&app, &live, taps, &ended, &cancelled);
            if outcome.is_err() {
                forget_this(&live);
            }
            outcome
        });
        let _ = done.send(outcome);
    });
}

fn talk(app: &AppHandle, live: &Arc<Live>, taps: Receiver<(u32, Vec<f32>)>, ended: &AtomicBool, cancelled: &AtomicBool) -> Result<(String, String), String> {
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
    let events = live.events.lock().unwrap();
    while events.try_recv().is_ok() {}
    if !ready.is_empty() {
        live.socket.send(&json!({ "realtimeInput": { "audio": { "data": crate::ai::base64(&pcm16(&ready)), "mimeType": "audio/pcm;rate=16000" } } }))?;
    }
    live.socket.send(&json!({ "realtimeInput": { "activityEnd": {} } }))?;
    answer(app, live, &events, Instant::now())
}

fn answer(app: &AppHandle, live: &Live, events: &Receiver<Value>, asked: Instant) -> Result<(String, String), String> {
    ANSWERING.store(true, Ordering::Relaxed);
    crate::cue::hold_voice(true);
    let heard = listen(app, live, events, asked);
    crate::cue::hold_voice(false);
    ANSWERING.store(false, Ordering::Relaxed);
    heard
}

fn listen(app: &AppHandle, live: &Live, events: &Receiver<Value>, asked: Instant) -> Result<(String, String), String> {
    let mut said = String::new();
    let mut reply = String::new();
    let mut to_speaker = None;
    let mut first_sound = None;
    let mut owed = false;
    loop {
        let message = events.recv_timeout(PATIENCE).map_err(|_| "Gemini se quedó callado.".to_string())?;
        if let Some(why) = message.get("cerrada").and_then(Value::as_str) {
            return Err(why.to_string());
        }
        if let Some(content) = message.get("serverContent") {
            if let Some(text) = content["inputTranscription"]["text"].as_str() {
                said.push_str(text);
            }
            if let Some(text) = content["outputTranscription"]["text"].as_str() {
                reply.push_str(text);
            }
            for part in content["modelTurn"]["parts"].as_array().into_iter().flatten() {
                owed = false;
                if let Some(data) = part["inlineData"]["data"].as_str() {
                    first_sound.get_or_insert_with(|| asked.elapsed());
                    play(&crate::ai::unbase64(data), &mut to_speaker);
                }
            }
            if content["interrupted"].as_bool() == Some(true) {
                hush();
            }
            if content["turnComplete"].as_bool() == Some(true) && !owed {
                if cfg!(debug_assertions) {
                    eprintln!("[vivo] primera voz a los {first_sound:?}, todo en {:?}", asked.elapsed());
                }
                return Ok((said.trim().to_string(), reply.trim().to_string()));
            }
        }
        if let Some(calls) = message["toolCall"]["functionCalls"].as_array() {
            let mut responses = Vec::new();
            for call in calls {
                let name = call["name"].as_str().unwrap_or("");
                let args = call.get("args").cloned().unwrap_or_else(|| json!({}));
                let result = crate::agent::call_tool(app, name, &args);
                if cfg!(debug_assertions) {
                    eprintln!("[vivo] {name} {args} -> {}", result.chars().take(200).collect::<String>().replace('\n', " | "));
                }
                responses.push(json!({ "id": call["id"], "name": name, "response": { "result": result } }));
            }
            live.socket.send(&json!({ "toolResponse": { "functionResponses": responses } }))?;
            owed = true;
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
    match turn.result.recv_timeout(PATIENCE + Duration::from_secs(10)) {
        Ok(Ok((said, reply))) => Ended::Spoke(said, reply),
        Ok(Err(why)) => Ended::Failed(why),
        Err(_) => {
            forget();
            Ended::Failed("Gemini se quedó callado.".into())
        }
    }
}

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
    let heard = talk(app, &live, taps, &ended, &AtomicBool::new(false));
    if cfg!(debug_assertions) {
        eprintln!("[vivo] desde que empezó a hablar: {:?}", started.elapsed());
    }
    while !QUEUE.lock().unwrap().is_empty() {
        std::thread::sleep(Duration::from_millis(100));
    }
    heard
}

pub fn probe(app: &AppHandle, text: &str) -> Result<(String, String), String> {
    let started = Instant::now();
    let live = connect(app)?;
    let events = live.events.lock().unwrap();
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
