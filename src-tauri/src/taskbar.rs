// la barra de tareas a tu gusto. todo por eventos: si no cambia nada, este hilo ni se despierta

use std::cell::Cell;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use windows::core::{s, w, BOOL};
use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{MonitorFromWindow, MONITOR_DEFAULTTONEAREST};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows::Win32::System::Registry::{RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, REG_DWORD, RRF_RT_REG_SZ};
use windows::Win32::System::Threading::{GetCurrentProcessId, GetCurrentThreadId};
use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation, SetWinEventHook, TreeScope_Children, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::Shell::{SHAppBarMessage, ABM_GETSTATE, ABM_SETSTATE, ABS_AUTOHIDE, APPBARDATA};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, EnumChildWindows, FindWindowExW, FindWindowW, GetClassNameW, GetParent, GetTopWindow, GetWindow, GetWindowRect,
    GetWindowThreadProcessId, IsWindow, IsZoomed, MsgWaitForMultipleObjectsEx, PeekMessageW, PostThreadMessageW, SendMessageTimeoutW,
    SendNotifyMessageW, SetWindowPos, EVENT_OBJECT_CREATE, EVENT_OBJECT_HIDE, EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_REORDER, EVENT_SYSTEM_FOREGROUND, GW_HWNDNEXT, HWND_BROADCAST,
    MSG, MWMO_INPUTAVAILABLE, PM_NOREMOVE, PM_REMOVE, QS_ALLINPUT, SMTO_ABORTIFHUNG, SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, WINEVENT_OUTOFCONTEXT,
    WINEVENT_SKIPOWNPROCESS, WM_APP, WM_SETTINGCHANGE,
};

const WAKE: u32 = WM_APP + 7;
const WM_DWMCOMPOSITIONCHANGED: u32 = 0x031E;
const SETTLE: Duration = Duration::from_millis(120);
const GLIDE: Duration = Duration::from_millis(320);

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Style {
    #[default]
    Normal,
    Clear,
    Blur,
    Acrylic,
}

impl Style {
    pub fn from_name(name: &str) -> Style {
        match name {
            "clear" => Style::Clear,
            "blur" => Style::Blur,
            "acrylic" => Style::Acrylic,
            _ => Style::Normal,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Look {
    pub style: Style,
    pub opacity: u8,
    pub center: bool,
    pub solid_when_max: bool,
}

impl Look {
    fn untouched(&self) -> bool {
        self.style == Style::Normal && !self.center
    }
}

static LOOK: Mutex<Option<Look>> = Mutex::new(None);
static THREAD: Mutex<Option<u32>> = Mutex::new(None);

thread_local! {
    static DUE: Cell<Option<Instant>> = const { Cell::new(None) };
}

#[repr(C)]
struct Accent {
    state: u32,
    flags: u32,
    color: u32,
    animation: u32,
}

#[repr(C)]
struct Composition {
    attribute: u32,
    data: *mut Accent,
    size: usize,
}

type SetComposition = unsafe extern "system" fn(HWND, *mut Composition) -> BOOL;

fn set_composition() -> Option<SetComposition> {
    static FOUND: std::sync::OnceLock<Option<usize>> = std::sync::OnceLock::new();
    let address = *FOUND.get_or_init(|| unsafe {
        let user32 = GetModuleHandleW(w!("user32.dll")).ok()?;
        GetProcAddress(user32, s!("SetWindowCompositionAttribute")).map(|f| f as usize)
    });
    address.map(|a| unsafe { std::mem::transmute::<usize, SetComposition>(a) })
}

fn accent(style: Style, opacity: u8) -> (u32, u32) {
    let alpha = (u32::from(opacity.min(100)) * 255 + 50) / 100;
    match style {
        Style::Normal => (0, 0),
        Style::Clear => (2, alpha << 24),
        Style::Blur => (3, alpha << 24),
        Style::Acrylic => (4, alpha.max(1) << 24), // con alfa 0 el acrilico se traba al arrastrar ventanas, cosas de windows
    }
}

unsafe fn paint(bar: HWND, style: Style, opacity: u8) {
    if style == Style::Normal {
        let _ = SendNotifyMessageW(bar, WM_DWMCOMPOSITIONCHANGED, WPARAM(1), LPARAM(0));
        return;
    }
    let Some(set) = set_composition() else { return };
    let (state, color) = accent(style, opacity);
    let mut accent = Accent { state, flags: 2, color, animation: 0 };
    let mut data = Composition { attribute: 19, data: &mut accent, size: std::mem::size_of::<Accent>() };
    let _ = set(bar, &mut data);
}

fn build() -> u32 {
    let mut text = [0u16; 16];
    let mut size = std::mem::size_of_val(&text) as u32;
    let read = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w!("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion"),
            w!("CurrentBuildNumber"),
            RRF_RT_REG_SZ,
            None,
            Some(text.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if read.is_err() {
        return 0;
    }
    String::from_utf16_lossy(&text).trim_end_matches('\0').trim().parse().unwrap_or(0)
}

pub fn windows11() -> bool {
    static ELEVEN: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ELEVEN.get_or_init(|| build() >= 22000)
}

fn bars() -> Vec<HWND> {
    let mut found = Vec::new();
    unsafe {
        if let Ok(main) = FindWindowW(w!("Shell_TrayWnd"), None) {
            found.push(main);
        }
        let mut after = None;
        while let Ok(other) = FindWindowExW(None, after, w!("Shell_SecondaryTrayWnd"), None) {
            found.push(other);
            after = Some(other);
        }
    }
    found
}

fn class_of(hwnd: HWND) -> String {
    let mut class = [0u16; 64];
    let len = unsafe { GetClassNameW(hwnd, &mut class) } as usize;
    String::from_utf16_lossy(&class[..len])
}

unsafe extern "system" fn find_list(hwnd: HWND, out: LPARAM) -> BOOL {
    if class_of(hwnd) == "MSTaskListWClass" {
        *(out.0 as *mut Option<HWND>) = Some(hwnd);
        return false.into();
    }
    true.into()
}

fn task_list(bar: HWND) -> Option<HWND> {
    let mut found: Option<HWND> = None;
    unsafe {
        let _ = EnumChildWindows(Some(bar), Some(find_list), LPARAM(&mut found as *mut Option<HWND> as isize));
    }
    found
}

fn rect(hwnd: HWND) -> Option<RECT> {
    let mut r = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut r) }.ok().map(|_| r)
}

fn centered(bar: (i32, i32), parent: (i32, i32), icons: i32, lead: i32) -> i32 {
    let want = bar.0 + (bar.1 - bar.0 - icons) / 2;
    (want - parent.0 - lead).clamp(0, (parent.1 - parent.0 - icons - lead).max(0))
}

fn eased(from: i32, to: i32, t: f32) -> i32 {
    let t = t.clamp(0.0, 1.0);
    let k = 1.0 - (1.0 - t).powi(3);
    from + ((to - from) as f32 * k).round() as i32
}

unsafe fn icons(uia: &IUIAutomation, list: HWND) -> Option<(i32, i32)> {
    let element = uia.ElementFromHandle(list).ok()?;
    let all = element.FindAll(TreeScope_Children, &uia.CreateTrueCondition().ok()?).ok()?;
    let mut span: Option<(i32, i32)> = None;
    for index in 0..all.Length().ok()? {
        let Ok(r) = all.GetElement(index).and_then(|e| e.CurrentBoundingRectangle()) else { continue };
        if r.right > r.left {
            span = Some(span.map_or((r.left, r.right), |(a, b)| (a.min(r.left), b.max(r.right))));
        }
    }
    span
}

unsafe fn glide(list: HWND, y: i32, from: i32, to: i32) {
    let flags = SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS;
    if (to - from).abs() > 2 {
        let start = Instant::now();
        while start.elapsed() < GLIDE {
            let x = eased(from, to, start.elapsed().as_secs_f32() / GLIDE.as_secs_f32());
            let _ = SetWindowPos(list, None, x, y, 0, 0, flags);
            std::thread::sleep(Duration::from_millis(15));
        }
    }
    let _ = SetWindowPos(list, None, to, y, 0, 0, flags);
}

unsafe fn center(uia: Option<&IUIAutomation>, bar: HWND, on: bool) {
    let Some(list) = task_list(bar) else { return };
    let Ok(parent) = GetParent(list) else { return };
    let (Some(b), Some(p), Some(l)) = (rect(bar), rect(parent), rect(list)) else { return };
    if b.bottom - b.top > b.right - b.left {
        return;
    }
    let now = l.left - p.left;
    let y = l.top - p.top;
    let target = match (on, uia.and_then(|uia| icons(uia, list))) {
        (true, Some((first, last))) => centered((b.left, b.right), (p.left, p.right), last - first, first - l.left),
        (true, None) => return,
        (false, _) => 0,
    };
    if (target - now).abs() > 1 {
        glide(list, y, now, target);
    }
}

unsafe fn maximized_on(bar: HWND) -> bool {
    let monitor = MonitorFromWindow(bar, MONITOR_DEFAULTTONEAREST);
    let own = GetCurrentProcessId();
    let mut next = GetTopWindow(None).ok();
    while let Some(hwnd) = next {
        if IsZoomed(hwnd).as_bool() && crate::placement::is_app_window(hwnd, own) && MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) == monitor {
            return true;
        }
        next = GetWindow(hwnd, GW_HWNDNEXT).ok();
    }
    false
}

fn set_alignment(center: bool) {
    let value: u32 = if center { 1 } else { 0 };
    unsafe {
        let _ = RegSetKeyValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Advanced"),
            w!("TaskbarAl"),
            REG_DWORD.0,
            Some((&value as *const u32).cast()),
            4,
        );
        let _ = SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, WPARAM(0), LPARAM(w!("TraySettings").as_ptr() as isize), SMTO_ABORTIFHUNG, 200, None);
    }
}

unsafe extern "system" fn on_event(_hook: HWINEVENTHOOK, _event: u32, _hwnd: HWND, _object: i32, _child: i32, _thread: u32, _time: u32) {
    DUE.with(|due| {
        if due.get().is_none() {
            due.set(Some(Instant::now() + SETTLE));
        }
    });
}

struct Hooks {
    on: Vec<HWINEVENTHOOK>,
    shell: u32,
}

impl Hooks {
    unsafe fn set(&mut self, wanted: bool) {
        let shell = bars().first().map(|bar| GetWindowThreadProcessId(*bar, None)).unwrap_or(0);
        if wanted && !self.on.is_empty() && shell == self.shell {
            return;
        }
        for hook in self.on.drain(..) {
            let _ = UnhookWinEvent(hook);
        }
        self.shell = 0;
        if !wanted || shell == 0 {
            return;
        }
        let mut explorer = 0;
        GetWindowThreadProcessId(bars()[0], Some(&mut explorer));
        let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
        self.on.push(SetWinEventHook(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND, None, Some(on_event), 0, 0, flags));
        for (from, to) in [(EVENT_OBJECT_CREATE, EVENT_OBJECT_HIDE), (EVENT_OBJECT_REORDER, EVENT_OBJECT_REORDER), (EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_LOCATIONCHANGE)] {
            self.on.push(SetWinEventHook(from, to, None, Some(on_event), explorer, shell, flags));
        }
        self.on.retain(|hook| !hook.is_invalid());
        self.shell = shell;
    }
}

fn run(ready: std::sync::mpsc::Sender<u32>) {
    unsafe {
        let mut queue = MSG::default();
        let _ = PeekMessageW(&mut queue, None, 0, 0, PM_NOREMOVE);
        let _ = ready.send(GetCurrentThreadId());
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let uia: Option<IUIAutomation> = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok();
        let mut hooks = Hooks { on: Vec::new(), shell: 0 };
        let mut shown = Look::default();
        let mut painted: std::collections::HashMap<isize, Style> = std::collections::HashMap::new();
        let mut first = true;
        let eleven = windows11();
        loop {
            let wait = DUE.with(|due| due.get()).map_or(u32::MAX, |at| at.saturating_duration_since(Instant::now()).as_millis() as u32);
            if wait > 0 {
                MsgWaitForMultipleObjectsEx(None, wait, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
            }
            let mut message = MSG::default();
            let mut changed = false;
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                if message.message == WAKE {
                    changed = true;
                }
                DispatchMessageW(&message);
            }
            let ready = DUE.with(|due| due.get().is_some_and(|at| at <= Instant::now()));
            if !changed && !ready {
                continue;
            }
            DUE.with(|due| due.set(None));
            let Some(look) = *LOOK.lock().unwrap() else { continue };
            hooks.set(!look.untouched());

            if changed && eleven && (look.center != shown.center || (first && look.center)) {
                set_alignment(look.center);
            }
            painted.retain(|bar, _| IsWindow(Some(HWND(*bar as _))).as_bool());
            for bar in bars() {
                let plain = look.solid_when_max && maximized_on(bar);
                let style = if plain { Style::Normal } else { look.style };
                let before = painted.get(&(bar.0 as isize)).copied();
                if style != Style::Normal {
                    if changed && before == Some(style) && shown.opacity != look.opacity {
                        let start = Instant::now();
                        while start.elapsed() < Duration::from_millis(220) {
                            let t = start.elapsed().as_secs_f32() / 0.22;
                            paint(bar, style, eased(i32::from(shown.opacity), i32::from(look.opacity), t).clamp(0, 100) as u8);
                            std::thread::sleep(Duration::from_millis(15));
                        }
                    }
                    paint(bar, style, look.opacity);
                } else if before.is_some_and(|was| was != Style::Normal) {
                    paint(bar, Style::Normal, 0);
                }
                painted.insert(bar.0 as isize, style);
                if !eleven && (look.center || shown.center) {
                    center(uia.as_ref(), bar, look.center);
                }
            }
            shown = look;
            first = false;
        }
    }
}

pub fn configure(look: Look) {
    let before = LOOK.lock().unwrap().replace(look);
    if before == Some(look) {
        return;
    }
    if before.is_none() && look.untouched() {
        return;
    }
    let mut thread = THREAD.lock().unwrap();
    if thread.is_none() {
        let (sender, receiver) = std::sync::mpsc::channel();
        let _ = std::thread::Builder::new().name("barra".into()).spawn(move || run(sender));
        *thread = receiver.recv_timeout(Duration::from_secs(2)).ok();
    }
    if let Some(id) = *thread {
        unsafe {
            let _ = PostThreadMessageW(id, WAKE, WPARAM(0), LPARAM(0));
        }
    }
}

pub fn restore() {
    let shown = LOOK.lock().unwrap().take();
    if shown.is_none_or(|look| look.untouched()) {
        return;
    }
    unsafe {
        let centered = shown.is_some_and(|look| look.center);
        for bar in bars() {
            if shown.is_some_and(|look| look.style != Style::Normal) {
                paint(bar, Style::Normal, 0);
            }
            if centered && !windows11() {
                if let Some(list) = task_list(bar) {
                    if let (Ok(parent), Some(l)) = (GetParent(list), rect(list)) {
                        if let Some(p) = rect(parent) {
                            let _ = SetWindowPos(list, None, 0, l.top - p.top, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
                        }
                    }
                }
            }
        }
    }
}

fn bar_data() -> APPBARDATA {
    APPBARDATA { cbSize: std::mem::size_of::<APPBARDATA>() as u32, hWnd: bars().first().copied().unwrap_or_default(), ..Default::default() }
}

#[derive(Serialize)]
pub struct BarState {
    autohide: bool,
    windows11: bool,
    rival: Option<String>,
}

const RIVALS: [&str; 6] = ["TaskbarX", "TranslucentTB", "RoundedTB", "StartAllBack", "StartIsBack", "ExplorerPatcher"];

fn rival() -> Option<String> {
    let mut found = None;
    for process in crate::system::process_names() {
        if let Some(name) = RIVALS.iter().find(|rival| process.eq_ignore_ascii_case(&format!("{rival}.exe")) || process.eq_ignore_ascii_case(rival)) {
            found = Some(name.to_string());
        }
    }
    found
}

#[tauri::command]
pub fn taskbar_state() -> BarState {
    let mut data = bar_data();
    let state = unsafe { SHAppBarMessage(ABM_GETSTATE, &mut data) } as u32;
    BarState { autohide: state & ABS_AUTOHIDE != 0, windows11: windows11(), rival: rival() }
}

#[tauri::command]
pub fn taskbar_autohide(on: bool) {
    let mut data = bar_data();
    data.lParam = LPARAM(if on { ABS_AUTOHIDE as isize } else { 0 });
    unsafe {
        SHAppBarMessage(ABM_SETSTATE, &mut data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn how_see_through_each_style_is() {
        assert_eq!(accent(Style::Clear, 0), (2, 0));
        assert_eq!(accent(Style::Clear, 100), (2, 0xFF00_0000));
        assert_eq!(accent(Style::Blur, 50), (3, 128 << 24));
        assert_eq!(accent(Style::Acrylic, 0), (4, 1 << 24));
        assert_eq!(accent(Style::Clear, 250), (2, 0xFF00_0000));
        assert_eq!(Style::from_name("blur"), Style::Blur);
        assert_eq!(Style::from_name("lo que sea"), Style::Normal);
    }

    #[test]
    fn the_icons_land_in_the_middle_without_leaving_their_strip() {
        assert_eq!(centered((0, 1920), (150, 1700), 400, 0), 610);
        assert_eq!(centered((0, 1920), (150, 1700), 1600, 0), 0);
        assert_eq!(centered((0, 1920), (150, 1000), 400, 0), 450);
        assert_eq!(centered((1920, 3840), (2000, 3500), 200, 0), 780);
        assert_eq!(centered((0, 1920), (150, 1700), 400, 10), 600);
    }

    #[test]
    fn the_glide_eases_out_and_lands_exactly() {
        assert_eq!(eased(0, 600, 0.0), 0);
        assert_eq!(eased(0, 600, 1.0), 600);
        assert!(eased(0, 600, 0.5) > 450);
        assert_eq!(eased(600, 0, 2.0), 0);
    }

    unsafe fn accent_now(bar: HWND) -> (u32, u32) {
        let user32 = GetModuleHandleW(w!("user32.dll")).unwrap();
        let get: SetComposition = std::mem::transmute(GetProcAddress(user32, s!("GetWindowCompositionAttribute")).unwrap());
        let mut accent = Accent { state: 99, flags: 0, color: 0, animation: 0 };
        let mut data = Composition { attribute: 19, data: &mut accent, size: std::mem::size_of::<Accent>() };
        let _ = get(bar, &mut data);
        (accent.state, accent.color)
    }

    unsafe fn list_x(bar: HWND) -> i32 {
        let list = task_list(bar).unwrap();
        rect(list).unwrap().left - rect(GetParent(list).unwrap()).unwrap().left
    }

    #[test]
    #[ignore]
    fn barra_numeros() {
        unsafe {
            let bar = bars()[0];
            *LOOK.lock().unwrap() = Some(Look { center: true, style: Style::Clear, ..Look::default() });
            restore();
            std::thread::sleep(Duration::from_millis(700));
            println!("barra: tras restaurar x={} acento={:?}", list_x(bar), accent_now(bar));
            let started = Instant::now();
            configure(Look { style: Style::Clear, opacity: 0, center: true, solid_when_max: false });
            let mut moved = None;
            while started.elapsed() < Duration::from_secs(3) {
                if list_x(bar) > 5 && moved.is_none() {
                    moved = Some(started.elapsed());
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let uia: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).unwrap();
            let span = icons(&uia, task_list(bar).unwrap()).unwrap();
            let b = rect(bar).unwrap();
            println!("barra: empezo a moverse a los {:?}; iconos {:?}; centro de iconos {} vs centro de barra {}; acento={:?}", moved, span, (span.0 + span.1) / 2, (b.left + b.right) / 2, accent_now(bar));
            configure(Look { style: Style::Acrylic, opacity: 60, center: true, solid_when_max: false });
            std::thread::sleep(Duration::from_millis(600));
            println!("barra: acrilico acento={:?}", accent_now(bar));
            configure(Look { style: Style::Clear, opacity: 0, center: false, solid_when_max: false });
            std::thread::sleep(Duration::from_millis(900));
            println!("barra: sin centrar x={}", list_x(bar));
            restore();
            std::thread::sleep(Duration::from_millis(500));
            println!("barra: al final x={} acento={:?}", list_x(bar), accent_now(bar));
        }
    }

    #[test]
    #[ignore]
    fn barra_de_verdad() {
        let marker = std::env::temp_dir().join("cocuyo-barra.txt");
        *LOOK.lock().unwrap() = Some(Look { center: true, ..Look::default() });
        restore();
        std::thread::sleep(Duration::from_millis(800));
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let uia: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).unwrap();
            for bar in bars() {
                let list = task_list(bar);
                println!("barra {:?} lista {:?} iconos {:?}", rect(bar).map(|r| (r.left, r.right)), list.and_then(rect).map(|r| (r.left, r.right)), list.and_then(|l| icons(&uia, l)));
            }
        }
        std::fs::write(&marker, "izquierda").unwrap();
        std::thread::sleep(Duration::from_secs(2));
        let look = Look { style: Style::Clear, opacity: 0, center: true, solid_when_max: false };
        configure(look);
        std::thread::sleep(Duration::from_secs(2));
        std::fs::write(&marker, "aplicado").unwrap();
        std::thread::sleep(Duration::from_secs(4));
        configure(Look { opacity: 70, style: Style::Acrylic, ..look });
        std::thread::sleep(Duration::from_secs(1));
        std::fs::write(&marker, "acrilico").unwrap();
        std::thread::sleep(Duration::from_secs(4));
        restore();
        std::thread::sleep(Duration::from_secs(1));
        std::fs::write(&marker, "restaurado").unwrap();
        std::thread::sleep(Duration::from_secs(3));
        let _ = std::fs::remove_file(marker);
    }
}
