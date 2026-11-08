// la ventana de la isla: transparente y recortada para que los clics pasen

use serde::Deserialize;
use tauri::{AppHandle, Manager, WebviewWindow};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CombineRgn, CreateRectRgn, CreateRoundRectRgn, DeleteObject, GetMonitorInfoW, MonitorFromPoint, SetWindowRgn, MONITORINFO,
    MONITOR_DEFAULTTOPRIMARY, RGN_OR,
};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Mutex;

static LAST_REGION: Mutex<Option<Vec<[i32; 5]>>> = Mutex::new(None);

use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentProcessId};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowLongPtrW, GetWindowRect, GetWindowThreadProcessId, PostMessageW, SetForegroundWindow, SetWindowLongPtrW,
    SetWindowPos, ShowWindow, GWL_EXSTYLE, GWL_STYLE, STYLESTRUCT, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_NOZORDER, SW_SHOWNOACTIVATE, HWND_TOPMOST, WM_APP, WM_NCACTIVATE, WM_NCPAINT, WM_STYLECHANGING, WS_CAPTION, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_POPUP, WS_SYSMENU, WS_THICKFRAME,
};

pub const LABEL: &str = "overlay";

#[derive(Deserialize)]
pub struct Shape {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    r: f64,
}

fn hwnd(window: &WebviewWindow) -> tauri::Result<HWND> {
    Ok(HWND(window.hwnd()?.0 as _))
}

const FRAME: u32 = WS_CAPTION.0 | WS_SYSMENU.0 | WS_THICKFRAME.0 | WS_MINIMIZEBOX.0 | WS_MAXIMIZEBOX.0;
const WM_NCUAHDRAWCAPTION: u32 = 0x00AE;
const WM_NCUAHDRAWFRAME: u32 = 0x00AF;
const WM_SETTINGCHANGE: u32 = 0x001A;
const WM_DISPLAYCHANGE: u32 = 0x007E;
const WM_DPICHANGED: u32 = 0x02E0;
const SPI_SETWORKAREA: usize = 0x002F;
const FIT: u32 = WM_APP + 0x0C0C;

unsafe extern "system" fn frameless(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, _data: usize) -> LRESULT {
    match msg {
        WM_STYLECHANGING if wparam.0 as i32 == GWL_STYLE.0 => {
            let style = &mut *(lparam.0 as *mut STYLESTRUCT);
            style.styleNew = (style.styleNew & !FRAME) | WS_POPUP.0;
            DefSubclassProc(hwnd, msg, wparam, lparam)
        }
        WM_NCACTIVATE => DefSubclassProc(hwnd, msg, wparam, LPARAM(-1)),
        WM_NCPAINT | WM_NCUAHDRAWCAPTION | WM_NCUAHDRAWFRAME => LRESULT(0),
        WM_SETTINGCHANGE if wparam.0 == SPI_SETWORKAREA => {
            let _ = PostMessageW(Some(hwnd), FIT, WPARAM(0), LPARAM(0));
            DefSubclassProc(hwnd, msg, wparam, lparam)
        }
        WM_DISPLAYCHANGE | WM_DPICHANGED => {
            let _ = PostMessageW(Some(hwnd), FIT, WPARAM(0), LPARAM(0));
            DefSubclassProc(hwnd, msg, wparam, lparam)
        }
        FIT => {
            fit(hwnd);
            LRESULT(0)
        }
        _ => DefSubclassProc(hwnd, msg, wparam, lparam),
    }
}

// al prender el pc la barra a veces llega despues que cocuyo, o se esconde sola: la ventana sigue el escritorio de verdad
unsafe fn fit(hwnd: HWND) {
    if crate::placement::exclusive() {
        return;
    }
    let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    if !GetMonitorInfoW(MonitorFromPoint(POINT::default(), MONITOR_DEFAULTTOPRIMARY), &mut info).as_bool() {
        return;
    }
    let work = info.rcWork;
    let mut now = RECT::default();
    if work.right <= work.left || work.bottom <= work.top || (GetWindowRect(hwnd, &mut now).is_ok() && now == work) {
        return;
    }
    let _ = SetWindowPos(hwnd, None, work.left, work.top, work.right - work.left, work.bottom - work.top, SWP_NOZORDER | SWP_NOACTIVATE);
}

pub fn prepare(window: &WebviewWindow) -> tauri::Result<()> {
    let hwnd = hwnd(window)?;
    unsafe {
        let _ = SetWindowSubclass(hwnd, Some(frameless), 1, 0);
        SetWindowLongPtrW(hwnd, GWL_STYLE, GetWindowLongPtrW(hwnd, GWL_STYLE));
        let _ = SetWindowPos(hwnd, None, 0, 0, 0, 0, SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);

        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let extra = (WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0) as isize;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | extra);
        SetWindowRgn(hwnd, Some(CreateRectRgn(0, 0, 0, 0)), false);
    }
    if let Some(monitor) = window.primary_monitor()? {
        let area = monitor.work_area();
        window.set_position(area.position)?;
        window.set_size(area.size)?;
    }
    Ok(())
}

pub fn apply_region(window: &WebviewWindow, shapes: Option<&[Shape]>) -> tauri::Result<()> {
    let hwnd = hwnd(window)?;
    if crate::placement::exclusive() {
        *LAST_REGION.lock().unwrap() = Some(vec![[0, 0, 1, 1, -1]]);
        unsafe {
            SetWindowRgn(hwnd, Some(CreateRectRgn(0, 0, 1, 1)), false);
        }
        return Ok(());
    }
    let scale = window.scale_factor()?;
    let px = |v: f64| (v * scale).round() as i32;
    let wanted = shapes.map(|shapes| shapes.iter().map(|s| [px(s.x), px(s.y), px(s.x + s.w) + 1, px(s.y + s.h) + 1, px(s.r * 2.0)]).collect::<Vec<_>>());
    let key = wanted.clone().unwrap_or_else(|| vec![[0, 0, 0, 0, -2]]);
    let same = {
        let mut last = LAST_REGION.lock().unwrap();
        let same = last.as_ref() == Some(&key);
        *last = Some(key);
        same
    };
    unsafe {
        if !same {
            let region = wanted.map(|parts| {
                let union = CreateRectRgn(0, 0, 0, 0);
                for [l, t, r, b, d] in parts {
                    let part = CreateRoundRectRgn(l, t, r, b, d, d);
                    CombineRgn(Some(union), Some(union), Some(part), RGN_OR);
                    let _ = DeleteObject(part.into());
                }
                union
            });
            SetWindowRgn(hwnd, region, true);
        }
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
    Ok(())
}

pub fn hide(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let pixel = Shape { x: 0.0, y: 0.0, w: 0.0, h: 0.0, r: 0.0 };
        let _ = apply_region(&window, Some(&[pixel]));
    }
}

pub fn open_fully(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = apply_region(&window, None);
    }
}

pub fn cursor_in_window(app: &AppHandle) -> Option<(f64, f64)> {
    let window = app.get_webview_window(LABEL)?;
    let cursor = app.cursor_position().ok()?;
    let origin = window.outer_position().ok()?;
    let scale = window.scale_factor().ok()?;
    Some((
        (cursor.x - origin.x as f64) / scale,
        (cursor.y - origin.y as f64) / scale,
    ))
}

static FOCUS_BEFORE: AtomicIsize = AtomicIsize::new(0);

#[tauri::command]
pub fn set_focusable(window: WebviewWindow, on: bool) -> Result<(), String> {
    let hwnd = hwnd(&window).map_err(|e| e.to_string())?;
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let bit = WS_EX_NOACTIVATE.0 as isize;
        if on {
            let before = GetForegroundWindow();
            if before != hwnd {
                FOCUS_BEFORE.store(before.0 as isize, Ordering::Relaxed);
            }
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex & !bit);
            if !SetForegroundWindow(hwnd).as_bool() {
                borrow_front(hwnd, before);
            }
            window.set_focus().map_err(|e| e.to_string())?;
        } else {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | bit);
            let before = FOCUS_BEFORE.swap(0, Ordering::Relaxed);
            if before != 0 {
                let _ = SetForegroundWindow(HWND(before as _));
            }
        }
    }
    Ok(())
}

// a veces windows no le deja el teclado a la isla aunque el clic fue en ella: se le pide prestado a la de delante un momento
unsafe fn borrow_front(hwnd: HWND, front: HWND) {
    let mine = GetWindowThreadProcessId(hwnd, None);
    let theirs = GetWindowThreadProcessId(front, None);
    if theirs == 0 || theirs == mine {
        return;
    }
    if AttachThreadInput(mine, theirs, true).as_bool() {
        let _ = SetForegroundWindow(hwnd);
        let _ = AttachThreadInput(mine, theirs, false);
    }
}

#[tauri::command]
pub fn foco_en_la_isla() -> bool {
    unsafe {
        let mut process = 0;
        GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut process));
        process != 0 && process == GetCurrentProcessId()
    }
}

#[tauri::command]
pub fn set_region(window: WebviewWindow, shapes: Option<Vec<Shape>>) -> Result<(), String> {
    apply_region(&window, shapes.as_deref()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::core::w;
    use windows::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, DispatchMessageW, PeekMessageW, SendMessageW, MSG, PM_REMOVE, WINDOW_EX_STYLE};

    #[test]
    fn the_window_follows_the_desktop_when_the_taskbar_moves() {
        unsafe {
            let window = CreateWindowExW(WINDOW_EX_STYLE(0), w!("STATIC"), w!("isla"), WS_POPUP, 0, 0, 300, 200, None, None, None, None).unwrap();
            let _ = SetWindowSubclass(window, Some(frameless), 1, 0);
            SendMessageW(window, WM_SETTINGCHANGE, Some(WPARAM(0)), Some(LPARAM(0)));
            let mut message = MSG::default();
            while PeekMessageW(&mut message, Some(window), 0, 0, PM_REMOVE).as_bool() {
                DispatchMessageW(&message);
            }
            let mut now = RECT::default();
            GetWindowRect(window, &mut now).unwrap();
            assert_eq!((now.right - now.left, now.bottom - now.top), (300, 200));

            SendMessageW(window, WM_SETTINGCHANGE, Some(WPARAM(SPI_SETWORKAREA)), Some(LPARAM(0)));
            while PeekMessageW(&mut message, Some(window), 0, 0, PM_REMOVE).as_bool() {
                DispatchMessageW(&message);
            }
            let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            assert!(GetMonitorInfoW(MonitorFromPoint(POINT::default(), MONITOR_DEFAULTTOPRIMARY), &mut info).as_bool());
            GetWindowRect(window, &mut now).unwrap();
            assert_eq!(now, info.rcWork);
            let _ = DestroyWindow(window);
        }
    }
}
