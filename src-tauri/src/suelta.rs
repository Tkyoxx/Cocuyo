// soltar archivos en la isla. tauri pone su receptor cuando la ventana nace, pero la isla nace escondida y las ventanitas de webview2
// todavia no existen, asi que windows mostraba el simbolo de prohibido. este se pone cuando la isla ya esta, y otra vez si webview2 las rehace

use std::cell::Cell;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;

use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};
use windows::core::{implement, BOOL};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, POINTL};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::System::Com::{IDataObject, DVASPECT_CONTENT, FORMATETC, TYMED_HGLOBAL};
use windows::Win32::System::Ole::{IDropTarget, IDropTarget_Impl, RegisterDragDrop, RevokeDragDrop, CF_HDROP, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_NONE};
use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};
use windows::Win32::UI::WindowsAndMessaging::EnumChildWindows;

#[implement(IDropTarget)]
struct Catcher {
    app: AppHandle,
    hwnd: HWND,
    files: Cell<bool>,
}

fn paths_of(data: windows_core::Ref<'_, IDataObject>) -> Option<Vec<String>> {
    let format = FORMATETC { cfFormat: CF_HDROP.0, ptd: std::ptr::null_mut(), dwAspect: DVASPECT_CONTENT.0, lindex: -1, tymed: TYMED_HGLOBAL.0 as u32 };
    let medium = unsafe { data.as_ref()?.GetData(&format).ok()? };
    let drop = HDROP(unsafe { medium.u.hGlobal.0 } as _);
    let count = unsafe { DragQueryFileW(drop, 0xFFFF_FFFF, None) };
    let mut all = Vec::new();
    for i in 0..count.min(20) {
        let len = unsafe { DragQueryFileW(drop, i, None) } as usize;
        let mut buffer = vec![0u16; len + 1];
        unsafe { DragQueryFileW(drop, i, Some(&mut buffer)) };
        all.push(OsString::from_wide(&buffer[..len]).to_string_lossy().into_owned());
    }
    Some(all)
}

impl Catcher {
    fn tell(&self, kind: &str, pt: &POINTL, paths: &[String]) {
        let mut at = POINT { x: pt.x, y: pt.y };
        let _ = unsafe { ScreenToClient(self.hwnd, &mut at) };
        let scale = self.app.get_webview_window(crate::overlay::LABEL).and_then(|w| w.scale_factor().ok()).unwrap_or(1.0);
        let _ = self.app.emit_to(crate::overlay::LABEL, "suelta", json!({ "tipo": kind, "x": at.x as f64 / scale, "y": at.y as f64 / scale, "rutas": paths }));
    }
}

#[allow(non_snake_case)]
impl IDropTarget_Impl for Catcher_Impl {
    fn DragEnter(&self, data: windows_core::Ref<'_, IDataObject>, _keys: MODIFIERKEYS_FLAGS, pt: &POINTL, effect: *mut DROPEFFECT) -> windows::core::Result<()> {
        let paths = paths_of(data);
        self.files.set(paths.is_some());
        if let Some(paths) = paths {
            self.tell("enter", pt, &paths);
        }
        unsafe { *effect = if self.files.get() { DROPEFFECT_COPY } else { DROPEFFECT_NONE } };
        Ok(())
    }

    fn DragOver(&self, _keys: MODIFIERKEYS_FLAGS, pt: &POINTL, effect: *mut DROPEFFECT) -> windows::core::Result<()> {
        if self.files.get() {
            self.tell("over", pt, &[]);
        }
        unsafe { *effect = if self.files.get() { DROPEFFECT_COPY } else { DROPEFFECT_NONE } };
        Ok(())
    }

    fn DragLeave(&self) -> windows::core::Result<()> {
        if self.files.replace(false) {
            let _ = self.app.emit_to(crate::overlay::LABEL, "suelta", json!({ "tipo": "leave", "rutas": [] }));
        }
        Ok(())
    }

    fn Drop(&self, data: windows_core::Ref<'_, IDataObject>, _keys: MODIFIERKEYS_FLAGS, pt: &POINTL, effect: *mut DROPEFFECT) -> windows::core::Result<()> {
        let paths = paths_of(data).unwrap_or_default();
        self.files.set(false);
        self.tell("drop", pt, &paths);
        unsafe { *effect = if paths.is_empty() { DROPEFFECT_NONE } else { DROPEFFECT_COPY } };
        Ok(())
    }
}

thread_local! {
    static KEPT: std::cell::RefCell<Vec<(isize, IDropTarget)>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn catch_on(app: &AppHandle, top: HWND, hwnd: HWND) {
    let target: IDropTarget = Catcher { app: app.clone(), hwnd: top, files: Cell::new(false) }.into();
    unsafe {
        let _ = RevokeDragDrop(hwnd);
        if RegisterDragDrop(hwnd, &target).is_ok() {
            KEPT.with(|kept| {
                let mut kept = kept.borrow_mut();
                kept.retain(|(at, _)| *at != hwnd.0 as isize);
                kept.push((hwnd.0 as isize, target));
            });
        }
    }
}

pub fn accept(app: &AppHandle) {
    let Some(window) = app.get_webview_window(crate::overlay::LABEL) else { return };
    let Ok(top) = window.hwnd() else { return };
    let top = top.0 as isize;
    let app = app.clone();
    let _ = window.run_on_main_thread(move || {
        let top = HWND(top as _);
        let mut children: Vec<HWND> = Vec::new();
        unsafe extern "system" fn each(hwnd: HWND, list: LPARAM) -> BOOL {
            unsafe { (*(list.0 as *mut Vec<HWND>)).push(hwnd) };
            true.into()
        }
        unsafe {
            let _ = EnumChildWindows(Some(top), Some(each), LPARAM(&mut children as *mut Vec<HWND> as isize));
        }
        catch_on(&app, top, top);
        for child in children {
            catch_on(&app, top, child);
        }
    });
}

#[tauri::command]
pub fn suelta_lista(app: AppHandle) {
    accept(&app);
}
