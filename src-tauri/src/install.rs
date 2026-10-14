// instalarse y desinstalarse sin pedir admin

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use tauri::{AppHandle, Emitter};
use windows::core::{w, Interface, HSTRING, PCWSTR};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Com::{CoCreateInstance, IPersistFile, CLSCTX_INPROC_SERVER};
use windows::Win32::System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE};
use windows::Win32::System::Registry::{
    RegDeleteTreeW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_DWORD, REG_SZ,
};
use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
use windows::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, IDYES, MB_ICONINFORMATION, MB_ICONQUESTION, MB_OK, MB_YESNO,
};

use crate::apps::CREATE_NO_WINDOW;
use crate::{dialogs, overlay, settings, system};

const UNINSTALL_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Cocuyo");
const DATA_FOLDER: &str = "io.github.tkyoxx.cocuyo";

fn env_dir(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).map(PathBuf::from)
}

fn install_dir() -> Option<PathBuf> {
    Some(env_dir("LOCALAPPDATA")?.join("Programs").join("Cocuyo"))
}

fn start_menu_link() -> Option<PathBuf> {
    Some(env_dir("APPDATA")?.join("Microsoft\\Windows\\Start Menu\\Programs\\Cocuyo.lnk"))
}

fn is_installed_copy() -> bool {
    let (Ok(exe), Some(dir)) = (std::env::current_exe(), install_dir()) else { return false };
    let Some(parent) = exe.parent() else { return false };
    parent.to_string_lossy().eq_ignore_ascii_case(&dir.to_string_lossy()) || parent.join("uninstall.exe").is_file()
}

#[tauri::command]
pub fn install_state(app: AppHandle) -> &'static str {
    if is_installed_copy() {
        "installed"
    } else if cfg!(debug_assertions) || settings::load(&app).portable {
        "portable"
    } else {
        "fresh"
    }
}

fn create_shortcut(target: PathBuf, link: PathBuf) -> Result<(), String> {
    dialogs::on_sta(move || unsafe {
        let shortcut: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        shortcut.SetPath(&HSTRING::from(target.as_os_str())).ok()?;
        shortcut.SetDescription(w!("Cocuyo")).ok()?;
        if let Some(dir) = target.parent() {
            shortcut.SetWorkingDirectory(&HSTRING::from(dir.as_os_str())).ok()?;
        }
        shortcut.cast::<IPersistFile>().ok()?.Save(&HSTRING::from(link.as_os_str()), true).ok()
    })
    .ok_or_else(|| "No se pudo crear el acceso directo.".to_string())
}

fn set_string(name: PCWSTR, value: &str) -> Result<(), String> {
    let wide: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
    unsafe {
        RegSetKeyValueW(HKEY_CURRENT_USER, UNINSTALL_KEY, name, REG_SZ.0, Some(wide.as_ptr().cast()), (wide.len() * 2) as u32)
            .ok()
            .map_err(|e| e.to_string())
    }
}

fn register_uninstaller(exe: &Path) -> Result<(), String> {
    set_string(w!("DisplayName"), "Cocuyo")?;
    set_string(w!("DisplayVersion"), env!("CARGO_PKG_VERSION"))?;
    set_string(w!("Publisher"), "Tkyoxx")?;
    set_string(w!("DisplayIcon"), &exe.display().to_string())?;
    set_string(w!("InstallLocation"), &exe.parent().map(|p| p.display().to_string()).unwrap_or_default())?;
    set_string(w!("UninstallString"), &format!("\"{}\" --uninstall", exe.display()))?;
    let one = 1u32;
    unsafe {
        RegSetKeyValueW(HKEY_CURRENT_USER, UNINSTALL_KEY, w!("NoModify"), REG_DWORD.0, Some((&one as *const u32).cast()), 4)
            .ok()
            .map_err(|e| e.to_string())
    }
}

pub fn copy_and_link(dir: &Path, link: &Path) -> Result<PathBuf, String> {
    let source = std::env::current_exe().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dir).map_err(|e| format!("No se pudo crear {}: {e}", dir.display()))?;
    let target = dir.join("Cocuyo.exe");
    std::fs::copy(&source, &target).map_err(|e| format!("No se pudo copiar la app: {e}"))?;
    create_shortcut(target.clone(), link.to_path_buf())?;
    Ok(target)
}

#[tauri::command]
pub async fn install_run(app: AppHandle) -> Result<(), String> {
    let dir = install_dir().ok_or("No se encontró la carpeta de tu usuario.")?;
    let link = start_menu_link().ok_or("No se encontró el menú Inicio.")?;
    let step = |index: u8| {
        let _ = app.emit_to(overlay::LABEL, "install-step", index);
        std::thread::sleep(Duration::from_millis(420));
    };

    let source = std::env::current_exe().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("No se pudo crear {}: {e}", dir.display()))?;
    let target = dir.join("Cocuyo.exe");
    std::fs::copy(&source, &target).map_err(|e| format!("No se pudo copiar la app: {e}"))?;
    step(0);
    create_shortcut(target.clone(), link)?;
    step(1);
    register_uninstaller(&target)?;
    step(2);
    Ok(())
}

#[tauri::command]
pub fn install_relaunch(app: AppHandle) -> Result<(), String> {
    let target = install_dir().ok_or("No se encontró la carpeta de instalación.")?.join("Cocuyo.exe");
    Command::new(&target)
        .arg(format!("{AFTER}{}", std::process::id()))
        .spawn()
        .map_err(|e| format!("No se pudo abrir la copia instalada: {e}"))?;
    app.exit(0);
    Ok(())
}

pub(crate) const AFTER: &str = "--after=";

pub fn refresh() {
    if is_installed_copy() {
        let _ = set_string(w!("DisplayVersion"), env!("CARGO_PKG_VERSION"));
    }
}

pub fn wait_for_previous() {
    let Some(pid) = std::env::args().find_map(|arg| arg.strip_prefix(AFTER)?.parse::<u32>().ok()) else { return };
    unsafe {
        if let Ok(process) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
            WaitForSingleObject(process, 15_000);
            let _ = CloseHandle(process);
        }
    }
}

#[tauri::command]
pub fn install_skip(app: AppHandle) -> Result<(), String> {
    let mut config = settings::load(&app);
    config.portable = true;
    settings::set_settings(app, config)
}

pub fn uninstall() {
    unsafe {
        if let Some(link) = start_menu_link() {
            let _ = std::fs::remove_file(link);
        }
        let _ = system::set_autostart(false);
        let _ = RegDeleteTreeW(HKEY_CURRENT_USER, UNINSTALL_KEY);

        let language = env_dir("APPDATA")
            .and_then(|dir| std::fs::read(dir.join(DATA_FOLDER).join("ajustes.json")).ok())
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .and_then(|json| json["language"].as_str().map(str::to_owned))
            .unwrap_or_else(|| crate::cue::system_language().to_owned());
        crate::cue::configure(false, false, &language);
        let say = |text: &str| windows::core::HSTRING::from(crate::i18n::tr(text));
        let title = say("Desinstalar Cocuyo");

        let wipe = MessageBoxW(
            None,
            &say("¿Borrar también tus ajustes, tus tareas y la voz descargada (406 MB)?\n\nSi piensas volver a instalar Cocuyo, elige No."),
            &title,
            MB_YESNO | MB_ICONQUESTION,
        ) == IDYES;
        if wipe {
            for base in ["APPDATA", "LOCALAPPDATA"] {
                if let Some(dir) = env_dir(base) {
                    let _ = std::fs::remove_dir_all(dir.join(DATA_FOLDER));
                }
            }
        }

        if let Some(dir) = install_dir() {
            let _ = Command::new("cmd.exe")
                .raw_arg(format!(
                    "/c ping -n 3 127.0.0.1 >nul & taskkill /f /im Cocuyo.exe >nul 2>&1 & rmdir /s /q \"{}\"",
                    dir.display()
                ))
                .creation_flags(CREATE_NO_WINDOW)
                .spawn();
        }
        MessageBoxW(None, &say("Cocuyo se desinstaló."), &title, MB_OK | MB_ICONINFORMATION);
    }
}
