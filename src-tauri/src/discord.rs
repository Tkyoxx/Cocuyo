// discord desde cocuyo: el rpc local de la app de discord (la tuberia discord-ipc). cada quien usa su propia app del portal de desarrolladores

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Mutex;

use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

const REDIRECT: &str = "http://127.0.0.1";
const LOST: &str = "Discord se desconectó";
const CLOSED: &str = "Discord no está abierto.";

struct Pipe(File);

impl Pipe {
    fn open() -> Result<Pipe, String> {
        (0..10)
            .find_map(|i| OpenOptions::new().read(true).write(true).open(format!(r"\\.\pipe\discord-ipc-{i}")).ok())
            .map(Pipe)
            .ok_or_else(|| CLOSED.to_string())
    }

    fn send(&mut self, op: u32, body: &Value) -> Result<(), String> {
        let text = body.to_string();
        let mut frame = Vec::with_capacity(8 + text.len());
        frame.extend(op.to_le_bytes());
        frame.extend((text.len() as u32).to_le_bytes());
        frame.extend(text.as_bytes());
        self.0.write_all(&frame).map_err(|e| format!("{LOST}: {e}"))
    }

    fn receive(&mut self) -> Result<(u32, Value), String> {
        let mut head = [0u8; 8];
        self.0.read_exact(&mut head).map_err(|e| format!("{LOST}: {e}"))?;
        let op = u32::from_le_bytes([head[0], head[1], head[2], head[3]]);
        let length = u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as usize;
        let mut body = vec![0u8; length.min(1 << 20)];
        self.0.read_exact(&mut body).map_err(|e| format!("{LOST}: {e}"))?;
        Ok((op, serde_json::from_slice(&body).unwrap_or(Value::Null)))
    }

    fn command(&mut self, cmd: &str, args: Value) -> Result<Value, String> {
        let nonce = format!("{:x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
        self.send(1, &json!({ "cmd": cmd, "args": args, "nonce": nonce }))?;
        loop {
            let (op, message) = self.receive()?;
            if op == 2 {
                return Err(format!("{LOST}: {}", message["message"].as_str().unwrap_or("sin motivo")));
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

const SCOPES: [&str; 3] = ["rpc", "rpc.voice.read", "rpc.voice.write"];

fn curl_json(args: &[&str]) -> Result<Value, String> {
    use std::os::windows::process::CommandExt;
    let output = std::process::Command::new("curl.exe")
        .args(["--silent", "--max-time", "15"])
        .args(args)
        .creation_flags(crate::apps::CREATE_NO_WINDOW)
        .output()
        .map_err(|_| "No pude abrir curl.exe.".to_string())?;
    serde_json::from_slice(&output.stdout).map_err(|_| "Discord no respondió.".to_string())
}

fn owner_token(id: &str, secret: &str) -> Option<String> {
    let scope = format!("{} identify", SCOPES.join(" "));
    let body = format!("grant_type=client_credentials&scope={}", crate::voice::url_encode(&scope));
    let login = format!("{id}:{secret}");
    let answer = curl_json(&["-X", "POST", "-u", &login, "-H", "Content-Type: application/x-www-form-urlencoded", "--data", &body, "https://discord.com/api/oauth2/token"]).ok()?;
    answer["access_token"].as_str().map(str::to_string)
}

fn owner_name(token: &str) -> String {
    let header = format!("Authorization: Bearer {token}");
    curl_json(&["-H", &header, "https://discord.com/api/v10/users/@me"]).ok().and_then(|me| me["username"].as_str().map(str::to_string)).unwrap_or_default()
}

fn token_request(fields: &[(&str, &str)]) -> Result<Value, String> {
    use std::os::windows::process::CommandExt;
    let body = fields.iter().map(|(k, v)| format!("{k}={}", crate::voice::url_encode(v))).collect::<Vec<_>>().join("&");
    let output = std::process::Command::new("curl.exe")
        .args(["--silent", "--max-time", "15", "-X", "POST", "-H", "Content-Type: application/x-www-form-urlencoded", "--data", &body, "https://discord.com/api/oauth2/token"])
        .creation_flags(crate::apps::CREATE_NO_WINDOW)
        .output()
        .map_err(|_| "No pude abrir curl.exe.".to_string())?;
    if cfg!(debug_assertions) {
        let raw = String::from_utf8_lossy(&output.stdout);
        eprintln!("[discord] token: {}", if raw.contains("access_token") { "llegó la llave".to_string() } else { raw.chars().take(400).collect() });
    }
    let answer: Value = serde_json::from_slice(&output.stdout).map_err(|_| "Discord no respondió.".to_string())?;
    if answer["access_token"].is_string() {
        Ok(answer)
    } else {
        Err(format!("Discord no dio el permiso: {}", answer["error_description"].as_str().or(answer["error"].as_str()).unwrap_or("sin motivo")))
    }
}

fn other_account(owner: &str, here: &str) -> String {
    format!(
        "Tu app de Discord es de la cuenta «{owner}», pero Discord está abierto con «{here}». Abre Discord con «{owner}», o crea la app con «{here}». También sirve agregar a «{here}» en «App Testers» del portal y aceptar la invitación."
    )
}

fn plain(error: &str) -> String {
    if error.contains("redirect_uri") || error.contains("Redirect URI") {
        "Discord no encontró la dirección de vuelta de tu app. En el portal, entra a «OAuth2», y en «Redirects» deja una sola: http://127.0.0.1 (borra las demás con la X). Dale «Save Changes» abajo y vuelve a pulsar Conectar.".into()
    } else if error.contains("invalid_client") {
        "Discord no reconoce el Client Secret. En «OAuth2» pulsa «Reset Secret», copia el nuevo y pégalo otra vez.".into()
    } else if error.contains("access_denied") || error.contains("cancel") {
        "No le diste «Autorizar» en Discord. Pulsa Conectar otra vez y acepta.".into()
    } else {
        error.to_string()
    }
}

fn session(app: &AppHandle) -> Result<Pipe, String> {
    let mut stored = load(app);
    let id = stored["client_id"].as_str().filter(|id| !id.is_empty()).ok_or("Conecta Discord en Ajustes → Juegos.")?.to_string();
    let secret = stored["secret"].as_str().unwrap_or_default().to_string();
    let mut pipe = Pipe::open()?;
    pipe.send(0, &json!({ "v": 1, "client_id": id }))?;
    let (op, ready) = pipe.receive()?;
    if op == 2 {
        return Err("Discord no aceptó el Client ID.".into());
    }
    let here = ready["data"]["user"]["username"].as_str().unwrap_or("").to_string();
    if let (Ok(mut me), Some(id)) = (ME.lock(), ready["data"]["user"]["id"].as_str()) {
        *me = id.to_string();
    }
    let mut renewed = false;
    if let Some(access) = stored["access"].as_str() {
        if pipe.command("AUTHENTICATE", json!({ "access_token": access })).is_ok() {
            return Ok(pipe);
        }
    }
    let mut owner = String::new();
    if let Some(token) = owner_token(&id, &secret) {
        match pipe.command("AUTHENTICATE", json!({ "access_token": token })) {
            Ok(_) => {
                stored["access"] = json!(token);
                stored["refresh"] = Value::Null;
                keep(app, &stored)?;
                return Ok(pipe);
            }
            Err(why) => {
                if cfg!(debug_assertions) {
                    eprintln!("[discord] llave del dueño: {why}");
                }
                if why.contains("does not match") {
                    owner = owner_name(&token);
                }
            }
        }
    }
    if stored["access"].is_string() {
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
        let granted = pipe.command("AUTHORIZE", json!({ "client_id": id, "scopes": SCOPES }));
        if cfg!(debug_assertions) {
            eprintln!("[discord] authorize: {:?}", granted.as_ref().map(|g| g.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>())));
        }
        let granted = granted.map_err(|e| if owner.is_empty() { plain(&e) } else { other_account(&owner, &here) })?;
        let code = granted["code"].as_str().ok_or("Discord no dio el permiso.")?;
        let fresh = token_request(&[("client_id", &id), ("client_secret", &secret), ("grant_type", "authorization_code"), ("code", code), ("redirect_uri", REDIRECT)]).map_err(|e| plain(&e))?;
        stored["access"] = fresh["access_token"].clone();
        stored["refresh"] = fresh["refresh_token"].clone();
        keep(app, &stored)?;
    }
    pipe.command("AUTHENTICATE", json!({ "access_token": stored["access"] }))?;
    Ok(pipe)
}

static LINK: Mutex<Option<Pipe>> = Mutex::new(None);
static ME: Mutex<String> = Mutex::new(String::new());

fn with<T>(app: &AppHandle, job: impl Fn(&mut Pipe) -> Result<T, String>) -> Result<T, String> {
    let mut link = LINK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if link.is_none() {
        *link = Some(session(app)?);
    }
    let first = job(link.as_mut().ok_or(CLOSED)?);
    match first {
        Err(why) if why.starts_with(LOST) => {
            *link = None;
            *link = Some(session(app)?);
            let again = job(link.as_mut().ok_or(CLOSED)?);
            if again.as_ref().is_err_and(|why| why.starts_with(LOST)) {
                *link = None;
                return Err(CLOSED.into());
            }
            again
        }
        done => done,
    }
}

fn forget_link() {
    *LINK.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
}

const LOOKALIKES: [(&str, &str); 3] = [
    ("ᴀʙᴄᴅᴇꜰɢʜɪᴊᴋʟᴍɴᴏᴘꞯʀꜱᴛᴜᴠᴡxʏᴢ", "abcdefghijklmnopqrstuvwxyz"),
    ("ᵃᵇᶜᵈᵉᶠᵍʰⁱʲᵏˡᵐⁿᵒᵖqʳˢᵗᵘᵛʷˣʸᶻ", "abcdefghijklmnopqrstuvwxyz"),
    ("αβεικνορτυχωΑΒΕΗΙΚΜΝΟΡΤΥΧ", "abeikvoptuxwABEHIKMNOPTYX"),
];

fn plain_letter(c: char) -> char {
    let code = c as u32;
    let moved = match code {
        0x1D400..=0x1D6A3 => {
            let at = (code - 0x1D400) % 52;
            if at < 26 { 'A' as u32 + at } else { 'a' as u32 + at - 26 }
        }
        0x1D6A8..=0x1D7C9 => {
            let at = (code - 0x1D6A8) % 58;
            if at < 25 { 0x391 + at } else if (26..51).contains(&at) { 0x3B1 + at - 26 } else { code }
        }
        0x1D7CE..=0x1D7FF => '0' as u32 + (code - 0x1D7CE) % 10,
        0xFF10..=0xFF19 | 0xFF21..=0xFF3A | 0xFF41..=0xFF5A => code - 0xFEE0,
        _ => code,
    };
    let c = char::from_u32(moved).unwrap_or(c);
    for (fancy, plain) in LOOKALIKES {
        if let Some(at) = fancy.chars().position(|f| f == c) {
            return plain.chars().nth(at).unwrap_or(c);
        }
    }
    c
}

fn pretty(name: &str) -> String {
    let kept: String = name.chars().map(plain_letter).map(|c| if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.' | '\'') { c } else { ' ' }).collect();
    let kept = kept.split_whitespace().collect::<Vec<_>>().join(" ");
    if kept.is_empty() { name.trim().to_string() } else { kept }
}

fn voice(pipe: &mut Pipe) -> Result<Value, String> {
    pipe.command("GET_VOICE_SETTINGS", json!({}))
}

fn here(pipe: &mut Pipe) -> Result<Value, String> {
    pipe.command("GET_SELECTED_VOICE_CHANNEL", json!({}))
}

fn person_name(state: &Value) -> String {
    let user = &state["user"];
    let name = state["nick"].as_str().filter(|n| !n.is_empty()).or(user["global_name"].as_str()).or(user["username"].as_str()).unwrap_or("alguien");
    pretty(name)
}

fn me() -> String {
    ME.lock().map(|me| me.clone()).unwrap_or_default()
}

fn find_person<'a>(channel: &'a Value, name: &str) -> Option<&'a Value> {
    let mine = me();
    channel["voice_states"]
        .as_array()?
        .iter()
        .filter(|state| state["user"]["id"].as_str() != Some(mine.as_str()))
        .map(|state| {
            let user = &state["user"];
            let names = [person_name(state), user["global_name"].as_str().unwrap_or("").to_string(), user["username"].as_str().unwrap_or("").to_string()];
            let score = names.iter().filter(|n| !n.is_empty()).map(|n| crate::intent::similarity(name, n).max(crate::intent::covers(name, n))).fold(0.0f32, f32::max);
            (score, state)
        })
        .filter(|(score, _)| *score >= 0.55)
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, state)| state)
}

fn toggle(app: &AppHandle, field: &str, wanted: Option<bool>) -> Result<String, String> {
    let value = with(app, |pipe| {
        let now = voice(pipe)?;
        let value = wanted.unwrap_or(!now[field].as_bool().unwrap_or(false));
        pipe.command("SET_VOICE_SETTINGS", json!({ field: value }))?;
        Ok(value)
    })?;
    Ok(match (field, value) {
        ("mute", true) => "Silenciado en Discord.",
        ("mute", false) => "Micrófono activo en Discord.",
        (_, true) => "Ensordecido en Discord.",
        _ => "Ya oyes en Discord.",
    }
    .into())
}

fn guild_name(pipe: &mut Pipe, guild: &str) -> String {
    pipe.command("GET_GUILD", json!({ "guild_id": guild })).ok().and_then(|g| g["name"].as_str().map(pretty)).unwrap_or_default()
}

fn channels(pipe: &mut Pipe, kind: u64) -> Result<Vec<(String, String, String)>, String> {
    let current = here(pipe).ok().and_then(|c| c["guild_id"].as_str().map(str::to_string)).unwrap_or_default();
    let guilds = pipe.command("GET_GUILDS", json!({}))?;
    let mut ids: Vec<String> = guilds["guilds"].as_array().into_iter().flatten().filter_map(|g| g["id"].as_str().map(str::to_string)).take(60).collect();
    if let Some(at) = ids.iter().position(|id| *id == current) {
        let id = ids.remove(at);
        ids.insert(0, id);
    }
    let mut found = Vec::new();
    for guild in ids {
        let Ok(list) = pipe.command("GET_CHANNELS", json!({ "guild_id": guild })) else { continue };
        for channel in list["channels"].as_array().into_iter().flatten().filter(|c| c["type"].as_u64() == Some(kind)) {
            if let (Some(id), Some(name)) = (channel["id"].as_str(), channel["name"].as_str()) {
                found.push((id.to_string(), name.to_string(), guild.clone()));
            }
        }
    }
    Ok(found)
}

static LAST_GUILD: Mutex<String> = Mutex::new(String::new());

fn scored(list: Vec<(String, String, String)>, name: &str, current: &str) -> Vec<(f32, String, String, String)> {
    let mut all: Vec<(f32, String, String, String)> = list
        .into_iter()
        .map(|(id, title, guild)| {
            let plain = pretty(&title);
            let mut score = crate::intent::similarity(name, &plain).max(crate::intent::covers(name, &plain));
            if guild == current {
                score += 0.08;
            }
            (score, id, title, guild)
        })
        .collect();
    all.sort_by(|a, b| b.0.total_cmp(&a.0));
    all
}

fn best_channel(list: Vec<(String, String, String)>, name: &str, current: &str) -> Option<(f32, String, String, String)> {
    scored(list, name, current).into_iter().next()
}

fn preferred(app: &AppHandle, pipe: &mut Pipe) -> String {
    here(pipe)
        .ok()
        .and_then(|c| c["guild_id"].as_str().map(str::to_string))
        .filter(|id| !id.is_empty())
        .or_else(|| Some(crate::settings::load(app).discord_server).filter(|id| !id.is_empty()))
        .unwrap_or_else(|| LAST_GUILD.lock().map(|last| last.clone()).unwrap_or_default())
}

fn guild_called(pipe: &mut Pipe, said: &str) -> Result<Option<String>, String> {
    let guilds = pipe.command("GET_GUILDS", json!({}))?;
    Ok(guilds["guilds"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|g| Some((g["id"].as_str()?.to_string(), pretty(g["name"].as_str()?))))
        .map(|(id, title)| (crate::intent::similarity(said, &title).max(crate::intent::covers(said, &title)), id))
        .filter(|(score, _)| *score >= 0.6)
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, id)| id))
}

fn aimed(pipe: &mut Pipe, name: &str, list: &[(String, String, String)]) -> Result<(String, Option<String>), String> {
    let (channel, server) = split_server(name);
    if let Some(server) = server {
        let id = guild_called(pipe, &server)?.ok_or_else(|| format!("No estás en ningún servidor llamado «{server}»."))?;
        return Ok((channel, Some(id)));
    }
    let words: Vec<&str> = name.split(' ').collect();
    for at in (1..words.len().saturating_sub(1)).rev() {
        if !matches!(words[at], "en" | "in" | "no" | "na" | "on") {
            continue;
        }
        let (left, right) = (words[..at].join(" "), words[at + 1..].join(" "));
        if right.chars().count() < 3 {
            continue;
        }
        let Some(id) = guild_called(pipe, &right)? else { continue };
        let fits = list.iter().filter(|c| c.2 == id).any(|(_, title, _)| {
            let plain = pretty(title);
            crate::intent::similarity(&left, &plain).max(crate::intent::covers(&left, &plain)) >= 0.6
        });
        if fits {
            return Ok((left, Some(id)));
        }
    }
    Ok((name.to_string(), None))
}

fn split_server(name: &str) -> (String, Option<String>) {
    for mark in [" del servidor ", " en el servidor ", " in the server ", " on the server ", " no servidor ", " do servidor "] {
        if let Some((channel, server)) = name.split_once(mark) {
            return (channel.trim().to_string(), Some(server.trim().to_string()));
        }
    }
    (name.to_string(), None)
}

fn join(app: &AppHandle, name: &str) -> Result<String, String> {
    with(app, |pipe| {
        let current = preferred(app, pipe);
        let mut list = channels(pipe, 2)?;
        let (name, server) = aimed(pipe, name, &list)?;
        if let Some(wanted) = &server {
            list.retain(|(_, _, guild)| guild == wanted);
        }
        let all = scored(list, &name, server.as_deref().unwrap_or(&current));
        let (score, id, title, guild) = all.first().cloned().ok_or("No encontré canales de voz en tus servidores.")?;
        if score < 0.6 {
            return Err(format!("No encontré un canal de voz llamado «{name}»."));
        }
        if server.is_none() && guild != current {
            if let Some(twin) = all.iter().skip(1).find(|other| other.3 != guild && score - other.0 < 0.05) {
                let (one, two) = (guild_name(pipe, &guild), guild_name(pipe, &twin.3));
                return Err(format!("Hay un «{}» en {one} y otro en {two}. Dime «métete a {name} del servidor …» y el nombre.", pretty(&title)));
            }
        }
        pipe.command("SELECT_VOICE_CHANNEL", json!({ "channel_id": id, "force": true }))?;
        if let Ok(mut last) = LAST_GUILD.lock() {
            *last = guild;
        }
        Ok(format!("Entraste a «{}».", pretty(&title)))
    })
}

fn leave(app: &AppHandle) -> Result<String, String> {
    with(app, |pipe| {
        if let Some(guild) = here(pipe).ok().and_then(|c| c["guild_id"].as_str().map(str::to_string)).filter(|id| !id.is_empty()) {
            if let Ok(mut last) = LAST_GUILD.lock() {
                *last = guild;
            }
        }
        pipe.command("SELECT_VOICE_CHANNEL", json!({ "channel_id": Value::Null }))
    })?;
    Ok("Saliste del canal de voz.".into())
}

fn open_text(app: &AppHandle, name: &str) -> Result<String, String> {
    let (id, title, guild) = with(app, |pipe| {
        let current = preferred(app, pipe);
        let mut list = channels(pipe, 0)?;
        let (name, server) = aimed(pipe, name, &list)?;
        if let Some(wanted) = &server {
            list.retain(|(_, _, guild)| guild == wanted);
        }
        let (score, id, title, guild) = best_channel(list, &name, server.as_deref().unwrap_or(&current)).ok_or("No encontré canales de texto en tus servidores.")?;
        if score < 0.6 {
            return Err(format!("No encontré un canal de texto llamado «{name}»."));
        }
        let _ = pipe.command("SELECT_TEXT_CHANNEL", json!({ "channel_id": id }));
        Ok((id, title, guild))
    })?;
    let _ = crate::apps::open(&format!("discord://-/channels/{guild}/{id}"));
    Ok(format!("Abriendo «{}» en Discord.", pretty(&title)))
}

fn who(app: &AppHandle) -> Result<String, String> {
    let (channel, mine, server) = with(app, |pipe| {
        let channel = here(pipe)?;
        let mine = voice(pipe)?;
        let server = channel["guild_id"].as_str().map(|g| guild_name(pipe, g)).unwrap_or_default();
        Ok((channel, mine, server))
    })?;
    if channel.is_null() || channel["id"].is_null() {
        return Ok("No estás en ningún canal de voz.".into());
    }
    let mine_id = me();
    let title = pretty(channel["name"].as_str().unwrap_or(""));
    let others: Vec<&Value> = channel["voice_states"].as_array().into_iter().flatten().filter(|s| s["user"]["id"].as_str() != Some(mine_id.as_str())).collect();
    let rows: Vec<Value> = channel["voice_states"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|state| {
            let flags = &state["voice_state"];
            let status = if flags["self_deaf"].as_bool() == Some(true) || flags["deaf"].as_bool() == Some(true) {
                "Ensordecido"
            } else if flags["self_mute"].as_bool() == Some(true) || flags["mute"].as_bool() == Some(true) || state["mute"].as_bool() == Some(true) {
                "Silenciado"
            } else {
                "Hablando libre"
            };
            let name = if state["user"]["id"].as_str() == Some(mine_id.as_str()) { format!("{} (tú)", person_name(state)) } else { person_name(state) };
            json!({ "texto": name, "valor": crate::i18n::tr(status) })
        })
        .collect();
    crate::scripts::show_card(app, "Discord", &json!({ "titulo": title, "texto": server, "icono": "🎧", "filas": rows, "segundos": 25 }));
    let names: Vec<String> = others.iter().map(|s| person_name(s)).collect();
    let mut said = match names.len() {
        0 => format!("Estás solo en «{title}»."),
        1 => format!("Estás en «{title}» con {}.", names[0]),
        _ => format!("Estás en «{title}» con {} y {}.", names[..names.len() - 1].join(", "), names[names.len() - 1]),
    };
    said.truncate(said.trim_end().len());
    let deaf = if mine["deaf"].as_bool() == Some(true) { " Estás ensordecido." } else { "" };
    let tail = if mine["mute"].as_bool() == Some(true) { format!(" Tienes el micrófono silenciado.{deaf}") } else { deaf.to_string() };
    said = format!("{}{tail}", said.trim_end_matches(&['.'][..]).to_string() + ".");
    Ok(said)
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Side {
    Out,
    In,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Change {
    Set(u8),
    Up,
    Down,
}

fn changed(now: f64, change: Change, top: f64, step: f64) -> f64 {
    match change {
        Change::Set(value) => f64::from(value).min(top),
        Change::Up => (now + step).min(top),
        Change::Down => (now - step).max(0.0),
    }
}

fn set_volume(app: &AppHandle, side: Side, change: Change) -> Result<String, String> {
    let level = with(app, |pipe| {
        let now = voice(pipe)?;
        let key = if side == Side::In { "input" } else { "output" };
        let level = changed(now[key]["volume"].as_f64().unwrap_or(100.0), change, if side == Side::In { 100.0 } else { 200.0 }, 15.0).round();
        pipe.command("SET_VOICE_SETTINGS", json!({ key: { "volume": level } }))?;
        Ok(level)
    })?;
    Ok(match side {
        Side::In => format!("Tu micrófono en Discord al {level} %."),
        Side::Out => format!("Discord al {level} %."),
    })
}

fn person(app: &AppHandle, name: &str, change: Option<Change>, mute: Option<bool>) -> Result<String, String> {
    let (who, level, muted) = with(app, |pipe| {
        let channel = here(pipe)?;
        if channel.is_null() || channel["id"].is_null() {
            return Err("No estás en ningún canal de voz.".into());
        }
        let state = find_person(&channel, name).ok_or_else(|| format!("No veo a «{name}» en tu canal."))?;
        let id = state["user"]["id"].as_str().unwrap_or("").to_string();
        let mut args = json!({ "user_id": id });
        let mut level = state["volume"].as_f64().unwrap_or(100.0);
        if let Some(change) = change {
            level = changed(level, change, 200.0, 25.0).round();
            args["volume"] = json!(level);
        }
        if let Some(mute) = mute {
            args["mute"] = json!(mute);
        }
        pipe.command("SET_USER_VOICE_SETTINGS", args)?;
        Ok((person_name(state), level, mute))
    })?;
    Ok(match muted {
        Some(true) => format!("Silenciaste a {who} solo para ti."),
        Some(false) => format!("Ya oyes a {who}."),
        None => format!("{who} al {level} %, solo para ti."),
    })
}

fn talk_mode(app: &AppHandle, push: bool) -> Result<String, String> {
    let keys = with(app, |pipe| {
        let now = voice(pipe)?;
        let mut mode = now["mode"].clone();
        if !mode.is_object() {
            mode = json!({});
        }
        mode["type"] = json!(if push { "PUSH_TO_TALK" } else { "VOICE_ACTIVITY" });
        pipe.command("SET_VOICE_SETTINGS", json!({ "mode": mode }))?;
        Ok(now["mode"]["shortcut"].as_array().map(|keys| keys.iter().filter_map(|k| k["name"].as_str()).map(str::to_uppercase).collect::<Vec<_>>().join("+")).unwrap_or_default())
    })?;
    Ok(match (push, keys.is_empty()) {
        (true, false) => format!("Pulsar para hablar activado en Discord. La tecla es {keys}."),
        (true, true) => "Pulsar para hablar activado en Discord. Ponle una tecla en sus ajustes de voz.".into(),
        (false, _) => "Discord vuelve a detectar tu voz solo.".into(),
    })
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Filter {
    Noise,
    Echo,
    Gain,
}

fn filter(app: &AppHandle, which: Filter, on: bool) -> Result<String, String> {
    let key = match which {
        Filter::Noise => "noise_suppression",
        Filter::Echo => "echo_cancellation",
        Filter::Gain => "automatic_gain_control",
    };
    with(app, |pipe| pipe.command("SET_VOICE_SETTINGS", json!({ key: on })))?;
    Ok(match (which, on) {
        (Filter::Noise, true) => "Supresión de ruido de Discord activada.",
        (Filter::Noise, false) => "Supresión de ruido de Discord apagada.",
        (Filter::Echo, true) => "Cancelación de eco de Discord activada.",
        (Filter::Echo, false) => "Cancelación de eco de Discord apagada.",
        (Filter::Gain, true) => "Control de ganancia de Discord activado.",
        (Filter::Gain, false) => "Control de ganancia de Discord apagado.",
    }
    .into())
}

fn device(app: &AppHandle, side: Side, name: &str) -> Result<String, String> {
    let chosen = with(app, |pipe| {
        let now = voice(pipe)?;
        let key = if side == Side::In { "input" } else { "output" };
        let (score, id, title) = now[key]["available_devices"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|d| d["id"].as_str() != Some("default"))
            .filter_map(|d| {
                let title = d["name"].as_str()?.to_string();
                let score = crate::intent::similarity(name, &title).max(crate::intent::covers(name, &title));
                Some((score, d["id"].as_str()?.to_string(), title))
            })
            .max_by(|a, b| a.0.total_cmp(&b.0))
            .ok_or("Discord no me mostró sus dispositivos.")?;
        if score < 0.5 {
            return Err(format!("No encontré «{name}» en los dispositivos de Discord."));
        }
        pipe.command("SET_VOICE_SETTINGS", json!({ key: { "device_id": id } }))?;
        Ok(title)
    })?;
    Ok(match side {
        Side::In => format!("Discord ahora te escucha por {chosen}."),
        Side::Out => format!("Discord ahora suena por {chosen}."),
    })
}

fn invite(app: &AppHandle) -> Result<String, String> {
    let (code, title) = with(app, |pipe| {
        let channel = here(pipe)?;
        let id = channel["id"].as_str().ok_or("Métete primero a un canal de voz y te paso su invitación.")?.to_string();
        let made = pipe.command("CREATE_CHANNEL_INVITE", json!({ "channel_id": id }))?;
        Ok((made["code"].as_str().unwrap_or("").to_string(), pretty(channel["name"].as_str().unwrap_or(""))))
    })?;
    if code.is_empty() {
        return Err("Discord no me dio la invitación.".into());
    }
    crate::clip::ignore_own_changes();
    crate::inject::set_clipboard_text(&format!("https://discord.gg/{code}"));
    Ok(format!("Copié la invitación a «{title}». Pégala donde quieras."))
}

#[derive(Debug, PartialEq)]
pub enum Order {
    Mute(Option<bool>),
    Deaf(Option<bool>),
    Join(String),
    Leave,
    Stream,
    Who,
    Volume(Side, Change),
    Person(String, Option<Change>, Option<bool>),
    PushToTalk(bool),
    Filter(Filter, bool),
    Device(Side, String),
    Text(String),
    Invite,
}

fn cut_tail(text: &str) -> String {
    let mut text = format!(" {text} ");
    for tail in [" en discord ", " de discord ", " del discord ", " on discord ", " no discord ", " por favor ", " porfa "] {
        text = text.replace(tail, " ");
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn change_in(tokens: &[&str], up: bool, down: bool) -> Option<Change> {
    match crate::intent::number_in(tokens) {
        Some((value, _)) => Some(Change::Set(value)),
        None if up => Some(Change::Up),
        None if down => Some(Change::Down),
        None => None,
    }
}

pub fn understand(spoken: &str) -> Option<Order> {
    let clean = crate::intent::normalize(spoken);
    let clean = clean.split_whitespace().collect::<Vec<_>>().join(" ");
    let padded = format!(" {clean} ");
    let has = |options: &[&str]| options.iter().any(|option| padded.contains(option));
    let named = has(&[" discord "]);
    let off = has(&[" quita ", " quitale ", " desactiva ", " apaga ", " sin ", " off ", " disable ", " turn off ", " desliga "]);
    let tokens: Vec<&str> = clean.split_whitespace().collect();
    if has(&[" sal del canal", " salte del canal", " salir del canal", " desconectame de discord", " desconectate de discord", " cuelga en discord", " sal de la llamada", " leave the channel", " leave voice", " sai do canal"]) {
        return Some(Order::Leave);
    }
    if named && has(&[" transmite", " transmitir", " comparte la pantalla", " compartir pantalla", " go live", " stream"]) {
        return Some(Order::Stream);
    }
    if has(&[" en que canal estoy", " quien esta en el canal", " quienes estan en el canal", " quien esta en discord", " quienes estan en discord", " quien esta conmigo en discord", " estoy silenciado", " estoy muteado", " estoy ensordecido", " who is in the channel", " quem esta no canal"]) {
        return Some(Order::Who);
    }
    if has(&[" invitacion al canal", " invitacion del canal", " invitacion de discord", " link del canal", " enlace del canal", " link de discord", " invite link", " convite do canal"]) {
        return Some(Order::Invite);
    }
    for lead in [" abre el canal de texto ", " abreme el canal de texto ", " ve al canal de texto ", " entra al canal de texto ", " abre el chat ", " abreme el chat ", " open the text channel "] {
        if let Some(at) = padded.find(lead) {
            let name = cut_tail(&padded[at + lead.len()..]);
            if !name.is_empty() {
                return Some(Order::Text(name));
            }
        }
    }
    if named {
        if let Some(at) = padded.find(" abre el canal ") {
            let name = cut_tail(&padded[at + " abre el canal ".len()..]);
            if !name.is_empty() {
                return Some(Order::Text(name));
            }
        }
    }
    if has(&[" pulsar para hablar", " presionar para hablar", " push to talk", " apertar para falar"]) {
        return Some(Order::PushToTalk(!off));
    }
    if has(&[" deteccion de voz", " activacion por voz", " voice activity", " deteccao de voz"]) {
        return Some(Order::PushToTalk(off));
    }
    if named {
        if has(&[" supresion de ruido", " supresor de ruido", " quitar el ruido", " krisp", " noise suppression", " supressao de ruido"]) {
            return Some(Order::Filter(Filter::Noise, !off));
        }
        if has(&[" cancelacion de eco", " cancelar el eco", " echo cancellation", " cancelamento de eco"]) {
            return Some(Order::Filter(Filter::Echo, !off));
        }
        if has(&[" control de ganancia", " ganancia automatica", " gain control", " controle de ganho"]) {
            return Some(Order::Filter(Filter::Gain, !off));
        }
        const DEVICE_IN: [&str; 6] = [" usa el microfono ", " usa el mic ", " cambia el microfono a ", " pon el microfono ", " escuchame por el ", " use the mic "];
        const DEVICE_OUT: [&str; 6] = [" saca el sonido por ", " suena por ", " cambia la salida a ", " usa la salida ", " usa los audifonos ", " use the speakers "];
        for (side, leads) in [(Side::In, &DEVICE_IN), (Side::Out, &DEVICE_OUT)] {
            for lead in leads.iter() {
                if let Some(at) = padded.find(lead) {
                    let name = cut_tail(&padded[at + lead.len()..]);
                    let name = if *lead == " usa los audifonos " { format!("audifonos {name}") } else { name };
                    if !name.trim().is_empty() {
                        return Some(Order::Device(side, name.trim().to_string()));
                    }
                }
            }
        }
        const PEOPLE: [(&str, i8); 12] = [
            (" baja a ", -1), (" bajale a ", -1), (" sube a ", 1), (" subele a ", 1), (" pon a ", 0), (" deja a ", 0), (" silencia a ", 2), (" mutea a ", 2), (" mutealo a ", 2),
            (" desmutea a ", 3), (" desilencia a ", 3), (" quitale el silencio a ", 3),
        ];
        for (lead, kind) in PEOPLE {
            if let Some(at) = padded.find(lead) {
                let rest = cut_tail(&padded[at + lead.len()..]);
                let words: Vec<&str> = rest.split_whitespace().collect();
                let end = words.iter().position(|w| matches!(*w, "al" | "a" | "en" | "hasta" | "to")).unwrap_or(words.len());
                let name = words[..end].join(" ");
                if name.is_empty() || name == "discord" {
                    continue;
                }
                return Some(match kind {
                    2 => Order::Person(name, None, Some(true)),
                    3 => Order::Person(name, None, Some(false)),
                    _ => Order::Person(name, change_in(&words[end..], kind > 0, kind < 0).or(Some(if kind > 0 { Change::Up } else { Change::Down })), None),
                });
            }
        }
        let up = has(&[" sube", " subele", " mas duro", " mas alto", " louder", " aumenta"]);
        let down = has(&[" baja", " bajale", " mas bajo", " mas suave", " quieter", " diminui"]);
        let said_level = crate::intent::number_in(&tokens).is_some_and(|(_, level)| level) && has(&[" microfono", " mic ", " microphone", " microfone", " volumen", " volume"]);
        let level = has(&[" volumen", " volume"]) || up || down || said_level;
        if level && !has(&[" silencia", " mute"]) {
            let side = if has(&[" microfono", " mic ", " microphone", " microfone"]) { Side::In } else { Side::Out };
            if let Some(change) = change_in(&tokens, up, down) {
                return Some(Order::Volume(side, change));
            }
        }
    }
    if has(&[" desmuteame", " desmutame", " desmutea", " quitame el silencio", " activa mi microfono", " prende mi microfono", " unmute me", " desmuta"]) {
        return Some(Order::Mute(Some(false)));
    }
    if has(&[" muteame", " mutame", " silenciame", " mute me"]) || (named && has(&[" silencia", " mutea", " mute"])) {
        return Some(Order::Mute(Some(true)));
    }
    if has(&[" desensordeceme", " quitame lo sordo", " undeafen", " ya quiero oir"]) {
        return Some(Order::Deaf(Some(false)));
    }
    if has(&[" ensordeceme", " deafen me", " no quiero oir a nadie"]) || (named && has(&[" ensordece", " deafen"])) {
        return Some(Order::Deaf(Some(true)));
    }
    const LEADS: [&str; 12] = [
        " metete al canal ", " entra al canal ", " unete al canal ", " conectate al canal ", " metete en el canal ", " entra en el canal ", " join the channel ", " join channel ",
        " entra no canal ", " meteme al canal ", " llevame al canal ", " meteme en el canal ",
    ];
    const LOOSE: [&str; 10] = [" meteme a ", " meteme en ", " metete a ", " metete en ", " llevame a ", " entra a ", " unete a ", " conectame a ", " join ", " entra em "];
    let lead = match LEADS.iter().find_map(|lead| padded.find(lead).map(|at| at + lead.len())) {
        Some(lead) => lead,
        None => {
            let at = LOOSE.iter().find_map(|lead| padded.find(lead).map(|at| at + lead.len()))?;
            let rest = format!(" {}", &padded[at..]);
            let voiceish = [" canal", " chat", " voz", " llamada", " call", " vc", " sala", " general", " servidor", " server"].iter().any(|w| rest.contains(w));
            if !voiceish {
                return None;
            }
            at
        }
    };
    let name = cut_tail(&padded[lead..].replace("de voz", ""));
    (!name.is_empty()).then_some(Order::Join(name))
}

pub fn spoken(app: &AppHandle, spoken: &str) -> Option<Result<String, String>> {
    let order = understand(spoken)?;
    let said = crate::intent::normalize(spoken);
    if load(app)["client_id"].as_str().is_none_or(str::is_empty) && !said.contains("discord") && !(matches!(order, Order::Join(_)) && said.contains("canal")) {
        return None;
    }
    Some(match order {
        Order::Mute(wanted) => toggle(app, "mute", wanted),
        Order::Deaf(wanted) => toggle(app, "deaf", wanted),
        Order::Join(name) => join(app, &name),
        Order::Leave => leave(app),
        Order::Who => who(app),
        Order::Volume(side, change) => set_volume(app, side, change),
        Order::Person(name, change, mute) => person(app, &name, change, mute),
        Order::PushToTalk(on) => talk_mode(app, on),
        Order::Filter(which, on) => filter(app, which, on),
        Order::Device(side, name) => device(app, side, &name),
        Order::Text(name) => open_text(app, &name),
        Order::Invite => invite(app),
        Order::Stream => Err("Discord no deja empezar a transmitir desde fuera. Pídeselo al modo agente: «encárgate de transmitir mi pantalla en Discord».".into()),
    })
}

#[cfg(debug_assertions)]
#[cfg(debug_assertions)]
pub fn servers(app: &AppHandle) -> String {
    let listed = with(app, |pipe| {
        let now = here(pipe).ok().and_then(|c| c["name"].as_str().map(str::to_string));
        let guilds = pipe.command("GET_GUILDS", json!({}))?;
        let mut out = format!("en llamada: {now:?}");
        for guild in guilds["guilds"].as_array().into_iter().flatten().take(60) {
            let (Some(id), Some(name)) = (guild["id"].as_str(), guild["name"].as_str()) else { continue };
            let voice: Vec<String> = pipe
                .command("GET_CHANNELS", json!({ "guild_id": id }))
                .ok()
                .map(|list| list["channels"].as_array().into_iter().flatten().filter(|c| c["type"].as_u64() == Some(2)).filter_map(|c| c["name"].as_str().map(str::to_string)).collect())
                .unwrap_or_default();
            out += &format!("\n{name} ({id}): {voice:?}");
        }
        Ok(out)
    });
    listed.unwrap_or_else(|why| format!("falló: {why}"))
}

pub fn probe(app: &AppHandle) -> String {
    match with(app, panel) {
        Ok(panel) => format!("conectado: canal {:?} en {:?}, {} personas, mute {}, deaf {}, salida {}", panel.canal, panel.servidor, panel.gente.len(), panel.mute, panel.deaf, panel.salida),
        Err(e) => format!("falló: {e}"),
    }
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

#[derive(serde::Serialize)]
pub struct Persona {
    id: String,
    nombre: String,
    silenciado: bool,
    ensordecido: bool,
    volumen: f64,
    tu: bool,
    callado: bool,
}

#[derive(serde::Serialize)]
pub struct Panel {
    mute: bool,
    deaf: bool,
    entrada: f64,
    salida: f64,
    ruido: bool,
    eco: bool,
    pulsar: bool,
    canal: Option<String>,
    servidor: String,
    gente: Vec<Persona>,
}

fn panel(pipe: &mut Pipe) -> Result<Panel, String> {
    let settings = voice(pipe)?;
    let channel = here(pipe)?;
    let mine = me();
    let inside = channel["id"].is_string();
    let server = if inside { channel["guild_id"].as_str().map(|g| guild_name(pipe, g)).unwrap_or_default() } else { String::new() };
    let gente = channel["voice_states"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|state| {
            let flags = &state["voice_state"];
            let yes = |key: &str| flags[key].as_bool() == Some(true);
            Persona {
                id: state["user"]["id"].as_str().unwrap_or("").to_string(),
                nombre: person_name(state),
                silenciado: yes("self_mute") || yes("mute"),
                ensordecido: yes("self_deaf") || yes("deaf"),
                volumen: state["volume"].as_f64().unwrap_or(100.0).round(),
                tu: state["user"]["id"].as_str() == Some(mine.as_str()),
                callado: state["mute"].as_bool() == Some(true),
            }
        })
        .collect();
    Ok(Panel {
        mute: settings["mute"].as_bool().unwrap_or(false),
        deaf: settings["deaf"].as_bool().unwrap_or(false),
        entrada: settings["input"]["volume"].as_f64().unwrap_or(100.0).round(),
        salida: settings["output"]["volume"].as_f64().unwrap_or(100.0).round(),
        ruido: settings["noise_suppression"].as_bool().unwrap_or(false),
        eco: settings["echo_cancellation"].as_bool().unwrap_or(false),
        pulsar: settings["mode"]["type"].as_str() == Some("PUSH_TO_TALK"),
        canal: inside.then(|| pretty(channel["name"].as_str().unwrap_or(""))),
        servidor: server,
        gente,
    })
}

#[derive(serde::Serialize)]
pub struct Servidor {
    id: String,
    nombre: String,
}

#[tauri::command]
pub async fn discord_servidores(app: AppHandle) -> Result<Vec<Servidor>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if !load(&app)["access"].is_string() {
            return Ok(Vec::new());
        }
        with(&app, |pipe| {
            let guilds = pipe.command("GET_GUILDS", json!({}))?;
            Ok(guilds["guilds"].as_array().into_iter().flatten().filter_map(|g| Some(Servidor { id: g["id"].as_str()?.to_string(), nombre: pretty(g["name"].as_str()?) })).collect())
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn discord_panel(app: AppHandle) -> Result<Option<Panel>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if !load(&app)["access"].is_string() {
            return Ok(None);
        }
        with(&app, panel).map(Some)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn discord_hacer(app: AppHandle, accion: String, valor: Option<f64>, quien: Option<String>) -> Result<Panel, String> {
    tauri::async_runtime::spawn_blocking(move || {
        with(&app, |pipe| {
            let now = voice(pipe)?;
            let flip = |key: &str| !now[key].as_bool().unwrap_or(false);
            match accion.as_str() {
                "mute" => drop(pipe.command("SET_VOICE_SETTINGS", json!({ "mute": flip("mute") }))?),
                "deaf" => drop(pipe.command("SET_VOICE_SETTINGS", json!({ "deaf": flip("deaf") }))?),
                "ruido" => drop(pipe.command("SET_VOICE_SETTINGS", json!({ "noise_suppression": flip("noise_suppression") }))?),
                "eco" => drop(pipe.command("SET_VOICE_SETTINGS", json!({ "echo_cancellation": flip("echo_cancellation") }))?),
                "pulsar" => {
                    let mut mode = now["mode"].clone();
                    let push = mode["type"].as_str() != Some("PUSH_TO_TALK");
                    mode["type"] = json!(if push { "PUSH_TO_TALK" } else { "VOICE_ACTIVITY" });
                    drop(pipe.command("SET_VOICE_SETTINGS", json!({ "mode": mode }))?);
                }
                "entrada" => drop(pipe.command("SET_VOICE_SETTINGS", json!({ "input": { "volume": valor.unwrap_or(100.0).clamp(0.0, 100.0).round() } }))?),
                "salida" => drop(pipe.command("SET_VOICE_SETTINGS", json!({ "output": { "volume": valor.unwrap_or(100.0).clamp(0.0, 200.0).round() } }))?),
                "salir" => drop(pipe.command("SELECT_VOICE_CHANNEL", json!({ "channel_id": Value::Null }))?),
                "persona" => {
                    let id = quien.clone().unwrap_or_default();
                    let channel = here(pipe)?;
                    let state = channel["voice_states"].as_array().into_iter().flatten().find(|s| s["user"]["id"].as_str() == Some(id.as_str())).ok_or("Esa persona ya no está en el canal.")?;
                    let args = match valor {
                        Some(level) => json!({ "user_id": id, "volume": level.clamp(0.0, 200.0).round() }),
                        None => json!({ "user_id": id, "mute": state["mute"].as_bool() != Some(true) }),
                    };
                    drop(pipe.command("SET_USER_VOICE_SETTINGS", args)?);
                }
                other => return Err(format!("No sé hacer «{other}» en Discord.")),
            }
            panel(pipe)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn discord_conectar(app: AppHandle, id: String, secreto: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (id, secret) = (id.trim().to_string(), secreto.trim().to_string());
        if !id.chars().all(|c| c.is_ascii_digit()) || id.len() < 15 || secret.len() < 20 {
            return Err("Pega el Client ID (solo números) y el Client Secret de tu app de Discord.".to_string());
        }
        forget_link();
        keep(&app, &json!({ "client_id": id, "secret": secret }))?;
        let channel = with(&app, |pipe| here(pipe))?;
        Ok(match channel["name"].as_str() {
            Some(name) => format!("Discord conectado. Estás en «{}».", pretty(name)),
            None => "Discord conectado.".to_string(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn discord_quitar(app: AppHandle) {
    forget_link();
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
        assert_eq!(split_server("chat de voz 1 del servidor los pibes"), ("chat de voz 1".into(), Some("los pibes".into())));
        assert_eq!(split_server("chat de voz 1"), ("chat de voz 1".into(), None));
        assert_eq!(understand("méteme a chat de voz 1 en marihuana, guaro, poper"), Some(Order::Join("chat 1 en marihuana guaro poper".into())));
        assert_eq!(understand("llévame al canal General en Los Pibes"), Some(Order::Join("general en los pibes".into())));
        assert_eq!(understand("entra a YouTube"), None);
        assert_eq!(understand("méteme a la cola despacito"), None);
        assert_eq!(pretty("𝑭𝑨𝑴𝑰𝑳𝑰𝑨 𝑯𝑨𝑹𝑫𝑾𝑶𝑶𝑫"), "FAMILIA HARDWOOD");
        assert_eq!(pretty("🎃 𝐿𝘰𝜈𝑖𝑛𝑔 𝐷𝑟𝑒𝑎𝑚𝑠 ⭑ Social"), "Loving Dreams Social");
        assert_eq!(pretty("5ᴛʜ ꜱᴛʀᴇᴇᴛꜱᴀɪɴᴛꜱ™ [M.C]"), "5th streetsaints M.C");
        assert_eq!(pretty("ᵇˡᵒᵒᵈʸ ʰᵒˢᵖⁱᵗᵃˡ"), "bloody hospital");
        assert_eq!(pretty("🔊|| Chat de Voz  1 ||🔊"), "Chat de Voz 1");
        assert_eq!(understand("transmite mi pantalla en discord"), Some(Order::Stream));
        assert_eq!(understand("silencia el volumen"), None);
        assert_eq!(understand("pon salsa"), None);
        assert_eq!(understand("¿quién está en el canal?"), Some(Order::Who));
        assert_eq!(understand("¿estoy silenciado?"), Some(Order::Who));
        assert_eq!(understand("baja el volumen de Discord"), Some(Order::Volume(Side::Out, Change::Down)));
        assert_eq!(understand("volumen de Discord al 60"), Some(Order::Volume(Side::Out, Change::Set(60))));
        assert_eq!(understand("sube mi micrófono en Discord"), Some(Order::Volume(Side::In, Change::Up)));
        assert_eq!(understand("micrófono de Discord al 80"), Some(Order::Volume(Side::In, Change::Set(80))));
        assert_eq!(understand("baja a JaRRi en Discord"), Some(Order::Person("jarri".into(), Some(Change::Down), None)));
        assert_eq!(understand("pon a Juan al 30 en Discord"), Some(Order::Person("juan".into(), Some(Change::Set(30)), None)));
        assert_eq!(understand("silencia a Pedro en Discord"), Some(Order::Person("pedro".into(), None, Some(true))));
        assert_eq!(understand("desmutea a Pedro en Discord"), Some(Order::Person("pedro".into(), None, Some(false))));
        assert_eq!(understand("activa pulsar para hablar"), Some(Order::PushToTalk(true)));
        assert_eq!(understand("quita pulsar para hablar"), Some(Order::PushToTalk(false)));
        assert_eq!(understand("activa la supresión de ruido de Discord"), Some(Order::Filter(Filter::Noise, true)));
        assert_eq!(understand("apaga la cancelación de eco en Discord"), Some(Order::Filter(Filter::Echo, false)));
        assert_eq!(understand("usa el micrófono Fifine en Discord"), Some(Order::Device(Side::In, "fifine".into())));
        assert_eq!(understand("abre el canal de texto general en Discord"), Some(Order::Text("general".into())));
        assert_eq!(understand("pásame la invitación al canal"), Some(Order::Invite));
        assert_eq!(understand("sube el volumen"), None);
        assert_eq!(understand("activa la supresión de ruido"), None);
    }

    #[test]
    fn channel_names_lose_their_decoration() {
        assert_eq!(pretty("🔊|| Chat de Voz  1 ||🔊"), "Chat de Voz 1");
        assert_eq!(pretty("🎮"), "🎮");
        assert_eq!(changed(100.0, Change::Up, 100.0, 15.0), 100.0);
        assert_eq!(changed(10.0, Change::Down, 200.0, 15.0), 0.0);
        assert_eq!(changed(50.0, Change::Set(80), 100.0, 15.0), 80.0);
    }

    #[test]
    fn people_are_found_by_nick_or_name() {
        *ME.lock().unwrap() = "1".into();
        let channel = json!({ "voice_states": [
            { "nick": "JaRRi", "user": { "id": "2", "username": "thortitas_", "global_name": "Thortitas (Halloween Edition🦇)" } },
            { "nick": "T O K Y O", "user": { "id": "1", "username": "tkyo_0x" } }
        ] });
        assert_eq!(find_person(&channel, "jarri").map(|s| s["user"]["id"].clone()), Some(json!("2")));
        assert_eq!(find_person(&channel, "thortitas").map(|s| s["user"]["id"].clone()), Some(json!("2")));
        assert!(find_person(&channel, "tokyo").is_none());
        assert_eq!(person_name(&channel["voice_states"][0]), "JaRRi");
    }

    #[test]
    fn mismatched_accounts_are_explained() {
        assert!(plain("Discord: OAuth2 Error: invalid_request: Missing \"redirect_uri\" in request.").contains("deja una sola: http://127.0.0.1"));
        assert_eq!(plain("Discord no está abierto."), "Discord no está abierto.");
        assert!(other_account("ana_x", "ana").starts_with("Tu app de Discord es de la cuenta «ana_x», pero Discord está abierto con «ana»."));
    }
}
