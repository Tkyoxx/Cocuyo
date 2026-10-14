// apuntes: escucha la clase o la llamada (lo que suena en el pc y tu micro), la va escribiendo y al final te deja el resumen

use std::collections::VecDeque;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::talk::{pcm16, Resampler, Socket};

const RATE: f64 = 16_000.0;
const CHUNK: usize = 1_600;
const LONGEST: usize = 120_000;

struct Taking {
    stop: Arc<AtomicBool>,
    done: Receiver<Result<String, String>>,
    path: PathBuf,
    started: Instant,
}

static TAKING: Mutex<Option<Taking>> = Mutex::new(None);

pub fn busy() -> bool {
    TAKING.lock().unwrap().is_some()
}

fn model(app: &AppHandle) -> Result<String, String> {
    crate::ai::bidi_models(app)?
        .into_iter()
        .filter(|name| name.contains("transcribe"))
        .max_by_key(|name| name.split(|c: char| !c.is_ascii_digit()).filter_map(|n| n.parse::<u32>().ok()).take(2).collect::<Vec<_>>())
        .ok_or_else(|| "Tu clave de Gemini no trae el modelo de transcripción en vivo.".to_string())
}

fn connect(app: &AppHandle, model: &str) -> Result<(Arc<Socket>, Receiver<Value>), String> {
    let key = crate::ai::key(app).ok_or("Para tomar apuntes hace falta tu clave de Gemini (sección IA).")?;
    let socket = Arc::new(Socket::open(&key)?);
    socket.send(&json!({ "setup": {
        "model": format!("models/{model}"),
        "generationConfig": { "responseModalities": ["TEXT"], "speechConfig": { "languageCode": language() } },
        "inputAudioTranscription": {}
    } }))?;
    let first = socket.receive()?;
    if first.get("setupComplete").is_none() {
        return Err(format!("El modelo de transcripción no arrancó: {}", first.to_string().chars().take(200).collect::<String>()));
    }
    crate::ai::count(app, model);
    let (send, events) = channel();
    let reader = socket.clone();
    std::thread::spawn(move || loop {
        match reader.receive() {
            Ok(message) => {
                if send.send(message).is_err() {
                    break;
                }
            }
            Err(why) => {
                let _ = send.send(json!({ "cerrada": why }));
                break;
            }
        }
    });
    Ok((socket, events))
}

fn heard_in(message: &Value) -> String {
    let content = &message["serverContent"];
    let mut text = content["inputTranscription"]["text"].as_str().unwrap_or("").to_string();
    for part in content["modelTurn"]["parts"].as_array().into_iter().flatten() {
        if let Some(said) = part["text"].as_str() {
            text.push_str(said);
        }
    }
    text
}

fn language() -> &'static str {
    match crate::cue::language().0 {
        "en" => "en-US",
        "pt" => "pt-BR",
        _ => "es-US",
    }
}

fn clock() -> String {
    let now = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    format!("{:02}:{:02}", now.wHour, now.wMinute)
}

fn open_ear(device: &cpal::Device, config: cpal::SupportedStreamConfig, who: usize, feed: Sender<(usize, u32, Vec<f32>)>) -> Result<cpal::Stream, String> {
    let rate = config.sample_rate();
    let channels = config.channels() as usize;
    let format = config.sample_format();
    let config: cpal::StreamConfig = config.into();
    let mono = move |frames: Vec<f32>| frames.chunks(channels.max(1)).map(|f| f.iter().sum::<f32>() / f.len() as f32).collect::<Vec<f32>>();
    let stream = match format {
        cpal::SampleFormat::F32 => device.build_input_stream(config, move |data: &[f32], _: &_| {
            let _ = feed.send((who, rate, mono(data.to_vec())));
        }, |_| {}, None),
        cpal::SampleFormat::I16 => device.build_input_stream(config, move |data: &[i16], _: &_| {
            let _ = feed.send((who, rate, mono(data.iter().map(|s| *s as f32 / 32768.0).collect())));
        }, |_| {}, None),
        other => return Err(format!("formato de audio no soportado: {other:?}")),
    }
    .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(stream)
}

struct Writer {
    file: PathBuf,
    text: String,
    minute: String,
}

impl Writer {
    fn add(&mut self, piece: &str) {
        if piece.trim().is_empty() {
            return;
        }
        let now = clock();
        if now != self.minute {
            self.minute = now.clone();
            let mark = format!("{}[{now}] ", if self.text.is_empty() { "" } else { "\n\n" });
            self.text.push_str(&mark);
            self.save(&mark);
        }
        let piece = if self.text.ends_with([' ', '\n']) || piece.starts_with(' ') { piece.to_string() } else { format!(" {piece}") };
        self.text.push_str(&piece);
        self.save(&piece);
    }

    fn save(&self, piece: &str) {
        if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&self.file) {
            let _ = file.write_all(piece.as_bytes());
        }
    }
}

fn listen(app: &AppHandle, model: &str, raw: &Path, stop: &AtomicBool) -> Result<String, String> {
    let (feed, taps) = channel();
    let host = cpal::default_host();
    let mut ears = Vec::new();
    if let Some(speakers) = host.default_output_device() {
        if let Ok(config) = speakers.default_output_config() {
            if let Ok(stream) = open_ear(&speakers, config, 0, feed.clone()) {
                ears.push(stream);
            }
        }
    }
    if let Some(mic) = host.default_input_device() {
        if let Ok(config) = mic.default_input_config() {
            if let Ok(stream) = open_ear(&mic, config, 1, feed.clone()) {
                ears.push(stream);
            }
        }
    }
    drop(feed);
    if ears.is_empty() {
        return Err("No pude escuchar ni el PC ni el micrófono.".into());
    }
    let mut writer = Writer { file: raw.to_path_buf(), text: String::new(), minute: String::new() };
    let (mut socket, mut events) = connect(app, model)?;
    let mut lanes: [VecDeque<f32>; 2] = [VecDeque::new(), VecDeque::new()];
    let mut resamplers: [Option<Resampler>; 2] = [None, None];
    while !stop.load(Ordering::Relaxed) {
        match taps.recv_timeout(Duration::from_millis(100)) {
            Ok((who, rate, samples)) => {
                let resampler = resamplers[who].get_or_insert_with(|| Resampler::new(rate.max(8_000) as f64, RATE));
                lanes[who].extend(resampler.feed(&samples));
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(_) => break,
        }
        while lanes.iter().any(|lane| lane.len() >= CHUNK) {
            let mut mixed = vec![0f32; CHUNK];
            for lane in lanes.iter_mut() {
                for (at, sample) in lane.drain(..lane.len().min(CHUNK)).enumerate() {
                    mixed[at] += sample;
                }
            }
            let sent = socket.send(&json!({ "realtimeInput": { "audio": { "data": crate::ai::base64(&pcm16(&mixed)), "mimeType": "audio/pcm;rate=16000" } } }));
            if sent.is_err() {
                socket.shut();
                (socket, events) = connect(app, model)?;
            }
        }
        while let Ok(message) = events.try_recv() {
            if message.get("cerrada").is_some() || message.get("goAway").is_some() {
                socket.shut();
                (socket, events) = connect(app, model)?;
                break;
            }
            writer.add(&heard_in(&message));
        }
    }
    drop(ears);
    let _ = socket.send(&json!({ "realtimeInput": { "audioStreamEnd": true } }));
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        match events.recv_timeout(Duration::from_millis(300)) {
            Ok(message) if message.get("cerrada").is_none() => writer.add(&heard_in(&message)),
            Ok(_) => break,
            Err(_) => {}
        }
    }
    socket.shut();
    Ok(writer.text)
}

fn folder(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().document_dir().map_err(|e| e.to_string())?.join("Cocuyo").join("Apuntes");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

pub fn start(app: &AppHandle) -> Result<String, String> {
    if busy() {
        return Ok("Ya estoy tomando apuntes.".into());
    }
    if crate::ai::key(app).is_none() || !crate::settings::load(app).ai_url.contains("googleapis.com") {
        return Err("Para tomar apuntes hace falta Gemini con tu clave (sección IA).".into());
    }
    let model = model(app)?;
    let now = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    let path = folder(app)?.join(format!("{}-{:02}-{:02} {:02}.{:02}.md", now.wYear, now.wMonth, now.wDay, now.wHour, now.wMinute));
    let raw = path.with_extension("txt");
    let stop = Arc::new(AtomicBool::new(false));
    let (done, waiting) = channel();
    *TAKING.lock().unwrap() = Some(Taking { stop: stop.clone(), done: waiting, path, started: Instant::now() });
    let worker = app.clone();
    std::thread::Builder::new()
        .name("apuntes".into())
        .spawn(move || {
            let heard = listen(&worker, &model, &raw, &stop);
            if heard.is_err() {
                stop.store(true, Ordering::Relaxed);
            }
            let _ = done.send(heard);
        })
        .map_err(|e| e.to_string())?;
    let ticker = app.clone();
    std::thread::spawn(move || loop {
        let minutes = match TAKING.lock().unwrap().as_ref() {
            Some(taking) if !taking.stop.load(Ordering::Relaxed) => taking.started.elapsed().as_secs() / 60,
            _ => break,
        };
        let _ = ticker.emit_to(crate::overlay::LABEL, "script-progress", json!({ "script": "Apuntes", "valor": None::<f64>, "texto": format!("Escuchando · {minutes} min") }));
        std::thread::sleep(Duration::from_secs(20));
    });
    Ok("Tomando apuntes de lo que suena en el PC y de tu micrófono. Di «termina los apuntes» cuando acabe, y avísales a los demás que los estás grabando.".into())
}

pub fn finish(app: &AppHandle) -> Result<String, String> {
    let Some(taking) = TAKING.lock().unwrap().take() else { return Err("No estaba tomando apuntes.".into()) };
    taking.stop.store(true, Ordering::Relaxed);
    let _ = app.emit_to(crate::overlay::LABEL, "script-progress", json!({ "script": "Apuntes", "valor": None::<f64>, "texto": "Haciendo el resumen…" }));
    let heard = taking.done.recv_timeout(Duration::from_secs(20)).unwrap_or_else(|_| Err("La transcripción no respondió.".into()));
    let raw = taking.path.with_extension("txt");
    let finish_bar = || {
        let _ = app.emit_to(crate::overlay::LABEL, "script-progress", json!({ "script": "Apuntes", "valor": 1.0, "texto": "Listo" }));
    };
    let heard = match heard {
        Ok(text) => text,
        Err(why) => {
            finish_bar();
            return Err(format!("Se cortaron los apuntes: {why}"));
        }
    };
    if heard.split_whitespace().count() < 8 {
        finish_bar();
        let _ = std::fs::remove_file(&raw);
        return Ok("No alcancé a oír nada que apuntar.".into());
    }
    let tail: String = heard.chars().rev().take(LONGEST).collect::<Vec<_>>().into_iter().rev().collect();
    let summary = crate::ai::summarize_minutes(app, &tail).unwrap_or_else(|why| format!("(No pude hacer el resumen: {why})"));
    let now = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    let minutes = taking.started.elapsed().as_secs() / 60;
    let page = format!("# Apuntes del {:02}/{:02}/{} · {minutes} min\n\n{summary}\n\n---\n\n## Lo que se dijo\n\n{heard}\n", now.wDay, now.wMonth, now.wYear);
    std::fs::write(&taking.path, page).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&raw);
    finish_bar();
    let gist: String = summary.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with('#')).take(3).collect::<Vec<_>>().join(" ");
    let card = json!({
        "titulo": "Apuntes listos", "icono": "📝", "texto": gist.chars().take(260).collect::<String>(), "segundos": 60,
        "botones": [{ "texto": "Abrir", "abrir": taking.path.to_string_lossy() }, { "texto": "Carpeta", "abrir": taking.path.parent().map(|dir| dir.to_string_lossy().into_owned()).unwrap_or_default() }]
    });
    crate::scripts::show_card(app, "Apuntes", &card);
    Ok(format!("Listo, guardé {minutes} minutos de apuntes con su resumen en Documentos, Cocuyo, Apuntes."))
}

pub fn spoken(app: &AppHandle, spoken: &str) -> Option<Result<String, String>> {
    match wants(spoken)? {
        true => Some(start(app)),
        false => Some(finish(app)),
    }
}

fn wants(spoken: &str) -> Option<bool> {
    let clean = crate::intent::normalize(spoken);
    const START: [&str; 10] = [
        "toma apuntes", "empieza a tomar apuntes", "ponte a tomar apuntes", "graba la clase", "graba la reunion", "graba la llamada",
        "transcribe la clase", "transcribe la reunion", "transcribe la llamada", "take notes",
    ];
    const STOP: [&str; 10] = [
        "deja de tomar apuntes", "termina los apuntes", "termina de tomar apuntes", "para los apuntes", "deten los apuntes", "ya termino la clase",
        "ya acabo la clase", "ya termino la reunion", "ya acabo la reunion", "stop taking notes",
    ];
    if STOP.iter().any(|phrase| clean.starts_with(phrase)) {
        return Some(false);
    }
    START.iter().any(|phrase| clean.starts_with(phrase)).then_some(true)
}

#[cfg(debug_assertions)]
pub fn probe(app: &AppHandle, wav: &Path) -> Result<String, String> {
    let bytes = std::fs::read(wav).map_err(|e| e.to_string())?;
    let rate = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]);
    let samples: Vec<f32> = bytes[44..].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect();
    let model = model(app)?;
    eprintln!("[apuntes] modelo {model}");
    let (socket, events) = connect(app, &model)?;
    let mut down = Resampler::new(rate as f64, RATE);
    let audio = down.feed(&samples);
    for piece in audio.chunks(CHUNK) {
        socket.send(&json!({ "realtimeInput": { "audio": { "data": crate::ai::base64(&pcm16(piece)), "mimeType": "audio/pcm;rate=16000" } } }))?;
        std::thread::sleep(Duration::from_millis(100));
    }
    socket.send(&json!({ "realtimeInput": { "audio": { "data": crate::ai::base64(&pcm16(&vec![0.0; 16_000])), "mimeType": "audio/pcm;rate=16000" } } }))?;
    let _ = socket.send(&json!({ "realtimeInput": { "audioStreamEnd": true } }));
    let mut text = String::new();
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        if let Ok(message) = events.recv_timeout(Duration::from_millis(300)) {
            eprintln!("[apuntes] {}", message.to_string().chars().take(300).collect::<String>());
            text.push_str(&heard_in(&message));
        }
    }
    socket.shut();
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_knows_when_to_take_notes() {
        assert_eq!(wants("Toma apuntes de la clase"), Some(true));
        assert_eq!(wants("graba la reunión"), Some(true));
        assert_eq!(wants("ya terminó la clase"), Some(false));
        assert_eq!(wants("deja de tomar apuntes porfa"), Some(false));
        assert_eq!(wants("toma nota comprar pan"), None);
        assert_eq!(wants("apunta comprar pan"), None);
    }

    #[test]
    fn transcription_comes_from_either_field() {
        assert_eq!(heard_in(&json!({ "serverContent": { "inputTranscription": { "text": "hola " } } })), "hola ");
        assert_eq!(heard_in(&json!({ "serverContent": { "modelTurn": { "parts": [{ "text": "clase" }] } } })), "clase");
        assert_eq!(heard_in(&json!({ "setupComplete": {} })), "");
    }
}
