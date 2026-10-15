// los tonos y la voz de windows, para cuando hay un juego delante y no se ve nada

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::OnceLock;

use windows::core::{GUID, HSTRING, PCWSTR};
use windows::Win32::Globalization::GetUserDefaultUILanguage;
use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT, WAVEFORMATEX};
use windows::Win32::Media::Speech::{
    IEnumSpObjectTokens, ISpObjectToken, ISpObjectTokenCategory, ISpStream, ISpVoice, SpFileStream, SpObjectTokenCategory, SpVoice,
    SPCAT_VOICES, SPFM_CREATE_ALWAYS,
};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};

#[allow(non_upper_case_globals)]
const SPDFID_WaveFormatEx: GUID = GUID::from_u128(0xC31ADBAE_527F_4ff5_A230_F62BB61FF70C);

static NATURAL: std::sync::Mutex<Option<(tauri::AppHandle, String)>> = std::sync::Mutex::new(None);

pub fn attach(app: tauri::AppHandle, voice: &str) {
    *NATURAL.lock().unwrap() = Some((app, voice.trim().to_string()));
}

#[cfg(test)]
fn say_natural(_text: &str) -> bool {
    false
}

#[cfg(not(test))]
fn natural_wav(text: &str) -> Option<Result<std::path::PathBuf, String>> {
    let (app, voice) = NATURAL.lock().unwrap().clone().filter(|(_, voice)| !voice.is_empty())?;
    Some(if voice == "loquendero" {
        crate::tts::loquendero(&app, text)
    } else if let Some(id) = voice.strip_prefix("local:") {
        crate::tts::wav(&app, text, id)
    } else if let Some(name) = voice.strip_prefix("windows:") {
        crate::tts::system_wav(&app, text, name)
    } else {
        crate::ai::voice_wav(&app, text, &voice)
    })
}

#[cfg(test)]
pub fn wav_of(_text: &str) -> Result<Vec<u8>, String> {
    Err("sin voz en las pruebas".into())
}

#[cfg(not(test))]
pub fn wav_of(text: &str) -> Result<Vec<u8>, String> {
    let text = crate::i18n::tr(text).replace(['«', '»'], "");
    if let Some(Ok(file)) = natural_wav(&text) {
        return std::fs::read(file).map_err(|e| e.to_string());
    }
    static TURN: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let file = std::env::temp_dir().join(format!("cocuyo-celular-{}.wav", TURN.fetch_add(1, Ordering::Relaxed) % 4));
    render(&text, &file)?;
    let wav = std::fs::read(&file).map_err(|e| e.to_string());
    let _ = std::fs::remove_file(&file);
    wav
}

#[cfg(not(test))]
fn say_live(text: &str) -> Option<bool> {
    let (app, voice) = NATURAL.lock().unwrap().clone().filter(|(_, voice)| !voice.is_empty() && !voice.contains(':') && voice != "loquendero")?;
    if crate::ai::voice_file(&app, text, &voice).is_ok_and(|file| file.is_file()) || crate::ai::gemini_voice(&app).is_err() {
        return None;
    }
    Some(match crate::talk::speak(&app, text, &voice) {
        Ok(pcm) => {
            let _ = crate::ai::keep_voice(&app, text, &voice, &pcm);
            true
        }
        Err(why) => {
            if cfg!(debug_assertions) {
                eprintln!("[voz] en vivo: {why}");
            }
            false
        }
    })
}

#[cfg(not(test))]
fn say_natural(text: &str) -> bool {
    if let Some(said) = say_live(text) {
        return said;
    }
    let Some(spoken) = natural_wav(text) else { return false };
    match spoken {
        Ok(file) => unsafe {
            std::thread::sleep(std::time::Duration::from_millis(350));
            PlaySoundW(&HSTRING::from(file.as_os_str()), None, windows::Win32::Media::Audio::SND_FILENAME | SND_NODEFAULT).as_bool()
        },
        Err(why) => {
            if cfg!(debug_assertions) {
                eprintln!("[voz] {why}");
            }
            false
        }
    }
}

static CUES: AtomicBool = AtomicBool::new(true);
static SPEAK: AtomicBool = AtomicBool::new(true);
static LANGUAGE: AtomicU8 = AtomicU8::new(0);

const LANGUAGES: [(&str, &[&str], &str); 3] = [
    ("es", &["C0A", "80A", "40A", "540A", "2C0A", "240A", "200A"], "Hola, buenas tardes."),
    ("en", &["409", "809", "C09", "1009", "4009"], "Hello, good afternoon."),
    ("pt", &["416", "816"], "Olá, boa tarde."),
];

pub fn configure(cues: bool, speak: bool, language: &str) {
    CUES.store(cues, Ordering::Relaxed);
    SPEAK.store(speak, Ordering::Relaxed);
    LANGUAGE.store(LANGUAGES.iter().position(|(code, _, _)| *code == language).unwrap_or(0) as u8, Ordering::Relaxed);
}

pub fn language() -> (&'static str, &'static str) {
    let (code, _, guide) = LANGUAGES[LANGUAGE.load(Ordering::Relaxed) as usize % LANGUAGES.len()];
    (code, guide)
}

fn installer_language() -> Option<u16> {
    let mut text = [0u16; 16];
    let mut size = std::mem::size_of_val(&text) as u32;
    let read = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            windows::core::w!("Software\\github\\Cocuyo"),
            windows::core::w!("Installer Language"),
            RRF_RT_REG_SZ,
            None,
            Some(text.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if read.is_err() {
        return None;
    }
    String::from_utf16_lossy(&text).trim_end_matches('\0').trim().parse().ok()
}

pub fn system_language() -> &'static str {
    match installer_language().unwrap_or_else(|| unsafe { GetUserDefaultUILanguage() }) & 0x3FF {
        0x09 => "en",
        0x16 => "pt",
        _ => "es",
    }
}

unsafe fn voice_token() -> Result<ISpObjectToken, String> {
    let category: ISpObjectTokenCategory = CoCreateInstance(&SpObjectTokenCategory, None, CLSCTX_ALL).map_err(|e| format!("categoría: {e}"))?;
    category.SetId(SPCAT_VOICES, false).map_err(|e| format!("voces: {e}"))?;
    let (_, variants, _) = LANGUAGES[LANGUAGE.load(Ordering::Relaxed) as usize % LANGUAGES.len()];
    variants
        .iter()
        .find_map(|variant| {
            let wanted = HSTRING::from(format!("Language={variant}"));
            let voices: IEnumSpObjectTokens = category.EnumTokens(&wanted, PCWSTR::null()).ok()?;
            voices.Item(0).ok()
        })
        .ok_or_else(|| "Windows no tiene instalada ninguna voz en ese idioma".to_string())
}

pub fn render(text: &str, path: &std::path::Path) -> Result<(), String> {
    let text: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    let format = WAVEFORMATEX {
        wFormatTag: 1, // PCM
        nChannels: 1,
        nSamplesPerSec: 16_000,
        nAvgBytesPerSec: 32_000,
        nBlockAlign: 2,
        wBitsPerSample: 16,
        cbSize: 0,
    };
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let token = voice_token()?;
        let voice: ISpVoice = CoCreateInstance(&SpVoice, None, CLSCTX_ALL).map_err(|e| format!("voz: {e}"))?;
        voice.SetVoice(&token).map_err(|e| format!("elegir voz: {e}"))?;
        let file: ISpStream = CoCreateInstance(&SpFileStream, None, CLSCTX_ALL).map_err(|e| format!("archivo: {e}"))?;
        file.BindToFile(&HSTRING::from(path.as_os_str()), SPFM_CREATE_ALWAYS, Some(&SPDFID_WaveFormatEx), Some(&format), 0)
            .map_err(|e| format!("crear archivo: {e}"))?;
        voice.SetOutput(&file, true).map_err(|e| format!("salida: {e}"))?;
        let spoken = voice.Speak(PCWSTR(text.as_ptr()), 0x10, None).map_err(|e| format!("hablar: {e}"));
        let _ = file.Close();
        spoken
    }
}

#[derive(Clone, Copy)]
pub enum Cue {
    Listen,
    Done,
    Fail,
    Recording,
    Stopped,
    Kept,
}

const RATE: u32 = 22_050;

fn wav(notes: &[(f32, u32)]) -> Vec<u8> {
    let mut samples: Vec<i16> = Vec::new();
    for (hz, ms) in notes {
        let count = RATE * ms / 1000;
        for i in 0..count {
            let t = i as f32 / RATE as f32;
            let edge = (i.min(count - i) as f32 / (RATE as f32 * 0.012)).min(1.0);
            samples.push(((t * hz * std::f32::consts::TAU).sin() * edge * 0.22 * i16::MAX as f32) as i16);
        }
    }
    let data = samples.len() as u32 * 2;
    let mut out = Vec::with_capacity(44 + data as usize);
    out.extend(b"RIFF");
    out.extend((36 + data).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());
    out.extend(1u16.to_le_bytes()); // PCM
    out.extend(1u16.to_le_bytes()); // mono
    out.extend(RATE.to_le_bytes());
    out.extend((RATE * 2).to_le_bytes());
    out.extend(2u16.to_le_bytes());
    out.extend(16u16.to_le_bytes());
    out.extend(b"data");
    out.extend(data.to_le_bytes());
    out.extend(samples.iter().flat_map(|s| s.to_le_bytes()));
    out
}

pub fn ring(cue: Cue) {
    if matches!(cue, Cue::Recording | Cue::Kept) {
        QUIET_UNTIL.fetch_max(now_ms() + 1150, Ordering::Relaxed);
    }
    sound(cue);
}

pub fn play(cue: Cue) {
    if !CUES.load(Ordering::Relaxed) || !crate::placement::fullscreen() {
        return;
    }
    sound(cue);
}

static HELD: AtomicBool = AtomicBool::new(false);

pub fn hold_voice(on: bool) {
    HELD.store(on, Ordering::Relaxed);
}

pub fn answer(text: &str) {
    if text.trim().is_empty() || HELD.load(Ordering::Relaxed) {
        return;
    }
    ANSWERED.store(true, Ordering::Relaxed);
    let text = text.to_string();
    std::thread::spawn(move || {
        let _ = say(&text);
    });
}

static ANSWERED: AtomicBool = AtomicBool::new(false);

pub fn speak(text: &str) {
    if ANSWERED.swap(false, Ordering::Relaxed) || HELD.load(Ordering::Relaxed) {
        return;
    }
    if !SPEAK.load(Ordering::Relaxed) || !crate::placement::fullscreen() || text.trim().is_empty() {
        return;
    }
    let text = text.to_string();
    std::thread::spawn(move || {
        if let Err(why) = say(&text) {
            if cfg!(debug_assertions) {
                eprintln!("[voz] {why}");
            }
        }
    });
}

pub fn alert(text: &str) {
    sound(Cue::Done);
    let text = text.to_string();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(450));
        let _ = say(&text);
    });
}

fn say(text: &str) -> Result<(), String> {
    let text = &crate::i18n::tr(text);
    if say_natural(&text.replace(['«', '»'], "")) {
        return Ok(());
    }
    let text: Vec<u16> = text.replace(['«', '»'], "").encode_utf16().chain(Some(0)).collect();
    let step = |name: &str| {
        let name = name.to_string();
        move |e: windows::core::Error| format!("{name}: {e}")
    };
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let token = voice_token()?;
        let voice: ISpVoice = CoCreateInstance(&SpVoice, None, CLSCTX_ALL).map_err(step("voz"))?;
        voice.SetVoice(&token).map_err(step("elegir voz"))?;
        let _ = voice.SetRate(1);
        voice.Speak(PCWSTR(text.as_ptr()), 0x10, None).map_err(step("hablar"))
    }
}

fn sound(cue: Cue) {
    static SOUNDS: OnceLock<[Vec<u8>; 6]> = OnceLock::new();
    let sounds = SOUNDS.get_or_init(|| {
        [
            wav(&[(660.0, 70), (880.0, 90)]),   // sube: te escucho
            wav(&[(880.0, 70), (1320.0, 120)]), // sube más: hecho
            wav(&[(440.0, 90), (330.0, 150)]),  // baja: no pude
            chime(&[(0, 329.6, 0.9), (90, 493.9, 0.8), (180, 659.3, 0.75), (270, 987.8, 0.5)], 900), // campanitas que suben: grabando
            chime(&[(0, 784.0, 0.6), (100, 587.3, 0.75), (200, 392.0, 0.85), (310, 261.6, 1.0)], 1100), // bajan y se posan: guardado
            chime(&[(0, 880.0, 0.7), (0, 1318.5, 0.3), (120, 1046.5, 0.6)], 650),                   // destello: guarde lo ultimo
        ]
    });
    let tone = &sounds[cue as usize];
    unsafe {
        let _ = PlaySoundW(PCWSTR(tone.as_ptr().cast()), None, SND_MEMORY | SND_ASYNC | SND_NODEFAULT);
    }
}

fn chime(notes: &[(u32, f32, f32)], length_ms: u32) -> Vec<u8> {
    let count = (RATE * length_ms / 1000) as usize;
    let mut mix = vec![0f32; count];
    let partials = [(1.0, 1.0, 1.0), (2.0, 0.18, 1.8), (3.01, 0.05, 2.6)];
    for (start, hz, gain) in notes {
        let first = (RATE * start / 1000) as usize;
        for i in first..count {
            let t = (i - first) as f32 / RATE as f32;
            let attack = (t / 0.004).min(1.0);
            let mut sample = 0.0;
            for (ratio, level, fade) in partials {
                let pitch = hz * ratio;
                if pitch < RATE as f32 / 2.0 {
                    sample += (t * pitch * std::f32::consts::TAU).sin() * level * (-t * 5.0 * fade).exp();
                }
            }
            mix[i] += sample * attack * gain;
        }
    }
    let echo = (RATE * 110 / 1000) as usize;
    for i in echo..count {
        mix[i] += mix[i - echo] * 0.22;
    }
    let tail = (RATE / 25) as usize;
    for (n, sample) in mix.iter_mut().rev().take(tail).enumerate() {
        *sample *= n as f32 / tail as f32;
    }
    let peak = mix.iter().fold(0f32, |m, s| m.max(s.abs())).max(1e-6);
    let samples: Vec<i16> = mix.iter().map(|s| (s / peak * 0.055 * i16::MAX as f32) as i16).collect();
    let data = samples.len() as u32 * 2;
    let mut out = Vec::with_capacity(44 + data as usize);
    out.extend(b"RIFF");
    out.extend((36 + data).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(RATE.to_le_bytes());
    out.extend((RATE * 2).to_le_bytes());
    out.extend(2u16.to_le_bytes());
    out.extend(16u16.to_le_bytes());
    out.extend(b"data");
    out.extend(data.to_le_bytes());
    out.extend(samples.iter().flat_map(|s| s.to_le_bytes()));
    out
}

static QUIET_UNTIL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

pub fn quiet() -> bool {
    now_ms() < QUIET_UNTIL.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chimes_are_valid_wav_files() {
        let sets: [(&str, &[(u32, f32, f32)], u32); 3] = [
            ("grabando", &[(0, 329.6, 0.9), (90, 493.9, 0.8), (180, 659.3, 0.75), (270, 987.8, 0.5)], 900),
            ("guardado", &[(0, 784.0, 0.6), (100, 587.3, 0.75), (200, 392.0, 0.85), (310, 261.6, 1.0)], 1100),
            ("repeticion", &[(0, 880.0, 0.7), (0, 1318.5, 0.3), (120, 1046.5, 0.6)], 650),
        ];
        for (name, notes, length) in sets {
            let sound = chime(notes, length);
            assert_eq!(&sound[..4], b"RIFF");
            assert_eq!(sound.len(), 44 + (RATE * length / 1000) as usize * 2);
            if let Ok(dir) = std::env::var("COCUYO_SONIDOS") {
                let _ = std::fs::write(std::path::Path::new(&dir).join(format!("{name}.wav")), &sound);
            }
        }
    }

    #[test]
    fn tones_are_valid_wav_files() {
        let sound = wav(&[(660.0, 70), (880.0, 90)]);
        assert_eq!(&sound[..4], b"RIFF");
        assert_eq!(&sound[8..16], b"WAVEfmt ");
        let samples = (RATE * 70 / 1000 + RATE * 90 / 1000) as usize;
        assert_eq!(sound.len(), 44 + samples * 2);
        assert_eq!(&sound[44..46], &[0, 0]);
    }

    #[test]
    #[ignore]
    fn se_oye() {
        for cue in [Cue::Listen, Cue::Done, Cue::Fail] {
            sound(cue);
            std::thread::sleep(std::time::Duration::from_millis(600));
        }
        say("Cocuyo: prueba de voz.").unwrap();
    }
}

#[tauri::command]
pub async fn voice_try(text: String) -> Result<(), String> {
    say(&text)
}
