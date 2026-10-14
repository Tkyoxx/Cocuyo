// el login de spotify. la sesion queda cifrada en el equipo, tranquilos

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};
use windows::Win32::Foundation::{LocalFree, HLOCAL};
use windows::Win32::Security::Cryptography::{
    BCryptGenRandom, CryptProtectData, CryptUnprotectData, BCRYPT_USE_SYSTEM_PREFERRED_RNG,
    CRYPT_INTEGER_BLOB,
};

use crate::apps::{self, CREATE_NO_WINDOW};
use crate::system::base64;
use crate::{notes, overlay, settings};

const REDIRECT_PORT: u16 = 5173;
const REDIRECT_URI: &str = "http://127.0.0.1:5173/";
const SCOPES: &str = "user-read-playback-state user-modify-playback-state user-read-currently-playing playlist-read-private playlist-read-collaborative user-library-modify";
const WAIT_FOR_USER: Duration = Duration::from_secs(180);

struct Access {
    token: String,
    expires: Instant,
}

static ACCESS: Mutex<Option<Access>> = Mutex::new(None);
static CONNECTING: AtomicBool = AtomicBool::new(false);

fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98,
        0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786,
        0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8,
        0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
        0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819,
        0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a,
        0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] =
        [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let mut message = data.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());

    for block in message.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, word) in block.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7].wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [t1.wrapping_add(t2), v[0], v[1], v[2], v[3].wrapping_add(t1), v[4], v[5], v[6]];
        }
        for (state, value) in h.iter_mut().zip(v) {
            *state = state.wrapping_add(value);
        }
    }
    let mut out = [0u8; 32];
    for (chunk, word) in out.chunks_exact_mut(4).zip(h) {
        chunk.copy_from_slice(&word.to_be_bytes());
    }
    out
}

fn base64_url(bytes: &[u8]) -> String {
    base64(bytes).replace('+', "-").replace('/', "_").trim_end_matches('=').to_string()
}

fn challenge(verifier: &str) -> String {
    base64_url(&sha256(verifier.as_bytes()))
}

fn random_text(bytes: usize) -> String {
    let mut buffer = vec![0u8; bytes];
    unsafe {
        let _ = BCryptGenRandom(None, &mut buffer, BCRYPT_USE_SYSTEM_PREFERRED_RNG);
    }
    base64_url(&buffer)
}

fn url_encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn url_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok());
                match hex {
                    Some(byte) => {
                        out.push(byte);
                        i += 3;
                    }
                    None => {
                        out.push(b'%');
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn query_param(request_line: &str, name: &str) -> Option<String> {
    let query = request_line.split_whitespace().nth(1)?.split_once('?')?.1;
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == name).then(|| url_decode(value))
    })
}

pub(crate) fn protect(data: &[u8], unprotect: bool) -> Option<Vec<u8>> {
    let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        if unprotect {
            CryptUnprotectData(&input, None, None, None, None, 0, &mut output).ok()?;
        } else {
            CryptProtectData(&input, None, None, None, None, 0, &mut output).ok()?;
        }
        let bytes = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(output.pbData.cast())));
        Some(bytes)
    }
}

fn session_file(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_config_dir().ok()?.join("spotify.bin"))
}

fn load_refresh(app: &AppHandle) -> Option<String> {
    let sealed = std::fs::read(session_file(app)?).ok()?;
    String::from_utf8(protect(&sealed, true)?).ok()
}

fn save_refresh(app: &AppHandle, token: &str) -> Result<(), String> {
    let path = session_file(app).ok_or("No se encontró la carpeta de configuración.")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let sealed = protect(token.as_bytes(), false).ok_or("Windows no pudo cifrar la sesión.")?;
    std::fs::write(path, sealed).map_err(|e| e.to_string())
}

fn client_id_from_mis_notas() -> Option<String> {
    let file = PathBuf::from(std::env::var_os("APPDATA")?).join("Mis Notas").join("datos.json");
    let data: Value = serde_json::from_slice(&std::fs::read(file).ok()?).ok()?;
    let id = data.get("data")?.get("sp.clientId")?.as_str()?.trim().to_string();
    (!id.is_empty()).then_some(id)
}

fn client_id(app: &AppHandle) -> Option<String> {
    settings::load(app).spotify_client_id.filter(|id| !id.trim().is_empty()).or_else(client_id_from_mis_notas)
}

fn token_request(fields: &[(&str, &str)]) -> Result<Value, String> {
    let quote = |text: &str| text.replace('\\', "\\\\").replace('"', "\\\"");
    let mut config = String::from("url = \"https://accounts.spotify.com/api/token\"\n");
    for (name, value) in fields {
        config += &format!("data-urlencode = \"{}={}\"\n", name, quote(value));
    }
    let mut child = Command::new("curl.exe")
        .args(["--silent", "--max-time", "12", "--config", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| e.to_string())?;
    child.stdin.take().ok_or("curl no aceptó datos")?.write_all(config.as_bytes()).map_err(|e| e.to_string())?;
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    let json: Value = serde_json::from_slice(&output.stdout).map_err(|_| "Spotify no respondió. Revisa tu conexión.".to_string())?;
    match json.get("error") {
        Some(_) => Err(json["error_description"].as_str().or(json["error"].as_str()).unwrap_or("Spotify rechazó la sesión.").to_string()),
        None => Ok(json),
    }
}

fn remember(app: &AppHandle, response: &Value) -> Result<String, String> {
    let token = response["access_token"].as_str().ok_or("Spotify no devolvió un permiso.")?.to_string();
    let lifetime = response["expires_in"].as_u64().unwrap_or(3600).saturating_sub(120);
    *ACCESS.lock().unwrap() = Some(Access { token: token.clone(), expires: Instant::now() + Duration::from_secs(lifetime) });
    if let Some(refresh) = response["refresh_token"].as_str() {
        save_refresh(app, refresh)?;
    }
    Ok(token)
}

#[derive(Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    Cocuyo,
    MisNotas,
}

pub fn token(app: &AppHandle) -> Option<(String, Source)> {
    if let Some(access) = ACCESS.lock().unwrap().as_ref() {
        if Instant::now() < access.expires {
            return Some((access.token.clone(), Source::Cocuyo));
        }
    }
    if let (Some(refresh), Some(id)) = (load_refresh(app), client_id(app)) {
        match token_request(&[("grant_type", "refresh_token"), ("refresh_token", &refresh), ("client_id", &id)]) {
            Ok(response) => {
                if let Ok(token) = remember(app, &response) {
                    return Some((token, Source::Cocuyo));
                }
            }
            Err(why) if why.to_lowercase().contains("revoked") || why.to_lowercase().contains("invalid") => {
                if let Some(file) = session_file(app) {
                    let _ = std::fs::remove_file(file);
                }
            }
            Err(_) => {}
        }
    }
    let port = notes::find_port()?;
    let body = notes::request(port, "GET", "/api/spotify/token", None)?;
    let borrowed = serde_json::from_str::<Value>(&body).ok()?.get("access_token")?.as_str()?.to_string();
    (!borrowed.is_empty()).then_some((borrowed, Source::MisNotas))
}

const DONE_PAGE: &str = "<!doctype html><meta charset=utf-8><title>Cocuyo</title><body style=\"margin:0;display:grid;place-items:center;height:100vh;background:#000;color:#f4f5f8;font:500 18px Segoe UI,system-ui,sans-serif\"><p>Spotify quedó conectado con Cocuyo. Ya puedes cerrar esta pestaña.</p>";

fn wait_on_listener(listener: TcpListener, state: &str, timeout: Duration) -> Result<String, String> {
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let Ok((mut stream, _)) = listener.accept() else {
            std::thread::sleep(Duration::from_millis(150));
            continue;
        };
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
        let mut buffer = [0u8; 4096];
        let read = stream.read(&mut buffer).unwrap_or(0);
        let request = String::from_utf8_lossy(&buffer[..read]);
        let line = request.lines().next().unwrap_or_default();

        let answer = |stream: &mut std::net::TcpStream, body: &str| {
            let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        };
        if query_param(line, "state").as_deref() != Some(state) {
            answer(&mut stream, "");
            continue;
        }
        if let Some(error) = query_param(line, "error") {
            answer(&mut stream, "<meta charset=utf-8>No se dio permiso. Puedes cerrar esta pestaña.");
            return Err(if error == "access_denied" { "No diste permiso en Spotify.".into() } else { error });
        }
        if let Some(code) = query_param(line, "code") {
            answer(&mut stream, DONE_PAGE);
            return Ok(code);
        }
    }
    Err("Se acabó el tiempo esperando a que aceptaras en el navegador.".into())
}

fn wait_on_mis_notas(port: u16, state: &str, timeout: Duration) -> Result<String, String> {
    let deadline = Instant::now() + timeout;
    let path = format!("/api/auth/poll?state={}", url_encode(state));
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1200));
        let code = notes::request(port, "GET", &path, None)
            .and_then(|body| serde_json::from_str::<Value>(&body).ok())
            .and_then(|json| json["code"].as_str().map(str::to_string));
        if let Some(code) = code {
            return Ok(code);
        }
    }
    Err("Se acabó el tiempo esperando a que aceptaras en el navegador.".into())
}

fn connect(app: &AppHandle) -> Result<(), String> {
    let id = client_id(app).ok_or("Falta el Client ID de Spotify. Pégalo en Ajustes.")?;
    let verifier = random_text(64);
    let state = random_text(16);
    let authorize = format!(
        "https://accounts.spotify.com/authorize?response_type=code&client_id={}&scope={}&redirect_uri={}&code_challenge_method=S256&code_challenge={}&state={}",
        url_encode(&id),
        url_encode(SCOPES),
        url_encode(REDIRECT_URI),
        challenge(&verifier),
        url_encode(&state),
    );

    let address = SocketAddr::from(([127, 0, 0, 1], REDIRECT_PORT));
    let code = match TcpListener::bind(address) {
        Ok(listener) => {
            apps::open(&authorize)?;
            wait_on_listener(listener, &state, WAIT_FOR_USER)?
        }
        Err(_) if notes::find_port() == Some(REDIRECT_PORT) => {
            apps::open(&authorize)?;
            wait_on_mis_notas(REDIRECT_PORT, &state, WAIT_FOR_USER)?
        }
        Err(_) => return Err("Otro programa está usando el puerto 5173. Ciérralo e inténtalo otra vez.".into()),
    };

    let response = token_request(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", REDIRECT_URI),
        ("client_id", &id),
        ("code_verifier", &verifier),
    ])?;
    if response["refresh_token"].as_str().is_none() {
        return Err("Spotify no devolvió una sesión que se pueda guardar.".into());
    }
    remember(app, &response).map(|_| ())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    source: Option<Source>,
    has_client_id: bool,
    connecting: bool,
}

#[tauri::command]
pub async fn spotify_status(app: AppHandle) -> Status {
    let own = ACCESS.lock().unwrap().is_some() || load_refresh(&app).is_some();
    let source = if own {
        Some(Source::Cocuyo)
    } else {
        notes::find_port()
            .and_then(|port| notes::request(port, "GET", "/api/spotify/token", None))
            .filter(|body| body.contains("\"access_token\":\""))
            .map(|_| Source::MisNotas)
    };
    Status { source, has_client_id: client_id(&app).is_some(), connecting: CONNECTING.load(Ordering::Relaxed) }
}

#[tauri::command]
pub fn spotify_connect(app: AppHandle) {
    if CONNECTING.swap(true, Ordering::Relaxed) {
        return;
    }
    std::thread::spawn(move || {
        let result = connect(&app);
        CONNECTING.store(false, Ordering::Relaxed);
        let _ = app.emit_to(overlay::LABEL, "spotify", result.err());
    });
}

#[tauri::command]
pub fn spotify_disconnect(app: AppHandle) {
    *ACCESS.lock().unwrap() = None;
    if let Some(file) = session_file(&app) {
        let _ = std::fs::remove_file(file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn sha256_matches_the_standard_vectors() {
        assert_eq!(hex(&sha256(b"abc")), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(hex(&sha256(b"")), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(
            hex(&sha256(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn pkce_challenge_matches_the_rfc_example() {
        assert_eq!(challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn redirect_parameters_are_read_and_decoded() {
        let line = "GET /?code=AQ%2Bx_1&state=abc-123 HTTP/1.1";
        assert_eq!(query_param(line, "code").as_deref(), Some("AQ+x_1"));
        assert_eq!(query_param(line, "state").as_deref(), Some("abc-123"));
        assert_eq!(query_param("GET /favicon.ico HTTP/1.1", "code"), None);
    }

    #[test]
    fn the_listener_returns_the_code_for_the_right_state_only() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let browser = std::thread::spawn(move || {
            for request in ["GET /favicon.ico HTTP/1.1", "GET /?code=robado&state=otro HTTP/1.1", "GET /?code=bueno&state=mio HTTP/1.1"] {
                let mut stream = std::net::TcpStream::connect(address).unwrap();
                write!(stream, "{request}\r\nHost: x\r\n\r\n").unwrap();
                let mut sink = String::new();
                let _ = stream.read_to_string(&mut sink);
            }
        });
        assert_eq!(wait_on_listener(listener, "mio", Duration::from_secs(5)).as_deref(), Ok("bueno"));
        browser.join().unwrap();
    }

    #[test]
    fn the_saved_session_survives_encryption() {
        let sealed = protect(b"permiso-largo", false).expect("cifrar");
        assert_ne!(sealed, b"permiso-largo");
        assert_eq!(protect(&sealed, true).as_deref(), Some(&b"permiso-largo"[..]));
    }

    #[test]
    fn random_text_is_url_safe_and_not_repeated() {
        let (a, b) = (random_text(64), random_text(64));
        assert!(a.len() >= 80 && a.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
        assert_ne!(a, b);
    }
}
