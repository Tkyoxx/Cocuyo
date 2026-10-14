// volumen, procesos, energia, iconos... el cajon de cosas de windows

use std::collections::HashMap;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use windows::core::{w, HSTRING};
use windows::Win32::Foundation::{CloseHandle, FILETIME, SIZE};
use windows::Win32::System::Registry::{RegDeleteKeyValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};
use windows::Win32::Graphics::Gdi::{
    DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER,
    DIB_RGB_COLORS,
};
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Power::SetSuspendState;
use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows::Win32::System::Shutdown::LockWorkStation;
use windows::Win32::System::SystemInformation::{GetTickCount64, GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::{
    GetSystemTimes, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Shell::{IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK};
use windows::Win32::UI::WindowsAndMessaging::{
    SystemParametersInfoW, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE, SPI_SETDESKWALLPAPER,
};

use crate::apps::{self, CREATE_NO_WINDOW};
use crate::intent::Change;
use crate::{dialogs, settings};

pub fn lock() -> Result<(), String> {
    unsafe { LockWorkStation().map_err(|e| e.to_string()) }
}

#[tauri::command]
pub fn power(action: String) -> Result<(), String> {
    let shutdown = |flag: &str| {
        Command::new("shutdown.exe")
            .args([flag, "/t", "0"])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    };
    match action.as_str() {
        "lock" => lock(),
        "sleep" => {
            unsafe {
                let _ = SetSuspendState(false, false, false);
            }
            Ok(())
        }
        "restart" => shutdown("/r"),
        "shutdown" => shutdown("/s"),
        _ => Err("Acción desconocida.".into()),
    }
}

#[tauri::command]
pub fn open_settings_page(page: String) -> Result<(), String> {
    if !page.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err("Página no válida.".into());
    }
    apps::open(&format!("ms-settings:{page}"))
}

#[tauri::command]
pub fn open_capture_dir(app: AppHandle) -> Result<(), String> {
    let dir = settings::capture_dir(&app, &settings::load(&app)).ok_or("No hay carpeta de capturas.")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    apps::open(&dir.display().to_string())
}

pub fn set_autostart(on: bool) -> Result<(), String> {
    let key = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
    let name = w!("Cocuyo");
    unsafe {
        if !on {
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, key, name);
            return Ok(());
        }
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let command: Vec<u16> = format!("\"{}\" --inicio", exe.display()).encode_utf16().chain(Some(0)).collect();
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            key,
            name,
            REG_SZ.0,
            Some(command.as_ptr().cast()),
            (command.len() * 2) as u32,
        )
        .ok()
        .map_err(|e| e.to_string())
    }
}

fn endpoint() -> windows::core::Result<IAudioEndpointVolume> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        devices.GetDefaultAudioEndpoint(eRender, eConsole)?.Activate(CLSCTX_ALL, None)
    }
}

fn microphone() -> windows::core::Result<IAudioEndpointVolume> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        devices.GetDefaultAudioEndpoint(windows::Win32::Media::Audio::eCapture, eConsole)?.Activate(CLSCTX_ALL, None)
    }
}

#[tauri::command]
pub fn mic_muted() -> Option<bool> {
    unsafe { microphone().ok()?.GetMute().ok().map(|muted| muted.as_bool()) }
}

#[tauri::command]
pub fn mic_mute(on: bool) -> Result<bool, String> {
    unsafe {
        let mic = microphone().map_err(|_| "No hay ningún micrófono conectado.".to_string())?;
        mic.SetMute(on, std::ptr::null()).map_err(|e| e.to_string())?;
        Ok(on)
    }
}

pub fn mic_spoken(spoken: &str) -> Option<Result<String, String>> {
    let clean = crate::intent::normalize(spoken);
    if !clean.split_whitespace().any(|word| matches!(word, "microfono" | "micro" | "mic" | "microphone" | "microfone")) {
        return None;
    }
    let off = ["silencia", "mutea", "apaga", "calla", "cierra", "corta", "mute", "desactiva", "quita"].iter().any(|verb| clean.split_whitespace().any(|word| crate::intent::close(word, verb)));
    let on = ["activa", "enciende", "prende", "abre", "unmute", "desmutea", "devuelve", "reactiva"].iter().any(|verb| clean.split_whitespace().any(|word| crate::intent::close(word, verb)));
    let on = on || clean.contains("quita el silencio") || clean.contains("quitale el silencio");
    match (on, off) {
        (true, _) => Some(mic_mute(false).map(|_| "Micrófono activado.".to_string())),
        (false, true) => Some(mic_mute(true).map(|_| "Micrófono silenciado.".to_string())),
        _ => None,
    }
}

pub(crate) fn process_names() -> Vec<String> {
    let mut names = Vec::new();
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return names };
        let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut more = Process32FirstW(snapshot, &mut entry).is_ok();
        while more {
            let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(0);
            names.push(String::from_utf16_lossy(&entry.szExeFile[..len]));
            more = Process32NextW(snapshot, &mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
    }
    names
}

fn cpu_spent() -> HashMap<String, u64> {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::Threading::GetProcessTimes;
    let mut spent: HashMap<String, u64> = HashMap::new();
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return spent };
        let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut more = Process32FirstW(snapshot, &mut entry).is_ok();
        while more {
            if let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, entry.th32ProcessID) {
                let (mut made, mut ended, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
                if GetProcessTimes(process, &mut made, &mut ended, &mut kernel, &mut user).is_ok() {
                    let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(0);
                    let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
                    let ticks = |t: FILETIME| u64::from(t.dwHighDateTime) << 32 | u64::from(t.dwLowDateTime);
                    *spent.entry(name.strip_suffix(".exe").unwrap_or(&name).to_string()).or_default() += ticks(kernel) + ticks(user);
                }
                let _ = CloseHandle(process);
            }
            more = Process32NextW(snapshot, &mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
    }
    spent
}

pub fn slow_spoken(spoken: &str) -> Option<Result<String, String>> {
    let clean = crate::intent::normalize(spoken);
    let about_speed = ["lento", "lenta", "pegado", "pegada", "trabado", "trabada", "laggeado", "lag", "consume", "consumiendo", "pesado", "pesada", "slow", "lagging"]
        .iter()
        .any(|word| clean.split_whitespace().any(|said| said == *word));
    let asking = ["que", "quien", "por", "cual", "porque", "why", "what", "o"].contains(&clean.split_whitespace().next().unwrap_or(""));
    if !about_speed || !asking {
        return None;
    }
    let before = cpu_spent();
    let wait = std::time::Duration::from_millis(600);
    std::thread::sleep(wait);
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get()) as f64;
    let mut busy: Vec<(String, f64)> = cpu_spent()
        .into_iter()
        .filter(|(name, _)| !matches!(name.as_str(), "Idle" | "System Idle Process" | "[System Process]" | "cocuyo" | "Cocuyo"))
        .map(|(name, now)| {
            let used = now.saturating_sub(before.get(&name).copied().unwrap_or(now));
            (name, used as f64 / 1e7 / wait.as_secs_f64() / cores * 100.0)
        })
        .collect();
    busy.sort_by(|a, b| b.1.total_cmp(&a.1));
    let memory = top_processes();
    let heavy = memory.first().map(|top| format!("Lo que más memoria ocupa es {}, con {:.1} GB.", top.name, top.bytes as f64 / 1e9));
    let said = match busy.first() {
        Some((name, percent)) if *percent >= 8.0 => {
            format!("Lo que más trabaja ahora es {name}, con el {percent:.0} % del procesador. {} Di «cierra {name}» si quieres cerrarlo.", heavy.unwrap_or_default())
        }
        _ => format!("Ahora mismo nadie está forzando el procesador. {}", heavy.unwrap_or_default()),
    };
    crate::cue::answer(&crate::i18n::tr(&said));
    Some(Ok(said))
}

#[derive(Serialize)]
pub struct Volume {
    level: f32,
    muted: bool,
}

#[tauri::command]
pub async fn volume_get() -> Option<Volume> {
    unsafe {
        let volume = endpoint().ok()?;
        Some(Volume { level: volume.GetMasterVolumeLevelScalar().ok()?, muted: volume.GetMute().ok()?.as_bool() })
    }
}

#[tauri::command]
pub async fn volume_set(level: Option<f32>, muted: Option<bool>) -> Result<(), String> {
    unsafe {
        let volume = endpoint().map_err(|e| e.to_string())?;
        if let Some(level) = level {
            volume.SetMasterVolumeLevelScalar(level.clamp(0.0, 1.0), std::ptr::null()).map_err(|e| e.to_string())?;
        }
        if let Some(muted) = muted {
            volume.SetMute(muted, std::ptr::null()).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

pub fn change_volume(change: Change) -> Option<u8> {
    unsafe {
        let volume = endpoint().ok()?;
        let now = (volume.GetMasterVolumeLevelScalar().ok()? * 100.0).round() as i16;
        let muted = volume.GetMute().ok()?.as_bool();
        let target = match change {
            Change::By(step) => now + i16::from(step),
            Change::Set(percent) => i16::from(percent),
            Change::Mute | Change::Unmute => {
                let mute = change == Change::Mute && !muted;
                volume.SetMute(mute, std::ptr::null()).ok()?;
                return Some(if mute { 0 } else { now as u8 });
            }
        }
        .clamp(0, 100);
        if muted {
            volume.SetMute(false, std::ptr::null()).ok()?;
        }
        volume.SetMasterVolumeLevelScalar(target as f32 / 100.0, std::ptr::null()).ok()?;
        Some(target as u8)
    }
}

#[tauri::command]
pub async fn media_key(action: String) {
    use crate::media::Transport;
    let (transport, vk) = match action.as_str() {
        "play" => (Transport::Toggle, 0xB3),
        "next" => (Transport::Next, 0xB0),
        "previous" => (Transport::Previous, 0xB1),
        _ => return,
    };
    if !crate::media::control(transport) {
        crate::inject::tap(vk);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    cpu: f32,
    threads: usize,
    memory_used: u64,
    memory_total: u64,
    disk_used: u64,
    disk_total: u64,
    uptime_secs: u64,
    top: Vec<Top>,
}

#[derive(Serialize)]
pub struct Top {
    name: String,
    bytes: u64,
    path: String,
}

static LAST_CPU: Mutex<Option<(u64, u64)>> = Mutex::new(None);

fn ticks(t: FILETIME) -> u64 {
    (t.dwHighDateTime as u64) << 32 | t.dwLowDateTime as u64
}

fn cpu_percent() -> f32 {
    let (mut idle, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
    unsafe {
        if GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)).is_err() {
            return 0.0;
        }
    }
    let now = (ticks(idle), ticks(kernel) + ticks(user));
    let mut last = LAST_CPU.lock().unwrap();
    let percent = match *last {
        Some((idle0, total0)) if now.1 > total0 => {
            100.0 * (1.0 - (now.0 - idle0) as f32 / (now.1 - total0) as f32)
        }
        _ => 0.0,
    };
    *last = Some(now);
    percent.clamp(0.0, 100.0)
}

fn image_path(process: u32) -> String {
    unsafe {
        let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process) else { return String::new() };
        let mut path = [0u16; 520];
        let mut len = path.len() as u32;
        let named = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, windows::core::PWSTR(path.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(handle);
        if named {
            String::from_utf16_lossy(&path[..len as usize])
        } else {
            String::new()
        }
    }
}

fn top_processes() -> Vec<Top> {
    let mut by_name: HashMap<String, (u64, u64, u32)> = HashMap::new();
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return Vec::new() };
        let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut more = Process32FirstW(snapshot, &mut entry).is_ok();
        while more {
            if let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, entry.th32ProcessID) {
                let mut counters = PROCESS_MEMORY_COUNTERS::default();
                let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
                if GetProcessMemoryInfo(process, &mut counters, size).is_ok() {
                    let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(0);
                    let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
                    let name = name.strip_suffix(".exe").unwrap_or(&name).to_string();
                    let used = counters.WorkingSetSize as u64;
                    let total = by_name.entry(name).or_default();
                    total.0 += used;
                    if used > total.1 {
                        (total.1, total.2) = (used, entry.th32ProcessID);
                    }
                }
                let _ = CloseHandle(process);
            }
            more = Process32NextW(snapshot, &mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
    }
    let mut top: Vec<_> = by_name.into_iter().collect();
    top.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));
    top.truncate(4);
    top.into_iter().map(|(name, (bytes, _, process))| Top { name, bytes, path: image_path(process) }).collect()
}

#[derive(Serialize)]
pub struct Program {
    name: Option<String>,
    icon: Option<String>,
}

pub(crate) fn description(path: &str) -> Option<String> {
    let file = HSTRING::from(path);
    unsafe {
        let size = GetFileVersionInfoSizeW(&file, None);
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        GetFileVersionInfoW(&file, None, size, data.as_mut_ptr().cast()).ok()?;
        let (mut found, mut len) = (std::ptr::null_mut(), 0u32);
        if !VerQueryValueW(data.as_ptr().cast(), w!("\\VarFileInfo\\Translation"), &mut found, &mut len).as_bool() || len < 4 {
            return None;
        }
        let codes = std::slice::from_raw_parts(found as *const u16, 2);
        let key = HSTRING::from(format!("\\StringFileInfo\\{:04x}{:04x}\\FileDescription", codes[0], codes[1]));
        if !VerQueryValueW(data.as_ptr().cast(), &key, &mut found, &mut len).as_bool() || len < 2 {
            return None;
        }
        let text = String::from_utf16_lossy(std::slice::from_raw_parts(found as *const u16, len as usize - 1));
        let text = text.trim();
        (!text.is_empty()).then(|| text.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn iconos_de_verdad() {
        for top in top_processes() {
            let exists = std::path::Path::new(&top.path).is_file();
            let icon = shell_image(top.path.clone(), 32).map(|icon| icon.len());
            println!("{} | {} | existe={exists} | nombre={:?} | ícono={icon:?}", top.name, top.path, description(&top.path));
        }
    }
}

#[tauri::command]
pub async fn program_info(path: String) -> Program {
    if !path.to_lowercase().ends_with(".exe") || !std::path::Path::new(&path).is_file() {
        return Program { name: None, icon: None };
    }
    Program { name: description(&path), icon: shell_image(path, 32) }
}

#[tauri::command]
pub async fn system_stats() -> Stats {
    let mut memory = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    let (mut free, mut total) = (0u64, 0u64);
    unsafe {
        let _ = GlobalMemoryStatusEx(&mut memory);
        let _ = GetDiskFreeSpaceExW(&HSTRING::from("C:\\"), None, Some(&mut total), Some(&mut free));
    }
    Stats {
        cpu: cpu_percent(),
        threads: std::thread::available_parallelism().map_or(1, |n| n.get()),
        memory_used: memory.ullTotalPhys - memory.ullAvailPhys,
        memory_total: memory.ullTotalPhys,
        disk_used: total - free,
        disk_total: total,
        uptime_secs: unsafe { GetTickCount64() } / 1000,
        top: top_processes(),
    }
}

pub fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        for (i, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            out.push(if i <= chunk.len() { TABLE[(n >> shift & 63) as usize] as char } else { '=' });
        }
    }
    out
}

fn shell_image(target: String, side: i32) -> Option<String> {
    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    dialogs::on_sta(move || unsafe {
        let factory: IShellItemImageFactory = SHCreateItemFromParsingName(&HSTRING::from(target), None).ok()?;
        let bitmap = factory.GetImage(SIZE { cx: side, cy: side }, SIIGBF_BIGGERSIZEOK).ok()?;

        let mut info = BITMAP::default();
        GetObjectW(bitmap.into(), std::mem::size_of::<BITMAP>() as i32, Some((&mut info as *mut BITMAP).cast()));
        let (w, h) = (info.bmWidth, info.bmHeight);
        let mut header = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bgra = vec![0u8; w as usize * h as usize * 4];
        let screen = GetDC(None);
        let lines = GetDIBits(screen, bitmap, 0, h as u32, Some(bgra.as_mut_ptr().cast()), &mut header, DIB_RGB_COLORS);
        ReleaseDC(None, screen);
        let _ = DeleteObject(bitmap.into());
        if lines != h || w <= 0 {
            return None;
        }

        let opaque = bgra.chunks_exact(4).all(|p| p[3] == 0);
        let rgba: Vec<u8> = bgra
            .chunks_exact(4)
            .flat_map(|p| {
                let a = if opaque { 255 } else { p[3] };
                let un = |c: u8| if a == 0 { 0 } else { (c as u32 * 255 / a as u32).min(255) as u8 };
                [un(p[2]), un(p[1]), un(p[0]), a]
            })
            .collect();

        let mut png_bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut png_bytes, w as u32, h as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);
        encoder.write_header().ok()?.write_image_data(&rgba).ok()?;
        Some(format!("data:image/png;base64,{}", base64(&png_bytes)))
    })
}

#[tauri::command]
pub async fn app_icon(app: AppHandle, id: String) -> Option<String> {
    let found = apps::list(&app).into_iter().find(|a| a.id == id)?;
    let target = if found.custom { found.id } else { format!("shell:AppsFolder\\{}", found.id) };
    shell_image(target, 32)
}

fn wallpaper_dir(app: &AppHandle) -> Option<PathBuf> {
    settings::load(app).wallpaper_dir.or_else(|| app.path().picture_dir().ok())
}

#[derive(Serialize)]
pub struct Wallpapers {
    dir: String,
    files: Vec<String>,
}

fn images_in(dir: &std::path::Path, depth: u8, out: &mut Vec<(std::time::SystemTime, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth > 0 {
                images_in(&path, depth - 1, out);
            }
            continue;
        }
        let ext = path.extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
        if matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "bmp" | "mp4" | "webm" | "mkv" | "mov" | "gif") {
            if let Ok(modified) = entry.metadata().and_then(|m| m.modified()) {
                out.push((modified, path.display().to_string()));
            }
        }
    }
}

#[tauri::command]
pub async fn wallpapers_list(app: AppHandle) -> Option<Wallpapers> {
    let mut dir = wallpaper_dir(&app)?;
    let mut files = Vec::new();
    images_in(&dir, 2, &mut files);
    if files.is_empty() && settings::load(&app).wallpaper_dir.is_none() {
        dir = PathBuf::from(std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into())).join("Web\\Wallpaper");
        images_in(&dir, 2, &mut files);
    }
    files.sort_by(|a, b| b.0.cmp(&a.0));
    files.truncate(24);
    Some(Wallpapers { dir: dir.display().to_string(), files: files.into_iter().map(|f| f.1).collect() })
}

#[tauri::command]
pub async fn wallpaper_thumb(path: String) -> Option<String> {
    shell_image(path, 320)
}

#[tauri::command]
pub fn wallpaper_set(path: String, screen: Option<String>) -> Result<(), String> {
    if let Some(id) = screen {
        return wallpaper_on(path, &id);
    }
    let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
    unsafe {
        SystemParametersInfoW(
            SPI_SETDESKWALLPAPER,
            0,
            Some(wide.as_ptr() as *mut _),
            SPIF_UPDATEINIFILE | SPIF_SENDCHANGE,
        )
        .map_err(|e| e.to_string())
    }
}

fn wallpaper_on(path: String, id: &str) -> Result<(), String> {
    use windows::core::PCWSTR;
    use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_ALL};
    use windows::Win32::UI::Shell::{DesktopWallpaper, IDesktopWallpaper};
    let wanted = crate::player::screens().into_iter().find(|screen| screen.id == id).ok_or("Ese monitor ya no está conectado.")?.rect;
    dialogs::on_sta(move || unsafe {
        let desktop: IDesktopWallpaper = CoCreateInstance(&DesktopWallpaper, None, CLSCTX_ALL).ok()?;
        let picture = windows::core::HSTRING::from(path.as_str());
        let mut done = None;
        for index in 0..desktop.GetMonitorDevicePathCount().ok()? {
            let Ok(monitor) = desktop.GetMonitorDevicePathAt(index) else { continue };
            if done.is_none() && desktop.GetMonitorRECT(PCWSTR(monitor.0)).is_ok_and(|rect| rect == wanted) {
                done = desktop.SetWallpaper(PCWSTR(monitor.0), &picture).ok();
            }
            CoTaskMemFree(Some(monitor.0 as _));
        }
        done
    })
    .ok_or_else(|| "Windows no dejó cambiar el fondo de ese monitor.".to_string())
}

#[tauri::command]
pub async fn wallpaper_pick_dir(app: AppHandle) -> Result<Option<String>, String> {
    let Some(dir) = dialogs::pick_folder() else { return Ok(None) };
    let mut config = settings::load(&app);
    config.wallpaper_dir = Some(dir.clone());
    settings::set_settings(app, config)?;
    Ok(Some(dir.display().to_string()))
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Todo {
    text: String,
    done: bool,
}

fn todos_file(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_config_dir().ok()?.join("tareas.json"))
}

#[tauri::command]
pub fn todos_get(app: AppHandle) -> Vec<Todo> {
    todos_file(&app)
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

#[tauri::command]
pub fn todos_set(app: AppHandle, todos: Vec<Todo>) -> Result<(), String> {
    let path = todos_file(&app).ok_or("No se encontró la carpeta de configuración.")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(&todos).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}
