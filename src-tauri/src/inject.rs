// pegar y copiar con teclas de mentira

use std::sync::Mutex;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HANDLE, HGLOBAL};
use windows::core::{w, PCWSTR};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber, OpenClipboard, RegisterClipboardFormatW,
    SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_KEYUP, VIRTUAL_KEY,
};

const CF_UNICODETEXT: u32 = 13;
const VK_CONTROL: u16 = 0x11;
const VK_V: u16 = 0x56;

unsafe fn open_clipboard() -> bool {
    for _ in 0..10 {
        if OpenClipboard(None).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    false
}

pub fn clipboard_text() -> Option<String> {
    unsafe {
        if !open_clipboard() {
            return None;
        }
        let text = GetClipboardData(CF_UNICODETEXT).ok().and_then(|handle| {
            let memory = HGLOBAL(handle.0);
            let ptr = GlobalLock(memory) as *const u16;
            if ptr.is_null() {
                return None;
            }
            let len = (0..).take_while(|&i| *ptr.add(i) != 0).count();
            let text = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len));
            let _ = GlobalUnlock(memory);
            Some(text)
        });
        let _ = CloseClipboard();
        text
    }
}

pub fn set_clipboard_text(text: &str) -> bool {
    put(text, false)
}

const OUT_OF_HISTORY: [PCWSTR; 3] = [w!("ExcludeClipboardContentFromMonitorProcessing"), w!("CanIncludeInClipboardHistory"), w!("CanUploadToCloudClipboard")];

unsafe fn zero_dword(format: u32) {
    let Ok(memory) = GlobalAlloc(GMEM_MOVEABLE, 4) else { return };
    let ptr = GlobalLock(memory) as *mut u32;
    if ptr.is_null() {
        return;
    }
    *ptr = 0;
    let _ = GlobalUnlock(memory);
    let _ = SetClipboardData(format, Some(HANDLE(memory.0)));
}

fn put(text: &str, private: bool) -> bool {
    let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    unsafe {
        let Ok(memory) = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2) else { return false };
        let ptr = GlobalLock(memory) as *mut u16;
        if ptr.is_null() {
            return false;
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
        let _ = GlobalUnlock(memory);
        if !open_clipboard() {
            return false;
        }
        let _ = EmptyClipboard();
        let placed = SetClipboardData(CF_UNICODETEXT, Some(HANDLE(memory.0))).is_ok();
        if placed && private {
            for name in OUT_OF_HISTORY {
                let format = RegisterClipboardFormatW(name);
                if format != 0 {
                    zero_dword(format);
                }
            }
        }
        let _ = CloseClipboard();
        placed
    }
}

struct Restore {
    ours: u32,
    previous: Option<String>,
}

static PENDING: Mutex<Option<Restore>> = Mutex::new(None);

fn clipboard_seq() -> u32 {
    unsafe { GetClipboardSequenceNumber() }
}

fn place_for_paste(text: &str) -> Result<u32, String> {
    let previous = match PENDING.lock().unwrap().take() {
        Some(pending) if pending.ours == clipboard_seq() => pending.previous,
        _ => clipboard_text(),
    };
    if !put(text, true) {
        return Err("No se pudo usar el portapapeles.".into());
    }
    let ours = clipboard_seq();
    *PENDING.lock().unwrap() = Some(Restore { ours, previous });
    Ok(ours)
}

fn restore_later(ours: u32) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(700));
        let mut pending = PENDING.lock().unwrap();
        if pending.as_ref().is_some_and(|p| p.ours == ours) {
            let previous = pending.take().and_then(|p| p.previous);
            if clipboard_seq() == ours {
                if let Some(previous) = previous {
                    crate::clip::ignore_own_changes();
                    set_clipboard_text(&previous);
                }
            }
        }
    });
}

fn focused_is_text() -> Option<bool> {
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
    use windows::Win32::UI::Accessibility::*;
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let uia: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
        let kind = uia.GetFocusedElement().ok()?.CurrentControlType().ok()?;
        if kind == UIA_EditControlTypeId || kind == UIA_ComboBoxControlTypeId {
            return Some(true);
        }
        let never = [
            UIA_ListItemControlTypeId,
            UIA_ListControlTypeId,
            UIA_ButtonControlTypeId,
            UIA_MenuItemControlTypeId,
            UIA_TreeItemControlTypeId,
            UIA_TabItemControlTypeId,
            UIA_HyperlinkControlTypeId,
            UIA_CheckBoxControlTypeId,
            UIA_RadioButtonControlTypeId,
            UIA_ImageControlTypeId,
        ];
        never.contains(&kind).then_some(false)
    }
}

pub fn typing_place() -> Option<bool> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(focused_is_text());
    });
    rx.recv_timeout(Duration::from_millis(400)).ok().flatten()
}

pub enum Pasted {
    Here,
    Copied,
}

pub fn paste_dictation(text: &str) -> Result<Pasted, String> {
    if typing_place() != Some(false) {
        return paste(text).map(|()| Pasted::Here);
    }
    crate::clip::ignore_own_changes();
    PENDING.lock().unwrap().take();
    if !put(text, true) {
        return Err("No se pudo usar el portapapeles.".into());
    }
    Ok(Pasted::Copied)
}

fn key(vk: u16, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

pub fn tap(vk: u16) {
    unsafe {
        SendInput(&[key(vk, false), key(vk, true)], std::mem::size_of::<INPUT>() as i32);
    }
}

fn wait_for_modifiers_up() {
    let deadline = Instant::now() + Duration::from_millis(900);
    let held = || unsafe { [0x10, 0x11, 0x12, 0x5B, 0x5C].iter().any(|&vk| GetAsyncKeyState(vk) < 0) };
    while held() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
}

pub fn copy_selection() -> (Option<String>, Option<String>) {
    wait_for_modifiers_up();
    crate::clip::ignore_own_changes();
    let before = clipboard_text();
    set_clipboard_text("");
    unsafe {
        let keys = [key(VK_CONTROL, false), key(0x43, false), key(0x43, true), key(VK_CONTROL, true)];
        SendInput(&keys, std::mem::size_of::<INPUT>() as i32);
    }
    let deadline = Instant::now() + Duration::from_millis(600);
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(30));
        if let Some(text) = clipboard_text().filter(|text| !text.trim().is_empty()) {
            return (Some(text), before);
        }
    }
    (None, before)
}

pub fn paste(text: &str) -> Result<(), String> {
    wait_for_modifiers_up();
    crate::clip::ignore_own_changes();
    let ours = place_for_paste(text)?;
    unsafe {
        let keys = [key(VK_CONTROL, false), key(VK_V, false), key(VK_V, true), key(VK_CONTROL, true)];
        let sent = SendInput(&keys, std::mem::size_of::<INPUT>() as i32);
        if cfg!(debug_assertions) {
            use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW};
            let mut title = [0u16; 128];
            let len = GetWindowTextW(GetForegroundWindow(), &mut title) as usize;
            eprintln!("[pegar] {sent} teclas a «{}»", String::from_utf16_lossy(&title[..len]));
        }
    }
    std::thread::sleep(Duration::from_millis(180));
    restore_later(ours);
    Ok(())
}
