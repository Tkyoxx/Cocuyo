// abrir y cerrar apps. cerrar es pedirlo como la X, nunca a la fuerza

use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use windows::core::{w, HSTRING};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use crate::{dialogs, intent};

pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const FRESH_FOR: Duration = Duration::from_secs(600);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct App {
    pub name: String,
    pub id: String,
    #[serde(default)]
    pub custom: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct StartApp {
    name: String,
    #[serde(rename = "AppID")]
    app_id: String,
}

static CACHE: Mutex<Option<(Instant, Vec<App>)>> = Mutex::new(None);

fn custom_file(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_config_dir().ok()?.join("apps.json"))
}

fn load_custom(app: &AppHandle) -> Vec<App> {
    custom_file(app)
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn save_custom(app: &AppHandle, apps: &[App]) -> Result<(), String> {
    let path = custom_file(app).ok_or("No se encontró la carpeta de configuración.")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(apps).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

fn scan_start_menu() -> Vec<App> {
    let script = "[Console]::OutputEncoding=[Text.Encoding]::UTF8; Get-StartApps | ConvertTo-Json -Compress";
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    let Ok(output) = output else { return Vec::new() };
    let found: Vec<StartApp> = serde_json::from_slice(&output.stdout).unwrap_or_default();
    let mut seen = std::collections::HashSet::new();
    found
        .into_iter()
        .filter(|a| {
            let name = a.name.to_lowercase();
            let junk = ["uninstall", "desinstalar", "readme", "license", "licencia", "official site", "website", "sitio web", "manual", "http", "www.", "documentation", "release notes"];
            name.chars().next().is_some_and(char::is_alphanumeric)
                && !a.app_id.starts_with("http")
                && !a.app_id.to_lowercase().ends_with(".url")
                && !junk.iter().any(|w| name.contains(w))
        })
        .filter(|a| seen.insert(a.name.to_lowercase()))
        .map(|a| App { name: a.name, id: a.app_id, custom: false })
        .collect()
}

pub fn list(app: &AppHandle) -> Vec<App> {
    let mut cache = CACHE.lock().unwrap();
    let stale = cache.as_ref().map_or(true, |(at, _)| at.elapsed() > FRESH_FOR);
    if stale {
        *cache = Some((Instant::now(), scan_start_menu()));
    }
    let mut all = load_custom(app);
    all.extend(cache.as_ref().map(|(_, apps)| apps.clone()).unwrap_or_default());
    all.sort_by_key(|a| a.name.to_lowercase());
    all
}

pub fn open(target: &str) -> Result<(), String> {
    let result = unsafe { ShellExecuteW(None, w!("open"), &HSTRING::from(target), None, None, SW_SHOWNORMAL) };
    if result.0 as isize > 32 {
        Ok(())
    } else {
        Err(format!("Windows no pudo abrir «{target}»."))
    }
}

pub fn launch(app: &App) -> Result<(), String> {
    if app.custom {
        open(&app.id)
    } else {
        open(&format!("shell:AppsFolder\\{}", app.id))
    }
}

pub fn find(app: &AppHandle, spoken: &str, enough: f32) -> Option<App> {
    list(app)
        .into_iter()
        .map(|a| (intent::similarity(spoken, &a.name), a))
        .filter(|(score, _)| *score >= enough)
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, a)| a)
}

#[tauri::command]
pub async fn apps_list(app: AppHandle) -> Vec<App> {
    list(&app)
}

#[tauri::command]
pub fn app_launch(app: AppHandle, id: String) -> Result<(), String> {
    let found = list(&app).into_iter().find(|a| a.id == id).ok_or("Esa app ya no está en la lista.")?;
    launch(&found)
}

#[tauri::command]
pub async fn app_add(app: AppHandle) -> Result<Option<App>, String> {
    let Some(path) = dialogs::pick_program() else { return Ok(None) };
    let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let added = App { name, id: path.display().to_string(), custom: true };
    let mut custom = load_custom(&app);
    custom.retain(|a| a.id != added.id);
    custom.push(added.clone());
    save_custom(&app, &custom)?;
    Ok(Some(added))
}

#[tauri::command]
pub fn app_remove(app: AppHandle, id: String) -> Result<(), String> {
    let mut custom = load_custom(&app);
    custom.retain(|a| a.id != id);
    save_custom(&app, &custom)
}

#[tauri::command]
pub fn app_rename(app: AppHandle, id: String, name: String) -> Result<(), String> {
    let mut custom = load_custom(&app);
    for a in custom.iter_mut().filter(|a| a.id == id) {
        a.name = name.trim().to_string();
    }
    save_custom(&app, &custom)
}

fn top_windows() -> Vec<(isize, u32, bool)> {
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowTextLengthW, GetWindowThreadProcessId};

    unsafe extern "system" fn each(hwnd: HWND, list: LPARAM) -> BOOL {
        let list = &mut *(list.0 as *mut Vec<(isize, u32, bool)>);
        let mut process = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut process));
        list.push((hwnd.0 as isize, process, GetWindowTextLengthW(hwnd) > 0));
        BOOL(1)
    }
    let mut list = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(&mut list as *mut _ as isize));
    }
    list
}

fn image_of(process: u32) -> Option<String> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process).ok()?;
        let mut path = [0u16; 520];
        let mut len = path.len() as u32;
        let named = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, windows::core::PWSTR(path.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(handle);
        named.then(|| String::from_utf16_lossy(&path[..len as usize]))
    }
}

pub fn close(name: &str) -> Result<String, String> {
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::System::Threading::GetCurrentProcessId;
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId, PostMessageW, WM_CLOSE};
    const NOT_APPS: [&str; 12] = ["explorer", "dwm", "csrss", "winlogon", "svchost", "searchui", "searchapp", "shellexperiencehost", "startmenuexperiencehost", "textinputhost", "applicationframehost", "systemsettings"];
    let own = unsafe { GetCurrentProcessId() };
    let ask = |hwnd: isize| unsafe {
        let _ = PostMessageW(Some(HWND(hwnd as _)), WM_CLOSE, WPARAM(0), LPARAM(0));
    };

    if matches!(name, "esto" | "eso" | "ventana" | "esta ventana" | "this" | "this window" | "isso" | "janela") {
        let front = unsafe { GetForegroundWindow() };
        let mut process = 0;
        unsafe { GetWindowThreadProcessId(front, Some(&mut process)) };
        let stem = image_of(process).and_then(|path| std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().to_lowercase())).unwrap_or_default();
        if front.is_invalid() || process == own || NOT_APPS.contains(&stem.as_str()) {
            return Err("No hay ninguna ventana delante que cerrar.".into());
        }
        ask(front.0 as isize);
        return Ok("Cerrando la ventana.".into());
    }

    let windows = top_windows();
    let mut best: Option<(f32, String, String)> = None;
    let mut seen = Vec::new();
    for (_, process, titled) in &windows {
        if !titled || *process == own || seen.contains(process) {
            continue;
        }
        seen.push(*process);
        let Some(path) = image_of(*process) else { continue };
        let stem = std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        if NOT_APPS.contains(&stem.to_lowercase().as_str()) {
            continue;
        }
        let described = crate::system::description(&path).unwrap_or_default();
        let score = intent::similarity(name, &stem).max(intent::similarity(name, &described)).max(intent::covers(name, &described));
        if score >= 0.75 && best.as_ref().is_none_or(|(top, _, _)| score > *top) {
            best = Some((score, path, if described.is_empty() { stem } else { described }));
        }
    }
    let Some((_, path, label)) = best else {
        return Err(format!("No veo ninguna app abierta llamada «{name}»."));
    };
    for (hwnd, process, _) in &windows {
        if image_of(*process).as_deref() == Some(path.as_str()) {
            ask(*hwnd);
        }
    }
    Ok(format!("Cerrando {label}."))
}

pub fn close_spoken(spoken: &str) -> Option<Result<String, String>> {
    let clean = intent::normalize(spoken);
    const LEADS: [&str; 11] = ["cierra ", "cierrame ", "cierren ", "cerrar ", "cierre ", "cierrate ", "close ", "fecha ", "feche ", "fechar ", "quit "];
    let lead = LEADS.iter().find(|lead| clean.starts_with(**lead))?;
    let mut name = clean[lead.len()..].trim();
    for filler in ["la aplicacion ", "la app ", "el programa ", "el ", "la ", "los ", "las ", "mi ", "the ", "o ", "a "] {
        name = name.strip_prefix(filler).unwrap_or(name);
    }
    if name.is_empty() || name.starts_with("sesion") || name.starts_with("session") || name.starts_with("sessao") {
        return None;
    }
    Some(close(name))
}

#[cfg(test)]
mod close_tests {
    use super::*;

    #[test]
    fn closing_is_only_what_asks_to_close_an_app() {
        assert!(close_spoken("pon música").is_none());
        assert!(close_spoken("cierra sesión").is_none());
        assert!(close_spoken("cierra").is_none());
        assert_eq!(close_spoken("ciérrame la app programaquenoexiste123"), Some(Err("No veo ninguna app abierta llamada «programaquenoexiste123».".into())));
    }
}
