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

pub fn note(area: &str, detail: &str) {
    let Some(path) = file() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
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
        detail.replace(['\r', '\n'], " ")
    );
    if let Ok(mut out) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = std::io::Write::write_all(&mut out, line.as_bytes());
    }
}

pub fn tell(app: &AppHandle, said: &str) {
    let calm = crate::grabadora::nothing_in_front();
    let said = if calm {
        let _ = show();
        format!("{said} Te abrí el registro del error.")
    } else {
        format!("{said} Di «abre la carpeta de errores» para verlo.")
    };
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
