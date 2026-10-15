// que video va en que monitor, y la copia a medida para que un 4k no se coma el pc

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowExW, FindWindowW, SendMessageTimeoutW, SMTO_NORMAL};

use crate::player::{self, Screen};
use crate::settings::{self, Settings};

const LABEL: &str = "fondo";

static CURRENT: Mutex<Vec<(String, PathBuf)>> = Mutex::new(Vec::new());
static SEEN: Mutex<Vec<String>> = Mutex::new(Vec::new());
static PAGE: Mutex<Option<PathBuf>> = Mutex::new(None);
static PAUSED: AtomicBool = AtomicBool::new(false);
static FALLEN: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
static PREPARING: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

pub(crate) fn desktop_layer() -> Option<HWND> {
    unsafe {
        let progman = FindWindowW(w!("Progman"), None).ok()?;
        let _ = SendMessageTimeoutW(progman, 0x052C, WPARAM(0xD), LPARAM(1), SMTO_NORMAL, 1000, None);

        let mut worker = FindWindowExW(None, None, w!("WorkerW"), None).ok();
        while let Some(current) = worker {
            if FindWindowExW(Some(current), None, w!("SHELLDLL_DefView"), None).is_ok() {
                return FindWindowExW(None, Some(current), w!("WorkerW"), None).ok();
            }
            worker = FindWindowExW(None, Some(current), w!("WorkerW"), None).ok();
        }
        FindWindowExW(Some(progman), None, w!("WorkerW"), None).ok()
    }
}

fn open_window(app: &AppHandle, on: &Screen) -> Result<(), String> {
    if app.get_webview_window(LABEL).is_some() {
        return Ok(());
    }
    let layer = desktop_layer().ok_or("Windows no dejó colocar el fondo animado en el escritorio.")?;
    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("fondo.html".into()))
        .decorations(false)
        .shadow(false) // con sombra quedan 8 px de borde invisible y el fondo sale con una franja
        .resizable(false)
        .skip_taskbar(true)
        .focused(false)
        .visible(false)
        .additional_browser_args(crate::BROWSER_ARGS)
        .build()
        .map_err(|e| e.to_string())?;
    let hwnd = HWND(window.hwnd().map_err(|e| e.to_string())?.0 as _);
    unsafe { player::place(hwnd, layer, &on.rect).map_err(|e| e.to_string()) }
}

fn is_gif(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("gif"))
}

fn show_in_page(app: &AppHandle, path: &Path, on: &Screen) -> Result<(), String> {
    app.asset_protocol_scope().allow_file(path).map_err(|e| e.to_string())?;
    *PAGE.lock().unwrap() = Some(path.to_path_buf());
    open_window(app, on)?;
    let _ = app.emit_to(LABEL, "fondo", path.display().to_string());
    Ok(())
}

fn close_page(app: &AppHandle) {
    *PAGE.lock().unwrap() = None;
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.destroy();
    }
}

fn wanted(path: &Path) -> bool {
    CURRENT.lock().unwrap().iter().any(|(_, original)| original == path)
}

fn cache(app: &AppHandle, path: &Path, screen: (u32, u32)) -> Option<(PathBuf, PathBuf)> {
    let meta = std::fs::metadata(path).ok()?;
    let mut hasher = DefaultHasher::new();
    (path, meta.len(), meta.modified().ok(), screen).hash(&mut hasher);
    let dir = app.path().app_local_data_dir().ok()?.join("fondos");
    std::fs::create_dir_all(&dir).ok()?;
    let name = format!("{:016x}", hasher.finish());
    Some((dir.join(format!("{name}.mp4")), dir.join(format!("{name}.txt"))))
}

fn chosen(config: &Settings, screens: &[Screen]) -> Vec<(String, PathBuf)> {
    screens
        .iter()
        .filter_map(|screen| {
            let path = config.live_screens.get(&screen.id).or(config.live_wallpaper.as_ref())?;
            Some((screen.id.clone(), path.clone()))
        })
        .collect()
}

fn in_use(app: &AppHandle) -> Vec<PathBuf> {
    let screens = player::screens();
    let current = CURRENT.lock().unwrap().clone();
    let mut files = Vec::new();
    for (id, original) in current {
        let Some(screen) = screens.iter().find(|screen| screen.id == id) else { continue };
        if let Some((copy, note)) = cache(app, &original, screen.size()) {
            files.extend([copy, note]);
        }
    }
    files
}

fn again(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let _ = apply(&app);
    });
}

fn optimize_later(app: &AppHandle, original: PathBuf, copy: PathBuf, note: PathBuf, screen: (u32, u32)) {
    {
        let mut preparing = PREPARING.lock().unwrap();
        if preparing.contains(&copy) {
            return;
        }
        preparing.push(copy.clone());
    }
    let app = app.clone();
    let _ = std::thread::Builder::new().name("fondo-copia".into()).spawn(move || {
        let done = |made: bool| {
            PREPARING.lock().unwrap().retain(|file| *file != copy);
            if made && wanted(&original) {
                let _ = apply(&app);
            }
        };
        std::thread::sleep(std::time::Duration::from_secs(4));
        if !wanted(&original) {
            return done(false);
        }
        let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        if !wanted(&original) {
            return done(false);
        }
        let keep = in_use(&app);
        if let Some(dir) = copy.parent().and_then(|d| std::fs::read_dir(d).ok()) {
            for old in dir.flatten().map(|entry| entry.path()).filter(|file| !keep.contains(file)) {
                let _ = std::fs::remove_file(old);
            }
        }
        let frames = std::cell::Cell::new(0u32);
        let when_idle = || {
            frames.set(frames.get() + 1);
            if frames.get() % 30 == 1 {
                while wanted(&original) && (crate::placement::fullscreen() || crate::placement::exclusive()) {
                    std::thread::sleep(std::time::Duration::from_secs(3));
                }
            }
            wanted(&original)
        };
        match player::optimize(&original, &copy, screen, &when_idle) {
            Ok(true) => return done(true),
            Ok(false) if wanted(&original) => {
                let _ = std::fs::write(&note, "El video ya tiene el tamaño de la pantalla.");
            }
            Ok(false) => {}
            Err(why) => {
                let _ = std::fs::write(&note, format!("No se pudo preparar la copia: {why}"));
            }
        }
        done(false)
    });
}

pub fn apply(app: &AppHandle) -> Result<(), String> {
    let screens = player::screens();
    let config = settings::load(app);
    let configured = config.live_wallpaper.is_some() || !config.live_screens.is_empty();
    let originals: Vec<(String, PathBuf)> = chosen(&config, &screens).into_iter().filter(|(_, path)| path.is_file()).collect();
    *CURRENT.lock().unwrap() = originals.clone();
    *SEEN.lock().unwrap() = if configured { screens.iter().map(|screen| screen.id.clone()).collect() } else { Vec::new() };
    if originals.is_empty() {
        player::stop();
        close_page(app);
        return Ok(());
    }
    PAUSED.store(false, Ordering::Relaxed);
    player::pause(false);
    player::hide(0);
    crate::placement::recheck();

    let force_page = cfg!(debug_assertions) && std::env::var_os("COCUYO_FONDO_PAGINA").is_some();
    let fallen = FALLEN.lock().unwrap().clone();
    let mut plan: Vec<(String, PathBuf)> = Vec::new();
    let mut sources: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut page = None;
    let mut later = Vec::new();
    for (id, original) in &originals {
        let Some(screen) = screens.iter().find(|screen| screen.id == *id) else { continue };
        if is_gif(original) || force_page || fallen.contains(original) {
            if screen.main {
                page = Some((original.clone(), screen));
            }
            continue;
        }
        let file = match cache(app, original, screen.size()) {
            Some((copy, _)) if copy.is_file() => copy,
            Some((copy, note)) => {
                if !note.is_file() {
                    later.push((original.clone(), copy, note, screen.size()));
                }
                original.clone()
            }
            None => original.clone(),
        };
        sources.push((file.clone(), original.clone()));
        plan.push((id.clone(), file));
    }
    match page {
        Some((path, screen)) => show_in_page(app, &path, screen)?,
        None => close_page(app),
    }
    if plan.is_empty() {
        player::stop();
    } else {
        let handle = app.clone();
        player::play(plan, move |file, why| {
            eprintln!("[fondo] sin reproductor nativo para {}: {why}", file.display());
            crate::errores::note("fondo", &format!("el reproductor nativo no pudo con {}: {why}; sigue en la página", file.display()));
            let Some((_, original)) = sources.iter().find(|(played, _)| *played == file) else { return };
            if wanted(original) {
                FALLEN.lock().unwrap().push(original.clone());
                again(&handle);
            }
        });
    }
    for (original, copy, note, size) in later {
        optimize_later(app, original, copy, note, size);
    }
    Ok(())
}

pub fn choose(app: &AppHandle, path: Option<&Path>, screen: Option<&str>) -> Result<(), String> {
    let mut config = settings::load(app);
    match screen {
        None => {
            config.live_wallpaper = path.map(Path::to_path_buf);
            config.live_screens.clear();
        }
        Some(id) => {
            if let Some(everywhere) = config.live_wallpaper.take() {
                for other in player::screens() {
                    config.live_screens.entry(other.id).or_insert_with(|| everywhere.clone());
                }
            }
            match path {
                Some(path) => config.live_screens.insert(id.to_string(), path.to_path_buf()),
                None => config.live_screens.remove(id),
            };
        }
    }
    settings::set_settings(app.clone(), config)?;
    apply(app)
}

pub fn restore(app: &AppHandle) {
    let _ = apply(app);
}

pub fn set_covered(app: &AppHandle, hidden: u32) {
    let seen = SEEN.lock().unwrap().clone();
    if seen.is_empty() {
        return;
    }
    let screens = player::screens();
    if !screens.iter().map(|screen| &screen.id).eq(seen.iter()) {
        return again(app);
    }
    let covered = |index: usize| index < 32 && hidden & (1 << index) != 0;
    let all = (0..screens.len()).all(covered);
    let main = screens.iter().position(|screen| screen.main).is_some_and(covered);
    let mut power = SYSTEM_POWER_STATUS::default();
    let on_battery = unsafe { GetSystemPowerStatus(&mut power).is_ok() } && power.ACLineStatus == 0;
    let forced = cfg!(debug_assertions) && std::env::var_os("COCUYO_FONDO_SIEMPRE").is_some();
    player::hide(if forced { 0 } else { hidden });
    player::pause((all || on_battery) && !forced);
    let pause = (main || on_battery) && !forced;
    if PAUSED.swap(pause, Ordering::Relaxed) != pause && app.get_webview_window(LABEL).is_some() {
        let _ = app.emit_to(LABEL, "fondo-pausa", pause);
    }
}

#[derive(Serialize)]
pub struct ScreenInfo {
    id: String,
    number: u32,
    main: bool,
    width: u32,
    height: u32,
    live: Option<String>,
}

#[tauri::command]
pub fn screens_list(app: AppHandle) -> Vec<ScreenInfo> {
    let screens = player::screens();
    let shown = chosen(&settings::load(&app), &screens);
    screens
        .iter()
        .map(|screen| ScreenInfo {
            id: screen.id.clone(),
            number: screen.number(),
            main: screen.main,
            width: screen.size().0,
            height: screen.size().1,
            live: shown.iter().find(|(id, _)| *id == screen.id).map(|(_, path)| path.display().to_string()),
        })
        .collect()
}

#[tauri::command]
pub async fn live_wallpaper_set(app: AppHandle, path: Option<String>, screen: Option<String>) -> Result<(), String> {
    let path = path.map(PathBuf::from);
    if let (Some(path), Some(id)) = (&path, &screen) {
        if is_gif(path) && !player::screens().iter().any(|s| s.main && s.id == *id) {
            return Err("Los GIF solo se pueden poner en el monitor principal. Los videos van en cualquiera.".into());
        }
    }
    choose(&app, path.as_deref(), screen.as_deref())
}

#[tauri::command]
pub fn live_wallpaper_get() -> Option<(String, bool)> {
    let page = PAGE.lock().unwrap();
    page.as_ref().map(|p| (p.display().to_string(), PAUSED.load(Ordering::Relaxed)))
}

const CATCH_FOR: std::time::Duration = std::time::Duration::from_secs(12 * 60);

static CATCHING: Mutex<Option<std::time::Instant>> = Mutex::new(None);

fn is_animated(path: &Path) -> bool {
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .is_some_and(|e| matches!(e.as_str(), "mp4" | "webm" | "mkv" | "mov" | "gif"))
}

fn animated_in(dir: &Path) -> Vec<(PathBuf, u64)> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let size = entry.metadata().ok().filter(|m| m.is_file())?.len();
            is_animated(&path).then_some((path, size))
        })
        .collect()
}

fn adopt(app: &AppHandle, file: &Path) -> Result<PathBuf, String> {
    let chosen = settings::load(app).wallpaper_dir;
    let dir = match chosen {
        Some(dir) => dir,
        None => app.path().picture_dir().map_err(|e| e.to_string())?.join("Fondos de Cocuyo"),
    };
    let mut home = file.to_path_buf();
    if file.parent() != Some(dir.as_path()) {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let target = dir.join(file.file_name().unwrap_or_default());
        if std::fs::rename(file, &target).is_ok() || std::fs::copy(file, &target).map(|_| std::fs::remove_file(file)).is_ok() {
            home = target;
        }
    }
    choose(app, Some(&home), None)?;
    Ok(home)
}

#[derive(Default)]
struct Arrivals {
    known: Vec<PathBuf>,
    growing: Vec<(PathBuf, u64)>,
}

impl Arrivals {
    fn finished(&mut self, now: Vec<(PathBuf, u64)>) -> Vec<PathBuf> {
        let mut done = Vec::new();
        for (path, size) in now {
            if self.known.contains(&path) {
                continue;
            }
            let mut part = path.clone().into_os_string();
            part.push(".part");
            let settled = size > 0 && !Path::new(&part).exists() && self.growing.iter().any(|(p, s)| *p == path && *s == size);
            self.growing.retain(|(p, _)| *p != path);
            if settled {
                self.known.push(path.clone());
                done.push(path);
            } else {
                self.growing.push((path, size));
            }
        }
        done
    }
}

fn watch_downloads(app: AppHandle, downloads: PathBuf) {
    let mut arrivals = Arrivals { known: animated_in(&downloads).into_iter().map(|(path, _)| path).collect(), ..Default::default() };
    loop {
        std::thread::sleep(std::time::Duration::from_millis(1500));
        {
            let mut until = CATCHING.lock().unwrap();
            if until.is_none_or(|t| std::time::Instant::now() > t) {
                *until = None;
                return;
            }
        }
        for path in arrivals.finished(animated_in(&downloads)) {
            let name = path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let note = match adopt(&app, &path) {
                Ok(home) => {
                    arrivals.known.push(home);
                    *CATCHING.lock().unwrap() = Some(std::time::Instant::now() + CATCH_FOR);
                    let _ = app.emit_to(crate::overlay::LABEL, "wallpaper-caught", ());
                    format!("Fondo puesto: {name}.")
                }
                Err(why) => why,
            };
            let _ = app.emit_to(crate::overlay::LABEL, "note", note);
        }
    }
}

pub fn catch_downloads(app: &AppHandle) -> Result<(), String> {
    let downloads = app.path().download_dir().map_err(|_| "No se encontró la carpeta de descargas.".to_string())?;
    let already = CATCHING.lock().unwrap().replace(std::time::Instant::now() + CATCH_FOR).is_some();
    if !already {
        let app = app.clone();
        let _ = std::thread::Builder::new().name("fondos-nuevos".into()).spawn(move || watch_downloads(app, downloads));
    }
    Ok(())
}

#[tauri::command]
pub async fn wallpaper_browse(app: AppHandle) -> Result<(), String> {
    let site = match crate::cue::language().0 {
        "es" => "https://wallsflow.com/es/",
        _ => "https://wallsflow.com/",
    };
    crate::apps::open(site)?;
    catch_downloads(&app)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_monitor_shows_its_own_video_or_the_one_for_all() {
        let screen = |id: &str, main| Screen { id: id.into(), rect: windows::Win32::Foundation::RECT { left: 0, top: 0, right: 1920, bottom: 1080 }, main };
        let screens = [screen("A", true), screen("B", false)];
        let mut config = Settings { live_wallpaper: Some("mar.mp4".into()), ..Default::default() };
        assert_eq!(chosen(&config, &screens), vec![("A".to_string(), PathBuf::from("mar.mp4")), ("B".to_string(), PathBuf::from("mar.mp4"))]);
        config.live_screens.insert("B".into(), "lluvia.mp4".into());
        assert_eq!(chosen(&config, &screens)[1], ("B".to_string(), PathBuf::from("lluvia.mp4")));
        config.live_wallpaper = None;
        assert_eq!(chosen(&config, &screens), vec![("B".to_string(), PathBuf::from("lluvia.mp4"))]);
        config.live_screens.insert("Z".into(), "desconectado.mp4".into());
        assert_eq!(chosen(&config, &screens).len(), 1);
    }

    #[test]
    fn a_download_counts_once_it_stops_growing() {
        let dir = std::env::temp_dir().join(format!("cocuyo-descargas-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let old = dir.join("ya-estaba.mp4");
        std::fs::write(&old, b"viejo").unwrap();
        let mut arrivals = Arrivals { known: animated_in(&dir).into_iter().map(|(p, _)| p).collect(), ..Default::default() };

        std::fs::write(dir.join("notas.txt"), b"x").unwrap();
        std::fs::write(dir.join("a-medias.mp4.crdownload"), b"x").unwrap();
        assert!(arrivals.finished(animated_in(&dir)).is_empty());

        let new = dir.join("nuevo.mp4");
        std::fs::write(&new, vec![0u8; 10]).unwrap();
        assert!(arrivals.finished(animated_in(&dir)).is_empty());
        std::fs::write(&new, vec![0u8; 20]).unwrap();
        assert!(arrivals.finished(animated_in(&dir)).is_empty());
        assert_eq!(arrivals.finished(animated_in(&dir)), vec![new.clone()]);
        assert!(arrivals.finished(animated_in(&dir)).is_empty(), "no se pone dos veces");

        let fox = dir.join("zorro.webm");
        std::fs::write(&fox, vec![0u8; 5]).unwrap();
        std::fs::write(dir.join("zorro.webm.part"), b"x").unwrap();
        assert!(arrivals.finished(animated_in(&dir)).is_empty());
        assert!(arrivals.finished(animated_in(&dir)).is_empty());
        std::fs::remove_file(dir.join("zorro.webm.part")).unwrap();
        assert_eq!(arrivals.finished(animated_in(&dir)), vec![fox]);
        assert!(arrivals.finished(animated_in(&dir)).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
