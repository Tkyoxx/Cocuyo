// donde vive la isla. si hay un juego en exclusiva NO se muestra nada o windows lo saca al escritorio (aprendido a las malas)

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::Shell::{SHQueryUserNotificationState, QUNS_RUNNING_D3D_FULL_SCREEN};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetForegroundWindow, GetSystemMetrics, GetTopWindow, GetWindow,
    GetWindowLongPtrW, GetWindowRect, GetWindowThreadProcessId, IsIconic, IsWindowVisible, IsZoomed,
    KillTimer, SetTimer, EVENT_OBJECT_FOCUS, EVENT_OBJECT_LOCATIONCHANGE,
    EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_MINIMIZEEND, GWL_EXSTYLE, GW_HWNDNEXT, SM_CXSCREEN,
    SM_CYSCREEN, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
};

use crate::overlay;
use crate::settings::{self, Dock};

const ISLAND_HALF: i32 = 130;
const ISLAND_DEPTH: i32 = 36;
const SETTLE_MS: u32 = 160;
const OBJID_WINDOW: i32 = 0;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Place {
    Top,
    Right,
    Hidden,
}

struct State {
    dock: Dock,
    hide_on_fullscreen: bool,
    place: Place,
    follow: Option<isize>,
    followed: isize,
}

static APP: OnceLock<AppHandle> = OnceLock::new();
static TIMER: AtomicUsize = AtomicUsize::new(0);
static STATE: Mutex<State> = Mutex::new(State {
    dock: Dock::Auto,
    hide_on_fullscreen: true,
    place: Place::Top,
    follow: None,
    followed: 0,
});

pub fn install(app: AppHandle) {
    let _ = APP.set(app);
    let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
    unsafe {
        SetWinEventHook(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_MINIMIZEEND, None, Some(on_event), 0, 0, flags);
        SetWinEventHook(EVENT_OBJECT_FOCUS, EVENT_OBJECT_FOCUS, None, Some(on_event), 0, 0, flags);
    }
    decide_now();
}

pub fn refresh(app: &AppHandle) {
    let config = settings::load(app);
    {
        let mut state = STATE.lock().unwrap();
        state.dock = config.dock;
        state.hide_on_fullscreen = config.hide_on_fullscreen;
    }
    decide_now();
}

pub fn recheck() {
    decide_now();
}

#[tauri::command]
pub fn get_place() -> Place {
    STATE.lock().unwrap().place
}

pub fn on_timer(id: usize) {
    if id != 0 && id == TIMER.swap(0, Ordering::Relaxed) {
        unsafe {
            let _ = KillTimer(None, id);
        }
        decide_now();
    }
}

unsafe extern "system" fn on_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    _id_child: i32,
    _thread: u32,
    _time: u32,
) {
    if event == EVENT_OBJECT_LOCATIONCHANGE {
        let followed = STATE.lock().unwrap().followed;
        if id_object != OBJID_WINDOW || hwnd.0 as isize != followed {
            return;
        }
    }
    let id = SetTimer(None, TIMER.load(Ordering::Relaxed), SETTLE_MS, None);
    TIMER.store(id, Ordering::Relaxed);
}

fn decide_now() {
    let Some(app) = APP.get() else { return };
    let foreground = unsafe { GetForegroundWindow() };
    let mut state = STATE.lock().unwrap();

    if state.followed != foreground.0 as isize {
        unsafe {
            if let Some(hook) = state.follow.take() {
                let _ = UnhookWinEvent(HWINEVENTHOOK(hook as _));
            }
            state.followed = foreground.0 as isize;
            if !foreground.is_invalid() {
                let thread = GetWindowThreadProcessId(foreground, None);
                let hook = SetWinEventHook(
                    EVENT_OBJECT_LOCATIONCHANGE,
                    EVENT_OBJECT_LOCATIONCHANGE,
                    None,
                    Some(on_event),
                    0,
                    thread,
                    WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
                );
                state.follow = (!hook.is_invalid()).then_some(hook.0 as isize);
            }
        }
    }

    let place = decide(foreground, state.dock, state.hide_on_fullscreen);
    if place != state.place {
        state.place = place;
        let _ = app.emit_to(overlay::LABEL, "place", place);
        if fullscreen() {
            crate::gamelayer::still_recording(app);
        }
    }
    drop(state);
    crate::live::set_covered(app, desktop_covered());
}

fn desktop_covered() -> u32 {
    unsafe {
        let screens = crate::player::screens();
        let every = if screens.len() >= 32 { u32::MAX } else { (1u32 << screens.len()) - 1 };
        let mut hidden = 0u32;
        let own_process = GetCurrentProcessId();
        let mut next = GetTopWindow(None).ok();
        while let Some(hwnd) = next {
            if is_app_window(hwnd, own_process) {
                let mut r = RECT::default();
                if GetWindowRect(hwnd, &mut r).is_ok() {
                    let zoomed = IsZoomed(hwnd).as_bool();
                    for (index, screen) in screens.iter().enumerate().take(32) {
                        if crate::player::hides(&screen.rect, &r, zoomed) {
                            hidden |= 1 << index;
                        }
                    }
                    if hidden == every {
                        break;
                    }
                }
            }
            next = GetWindow(hwnd, GW_HWNDNEXT).ok();
        }
        hidden
    }
}

fn class_of(hwnd: HWND) -> String {
    let mut class = [0u16; 64];
    let len = unsafe { GetClassNameW(hwnd, &mut class) } as usize;
    String::from_utf16_lossy(&class[..len])
}

fn is_shell(class: &str) -> bool {
    matches!(class, "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "XamlExplorerHostIslandWindow" | "TopLevelWindowForOverflowXamlIsland")
}

pub(crate) unsafe fn is_app_window(hwnd: HWND, own_process: u32) -> bool {
    if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
        return false;
    }
    let overlay_bits = (WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0 | WS_EX_TRANSPARENT.0) as isize;
    if GetWindowLongPtrW(hwnd, GWL_EXSTYLE) & overlay_bits != 0 {
        return false;
    }
    let mut process = 0;
    GetWindowThreadProcessId(hwnd, Some(&mut process));
    if process == own_process || is_shell(&class_of(hwnd)) {
        return false;
    }
    let mut cloaked = 0u32;
    let _ = DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, (&mut cloaked as *mut u32).cast(), 4);
    cloaked == 0
}

unsafe fn something_under_island(center: i32) -> bool {
    let own_process = GetCurrentProcessId();
    let mut next = GetTopWindow(None).ok();
    while let Some(hwnd) = next {
        if is_app_window(hwnd, own_process) {
            let mut r = RECT::default();
            let touches = GetWindowRect(hwnd, &mut r).is_ok()
                && r.top <= ISLAND_DEPTH
                && r.bottom > 0
                && r.left < center + ISLAND_HALF
                && r.right > center - ISLAND_HALF;
            if touches {
                return true;
            }
        }
        next = GetWindow(hwnd, GW_HWNDNEXT).ok();
    }
    false
}

static FULLSCREEN: AtomicBool = AtomicBool::new(false);

pub fn fullscreen() -> bool {
    FULLSCREEN.load(Ordering::Relaxed)
}

pub fn exclusive() -> bool {
    unsafe { SHQueryUserNotificationState() }.is_ok_and(|state| state == QUNS_RUNNING_D3D_FULL_SCREEN)
}

// con la barra escondida, una ventana maximizada tambien ocupa toda la pantalla y no por eso es un juego
fn covers_screen(r: &RECT, width: i32, height: i32, maximized: bool) -> bool {
    !maximized && r.left <= 0 && r.top <= 0 && r.right >= width && r.bottom >= height
}

fn decide(foreground: HWND, dock: Dock, hide_on_fullscreen: bool) -> Place {
    unsafe {
        let (width, height) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));

        let mut r = RECT::default();
        let fullscreen = !foreground.is_invalid()
            && !is_shell(&class_of(foreground))
            && GetWindowRect(foreground, &mut r).is_ok()
            && covers_screen(&r, width, height, IsZoomed(foreground).as_bool());
        FULLSCREEN.store(fullscreen, Ordering::Relaxed);
        if hide_on_fullscreen && fullscreen {
            return Place::Hidden;
        }

        match dock {
            Dock::Top => Place::Top,
            Dock::Right => Place::Right,
            Dock::Auto if something_under_island(width / 2) => Place::Right,
            Dock::Auto => Place::Top,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_maximized_window_is_not_fullscreen_even_with_the_taskbar_hidden() {
        let maximized = RECT { left: -8, top: -8, right: 1928, bottom: 1088 };
        assert!(!covers_screen(&maximized, 1920, 1080, true));
        let game = RECT { left: 0, top: 0, right: 1920, bottom: 1080 };
        assert!(covers_screen(&game, 1920, 1080, false));
        let window = RECT { left: 100, top: 100, right: 900, bottom: 700 };
        assert!(!covers_screen(&window, 1920, 1080, false));
    }
}
