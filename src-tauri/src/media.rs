// spotify. ojo: mandar la cancion sola con uris no hace nada, hay que mandar el album con offset. me costo una tarde entera

use std::io::Write;
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use serde_json::{json, Value};
use windows::core::{Interface, PWSTR};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Media::Audio::{
    eMultimedia, eRender, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator, ISimpleAudioVolume,
    MMDeviceEnumerator,
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::Storage::Streams::DataReader;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};

use tauri::AppHandle;

use crate::apps::CREATE_NO_WINDOW;
use crate::intent::{covers, similarity, Change};
use crate::spotify;
use crate::system::base64;

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct NowPlaying {
    title: String,
    artist: String,
    source: String,
    playing: bool,
    position: f64,
    duration: f64,
    key: String,
    art: Option<String>,
}

fn pick_session(manager: &Manager) -> Option<Session> {
    if let Ok(sessions) = manager.GetSessions() {
        for session in sessions {
            let app = session.SourceAppUserModelId().map(|s| s.to_string()).unwrap_or_default();
            if app.to_lowercase().contains("spotify") {
                return Some(session);
            }
        }
    }
    manager.GetCurrentSession().ok()
}

fn seconds(span: windows::Foundation::TimeSpan) -> f64 {
    span.Duration as f64 / 10_000_000.0
}

fn read_art(props: &windows::Media::Control::GlobalSystemMediaTransportControlsSessionMediaProperties) -> Option<String> {
    let stream = props.Thumbnail().ok()?.OpenReadAsync().ok()?.join().ok()?;
    let size = stream.Size().ok()? as u32;
    if size == 0 || size > 600_000 {
        return None;
    }
    let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0).ok()?).ok()?;
    reader.LoadAsync(size).ok()?.join().ok()?;
    let mut bytes = vec![0u8; size as usize];
    reader.ReadBytes(&mut bytes).ok()?;
    let kind = stream.ContentType().map(|s| s.to_string()).unwrap_or_else(|_| "image/jpeg".into());
    Some(format!("data:{kind};base64,{}", base64(&bytes)))
}

#[tauri::command]
pub async fn media_now(known: Option<String>) -> Option<NowPlaying> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let manager = Manager::RequestAsync().ok()?.join().ok()?;
    let session = pick_session(&manager)?;
    let props = session.TryGetMediaPropertiesAsync().ok()?.join().ok()?;
    let title = props.Title().ok()?.to_string();
    let artist = props.Artist().map(|s| s.to_string()).unwrap_or_default();
    if title.is_empty() {
        return None;
    }

    let source = session.SourceAppUserModelId().map(|s| s.to_string()).unwrap_or_default();
    let source = source.split(['!', '.', '_']).next().unwrap_or_default().trim_end_matches("AB").to_string();
    let playing = session.GetPlaybackInfo().and_then(|i| i.PlaybackStatus()).is_ok_and(|s| s == Status::Playing);
    let (position, duration) = session
        .GetTimelineProperties()
        .ok()
        .and_then(|t| Some((seconds(t.Position().ok()?), seconds(t.EndTime().ok()?))))
        .unwrap_or_default();

    let key = format!("{title}\u{1}{artist}");
    let art = if known.as_deref() == Some(key.as_str()) { None } else { read_art(&props) };
    Some(NowPlaying { title, artist, source, playing, position, duration, key, art })
}

#[tauri::command]
pub async fn media_seek(app: AppHandle, seconds: f64) -> bool {
    seek_to(&app, seconds)
}

fn seek_to(app: &AppHandle, seconds: f64) -> bool {
    let seconds = seconds.max(0.0);
    let ticks = (seconds * 10_000_000.0) as i64;
    let local = current_session().and_then(|session| session.TryChangePlaybackPositionAsync(ticks).ok()?.join().ok()).unwrap_or(false);
    local
        || spotify::token(app)
            .and_then(|(token, _)| api(&token, "PUT", &format!("/me/player/seek?position_ms={}", (seconds * 1000.0) as u64), None))
            .is_some_and(|(status, _)| (200..300).contains(&status))
}

fn local_position(session: &Session) -> Option<(f64, f64)> {
    let timeline = session.GetTimelineProperties().ok()?;
    let mut position = seconds(timeline.Position().ok()?);
    let duration = seconds(timeline.EndTime().ok()?);
    let playing = session.GetPlaybackInfo().and_then(|i| i.PlaybackStatus()).is_ok_and(|s| s == Status::Playing);
    if playing {
        let updated = timeline.LastUpdatedTime().ok()?.UniversalTime;
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos() as i64 / 100 + 116_444_736_000_000_000;
        if updated > 0 && now > updated {
            position += (now - updated) as f64 / 10_000_000.0;
        }
    }
    (duration > 0.0).then_some((position.min(duration), duration))
}

fn position_now(app: &AppHandle) -> Option<(f64, f64)> {
    let session = current_session();
    let spotify_here = session.as_ref().is_none_or(|s| s.SourceAppUserModelId().is_ok_and(|id| id.to_string().to_lowercase().contains("spotify")));
    if spotify_here {
        if let Some((200, player)) = spotify::token(app).and_then(|(token, _)| api(&token, "GET", "/me/player", None)) {
            if let (Some(at), Some(length)) = (player["progress_ms"].as_f64(), player["item"]["duration_ms"].as_f64()) {
                return Some((at / 1000.0, length / 1000.0));
            }
        }
    }
    local_position(session.as_ref()?)
}

#[derive(Debug, PartialEq)]
pub enum Seek {
    By(i64),
    To(u64),
}

fn number_at(tokens: &[&str], at: usize) -> Option<(i64, usize)> {
    const WORDS: [(&str, i64); 40] = [
        ("un", 1), ("uno", 1), ("una", 1), ("dos", 2), ("tres", 3), ("cuatro", 4), ("cinco", 5), ("seis", 6), ("siete", 7), ("ocho", 8),
        ("nueve", 9), ("diez", 10), ("quince", 15), ("veinte", 20), ("treinta", 30), ("cuarenta", 40), ("cincuenta", 50),
        ("one", 1), ("two", 2), ("three", 3), ("four", 4), ("five", 5), ("ten", 10), ("fifteen", 15), ("twenty", 20), ("thirty", 30),
        ("forty", 40), ("fifty", 50), ("um", 1), ("uma", 1), ("dois", 2), ("duas", 2), ("quatro", 4), ("dez", 10), ("vinte", 20),
        ("trinta", 30), ("quarenta", 40), ("cinquenta", 50), ("a", 1), ("an", 1),
    ];
    let word = |token: &str| token.parse::<i64>().ok().or_else(|| WORDS.iter().find(|(name, _)| *name == token).map(|(_, n)| *n));
    let first = word(tokens.get(at)?)?;
    if first >= 20 && first % 10 == 0 {
        let joined = matches!(tokens.get(at + 1), Some(&("y" | "e" | "and")));
        let next = at + 1 + usize::from(joined);
        if let Some(unit) = tokens.get(next).and_then(|t| word(t)).filter(|unit| (1..10).contains(unit)) {
            return Some((first + unit, next + 1));
        }
    }
    Some((first, at + 1))
}

fn span_in(tokens: &[&str]) -> Option<i64> {
    const SECONDS: [&str; 7] = ["segundo", "segundos", "seg", "second", "seconds", "sec", "secs"];
    const MINUTES: [&str; 5] = ["minuto", "minutos", "min", "minute", "minutes"];
    let mut total = None;
    let mut i = 0;
    while i < tokens.len() {
        if MINUTES.contains(&tokens[i]) && i > 0 && matches!(tokens[i - 1], "medio" | "half") {
            total = Some(total.unwrap_or(0) + 30);
        } else if let Some((amount, end)) = number_at(tokens, i) {
            let unit = tokens.get(end).copied().unwrap_or_default();
            if SECONDS.contains(&unit) {
                total = Some(total.unwrap_or(0) + amount);
                i = end + 1;
                continue;
            }
            if MINUTES.contains(&unit) {
                let half = matches!(tokens.get(end + 1..end + 3), Some(["y" | "e" | "and", "medio" | "meio" | "half"]));
                total = Some(total.unwrap_or(0) + amount * 60 + if half { 30 } else { 0 });
                i = end + 1 + if half { 2 } else { 0 };
                continue;
            }
        }
        i += 1;
    }
    total
}

pub fn seek_order(spoken: &str) -> Option<Seek> {
    const POLITE: [&str; 9] = ["oye", "cocuyo", "porfa", "por", "favor", "please", "hey", "me", "le"];
    const AHEAD: [&str; 15] = ["adelanta", "adelantale", "adelantala", "adelantalo", "avanza", "avanzale", "avanzala", "forward", "adianta", "avanca", "pula", "salta", "saltate", "ahead", "fast"];
    const BACK: [&str; 18] = ["atrasa", "atrasale", "atrasala", "atrasalo", "retrocede", "retrocedele", "regresa", "regresale", "regresala", "devuelve", "devuelvele", "devuelvela", "rebobina", "rewind", "back", "volta", "voltar", "vuelve"];
    const GO: [&str; 21] = ["ve", "vete", "ir", "pon", "ponla", "ponlo", "pasa", "pasala", "lleva", "llevala", "go", "jump", "skip", "vai", "coloca", "salta", "regresa", "vuelve", "devuelvete", "volta", "retrocede"];
    let clean = crate::intent::normalize(spoken);
    let all: Vec<&str> = clean.split(' ').collect();
    let start = all.iter().position(|t| !POLITE.contains(t)).unwrap_or(all.len());
    let tokens = &all[start..];
    let first = *tokens.first()?;
    let second = tokens.get(1).copied().unwrap_or_default();
    let ahead = AHEAD.contains(&first) || (matches!(first, "go" | "skip" | "jump") && matches!(second, "ahead" | "forward"));
    let back = BACK.contains(&first) || (matches!(first, "go" | "skip" | "jump") && second == "back");
    let absolute = tokens.windows(2).any(|pair| matches!(pair, ["al" | "el" | "to" | "ao" | "no" | "los" | "aos", "minuto" | "segundo" | "minute" | "second"]) || matches!(pair, ["a" | "to" | "ao", "los" | "the" | "os"]));
    if GO.contains(&first) && absolute {
        if let Some(at) = tokens.iter().position(|t| matches!(*t, "minuto" | "minute")).filter(|at| *at > 0 && matches!(tokens[at - 1], "al" | "el" | "to" | "ao" | "no")) {
            let (minutes, end) = number_at(tokens, at + 1)?;
            let rest = &tokens[end..];
            let rest = rest.strip_prefix(&["con"]).or_else(|| rest.strip_prefix(&["y"])).or_else(|| rest.strip_prefix(&["e"])).unwrap_or(rest);
            let extra = match rest {
                ["medio" | "meio", ..] => 30,
                _ => number_at(rest, 0).map_or(0, |(s, _)| s).clamp(0, 59),
            };
            return Some(Seek::To((minutes * 60 + extra).max(0) as u64));
        }
        return span_in(tokens).map(|s| Seek::To(s.max(0) as u64));
    }
    if GO.contains(&first) && matches!(second, "al" | "to" | "ao" | "a") {
        if let (Some((minutes, end)), Some(seconds)) = (number_at(tokens, 2), tokens.get(3).and_then(|t| t.parse::<i64>().ok())) {
            if end == 3 && tokens.len() == 4 && (0..60).contains(&seconds) && tokens[2].parse::<i64>().is_ok() {
                return Some(Seek::To((minutes * 60 + seconds) as u64));
            }
        }
    }
    if !(ahead || back) {
        return None;
    }
    let span = span_in(tokens)?;
    Some(Seek::By(if back { -span } else { span }))
}

fn clock(seconds: f64) -> String {
    let whole = seconds.max(0.0).round() as u64;
    format!("{}:{:02}", whole / 60, whole % 60)
}

fn seek(app: &AppHandle, order: Seek) -> Result<String, String> {
    let (now, length) = position_now(app).ok_or("No hay nada sonando que pueda adelantar o atrasar.")?;
    let target = match order {
        Seek::By(step) => now + step as f64,
        Seek::To(at) => at as f64,
    }
    .clamp(0.0, (length - 1.0).max(0.0));
    if !seek_to(app, target) {
        return Err("Lo que suena no deja moverse dentro de la canción.".into());
    }
    Ok(match order {
        Seek::By(step) if step >= 0 => format!("Adelanté {step} s, va en {}.", clock(target)),
        Seek::By(step) => format!("Atrasé {} s, va en {}.", -step, clock(target)),
        Seek::To(_) => format!("Va en {}.", clock(target)),
    })
}

pub enum Transport {
    Pause,
    Resume,
    Toggle,
    Next,
    Previous,
}

fn current_session() -> Option<Session> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    pick_session(&Manager::RequestAsync().ok()?.join().ok()?)
}

pub fn control(action: Transport) -> bool {
    let Some(session) = current_session() else { return false };
    let request = match action {
        Transport::Pause => session.TryPauseAsync(),
        Transport::Resume => session.TryPlayAsync(),
        Transport::Toggle => session.TryTogglePlayPauseAsync(),
        Transport::Next => session.TrySkipNextAsync(),
        Transport::Previous => session.TrySkipPreviousAsync(),
    };
    request.ok().and_then(|op| op.join().ok()).unwrap_or(false)
}

pub fn shuffle_and_skip() -> bool {
    let Some(session) = current_session() else { return false };
    let _ = session.TryChangeShuffleActiveAsync(true).ok().and_then(|op| op.join().ok());
    session.TrySkipNextAsync().ok().and_then(|op| op.join().ok()).unwrap_or(false)
}

fn api(token: &str, method: &str, path: &str, body: Option<&str>) -> Option<(u16, Value)> {
    let answer = request(token, method, path, body);
    match answer {
        None | Some((500..=599, _)) if method == "GET" => request(token, method, path, body),
        answer => answer,
    }
}

fn request(token: &str, method: &str, path: &str, body: Option<&str>) -> Option<(u16, Value)> {
    let quote = |text: &str| text.replace('\\', "\\\\").replace('"', "\\\"");
    let mut config = format!(
        "url = \"https://api.spotify.com/v1{}\"\nrequest = \"{method}\"\nheader = \"Authorization: Bearer {}\"\nwrite-out = \"\\n%{{http_code}}\"\n",
        quote(path),
        quote(token)
    );
    match body {
        Some(body) => config += &format!("header = \"Content-Type: application/json\"\ndata = \"{}\"\n", quote(body)),
        None if method != "GET" => config += "header = \"Content-Length: 0\"\n",
        None => {}
    }

    let mut child = Command::new("curl.exe")
        .args(["--silent", "--max-time", "8", "--config", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .ok()?;
    child.stdin.take()?.write_all(config.as_bytes()).ok()?;
    let output = child.wait_with_output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let (json, status) = text.rsplit_once('\n')?;
    Some((status.trim().parse().ok()?, serde_json::from_str(json).unwrap_or(Value::Null)))
}

fn url_encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

static DEVICE: Mutex<Option<String>> = Mutex::new(None);

macro_rules! trace {
    ($($arg:tt)*) => {
        if cfg!(debug_assertions) {
            eprintln!("   [spotify] {}", format!($($arg)*));
        }
    };
}

fn find_device(token: &str) -> Option<String> {
    let (_, devices) = api(token, "GET", "/me/player/devices", None)?;
    let devices: Vec<&Value> = devices.get("devices")?.as_array()?.iter().filter(|d| d["is_restricted"].as_bool() != Some(true)).collect();
    let pick = devices
        .iter()
        .find(|d| d["is_active"].as_bool() == Some(true))
        .or_else(|| devices.iter().find(|d| d["type"].as_str() == Some("Computer")))
        .or_else(|| devices.first())?;
    pick["id"].as_str().map(str::to_string)
}

fn sounding_here() -> Option<String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let manager = Manager::RequestAsync().ok()?.join().ok()?;
    let session = manager.GetSessions().ok()?.into_iter().find(|session| {
        session.SourceAppUserModelId().is_ok_and(|app| app.to_string().to_lowercase().contains("spotify"))
    })?;
    if !session.GetPlaybackInfo().and_then(|info| info.PlaybackStatus()).is_ok_and(|status| status == Status::Playing) {
        return None;
    }
    let props = session.TryGetMediaPropertiesAsync().ok()?.join().ok()?;
    Some(format!("{}\u{1}{}", props.Title().ok()?, props.Artist().ok()?))
}

fn sounds(token: &str, before: &Option<String>, context: Option<&str>) -> bool {
    for _ in 0..8 {
        if sounding_here().is_some_and(|now| before.as_ref() != Some(&now)) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    let Some((200, player)) = api(token, "GET", "/me/player", None) else { return false };
    let now = player["context"]["uri"].as_str().unwrap_or_default();
    let same = context.is_none_or(|asked| asked == now || (asked.ends_with(":collection") && now.contains("collection")));
    trace!("¿suena lo pedido? suena={} contexto={same}", player["is_playing"]);
    player["is_playing"].as_bool() == Some(true) && same
}

fn wait_for_device(token: &str) -> Option<String> {
    for _ in 0..12 {
        std::thread::sleep(Duration::from_millis(1000));
        if let Some(device) = find_device(token) {
            return Some(device);
        }
    }
    None
}

static BEFORE_MUTE: AtomicU8 = AtomicU8::new(50);

fn apply(change: Change, now: u8) -> u8 {
    match change {
        Change::By(step) => (i16::from(now) + i16::from(step)).clamp(0, 100) as u8,
        Change::Set(percent) => percent.min(100),
        Change::Mute if now > 0 => {
            BEFORE_MUTE.store(now, Ordering::Relaxed);
            0
        }
        Change::Mute | Change::Unmute if now == 0 => BEFORE_MUTE.load(Ordering::Relaxed).max(10),
        Change::Mute | Change::Unmute => now,
    }
}

fn api_volume(app: &AppHandle, change: Change) -> Option<u8> {
    let (token, _) = spotify::token(app)?;
    let (200, player) = api(&token, "GET", "/me/player", None)? else { return None };
    let device = &player["device"];
    if device["supports_volume"].as_bool() == Some(false) {
        return None;
    }
    let target = apply(change, device["volume_percent"].as_u64()?.min(100) as u8);
    let (status, _) = api(&token, "PUT", &format!("/me/player/volume?volume_percent={target}"), None)?;
    (200..300).contains(&status).then_some(target)
}

fn is_spotify(pid: u32) -> bool {
    unsafe {
        let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return false };
        let mut path = [0u16; 520];
        let mut len = path.len() as u32;
        let named = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(path.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(process);
        named && String::from_utf16_lossy(&path[..len as usize]).to_lowercase().ends_with("\\spotify.exe")
    }
}

fn mixer_volume(change: Change) -> Option<u8> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let manager: IAudioSessionManager2 = devices.GetDefaultAudioEndpoint(eRender, eMultimedia).ok()?.Activate(CLSCTX_ALL, None).ok()?;
        let sessions = manager.GetSessionEnumerator().ok()?;
        let mut result = None;
        for index in 0..sessions.GetCount().ok()? {
            let Ok(control) = sessions.GetSession(index).and_then(|session| session.cast::<IAudioSessionControl2>()) else { continue };
            if !control.GetProcessId().is_ok_and(is_spotify) {
                continue;
            }
            let Ok(volume) = control.cast::<ISimpleAudioVolume>() else { continue };
            let muted = volume.GetMute().is_ok_and(|muted| muted.as_bool());
            let now = if muted { 0 } else { (volume.GetMasterVolume().ok()? * 100.0).round() as u8 };
            let target = apply(change, now);
            let _ = volume.SetMute(target == 0, std::ptr::null());
            if target > 0 {
                volume.SetMasterVolume(f32::from(target) / 100.0, std::ptr::null()).ok()?;
            }
            result = Some(target);
        }
        result
    }
}

#[cfg(debug_assertions)]
pub fn hush(on: bool) -> Option<u8> {
    mixer_volume(if on { Change::Mute } else { Change::Unmute })
}

pub fn volume(app: &AppHandle, change: Change) -> Option<u8> {
    api_volume(app, change).or_else(|| mixer_volume(change))
}

#[cfg(debug_assertions)]
pub fn raw(app: &AppHandle, path: &str) -> String {
    let Some((token, _)) = spotify::token(app) else { return "sin sesión".into() };
    match api(&token, "GET", path, None) {
        Some((status, json)) => format!("{status} {}", json.to_string().chars().take(60000).collect::<String>()),
        None => "sin respuesta".into(),
    }
}

#[cfg(debug_assertions)]
pub fn library_check(app: &AppHandle) -> String {
    let Some((token, source)) = spotify::token(app) else { return "sin sesión de Spotify".into() };
    let mut out = format!("llave {} · permisos: {}", if source == spotify::Source::Cocuyo { "propia" } else { "de Mis Notas" }, spotify::GRANTED.lock().map(|g| g.clone()).unwrap_or_default());
    let show = |label: &str, answer: Option<(u16, Value)>| match answer {
        Some((status, json)) => format!("\n{label}: {status} {}", json.to_string().chars().take(300).collect::<String>()),
        None => format!("\n{label}: sin respuesta"),
    };
    let now = api(&token, "GET", "/me/player/currently-playing", None);
    let id = now.as_ref().and_then(|(_, j)| j["item"]["id"].as_str().map(str::to_string)).unwrap_or_else(|| "4uLU6hMCjMI75M1A2tKUQC".into());
    out += &format!("\ncanción: {id}");
    out += &show("library contains", api(&token, "GET", &format!("/me/library/contains?uris=spotify:track:{id}"), None));
    out += &show("tracks", api(&token, "GET", "/me/tracks?limit=1", None).map(|(s, j)| (s, json!({ "total": j["total"], "error": j["error"] }))));
    out += &show("me", api(&token, "GET", "/me", None).map(|(s, j)| (s, json!({ "id": j["id"], "product": j["product"], "country": j["country"] }))));
    let lists = api(&token, "GET", "/me/playlists?limit=50", None);
    out += &show("listas", lists.clone().map(|(s, j)| (s, json!({ "total": j["total"], "primera": j["items"][0].as_object().map(|o| o.keys().cloned().collect::<Vec<_>>()) }))));
    let mine = api(&token, "GET", "/me", None).and_then(|(_, me)| me["id"].as_str().map(str::to_string)).unwrap_or_default();
    let owned = lists.and_then(|(_, j)| j["items"].as_array()?.iter().find(|l| l["owner"]["id"].as_str() == Some(mine.as_str())).cloned());
    if let Some(list) = owned {
        let list_id = list["id"].as_str().unwrap_or_default();
        out += &format!("\nlista propia: {} ({})", list["name"], list_id);
        out += &show("items", api(&token, "GET", &format!("/playlists/{list_id}/items?limit=1"), None).map(|(s, j)| (s, json!({ "total": j["total"], "llaves": j["items"][0].as_object().map(|o| o.keys().cloned().collect::<Vec<_>>()), "error": j["error"] }))));
        out += &show("tracks viejo", api(&token, "GET", &format!("/playlists/{list_id}/tracks?limit=1"), None).map(|(s, j)| (s, json!({ "total": j["total"], "error": j["error"] }))));
    }
    out += &show("search 10", api(&token, "GET", "/search?q=salsa&type=track&limit=10", None).map(|(s, j)| (s, json!({ "n": j["tracks"]["items"].as_array().map(Vec::len), "error": j["error"] }))));
    out += &show("player", api(&token, "GET", "/me/player", None).map(|(s, j)| (s, json!({ "suena": j["is_playing"], "contexto": j["context"]["uri"], "en": j["progress_ms"], "dura": j["item"]["duration_ms"], "aleatorio": j["shuffle_state"] }))));
    out
}

#[cfg(debug_assertions)]
pub fn drop_list(app: &AppHandle, name: &str) -> String {
    let Some((token, _)) = spotify::token(app) else { return "sin sesión de Spotify".into() };
    let Ok(lists) = my_lists(&token) else { return "sin listas".into() };
    let Some(list) = lists.iter().find(|list| list["name"].as_str() == Some(name)) else { return format!("no hay lista «{name}»") };
    let id = list["id"].as_str().unwrap_or_default();
    let inside = list_entries(&token, id).into_iter().map(|(_, name, artist)| format!("{name} · {artist}")).collect::<Vec<_>>();
    let gone = api(&token, "DELETE", &format!("/me/library?uris=spotify:playlist:{id}"), None).map(|(s, _)| s);
    let old = if gone.is_some_and(|s| (200..300).contains(&s)) { None } else { api(&token, "DELETE", &format!("/playlists/{id}/followers"), None).map(|(s, _)| s) };
    format!("lista «{name}» ({id}) tenía {inside:?}; quitada de la biblioteca: {gone:?} {old:?}")
}

#[cfg(debug_assertions)]
pub fn try_context(app: &AppHandle, what: &str) -> String {
    let Some((token, _)) = spotify::token(app) else { return "sin sesión".into() };
    let device = find_device(&token);
    let body = if what.contains('|') || what.starts_with("spotify:track:") {
        json!({ "uris": what.split('|').collect::<Vec<_>>() })
    } else {
        json!({ "context_uri": what })
    };
    let path = device.as_deref().map_or("/me/player/play".to_string(), |id| format!("/me/player/play?device_id={id}"));
    let sent = api(&token, "PUT", &path, Some(&body.to_string()));
    let mut seen = Vec::new();
    for _ in 0..7 {
        std::thread::sleep(Duration::from_millis(1000));
        let player = api(&token, "GET", "/me/player", None).map(|(_, p)| format!("{}·{}·{}", p["is_playing"], p["context"]["uri"], p["item"]["name"]));
        seen.push(player.unwrap_or_default());
    }
    format!("{what}: {:?} -> {seen:?}", sent.map(|(s, j)| (s, j["error"]["message"].clone())))
}

#[cfg(debug_assertions)]
pub fn snapshot(app: &AppHandle) -> Option<String> {
    let (token, _) = spotify::token(app)?;
    let (200, player) = api(&token, "GET", "/me/player", None)? else { return None };
    Some(json!({ "contexto": player["context"]["uri"], "cancion": player["item"]["uri"], "en": player["progress_ms"], "suena": player["is_playing"], "aleatorio": player["shuffle_state"] }).to_string())
}

#[cfg(debug_assertions)]
pub fn restore(app: &AppHandle, saved: &str) -> String {
    let Some((token, _)) = spotify::token(app) else { return "sin sesión".into() };
    let Ok(saved) = serde_json::from_str::<Value>(saved) else { return "sin foto".into() };
    let _ = api(&token, "PUT", &format!("/me/player/shuffle?state={}", saved["aleatorio"].as_bool().unwrap_or(false)), None);
    let mut body = json!({ "position_ms": saved["en"].as_u64().unwrap_or(0) });
    match saved["contexto"].as_str() {
        Some(context) => {
            body["context_uri"] = json!(context);
            body["offset"] = json!({ "uri": saved["cancion"] });
        }
        None => body["uris"] = json!([saved["cancion"]]),
    }
    let played = api(&token, "PUT", "/me/player/play", Some(&body.to_string())).map(|(s, _)| s);
    let paused = if saved["suena"].as_bool() == Some(true) { None } else { api(&token, "PUT", "/me/player/pause", None).map(|(s, _)| s) };
    format!("vuelta a como estaba: {played:?} {paused:?}")
}

#[cfg(debug_assertions)]
pub fn probe(app: &AppHandle, query: &str) -> String {
    let Some((token, source)) = spotify::token(app) else { return "sin sesión de Spotify".into() };
    let source = if source == spotify::Source::Cocuyo { "propia" } else { "de Mis Notas" };
    let choice = if crate::intent::parse(&format!("pon {query}")) == crate::intent::Intent::Mood(query.into()) { pick_mix(&token, query).ok_or_else(|| "sin listas".to_string()) } else { pick(&token, query) };
    let track = choice.map(|(_, played)| sounding_name(&played));
    let write = api(&token, "GET", "/me/player", None).and_then(|(_, player)| {
        let volume = player["device"]["volume_percent"].as_u64()?;
        api(&token, "PUT", &format!("/me/player/volume?volume_percent={volume}"), None).map(|(status, _)| status)
    });
    format!(
        "sesión {source}; búsqueda {track:?}; dispositivo encontrado: {}; orden de escritura (mismo volumen): {write:?}",
        find_device(&token).is_some()
    )
}

pub enum Played {
    Track(String, String),
    Artist(String),
    List(String),
    Mix(String),
    NoSession,
    Failed(String),
}

fn start_playback(token: &str, body: &str) -> Result<(), String> {
    let start = |device: Option<&str>| {
        let path = match device {
            Some(id) => format!("/me/player/play?device_id={}", url_encode(id)),
            None => "/me/player/play".to_string(),
        };
        let status = api(token, "PUT", &path, Some(body)).map(|(status, _)| status);
        trace!("reproducir en {device:?}: {status:?}");
        status
    };
    let refused = |status: Option<u16>| match status {
        Some(403) => Some("Spotify solo deja elegir la canción desde fuera con una cuenta Premium."),
        Some(401) => Some("La sesión de Spotify venció. Vuelve a conectarla en Ajustes."),
        _ => None,
    };

    let context = serde_json::from_str::<Value>(body).ok().and_then(|body| body["context_uri"].as_str().map(str::to_string));
    let before = sounding_here();
    let known = DEVICE.lock().unwrap().clone();
    let status = start(known.as_deref());
    if let Some(why) = refused(status) {
        return Err(why.into());
    }
    if matches!(status, Some(200..=299)) && sounds(token, &before, context.as_deref()) {
        return Ok(());
    }

    let device = match find_device(token) {
        Some(device) => device,
        None => {
            trace!("sin dispositivos: abriendo Spotify");
            crate::apps::open("spotify:").map_err(|_| "Spotify no está abierto y no lo pude abrir.".to_string())?;
            wait_for_device(token).ok_or("Abrí Spotify, pero no llegó a estar listo. Inténtalo otra vez en un momento.")?
        }
    };
    let transfer = serde_json::json!({ "device_ids": [device], "play": false }).to_string();
    let woke = api(token, "PUT", "/me/player", Some(&transfer)).map(|(status, _)| status);
    trace!("despertar {device}: {woke:?}");
    std::thread::sleep(Duration::from_millis(700));
    for attempt in 0..2 {
        let status = start(Some(&device));
        if let Some(why) = refused(status) {
            return Err(why.into());
        }
        if matches!(status, Some(200..=299)) && sounds(token, &before, context.as_deref()) {
            *DEVICE.lock().unwrap() = Some(device);
            return Ok(());
        }
        trace!("intento {attempt}: sigue sin sonar");
        std::thread::sleep(Duration::from_millis(900));
    }
    *DEVICE.lock().unwrap() = None;
    Err("Spotify recibió la orden pero no empezó a sonar. Toca reproducir una vez en Spotify y vuelve a pedírmelo.".into())
}

pub fn play_random(app: &AppHandle) -> Played {
    let Some((token, _)) = spotify::token(app) else { return Played::NoSession };
    let Some((200, lists)) = api(&token, "GET", "/me/playlists?limit=50", None) else {
        return Played::Failed("Spotify no devolvió tus listas.".into());
    };
    let lists: Vec<&Value> = lists["items"].as_array().map(|a| a.iter().filter(|l| l["uri"].is_string()).collect()).unwrap_or_default();
    if lists.is_empty() {
        return Played::Failed("No tienes listas en Spotify de donde elegir.".into());
    }
    let dice = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(7, |d| d.subsec_nanos() as usize);
    let list = lists[dice % lists.len()];
    let total = list["tracks"]["total"].as_u64().or_else(|| list["items"]["total"].as_u64()).unwrap_or(1).max(1) as usize;
    let body = serde_json::json!({
        "context_uri": list["uri"],
        "offset": { "position": (dice / 7) % total },
    })
    .to_string();

    let _ = api(&token, "PUT", "/me/player/shuffle?state=true", None);
    match start_playback(&token, &body) {
        Ok(()) => Played::List(list["name"].as_str().unwrap_or("una lista").to_string()),
        Err(why) => Played::Failed(why),
    }
}

pub fn play_list(app: &AppHandle, name: &str) -> Played {
    let Some((token, _)) = spotify::token(app) else { return Played::NoSession };
    let Some((200, lists)) = api(&token, "GET", "/me/playlists?limit=50", None) else {
        return Played::Failed("Spotify no devolvió tus listas.".into());
    };
    let best = lists["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|list| Some((similarity(name, list["name"].as_str()?), list)))
        .filter(|(score, list)| *score >= 0.72 && list["uri"].is_string())
        .max_by(|a, b| a.0.total_cmp(&b.0));
    let Some((_, list)) = best else {
        return Played::Failed(format!("No encontré ninguna lista tuya llamada «{name}»."));
    };
    match start_playback(&token, &serde_json::json!({ "context_uri": list["uri"] }).to_string()) {
        Ok(()) => Played::List(list["name"].as_str().unwrap_or(name).to_string()),
        Err(why) => Played::Failed(why),
    }
}

pub fn restart(app: &AppHandle) -> bool {
    let local = current_session().and_then(|session| session.TryChangePlaybackPositionAsync(0).ok()?.join().ok()).unwrap_or(false);
    local
        || spotify::token(app)
            .and_then(|(token, _)| api(&token, "PUT", "/me/player/seek?position_ms=0", None))
            .is_some_and(|(status, _)| (200..300).contains(&status))
}

#[cfg(debug_assertions)]
fn sounding_name(played: &Played) -> String {
    match played {
        Played::Track(title, artist) => format!("canción «{title}» de {artist}"),
        Played::Artist(name) => format!("artista {name}"),
        Played::List(name) => format!("lista «{name}»"),
        Played::Mix(name) => format!("lista pública «{name}»"),
        _ => "nada".into(),
    }
}

fn bare_title(title: &str) -> &str {
    let title = title.split(" - ").next().unwrap_or(title);
    title.split(" (").next().unwrap_or(title).trim()
}

fn pick(token: &str, query: &str) -> Result<(String, Played), String> {
    let search = format!("/search?q={}&type=track%2Cartist&limit=5", url_encode(query));
    let Some((200, found)) = api(token, "GET", &search, None) else {
        return Err("Spotify no respondió a la búsqueda.".into());
    };
    let names = |track: &Value| -> (String, String) {
        let artists: Vec<&str> = track["artists"].as_array().into_iter().flatten().filter_map(|a| a["name"].as_str()).collect();
        (track["name"].as_str().unwrap_or_default().to_string(), artists.join(", "))
    };
    let mut best: Option<(f32, f32, &Value)> = None;
    for track in found["tracks"]["items"].as_array().into_iter().flatten().filter(|t| t["uri"].is_string()) {
        let (title, artists) = names(track);
        let by_title = similarity(query, bare_title(&title));
        let score = by_title.max(0.95 * covers(query, &format!("{} {artists}", bare_title(&title))));
        let score = if bare_title(&title).len() < title.len() { score - 0.02 } else { score };
        if best.is_none_or(|(top, _, _)| score > top) {
            best = Some((score, by_title, track));
        }
    }
    let artist = found["artists"]["items"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|artist| artist["name"].as_str().is_some_and(|name| similarity(query, name) >= 0.9))
        .filter(|_| !best.is_some_and(|(_, by_title, _)| by_title >= 0.9));
    if let Some((uri, name)) = artist.and_then(|artist| Some((artist["uri"].as_str()?, artist["name"].as_str()?))) {
        return Ok((serde_json::json!({ "context_uri": uri }).to_string(), Played::Artist(name.to_string())));
    }
    let Some((_, _, track)) = best else {
        return Err(format!("Spotify no encontró nada para «{query}»."));
    };
    let (title, artists) = names(track);
    let body = match track["album"]["uri"].as_str() {
        Some(album) => serde_json::json!({ "context_uri": album, "offset": { "uri": track["uri"] } }),
        None => serde_json::json!({ "uris": [track["uri"]] }),
    };
    Ok((body.to_string(), Played::Track(title, artists)))
}

fn pick_mix(token: &str, query: &str) -> Option<(String, Played)> {
    let search = format!("/search?q={}&type=playlist&limit=5", url_encode(query));
    let (200, found) = api(token, "GET", &search, None)? else { return None };
    let list = found["playlists"]["items"]
        .as_array()?
        .iter()
        .rev()
        .filter(|list| list["uri"].is_string())
        .max_by(|a, b| covers(query, a["name"].as_str().unwrap_or_default()).total_cmp(&covers(query, b["name"].as_str().unwrap_or_default())))?;
    let name: String = list["name"].as_str().unwrap_or(query).chars().take(48).collect();
    Some((serde_json::json!({ "context_uri": list["uri"] }).to_string(), Played::Mix(name.trim().to_string())))
}

pub fn play_mood(app: &AppHandle, query: &str) -> Played {
    let Some((token, _)) = spotify::token(app) else { return Played::NoSession };
    let choice = pick_mix(&token, query).map_or_else(|| pick(&token, query), Ok);
    match choice.and_then(|(body, played)| start_playback(&token, &body).map(|()| played)) {
        Ok(played) => played,
        Err(why) => Played::Failed(why),
    }
}

pub fn play(app: &AppHandle, query: &str) -> Played {
    let Some((token, _)) = spotify::token(app) else { return Played::NoSession };
    match pick(&token, query).and_then(|(body, played)| start_playback(&token, &body).map(|()| played)) {
        Ok(played) => played,
        Err(why) => Played::Failed(why),
    }
}

fn outcome(result: Option<(u16, Value)>) -> Result<Value, String> {
    match result {
        None => Err("Spotify no respondió. Revisa tu conexión.".into()),
        Some((status, body)) if (200..300).contains(&status) => Ok(body),
        Some((401 | 403, _)) => Err("A la sesión de Spotify le falta ese permiso. Desconecta y vuelve a conectar Spotify en Ajustes.".into()),
        Some((404, _)) => Err("Spotify no tiene nada sonando en ningún dispositivo.".into()),
        Some((status, _)) => Err(format!("Spotify respondió con el error {status}.")),
    }
}

fn session(app: &AppHandle) -> Result<String, String> {
    spotify::token(app).map(|(token, _)| token).ok_or_else(|| "Conecta Spotify en Ajustes.".to_string())
}

fn renamed(token: &str, method: &str, path: &str, old: &str, body: Option<&str>, old_body: Option<&str>) -> Option<(u16, Value)> {
    match api(token, method, path, body) {
        Some((403 | 404 | 405, _)) | None => api(token, method, old, old_body.or(body)),
        answer => answer,
    }
}

fn borrowed_key(app: &AppHandle, failed: Result<Value, String>) -> Result<Value, String> {
    match failed {
        Err(why) if why.starts_with("A la sesión de Spotify le falta") && spotify::token(app).is_some_and(|(_, source)| source == spotify::Source::MisNotas) => {
            Err("Estoy usando la llave de Mis Notas y no trae ese permiso. Conecta tu propia llave en Ajustes → Música.".into())
        }
        other => other,
    }
}

fn playing(token: &str) -> Result<(String, String, String), String> {
    let now = outcome(api(token, "GET", "/me/player/currently-playing", None))?;
    match (now["item"]["id"].as_str(), now["item"]["uri"].as_str(), now["item"]["name"].as_str()) {
        (Some(id), Some(uri), Some(name)) => Ok((id.to_string(), uri.to_string(), name.to_string())),
        _ => Err("Spotify no tiene nada sonando en ningún dispositivo.".into()),
    }
}

fn is_liked(app: &AppHandle) -> Result<String, String> {
    let token = session(app)?;
    let (id, _, name) = playing(&token)?;
    let answer = renamed(&token, "GET", &format!("/me/library/contains?uris=spotify:track:{id}"), &format!("/me/tracks/contains?ids={id}"), None, None);
    let saved = borrowed_key(app, outcome(answer))?;
    let said = if saved[0].as_bool() == Some(true) {
        format!("Sí, «{name}» está en tus Me gusta.")
    } else {
        format!("No, «{name}» no está en tus Me gusta. Dime «dale me gusta» y la guardo.")
    };
    crate::cue::answer(&crate::i18n::tr(&said));
    Ok(said)
}

#[derive(Debug, PartialEq)]
pub enum Like {
    Save(Option<String>),
    Drop,
    Ask,
}

const NOW_SONG: [&str; 14] = ["", "la", "esta", "esta cancion", "la cancion", "esta rola", "este tema", "lo que suena", "la que suena", "esta musica", "this", "this song", "essa", "essa musica"];

pub fn like_order(clean: &str) -> Option<Like> {
    let has = |options: &[&str]| options.iter().any(|option| clean.contains(option));
    let words: Vec<&str> = clean.split(' ').collect();
    let start = words.iter().position(|w| !matches!(*w, "oye" | "cocuyo" | "porfa" | "please" | "hey" | "ya")).unwrap_or(words.len());
    let clean = words[start..].join(" ");
    let clean = clean.as_str();
    if has(&["quita esta de me gusta", "quitala de me gusta", "quitala de mis me gusta", "quita de mis me gusta", "quita esta cancion de me gusta", "quita esta cancion de mis me gusta", "sacala de me gusta", "sacala de mis me gusta", "ya no me gusta esta", "quita el me gusta", "quitale el me gusta", "quita el like", "quitale el like", "unlike this", "remove from liked", "descurti"]) {
        return Some(Like::Drop);
    }
    const PLACES: [&str; 18] = [
        " a mis me gusta", " en mis me gusta", " a me gusta", " en me gusta", " a los me gusta", " en los me gusta", " a mis favoritos", " en mis favoritos", " a favoritos", " en favoritos",
        " a mis favoritas", " en mis favoritas", " to my liked songs", " to liked songs", " to my likes", " nas curtidas", " nas minhas curtidas", " em curtidas",
    ];
    const SAVERS: [&str; 22] = ["guarda", "guardala", "guardame", "guardamela", "agrega", "agregala", "anade", "anadela", "añade", "añadela", "mete", "metela", "manda", "mandala", "save", "add", "salva", "salvala", "adiciona", "coloca", "pon", "ponla"];
    let lead = SAVERS.iter().filter(|verb| clean == **verb || clean.starts_with(&format!("{verb} "))).max_by_key(|verb| verb.len());
    if let Some(verb) = lead {
        let rest = clean[verb.len()..].trim();
        let padded = format!(" {rest}");
        let (what, placed) = PLACES.iter().find_map(|place| padded.strip_suffix(place).map(|left| (left.trim(), true))).unwrap_or((rest, false));
        if placed {
            return Some(Like::Save((!NOW_SONG.contains(&what)).then(|| what.to_string())));
        }
        if !matches!(*verb, "pon" | "ponla" | "coloca" | "add" | "manda" | "mandala") && NOW_SONG.contains(&what) && (what != "" || verb.ends_with("la")) {
            return Some(Like::Save(None));
        }
    }
    let first = clean.split(' ').next().unwrap_or_default();
    let strict = matches!(first, "pon" | "ponla");
    if strict || matches!(first, "dale" | "dele" | "ponle" | "ponele" | "metele" | "regalale" | "da" | "dame") {
        let rest = clean[first.len()..].trim();
        let fits = ["un me gusta", "el me gusta", "me gusta", "un like", "el like", "like", "un corazon", "el corazon", "corazon", "corazoncito"].iter().any(|mark| {
            let Some(after) = rest.strip_prefix(mark).filter(|after| after.is_empty() || after.starts_with(' ')) else { return false };
            let after = after.trim();
            let to_this = ["a esta", "a esa", "a la cancion", "a la rola", "a lo que suena", "a este tema"].iter().any(|to| after == *to || after.starts_with(&format!("{to} ")));
            after.is_empty() || to_this || (!strict && !after.starts_with("a "))
        });
        if fits {
            return Some(Like::Save(None));
        }
    }
    if has(&["esta en mis me gusta", "esta en me gusta", "esta en los me gusta", "esta en mis favoritos", "la tengo en me gusta", "la tengo en mis me gusta", "ya le di me gusta", "ya tiene me gusta", "tiene me gusta", "esta guardada en", "is this liked", "is this song liked", "in my liked songs", "ta nas curtidas", "esta nas curtidas", "ja curti essa"]) {
        return Some(Like::Ask);
    }
    if clean.starts_with("me gusta esta") || clean.starts_with("me encanta esta") || matches!(clean, "like" | "like this song" | "i like this song" | "save this song" | "curti essa" | "curti essa musica") {
        return Some(Like::Save(None));
    }
    None
}

fn like_named(app: &AppHandle, what: &str) -> Result<String, String> {
    let token = session(app)?;
    let (uri, name, _) = find_track(&token, what)?;
    let id = uri.rsplit(':').next().unwrap_or_default().to_string();
    borrowed_key(app, library(&token, "PUT", &id))?;
    Ok(format!("Guardada en tus Me gusta: «{name}»."))
}

fn liked_order(clean: &str) -> Option<bool> {
    const PLAY: [&str; 13] = ["pon ", "ponme ", "reproduce ", "reproduceme ", "play ", "toca ", "tocame ", "coloca ", "dale play a ", "escuchar ", "quiero escuchar ", "quiero oir ", "bota "];
    const LIKED: [&str; 13] = ["mis me gusta", "los me gusta", "mis canciones que me gustan", "las canciones que me gustan", "mis canciones favoritas", "mis favoritas", "mis likes", "mis canciones guardadas", "my liked songs", "liked songs", "my likes", "minhas curtidas", "as curtidas"];
    let rest = PLAY.iter().filter_map(|lead| clean.strip_prefix(lead)).next()?.trim();
    let rest = rest.trim_end_matches(" en spotify").trim_end_matches(" on spotify").trim_end_matches(" no spotify");
    let (rest, shuffle) = [" en aleatorio", " en modo aleatorio", " mezcladas", " revueltas", " shuffled", " on shuffle", " no aleatorio", " aleatorio"]
        .iter()
        .find_map(|end| rest.strip_suffix(end).map(|left| (left.trim(), true)))
        .unwrap_or((rest, false));
    LIKED.contains(&rest).then_some(shuffle)
}

const MIRROR_MAX: usize = 500;

fn mirror_file(app: &AppHandle) -> Option<std::path::PathBuf> {
    use tauri::Manager;
    Some(app.path().app_config_dir().ok()?.join("spotify-espejo.json"))
}

fn fingerprint(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3))
}

fn liked_page(page: &Value) -> Vec<String> {
    page["items"].as_array().into_iter().flatten().filter_map(|entry| entry["track"]["uri"].as_str()).filter(|uri| uri.starts_with("spotify:track:")).map(str::to_string).collect()
}

fn mirror_names() -> (&'static str, &'static str) {
    match crate::cue::language().0 {
        "en" => ("My Liked Songs · Cocuyo", "Your Liked Songs, kept up to date. Cocuyo makes it so you can play them by voice, since Spotify won't let it play them directly."),
        "pt" => ("Minhas Curtidas · Cocuyo", "Suas Curtidas em dia. O Cocuyo monta para você tocar por voz, porque o Spotify não deixa tocar direto."),
        _ => ("Mis Me gusta · Cocuyo", "Tus Me gusta al día. Cocuyo la arma para ponerlos por voz, porque Spotify no deja ponerlos directo."),
    }
}

fn mirror(app: &AppHandle, token: &str) -> Result<String, String> {
    let first = borrowed_key(app, outcome(api(token, "GET", "/me/tracks?limit=50", None)))?;
    let total = first["total"].as_u64().unwrap_or(0) as usize;
    let mut uris = liked_page(&first);
    if uris.is_empty() {
        return Err("No tienes canciones en tus Me gusta.".into());
    }
    let sign = format!("{total}:{:x}", fingerprint(&uris.join(",")));
    let file = mirror_file(app);
    let saved: Value = file.as_ref().and_then(|file| std::fs::read(file).ok()).and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
    let known = saved["id"].as_str().filter(|id| !id.is_empty()).map(str::to_string).filter(|id| {
        let there = renamed(token, "GET", &format!("/me/library/contains?uris=spotify:playlist:{id}"), &format!("/playlists/{id}/followers/contains"), None, None);
        !matches!(there, Some((200, ref answer)) if answer[0].as_bool() == Some(false))
    });
    if let Some(id) = known.as_ref().filter(|_| saved["firma"].as_str() == Some(sign.as_str())) {
        trace!("espejo al día: {id}");
        return Ok(id.clone());
    }
    let id = match known {
        Some(id) => id,
        None => {
            let (name, about) = mirror_names();
            let body = json!({ "name": name, "public": false, "description": about }).to_string();
            let made = match api(token, "POST", "/me/playlists", Some(&body)) {
                Some((403 | 404 | 405, _)) | None => {
                    let me = outcome(api(token, "GET", "/me", None))?;
                    let mine = me["id"].as_str().ok_or("Spotify no dijo quién eres.")?;
                    api(token, "POST", &format!("/users/{mine}/playlists"), Some(&body))
                }
                answer => answer,
            };
            borrowed_key(app, outcome(made))?["id"].as_str().ok_or("Spotify no creó la lista.")?.to_string()
        }
    };
    let mut offset = 50;
    while uris.len() < total.min(MIRROR_MAX) && offset < MIRROR_MAX {
        let page = borrowed_key(app, outcome(api(token, "GET", &format!("/me/tracks?limit=50&offset={offset}"), None)))?;
        let more = liked_page(&page);
        if page["items"].as_array().is_none_or(Vec::is_empty) {
            break;
        }
        uris.extend(more);
        offset += 50;
    }
    uris.truncate(MIRROR_MAX);
    for (index, chunk) in uris.chunks(100).enumerate() {
        let body = json!({ "uris": chunk }).to_string();
        let method = if index == 0 { "PUT" } else { "POST" };
        borrowed_key(app, outcome(renamed(token, method, &format!("/playlists/{id}/items"), &format!("/playlists/{id}/tracks"), Some(&body), None)))?;
    }
    trace!("espejo puesto al día: {id} con {} canciones", uris.len());
    if let Some(file) = file {
        let _ = std::fs::write(file, json!({ "id": id, "firma": sign }).to_string());
    }
    Ok(id)
}

fn play_liked(app: &AppHandle, shuffle: bool) -> Result<String, String> {
    let token = session(app)?;
    let id = mirror(app, &token)?;
    if shuffle {
        let _ = api(&token, "PUT", "/me/player/shuffle?state=true", None);
    }
    start_playback(&token, &json!({ "context_uri": format!("spotify:playlist:{id}") }).to_string())?;
    Ok(if shuffle { "Sonando tus Me gusta en aleatorio." } else { "Sonando tus Me gusta." }.into())
}

fn queue(app: &AppHandle, query: &str) -> Result<String, String> {
    let token = session(app)?;
    let (uri, name, artist) = find_track(&token, query)?;
    outcome(api(&token, "POST", &format!("/me/player/queue?uri={}", url_encode(&uri)), None))?;
    Ok(format!("En cola: «{name}» de {artist}."))
}

fn library(token: &str, method: &str, id: &str) -> Result<Value, String> {
    match api(token, method, &format!("/me/library?uris=spotify:track:{id}"), None) {
        Some((404 | 405, _)) | None => outcome(api(token, method, &format!("/me/tracks?ids={id}"), None)),
        answer => outcome(answer),
    }
}

fn like(app: &AppHandle) -> Result<String, String> {
    let token = session(app)?;
    let (id, _, name) = playing(&token)?;
    borrowed_key(app, library(&token, "PUT", &id))?;
    Ok(format!("Guardada en tus Me gusta: «{name}»."))
}

fn unlike(app: &AppHandle) -> Result<String, String> {
    let token = session(app)?;
    let (id, _, name) = playing(&token)?;
    borrowed_key(app, library(&token, "DELETE", &id))?;
    Ok(format!("Quitada de tus Me gusta: «{name}»."))
}

fn track_score(query: &str, name: &str, artist: &str) -> f32 {
    let bare = bare_title(name);
    let score = similarity(query, bare).max(0.95 * covers(query, &format!("{bare} {artist}")));
    if bare.len() < name.len() { score - 0.02 } else { score }
}

fn find_track(token: &str, query: &str) -> Result<(String, String, String), String> {
    let found = outcome(api(token, "GET", &format!("/search?type=track&limit=10&q={}", url_encode(query)), None))?;
    found["tracks"]["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|track| Some((track["uri"].as_str()?, track["name"].as_str()?, track["artists"][0]["name"].as_str().unwrap_or(""))))
        .fold(None::<(f32, (&str, &str, &str))>, |best, found| {
            let score = track_score(query, found.1, found.2);
            if best.is_none_or(|(top, _)| score > top) { Some((score, found)) } else { best }
        })
        .map(|(_, (uri, name, artist))| (uri.to_string(), name.to_string(), artist.to_string()))
        .ok_or_else(|| format!("No encontré «{query}» en Spotify."))
}

fn my_lists(token: &str) -> Result<Vec<Value>, String> {
    let me = outcome(api(token, "GET", "/me", None))?;
    let mine = me["id"].as_str().unwrap_or_default().to_string();
    let lists = outcome(api(token, "GET", "/me/playlists?limit=50", None))?;
    Ok(lists["items"].as_array().into_iter().flatten().filter(|list| list["owner"]["id"].as_str() == Some(mine.as_str()) || list["collaborative"].as_bool() == Some(true)).cloned().collect())
}

fn find_list<'a>(lists: &'a [Value], list: &str) -> Option<&'a Value> {
    lists
        .iter()
        .filter_map(|item| Some((similarity(list, item["name"].as_str()?), item)))
        .filter(|(score, _)| *score >= 0.72)
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, item)| item)
}

fn add_to_list(app: &AppHandle, what: &str, list: &str) -> Result<String, String> {
    let token = session(app)?;
    let (uri, name) = if what.is_empty() {
        let (_, uri, name) = playing(&token)?;
        (uri, name)
    } else {
        let (uri, name, _) = find_track(&token, what)?;
        (uri, name)
    };
    let lists = my_lists(&token)?;
    let best = find_list(&lists, list).ok_or_else(|| format!("No encontré ninguna lista tuya llamada «{list}». Dime «crea una lista {list}» y la hago."))?;
    let id = best["id"].as_str().unwrap_or_default();
    let body = json!({ "uris": [uri] }).to_string();
    borrowed_key(app, outcome(renamed(&token, "POST", &format!("/playlists/{id}/items"), &format!("/playlists/{id}/tracks"), Some(&body), None)))?;
    Ok(format!("Agregada «{name}» a tu lista «{}».", best["name"].as_str().unwrap_or(list)))
}

fn list_entries(token: &str, id: &str) -> Vec<(String, String, String)> {
    let mut found = Vec::new();
    let mut path = format!("/playlists/{id}/items?limit=50");
    for _ in 0..10 {
        let page = match api(token, "GET", &path, None) {
            Some((403 | 404 | 405, _)) if path.contains("/items") => api(token, "GET", &path.replace("/items", "/tracks"), None),
            answer => answer,
        };
        let Some((200, page)) = page else { break };
        for entry in page["items"].as_array().into_iter().flatten() {
            let song = if entry["item"].is_object() { &entry["item"] } else { &entry["track"] };
            if let (Some(uri), Some(name)) = (song["uri"].as_str(), song["name"].as_str()) {
                let artist = song["artists"][0]["name"].as_str().unwrap_or_default();
                found.push((uri.to_string(), name.to_string(), artist.to_string()));
            }
        }
        match page["next"].as_str().and_then(|next| next.split_once("/v1")) {
            Some((_, next)) => path = next.to_string(),
            None => break,
        }
    }
    found
}

fn remove_from_list(app: &AppHandle, what: &str, list: &str) -> Result<String, String> {
    let token = session(app)?;
    let lists = my_lists(&token)?;
    let best = find_list(&lists, list).ok_or_else(|| format!("No encontré ninguna lista tuya llamada «{list}»."))?;
    let id = best["id"].as_str().unwrap_or_default();
    let list_name = best["name"].as_str().unwrap_or(list);
    let entries = list_entries(&token, id);
    let (uri, name) = if what.is_empty() {
        let (_, uri, name) = playing(&token)?;
        (uri, name)
    } else {
        entries
            .iter()
            .map(|(uri, name, artist)| (similarity(what, name).max(covers(what, &format!("{name} {artist}"))), uri, name))
            .filter(|(score, _, _)| *score >= 0.6)
            .max_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, uri, name)| (uri.clone(), name.clone()))
            .ok_or_else(|| format!("«{what}» no está en tu lista «{list_name}»."))?
    };
    if !entries.iter().any(|(there, _, _)| *there == uri) {
        return Err(format!("«{name}» no está en tu lista «{list_name}»."));
    }
    let new = json!({ "items": [{ "uri": uri }] }).to_string();
    let old = json!({ "tracks": [{ "uri": uri }] }).to_string();
    borrowed_key(app, outcome(renamed(&token, "DELETE", &format!("/playlists/{id}/items"), &format!("/playlists/{id}/tracks"), Some(&new), Some(&old))))?;
    Ok(format!("Quitada «{name}» de tu lista «{list_name}»."))
}

fn create_list(app: &AppHandle, name: &str) -> Result<String, String> {
    let token = session(app)?;
    if let Some(there) = my_lists(&token)?.iter().find(|list| list["name"].as_str().is_some_and(|known| crate::intent::normalize(known) == crate::intent::normalize(name))) {
        return Ok(format!("Ya tienes una lista llamada «{}».", there["name"].as_str().unwrap_or(name)));
    }
    let body = json!({ "name": name, "public": false }).to_string();
    let made = match api(&token, "POST", "/me/playlists", Some(&body)) {
        Some((403 | 404 | 405, _)) | None => {
            let me = outcome(api(&token, "GET", "/me", None))?;
            let id = me["id"].as_str().ok_or("Spotify no dijo quién eres.")?;
            api(&token, "POST", &format!("/users/{id}/playlists"), Some(&body))
        }
        answer => answer,
    };
    let made = borrowed_key(app, outcome(made))?;
    Ok(format!("Lista «{}» creada.", made["name"].as_str().unwrap_or(name)))
}

fn as_said(spoken: &str, tail: &str) -> String {
    let words: Vec<&str> = spoken.split_whitespace().collect();
    let count = tail.split(' ').count().min(words.len());
    let said = words[words.len() - count..].join(" ");
    let said = said.trim_matches(|c: char| ".,;:!?¡¿\"'«»“”".contains(c)).trim().to_string();
    if crate::intent::normalize(&said) == tail { said } else { tail.to_string() }
}

const ADD_VERBS: &[&str] = &["agrega", "agregala", "anade", "anadela", "añade", "añadela", "mete", "metela", "guarda", "guardala", "suma", "add", "adiciona", "salva"];
const REMOVE_VERBS: &[&str] = &["quita", "quitala", "saca", "sacala", "borra", "borrala", "elimina", "eliminala", "remove", "delete", "tira", "remova"];

fn list_order(spoken: &str, marks: &[&str], verbs: &[&str]) -> Option<(String, String)> {
    let clean = crate::intent::normalize(spoken);
    let (at, mark) = marks.iter().filter_map(|mark| clean.find(mark).map(|at| (at, *mark))).min_by_key(|(at, _)| *at)?;
    let before = &clean[..at];
    let list = clean[at + mark.len()..].trim().trim_start_matches("llamada ").trim_start_matches("que se llama ").trim().to_string();
    let verb = verbs.iter().filter(|verb| before.starts_with(**verb)).max_by_key(|verb| verb.len())?;
    let what = before[verb.len()..].trim();
    const CURRENT: [&str; 14] = ["", "la", "esta", "esta cancion", "la cancion", "esta rola", "este tema", "lo que suena", "la que suena", "esta musica", "this", "this song", "essa", "essa musica"];
    let what = if CURRENT.contains(&what) { String::new() } else { what.to_string() };
    (!list.is_empty()).then_some((what, list))
}

pub fn playlist_order(spoken: &str) -> Option<(String, String)> {
    const MARKS: [&str; 10] = [" a la lista ", " a mi lista ", " en la lista ", " en mi lista ", " a la playlist ", " a mi playlist ", " to my playlist ", " to the playlist ", " na lista ", " na minha playlist "];
    list_order(spoken, &MARKS, ADD_VERBS)
}

pub fn unlist_order(spoken: &str) -> Option<(String, String)> {
    const MARKS: [&str; 8] = [" de la lista ", " de mi lista ", " de la playlist ", " de mi playlist ", " from my playlist ", " from the playlist ", " da lista ", " da minha playlist "];
    list_order(spoken, &MARKS, REMOVE_VERBS)
}

fn player(app: &AppHandle, path: &str, done: &str) -> Result<String, String> {
    outcome(api(&session(app)?, "PUT", path, None)).map(|_| done.to_string())
}

pub fn spoken(app: &AppHandle, spoken: &str) -> Option<Result<String, String>> {
    let clean = crate::intent::normalize(spoken);
    let has = |options: &[&str]| options.iter().any(|option| clean.contains(option));
    let after = |leads: &[&str]| leads.iter().find(|lead| clean.starts_with(**lead)).map(|lead| clean[lead.len()..].trim().to_string());
    let off = has(&["quita", "desactiva", "apaga", "deja de", "sin ", "no repitas", " off", "tira ", "desliga"]);

    if has(&["que suena", "que esta sonando", "que cancion es", "como se llama esta cancion", "como se llama la cancion", "what s playing", "what song is this", "que musica e essa", "o que esta tocando"]) {
        let now = tauri::async_runtime::block_on(media_now(None)).filter(|now| !now.title.is_empty());
        let playing = match now {
            Some(now) if now.artist.is_empty() => Ok(format!("Suena «{}».", now.title)),
            Some(now) => Ok(format!("Suena «{}» de {}.", now.title, now.artist)),
            None => Err("Ahora mismo no suena nada.".to_string()),
        };
        if let Ok(said) = &playing {
            crate::cue::answer(&crate::i18n::tr(said));
        }
        return Some(playing);
    }
    if let Some(order) = seek_order(spoken) {
        return Some(seek(app, order));
    }
    if let Some(shuffle) = liked_order(&clean) {
        return Some(play_liked(app, shuffle));
    }
    let queued = after(&["pon en cola ", "pon en la cola ", "agrega a la cola ", "anade a la cola ", "añade a la cola ", "mete en la cola ", "encola ", "pon despues ", "despues pon ", "luego pon ", "a continuacion pon ", "queue ", "add to the queue ", "coloca na fila "]).or_else(|| {
        let end = [" a la cola", " en la cola", " en cola", " to the queue", " na fila"].iter().find(|end| clean.ends_with(**end))?;
        let middle = &clean[..clean.len() - end.len()];
        ["agrega ", "anade ", "añade ", "pon ", "mete ", "suma ", "add ", "coloca "].iter().find(|verb| middle.starts_with(**verb)).map(|verb| middle[verb.len()..].trim().to_string())
    });
    if let Some(query) = queued.filter(|query| !query.is_empty()) {
        return Some(queue(app, &query));
    }
    if let Some((what, list)) = unlist_order(spoken) {
        return Some(remove_from_list(app, &what, &list));
    }
    if let Some((what, list)) = playlist_order(spoken) {
        return Some(add_to_list(app, &what, &list));
    }
    if let Some(name) = after(&["crea una lista llamada ", "crea una lista ", "crea una playlist llamada ", "crea una playlist ", "haz una lista llamada ", "haz una lista ", "create a playlist called ", "create a playlist ", "cria uma playlist "]).filter(|name| !name.is_empty()) {
        return Some(create_list(app, &as_said(spoken, &name)));
    }
    match like_order(&clean) {
        Some(Like::Save(None)) => return Some(like(app)),
        Some(Like::Save(Some(what))) => return Some(like_named(app, &what)),
        Some(Like::Drop) => return Some(unlike(app)),
        Some(Like::Ask) => return Some(is_liked(app)),
        None => {}
    }
    if has(&["modo aleatorio", "el aleatorio", "shuffle", "reproduccion aleatoria"]) && !clean.starts_with("pon algo") {
        return Some(if off { player(app, "/me/player/shuffle?state=false", "Aleatorio desactivado.") } else { player(app, "/me/player/shuffle?state=true", "Aleatorio activado.") });
    }
    if has(&["en bucle", "repite siempre", "repetir siempre", "sin parar", "deja de repetir", "no repitas", "quita el bucle", "quita la repeticion", "repeat on", "repeat off", "on repeat"]) {
        let all = has(&["la lista", "el album", "todo", "playlist"]);
        return Some(match (off, all) {
            (true, _) => player(app, "/me/player/repeat?state=off", "Ya no se repite."),
            (false, true) => player(app, "/me/player/repeat?state=context", "Repitiendo la lista."),
            (false, false) => player(app, "/me/player/repeat?state=track", "Repitiendo esta canción."),
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playlist_orders_are_understood() {
        assert_eq!(playlist_order("Agrega esta canción a mi lista Gym"), Some((String::new(), "gym".into())));
        assert_eq!(playlist_order("añádela a la lista para el carro"), Some((String::new(), "para el carro".into())));
        assert_eq!(playlist_order("mete despacito en la lista fiesta"), Some(("despacito".into(), "fiesta".into())));
        assert_eq!(playlist_order("add this song to my playlist chill"), Some((String::new(), "chill".into())));
        assert_eq!(playlist_order("pon salsa"), None);
    }

    #[test]
    fn removing_from_a_playlist_is_understood() {
        assert_eq!(unlist_order("Quita esta canción de mi lista Gym"), Some((String::new(), "gym".into())));
        assert_eq!(unlist_order("saca despacito de la lista fiesta"), Some(("despacito".into(), "fiesta".into())));
        assert_eq!(unlist_order("remove this song from my playlist chill"), Some((String::new(), "chill".into())));
        assert_eq!(unlist_order("quita esta de me gusta"), None);
        assert_eq!(playlist_order("Quita esta canción de mi lista Gym"), None);
    }

    #[test]
    fn seeking_is_understood() {
        assert_eq!(seek_order("adelanta 30 segundos"), Some(Seek::By(30)));
        assert_eq!(seek_order("Adelántale treinta segundos"), Some(Seek::By(30)));
        assert_eq!(seek_order("atrasa diez segundos"), Some(Seek::By(-10)));
        assert_eq!(seek_order("regresa un minuto"), Some(Seek::By(-60)));
        assert_eq!(seek_order("adelanta un minuto y medio"), Some(Seek::By(90)));
        assert_eq!(seek_order("atrasa medio minuto"), Some(Seek::By(-30)));
        assert_eq!(seek_order("adelanta cuarenta y cinco segundos"), Some(Seek::By(45)));
        assert_eq!(seek_order("oye cocuyo rebobina 15 segundos"), Some(Seek::By(-15)));
        assert_eq!(seek_order("skip ahead 30 seconds"), Some(Seek::By(30)));
        assert_eq!(seek_order("go back 10 seconds"), Some(Seek::By(-10)));
        assert_eq!(seek_order("volta 10 segundos"), Some(Seek::By(-10)));
        assert_eq!(seek_order("ve al minuto 2"), Some(Seek::To(120)));
        assert_eq!(seek_order("salta al minuto 1 con 30"), Some(Seek::To(90)));
        assert_eq!(seek_order("pon la canción en el minuto 3"), Some(Seek::To(180)));
        assert_eq!(seek_order("regresa al minuto uno"), Some(Seek::To(60)));
        assert_eq!(seek_order("ve al 1:30"), Some(Seek::To(90)));
        assert_eq!(seek_order("go to minute 2"), Some(Seek::To(120)));
        assert_eq!(seek_order("ve a los 45 segundos"), Some(Seek::To(45)));
        assert_eq!(seek_order("adelanta"), None);
        assert_eq!(seek_order("regresa"), None);
        assert_eq!(seek_order("salta esta canción"), None);
        assert_eq!(seek_order("pon el volumen al 30"), None);
        assert_eq!(seek_order("pon a los Beatles"), None);
        assert_eq!(seek_order("recuérdame en 5 minutos que vuelva"), None);
        assert_eq!(seek_order("pon un temporizador de 5 minutos"), None);
    }

    #[test]
    fn liked_songs_are_understood() {
        let clean = |text: &str| crate::intent::normalize(text);
        assert_eq!(liked_order(&clean("Pon mis Me gusta")), Some(false));
        assert_eq!(liked_order(&clean("ponme mis canciones favoritas en aleatorio")), Some(true));
        assert_eq!(liked_order(&clean("reproduce mis me gusta en Spotify")), Some(false));
        assert_eq!(liked_order(&clean("play my liked songs")), Some(false));
        assert_eq!(liked_order(&clean("toca minhas curtidas")), Some(false));
        assert_eq!(liked_order(&clean("pon salsa")), None);
        assert_eq!(liked_order(&clean("ponle me gusta a esta")), None);
        assert_eq!(liked_order(&clean("pon me gusta")), None);
    }

    #[test]
    fn liking_never_plays_a_song_called_me_gusta() {
        let like = |text: &str| like_order(&crate::intent::normalize(text));
        for phrase in [
            "dale me gusta", "Dale me gusta a esta canción", "ponle me gusta", "pon me gusta", "pon un me gusta a esta", "Guarda esta canción",
            "guarda la canción", "guárdala", "guárdamela", "guárdala en mis me gusta", "agrégala a me gusta", "agrega esta canción a mis favoritos",
            "mete esta canción en me gusta", "me gusta esta canción", "dale like", "dale corazón", "like this song", "save this song", "curti essa",
            "oye cocuyo dale me gusta porfa", "guarda esta en me gusta", "dale me gusta a lo que suena",
        ] {
            assert_eq!(like(phrase), Some(Like::Save(None)), "{phrase}");
        }
        assert_eq!(like("agrega despacito a mis me gusta"), Some(Like::Save(Some("despacito".into()))));
        assert_eq!(like("quítala de me gusta"), Some(Like::Drop));
        assert_eq!(like("quita el like"), Some(Like::Drop));
        assert_eq!(like("¿esta canción está en mis me gusta?"), Some(Like::Ask));
        assert_eq!(like("ya le di me gusta a esta?"), Some(Like::Ask));
        for phrase in [
            "pon me gusta de silvestre dangond", "pon la canción me gusta", "dale play a like a prayer", "guarda la nota comprar pan", "pon mis me gusta",
            "agrega esta canción a mi lista gym", "pon salsa", "pon la", "pon me gusta me gusta",
        ] {
            assert_eq!(like(phrase), None, "{phrase}");
        }
    }

    #[test]
    fn a_new_list_keeps_how_you_said_it() {
        assert_eq!(as_said("crea una lista Cocuyo prueba", "cocuyo prueba"), "Cocuyo prueba");
        assert_eq!(as_said("Crea una lista llamada Para el Carro.", "para el carro"), "Para el Carro");
        assert_eq!(as_said("crea una playlist Música Ñoña", "musica ñoña"), "Música Ñoña");
    }
}
