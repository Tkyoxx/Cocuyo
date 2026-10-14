// voz: grabar, transcribir y adivinar que quiso decir el usuario

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::apps::{self, CREATE_NO_WINDOW};
use crate::hotkeys::Chord;
use crate::intent::{self, Change, Intent};
use crate::media::{Played, Transport};
use crate::{capture, inject, mic, overlay};

struct Download {
    name: &'static str,
    url: &'static str,
    sha256: &'static str,
    bytes: u64,
}

const ENGINE: Download = Download {
    name: "el motor de voz",
    url: "https://github.com/ggml-org/whisper.cpp/releases/download/b5454/whisper-bin-x64.zip",
    sha256: "6ba69e3482d7826214f90a6a9c84ca07782aec1e1d0c6a7c30c994fd5d816ccb",
    bytes: 8_928_640,
};
const MODEL: Download = Download {
    name: "el modelo de voz",
    url: "https://huggingface.co/ggml-org/parakeet-GGUF/resolve/main/ggml-parakeet-tdt-0.6b-v3-q4_k.bin",
    sha256: "8b205b8b39c6535e153de6fb11c51db46125d45c4f16ba496fe41a0fe71b885e",
    bytes: 415_611_879,
};
const ATTEMPTS: u32 = 5;
const MODEL_FILE: &str = "parakeet-tdt-0.6b-v3-q4_k.bin";
const ENGINE_EXE: &str = "parakeet-cli.exe";

const SPEECH_LEVEL: f32 = 0.01;
const MIN_SPEECH_MS: usize = 240;

static INSTALLING: AtomicBool = AtomicBool::new(false);
static PRACTICE: AtomicBool = AtomicBool::new(false);

pub(crate) fn dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().app_local_data_dir().map_err(|e| e.to_string())?.join("voz"))
}

fn installed_in(dir: &Path) -> bool {
    dir.join("motor").join(ENGINE_EXE).is_file() && dir.join(MODEL_FILE).is_file()
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    installed: bool,
    installing: bool,
    megabytes: u64,
}

#[tauri::command]
pub fn voice_status(app: AppHandle) -> Status {
    Status {
        installed: dir(&app).map(|d| installed_in(&d)).unwrap_or(false),
        installing: INSTALLING.load(Ordering::Relaxed),
        megabytes: (ENGINE.bytes + MODEL.bytes) / 1_000_000,
    }
}

#[tauri::command]
pub fn set_voice_practice(on: bool) {
    PRACTICE.store(on, Ordering::Relaxed);
}

#[derive(Clone, Serialize)]
struct Progress {
    fraction: f64,
    done: bool,
    error: Option<String>,
}

pub(crate) fn sha256_of(path: &Path) -> Option<String> {
    let output = Command::new("certutil.exe")
        .args(["-hashfile", &path.display().to_string(), "SHA256"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stdout).lines().nth(1).map(|l| l.replace(' ', "").trim().to_lowercase())
}

struct Failure {
    why: String,
    fatal: bool,
    old_curl: bool,
    no_resume: bool,
}

fn failure(code: Option<i32>, detail: &str) -> Failure {
    let (why, fatal) = match code {
        Some(6) => ("no se encontró el servidor. Revisa que haya internet.", false),
        Some(7) => ("no se pudo conectar con el servidor. Revisa tu conexión, o si un cortafuegos bloquea a Cocuyo.", false),
        Some(18 | 52 | 55 | 56) => ("la conexión se cortó a mitad de camino.", false),
        Some(28) => ("la conexión se quedó sin responder.", false),
        Some(35 | 60 | 77) => (
            "Windows no pudo comprobar la conexión segura. Suele ser la fecha del equipo mal puesta, o un antivirus que revisa el tráfico.",
            false,
        ),
        Some(23) => ("no se pudo guardar en el disco. Hacen falta unos 500 MB libres.", true),
        Some(22) if detail.contains("404") || detail.contains("403") || detail.contains("410") => ("el archivo ya no está en el servidor. Hace falta una versión más nueva de Cocuyo.", true),
        Some(22) => ("el servidor respondió con un error.", false),
        _ => ("la descarga se interrumpió.", false),
    };
    let detail: String = detail.trim().trim_start_matches("curl: ").chars().take(120).collect();
    let tag = code.map_or(String::new(), |code| format!(" [{code}{}{detail}]", if detail.is_empty() { "" } else { ": " }));
    Failure { why: format!("{why}{tag}"), fatal, old_curl: code == Some(2), no_resume: matches!(code, Some(33 | 36)) }
}

fn fetch(url: &str, part: &Path, plain: bool, report: &dyn Fn(u64)) -> Result<(), Failure> {
    let mut curl = Command::new("curl.exe");
    curl.args(["--location", "--fail", "--silent", "--show-error", "--continue-at", "-"]);
    curl.args(["--connect-timeout", "20", "--speed-limit", "1024", "--speed-time", "45"]);
    if !plain {
        curl.arg("--ssl-revoke-best-effort");
    }
    let mut child = curl
        .arg("--output")
        .arg(part)
        .arg(url)
        .stderr(std::process::Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| Failure { why: format!("no se pudo iniciar curl.exe ({e})."), fatal: true, old_curl: false, no_resume: false })?;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(e) => return Err(Failure { why: e.to_string(), fatal: true, old_curl: false, no_resume: false }),
        }
        report(std::fs::metadata(part).map(|m| m.len()).unwrap_or(0));
        std::thread::sleep(Duration::from_millis(250));
    };
    if status.success() {
        return Ok(());
    }
    let mut detail = String::new();
    if let Some(mut stderr) = child.stderr.take() {
        let _ = std::io::Read::read_to_string(&mut stderr, &mut detail);
    }
    Err(failure(status.code(), &detail))
}

fn download(item: &Download, dest: &Path, report: &dyn Fn(u64)) -> Result<(), String> {
    let part = dest.with_extension("part");
    let size = || std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    let (mut stuck, mut plain, mut restarted) = (0, false, false);
    loop {
        if size() < item.bytes {
            let before = size();
            if let Err(failure) = fetch(item.url, &part, plain, report) {
                if failure.old_curl && !plain {
                    plain = true;
                    continue;
                }
                if failure.no_resume {
                    let _ = std::fs::remove_file(&part);
                }
                stuck = if size() > before { 0 } else { stuck + 1 };
                if failure.fatal || stuck >= ATTEMPTS {
                    let kept = if size() > 0 { " Pulsa Descargar otra vez: sigue desde donde quedó." } else { "" };
                    return Err(format!("No se pudo descargar {}: {}{kept}", item.name, failure.why));
                }
                std::thread::sleep(Duration::from_secs(2 * u64::from(stuck)));
                continue;
            }
        }
        if sha256_of(&part).as_deref() == Some(item.sha256) {
            return std::fs::rename(&part, dest).map_err(|e| e.to_string());
        }
        let _ = std::fs::remove_file(&part);
        if restarted {
            return Err(format!("Lo descargado de {} no coincide con el archivo esperado y se descartó. Si usas un proxy o un antivirus que revisa las descargas, puede estar cambiándolo.", item.name));
        }
        restarted = true;
    }
}

fn install(app: &AppHandle) -> Result<(), String> {
    let dir = dir(app)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let total = (ENGINE.bytes + MODEL.bytes) as f64;
    let emit = |bytes: u64| {
        let _ = app.emit_to(
            overlay::LABEL,
            "voice-install",
            Progress { fraction: (bytes as f64 / total).min(0.99), done: false, error: None },
        );
    };

    let engine_dir = dir.join("motor");
    if !engine_dir.join(ENGINE_EXE).is_file() {
        let zip = dir.join("motor.zip");
        download(&ENGINE, &zip, &emit)?;
        let unpacked = dir.join("motor-tmp");
        let _ = std::fs::remove_dir_all(&unpacked);
        let script = format!(
            "Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
            zip.display(),
            unpacked.display()
        );
        let ok = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            return Err("No se pudo descomprimir el motor de voz.".into());
        }
        std::fs::create_dir_all(&engine_dir).map_err(|e| e.to_string())?;
        let release = unpacked.join("Release");
        for entry in std::fs::read_dir(&release).map_err(|e| e.to_string())?.flatten() {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if name == ENGINE_EXE || name == "parakeet.dll" || name.starts_with("ggml") {
                std::fs::rename(entry.path(), engine_dir.join(entry.file_name())).map_err(|e| e.to_string())?;
            }
        }
        let _ = std::fs::remove_dir_all(&unpacked);
        let _ = std::fs::remove_file(&zip);
    }

    let model = dir.join(MODEL_FILE);
    if !model.is_file() {
        download(&MODEL, &model, &|bytes| emit(ENGINE.bytes + bytes))?;
    }
    Ok(())
}

#[tauri::command]
pub fn voice_install(app: AppHandle) {
    if INSTALLING.swap(true, Ordering::Relaxed) {
        return;
    }
    std::thread::spawn(move || {
        let result = install(&app);
        INSTALLING.store(false, Ordering::Relaxed);
        let _ = app.emit_to(
            overlay::LABEL,
            "voice-install",
            Progress { fraction: 1.0, done: result.is_ok(), error: result.err() },
        );
    });
}

fn to_16k(samples: &[f32], rate: u32) -> Vec<f32> {
    let step = rate as f64 / 16_000.0;
    let count = (samples.len() as f64 / step) as usize;
    (0..count)
        .map(|i| {
            let start = (i as f64 * step) as usize;
            let end = (((i + 1) as f64 * step) as usize).clamp(start + 1, samples.len());
            samples[start..end].iter().sum::<f32>() / (end - start) as f32
        })
        .collect()
}

fn trim_silence(samples: &[f32]) -> Option<&[f32]> {
    const FRAME: usize = 320; // 20 ms a 16 kHz
    let loud: Vec<bool> = samples
        .chunks(FRAME)
        .map(|f| (f.iter().map(|s| s * s).sum::<f32>() / f.len() as f32).sqrt() > SPEECH_LEVEL)
        .collect();
    if loud.iter().filter(|&&l| l).count() * 20 < MIN_SPEECH_MS {
        return None;
    }
    let first = loud.iter().position(|&l| l)?;
    let last = loud.iter().rposition(|&l| l)?;
    let start = first.saturating_sub(10) * FRAME;
    let end = ((last + 11) * FRAME).min(samples.len());
    Some(&samples[start..end])
}

fn wav_bytes(samples: &[f32]) -> Vec<u8> {
    let data = samples.len() as u32 * 2;
    let mut bytes = Vec::with_capacity(44 + data as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
    bytes.extend_from_slice(&1u16.to_le_bytes()); // un canal
    bytes.extend_from_slice(&16_000u32.to_le_bytes());
    bytes.extend_from_slice(&32_000u32.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        bytes.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    bytes
}

fn write_wav(path: &Path, samples: &[f32]) -> std::io::Result<()> {
    std::fs::write(path, wav_bytes(samples))
}

pub fn transcribe_file(app: &AppHandle, wav: &Path) -> Result<String, String> {
    let dir = dir(app)?;
    if !installed_in(&dir) {
        return Err("Falta la voz local. Descárgala en la pestaña Ajustes del menú.".into());
    }
    let threads = std::thread::available_parallelism().map_or(2, |n| n.get()).min(4);
    let output = Command::new(dir.join("motor").join(ENGINE_EXE))
        .arg("-m")
        .arg(dir.join(MODEL_FILE))
        .arg("-f")
        .arg(wav)
        .args(["-np", "-t", &threads.to_string()])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("El motor de voz no arrancó: {e}"))?;
    if !output.status.success() {
        return Err("El motor de voz falló al transcribir.".into());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(text.split_whitespace().collect::<Vec<_>>().join(" "))
}

static GUIDE: Mutex<Option<(&'static str, Vec<f32>)>> = Mutex::new(None);

fn guide(app: &AppHandle) -> Option<(&'static str, Vec<f32>)> {
    let (language, phrase) = crate::cue::language();
    let mut cached = GUIDE.lock().unwrap();
    if cached.as_ref().is_none_or(|(of, _)| *of != language) {
        let file = dir(app).ok()?.join(format!("guia-{language}.wav"));
        if !file.is_file() {
            std::fs::create_dir_all(file.parent()?).ok()?;
            crate::cue::render(phrase, &file).ok()?;
        }
        let bytes = std::fs::read(&file).ok()?;
        let data = bytes.windows(4).position(|tag| tag == b"data")? + 8;
        let samples: Vec<f32> = bytes.get(data..)?.chunks_exact(2).map(|pair| i16::from_le_bytes([pair[0], pair[1]]) as f32 / 32768.0).collect();
        *cached = Some((language, trim_silence(&samples)?.to_vec()));
    }
    let (_, samples) = cached.as_ref()?;
    Some((phrase, samples.clone()))
}

fn without_guide(text: &str, phrase: &str) -> String {
    let guide = intent::normalize(phrase);
    let mut expected = guide.split(' ').peekable();
    let mut words = text.split_whitespace().peekable();
    while let (Some(word), Some(wanted)) = (words.peek(), expected.peek()) {
        let heard = intent::normalize(word);
        if heard != *wanted && !intent::close(&heard, wanted) {
            break;
        }
        words.next();
        expected.next();
    }
    words.collect::<Vec<_>>().join(" ")
}

fn transcribe(app: &AppHandle, recording: mic::Recording) -> Result<Option<String>, String> {
    let audio = to_16k(&recording.samples, recording.rate);
    let Some(speech) = trim_silence(&audio) else { return Ok(None) };
    let wav = dir(app)?.join("frase.wav");
    if let Some(parent) = wav.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let guide = guide(app);
    match &guide {
        Some((_, lead)) => {
            let mut all = Vec::with_capacity(lead.len() + 3_200 + speech.len());
            all.extend_from_slice(lead);
            all.resize(lead.len() + 3_200, 0.0);
            all.extend_from_slice(speech);
            write_wav(&wav, &all)
        }
        None => write_wav(&wav, speech),
    }
    .map_err(|e| e.to_string())?;
    let text = transcribe_file(app, &wav);
    let _ = std::fs::remove_file(&wav);
    let text = match &guide {
        Some((phrase, _)) => text.map(|t| without_guide(&t, phrase)),
        None => text,
    };
    text.map(|t| (!t.is_empty()).then_some(t))
}

#[cfg(debug_assertions)]
pub fn transcribe_clip(app: &AppHandle, wav: &Path) -> Result<Option<String>, String> {
    let bytes = std::fs::read(wav).map_err(|e| e.to_string())?;
    let rate = u32::from_le_bytes(bytes.get(24..28).ok_or("archivo corto")?.try_into().unwrap());
    let data = bytes.windows(4).position(|tag| tag == b"data").ok_or("sin datos")? + 8;
    let samples = bytes[data..].chunks_exact(2).map(|pair| i16::from_le_bytes([pair[0], pair[1]]) as f32 / 32768.0).collect();
    transcribe(app, mic::Recording { samples, rate })
}

fn url_encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => (b as char).to_string(),
            b' ' => "+".into(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn first_things(app: &AppHandle, spoken: &str) -> Option<Result<String, String>> {
    if let Some(done) = crate::minutes::spoken(app, spoken) {
        return Some(done);
    }
    if let Some(done) = crate::ocr::spoken(spoken).or_else(|| crate::system::slow_spoken(spoken)).or_else(|| edit_selection(app, spoken)) {
        return Some(done);
    }
    if let Some(done) = crate::system::mic_spoken(spoken) {
        return Some(done);
    }
    if let Some(done) = apps::close_spoken(spoken) {
        return Some(done);
    }
    if let Some(done) = crate::weather::spoken(app, spoken) {
        if let Ok(said) = &done {
            crate::cue::answer(&crate::i18n::tr(said));
        }
        return Some(done);
    }
    if let Some(done) = crate::media::spoken(app, spoken) {
        return Some(done);
    }
    if let Some(done) = remember(app, spoken) {
        return Some(done);
    }
    if let Some(done) = crate::agenda::manage(app, spoken) {
        return Some(done);
    }
    if crate::notes::note_text(spoken).is_none() {
        if let Some(planned) = crate::agenda::schedule(app, spoken) {
            return Some(planned);
        }
    }
    if let Some(done) = crate::scripts::by_name(app, spoken) {
        return Some(done);
    }
    None
}

pub fn local_order(app: &AppHandle, spoken: &str) -> Result<String, String> {
    if spoken.trim().is_empty() {
        return Err("No me dijiste qué hacer.".into());
    }
    first_things(app, spoken).unwrap_or_else(|| act(app, spoken))
}

fn agent(app: &AppHandle, task: &str) -> Result<String, String> {
    let (answer, aloud) = crate::agent::run(app, task)?;
    if aloud {
        crate::cue::answer(&answer);
    }
    Ok(answer)
}

fn quick(app: &AppHandle, spoken: &str) -> Option<Result<String, String>> {
    let config = crate::settings::load(app);
    if !config.ai_agent || !config.agent_hands {
        return None;
    }
    let (chat, who, text) = crate::agent::quick_message(spoken)?;
    Some(match crate::agent::send_message(app, &chat, &who, &text) {
        Err(why) if crate::agent::wanted(app) && !crate::pc::let_go() => {
            if cfg!(debug_assertions) {
                eprintln!("[rápido] no salió ({why}); lo intenta el agente");
            }
            agent(app, spoken)
        }
        done => done,
    })
}

fn task_for(app: &AppHandle, spoken: &str) -> Option<String> {
    if !crate::agent::wanted(app) {
        return None;
    }
    crate::agent::called(spoken).or_else(|| (crate::agent::asked() || crate::agent::needs_steps(spoken)).then(|| spoken.to_string()))
}

pub fn run_order(app: &AppHandle, spoken: &str) -> Result<String, String> {
    if let Some(done) = crate::scripts::answered(app, spoken) {
        return done;
    }
    if let Some(done) = quick(app, spoken) {
        return done;
    }
    if let Some(task) = task_for(app, spoken) {
        return agent(app, &task);
    }
    if let Some(done) = crate::extensions::spoken(app, spoken) {
        return done;
    }
    if crate::ai::asking() && crate::ai::mode(app).is_some() {
        if let Ok(order) = crate::ai::rewrite(app, spoken, &crate::scripts::list(app)) {
            return follow(app, spoken, &order);
        }
    }
    if let Some(done) = first_things(app, spoken) {
        return done;
    }
    match crate::ai::mode(app) {
        Some(always) if (always && has_names(app, spoken)) || !understood(app, spoken) => {
            match crate::ai::rewrite(app, spoken, &crate::scripts::list(app)) {
                Ok(order) => follow(app, spoken, &order),
                Err(_) if understood(app, spoken) => act(app, spoken),
                Err(why) => Err(format!("No entendí «{spoken}», y la IA no ayudó: {why}")),
            }
        }
        _ => act(app, spoken),
    }
}

fn edit_selection(app: &AppHandle, spoken: &str) -> Option<Result<String, String>> {
    let clean = intent::normalize(spoken);
    const ASKS: [&str; 28] = [
        "corrige esto", "corrigelo", "corrige el texto", "corrigeme esto", "arregla esto", "arreglalo", "arregla el texto", "reescribe esto", "reescribelo",
        "hazlo mas ", "ponlo mas ", "resume esto", "resumelo", "traduce esto", "traducelo", "mejora esto", "mejoralo", "acorta esto", "acortalo",
        "fix this", "rewrite this", "make it ", "translate this", "summarize this", "corrija isso", "traduza isso", "resuma isso", "melhore isso",
    ];
    if !ASKS.iter().any(|ask| clean.starts_with(ask)) {
        return None;
    }
    if crate::ai::mode(app).is_none() {
        return Some(Err("Para cambiar un texto hace falta un modelo de IA: conéctalo en Ajustes, sección IA.".into()));
    }
    let (selected, before) = inject::copy_selection();
    let restore = |before: Option<String>| {
        crate::clip::ignore_own_changes();
        inject::set_clipboard_text(before.as_deref().unwrap_or(""));
    };
    let Some(selected) = selected else {
        restore(before);
        return Some(Err("Selecciona primero el texto que quieres cambiar.".into()));
    };
    let done = crate::ai::edit(app, spoken, &selected).and_then(|changed| inject::paste(&changed)).map(|()| "Hecho.".to_string());
    restore(before);
    Some(done)
}

fn remember(app: &AppHandle, spoken: &str) -> Option<Result<String, String>> {
    let clean = intent::normalize(spoken);
    const KEEP: [&str; 9] = ["recuerda que ", "acuerdate de que ", "acuerdate que ", "memoriza que ", "ten en cuenta que ", "apunta en tu memoria que ", "remember that ", "lembra que ", "lembre que "];
    if let Some(lead) = KEEP.iter().find(|lead| clean.starts_with(**lead)).filter(|_| !crate::agenda::soon(spoken)) {
        let fact = spoken.split_whitespace().skip(lead.split_whitespace().count()).collect::<Vec<_>>().join(" ");
        if fact.is_empty() {
            return None;
        }
        return Some(crate::ai::memorize(app, &fact).map(|()| format!("Lo recordaré: {fact}")));
    }
    const WIPE: [&str; 6] = ["olvida todo", "olvidalo todo", "borra tu memoria", "borra la memoria", "forget everything", "esquece tudo"];
    if WIPE.iter().any(|lead| clean.starts_with(lead)) {
        return Some(crate::ai::forget(app, None).map(|_| "Memoria borrada.".to_string()));
    }
    const DROP: [&str; 7] = ["olvida lo de ", "olvida que ", "olvidate de ", "olvida ", "forget about ", "forget that ", "esquece "];
    let lead = DROP.iter().find(|lead| clean.starts_with(**lead))?;
    let about = &clean[lead.len()..];
    Some(crate::ai::forget(app, Some(about)).map(|gone| if gone > 0 { "Olvidado.".to_string() } else { format!("No tenía nada guardado sobre «{about}».") }))
}

fn has_names(app: &AppHandle, spoken: &str) -> bool {
    match intent::parse(spoken) {
        Intent::Spotify(_) | Intent::Mood(_) | Intent::Playlist(_) | Intent::Unknown => true,
        Intent::OpenApp(name) => apps::find(app, &name, 0.9).is_none(),
        _ => false,
    }
}

fn understood(app: &AppHandle, spoken: &str) -> bool {
    crate::notes::note_text(spoken).is_some()
        || !matches!(intent::parse(spoken), Intent::Unknown)
        || (spoken.split_whitespace().count() <= 4 && apps::find(app, spoken, 0.9).is_some())
}

fn follow(app: &AppHandle, spoken: &str, order: &str) -> Result<String, String> {
    let order = order.trim();
    let lower = order.to_lowercase();
    if let Some(task) = lower.strip_prefix("agente").filter(|rest| rest.is_empty() || rest.starts_with([' ', ':'])) {
        let task = order[order.len() - task.len()..].trim_start_matches([' ', ':']).trim();
        let task = if task.is_empty() { spoken } else { task };
        if crate::agent::wanted(app) {
            return agent(app, task);
        }
        if !understood(app, spoken) {
            return Err("Eso tiene varios pasos: enciende el modo agente en Ajustes, sección Agente, y lo hago yo.".into());
        }
    }
    if (lower.is_empty() || lower == "nada") && crate::agent::wanted(app) && !understood(app, spoken) {
        return agent(app, spoken);
    }
    if let Some(rest) = lower.strip_prefix("script ") {
        return crate::scripts::named(app, rest, spoken);
    }
    if let Some(done) = remember(app, order) {
        return done;
    }
    if let Some(planned) = crate::agenda::schedule(app, order) {
        return planned;
    }
    if lower.starts_with("responde ") {
        let answer = order["responde ".len()..].trim().trim_start_matches(':').trim().to_string();
        crate::cue::answer(&answer);
        return Ok(answer);
    }
    if !lower.is_empty() && lower != "nada" && understood(app, order) {
        return act(app, order);
    }
    if understood(app, spoken) {
        return act(app, spoken);
    }
    Err(format!("No entendí qué hacer con «{spoken}». Prueba con «pon…», «abre…» o «busca…»."))
}

fn act(app: &AppHandle, spoken: &str) -> Result<String, String> {
    if let Some(text) = crate::notes::note_text(spoken) {
        return crate::notes::add_note(app, &text);
    }
    match intent::parse(spoken) {
        Intent::OpenApp(name) => open_apps(app, &name),
        Intent::OpenUrl(url) => apps::open(&url).map(|()| "Abriendo la página.".into()),
        Intent::YouTube(q) => apps::open(&format!("https://www.youtube.com/results?search_query={}", url_encode(&q)))
            .map(|()| format!("Buscando «{q}» en YouTube.")),
        Intent::Spotify(q) => match crate::media::play(app, &q) {
            Played::NoSession => apps::open(&format!("spotify:search:{}", url_encode(&q).replace('+', "%20")))
                .map(|()| format!("Buscando «{q}» en Spotify. Conecta Spotify en Ajustes para que la ponga a sonar directamente.")),
            played => sounding(played),
        },
        Intent::Mood(q) => match crate::media::play_mood(app, &q) {
            Played::NoSession => apps::open(&format!("spotify:search:{}", url_encode(&q).replace('+', "%20")))
                .map(|()| format!("Buscando «{q}» en Spotify. Conecta Spotify en Ajustes para que lo ponga a sonar directamente.")),
            played => sounding(played),
        },
        Intent::Playlist(name) => match crate::media::play_list(app, &name) {
            Played::NoSession => Err("Para poner tus listas, conecta Spotify en Ajustes.".into()),
            played => sounding(played),
        },
        Intent::Restart => {
            if crate::media::restart(app) {
                Ok("Otra vez, desde el principio.".into())
            } else {
                Err("No hay nada sonando que pueda repetir.".into())
            }
        }
        Intent::Google(q) => apps::open(&format!("https://www.google.com/search?q={}", url_encode(&q)))
            .map(|()| format!("Buscando «{q}» en Google.")),
        Intent::Volume { change, system } => set_volume(app, change, system),
        Intent::Pause => Ok(transport(Transport::Pause, 0xB3, "En pausa.")),
        Intent::Resume => Ok(transport(Transport::Resume, 0xB3, "Sonando.")),
        Intent::Next => Ok(transport(Transport::Next, 0xB0, "Siguiente.")),
        Intent::Previous => Ok(transport(Transport::Previous, 0xB1, "Anterior.")),
        Intent::Random => match crate::media::play_random(app) {
            Played::List(list) => Ok(format!("Algo al azar de tu lista «{list}».")),
            Played::NoSession => {
                if crate::media::shuffle_and_skip() {
                    Ok("Puse el modo aleatorio y salté a otra canción.".into())
                } else {
                    Err("No hay nada sonando que pueda mezclar. Conecta Spotify en Ajustes para elegir de tus listas.".into())
                }
            }
            played => sounding(played),
        },
        Intent::Screenshot => {
            let app = app.clone();
            std::thread::spawn(move || capture::hotkey(&app, 3));
            Ok(String::new())
        }
        Intent::Lock => crate::system::lock().map(|()| "Equipo bloqueado.".into()),
        Intent::Settings => {
            let _ = app.emit_to(overlay::LABEL, "settings", ());
            Ok(String::new())
        }
        Intent::Unknown => match apps::find(app, spoken, 0.9).filter(|_| spoken.split_whitespace().count() <= 4) {
            Some(found) => apps::launch(&found).map(|()| format!("Abriendo {}.", found.name)),
            None => Err(format!("No entendí qué hacer con «{spoken}». Prueba con «pon…», «abre…» o «busca…».")),
        },
    }
}

fn sounding(played: Played) -> Result<String, String> {
    match played {
        Played::Track(title, artist) => Ok(format!("Sonando «{title}» de {artist}.")),
        Played::Artist(name) => Ok(format!("Sonando {name}.")),
        Played::List(name) => Ok(format!("Sonando tu lista «{name}».")),
        Played::Mix(name) => Ok(format!("Sonando la lista «{name}».")),
        Played::Failed(why) => Err(why),
        Played::NoSession => Err("Conecta Spotify en Ajustes.".into()),
    }
}

fn open_apps(app: &AppHandle, name: &str) -> Result<String, String> {
    let found: Vec<_> = match apps::find(app, name, 0.72) {
        Some(found) => vec![found],
        None => name.split(" y ").filter_map(|part| apps::find(app, part, 0.72)).collect(),
    };
    if found.is_empty() {
        return Err(format!("No encontré ninguna app llamada «{name}». Puedes añadirla en la pestaña Apps."));
    }
    let mut names = Vec::new();
    for found in &found {
        apps::launch(found)?;
        names.push(found.name.as_str());
    }
    Ok(format!("Abriendo {}.", names.join(" y ")))
}

/// El volumen va a Spotify. Al del equipo solo si la orden lo nombra ("de la PC")
fn set_volume(app: &AppHandle, change: Change, system: bool) -> Result<String, String> {
    let said = |place: &str, percent: u8| match percent {
        0 => format!("{place} en silencio."),
        percent => format!("{place} al {percent} %."),
    };
    if !system {
        if let Some(percent) = crate::media::volume(app, change) {
            return Ok(said("Spotify", percent));
        }
    }
    let percent = crate::system::change_volume(change).ok_or("Windows no dejó cambiar el volumen.")?;
    Ok(if system { said("Volumen del equipo", percent) } else { format!("Spotify no está sonando. {}", said("Volumen del equipo", percent)) })
}

fn transport(action: Transport, fallback_key: u16, done: &str) -> String {
    if !crate::media::control(action) {
        inject::tap(fallback_key);
    }
    done.into()
}

#[derive(Clone, Serialize)]
struct Heard {
    chord: Chord,
    text: String,
    reply: String,
    error: Option<String>,
    live: bool,
}

static LASSO_LIVE: AtomicBool = AtomicBool::new(false);
static LASSO_HEARD: std::sync::Mutex<Option<mic::Recording>> = std::sync::Mutex::new(None);

pub fn lasso_listen(app: &AppHandle) -> bool {
    *QUESTION.lock().unwrap() = None;
    if !PRACTICE.load(Ordering::Relaxed) && crate::talk::wanted(app) {
        mic::start();
        LASSO_LIVE.store(crate::talk::begin_lasso(app), Ordering::Relaxed);
        if LASSO_LIVE.load(Ordering::Relaxed) {
            return true;
        }
        let _ = mic::stop();
    }
    let ready = !PRACTICE.load(Ordering::Relaxed) && dir(app).map(|d| installed_in(&d)).unwrap_or(false);
    if ready {
        mic::start();
    }
    LASSO_LISTENING.store(ready, Ordering::Relaxed);
    *QUESTION.lock().unwrap() = None;
    ready
}

pub fn lasso_heard(app: &AppHandle, cancelled: bool) -> bool {
    if LASSO_LIVE.load(Ordering::Relaxed) {
        let recording = mic::stop();
        crate::talk::lasso_end(cancelled);
        if cancelled {
            LASSO_LIVE.store(false, Ordering::Relaxed);
            return false;
        }
        *LASSO_HEARD.lock().unwrap() = recording.ok();
        return true;
    }
    if !LASSO_LISTENING.swap(false, Ordering::Relaxed) {
        return false;
    }
    let recording = mic::stop();
    if cancelled {
        return false;
    }
    let app = app.clone();
    *QUESTION.lock().unwrap() = Some(std::thread::spawn(move || recording.ok().and_then(|r| transcribe(&app, r).ok().flatten())));
    true
}

pub fn question_pending() -> bool {
    QUESTION.lock().unwrap().is_some() || LASSO_LIVE.load(Ordering::Relaxed)
}

static LASSO_LISTENING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static QUESTION: std::sync::Mutex<Option<std::thread::JoinHandle<Option<String>>>> = std::sync::Mutex::new(None);

#[derive(serde::Serialize)]
pub struct Asked {
    question: String,
    answer: String,
    live: bool,
}

#[tauri::command]
pub async fn lasso_ask(app: AppHandle) -> Result<Option<Asked>, String> {
    if LASSO_LIVE.swap(false, Ordering::Relaxed) {
        let image = crate::capture::asked_about();
        let recording = LASSO_HEARD.lock().unwrap().take();
        match crate::talk::lasso_answer(image.clone()) {
            crate::talk::Ended::Spoke(question, answer) => {
                crate::ai::talked(&question, &answer);
                return Ok(Some(Asked { question, answer, live: true }));
            }
            _ => {
                let question = recording.and_then(|r| transcribe(&app, r).ok().flatten()).ok_or("No te escuché.")?;
                let image = image.ok_or("No quedó imagen de lo que rodeaste.")?;
                let answer = crate::ai::see(&app, &question, &image).map_err(|why| crate::i18n::tr(&why))?;
                crate::ai::talked(&question, &answer);
                return Ok(Some(Asked { question, answer: crate::i18n::tr(&answer), live: false }));
            }
        }
    }
    let Some(listening) = QUESTION.lock().unwrap().take() else { return Ok(None) };
    let image = crate::capture::asked_about();
    let Some(question) = listening.join().ok().flatten() else { return Ok(None) };
    if crate::ocr::wants(&question) {
        let copied = image.as_deref().ok_or("No quedó imagen de lo que rodeaste.".to_string()).and_then(crate::ocr::copy_from_png);
        return copied.map(|answer| Some(Asked { question, answer: crate::i18n::tr(&answer), live: false })).map_err(|why| crate::i18n::tr(&why));
    }
    if crate::ai::mode(&app).is_none() {
        return Ok(None);
    }
    let answer = match image {
        Some(image) => crate::ai::see(&app, &question, &image),
        None => Err("No quedó imagen de lo que rodeaste.".to_string()),
    };
    match answer {
        Ok(answer) => {
            crate::ai::talked(&question, &answer);
            crate::cue::play(crate::cue::Cue::Done);
            crate::cue::answer(&answer);
            Ok(Some(Asked { question, answer, live: false }))
        }
        Err(why) => {
            crate::cue::play(crate::cue::Cue::Fail);
            Err(crate::i18n::tr(&why))
        }
    }
}

fn hear_with_ai(app: &AppHandle, recording: &mic::Recording, dictation: bool) -> Option<(String, String)> {
    if !crate::settings::load(app).ai_hear || crate::ai::mode(app).is_none() {
        return None;
    }
    let audio = to_16k(&recording.samples, recording.rate);
    let speech = trim_silence(&audio)?;
    let heard = crate::ai::hear(app, &wav_bytes(speech), dictation, &crate::scripts::list(app));
    if cfg!(debug_assertions) {
        eprintln!("[oído] {heard:?}");
    }
    heard.ok()
}

pub fn finish(app: &AppHandle, chord: Chord, cancelled: bool) {
    let recording = mic::stop();
    crate::ai::maybe_distill(app);
    if chord == Chord::Command {
        match crate::talk::finish(cancelled || PRACTICE.load(Ordering::Relaxed)) {
            crate::talk::Ended::Spoke(said, reply) => {
                crate::ai::talked(&said, &reply);
                let error = (said.is_empty() && reply.is_empty()).then(|| crate::i18n::tr("No te escuché. Mantén las teclas mientras hablas."));
                match &error {
                    Some(error) => {
                        crate::cue::play(crate::cue::Cue::Fail);
                        crate::gamelayer::finished(&said, error, true, false);
                    }
                    None => crate::gamelayer::finished(&said, &reply, false, false),
                }
                let _ = app.emit_to(overlay::LABEL, "heard", Heard { chord, text: said, reply, error, live: true });
                return;
            }
            crate::talk::Ended::Failed(why) if cfg!(debug_assertions) => eprintln!("[vivo] no salió en vivo: {why}"),
            _ => {}
        }
    }
    if cancelled || PRACTICE.load(Ordering::Relaxed) {
        crate::gamelayer::cancel();
        return;
    }
    let mut heard = Heard { chord, text: String::new(), reply: String::new(), error: None, live: false };
    let dictation = chord == Chord::Dictation;
    let direct = recording.as_ref().ok().and_then(|recording| hear_with_ai(app, recording, dictation));
    let was_direct = direct.is_some();
    let understood = match direct {
        Some((said, result)) if dictation => Ok(Some((result, None, said.is_empty()))),
        Some((said, order)) => Ok(Some((said, Some(order), false))),
        None => recording.and_then(|r| transcribe(app, r)).map(|text| text.map(|text| (text, None, false))),
    };
    match understood {
        Ok(Some((text, order, _))) => {
            let polishing = dictation && !was_direct && text.split_whitespace().count() > 3 && crate::settings::load(app).ai_dictation && crate::ai::mode(app).is_some();
            let text = if polishing { crate::ai::polish(app, &text).unwrap_or(text) } else { text };
            let into = crate::settings::load(app).dictation_into;
            let text = match into.as_str() {
                "en" | "es" | "pt" if dictation && crate::ai::mode(app).is_some() => {
                    let language = match into.as_str() {
                        "en" => "inglés",
                        "pt" => "portugués",
                        _ => "español",
                    };
                    crate::ai::edit(app, &format!("Tradúcelo al {language}. Si ya está en {language}, déjalo igual."), &text).unwrap_or(text)
                }
                _ => text,
            };
            let outcome = if dictation {
                inject::paste(&text).map(|()| String::new())
            } else {
                let outcome = match &order {
                    Some(order) => match crate::scripts::answered(app, &text).or_else(|| quick(app, &text)).or_else(|| crate::extensions::spoken(app, &text)) {
                        Some(done) => done,
                        None if task_for(app, &text).is_some() => agent(app, &task_for(app, &text).unwrap_or_else(|| text.clone())),
                        None if crate::ai::asking() => follow(app, &text, order),
                        None => first_things(app, &text).unwrap_or_else(|| follow(app, &text, order)),
                    },
                    None => run_order(app, &text),
                };
                match &outcome {
                    Ok(reply) => crate::ai::talked(&text, reply),
                    Err(why) => crate::ai::talked(&text, &format!("(no pude: {why})")),
                }
                outcome
            };
            heard.text = text;
            match outcome {
                Ok(reply) => heard.reply = reply,
                Err(error) => heard.error = Some(error),
            }
        }
        Ok(None) => heard.error = Some("No te escuché. Mantén las teclas mientras hablas.".into()),
        Err(error) => heard.error = Some(error),
    }
    heard.reply = crate::i18n::tr(&heard.reply);
    heard.error = heard.error.map(|error| crate::i18n::tr(&error));
    match &heard.error {
        Some(error) => {
            crate::cue::play(crate::cue::Cue::Fail);
            crate::cue::speak(error);
            crate::gamelayer::finished(&heard.text, error, true, chord == Chord::Dictation);
        }
        None => {
            crate::cue::play(crate::cue::Cue::Done);
            crate::cue::speak(&heard.reply);
            crate::gamelayer::finished(&heard.text, &heard.reply, false, chord == Chord::Dictation);
        }
    }
    let _ = app.emit_to(overlay::LABEL, "heard", heard);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_never_reaches_the_model() {
        assert!(trim_silence(&vec![0.0; 16_000]).is_none());
        let mut click = vec![0.0; 16_000];
        click[8_000..8_960].fill(0.5);
        assert!(trim_silence(&click).is_none());
    }

    #[test]
    fn speech_is_kept_with_a_margin() {
        let mut audio = vec![0.0; 48_000];
        for (i, s) in audio[16_000..32_000].iter_mut().enumerate() {
            *s = (i as f32 * 0.3).sin() * 0.3;
        }
        let kept = trim_silence(&audio).expect("hay voz");
        assert!(kept.len() >= 16_000 && kept.len() <= 16_000 + 2 * 3_520 + 640);
    }

    #[test]
    fn resampling_keeps_the_duration() {
        assert_eq!(to_16k(&vec![0.1; 48_000], 48_000).len(), 16_000);
        assert_eq!(to_16k(&vec![0.1; 44_100], 44_100).len(), 16_000);
    }

    #[test]
    fn queries_are_safe_in_urls() {
        assert_eq!(url_encode("bad bunny"), "bad+bunny");
        assert_eq!(url_encode("año & más"), "a%C3%B1o+%26+m%C3%A1s");
    }

    #[test]
    fn the_guide_phrase_is_removed() {
        let guide = "Hola, buenas tardes.";
        assert_eq!(without_guide("Hola, buenas tardes. Pon Blinding Lights de The Weeknd.", guide), "Pon Blinding Lights de The Weeknd.");
        assert_eq!(without_guide("hola buenas tardes súbele un poquito", guide), "súbele un poquito");
        assert_eq!(without_guide("Ola, buenas. Abre Spotify.", guide), "Abre Spotify.");
        assert_eq!(without_guide("Abre Spotify.", guide), "Abre Spotify.");
        assert_eq!(without_guide("Hola, buenas tardes. Hola, pon música.", guide), "Hola, pon música.");
    }

    #[test]
    fn failures_say_what_to_do() {
        let cut = failure(Some(56), "curl: (56) Recv failure: Connection was reset");
        assert!(cut.why.contains("se cortó") && cut.why.contains("[56: (56) Recv failure") && !cut.fatal);
        assert!(failure(Some(23), "").fatal, "sin disco no tiene sentido reintentar");
        assert!(failure(Some(22), "curl: (22) The requested URL returned error: 404").fatal);
        assert!(!failure(Some(22), "curl: (22) The requested URL returned error: 503").fatal);
        assert!(failure(Some(2), "curl: option --ssl-revoke-best-effort: is unknown").old_curl);
        assert!(failure(Some(33), "").no_resume);
    }

    #[test]
    #[ignore]
    fn descarga_continua_y_se_repone() {
        let dir = std::env::temp_dir().join("cocuyo-prueba-descarga");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (dest, part) = (dir.join("motor.zip"), dir.join("motor.part"));

        download(&ENGINE, &dest, &|_| {}).unwrap();
        assert_eq!(std::fs::metadata(&dest).unwrap().len(), ENGINE.bytes);

        let whole = std::fs::read(&dest).unwrap();
        std::fs::remove_file(&dest).unwrap();
        std::fs::write(&part, &whole[..whole.len() / 2]).unwrap();
        let first = std::cell::Cell::new(0);
        download(&ENGINE, &dest, &|bytes| {
            if first.get() == 0 {
                first.set(bytes);
            }
        })
        .unwrap();
        assert!(first.get() >= whole.len() as u64 / 2, "no empezó de cero: {}", first.get());
        assert_eq!(std::fs::read(&dest).unwrap(), whole);

        std::fs::remove_file(&dest).unwrap();
        std::fs::write(&part, vec![7u8; whole.len()]).unwrap();
        download(&ENGINE, &dest, &|_| {}).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), whole);

        let nowhere = Download { name: "la prueba", url: "https://no-existe.invalid/x.bin", sha256: "", bytes: 10 };
        let error = download(&nowhere, &dir.join("x.bin"), &|_| {}).unwrap_err();
        assert!(error.contains("No se pudo descargar la prueba") && error.contains("servidor"), "{error}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
