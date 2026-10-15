// spotify. ojo: mandar la cancion sola con uris no hace nada, hay que mandar el album con offset. me costo una tarde entera

use std::io::Write;
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
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
    let seconds = seconds.max(0.0);
    let ticks = (seconds * 10_000_000.0) as i64;
    let local = current_session().and_then(|session| session.TryChangePlaybackPositionAsync(ticks).ok()?.join().ok()).unwrap_or(false);
    local
        || spotify::token(&app)
            .and_then(|(token, _)| api(&token, "PUT", &format!("/me/player/seek?position_ms={}", (seconds * 1000.0) as u64), None))
            .is_some_and(|(status, _)| (200..300).contains(&status))
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
    let same = context.is_none() || player["context"]["uri"].as_str() == context;
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

fn queue(app: &AppHandle, query: &str) -> Result<String, String> {
    let token = session(app)?;
    let found = outcome(api(&token, "GET", &format!("/search?type=track&limit=5&q={}", url_encode(query)), None))?;
    let best = found["tracks"]["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|track| Some((track["uri"].as_str()?, track["name"].as_str()?, track["artists"][0]["name"].as_str().unwrap_or(""))))
        .max_by(|a, b| {
            let score = |(_, name, artist): &(&str, &str, &str)| crate::intent::similarity(query, name).max(crate::intent::covers(query, &format!("{name} {artist}")));
            score(a).total_cmp(&score(b))
        })
        .ok_or_else(|| format!("No encontré «{query}» en Spotify."))?;
    outcome(api(&token, "POST", &format!("/me/player/queue?uri={}", url_encode(best.0)), None))?;
    Ok(format!("En cola: «{}» de {}.", best.1, best.2))
}

fn like(app: &AppHandle) -> Result<String, String> {
    let token = session(app)?;
    let now = outcome(api(&token, "GET", "/me/player/currently-playing", None))?;
    let (Some(id), Some(name)) = (now["item"]["id"].as_str(), now["item"]["name"].as_str()) else {
        return Err("Spotify no tiene nada sonando en ningún dispositivo.".into());
    };
    outcome(api(&token, "PUT", &format!("/me/tracks?ids={id}"), None))?;
    Ok(format!("Guardada en tus Me gusta: «{name}»."))
}

fn unlike(app: &AppHandle) -> Result<String, String> {
    let token = session(app)?;
    let now = outcome(api(&token, "GET", "/me/player/currently-playing", None))?;
    let (Some(id), Some(name)) = (now["item"]["id"].as_str(), now["item"]["name"].as_str()) else {
        return Err("Spotify no tiene nada sonando en ningún dispositivo.".into());
    };
    outcome(api(&token, "DELETE", &format!("/me/tracks?ids={id}"), None))?;
    Ok(format!("Quitada de tus Me gusta: «{name}»."))
}

fn find_track(token: &str, query: &str) -> Result<(String, String), String> {
    let found = outcome(api(token, "GET", &format!("/search?type=track&limit=5&q={}", url_encode(query)), None))?;
    found["tracks"]["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|track| Some((track["uri"].as_str()?, track["name"].as_str()?, track["artists"][0]["name"].as_str().unwrap_or(""))))
        .max_by(|a, b| {
            let score = |(_, name, artist): &(&str, &str, &str)| crate::intent::similarity(query, name).max(crate::intent::covers(query, &format!("{name} {artist}")));
            score(a).total_cmp(&score(b))
        })
        .map(|(uri, name, _)| (uri.to_string(), name.to_string()))
        .ok_or_else(|| format!("No encontré «{query}» en Spotify."))
}

fn my_lists(token: &str) -> Result<Vec<Value>, String> {
    let me = outcome(api(token, "GET", "/me", None))?;
    let mine = me["id"].as_str().unwrap_or_default().to_string();
    let lists = outcome(api(token, "GET", "/me/playlists?limit=50", None))?;
    Ok(lists["items"].as_array().into_iter().flatten().filter(|list| list["owner"]["id"].as_str() == Some(mine.as_str()) || list["collaborative"].as_bool() == Some(true)).cloned().collect())
}

fn add_to_list(app: &AppHandle, what: &str, list: &str) -> Result<String, String> {
    let token = session(app)?;
    let (uri, name) = if what.is_empty() {
        let now = outcome(api(&token, "GET", "/me/player/currently-playing", None))?;
        match (now["item"]["uri"].as_str(), now["item"]["name"].as_str()) {
            (Some(uri), Some(name)) => (uri.to_string(), name.to_string()),
            _ => return Err("Spotify no tiene nada sonando en ningún dispositivo.".into()),
        }
    } else {
        find_track(&token, what)?
    };
    let lists = my_lists(&token)?;
    let best = lists
        .iter()
        .filter_map(|item| Some((similarity(list, item["name"].as_str()?), item)))
        .filter(|(score, _)| *score >= 0.72)
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, item)| item)
        .ok_or_else(|| format!("No encontré ninguna lista tuya llamada «{list}». Dime «crea una lista {list}» y la hago."))?;
    let id = best["id"].as_str().unwrap_or_default();
    outcome(api(&token, "POST", &format!("/playlists/{id}/tracks"), Some(&serde_json::json!({ "uris": [uri] }).to_string())))?;
    Ok(format!("Agregada «{name}» a tu lista «{}».", best["name"].as_str().unwrap_or(list)))
}

fn create_list(app: &AppHandle, name: &str) -> Result<String, String> {
    let token = session(app)?;
    let me = outcome(api(&token, "GET", "/me", None))?;
    let id = me["id"].as_str().ok_or("Spotify no dijo quién eres.")?;
    let made = outcome(api(&token, "POST", &format!("/users/{id}/playlists"), Some(&serde_json::json!({ "name": name, "public": false }).to_string())))?;
    Ok(format!("Lista «{}» creada.", made["name"].as_str().unwrap_or(name)))
}

pub fn playlist_order(spoken: &str) -> Option<(String, String)> {
    let clean = crate::intent::normalize(spoken);
    const MARKS: [&str; 10] = [" a la lista ", " a mi lista ", " en la lista ", " en mi lista ", " a la playlist ", " a mi playlist ", " to my playlist ", " to the playlist ", " na lista ", " na minha playlist "];
    let (at, mark) = MARKS.iter().filter_map(|mark| clean.find(mark).map(|at| (at, *mark))).min_by_key(|(at, _)| *at)?;
    let before = &clean[..at];
    let list = clean[at + mark.len()..].trim().trim_start_matches("llamada ").trim_start_matches("que se llama ").trim().to_string();
    const VERBS: &[&str] = &["agrega", "agregala", "anade", "anadela", "añade", "añadela", "mete", "metela", "guarda", "guardala", "suma", "add", "adiciona", "salva"];
    let verb = VERBS.iter().filter(|verb| before.starts_with(**verb)).max_by_key(|verb| verb.len())?;
    let what = before[verb.len()..].trim();
    const CURRENT: [&str; 14] = ["", "la", "esta", "esta cancion", "la cancion", "esta rola", "este tema", "lo que suena", "la que suena", "esta musica", "this", "this song", "essa", "essa musica"];
    let what = if CURRENT.contains(&what) { String::new() } else { what.to_string() };
    (!list.is_empty()).then_some((what, list))
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
    let queued = after(&["pon en cola ", "pon en la cola ", "agrega a la cola ", "anade a la cola ", "añade a la cola ", "mete en la cola ", "encola ", "pon despues ", "despues pon ", "luego pon ", "a continuacion pon ", "queue ", "add to the queue ", "coloca na fila "]).or_else(|| {
        let end = [" a la cola", " en la cola", " en cola", " to the queue", " na fila"].iter().find(|end| clean.ends_with(**end))?;
        let middle = &clean[..clean.len() - end.len()];
        ["agrega ", "anade ", "añade ", "pon ", "mete ", "suma ", "add ", "coloca "].iter().find(|verb| middle.starts_with(**verb)).map(|verb| middle[verb.len()..].trim().to_string())
    });
    if let Some(query) = queued.filter(|query| !query.is_empty()) {
        return Some(queue(app, &query));
    }
    if let Some((what, list)) = playlist_order(spoken) {
        return Some(add_to_list(app, &what, &list));
    }
    if let Some(name) = after(&["crea una lista llamada ", "crea una lista ", "crea una playlist llamada ", "crea una playlist ", "haz una lista llamada ", "haz una lista ", "create a playlist called ", "create a playlist ", "cria uma playlist "]).filter(|name| !name.is_empty()) {
        return Some(create_list(app, &name));
    }
    if has(&["quita esta de me gusta", "quitala de me gusta", "quita de mis me gusta", "ya no me gusta esta", "quita el me gusta", "unlike this", "descurti"]) {
        return Some(unlike(app));
    }
    if has(&["me gusta esta", "guarda esta cancion", "a mis me gusta", "dale like", "dale me gusta", "like this song", "curti essa"]) {
        return Some(like(app));
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
}
