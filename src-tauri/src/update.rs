// se actualiza sola desde github: baja el exe nuevo, se cambia por el viejo y vuelve a abrir. si algo falla se queda con la que tenia

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter};

use crate::{install, overlay, placement, settings, voice};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const LATEST: &str = "https://api.github.com/repos/Tkyoxx/Cocuyo/releases/latest";
const DOWNLOADS: &str = "https://github.com/Tkyoxx/Cocuyo/releases/download/";
const PORTABLE: &str = "/Cocuyo.exe";
const UPDATED: &str = "--actualizado=";
const EVERY: Duration = Duration::from_secs(6 * 3600);

#[derive(Clone, Serialize)]
pub struct Update {
    version: String,
    notes: String,
    #[serde(skip)]
    url: String,
    #[serde(skip)]
    sha256: String,
}

pub fn current() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

fn newer(theirs: &str, ours: &str) -> bool {
    let numbers = |version: &str| -> Vec<u32> { version.trim_start_matches(['v', 'V']).split(['.', '-']).map_while(|part| part.parse().ok()).collect() };
    numbers(theirs) > numbers(ours)
}

pub fn check() -> Option<Update> {
    let output = Command::new("curl.exe")
        .args(["--silent", "--location", "--max-time", "10", "--header", "User-Agent: Cocuyo", "--header", "Accept: application/vnd.github+json", LATEST])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    let own = || std::env::current_exe().ok().and_then(|exe| voice::sha256_of(&exe));
    read(&serde_json::from_slice(&output.stdout).ok()?, current(), own)
}

fn read(release: &Value, ours: &str, own: impl FnOnce() -> Option<String>) -> Option<Update> {
    let version = release["tag_name"].as_str()?.trim_start_matches(['v', 'V']).to_string();
    let plain = !version.is_empty() && version.len() <= 20 && version.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    let same = !newer(&version, ours) && !newer(ours, &version);
    if !plain || !(newer(&version, ours) || same) {
        return None;
    }
    let (url, sha256) = release["assets"].as_array()?.iter().find_map(|asset| {
        let url = asset["browser_download_url"].as_str()?;
        let sha256 = asset["digest"].as_str()?.strip_prefix("sha256:")?;
        let ours = url.starts_with(DOWNLOADS) && url.ends_with(PORTABLE) && !url.contains("..");
        (ours && sha256.len() == 64 && sha256.chars().all(|c| c.is_ascii_hexdigit())).then(|| (url.to_string(), sha256.to_lowercase()))
    })?;
    if same && own().is_none_or(|mine| mine.eq_ignore_ascii_case(&sha256)) {
        return None;
    }
    let notes: String = release["body"].as_str().unwrap_or("").chars().take(600).collect();
    Some(Update { version, notes: notes.trim().to_string(), url, sha256 })
}

fn fetch(update: &Update) -> Result<PathBuf, String> {
    let failed = || "No se pudo descargar la versión nueva. Revisa tu conexión.".to_string();
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let fresh = exe.with_extension("nuevo.exe");
    let _ = std::fs::remove_file(&fresh);
    let done = Command::new("curl.exe")
        .args(["--silent", "--show-error", "--location", "--fail", "--max-time", "600", "--output"])
        .arg(&fresh)
        .arg(&update.url)
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|_| failed())?;
    let whole = done.success() && std::fs::metadata(&fresh).is_ok_and(|m| m.len() > 1_000_000) && voice::sha256_of(&fresh).is_some_and(|sha| sha == update.sha256);
    if !whole {
        let _ = std::fs::remove_file(&fresh);
        return Err(failed());
    }
    Ok(fresh)
}

fn swap(current: &Path, fresh: &Path) -> Result<(), String> {
    let old = current.with_extension("viejo.exe");
    let _ = std::fs::remove_file(&old);
    std::fs::rename(current, &old).map_err(|e| format!("No se pudo actualizar Cocuyo: {e}"))?;
    if let Err(e) = std::fs::rename(fresh, current) {
        let _ = std::fs::rename(&old, current);
        return Err(format!("No se pudo actualizar Cocuyo: {e}"));
    }
    Ok(())
}

fn unswap(current: &Path) {
    let old = current.with_extension("viejo.exe");
    if old.is_file() {
        let _ = std::fs::remove_file(current);
        let _ = std::fs::rename(&old, current);
    }
}

fn install(app: &AppHandle, update: &Update) -> Result<(), String> {
    if cfg!(debug_assertions) {
        return Err("Esta es una compilación de desarrollo: no se actualiza sola.".into());
    }
    let fresh = fetch(update)?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let _ = app.emit_to(overlay::LABEL, "update", update.clone());
    std::thread::sleep(Duration::from_millis(2500));
    swap(&exe, &fresh)?;
    let relay = Command::new(&exe).arg(format!("{}{}", install::AFTER, std::process::id())).arg(format!("{UPDATED}{}", update.version)).spawn();
    if let Err(e) = relay {
        unswap(&exe);
        return Err(format!("No se pudo abrir la versión nueva: {e}"));
    }
    app.exit(0);
    Ok(())
}

pub fn tidy() {
    let Ok(exe) = std::env::current_exe() else { return };
    let _ = std::fs::remove_file(exe.with_extension("viejo.exe"));
    let _ = std::fs::remove_file(exe.with_extension("nuevo.exe"));
    if updated().is_some() {
        install::refresh();
    }
}

fn updated() -> Option<String> {
    std::env::args().find_map(|arg| arg.strip_prefix(UPDATED).map(str::to_owned))
}

pub fn watch(app: AppHandle) {
    if cfg!(debug_assertions) {
        return;
    }
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(45));
        loop {
            if settings::load(&app).check_updates {
                if let Some(found) = check() {
                    while placement::fullscreen() {
                        std::thread::sleep(Duration::from_secs(60));
                    }
                    if settings::load(&app).check_updates {
                        let _ = install(&app, &found);
                    }
                }
            }
            std::thread::sleep(EVERY);
        }
    });
}

#[tauri::command]
pub fn version() -> &'static str {
    current()
}

#[tauri::command]
pub fn update_done() -> Option<String> {
    updated()
}

#[tauri::command]
pub async fn update_check() -> Option<Update> {
    check()
}

#[tauri::command]
pub async fn update_install(app: AppHandle) -> Result<(), String> {
    let found = check().ok_or("Estás al día.")?;
    install(&app, &found)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA: &str = "4a27a61b06e688308a5c17333008b6df52fd5780fa7a313a9ce8fb4a80dfb9ac";

    #[test]
    fn versions_compare_by_number() {
        assert!(newer("0.2.0", "0.1.0") && newer("v0.10.0", "0.9.3") && newer("1.0", "0.9.9"));
        assert!(!newer("0.1.0", "0.1.0") && !newer("0.1.0", "0.2.0") && !newer("basura", "0.1.0"));
    }

    #[test]
    fn only_a_newer_release_with_our_exe_and_its_hash_counts() {
        let release = serde_json::json!({
            "tag_name": "v0.2.0",
            "body": "Mando, voces y clima.",
            "assets": [
                { "browser_download_url": "https://github.com/Tkyoxx/Cocuyo/releases/download/v0.2.0/Cocuyo_0.2.0_x64-setup.exe", "digest": format!("sha256:{SHA}") },
                { "browser_download_url": "https://github.com/Tkyoxx/Cocuyo/releases/download/v0.2.0/Cocuyo.exe", "digest": format!("sha256:{}", SHA.to_uppercase()) }
            ]
        });
        let update = read(&release, "0.1.0", || None).unwrap();
        assert_eq!((update.version.as_str(), update.notes.as_str(), update.sha256.as_str()), ("0.2.0", "Mando, voces y clima.", SHA));
        assert!(update.url.ends_with("/v0.2.0/Cocuyo.exe"));
        assert!(read(&release, "0.2.0", || Some(SHA.into())).is_none());

        let fixed = read(&release, "0.2.0", || Some("otro".into())).unwrap();
        assert_eq!(fixed.version, "0.2.0");
        assert!(read(&release, "0.2.0", || None).is_none());
        assert!(read(&release, "0.3.0", || Some("otro".into())).is_none());
        let elsewhere = serde_json::json!({ "tag_name": "v9.0.0", "assets": [{ "browser_download_url": "https://ejemplo.com/Cocuyo.exe", "digest": format!("sha256:{SHA}") }] });
        assert!(read(&elsewhere, "0.1.0", || None).is_none());
        let unsigned = serde_json::json!({ "tag_name": "v9.0.0", "assets": [{ "browser_download_url": "https://github.com/Tkyoxx/Cocuyo/releases/download/v9.0.0/Cocuyo.exe" }] });
        assert!(read(&unsigned, "0.1.0", || None).is_none());
        let odd = serde_json::json!({ "tag_name": "v9 --uninstall", "assets": [{ "browser_download_url": "https://github.com/Tkyoxx/Cocuyo/releases/download/v9/Cocuyo.exe", "digest": format!("sha256:{SHA}") }] });
        assert!(read(&odd, "0.1.0", || None).is_none());
    }

    #[test]
    fn the_new_exe_takes_the_place_of_the_old_one() {
        let dir = std::env::temp_dir().join(format!("cocuyo-cambio-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (current, fresh) = (dir.join("Cocuyo.exe"), dir.join("Cocuyo.nuevo.exe"));
        std::fs::write(&current, "vieja").unwrap();
        std::fs::write(&fresh, "nueva").unwrap();
        swap(&current, &fresh).unwrap();
        assert_eq!(std::fs::read_to_string(&current).unwrap(), "nueva");
        assert_eq!(std::fs::read_to_string(dir.join("Cocuyo.viejo.exe")).unwrap(), "vieja");
        assert!(!fresh.exists());

        unswap(&current);
        assert_eq!(std::fs::read_to_string(&current).unwrap(), "vieja");

        assert!(swap(&current, &dir.join("no-existe.exe")).is_err());
        assert_eq!(std::fs::read_to_string(&current).unwrap(), "vieja");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
