// voces que suenan a gente. kokoro suena brutal pero en un i3 tarda una eternidad, avisados quedan

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const SOURCE: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/";

struct Pack {
    path: &'static str,
    bytes: u64,
    sha256: &'static str,
}

impl Pack {
    fn file(&self) -> &'static str {
        self.path.rsplit('/').next().unwrap_or(self.path)
    }

    fn folder(&self) -> &'static str {
        self.file().trim_end_matches(".tar.bz2")
    }
}

const ENGINE: Pack = Pack {
    path: "v1.13.8/sherpa-onnx-v1.13.8-win-x64-shared-MT-Release.tar.bz2",
    bytes: 24_805_859,
    sha256: "6dffdc715a4465b989446a6105265d2cb345e7101591a17d35534b6758f6e8df",
};
const KOKORO: Pack = Pack {
    path: "tts-models/kokoro-int8-multi-lang-v1_0.tar.bz2",
    bytes: 132_303_094,
    sha256: "4c3052abaa60943a341f193888cf6abd68787dae6ab8ae5c925a706caa247e4e",
};
const DAVEFX: Pack = Pack {
    path: "tts-models/vits-piper-es_ES-davefx-medium.tar.bz2",
    bytes: 67_184_952,
    sha256: "a3f6beb54a9cb893279f72978a22f807a4d9fc9c7848157b524d5cc7b7f58b22",
};
const CLAUDE: Pack = Pack {
    path: "tts-models/vits-piper-es_MX-claude-high.tar.bz2",
    bytes: 67_207_890,
    sha256: "ec33fb689c248fe64810aab564cba97babf0f506672cfd404928d46e751a4721",
};
const ALD: Pack = Pack {
    path: "tts-models/vits-piper-es_MX-ald-medium.tar.bz2",
    bytes: 67_196_497,
    sha256: "c60cf7bc853eb01a875fba0756523fecf9d9f46c763b19785d223fc981fb4fb6",
};
const DANIELA: Pack = Pack {
    path: "tts-models/vits-piper-es_AR-daniela-high.tar.bz2",
    bytes: 115_562_134,
    sha256: "71cbf6b7f646ab74f3c51336151abef41e6c54467ac929ffdb19ed706a07dd7b",
};

struct Voice {
    id: &'static str,
    label: &'static str,
    pack: &'static Pack,
    speaker: Option<u8>,
}

const VOICES: [Voice; 7] = [
    Voice { id: "piper-davefx", label: "Davefx · España", pack: &DAVEFX, speaker: None },
    Voice { id: "piper-claude", label: "Claude · México", pack: &CLAUDE, speaker: None },
    Voice { id: "piper-ald", label: "Ald · México", pack: &ALD, speaker: None },
    Voice { id: "piper-daniela", label: "Daniela · Argentina", pack: &DANIELA, speaker: None },
    Voice { id: "kokoro-dora", label: "Dora · Kokoro", pack: &KOKORO, speaker: Some(28) },
    Voice { id: "kokoro-alex", label: "Alex · Kokoro", pack: &KOKORO, speaker: Some(29) },
    Voice { id: "kokoro-santa", label: "Santa · Kokoro", pack: &KOKORO, speaker: Some(30) },
];

static INSTALLING: Mutex<Option<&'static str>> = Mutex::new(None);

fn home(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(crate::voice::dir(app)?.join("natural"))
}

fn engine(home: &Path) -> PathBuf {
    home.join(ENGINE.folder()).join("bin").join("sherpa-onnx-offline-tts.exe")
}

fn model(home: &Path, voice: &Voice) -> PathBuf {
    let folder = home.join(voice.pack.folder());
    match voice.speaker {
        Some(_) => folder.join("model.int8.onnx"),
        None => folder.join(format!("{}.onnx", voice.pack.folder().trim_start_matches("vits-piper-"))),
    }
}

fn ready(home: &Path, voice: &Voice) -> bool {
    engine(home).is_file() && model(home, voice).is_file()
}

#[derive(Serialize)]
pub struct Listed {
    id: &'static str,
    label: &'static str,
    megabytes: u64,
    installed: bool,
    installing: bool,
    slow: bool,
}

#[tauri::command]
pub fn tts_voices(app: AppHandle) -> Vec<Listed> {
    let home = home(&app).unwrap_or_default();
    let installing = *INSTALLING.lock().unwrap();
    VOICES
        .iter()
        .map(|voice| Listed {
            id: voice.id,
            label: voice.label,
            megabytes: voice.pack.bytes / 1_000_000,
            installed: ready(&home, voice),
            installing: installing == Some(voice.id),
            slow: voice.speaker.is_some(),
        })
        .collect()
}

fn fetch(home: &Path, pack: &Pack) -> Result<(), String> {
    let archive = home.join(pack.file());
    let done = Command::new("curl.exe")
        .args(["--location", "--silent", "--show-error", "--fail", "--retry", "3", "--continue-at", "-", "--connect-timeout", "20", "--output"])
        .arg(&archive)
        .arg(format!("{SOURCE}{}", pack.path))
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| e.to_string())?;
    let size = std::fs::metadata(&archive).map(|m| m.len()).unwrap_or(0);
    if size != pack.bytes || crate::voice::sha256_of(&archive).as_deref() != Some(pack.sha256) {
        let _ = std::fs::remove_file(&archive);
        let why = String::from_utf8_lossy(&done.stderr).trim().to_string();
        return Err(if why.is_empty() { "Lo descargado no coincide con el archivo esperado y se descartó.".into() } else { format!("La descarga falló: {why}") });
    }
    let unpacked = std::fs::File::open(&archive)
        .map_err(|e| e.to_string())
        .and_then(|file| tar::Archive::new(bzip2_rs::DecoderReader::new(std::io::BufReader::new(file))).unpack(home).map_err(|e| e.to_string()));
    let _ = std::fs::remove_file(&archive);
    unpacked.map_err(|why| format!("No se pudo descomprimir la voz: {why}"))
}

fn install(app: &AppHandle, voice: &Voice) -> Result<(), String> {
    let home = home(app)?;
    std::fs::create_dir_all(&home).map_err(|e| e.to_string())?;
    if !engine(&home).is_file() {
        fetch(&home, &ENGINE)?;
        let bin = home.join(ENGINE.folder()).join("bin");
        for entry in std::fs::read_dir(&bin).into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if name != "sherpa-onnx-offline-tts.exe" && !name.starts_with("onnxruntime") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
        for extra in ["lib", "include"] {
            let _ = std::fs::remove_dir_all(home.join(ENGINE.folder()).join(extra));
        }
    }
    if !model(&home, voice).is_file() {
        fetch(&home, voice.pack)?;
    }
    if ready(&home, voice) {
        Ok(())
    } else {
        Err("La voz se descargó, pero le falta algún archivo.".into())
    }
}

#[derive(Clone, Serialize)]
struct Progress {
    id: &'static str,
    error: Option<String>,
}

#[tauri::command]
pub fn tts_install(app: AppHandle, id: String) -> Result<(), String> {
    let voice = VOICES.iter().find(|voice| voice.id == id).ok_or("Esa voz no está en la lista.")?;
    {
        let mut installing = INSTALLING.lock().unwrap();
        if installing.is_some() {
            return Err("Ya se está descargando una voz. Espera a que termine.".into());
        }
        *installing = Some(voice.id);
    }
    std::thread::spawn(move || {
        let error = install(&app, voice).err();
        *INSTALLING.lock().unwrap() = None;
        let _ = app.emit_to(crate::overlay::LABEL, "tts-install", Progress { id: voice.id, error });
    });
    Ok(())
}

#[cfg(debug_assertions)]
pub fn try_out(app: &AppHandle, id: &str, text: &str) -> String {
    let Some(voice) = VOICES.iter().find(|voice| voice.id == id) else { return "no existe".into() };
    let started = std::time::Instant::now();
    if let Err(why) = install(app, voice) {
        return format!("no se instaló: {why}");
    }
    let installed = started.elapsed();
    let first = std::time::Instant::now();
    let said = wav(app, text, id);
    let first = first.elapsed();
    let again = std::time::Instant::now();
    let _ = wav(app, text, id);
    format!("instalada en {installed:.1?}; primera vez {first:.2?} -> {:?}; segunda {:.1?}", said.map(|file| std::fs::metadata(file).map(|m| m.len()).unwrap_or(0)), again.elapsed())
}

#[cfg_attr(test, allow(dead_code))]
pub fn wav(app: &AppHandle, text: &str, id: &str) -> Result<PathBuf, String> {
    use std::hash::{Hash, Hasher};
    let voice = VOICES.iter().find(|voice| voice.id == id).ok_or("Esa voz no está en la lista.")?;
    let said = crate::voice::dir(app)?.join("dichos");
    std::fs::create_dir_all(&said).map_err(|e| e.to_string())?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    ("local", id, text.trim().to_lowercase()).hash(&mut hasher);
    let file = said.join(format!("{:016x}.wav", hasher.finish()));
    if file.is_file() {
        return Ok(file);
    }
    let home = home(app)?;
    if !ready(&home, voice) {
        return Err("Esa voz no está descargada.".into());
    }
    let folder = home.join(voice.pack.folder());
    let at = |name: &str| folder.join(name).display().to_string();
    let mut command = Command::new(engine(&home));
    match voice.speaker {
        Some(speaker) => command.args([
            format!("--kokoro-model={}", at("model.int8.onnx")),
            format!("--kokoro-voices={}", at("voices.bin")),
            format!("--kokoro-tokens={}", at("tokens.txt")),
            format!("--kokoro-data-dir={}", at("espeak-ng-data")),
            format!("--kokoro-lexicon={}", at("lexicon-us-en.txt")),
            "--kokoro-lang=es".to_string(),
            format!("--sid={speaker}"),
        ]),
        None => command.args([
            format!("--vits-model={}", model(&home, voice).display()),
            format!("--vits-tokens={}", at("tokens.txt")),
            format!("--vits-data-dir={}", at("espeak-ng-data")),
        ]),
    };
    let threads = std::thread::available_parallelism().map_or(2, |n| n.get()).min(4);
    let draft = file.with_extension("tmp.wav");
    let done = command
        .arg(format!("--num-threads={threads}"))
        .arg(format!("--output-filename={}", draft.display()))
        .arg(text)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("El motor de la voz no arrancó: {e}"))?;
    if !done.status.success() || !draft.is_file() {
        let _ = std::fs::remove_file(&draft);
        return Err("El motor de la voz falló al hablar.".into());
    }
    std::fs::rename(&draft, &file).map_err(|e| e.to_string())?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_voice_knows_where_its_files_go() {
        let home = Path::new("C:\\x");
        assert!(engine(home).ends_with("sherpa-onnx-v1.13.8-win-x64-shared-MT-Release\\bin\\sherpa-onnx-offline-tts.exe"));
        assert!(model(home, &VOICES[1]).ends_with("vits-piper-es_MX-claude-high\\es_MX-claude-high.onnx"));
        assert!(model(home, &VOICES[4]).ends_with("kokoro-int8-multi-lang-v1_0\\model.int8.onnx"));
        assert!(VOICES.iter().all(|voice| voice.pack.sha256.len() == 64 && voice.pack.file().ends_with(".tar.bz2")));
        let mut ids: Vec<_> = VOICES.iter().map(|voice| voice.id).collect();
        ids.dedup();
        assert_eq!(ids.len(), VOICES.len());
    }
}

fn said_file(app: &AppHandle, voice: &str, text: &str) -> Result<PathBuf, String> {
    use std::hash::{Hash, Hasher};
    let said = crate::voice::dir(app)?.join("dichos");
    std::fs::create_dir_all(&said).map_err(|e| e.to_string())?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    ("windows", voice, text.trim().to_lowercase()).hash(&mut hasher);
    Ok(said.join(format!("{:016x}.wav", hasher.finish())))
}

#[derive(Serialize)]
pub struct SystemVoice {
    name: String,
    language: String,
}

#[tauri::command]
pub async fn system_voices() -> Vec<SystemVoice> {
    use windows::Media::SpeechSynthesis::SpeechSynthesizer;
    let Ok(voices) = SpeechSynthesizer::AllVoices() else { return Vec::new() };
    voices
        .into_iter()
        .filter_map(|voice| Some(SystemVoice { name: voice.DisplayName().ok()?.to_string().replace("Microsoft ", ""), language: voice.Language().ok()?.to_string() }))
        .collect()
}

fn modern(name: &str, text: &str, pitch: f64, rate: f64) -> Result<Vec<u8>, String> {
    use windows::Media::SpeechSynthesis::SpeechSynthesizer;
    use windows::Storage::Streams::DataReader;
    let fail = |e: windows::core::Error| e.to_string();
    let voice = SpeechSynthesizer::AllVoices()
        .map_err(fail)?
        .into_iter()
        .find(|voice| voice.DisplayName().is_ok_and(|shown| shown.to_string().to_lowercase().contains(&name.to_lowercase())))
        .ok_or("Windows no tiene instalada esa voz.")?;
    let speaker = SpeechSynthesizer::new().map_err(fail)?;
    speaker.SetVoice(&voice).map_err(fail)?;
    if let Ok(options) = speaker.Options() {
        let _ = options.SetAudioPitch(pitch);
        let _ = options.SetSpeakingRate(rate);
    }
    let stream = speaker.SynthesizeTextToStreamAsync(&windows::core::HSTRING::from(text)).map_err(fail)?.join().map_err(fail)?;
    let size = stream.Size().map_err(fail)? as u32;
    let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0).map_err(fail)?).map_err(fail)?;
    reader.LoadAsync(size).map_err(fail)?.join().map_err(fail)?;
    let mut bytes = vec![0u8; size as usize];
    reader.ReadBytes(&mut bytes).map_err(fail)?;
    Ok(bytes)
}

pub fn system_wav(app: &AppHandle, text: &str, name: &str) -> Result<PathBuf, String> {
    let file = said_file(app, name, text)?;
    if !file.is_file() {
        std::fs::write(&file, modern(name, text, 1.0, 1.0)?).map_err(|e| e.to_string())?;
    }
    Ok(file)
}

static NO_LOQUENDO: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn loquendero(app: &AppHandle, text: &str) -> Result<PathBuf, String> {
    let file = said_file(app, "loquendero", text)?;
    if file.is_file() {
        return Ok(file);
    }
    if !NO_LOQUENDO.load(std::sync::atomic::Ordering::Relaxed) {
        const SCRIPT: &str = "Add-Type -AssemblyName System.Speech; $v = New-Object System.Speech.Synthesis.SpeechSynthesizer; \
            $n = $v.GetInstalledVoices() | ForEach-Object { $_.VoiceInfo.Name } | Where-Object { $_ -match 'Loquendo|Jorge' } | Select-Object -First 1; \
            if (-not $n) { exit 3 }; $v.SelectVoice($n); $v.SetOutputToWaveFile($env:COCUYO_SALIDA); $v.Speak($env:COCUYO_TEXTO); $v.Dispose()";
        let windows = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
        for folder in ["SysWOW64", "System32"] {
            let shell = Path::new(&windows).join(folder).join("WindowsPowerShell\\v1.0\\powershell.exe");
            let done = Command::new(shell)
                .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
                .env("COCUYO_TEXTO", text)
                .env("COCUYO_SALIDA", &file)
                .creation_flags(CREATE_NO_WINDOW)
                .status();
            if done.is_ok_and(|status| status.success()) && file.is_file() {
                return Ok(file);
            }
        }
        NO_LOQUENDO.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    let bytes = modern("Pablo", text, 0.82, 1.08).or_else(|_| modern("Helena", text, 0.8, 1.08))?;
    std::fs::write(&file, bytes).map_err(|e| e.to_string())?;
    Ok(file)
}
