// discord desde cocuyo: el rpc local de la app de discord (la tuberia discord-ipc). cada quien usa su propia app del portal de desarrolladores

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;

use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

const REDIRECT: &str = "http://127.0.0.1";

struct Pipe(File);

impl Pipe {
    fn open() -> Result<Pipe, String> {
        (0..10)
            .find_map(|i| OpenOptions::new().read(true).write(true).open(format!(r"\\.\pipe\discord-ipc-{i}")).ok())
            .map(Pipe)
            .ok_or_else(|| "Discord no está abierto.".to_string())
    }

    fn send(&mut self, op: u32, body: &Value) -> Result<(), String> {
        let text = body.to_string();
        let mut frame = Vec::with_capacity(8 + text.len());
        frame.extend(op.to_le_bytes());
        frame.extend((text.len() as u32).to_le_bytes());
        frame.extend(text.as_bytes());
        self.0.write_all(&frame).map_err(|e| format!("Discord: {e}"))
    }

    fn receive(&mut self) -> Result<(u32, Value), String> {
        let mut head = [0u8; 8];
        self.0.read_exact(&mut head).map_err(|e| format!("Discord: {e}"))?;
        let op = u32::from_le_bytes([head[0], head[1], head[2], head[3]]);
        let length = u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as usize;
        let mut body = vec![0u8; length.min(1 << 20)];
        self.0.read_exact(&mut body).map_err(|e| format!("Discord: {e}"))?;
        Ok((op, serde_json::from_slice(&body).unwrap_or(Value::Null)))
    }

    fn command(&mut self, cmd: &str, args: Value) -> Result<Value, String> {
        let nonce = format!("{:x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
        self.send(1, &json!({ "cmd": cmd, "args": args, "nonce": nonce }))?;
        loop {
            let (op, message) = self.receive()?;
            if op == 2 {
                return Err(format!("Discord cerró la conexión: {}", message["message"].as_str().unwrap_or("sin motivo")));
            }
            if message["nonce"].as_str() == Some(nonce.as_str()) {
                if message["evt"].as_str() == Some("ERROR") {
                    return Err(format!("Discord: {}", message["data"]["message"].as_str().unwrap_or("error")));
                }
                return Ok(message["data"].clone());
            }
        }
    }
}

fn vault(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_config_dir().ok()?.join("discord.bin"))
}

fn load(app: &AppHandle) -> Value {
    vault(app)
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|sealed| crate::spotify::protect(&sealed, true))
        .and_then(|plain| serde_json::from_slice(&plain).ok())
        .unwrap_or(Value::Null)
}

fn keep(app: &AppHandle, data: &Value) -> Result<(), String> {
    let path = vault(app).ok_or("No encontré la carpeta de configuración.")?;
    let sealed = crate::spotify::protect(data.to_string().as_bytes(), false).ok_or("Windows no dejó cifrar la llave de Discord.")?;
    std::fs::write(path, sealed).map_err(|e| e.to_string())
}

fn token_request(fields: &[(&str, &str)]) -> Result<Value, String> {
    use std::os::windows::process::CommandExt;
    let body = fields.iter().map(|(k, v)| format!("{k}={}", crate::voice::url_encode(v))).collect::<Vec<_>>().join("&");
    let output = std::process::Command::new("curl.exe")
        .args(["--silent", "--max-time", "15", "-X", "POST", "-H", "Content-Type: application/x-www-form-urlencoded", "--data", &body, "https://discord.com/api/oauth2/token"])
        .creation_flags(crate::apps::CREATE_NO_WINDOW)
        .output()
        .map_err(|_| "No pude abrir curl.exe.".to_string())?;
    let answer: Value = serde_json::from_slice(&output.stdout).map_err(|_| "Discord no respondió.".to_string())?;
    if answer["access_token"].is_string() {
        Ok(answer)
    } else {
        Err(format!("Discord no dio el permiso: {}", answer["error_description"].as_str().or(answer["error"].as_str()).unwrap_or("sin motivo")))
    }
}

fn session(app: &AppHandle) -> Result<Pipe, String> {
    let mut stored = load(app);
    let id = stored["client_id"].as_str().filter(|id| !id.is_empty()).ok_or("Conecta Discord en Ajustes → Juegos.")?.to_string();
    let secret = stored["secret"].as_str().unwrap_or_default().to_string();
    let mut pipe = Pipe::open()?;
    pipe.send(0, &json!({ "v": 1, "client_id": id }))?;
    let (op, _) = pipe.receive()?;
    if op == 2 {
        return Err("Discord no aceptó el Client ID.".into());
    }
    let mut renewed = false;
    if let Some(access) = stored["access"].as_str() {
        if pipe.command("AUTHENTICATE", json!({ "access_token": access })).is_ok() {
            return Ok(pipe);
        }
        if let Some(refresh) = stored["refresh"].as_str().filter(|r| !r.is_empty()) {
            if let Ok(fresh) = token_request(&[("client_id", &id), ("client_secret", &secret), ("grant_type", "refresh_token"), ("refresh_token", refresh)]) {
                stored["access"] = fresh["access_token"].clone();
                stored["refresh"] = fresh["refresh_token"].clone();
                keep(app, &stored)?;
                renewed = true;
            }
        }
    }
    if !renewed {
        let granted = pipe.command("AUTHORIZE", json!({ "client_id": id, "scopes": ["rpc", "rpc.voice.read", "rpc.voice.write"] }))?;
        let code = granted["code"].as_str().ok_or("Discord no dio el permiso.")?;
        let fresh = token_request(&[("client_id", &id), ("client_secret", &secret), ("grant_type", "authorization_code"), ("code", code), ("redirect_uri", REDIRECT)])?;
        stored["access"] = fresh["access_token"].clone();
        stored["refresh"] = fresh["refresh_token"].clone();
        keep(app, &stored)?;
    }
    pipe.command("AUTHENTICATE", json!({ "access_token": stored["access"] }))?;
    Ok(pipe)
}

fn toggle(app: &AppHandle, field: &str, wanted: Option<bool>) -> Result<String, String> {
    let mut pipe = session(app)?;
    let now = pipe.command("GET_VOICE_SETTINGS", json!({}))?;
    let value = wanted.unwrap_or(!now[field].as_bool().unwrap_or(false));
    pipe.command("SET_VOICE_SETTINGS", json!({ field: value }))?;
    Ok(match (field, value) {
        ("mute", true) => "Silenciado en Discord.",
        ("mute", false) => "Micrófono activo en Discord.",
        (_, true) => "Ensordecido en Discord.",
        _ => "Ya oyes en Discord.",
    }
    .into())
}

fn join(app: &AppHandle, name: &str) -> Result<String, String> {
    let mut pipe = session(app)?;
    let guilds = pipe.command("GET_GUILDS", json!({}))?;
    let mut best: Option<(f32, String, String)> = None;
    for guild in guilds["guilds"].as_array().into_iter().flatten().take(40) {
        let Some(guild_id) = guild["id"].as_str() else { continue };
        let Ok(channels) = pipe.command("GET_CHANNELS", json!({ "guild_id": guild_id })) else { continue };
        for channel in channels["channels"].as_array().into_iter().flatten().filter(|c| c["type"].as_u64() == Some(2)) {
            let (Some(id), Some(title)) = (channel["id"].as_str(), channel["name"].as_str()) else { continue };
            let score = crate::intent::similarity(name, title);
            if best.as_ref().is_none_or(|(top, _, _)| score > *top) {
                best = Some((score, id.to_string(), title.to_string()));
            }
        }
    }
    let (score, id, title) = best.ok_or("No encontré canales de voz en tus servidores.")?;
    if score < 0.6 {
        return Err(format!("No encontré un canal de voz llamado «{name}»."));
    }
    pipe.command("SELECT_VOICE_CHANNEL", json!({ "channel_id": id, "force": true }))?;
    Ok(format!("Entraste a «{title}»."))
}

fn leave(app: &AppHandle) -> Result<String, String> {
    session(app)?.command("SELECT_VOICE_CHANNEL", json!({ "channel_id": Value::Null }))?;
    Ok("Saliste del canal de voz.".into())
}

#[derive(Debug, PartialEq)]
pub enum Order {
    Mute(Option<bool>),
    Deaf(Option<bool>),
    Join(String),
    Leave,
    Stream,
}

pub fn understand(spoken: &str) -> Option<Order> {
    let clean = crate::intent::normalize(spoken);
    let has = |options: &[&str]| options.iter().any(|option| clean.contains(option));
    let named = clean.contains("discord");
    if has(&["desmuteame", "desmutame", "desmutea", "quitame el silencio", "activa mi microfono", "prende mi microfono", "unmute me", "desmuta"]) {
        return Some(Order::Mute(Some(false)));
    }
    if has(&["muteame", "mutame", "silenciame", "mute me"]) || (named && has(&["silencia", "mutea", "mute"])) {
        return Some(Order::Mute(Some(true)));
    }
    if has(&["desensordeceme", "quitame lo sordo", "undeafen", "ya quiero oir"]) {
        return Some(Order::Deaf(Some(false)));
    }
    if has(&["ensordeceme", "deafen me", "no quiero oir a nadie"]) || (named && has(&["ensordece", "deafen"])) {
        return Some(Order::Deaf(Some(true)));
    }
    if has(&["sal del canal", "salte del canal", "salir del canal", "desconectame de discord", "desconectate de discord", "cuelga en discord", "sal de la llamada", "leave the channel", "leave voice", "sai do canal"]) {
        return Some(Order::Leave);
    }
    if named && has(&["transmite", "transmitir", "comparte la pantalla", "go live", "stream"]) {
        return Some(Order::Stream);
    }
    const LEADS: [&str; 9] = ["metete al canal ", "entra al canal ", "unete al canal ", "conectate al canal ", "metete en el canal ", "entra en el canal ", "join the channel ", "join channel ", "entra no canal "];
    let lead = LEADS.iter().find_map(|lead| clean.find(lead).map(|at| at + lead.len()))?;
    let name = clean[lead..].replace("de voz", "").replace("en discord", "").replace("de discord", "");
    let name = name.trim();
    (!name.is_empty()).then(|| Order::Join(name.to_string()))
}

pub fn spoken(app: &AppHandle, spoken: &str) -> Option<Result<String, String>> {
    let order = understand(spoken)?;
    if load(app)["client_id"].as_str().is_none_or(str::is_empty) && !crate::intent::normalize(spoken).contains("discord") && !matches!(order, Order::Join(_)) {
        return None;
    }
    Some(match order {
        Order::Mute(wanted) => toggle(app, "mute", wanted),
        Order::Deaf(wanted) => toggle(app, "deaf", wanted),
        Order::Join(name) => join(app, &name),
        Order::Leave => leave(app),
        Order::Stream => Err("Discord no deja empezar a transmitir desde fuera. Pídeselo al modo agente: «encárgate de transmitir mi pantalla en Discord».".into()),
    })
}

#[derive(serde::Serialize)]
pub struct Estado {
    configurado: bool,
    conectado: bool,
}

#[tauri::command]
pub fn discord_estado(app: AppHandle) -> Estado {
    let stored = load(&app);
    Estado { configurado: stored["client_id"].as_str().is_some_and(|id| !id.is_empty()), conectado: stored["access"].is_string() }
}

#[tauri::command]
pub async fn discord_conectar(app: AppHandle, id: String, secreto: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (id, secret) = (id.trim().to_string(), secreto.trim().to_string());
        if !id.chars().all(|c| c.is_ascii_digit()) || id.len() < 15 || secret.len() < 20 {
            return Err("Pega el Client ID (solo números) y el Client Secret de tu app de Discord.".to_string());
        }
        keep(&app, &json!({ "client_id": id, "secret": secret }))?;
        let mut pipe = session(&app)?;
        let me = pipe.command("GET_SELECTED_VOICE_CHANNEL", json!({})).ok();
        Ok(match me.and_then(|c| c["name"].as_str().map(str::to_string)) {
            Some(channel) => format!("Discord conectado. Estás en «{channel}»."),
            None => "Discord conectado.".to_string(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn discord_quitar(app: AppHandle) {
    if let Some(path) = vault(&app) {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discord_orders_are_understood() {
        assert_eq!(understand("silénciame en Discord"), Some(Order::Mute(Some(true))));
        assert_eq!(understand("desmutéame"), Some(Order::Mute(Some(false))));
        assert_eq!(understand("ensordéceme"), Some(Order::Deaf(Some(true))));
        assert_eq!(understand("métete al canal General"), Some(Order::Join("general".into())));
        assert_eq!(understand("entra al canal de voz Squad en Discord"), Some(Order::Join("squad".into())));
        assert_eq!(understand("sal del canal"), Some(Order::Leave));
        assert_eq!(understand("transmite mi pantalla en discord"), Some(Order::Stream));
        assert_eq!(understand("silencia el volumen"), None);
        assert_eq!(understand("pon salsa"), None);
    }
}
