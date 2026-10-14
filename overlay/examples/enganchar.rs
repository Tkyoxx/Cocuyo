#[path = "../src/shared.rs"]
#[allow(dead_code)]
mod shared;

use std::time::Duration;

use shared::{Shared, CAPTION_APP, CAPTION_NONE, CAPTION_YOU, MAP_NAME, PILL_DONE, PILL_HIDDEN, PILL_LISTENING, PILL_THINKING};
use windows::core::{s, w, HSTRING, PCWSTR};
use windows::Win32::Foundation::{INVALID_HANDLE_VALUE, LPARAM, WPARAM};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::Win32::System::Memory::{CreateFileMappingW, MapViewOfFile, FILE_MAP_WRITE, PAGE_READWRITE};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GetWindowThreadProcessId, PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx, HOOKPROC, WH_GETMESSAGE,
    WM_NULL,
};

unsafe fn caption(shared: *mut Shared, kind: u32, who: &str, text: &str) {
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
    println!("subtítulo {kind}: {text}");
}

fn main() {
    let pause = |ms: u64| std::thread::sleep(Duration::from_millis(ms));
    unsafe {
        let window = FindWindowW(None, w!("Juego de prueba de Cocuyo")).expect("abre antes el juego de prueba");
        let thread = GetWindowThreadProcessId(window, None);

        let name: Vec<u16> = MAP_NAME.encode_utf16().chain(Some(0)).collect();
        let size = std::mem::size_of::<Shared>() as u32;
        let mapping = CreateFileMappingW(INVALID_HANDLE_VALUE, None, PAGE_READWRITE, 0, size, PCWSTR(name.as_ptr())).expect("bloque");
        let shared: *mut Shared = MapViewOfFile(mapping, FILE_MAP_WRITE, 0, 0, size as usize).Value.cast();
        (*shared).enabled = 1;
        (*shared).scale = 1.0;
        (*shared).pill = PILL_LISTENING;

        let library = std::env::current_exe().unwrap().parent().unwrap().parent().unwrap().join("cocuyo_overlay.dll");
        let module = LoadLibraryW(&HSTRING::from(library.as_os_str())).expect("librería");
        let entry: HOOKPROC = std::mem::transmute(GetProcAddress(module, s!("cocuyo_hook")));
        let hook = SetWindowsHookExW(WH_GETMESSAGE, entry, Some(module.into()), thread).expect("gancho");
        let _ = PostThreadMessageW(thread, WM_NULL, WPARAM(0), LPARAM(0));
        println!("gancho puesto en el hilo {thread}: escuchando");

        for tick in 0..60 {
            (*shared).level = (0.55 + 0.45 * (tick as f32 * 0.9).sin()).clamp(0.0, 1.0);
            pause(50);
        }
        let _ = UnhookWindowsHookEx(hook);
        (*shared).level = 0.0;
        (*shared).pill = PILL_THINKING;
        println!("pensando");
        pause(2400);
        (*shared).pill = PILL_DONE;
        caption(shared, CAPTION_YOU, "Tú", "Pon Babydoll de Dominic Fike.");
        pause(950);
        (*shared).pill = PILL_HIDDEN;
        pause(350);
        caption(shared, CAPTION_APP, "Cocuyo", "Sonando «Babydoll» de Dominic Fike.");
        pause(3400);
        caption(shared, CAPTION_NONE, "", "");
        pause(800);
    }
}
