// la ventana de la isla: transparente y recortada para que los clics pasen

use serde::Deserialize;
use tauri::{AppHandle, Manager, WebviewWindow};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CombineRgn, CreateRectRgn, CreateRoundRectRgn, DeleteObject, SetWindowRgn, RGN_OR,
};
use std::sync::atomic::{AtomicIsize, Ordering};

use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowLongPtrW, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    GWL_EXSTYLE, GWL_STYLE, STYLESTRUCT, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    SW_SHOWNOACTIVATE, HWND_TOPMOST, WM_NCACTIVATE, WM_NCPAINT, WM_STYLECHANGING, WS_CAPTION, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_POPUP, WS_SYSMENU, WS_THICKFRAME,
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

unsafe extern "system" fn frameless(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, _data: usize) -> LRESULT {
    match msg {
        WM_STYLECHANGING if wparam.0 as i32 == GWL_STYLE.0 => {
            let style = &mut *(lparam.0 as *mut STYLESTRUCT);
            style.styleNew = (style.styleNew & !FRAME) | WS_POPUP.0;
            DefSubclassProc(hwnd, msg, wparam, lparam)
        }
        WM_NCACTIVATE => DefSubclassProc(hwnd, msg, wparam, LPARAM(-1)),
        WM_NCPAINT | WM_NCUAHDRAWCAPTION | WM_NCUAHDRAWFRAME => LRESULT(0),
        _ => DefSubclassProc(hwnd, msg, wparam, lparam),
    }
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
        unsafe {
            SetWindowRgn(hwnd, Some(CreateRectRgn(0, 0, 1, 1)), false);
        }
        return Ok(());
    }
    let scale = window.scale_factor()?;
    let px = |v: f64| (v * scale).round() as i32;
    unsafe {
        let region = shapes.map(|shapes| {
            let union = CreateRectRgn(0, 0, 0, 0);
            for s in shapes {
                let d = px(s.r * 2.0);
                let part = CreateRoundRectRgn(
                    px(s.x),
                    px(s.y),
                    px(s.x + s.w) + 1,
                    px(s.y + s.h) + 1,
                    d,
                    d,
                );
                CombineRgn(Some(union), Some(union), Some(part), RGN_OR);
                let _ = DeleteObject(part.into());
            }
            union
        });
        SetWindowRgn(hwnd, region, true);
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
            FOCUS_BEFORE.store(GetForegroundWindow().0 as isize, Ordering::Relaxed);
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex & !bit);
            let _ = SetForegroundWindow(hwnd);
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

#[tauri::command]
pub fn set_region(window: WebviewWindow, shapes: Option<Vec<Shape>>) -> Result<(), String> {
    apply_region(&window, shapes.as_deref()).map_err(|e| e.to_string())
}
