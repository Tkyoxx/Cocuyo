// se actualiza sola desde github: la baja callada, te pregunta, se cambia por la vieja y vuelve a abrir. si dices mas tarde, se pone al reiniciar
// o cuando no estes usando el pc. si algo falla se queda con la que tenia

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

use crate::{install, overlay, placement, settings, voice};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const LATEST: &str = "https://api.github.com/repos/Tkyoxx/Cocuyo/releases/latest";
const DOWNLOADS: &str = "https://github.com/Tkyoxx/Cocuyo/releases/download/";
const COUNT: &str = "https://github.com/Tkyoxx/Cocuyo/releases/download/contador/instalacion";
const PORTABLE: &str = "/Cocuyo.exe";
const UPDATED: &str = "--actualizado=";
const EVERY: Duration = Duration::from_secs(6 * 3600);
const QUIET: u32 = 0x0000_4000;

#[derive(Clone, Serialize)]
pub struct Update {
    version: String,
    notes: String,
    actual: String,
    tamano: u64,
    novedades: Vec<(String, String)>,
    #[serde(skip)]
    url: String,
    #[serde(skip)]
    sha256: String,
}

fn plain_text(line: &str) -> String {
    let mut out = String::new();
    let mut rest = line.replace("**", "").replace('`', "");
    while let Some(open) = rest.find('[') {
        let Some(close) = rest[open..].find("](").map(|at| open + at) else { break };
        let Some(end) = rest[close..].find(')').map(|at| close + at) else { break };
        out.push_str(&rest[..open]);
        out.push_str(&rest[open + 1..close]);
        rest = rest[end + 1..].to_string();
    }
    out.push_str(&rest);
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn short(text: &str) -> String {
    let sentence = text.find(". ").map_or(text, |end| &text[..=end]);
    if sentence.chars().count() <= 150 {
        return sentence.to_string();
    }
    let cut: String = sentence.chars().take(150).collect();
    format!("{}…", cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head).trim_end_matches([',', ';', ':']))
}

pub fn highlights(notes: &str) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = Vec::new();
    for line in notes.lines().map(str::trim) {
        if let Some(title) = line.strip_prefix("**").and_then(|rest| rest.split_once("**")).map(|(title, _)| title.trim()) {
            if !title.is_empty() && !["por dentro", "under the hood"].contains(&title.to_lowercase().as_str()) {
                found.push((plain_text(title), String::new()));
            }
        } else if let Some(item) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
            match found.last_mut() {
                Some((_, detail)) if detail.is_empty() => *detail = short(&plain_text(item)),
                None => found.push((plain_text(item).chars().take(70).collect(), String::new())),
                _ => {}
            }
        }
    }
    found.retain(|(title, _)| !title.is_empty());
    found.truncate(4);
    found
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
    let (url, sha256, tamano) = release["assets"].as_array()?.iter().find_map(|asset| {
        let url = asset["browser_download_url"].as_str()?;
        let sha256 = asset["digest"].as_str()?.strip_prefix("sha256:")?;
        let ours = url.starts_with(DOWNLOADS) && url.ends_with(PORTABLE) && !url.contains("..");
        (ours && sha256.len() == 64 && sha256.chars().all(|c| c.is_ascii_hexdigit())).then(|| (url.to_string(), sha256.to_lowercase(), asset["size"].as_u64().unwrap_or(0)))
    })?;
    if same && own().is_none_or(|mine| mine.eq_ignore_ascii_case(&sha256)) {
        return None;
    }
    let notes: String = release["body"].as_str().unwrap_or("").chars().take(6000).collect();
    let novedades = highlights(&notes);
    Some(Update { version, notes: notes.trim().chars().take(600).collect(), actual: ours.to_string(), tamano, novedades, url, sha256 })
}

fn waiting(exe: &Path) -> (PathBuf, PathBuf) {
    (exe.with_extension("nuevo.exe"), exe.with_extension("nuevo.txt"))
}

fn fetch(app: Option<&AppHandle>, update: &Update) -> Result<PathBuf, String> {
    let failed = || "No se pudo descargar la versión nueva. Revisa tu conexión.".to_string();
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let (fresh, note) = waiting(&exe);
    if voice::sha256_of(&fresh).is_some_and(|sha| sha == update.sha256) {
        let _ = std::fs::write(&note, format!("{}\n{}", update.version, update.sha256));
        return Ok(fresh);
    }
    let _ = std::fs::remove_file(&fresh);
    let mut child = Command::new("curl.exe")
        .args(["--silent", "--show-error", "--location", "--fail", "--max-time", "600", "--output"])
        .arg(&fresh)
        .arg(&update.url)
        .creation_flags(CREATE_NO_WINDOW | if app.is_some() { 0 } else { QUIET })
        .spawn()
        .map_err(|_| failed())?;
    let done = loop {
        if let Ok(Some(status)) = child.try_wait() {
            break status.success();
        }
        if let (Some(app), true) = (app, update.tamano > 0) {
            let got = std::fs::metadata(&fresh).map_or(0, |m| m.len());
            let _ = app.emit_to(overlay::LABEL, "update-progress", (got as f64 / update.tamano as f64).min(1.0));
        }
        std::thread::sleep(Duration::from_millis(if app.is_some() { 200 } else { 500 }));
    };
    let whole = done && std::fs::metadata(&fresh).is_ok_and(|m| m.len() > 1_000_000) && voice::sha256_of(&fresh).is_some_and(|sha| sha == update.sha256);
    if !whole {
        let _ = std::fs::remove_file(&fresh);
        return Err(failed());
    }
    let _ = std::fs::write(&note, format!("{}\n{}", update.version, update.sha256));
    Ok(fresh)
}

fn ready_at_start(exe: &Path) -> Option<String> {
    let (fresh, note) = waiting(exe);
    let note = std::fs::read_to_string(note).ok()?;
    let (version, sha) = note.trim().split_once('\n')?;
    let plain = version.len() <= 20 && version.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    (plain && newer(version, current()) && voice::sha256_of(&fresh).is_some_and(|got| got.eq_ignore_ascii_case(sha.trim()))).then(|| version.to_string())
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
    let fresh = fetch(Some(app), update)?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let _ = app.emit_to(overlay::LABEL, "update", update.clone());
    std::thread::sleep(Duration::from_millis(1800));
    swap(&exe, &fresh)?;
    let _ = std::fs::remove_file(waiting(&exe).1);
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
    if !cfg!(debug_assertions) && updated().is_none() {
        if let Some(version) = ready_at_start(&exe) {
            let (fresh, note) = waiting(&exe);
            if swap(&exe, &fresh).is_ok() {
                let _ = std::fs::remove_file(note);
                let args: Vec<String> = std::env::args().skip(1).filter(|arg| !arg.starts_with(install::AFTER) && !arg.starts_with(UPDATED)).collect();
                let relay = Command::new(&exe).args(args).arg(format!("{}{}", install::AFTER, std::process::id())).arg(format!("{UPDATED}{version}")).spawn();
                if relay.is_ok() {
                    std::process::exit(0);
                }
                unswap(&exe);
            }
        }
    }
    let (fresh, note) = waiting(&exe);
    if ready_at_start(&exe).is_none() {
        let _ = std::fs::remove_file(fresh);
        let _ = std::fs::remove_file(note);
    }
    if updated().is_some() {
        install::refresh();
    }
}

fn updated() -> Option<String> {
    std::env::args().find_map(|arg| arg.strip_prefix(UPDATED).map(str::to_owned))
}

static CHOICE: std::sync::Mutex<Option<std::sync::mpsc::Sender<bool>>> = std::sync::Mutex::new(None);

#[tauri::command]
pub fn update_choice(now: bool) {
    if let Some(answer) = CHOICE.lock().unwrap().take() {
        let _ = answer.send(now);
    }
}

fn wants_it_now(app: &AppHandle, found: &Update) -> Option<bool> {
    let (answer, choice) = std::sync::mpsc::channel();
    *CHOICE.lock().unwrap() = Some(answer);
    let _ = app.emit_to(crate::overlay::LABEL, "update-offer", found);
    let now = choice.recv_timeout(Duration::from_secs(5 * 60)).ok();
    CHOICE.lock().unwrap().take();
    now
}

fn idle_for() -> Duration {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    let mut info = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    if !unsafe { GetLastInputInfo(&mut info) }.as_bool() {
        return Duration::ZERO;
    }
    let now = unsafe { windows::Win32::System::SystemInformation::GetTickCount() };
    Duration::from_millis(now.wrapping_sub(info.dwTime) as u64)
}

fn busy() -> bool {
    placement::fullscreen() || crate::grabadora::recording() || crate::cable::heard()
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
                    if fetch(None, &found).is_ok() {
                        while busy() {
                            std::thread::sleep(Duration::from_secs(60));
                        }
                        let answer = if settings::load(&app).check_updates { wants_it_now(&app, &found) } else { Some(false) };
                        if answer == Some(true) {
                            let _ = install(&app, &found);
                        } else if answer.is_none() {
                            let until = Instant::now() + EVERY;
                            while Instant::now() < until {
                                std::thread::sleep(Duration::from_secs(60));
                                if idle_for() > Duration::from_secs(8 * 60) && !busy() {
                                    let _ = install(&app, &found);
                                    break;
                                }
                            }
                            continue;
                        }
                    }
                }
            }
            std::thread::sleep(EVERY);
        }
    });
}

pub fn count_install(app: AppHandle) {
    if cfg!(debug_assertions) {
        return;
    }
    let Some(mark) = app.path().app_config_dir().ok().map(|dir| dir.join("contado")) else { return };
    if mark.exists() {
        return;
    }
    std::thread::spawn(move || {
        for wait in [5, 60, 300, 1800] {
            std::thread::sleep(Duration::from_secs(wait));
            if !settings::load(&app).count_me {
                return;
            }
            let counted = Command::new("curl.exe")
                .args(["--silent", "--fail", "--location", "--max-time", "20", "--output", "NUL", "--user-agent", "Cocuyo", COUNT])
                .creation_flags(CREATE_NO_WINDOW)
                .status()
                .is_ok_and(|status| status.success());
            if counted {
                if let Some(dir) = mark.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                let _ = std::fs::write(&mark, b"");
                return;
            }
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
    tauri::async_runtime::spawn_blocking(move || {
        let found = check().ok_or("Estás al día.")?;
        install(&app, &found)
    })
    .await
    .map_err(|e| e.to_string())?
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
        assert_eq!(update.actual, "0.1.0");
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
    fn the_changelog_turns_into_a_few_highlights() {
        let notes = "## 0.4.2\n\n**WhatsApp con stickers** (Ajustes → WhatsApp)\n- Los stickers y las fotos salen dentro del chat, y las notas de voz se escuchan ahí.\n- Enter envía.\n\n**La grabadora oye el PC** \n- Sigue al parlante de Windows: [más](https://x.y).\n\n**Por dentro**\n- Cosas.";
        let found = highlights(notes);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0], ("WhatsApp con stickers".to_string(), "Los stickers y las fotos salen dentro del chat, y las notas de voz se escuchan ahí.".to_string()));
        assert_eq!(found[1].1, "Sigue al parlante de Windows: más.");
        assert!(highlights("Full Changelog: v0.4.0...v0.4.1").is_empty());
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
