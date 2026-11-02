// avisos al celular aunque tenga cocuyo cerrado: web push cifrado (rfc 8291) y firmado con vapid (rfc 8292), sin servidor propio
// el aviso pasa por el servicio de avisos del navegador del celular (en android, el de google), que solo ve un paquete cifrado

use std::io::Write;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, Manager};

use crate::apps::CREATE_NO_WINDOW;
use crate::candado::{self, Pair};
use crate::remoto::{b64, unb64};

const WHO: &str = "https://github.com/Tkyoxx/Cocuyo";

#[derive(Clone, Serialize, Deserialize)]
struct Subscription {
    id: String,
    endpoint: String,
    p256dh: String,
    auth: String,
}

#[derive(Default, Serialize, Deserialize)]
struct Book {
    #[serde(default)]
    vapid: String,
    #[serde(default)]
    celulares: Vec<Subscription>,
}

fn path(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_config_dir().ok()?.join("avisos.json"))
}

fn load(app: &AppHandle) -> Book {
    path(app).and_then(|path| std::fs::read(path).ok()).and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
}

fn save(app: &AppHandle, book: &Book) -> Result<(), String> {
    let path = path(app).ok_or("No se encontró la carpeta de configuración.")?;
    std::fs::write(path, serde_json::to_vec_pretty(book).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

fn vapid(app: &AppHandle) -> Result<Pair, String> {
    let mut book = load(app);
    if let Some(pair) = unb64(&book.vapid).and_then(|sealed| crate::spotify::protect(&sealed, true)).and_then(|secret| Pair::from_secret(&secret).ok()) {
        return Ok(pair);
    }
    let pair = Pair::generate()?;
    let sealed = crate::spotify::protect(pair.secret(), false).ok_or("Windows no pudo cifrar la llave de los avisos.")?;
    book.vapid = b64(&sealed);
    save(app, &book)?;
    Ok(pair)
}

pub fn public_key(app: &AppHandle) -> Option<String> {
    vapid(app).ok().map(|pair| b64(&pair.public()))
}

pub fn subscribe(app: &AppHandle, id: &str, endpoint: &str, p256dh: &str, auth: &str) -> Result<(), String> {
    let fine = endpoint.starts_with("https://")
        && endpoint.len() < 2000
        && unb64(p256dh).is_some_and(|key| key.len() == candado::PUBLIC_LEN && key[0] == 4)
        && unb64(auth).is_some_and(|secret| secret.len() == 16);
    if !fine {
        return Err("El celular mandó un aviso mal formado.".into());
    }
    let mut book = load(app);
    book.celulares.retain(|known| known.id != id && known.endpoint != endpoint);
    book.celulares.push(Subscription { id: id.into(), endpoint: endpoint.into(), p256dh: p256dh.into(), auth: auth.into() });
    save(app, &book)
}

pub fn forget(app: &AppHandle, id: &str) {
    let mut book = load(app);
    let before = book.celulares.len();
    book.celulares.retain(|known| known.id != id);
    if book.celulares.len() != before {
        let _ = save(app, &book);
    }
}

pub fn encrypt_with(mine: &Pair, salt: &[u8; 16], ua_public: &[u8], auth: &[u8], payload: &[u8]) -> Result<Vec<u8>, String> {
    let shared = mine.agree(ua_public)?;
    let mut info = b"WebPush: info\0".to_vec();
    info.extend_from_slice(ua_public);
    info.extend_from_slice(&mine.public());
    let ikm = candado::hkdf(&shared, auth, &info, 32);
    let key = candado::hkdf(&ikm, salt, b"Content-Encoding: aes128gcm\0", 16);
    let nonce: [u8; 12] = candado::hkdf(&ikm, salt, b"Content-Encoding: nonce\0", 12).try_into().map_err(|_| "nonce raro")?;
    let mut plain = payload.to_vec();
    plain.push(2);
    let sealed = candado::seal(&key, &nonce, b"", &plain)?;
    let mut body = salt.to_vec();
    body.extend_from_slice(&4096u32.to_be_bytes());
    body.push(candado::PUBLIC_LEN as u8);
    body.extend_from_slice(&mine.public());
    body.extend_from_slice(&sealed);
    Ok(body)
}

fn encrypt(ua_public: &[u8], auth: &[u8], payload: &[u8]) -> Result<Vec<u8>, String> {
    let salt: [u8; 16] = candado::random(16).try_into().map_err(|_| "sal rara")?;
    encrypt_with(&Pair::generate()?, &salt, ua_public, auth, payload)
}

fn token(pair: &Pair, endpoint: &str, now: i64) -> Result<String, String> {
    let audience = endpoint.splitn(4, '/').take(3).collect::<Vec<_>>().join("/");
    let head = b64(br#"{"typ":"JWT","alg":"ES256"}"#);
    let claims = b64(json!({ "aud": audience, "exp": now + 12 * 3600, "sub": WHO }).to_string().as_bytes());
    let signed = format!("{head}.{claims}");
    Ok(format!("{signed}.{}", b64(&pair.sign(signed.as_bytes())?)))
}

fn post(endpoint: &str, authorization: &str, body: &[u8]) -> Result<u16, String> {
    let mut child = Command::new("curl.exe")
        .args(["-s", "-o", "NUL", "-w", "%{http_code}", "--max-time", "20", "-X", "POST"])
        .args(["-H", "TTL: 86400", "-H", "Urgency: high", "-H", "Content-Encoding: aes128gcm", "-H", "Content-Type: application/octet-stream"])
        .arg("-H")
        .arg(format!("Authorization: {authorization}"))
        .args(["--data-binary", "@-"])
        .arg(endpoint)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| e.to_string())?;
    child.stdin.take().ok_or("sin entrada")?.write_all(body).map_err(|e| e.to_string())?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    String::from_utf8_lossy(&out.stdout).trim().parse().map_err(|_| "curl no respondió".to_string())
}

pub fn tell(app: &AppHandle, title: &str, text: &str, tag: &str) {
    let book = load(app);
    if book.celulares.is_empty() || !crate::settings::load(app).remote_on {
        return;
    }
    let Ok(pair) = vapid(app) else { return };
    let payload = json!({ "titulo": crate::i18n::tr(title), "texto": crate::i18n::tr(text).chars().take(300).collect::<String>(), "etiqueta": tag }).to_string();
    let app = app.clone();
    std::thread::spawn(move || {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
        let mut gone = Vec::new();
        for phone in &book.celulares {
            let (Some(p256dh), Some(auth)) = (unb64(&phone.p256dh), unb64(&phone.auth)) else { continue };
            let (Ok(body), Ok(token)) = (encrypt(&p256dh, &auth, payload.as_bytes()), token(&pair, &phone.endpoint, now)) else { continue };
            let status = post(&phone.endpoint, &format!("vapid t={token}, k={}", b64(&pair.public())), &body);
            if cfg!(debug_assertions) {
                eprintln!("[avisos] {status:?}");
            }
            if matches!(status, Ok(404 | 410)) {
                gone.push(phone.endpoint.clone());
            }
        }
        if !gone.is_empty() {
            let mut book = load(&app);
            book.celulares.retain(|phone| !gone.contains(&phone.endpoint));
            let _ = save(&app, &book);
        }
    });
}

pub fn away() -> bool {
    use windows::Win32::System::SystemInformation::GetTickCount;
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    let mut last = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    let idle = unsafe { GetLastInputInfo(&mut last) }.as_bool().then(|| unsafe { GetTickCount() }.wrapping_sub(last.dwTime));
    idle.is_some_and(|ms| ms > 3 * 60 * 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_example_of_rfc_8291() {
        let part = |text: &str| unb64(text).unwrap();
        let sender = candado::pair_from_parts(
            &part("BP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8"),
            &part("yfWPiYE-n46HLnH0KqZOF1fJJU3MYrct3AELtAQ-oRw"),
        )
        .unwrap();
        let salt: [u8; 16] = part("DGv6ra1nlYgDCS1FRnbzlw").try_into().unwrap();
        let body = encrypt_with(
            &sender,
            &salt,
            &part("BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4"),
            &part("BTBZMqHH6r4Tts7J_aSIgg"),
            b"When I grow up, I want to be a watermelon",
        )
        .unwrap();
        assert_eq!(
            b64(&body),
            "DGv6ra1nlYgDCS1FRnbzlwAAEABBBP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A_yl95bQpu6cVPTpK4Mqgkf1CXztLVBSt2Ks3oZwbuwXPXLWyouBWLVWGNWQexSgSxsj_Qulcy4a-fN"
        );
    }

    #[test]
    fn the_vapid_token_is_signed_for_the_push_service() {
        let pair = Pair::generate().unwrap();
        let token = token(&pair, "https://fcm.googleapis.com/fcm/send/abc:123", 1_800_000_000).unwrap();
        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3);
        let claims: serde_json::Value = serde_json::from_slice(&unb64(parts[1]).unwrap()).unwrap();
        assert_eq!(claims["aud"], "https://fcm.googleapis.com");
        assert_eq!(claims["exp"], 1_800_000_000 + 12 * 3600);
        assert!(candado::verify(&pair.public(), format!("{}.{}", parts[0], parts[1]).as_bytes(), &unb64(parts[2]).unwrap()));
        assert!(!candado::verify(&pair.public(), b"otra cosa", &unb64(parts[2]).unwrap()));
    }
}
