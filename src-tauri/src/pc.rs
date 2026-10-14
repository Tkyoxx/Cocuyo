// las manos de cocuyo: mira una ventana, hace clic y escribe como lo haria uno. si tocas el mouse, suelta todo

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::Value;
use tauri::AppHandle;
use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
use windows::Win32::UI::Accessibility::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows_core::BOOL;

const MOST: usize = 140;
const MOST_TEXTS: usize = 50;
const LONGEST_NAME: usize = 70;
const MOVED_BY_HAND: i32 = 30;

#[derive(Clone)]
struct Thing {
    kind: &'static str,
    name: String,
    value: String,
    rect: RECT,
    password: bool,
    focused: bool,
    enabled: bool,
}

struct Seen {
    window: isize,
    frame: RECT,
    things: Vec<Thing>,
}

static SEEN: Mutex<Option<Seen>> = Mutex::new(None);
static SHOWN: Mutex<Vec<Thing>> = Mutex::new(Vec::new());
static HAND: Mutex<Option<(i32, i32)>> = Mutex::new(None);
static LET_GO: AtomicBool = AtomicBool::new(false);
static STALE: AtomicBool = AtomicBool::new(false);

pub fn begin() {
    *HAND.lock().unwrap() = None;
    *SEEN.lock().unwrap() = None;
    SHOWN.lock().unwrap().clear();
    LET_GO.store(false, Ordering::Relaxed);
    STALE.store(false, Ordering::Relaxed);
}

pub(crate) fn boxed_in() -> Option<String> {
    if !cfg!(debug_assertions) {
        return None;
    }
    std::env::var("COCUYO_DEMO_SOLO").ok().filter(|only| !only.trim().is_empty())
}

unsafe fn allowed(hwnd: HWND) -> bool {
    boxed_in().is_none_or(|only| simple(&title_of(hwnd)).contains(&simple(&only)))
}

fn kind_of(id: i32) -> Option<&'static str> {
    Some(match id {
        50000 | 50031 => "botón",
        50002 => "casilla",
        50003 => "desplegable",
        50004 => "campo",
        50005 => "enlace",
        50007 | 50029 => "elemento",
        50011 => "menú",
        50013 => "opción",
        50015 => "deslizador",
        50019 => "pestaña",
        50024 => "rama",
        50035 => "columna",
        50020 => "texto",
        50006 => "imagen",
        _ => return None,
    })
}

fn quiet(kind: &str) -> bool {
    matches!(kind, "texto" | "imagen")
}

fn plain(text: &str) -> String {
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut cut: String = flat.chars().take(LONGEST_NAME).collect();
    if flat.chars().count() > LONGEST_NAME {
        cut.push('…');
    }
    cut
}

fn overlaps(a: &RECT, b: &RECT) -> bool {
    a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom
}

fn inside(small: &RECT, big: &RECT) -> bool {
    small.left >= big.left && small.right <= big.right && small.top >= big.top && small.bottom <= big.bottom
}

unsafe fn frame_of(hwnd: HWND) -> RECT {
    let mut rect = RECT::default();
    if DwmGetWindowAttribute(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS, (&mut rect as *mut RECT).cast(), std::mem::size_of::<RECT>() as u32).is_err() {
        let _ = GetWindowRect(hwnd, &mut rect);
    }
    rect
}

unsafe fn title_of(hwnd: HWND) -> String {
    let mut buffer = [0u16; 256];
    let len = GetWindowTextW(hwnd, &mut buffer) as usize;
    String::from_utf16_lossy(&buffer[..len])
}

unsafe fn exe_of(hwnd: HWND) -> String {
    let mut pid = 0;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return String::new() };
    let mut buffer = [0u16; 512];
    let mut len = buffer.len() as u32;
    let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, windows::core::PWSTR(buffer.as_mut_ptr()), &mut len).is_ok();
    let _ = windows::Win32::Foundation::CloseHandle(process);
    if !ok {
        return String::new();
    }
    let path = String::from_utf16_lossy(&buffer[..len as usize]);
    path.rsplit('\\').next().unwrap_or("").to_string()
}

unsafe fn app_windows() -> Vec<HWND> {
    unsafe extern "system" fn each(hwnd: HWND, found: LPARAM) -> BOOL {
        let found = &mut *(found.0 as *mut Vec<HWND>);
        let mut rect = RECT::default();
        let _ = GetWindowRect(hwnd, &mut rect);
        let real = GetWindow(hwnd, GW_OWNER).map_or(true, |owner| owner.is_invalid()) && rect.right - rect.left > 120 && rect.bottom - rect.top > 80;
        if real && GetWindowTextLengthW(hwnd) > 0 && crate::placement::is_app_window(hwnd, std::process::id()) && allowed(hwnd) {
            found.push(hwnd);
        }
        true.into()
    }
    let mut found: Vec<HWND> = Vec::new();
    let _ = EnumWindows(Some(each), LPARAM(&mut found as *mut Vec<HWND> as isize));
    found
}

unsafe fn target() -> Option<HWND> {
    let foreground = GetForegroundWindow();
    if !foreground.is_invalid() && crate::placement::is_app_window(foreground, std::process::id()) && allowed(foreground) {
        return Some(foreground);
    }
    if let Some(seen) = SEEN.lock().unwrap().as_ref() {
        let hwnd = HWND(seen.window as *mut _);
        if IsWindow(Some(hwnd)).as_bool() && IsWindowVisible(hwnd).as_bool() && allowed(hwnd) {
            return Some(hwnd);
        }
    }
    app_windows().into_iter().next()
}

unsafe fn things_in(hwnd: HWND, frame: &RECT) -> Vec<Thing> {
    let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    let Ok(uia) = CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER) else { return Vec::new() };
    let Ok(root) = uia.ElementFromHandle(hwnd) else { return Vec::new() };
    let Ok(cache) = uia.CreateCacheRequest() else { return Vec::new() };
    for property in [UIA_NamePropertyId, UIA_ControlTypePropertyId, UIA_BoundingRectanglePropertyId, UIA_IsOffscreenPropertyId, UIA_IsPasswordPropertyId, UIA_IsEnabledPropertyId, UIA_HasKeyboardFocusPropertyId, UIA_ValueValuePropertyId] {
        let _ = cache.AddProperty(property);
    }
    let Ok(condition) = uia.ControlViewCondition() else { return Vec::new() };
    let Ok(all) = root.FindAllBuildCache(TreeScope_Descendants, &condition, &cache) else { return Vec::new() };
    let mut things = Vec::new();
    for at in 0..all.Length().unwrap_or(0) {
        let Ok(element) = all.GetElement(at) else { continue };
        let Some(kind) = element.CachedControlType().ok().and_then(|id| kind_of(id.0)) else { continue };
        if element.CachedIsOffscreen().is_ok_and(|off| off.as_bool()) {
            continue;
        }
        let Ok(rect) = element.CachedBoundingRectangle() else { continue };
        if rect.right - rect.left < 3 || rect.bottom - rect.top < 3 || !overlaps(&rect, frame) {
            continue;
        }
        let name = plain(&element.CachedName().map(|name| name.to_string()).unwrap_or_default());
        if name.is_empty() && kind != "campo" {
            continue;
        }
        let password = element.CachedIsPassword().is_ok_and(|on| on.as_bool());
        let value = if kind == "campo" && !password {
            element.GetCachedPropertyValue(UIA_ValueValuePropertyId).ok().and_then(|value| windows_core::BSTR::try_from(&value).ok()).map(|value| plain(&value.to_string())).unwrap_or_default()
        } else {
            String::new()
        };
        things.push(Thing {
            kind,
            name,
            value,
            rect,
            password,
            focused: element.CachedHasKeyboardFocus().is_ok_and(|on| on.as_bool()),
            enabled: element.CachedIsEnabled().map_or(true, |on| on.as_bool()),
        });
    }
    things
}

fn trimmed(things: Vec<Thing>) -> Vec<Thing> {
    let doers: Vec<(String, RECT)> = things.iter().filter(|thing| !quiet(thing.kind)).map(|thing| (thing.name.clone(), thing.rect)).collect();
    let pressable: Vec<(String, RECT)> = things.iter().filter(|thing| matches!(thing.kind, "botón" | "enlace")).map(|thing| (thing.name.clone(), thing.rect)).collect();
    let things: Vec<Thing> = things
        .into_iter()
        .filter(|thing| thing.kind != "elemento" || !pressable.iter().any(|(name, rect)| *name == thing.name && inside(rect, &thing.rect)))
        .collect();
    let mut kept: Vec<(usize, Thing)> = things
        .into_iter()
        .enumerate()
        .filter(|(_, thing)| !quiet(thing.kind) || !doers.iter().any(|(name, rect)| inside(&thing.rect, rect) && (name.is_empty() || name.contains(thing.name.as_str()))))
        .collect();
    let texts: Vec<usize> = kept.iter().filter(|(_, thing)| quiet(thing.kind)).map(|(at, _)| *at).collect();
    if texts.len() > MOST_TEXTS {
        let dropped: Vec<usize> = texts[..texts.len() - MOST_TEXTS].to_vec();
        kept.retain(|(at, _)| !dropped.contains(at));
    }
    if kept.len() > MOST {
        let doers = kept.iter().filter(|(_, thing)| !quiet(thing.kind)).count();
        let text_room = (MOST / 4).max(MOST.saturating_sub(doers));
        let fresh: Vec<usize> = kept.iter().filter(|(_, thing)| quiet(thing.kind)).map(|(at, _)| *at).rev().take(text_room).collect();
        let mut doers_left = MOST - text_room;
        kept.retain(|(at, thing)| {
            if quiet(thing.kind) {
                return fresh.contains(at);
            }
            let keep = doers_left > 0;
            doers_left = doers_left.saturating_sub(1);
            keep
        });
    }
    kept.into_iter().map(|(_, thing)| thing).collect()
}

unsafe fn things_by_ocr(frame: &RECT) -> Vec<Thing> {
    let (w, h) = (frame.right - frame.left, frame.bottom - frame.top);
    let Some((bgra, w, h)) = crate::capture::pixels(frame.left, frame.top, w, h) else { return Vec::new() };
    crate::ocr::lines(&bgra, w, h)
        .unwrap_or_default()
        .into_iter()
        .map(|(text, x, y, width, height)| Thing {
            kind: "texto",
            name: plain(&text),
            value: String::new(),
            rect: RECT { left: frame.left + x as i32, top: frame.top + y as i32, right: frame.left + (x + width) as i32, bottom: frame.top + (y + height) as i32 },
            password: false,
            focused: false,
            enabled: true,
        })
        .take(MOST)
        .collect()
}

fn spot(rect: &RECT, frame: &RECT) -> (i32, i32) {
    let (w, h) = ((frame.right - frame.left).max(1), (frame.bottom - frame.top).max(1));
    let (x, y) = ((rect.left + rect.right) / 2 - frame.left, (rect.top + rect.bottom) / 2 - frame.top);
    ((x * 1000 / w).clamp(0, 1000), (y * 1000 / h).clamp(0, 1000))
}

fn describe(title: &str, exe: &str, frame: &RECT, things: &[Thing], by_ocr: bool) -> String {
    let mut text = format!("Ventana: «{}» ({exe}), {}×{} px.\n", plain(title), frame.right - frame.left, frame.bottom - frame.top);
    if things.is_empty() {
        text.push_str("No pude leer sus elementos. Pide la imagen (cocuyo_mirar con imagen) para ver dónde hacer clic.");
        return text;
    }
    text.push_str(if by_ocr {
        "La app no deja leer sus botones; esto es el texto que se ve, con su posición (x,y de 0 a 1000 dentro de la ventana):\n"
    } else {
        "Elementos (número, tipo, nombre, posición x,y de 0 a 1000 dentro de la ventana):\n"
    });
    for (at, thing) in things.iter().enumerate() {
        let (x, y) = spot(&thing.rect, frame);
        let mut line = format!("[{}] {} «{}» ({x},{y})", at + 1, thing.kind, thing.name);
        if !thing.value.is_empty() && thing.value != thing.name {
            line.push_str(&format!(" dice: «{}»", thing.value));
        }
        if thing.focused {
            line.push_str(" ← tiene el foco");
        }
        if thing.password {
            line.push_str(" (contraseña)");
        }
        if !thing.enabled {
            line.push_str(" (desactivado)");
        }
        text.push_str(&line);
        text.push('\n');
    }
    text
}

unsafe fn view() -> Result<(HWND, RECT, Vec<Thing>, bool), String> {
    let hwnd = target().ok_or("No hay ninguna ventana abierta para mirar.")?;
    let frame = frame_of(hwnd);
    let mut things = trimmed(things_in(hwnd, &frame));
    if things.len() < 6 {
        std::thread::sleep(Duration::from_millis(450));
        things = trimmed(things_in(hwnd, &frame));
    }
    let by_ocr = things.iter().filter(|thing| !quiet(thing.kind)).count() < 3;
    if by_ocr {
        let read = things_by_ocr(&frame);
        if !read.is_empty() {
            things = read;
        }
    }
    *SEEN.lock().unwrap() = Some(Seen { window: hwnd.0 as isize, frame, things: things.clone() });
    STALE.store(false, Ordering::Relaxed);
    Ok((hwnd, frame, things, by_ocr))
}

pub fn look(with_image: bool) -> Result<(String, Option<Vec<u8>>), String> {
    unsafe {
        let (hwnd, frame, things, by_ocr) = view()?;
        let text = describe(&title_of(hwnd), &exe_of(hwnd), &frame, &things, by_ocr);
        let image = if with_image || things.is_empty() {
            crate::capture::png_of(frame.left, frame.top, frame.right - frame.left, frame.bottom - frame.top)
        } else {
            None
        };
        *SHOWN.lock().unwrap() = things;
        Ok((text, image))
    }
}

pub fn refresh() -> Result<(), String> {
    unsafe { view().map(|_| ()) }
}

fn heading(seen: &Seen) -> Vec<String> {
    let (w, h) = (seen.frame.right - seen.frame.left, seen.frame.bottom - seen.frame.top);
    seen.things
        .iter()
        .filter(|thing| {
            let (x, y) = ((thing.rect.left + thing.rect.right) / 2 - seen.frame.left, (thing.rect.top + thing.rect.bottom) / 2 - seen.frame.top);
            thing.focused || (y < h * 18 / 100 && x > w * 28 / 100 && !thing.name.is_empty() && thing.kind != "campo")
        })
        .map(|thing| if thing.focused && thing.kind == "campo" { thing.name.clone() } else { format!("{} «{}»", thing.kind, thing.name) })
        .filter(|name| !matches!(name.as_str(), "botón «Minimizar»" | "botón «Maximizar»" | "botón «Cerrar»" | "botón «Minimize»" | "botón «Maximize»" | "botón «Close»"))
        .take(4)
        .collect()
}

pub fn where_now() -> String {
    let seen = SEEN.lock().unwrap();
    let Some(seen) = seen.as_ref() else { return String::new() };
    let title = unsafe { title_of(HWND(seen.window as *mut _)) };
    let mut text = format!("Ventana «{}»", plain(&title));
    let top = heading(seen);
    if !top.is_empty() {
        text.push_str(&format!(" · {}", top.join(" · ")));
    }
    text
}

pub fn shows(who: &str) -> bool {
    let wanted = crate::intent::normalize(who);
    if wanted.is_empty() {
        return true;
    }
    let seen = SEEN.lock().unwrap();
    let Some(seen) = seen.as_ref() else { return false };
    let title = unsafe { title_of(HWND(seen.window as *mut _)) };
    let mut places = heading(seen);
    places.push(title);
    places.iter().any(|place| crate::intent::normalize(place).contains(&wanted))
}

unsafe fn bring(hwnd: HWND) -> bool {
    if IsIconic(hwnd).as_bool() {
        let _ = ShowWindow(hwnd, SW_RESTORE);
    }
    if GetForegroundWindow() == hwnd {
        return true;
    }
    let alt = |up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VK_MENU, wScan: 0, dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) }, time: 0, dwExtraInfo: 0 } },
    };
    SendInput(&[alt(false), alt(true)], std::mem::size_of::<INPUT>() as i32);
    let _ = SetForegroundWindow(hwnd);
    let deadline = Instant::now() + Duration::from_millis(800);
    while Instant::now() < deadline {
        if GetForegroundWindow() == hwnd {
            std::thread::sleep(Duration::from_millis(120));
            return true;
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    false
}

fn simple(text: &str) -> String {
    crate::intent::normalize(text).replace(".exe", "").replace(' ', "")
}

pub fn open_windows() -> Vec<String> {
    unsafe { app_windows().into_iter().take(15).map(|hwnd| format!("«{}» ({})", plain(&title_of(hwnd)), exe_of(hwnd))).collect() }
}

unsafe fn window_called(name: &str) -> Option<HWND> {
    let wanted = simple(name);
    if wanted.is_empty() {
        return None;
    }
    let windows = app_windows();
    windows
        .iter()
        .copied()
        .find(|&hwnd| simple(&exe_of(hwnd)) == wanted)
        .or_else(|| windows.iter().copied().find(|&hwnd| simple(&title_of(hwnd)).contains(&wanted) || simple(&exe_of(hwnd)).contains(&wanted)))
}

pub fn open(app: &AppHandle, name: &str) -> Result<String, String> {
    unsafe {
        if let Some(hwnd) = window_called(name) {
            if !bring(hwnd) {
                return Err(format!("«{}» está abierta, pero Windows no me dejó traerla al frente.", title_of(hwnd)));
            }
            *SEEN.lock().unwrap() = None;
            return look(false).map(|(text, _)| text);
        }
        if boxed_in().is_some() {
            return Err("(prueba) solo puedo usar la ventana de prueba".into());
        }
        let found = crate::apps::find(app, name, 0.75).ok_or_else(|| format!("No encontré ninguna app llamada «{name}» abierta ni instalada."))?;
        let before = GetForegroundWindow();
        crate::apps::launch(&found)?;
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(400));
            if let Some(hwnd) = window_called(name).or_else(|| window_called(&found.name)) {
                bring(hwnd);
                std::thread::sleep(Duration::from_millis(900));
                return look(false).map(|(text, _)| text);
            }
            let now = GetForegroundWindow();
            if now != before && !now.is_invalid() && crate::placement::is_app_window(now, std::process::id()) && Instant::now() + Duration::from_secs(9) > deadline {
                return look(false).map(|(text, _)| text);
            }
        }
        Err(format!("Abrí {}, pero su ventana no apareció.", found.name))
    }
}

fn moved_by_hand() -> bool {
    if LET_GO.load(Ordering::Relaxed) {
        return true;
    }
    let Some((x, y)) = *HAND.lock().unwrap() else { return false };
    let mut now = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut now);
    }
    let moved = (now.x - x).abs() > MOVED_BY_HAND || (now.y - y).abs() > MOVED_BY_HAND;
    if moved {
        LET_GO.store(true, Ordering::Relaxed);
    }
    moved
}

fn mouse(flags: MOUSE_EVENT_FLAGS, data: i32) -> INPUT {
    INPUT { r#type: INPUT_MOUSE, Anonymous: INPUT_0 { mi: MOUSEINPUT { dx: 0, dy: 0, mouseData: data as u32, dwFlags: flags, time: 0, dwExtraInfo: 0 } } }
}

unsafe fn point_at(x: i32, y: i32) {
    let _ = SetCursorPos(x, y);
    *HAND.lock().unwrap() = Some((x, y));
    std::thread::sleep(Duration::from_millis(60));
}

unsafe fn click(x: i32, y: i32, right: bool, double: bool) {
    point_at(x, y);
    let (down, up) = if right { (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP) } else { (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP) };
    let once = [mouse(down, 0), mouse(up, 0)];
    SendInput(&once, std::mem::size_of::<INPUT>() as i32);
    if double {
        std::thread::sleep(Duration::from_millis(70));
        SendInput(&once, std::mem::size_of::<INPUT>() as i32);
    }
}

fn virtual_key(name: &str) -> Option<(u16, bool)> {
    let name = crate::intent::normalize(name);
    let extended = |vk: u16| Some((vk, true));
    match name.as_str() {
        "enter" | "intro" | "return" => Some((0x0D, false)),
        "tab" | "tabulador" => Some((0x09, false)),
        "esc" | "escape" | "escapar" => Some((0x1B, false)),
        "espacio" | "space" => Some((0x20, false)),
        "borrar" | "backspace" | "retroceso" => Some((0x08, false)),
        "suprimir" | "supr" | "delete" | "del" => extended(0x2E),
        "arriba" | "up" | "flecha arriba" => extended(0x26),
        "abajo" | "down" | "flecha abajo" => extended(0x28),
        "izquierda" | "left" | "flecha izquierda" => extended(0x25),
        "derecha" | "right" | "flecha derecha" => extended(0x27),
        "inicio" | "home" => extended(0x24),
        "fin" | "end" => extended(0x23),
        "repag" | "pageup" | "re pag" => extended(0x21),
        "avpag" | "pagedown" | "av pag" => extended(0x22),
        "ctrl" | "control" => Some((0x11, false)),
        "shift" | "mayus" | "mayusculas" => Some((0x10, false)),
        "alt" => Some((0x12, false)),
        "win" | "windows" => extended(0x5B),
        _ if name.len() > 1 && name.starts_with('f') && name[1..].parse::<u16>().is_ok_and(|n| (1..=12).contains(&n)) => Some((0x6F + name[1..].parse::<u16>().unwrap(), false)),
        _ if name.chars().count() == 1 => {
            let c = name.chars().next()?.to_ascii_uppercase();
            match c {
                'A'..='Z' | '0'..='9' => Some((c as u16, false)),
                '/' => Some((0xBF, false)),
                ',' => Some((0xBC, false)),
                '.' => Some((0xBE, false)),
                '-' => Some((0xBD, false)),
                _ => None,
            }
        }
        _ => None,
    }
}

fn press(combo: &str) -> Result<(), String> {
    let keys: Vec<(u16, bool)> = combo.split('+').map(str::trim).filter(|k| !k.is_empty()).map(|k| virtual_key(k).ok_or_else(|| format!("no conozco la tecla «{k}»"))).collect::<Result<_, _>>()?;
    if keys.is_empty() {
        return Err("no dijiste qué tecla".into());
    }
    let has = |vk: u16| keys.iter().any(|(key, _)| *key == vk);
    if (has(0x11) && has(0x12) && has(0x2E)) || (has(0x5B) && has(0x4C)) {
        return Err("esa combinación no la pulso".into());
    }
    let key = |(vk, extended): (u16, bool), up: bool| {
        let mut flags = if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) };
        if extended {
            flags |= KEYEVENTF_EXTENDEDKEY;
        }
        INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VIRTUAL_KEY(vk), wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 } } }
    };
    let mut inputs: Vec<INPUT> = keys.iter().map(|k| key(*k, false)).collect();
    inputs.extend(keys.iter().rev().map(|k| key(*k, true)));
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
    Ok(())
}

fn number(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)).or_else(|| v.as_str().and_then(|s| s.trim().trim_start_matches('[').trim_end_matches(']').parse().ok())))
}

fn center(rect: &RECT) -> (i32, i32) {
    ((rect.left + rect.right) / 2, (rect.top + rect.bottom) / 2)
}

fn same_as(wanted: &Thing, things: &[Thing]) -> Option<Thing> {
    let (x, y) = center(&wanted.rect);
    things
        .iter()
        .filter(|thing| thing.kind == wanted.kind && thing.name == wanted.name)
        .min_by_key(|thing| {
            let (tx, ty) = center(&thing.rect);
            (tx - x).abs() + (ty - y).abs()
        })
        .cloned()
}

fn where_to(action: &Value) -> Result<Option<(i32, i32, Thing)>, String> {
    if let Some(id) = number(action, "id") {
        let wanted = SHOWN.lock().unwrap().get((id as usize).wrapping_sub(1)).cloned().ok_or_else(|| format!("no hay ningún elemento [{id}]"))?;
        if STALE.load(Ordering::Relaxed) {
            refresh()?;
        }
        let seen = SEEN.lock().unwrap();
        let things = seen.as_ref().map(|seen| seen.things.as_slice()).unwrap_or(&[]);
        let thing = same_as(&wanted, things).ok_or_else(|| format!("la ventana cambió y ya no está el {} «{}» [{id}]: mírala otra vez antes de seguir", wanted.kind, wanted.name))?;
        let (x, y) = center(&thing.rect);
        return Ok(Some((x, y, thing)));
    }
    let seen = SEEN.lock().unwrap();
    let seen = seen.as_ref().ok_or("primero mira la ventana")?;
    if let (Some(x), Some(y)) = (number(action, "x"), number(action, "y")) {
        let (w, h) = (seen.frame.right - seen.frame.left, seen.frame.bottom - seen.frame.top);
        let (x, y) = (seen.frame.left + (x.clamp(0, 1000) as i32 * w / 1000), seen.frame.top + (y.clamp(0, 1000) as i32 * h / 1000));
        let thing = Thing { kind: "punto", name: String::new(), value: String::new(), rect: RECT { left: x, top: y, right: x + 1, bottom: y + 1 }, password: false, focused: false, enabled: true };
        return Ok(Some((x, y, thing)));
    }
    Ok(None)
}

fn seen_window() -> Option<HWND> {
    SEEN.lock().unwrap().as_ref().map(|seen| HWND(seen.window as *mut _))
}

fn one(action: &Value) -> Result<String, String> {
    let kind = crate::intent::normalize(action.get("accion").and_then(Value::as_str).unwrap_or(""));
    let spot = where_to(action)?;
    let name = |thing: &Thing| if thing.name.is_empty() { thing.kind.to_string() } else { format!("{} «{}»", thing.kind, thing.name) };
    unsafe {
        match kind.as_str() {
            "clic" | "click" | "doble clic" | "doble_clic" | "clic derecho" | "clic_derecho" => {
                let (x, y, thing) = spot.ok_or("falta el número del elemento (o x,y)")?;
                let double = kind.starts_with("doble");
                let right = kind.ends_with("derecho");
                click(x, y, right, double);
                STALE.store(true, Ordering::Relaxed);
                Ok(format!("{} en {}", if double { "doble clic" } else if right { "clic derecho" } else { "clic" }, name(&thing)))
            }
            "escribir" | "escribe" | "type" => {
                let text = action.get("texto").and_then(Value::as_str).unwrap_or("");
                if text.is_empty() {
                    return Err("falta el texto".into());
                }
                if let Some((x, y, thing)) = &spot {
                    if thing.password {
                        return Err("ese campo es de contraseña: eso lo escribe la persona".into());
                    }
                    click(*x, *y, false, false);
                    STALE.store(false, Ordering::Relaxed);
                    std::thread::sleep(Duration::from_millis(150));
                }
                if SEEN.lock().unwrap().as_ref().is_some_and(|seen| seen.things.iter().any(|thing| thing.focused && thing.password)) && spot.is_none() {
                    return Err("el foco está en una contraseña: eso lo escribe la persona".into());
                }
                crate::inject::paste(text)?;
                Ok(format!("escribí «{}»{}", plain(text), spot.map(|(_, _, thing)| format!(" en {}", name(&thing))).unwrap_or_default()))
            }
            "tecla" | "teclas" | "pulsar" | "key" => {
                let combo = action.get("teclas").or_else(|| action.get("texto")).and_then(Value::as_str).unwrap_or("");
                press(combo)?;
                STALE.store(true, Ordering::Relaxed);
                Ok(format!("pulsé {combo}"))
            }
            "esperar" | "espera" | "wait" => {
                let seconds = action.get("segundos").and_then(Value::as_f64).unwrap_or(1.0).clamp(0.2, 6.0);
                std::thread::sleep(Duration::from_secs_f64(seconds));
                Ok(format!("esperé {seconds} s"))
            }
            "rueda" | "scroll" | "desplazar" => {
                let down = !crate::intent::normalize(action.get("texto").and_then(Value::as_str).unwrap_or("abajo")).contains("arriba");
                let times = action.get("veces").and_then(Value::as_i64).unwrap_or(3).clamp(1, 15) as i32;
                if let Some((x, y, _)) = &spot {
                    point_at(*x, *y);
                } else if let Some(seen) = SEEN.lock().unwrap().as_ref() {
                    point_at((seen.frame.left + seen.frame.right) / 2, (seen.frame.top + seen.frame.bottom) / 2);
                }
                SendInput(&[mouse(MOUSEEVENTF_WHEEL, if down { -120 } else { 120 } * times)], std::mem::size_of::<INPUT>() as i32);
                STALE.store(true, Ordering::Relaxed);
                Ok(format!("rueda hacia {}", if down { "abajo" } else { "arriba" }))
            }
            other => Err(format!("no sé hacer «{other}»: usa clic, doble_clic, clic_derecho, escribir, tecla, esperar o rueda")),
        }
    }
}

pub fn act(actions: &[Value]) -> Result<String, String> {
    if actions.is_empty() {
        return Err("No me diste acciones.".into());
    }
    if let Some(hwnd) = seen_window() {
        unsafe {
            if IsWindow(Some(hwnd)).as_bool() && GetForegroundWindow() != hwnd && crate::placement::is_app_window(hwnd, std::process::id()) {
                bring(hwnd);
            }
        }
    }
    let mut log = Vec::new();
    for action in actions.iter().take(20) {
        if boxed_in().is_some() && unsafe { !allowed(GetForegroundWindow()) } {
            log.push("(prueba) la ventana de adelante no es la de prueba: me detuve".to_string());
            return Err(log.join("\n"));
        }
        if moved_by_hand() {
            log.push("La persona movió el mouse: le devolví el control y me detuve.".to_string());
            return Err(log.join("\n"));
        }
        match one(action) {
            Ok(done) => log.push(format!("✓ {done}")),
            Err(why) => {
                log.push(format!("✗ {why}"));
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(220));
    }
    Ok(log.join("\n"))
}

pub fn let_go() -> bool {
    LET_GO.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_understood_in_spanish_and_english() {
        assert_eq!(virtual_key("Enter"), Some((0x0D, false)));
        assert_eq!(virtual_key("flecha abajo"), Some((0x28, true)));
        assert_eq!(virtual_key("F5"), Some((0x74, false)));
        assert_eq!(virtual_key("k"), Some((0x4B, false)));
        assert_eq!(virtual_key("hiperespacio"), None);
        assert!(press("ctrl + alt + supr").is_err());
    }

    #[test]
    fn positions_follow_the_window_not_the_screen() {
        let frame = RECT { left: 1920, top: 100, right: 2920, bottom: 600 };
        let button = RECT { left: 2410, top: 340, right: 2430, bottom: 360 };
        assert_eq!(spot(&button, &frame), (500, 500));
    }

    #[test]
    fn texts_inside_buttons_are_not_repeated() {
        let thing = |kind, name: &str, rect| Thing { kind, name: name.into(), value: String::new(), rect, password: false, focused: false, enabled: true };
        let moved = vec![thing("campo", "Escribir un mensaje para Angela", RECT { left: 0, top: 500, right: 400, bottom: 540 }), thing("campo", "Buscar", RECT { left: 0, top: 0, right: 100, bottom: 20 })];
        assert!(same_as(&thing("campo", "Escribir un mensaje para +57 300", RECT { left: 0, top: 500, right: 400, bottom: 540 }), &moved).is_none());
        assert_eq!(same_as(&thing("campo", "Buscar", RECT { left: 5, top: 5, right: 90, bottom: 15 }), &moved).map(|t| t.rect.right), Some(100));
        let things = vec![
            thing("botón", "Enviar", RECT { left: 0, top: 0, right: 100, bottom: 40 }),
            thing("texto", "Enviar", RECT { left: 10, top: 10, right: 60, bottom: 30 }),
            thing("texto", "Hola Angela", RECT { left: 0, top: 50, right: 100, bottom: 70 }),
        ];
        let kept = trimmed(things);
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[1].name, "Hola Angela");
    }

    #[test]
    #[ignore]
    fn mira_una_ventana() {
        let name = std::env::var("COCUYO_VENTANA").unwrap();
        let hwnd = unsafe { window_called(&name) }.expect("no está");
        assert!(unsafe { bring(hwnd) });
        let started = Instant::now();
        let (text, _) = look(false).unwrap();
        println!("{text}
mirar tardó {:?}", started.elapsed());
    }

    #[test]
    #[ignore]
    fn usa_el_bloc_de_notas() {
        let mut notepad = std::process::Command::new("notepad.exe").spawn().unwrap();
        std::thread::sleep(Duration::from_millis(1500));
        let started = Instant::now();
        let hwnd = unsafe { window_called("notepad") }.expect("no apareció el bloc");
        assert!(unsafe { bring(hwnd) });
        let (text, _) = look(false).unwrap();
        println!("{text}
mirar tardó {:?}", started.elapsed());
        let field = text.lines().find(|line| line.contains(" campo ")).and_then(|line| line[1..].split(']').next()).and_then(|n| n.parse::<i64>().ok()).expect("sin campo");
        let done = act(&[serde_json::json!({ "accion": "escribir", "id": field, "texto": "hola desde las manos de cocuyo ñandú" }), serde_json::json!({ "accion": "tecla", "teclas": "ctrl+inicio" })]).unwrap();
        println!("{done}");
        std::thread::sleep(Duration::from_millis(400));
        let (after, png) = look(true).unwrap();
        println!("{after}
captura: {} bytes", png.map_or(0, |png| png.len()));
        let _ = notepad.kill();
        assert!(after.contains("ñandú"));
    }
}
