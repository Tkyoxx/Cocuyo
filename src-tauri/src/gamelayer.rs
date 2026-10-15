// el enlace con la libreria que pinta dentro del juego

#[path = "../../overlay/src/shared.rs"]
#[allow(dead_code)]
mod shared;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use shared::{Shared, CAPTION_APP, CAPTION_NONE, CAPTION_YOU, MAP_NAME, PILL_DONE, PILL_HIDDEN, PILL_LISTENING, PILL_THINKING};
use tauri::{AppHandle, Manager};
use windows::core::{s, BOOL, HSTRING, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, HMODULE, HWND, INVALID_HANDLE_VALUE, LPARAM, WPARAM};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::Win32::System::Memory::{CreateFileMappingW, MapViewOfFile, FILE_MAP_WRITE, PAGE_READWRITE};
use windows::Win32::System::Threading::{
    GetCurrentProcessId, IsWow64Process, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetSystemMetrics, GetWindowThreadProcessId, PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx,
    HOOKPROC, SM_CYSCREEN, WH_GETMESSAGE, WM_NULL,
};

static LIBRARY: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../overlay/target/release/cocuyo_overlay.dll"));

const NOT_GAMES: &[&str] = &[
    "chrome", "msedge", "firefox", "brave", "opera", "opera_gx", "vivaldi", "iexplore", "explorer", "vlc", "mpv", "mpc-hc64",
    "mpc-be64", "wmplayer", "potplayermini64", "powerpnt", "obs64", "discord", "spotify", "code", "claude",
];

static ENABLED: AtomicBool = AtomicBool::new(false);
static LISTENING: AtomicBool = AtomicBool::new(false);
static TURN: AtomicU32 = AtomicU32::new(0);
static RECORDING_SINCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

struct Link {
    shared: *mut Shared,
    module: HMODULE,
    entry: HOOKPROC,
    tried: HashSet<u32>,
}
unsafe impl Send for Link {}

static LINK: Mutex<Option<Link>> = Mutex::new(None);

pub fn configure(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
    if let Some(link) = LINK.lock().unwrap().as_mut() {
        unsafe {
            (*link.shared).enabled = u32::from(enabled);
            if !enabled {
                (*link.shared).pill = PILL_HIDDEN;
                set_caption(link.shared, CAPTION_NONE, "", "");
            }
        }
    }
}

fn library_path(app: &AppHandle) -> Option<PathBuf> {
    let hash = LIBRARY.iter().fold(0xcbf29ce484222325u64, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3));
    let dir = app.path().app_local_data_dir().ok()?.join("capa");
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join(format!("cocuyo_overlay-{:08x}.dll", hash as u32));
    if std::fs::metadata(&path).map(|m| m.len()).ok() != Some(LIBRARY.len() as u64) {
        std::fs::write(&path, LIBRARY).ok()?;
    }
    Some(path)
}

unsafe fn open(app: &AppHandle) -> Option<Link> {
    let name: Vec<u16> = MAP_NAME.encode_utf16().chain(Some(0)).collect();
    let size = std::mem::size_of::<Shared>() as u32;
    let mapping = CreateFileMappingW(INVALID_HANDLE_VALUE, None, PAGE_READWRITE, 0, size, PCWSTR(name.as_ptr())).ok()?;
    let view = MapViewOfFile(mapping, FILE_MAP_WRITE, 0, 0, size as usize);
    if view.Value.is_null() {
        return None;
    }
    let module = LoadLibraryW(&HSTRING::from(library_path(app)?.as_os_str())).ok()?;
    let entry: HOOKPROC = std::mem::transmute(GetProcAddress(module, s!("cocuyo_hook")));
    entry?;
    Some(Link { shared: view.Value.cast(), module, entry, tried: HashSet::new() })
}

unsafe fn attach(link: &mut Link, window: HWND) {
    let mut process = 0;
    let thread = GetWindowThreadProcessId(window, Some(&mut process));
    if thread == 0 || process == GetCurrentProcessId() || !link.tried.insert(process) {
        return;
    }
    let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process) else { return };
    let mut narrow = BOOL(0);
    let is_32 = IsWow64Process(handle, &mut narrow).is_ok() && narrow.as_bool();
    let mut path = [0u16; 520];
    let mut len = path.len() as u32;
    let named = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, windows::core::PWSTR(path.as_mut_ptr()), &mut len).is_ok();
    let _ = CloseHandle(handle);
    let path = String::from_utf16_lossy(&path[..len as usize]).to_lowercase();
    let not_a_game = NOT_GAMES.iter().any(|name| path.ends_with(&format!("\\{name}.exe"))) || path.contains("\\windows\\");
    if is_32 || !named || not_a_game {
        return;
    }
    let Ok(hook) = SetWindowsHookExW(WH_GETMESSAGE, link.entry, Some(link.module.into()), thread) else { return };
    let _ = PostThreadMessageW(thread, WM_NULL, WPARAM(0), LPARAM(0));
    let hook = hook.0 as isize;
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(1500));
        let _ = UnhookWindowsHookEx(windows::Win32::UI::WindowsAndMessaging::HHOOK(hook as _));
    });
}

unsafe fn set_caption(shared: *mut Shared, kind: u32, who: &str, text: &str) {
    let fill = |slots: &mut [u16], text: &str| {
        slots.fill(0);
        let room = slots.len() - 1;
        for (slot, unit) in slots.iter_mut().take(room).zip(text.encode_utf16()) {
            *slot = unit;
        }
    };
    fill(&mut (*shared).who, who);
    fill(&mut (*shared).text, text);
    (*shared).caption = kind;
    (*shared).caption_seq = (*shared).caption_seq.wrapping_add(1);
}

fn with_link(app: &AppHandle, target: Option<HWND>, change: impl FnOnce(*mut Shared)) -> bool {
    let needed = target.is_some() || (crate::placement::fullscreen() && crate::placement::exclusive());
    if !ENABLED.load(Ordering::Relaxed) || !needed {
        return false;
    }
    let mut guard = LINK.lock().unwrap();
    unsafe {
        if guard.is_none() {
            *guard = open(app);
        }
        let Some(link) = guard.as_mut() else { return false };
        let shared = link.shared;
        (*shared).enabled = 1;
        (*shared).screen_h = GetSystemMetrics(SM_CYSCREEN) as f32;
        if let Some(window) = app.get_webview_window(crate::overlay::LABEL) {
            (*shared).scale = window.scale_factor().unwrap_or(1.0) as f32;
            let (top, height) = (window.outer_position().map_or(0, |p| p.y), window.outer_size().map_or(0, |s| s.height));
            (*shared).view_h = height as f32;
            (*shared).inset_bottom = ((*shared).screen_h - top as f32 - height as f32).max(0.0);
        }
        let since = RECORDING_SINCE.load(Ordering::Relaxed);
        (*shared).recording = u32::from(since != 0);
        (*shared).rec_since = since;
        change(shared);
        attach(link, target.unwrap_or_else(|| GetForegroundWindow()));
    }
    true
}

fn after(ms: u64, turn: u32, change: impl FnOnce(*mut Shared) + Send + 'static) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(ms));
        if TURN.load(Ordering::Relaxed) != turn {
            return;
        }
        if let Some(link) = LINK.lock().unwrap().as_mut() {
            change(link.shared);
        }
    });
}

pub fn recording(app: &AppHandle, since: Option<u64>) {
    RECORDING_SINCE.store(since.unwrap_or(0), Ordering::Relaxed);
    if !with_link(app, None, |_| {}) {
        if let Some(link) = LINK.lock().unwrap().as_mut() {
            unsafe {
                (*link.shared).recording = u32::from(since.is_some());
                (*link.shared).rec_since = since.unwrap_or(0);
            }
        }
    }
}

pub fn still_recording(app: &AppHandle) {
    if RECORDING_SINCE.load(Ordering::Relaxed) != 0 {
        with_link(app, None, |_| {});
    }
}

pub fn listening(app: &AppHandle) {
    TURN.fetch_add(1, Ordering::Relaxed);
    let shown = with_link(app, None, |shared| unsafe {
        (*shared).level = 0.0;
        (*shared).pill = PILL_LISTENING;
        set_caption(shared, CAPTION_NONE, "", "");
    });
    LISTENING.store(shown, Ordering::Relaxed);
}

pub fn level(peak: f32) {
    if !LISTENING.load(Ordering::Relaxed) {
        return;
    }
    if let Ok(guard) = LINK.try_lock() {
        if let Some(link) = guard.as_ref() {
            unsafe { (*link.shared).level = (peak.max(0.0).sqrt() * 1.7).min(1.0) };
        }
    }
}

pub fn thinking() {
    LISTENING.store(false, Ordering::Relaxed);
    let turn = TURN.load(Ordering::Relaxed);
    if let Some(link) = LINK.lock().unwrap().as_mut() {
        unsafe {
            if (*link.shared).pill != PILL_HIDDEN {
                (*link.shared).level = 0.0;
                (*link.shared).pill = PILL_THINKING;
            }
        }
    }
    after(25_000, turn, |shared| unsafe { (*shared).pill = PILL_HIDDEN });
}

pub fn cancel() {
    LISTENING.store(false, Ordering::Relaxed);
    TURN.fetch_add(1, Ordering::Relaxed);
    if let Some(link) = LINK.lock().unwrap().as_mut() {
        unsafe {
            (*link.shared).pill = PILL_HIDDEN;
            set_caption(link.shared, CAPTION_NONE, "", "");
        }
    }
}

pub fn finished(heard: &str, reply: &str, failed: bool, dictation: bool) {
    LISTENING.store(false, Ordering::Relaxed);
    let turn = TURN.fetch_add(1, Ordering::Relaxed) + 1;
    let Some(shared) = LINK.lock().unwrap().as_ref().map(|link| link.shared as usize) else { return };
    let shared = shared as *mut Shared;
    if failed {
        unsafe { (*shared).pill = PILL_HIDDEN };
    } else {
        unsafe { (*shared).pill = PILL_DONE };
        after(950, turn, |shared| unsafe { (*shared).pill = PILL_HIDDEN });
    }
    if dictation && !failed {
        return;
    }
    let wait = if heard.is_empty() {
        0
    } else {
        unsafe { set_caption(shared, CAPTION_YOU, "Tú", heard) };
        1300
    };
    if reply.is_empty() {
        after(1600, turn, |shared| unsafe { set_caption(shared, CAPTION_NONE, "", "") });
        return;
    }
    let (reply, lasts) = (reply.to_string(), if failed { 5200 } else { 3400 });
    after(wait, turn, move |shared| unsafe { set_caption(shared, CAPTION_APP, "Cocuyo", &reply) });
    after(wait + lasts, turn, |shared| unsafe { set_caption(shared, CAPTION_NONE, "", "") });
}

#[cfg(debug_assertions)]
pub fn probe(app: &AppHandle) -> String {
    ENABLED.store(true, Ordering::Relaxed);
    TURN.fetch_add(1, Ordering::Relaxed);
    let Ok(game) = (unsafe { windows::Win32::UI::WindowsAndMessaging::FindWindowW(None, windows::core::w!("Juego de prueba de Cocuyo")) }) else {
        return "el juego de prueba no está abierto".into();
    };
    let shown = with_link(app, Some(game), |shared| unsafe {
        (*shared).level = 0.6;
        (*shared).pill = PILL_LISTENING;
    });
    if !shown {
        return "no se pudo preparar la librería".into();
    }
    std::thread::sleep(Duration::from_millis(2500));
    thinking();
    std::thread::sleep(Duration::from_millis(1500));
    finished("Pon Babydoll de Dominic Fike.", "Sonando «Babydoll» de Dominic Fike.", false, false);
    format!("orden de prueba enviada; procesos intentados: {:?}", LINK.lock().unwrap().as_ref().map(|link| link.tried.clone()))
}
