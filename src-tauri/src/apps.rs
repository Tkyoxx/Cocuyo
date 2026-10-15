// abrir y cerrar apps. cerrar es pedirlo como la X, nunca a la fuerza

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alias: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub icon: String,
}

const KNOWN: [(&str, &[&str]); 24] = [
    ("counter strike global offensive", &["csgo", "counter"]),
    ("counter strike 2", &["cs", "counter", "csgo"]),
    ("tom clancy s rainbow six siege", &["r6", "rainbow six", "siege"]),
    ("pubg battlegrounds", &["pubg", "battlegrounds"]),
    ("playerunknown s battlegrounds", &["pubg"]),
    ("grand theft auto v", &["gta", "gta v", "gta 5", "gta5"]),
    ("grand theft auto san andreas", &["gta sa", "san andreas"]),
    ("grand theft auto iv", &["gta 4", "gta iv"]),
    ("league of legends", &["lol"]),
    ("rocket league", &["rl", "rocket"]),
    ("call of duty", &["cod"]),
    ("dota 2", &["dota"]),
    ("apex legends", &["apex"]),
    ("valorant", &["valo"]),
    ("fortnite", &["fort"]),
    ("minecraft", &["mc", "maicra"]),
    ("red dead redemption 2", &["rdr2", "red dead"]),
    ("the elder scrolls v skyrim", &["skyrim"]),
    ("euro truck simulator 2", &["ets2", "euro truck"]),
    ("american truck simulator", &["ats"]),
    ("geometry dash", &["gd"]),
    ("dead by daylight", &["dbd"]),
    ("team fortress 2", &["tf2"]),
    ("mortal kombat 11", &["mk11", "mortal kombat"]),
];

const NOT_THE_GAME: [&str; 22] = [
    "unins", "setup", "install", "redist", "vc_redist", "vcredist", "dxsetup", "directx", "crash", "report", "helper", "prereq", "easyanticheat",
    "battleye", "be_service", "eac", "updater", "launcherpatcher", "dotnet", "ue4prereq", "cefprocess", "webhelper",
];

fn roman(word: &str) -> Option<&'static str> {
    Some(match word {
        "ii" => "2",
        "iii" => "3",
        "iv" => "4",
        "v" => "5",
        "vi" => "6",
        "vii" => "7",
        "viii" => "8",
        "ix" => "9",
        "x" => "10",
        _ => return None,
    })
}

pub fn nicknames(name: &str) -> Vec<String> {
    let clean = intent::normalize(&name.replace(['™', '®', '©'], " ").replace(['-', ':', '_', '.'], " "));
    let words: Vec<&str> = clean.split_whitespace().collect();
    let mut out = Vec::new();
    if words.len() >= 2 {
        let mut letters = String::new();
        let mut tail = String::new();
        for (i, word) in words.iter().enumerate() {
            if let Some(number) = roman(word).filter(|_| i > 0).or_else(|| word.chars().all(|c| c.is_ascii_digit()).then_some(*word)) {
                tail.push_str(number);
            } else if let Some(first) = word.chars().next() {
                letters.push(first);
            }
        }
        if letters.len() >= 2 {
            out.push(format!("{letters}{tail}"));
            if !tail.is_empty() {
                out.push(format!("{letters} {tail}"));
                out.push(letters.clone());
            }
        }
        let numbered: Vec<String> = words.iter().enumerate().map(|(i, w)| roman(w).filter(|_| i > 0).unwrap_or(w).to_string()).collect();
        let numbered = numbered.join(" ");
        if numbered != clean {
            out.push(numbered);
        }
    }
    for (known, extra) in KNOWN {
        if clean == known || clean.starts_with(&format!("{known} ")) {
            out.extend(extra.iter().map(|e| e.to_string()));
        }
    }
    out.retain(|n| *n != clean && n.len() >= 2);
    out.dedup();
    out
}

fn words_of(stem: &str) -> String {
    let stem = stem.replace(['_', '-'], " ");
    if stem.contains(' ') {
        return stem.trim().to_string();
    }
    let mut out = String::new();
    let mut last: Option<char> = None;
    for c in stem.chars() {
        if last.is_some_and(|l| (l.is_lowercase() && c.is_uppercase()) || (l.is_alphabetic() && c.is_ascii_digit())) {
            out.push(' ');
        }
        out.push(c);
        last = Some(c);
    }
    out.trim().to_string()
}

pub(crate) fn reg_string(key: windows::Win32::System::Registry::HKEY, path: &str, value: &str) -> Option<String> {
    use windows::Win32::System::Registry::{RegGetValueW, RRF_RT_REG_SZ};
    let mut buffer = [0u16; 520];
    let mut size = (buffer.len() * 2) as u32;
    let found = unsafe { RegGetValueW(key, &HSTRING::from(path), &HSTRING::from(value), RRF_RT_REG_SZ, None, Some(buffer.as_mut_ptr().cast()), Some(&mut size)) };
    found.is_ok().then(|| String::from_utf16_lossy(&buffer[..(size as usize / 2).saturating_sub(1)]))
}

fn vdf_values<'a>(text: &'a str, key: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('"').collect();
            (parts.len() >= 4 && parts[1].eq_ignore_ascii_case(key)).then(|| parts[3].replace("\\\\", "\\"))
        })
        .collect()
}

fn steam_libraries() -> Vec<PathBuf> {
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    let mut roots: Vec<PathBuf> = [
        reg_string(HKEY_CURRENT_USER, "Software\\Valve\\Steam", "SteamPath"),
        reg_string(HKEY_LOCAL_MACHINE, "SOFTWARE\\WOW6432Node\\Valve\\Steam", "InstallPath"),
        reg_string(HKEY_LOCAL_MACHINE, "SOFTWARE\\Valve\\Steam", "InstallPath"),
    ]
    .into_iter()
    .flatten()
    .map(PathBuf::from)
    .collect();
    let drives = unsafe { windows::Win32::Storage::FileSystem::GetLogicalDrives() };
    for (i, letter) in ('A'..='Z').enumerate() {
        if drives & (1 << i) == 0 {
            continue;
        }
        for guess in ["SteamLibrary", "Steam", "Games\\Steam", "Games\\SteamLibrary", "Juegos\\Steam", "Program Files (x86)\\Steam", "Program Files\\Steam"] {
            roots.push(PathBuf::from(format!("{letter}:\\{guess}")));
        }
    }
    let mut libraries = Vec::new();
    for root in &roots {
        if let Ok(text) = std::fs::read_to_string(root.join("steamapps").join("libraryfolders.vdf")) {
            libraries.extend(vdf_values(&text, "path").into_iter().map(PathBuf::from));
        }
        libraries.push(root.clone());
    }
    let mut seen = std::collections::HashSet::new();
    libraries
        .into_iter()
        .filter(|library| library.join("steamapps").is_dir())
        .filter(|library| seen.insert(library.to_string_lossy().to_lowercase().replace('/', "\\").trim_end_matches('\\').to_string()))
        .collect()
}

fn game_exes(dir: &Path, depth: u32, out: &mut Vec<(u64, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth > 0 {
                game_exes(&path, depth - 1, out);
            }
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe")) {
            let stem = path.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
            if !NOT_THE_GAME.iter().any(|junk| stem.contains(junk)) {
                out.push((entry.metadata().map(|m| m.len()).unwrap_or(0), path));
            }
        }
    }
}

fn scan_steam() -> Vec<App> {
    let mut found = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for library in steam_libraries() {
        let apps = library.join("steamapps");
        let Ok(entries) = std::fs::read_dir(&apps) else { continue };
        for entry in entries.flatten() {
            let file = entry.file_name().to_string_lossy().to_string();
            if !file.starts_with("appmanifest_") || !file.ends_with(".acf") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(entry.path()) else { continue };
            let (Some(id), Some(name)) = (vdf_values(&text, "appid").into_iter().next(), vdf_values(&text, "name").into_iter().next()) else { continue };
            let lower = name.to_lowercase();
            if id == "228980" || ["redistributable", "steamworks", "proton", "steam linux runtime", "dedicated server", "soundtrack", "sdk"].iter().any(|w| lower.contains(w)) || !seen.insert(id.clone()) {
                continue;
            }
            let mut alias = nicknames(&name);
            let mut icon = String::new();
            if let Some(dir) = vdf_values(&text, "installdir").into_iter().next() {
                let mut exes = Vec::new();
                game_exes(&apps.join("common").join(dir), 3, &mut exes);
                exes.sort_by(|a, b| b.0.cmp(&a.0));
                if let Some((_, biggest)) = exes.first() {
                    icon = biggest.display().to_string();
                }
                if let Some(stem) = exes.first().and_then(|(_, exe)| exe.file_stem()) {
                    alias.push(words_of(&stem.to_string_lossy()));
                }
            }
            alias.dedup();
            found.push(App { name, id: format!("steam://rungameid/{id}"), custom: false, alias, icon });
        }
    }
    found
}

fn scan_desktop() -> Vec<App> {
    let mut dirs = Vec::new();
    if let Ok(home) = std::env::var("USERPROFILE") {
        dirs.push(PathBuf::from(home).join("Desktop"));
    }
    if let Ok(public) = std::env::var("PUBLIC") {
        dirs.push(PathBuf::from(public).join("Desktop"));
    }
    if let Some(desktop) = dirs::desktop() {
        dirs.push(desktop);
    }
    let mut found: Vec<App> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for dir in dirs.into_iter().filter(|dir| seen.insert(dir.to_string_lossy().to_lowercase())) {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let kind = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            if kind != "lnk" && kind != "url" {
                continue;
            }
            let name = path.file_stem().map(|s| words_of(&s.to_string_lossy().replace(['®', '™'], ""))).unwrap_or_default();
            let lower = name.to_lowercase();
            if name.is_empty() || ["uninstall", "desinstalar", "readme", "website", "sitio web"].iter().any(|w| lower.contains(w)) {
                continue;
            }
            found.push(App { alias: nicknames(&name), name, id: path.display().to_string(), custom: false, icon: String::new() });
        }
    }
    found
}

mod dirs {
    pub fn desktop() -> Option<std::path::PathBuf> {
        use windows::Win32::UI::Shell::{FOLDERID_Desktop, SHGetKnownFolderPath, KF_FLAG_DEFAULT};
        unsafe {
            let path = SHGetKnownFolderPath(&FOLDERID_Desktop, KF_FLAG_DEFAULT, None).ok()?;
            let text = path.to_string().ok();
            windows::Win32::System::Com::CoTaskMemFree(Some(path.0 as _));
            text.map(std::path::PathBuf::from)
        }
    }
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
        .map(|a| App { alias: nicknames(&a.name), name: a.name, id: a.app_id, custom: false, icon: String::new() })
        .collect()
}

pub fn list(app: &AppHandle) -> Vec<App> {
    let mut cache = CACHE.lock().unwrap();
    let stale = cache.as_ref().map_or(true, |(at, _)| at.elapsed() > FRESH_FOR);
    if stale {
        let (steam, desktop) = std::thread::scope(|scope| {
            let steam = scope.spawn(scan_steam);
            let desktop = scope.spawn(scan_desktop);
            (steam.join().unwrap_or_default(), desktop.join().unwrap_or_default())
        });
        let mut scanned = scan_start_menu();
        let mut known: std::collections::HashSet<String> = scanned.iter().map(|a| intent::normalize(&a.name)).collect();
        for found in steam.into_iter().chain(desktop) {
            let key = intent::normalize(&found.name);
            if let Some(same) = scanned.iter_mut().find(|a| intent::normalize(&a.name) == key) {
                same.alias.extend(found.alias);
                if same.icon.is_empty() {
                    same.icon = found.icon;
                }
                if found.id.starts_with("steam://") {
                    same.id = found.id;
                }
            } else if known.insert(key) {
                scanned.push(found);
            }
        }
        *cache = Some((Instant::now(), scanned));
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

fn plain_target(id: &str) -> bool {
    id.contains("://") || id.contains(":\\") || id.starts_with("\\\\")
}

pub fn launch(app: &App) -> Result<(), String> {
    if app.custom || plain_target(&app.id) {
        open(&app.id)
    } else {
        open(&format!("shell:AppsFolder\\{}", app.id))
    }
}

pub fn find(app: &AppHandle, spoken: &str, enough: f32) -> Option<App> {
    let spelled = spelled(spoken);
    let compact = |text: &str| intent::normalize(text).replace(' ', "");
    let said = compact(&spelled);
    list(app)
        .into_iter()
        .map(|a| {
            let named = intent::similarity(&spelled, &a.name);
            let nick = a.alias.iter().map(|alias| if compact(alias) == said { 0.97 } else { intent::similarity(&spelled, alias) * 0.97 }).fold(0.0, f32::max);
            (named.max(nick), a)
        })
        .filter(|(score, _)| *score >= enough)
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, a)| a)
}

fn spelled(spoken: &str) -> String {
    const LETTERS: [(&str, &str); 30] = [
        ("a", "a"), ("be", "b"), ("ce", "c"), ("de", "d"), ("efe", "f"), ("ge", "g"), ("hache", "h"), ("jota", "j"), ("ka", "k"), ("ele", "l"),
        ("eme", "m"), ("ene", "n"), ("pe", "p"), ("cu", "q"), ("erre", "r"), ("ese", "s"), ("te", "t"), ("uve", "v"), ("ve", "v"), ("equis", "x"),
        ("ye", "y"), ("zeta", "z"), ("uno", "1"), ("dos", "2"), ("tres", "3"), ("cuatro", "4"), ("cinco", "5"), ("seis", "6"), ("siete", "7"), ("once", "11"),
    ];
    let words: Vec<String> = intent::normalize(spoken).split_whitespace().map(str::to_string).collect();
    fn letter(w: &str) -> Option<String> {
        LETTERS.iter().find(|(said, _)| *said == w).map(|(_, l)| l.to_string()).or_else(|| (w.len() == 1 || w.chars().all(|c| c.is_ascii_digit())).then(|| w.to_string()))
    }
    if words.len() >= 2 && words.iter().filter(|w| letter(w).is_some()).count() >= 2 && words.iter().all(|w| letter(w).is_some()) {
        return words.iter().filter_map(|w| letter(w)).collect::<String>();
    }
    spoken.to_string()
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
    let added = App { name, id: path.display().to_string(), custom: true, alias: Vec::new(), icon: String::new() };
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

const EVERYTHING: [&str; 14] = [
    "todo", "todas", "todas las apps", "todas las aplicaciones", "todas las ventanas", "todos los programas", "todo lo abierto", "todo lo que tengo abierto",
    "everything", "all", "all apps", "all windows", "tudo", "todos os apps",
];
const BUT: [&str; 6] = [" menos ", " excepto ", " salvo ", " except ", " but ", " exceto "];

fn names_in(list: &str) -> Vec<String> {
    let mut list = format!(" {} ", list.replace(',', " , "));
    for joiner in [" y ", " e ", " and ", " tambien ", " , "] {
        list = list.replace(joiner, "|");
    }
    list.split('|').map(str::trim).filter(|name| !name.is_empty()).map(str::to_string).collect()
}

pub fn close(name: &str) -> Result<String, String> {
    let name = name.trim();
    let (what, keep) = match BUT.iter().find_map(|but| format!(" {name} ").split_once(but).map(|(what, keep)| (what.trim().to_string(), keep.to_string()))) {
        Some((what, keep)) => (what, names_in(&keep)),
        None => (name.to_string(), Vec::new()),
    };
    if EVERYTHING.contains(&what.as_str()) {
        return close_all(&keep);
    }
    let names = names_in(&what);
    if names.len() < 2 {
        return close_one(&what);
    }
    let mut closed = Vec::new();
    let mut missing = Vec::new();
    for name in &names {
        match close_one(name) {
            Ok(said) => closed.push(said.trim_start_matches("Cerrando ").trim_end_matches('.').to_string()),
            Err(_) => missing.push(name.clone()),
        }
    }
    match (closed.is_empty(), missing.is_empty()) {
        (true, _) => Err(format!("No veo abierta ninguna de esas apps: {}.", missing.join(", "))),
        (false, true) => Ok(format!("Cerrando {}.", list_of(&closed))),
        (false, false) => Ok(format!("Cerrando {}. No vi abierta {}.", list_of(&closed), list_of(&missing))),
    }
}

fn list_of(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} y {last}", rest.join(", ")),
    }
}

fn on_screen(hwnd: isize) -> bool {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
    use windows::Win32::UI::WindowsAndMessaging::{GetWindow, GetWindowLongPtrW, IsWindowVisible, GWL_EXSTYLE, GW_OWNER, WS_EX_TOOLWINDOW};
    let hwnd = HWND(hwnd as _);
    unsafe {
        let mut cloaked = 0u32;
        let _ = DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, (&mut cloaked as *mut u32).cast(), 4);
        IsWindowVisible(hwnd).as_bool()
            && GetWindow(hwnd, GW_OWNER).is_err()
            && GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 == 0
            && cloaked == 0
    }
}

fn close_all(keep: &[String]) -> Result<String, String> {
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::System::Threading::GetCurrentProcessId;
    use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};
    let own = unsafe { GetCurrentProcessId() };
    let windows: Vec<(isize, u32, bool)> = top_windows().into_iter().filter(|(hwnd, _, titled)| *titled && on_screen(*hwnd)).collect();
    let mut picked: Vec<(String, String)> = Vec::new();
    let mut skipped: Vec<u32> = Vec::new();
    for (_, process, titled) in &windows {
        if !titled || *process == own || skipped.contains(process) || picked.iter().any(|(path, _)| image_of(*process).as_deref() == Some(path.as_str())) {
            continue;
        }
        let Some(path) = image_of(*process) else {
            skipped.push(*process);
            continue;
        };
        let stem = std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let described = crate::system::description(&path).unwrap_or_default();
        let kept = keep.iter().any(|name| intent::similarity(name, &stem).max(intent::similarity(name, &described)).max(intent::covers(name, &described)) >= 0.75);
        if NOT_APPS.contains(&stem.to_lowercase().as_str()) || kept {
            skipped.push(*process);
            continue;
        }
        picked.push((path, if described.is_empty() { stem } else { described }));
    }
    if picked.is_empty() {
        return Err("No hay apps abiertas que cerrar.".into());
    }
    for (hwnd, process, titled) in &windows {
        if *titled && image_of(*process).is_some_and(|path| picked.iter().any(|(chosen, _)| *chosen == path)) {
            unsafe {
                let _ = PostMessageW(Some(HWND(*hwnd as _)), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
    }
    let names: Vec<String> = picked.into_iter().map(|(_, label)| label).collect();
    Ok(match names.len() {
        1 => format!("Cerrando {}.", names[0]),
        count if count <= 5 => format!("Cerrando {count} apps: {}.", list_of(&names)),
        count => format!("Cerrando {count} apps."),
    })
}

const NOT_APPS: [&str; 12] = ["explorer", "dwm", "csrss", "winlogon", "svchost", "searchui", "searchapp", "shellexperiencehost", "startmenuexperiencehost", "textinputhost", "applicationframehost", "systemsettings"];

fn close_one(name: &str) -> Result<String, String> {
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::System::Threading::GetCurrentProcessId;
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId, PostMessageW, WM_CLOSE};
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

pub fn wants_close(spoken: &str) -> bool {
    let clean = intent::normalize(spoken);
    CLOSE_LEADS.iter().any(|lead| clean.starts_with(lead)) && clean.split_whitespace().count() >= 2
}

const CLOSE_LEADS: [&str; 11] = ["cierra ", "cierrame ", "cierren ", "cerrar ", "cierre ", "cierrate ", "close ", "fecha ", "feche ", "fechar ", "quit "];

pub fn close_spoken(spoken: &str) -> Option<Result<String, String>> {
    let clean = intent::normalize(&spoken.replace(',', " y "));
    let lead = CLOSE_LEADS.iter().find(|lead| clean.starts_with(**lead))?;
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
        assert_eq!(close_spoken("cierra programaquenoexiste1, programaquenoexiste2 y programaquenoexiste3"), Some(Err("No veo abierta ninguna de esas apps: programaquenoexiste1, programaquenoexiste2, programaquenoexiste3.".into())));
        assert_eq!(names_in("spotify y discord e opera"), vec!["spotify", "discord", "opera"]);
        assert!(wants_close("cierra todo") && !wants_close("cierra") && !wants_close("pon música"));
    }

    #[test]
    fn games_get_their_short_names() {
        let has = |name: &str, nick: &str| nicknames(name).iter().any(|n| n == nick);
        assert!(has("Counter-Strike 2", "cs2"));
        assert!(has("Counter-Strike 2", "cs 2"));
        assert!(has("Grand Theft Auto V", "gta5"));
        assert!(has("Grand Theft Auto V", "gta 5"));
        assert!(has("Grand Theft Auto: San Andreas", "gtasa"));
        assert!(has("Rocket League®", "rl"));
        assert!(has("Tom Clancy's Rainbow Six® Siege", "r6"));
        assert!(has("League of Legends", "lol"));
        assert!(has("Red Dead Redemption 2", "rdr2"));
        assert!(nicknames("Spotify").is_empty());
        assert_eq!(words_of("GhostOfTsushima"), "Ghost Of Tsushima");
        assert_eq!(words_of("Overcooked2"), "Overcooked 2");
        assert_eq!(words_of("Rocket League"), "Rocket League");
        assert!(has(&words_of("GhostOfTsushima"), "got"));
        assert_eq!(spelled("ce ese dos"), "cs2");
        assert_eq!(spelled("ge te a cinco"), "gta5");
        assert_eq!(spelled("abre la tienda"), "abre la tienda");
        assert_eq!(vdf_values("\t\t\"path\"\t\t\"D:\\\\SteamLibrary\"", "path"), vec!["D:\\SteamLibrary".to_string()]);
    }

    #[test]
    #[ignore]
    fn shows_what_this_pc_has() {
        for app in scan_steam().iter().chain(scan_desktop().iter()) {
            println!("{} | {} | {:?} | {}", app.name, app.id, app.alias, app.icon);
        }
    }
}
