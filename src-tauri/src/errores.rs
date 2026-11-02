// cuando algo falla de verdad queda escrito en errores.txt y cocuyo te cuenta en cristiano que paso

use std::path::PathBuf;

use tauri::AppHandle;

pub fn dir() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var("LOCALAPPDATA").ok()?).join("io.github.tkyoxx.cocuyo"))
}

pub fn file() -> Option<PathBuf> {
    Some(dir()?.join("errores.txt"))
}

fn windows_version() -> String {
    use windows::Win32::System::Registry::HKEY_LOCAL_MACHINE;
    let read = |value: &str| crate::apps::reg_string(HKEY_LOCAL_MACHINE, "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion", value).unwrap_or_default();
    format!("{} {} (build {})", read("ProductName"), read("DisplayVersion"), read("CurrentBuild")).trim().to_string()
}

const MAX_LOG: u64 = 1_000_000;

pub fn clean(detail: &str) -> String {
    let flat: String = detail.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    flat.split(' ')
        .map(|word| match word.find("://").and(word.find('?')) {
            Some(cut) if word.contains("://") => format!("{}?…", &word[..cut]),
            _ => word.to_string(),
        })
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(2000)
        .collect()
}

pub fn note(area: &str, detail: &str) {
    let Some(path) = file() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_LOG) {
        let _ = std::fs::rename(&path, path.with_extension("viejo.txt"));
    }
    let now = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    let line = format!(
        "{}-{:02}-{:02} {:02}:{:02}:{:02} · Cocuyo v{} · {} · [{area}] {}\r\n",
        now.wYear,
        now.wMonth,
        now.wDay,
        now.wHour,
        now.wMinute,
        now.wSecond,
        env!("CARGO_PKG_VERSION"),
        windows_version(),
        clean(detail)
    );
    if let Ok(mut out) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = std::io::Write::write_all(&mut out, line.as_bytes());
    }
}

pub fn tell(app: &AppHandle, said: &str) {
    if crate::grabadora::nothing_in_front() {
        let log = file().filter(|path| path.exists()).map(|path| path.display().to_string()).unwrap_or_default();
        let mut buttons = vec![serde_json::json!({ "texto": "Copiar", "copiar": said })];
        if !log.is_empty() {
            buttons.insert(0, serde_json::json!({ "texto": "Ver el registro", "abrir": log }));
        }
        let card = serde_json::json!({
            "id": 800_000 + (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() % 90_000).unwrap_or(0)),
            "script": "Cocuyo", "titulo": "Algo falló", "texto": crate::i18n::tr(said), "icono": "⚠️", "color": "#f4505e",
            "botones": buttons, "filas": [], "segundos": 25
        });
        let _ = tauri::Emitter::emit_to(app, crate::overlay::LABEL, "script-card", card);
        crate::cue::answer(said);
        return;
    }
    let said = format!("{said} Di «abre la carpeta de errores» para verlo.");
    let _ = tauri::Emitter::emit_to(app, crate::overlay::LABEL, "note", crate::i18n::tr(&said));
    crate::cue::answer(&said);
}

fn show() -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let dir = dir().ok_or("No encontré la carpeta de Cocuyo.")?;
    let _ = std::fs::create_dir_all(&dir);
    match file().filter(|path| path.exists()) {
        Some(path) => std::process::Command::new("explorer.exe")
            .raw_arg(format!("/select,\"{}\"", path.display()))
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string()),
        None => crate::apps::open(&dir.to_string_lossy()),
    }
}

const OPEN: [&str; 12] = ["abre", "abreme", "abrir", "muestrame", "ensename", "ver", "open", "show", "abra", "mostra", "mostre", "dame"];
const LOGS: [&str; 12] = [
    "carpeta de errores", "registro de errores", "los errores", "los logs", "el log", "los registros", "el registro", "error log", "errors folder",
    "the errors", "pasta de erros", "registro de erros",
];

pub fn spoken(spoken: &str) -> Option<Result<String, String>> {
    let clean = format!(" {} ", crate::intent::normalize(spoken));
    let first = clean.split_whitespace().next().unwrap_or("");
    if !OPEN.contains(&first) || !LOGS.iter().any(|logs| clean.contains(&format!(" {logs} "))) {
        return None;
    }
    let done = show().map(|()| {
        if file().is_some_and(|path| path.exists()) { "Ahí está el registro de errores.".to_string() } else { "Todavía no hay errores anotados. Ahí está la carpeta.".to_string() }
    });
    if let Ok(said) = &done {
        crate::cue::answer(said);
    }
    Some(done)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asking_for_the_logs_is_understood() {
        let asks = |text: &str| {
            let clean = format!(" {} ", crate::intent::normalize(text));
            OPEN.contains(&clean.split_whitespace().next().unwrap_or("")) && LOGS.iter().any(|logs| clean.contains(&format!(" {logs} ")))
        };
        assert!(asks("Abre la carpeta de errores"));
        assert!(asks("muéstrame el registro de errores"));
        assert!(asks("abre los logs"));
        assert!(asks("open the error log"));
        assert!(!asks("abre spotify"));
        assert!(!asks("borra los errores"));
    }
}

fn tail(path: &std::path::Path, lines: usize) -> String {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\r\n")
}

#[tauri::command]
pub fn diagnostico(app: AppHandle) -> String {
    let settings = crate::settings::load(&app);
    let ai = if !settings.ai_on {
        "apagada".to_string()
    } else if settings.ai_url.starts_with("claude:") {
        "Tu Claude".to_string()
    } else {
        settings.ai_url.split("://").nth(1).and_then(|rest| rest.split('/').next()).unwrap_or("?").to_string()
    };
    let mics = crate::mic::mic_list().len();
    let crashes = dir().map(|d| d.join("cierres.txt")).filter(|p| p.exists()).map(|p| tail(&p, 10)).unwrap_or_default();
    let errors = file().filter(|p| p.exists()).map(|p| tail(&p, 15)).unwrap_or_default();
    let report = format!(
        "Cocuyo v{}\r\nWindows: {}\r\nIdioma: {}\r\nIA: {}\r\nMicrófonos: {mics}\r\nRemoto: {}\r\n\r\nÚltimos errores:\r\n{}\r\n\r\nÚltimos cierres:\r\n{}\r\n",
        env!("CARGO_PKG_VERSION"),
        windows_version(),
        settings.language,
        ai,
        if settings.remote_on { "prendido" } else { "apagado" },
        if errors.is_empty() { "ninguno".into() } else { errors },
        if crashes.is_empty() { "ninguno".into() } else { crashes },
    );
    crate::clip::ignore_own_changes();
    crate::inject::set_clipboard_text(&report);
    report
}

#[cfg(test)]
mod limpio {
    use super::*;

    #[test]
    fn the_log_never_keeps_secrets_in_links_or_control_characters() {
        assert_eq!(clean("falló https://api.x.com/v1?key=SECRETO otra vez"), "falló https://api.x.com/v1?… otra vez");
        assert_eq!(clean("línea\nnueva\u{1b}[31m"), "línea nueva [31m");
        assert_eq!(clean("sin enlaces"), "sin enlaces");
    }
}
