// mandos. los de xbox por xinput; los de play leyendo el hid a mano porque windows no los quiere

use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU8, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use tauri::AppHandle;
use windows::core::HSTRING;
use windows::Win32::Devices::HumanInterfaceDevice::{HidD_GetAttributes, HidD_GetInputReport, HIDD_ATTRIBUTES};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Storage::FileSystem::{CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING};
use windows::Win32::UI::Input::XboxController::{XInputGetState, XInputSetState, XINPUT_STATE, XINPUT_VIBRATION};
use windows::Win32::UI::Input::{GetRawInputDeviceInfoW, GetRawInputDeviceList, RAWINPUTDEVICELIST, RIDI_DEVICENAME, RIM_TYPEHID};

pub const A: u16 = 1;
pub const B: u16 = 1 << 1;
pub const X: u16 = 1 << 2;
pub const Y: u16 = 1 << 3;
pub const LB: u16 = 1 << 4;
pub const RB: u16 = 1 << 5;
pub const SELECT: u16 = 1 << 6;
pub const START: u16 = 1 << 7;
pub const L3: u16 = 1 << 8;
pub const R3: u16 = 1 << 9;
pub const HOME: u16 = 1 << 10;
pub const LT: u16 = 1 << 11;
pub const RT: u16 = 1 << 12;

const NAMES: [(u16, &str); 13] = [
    (A, "A"), (B, "B"), (X, "X"), (Y, "Y"), (LB, "LB"), (RB, "RB"), (LT, "LT"), (RT, "RT"),
    (SELECT, "Select"), (START, "Start"), (L3, "L3"), (R3, "R3"), (HOME, "PS"),
];

pub fn label(buttons: u16) -> String {
    NAMES.iter().filter(|(bit, _)| buttons & bit != 0).map(|(_, name)| *name).collect::<Vec<_>>().join(" + ")
}

const COMBOS: [u16; 3] = [L3 | R3, SELECT | START, HOME];
const HOLD: Duration = Duration::from_millis(280);
const LOOK_EVERY: Duration = Duration::from_millis(60);
const SEARCH_EVERY: Duration = Duration::from_secs(2);
const SONY: u16 = 0x054C;

static ENABLED: AtomicBool = AtomicBool::new(false);
static COMBO: AtomicU8 = AtomicU8::new(0);
static CUSTOM: AtomicU16 = AtomicU16::new(0);
static RECORDING: AtomicBool = AtomicBool::new(false);
static RUNNING: AtomicBool = AtomicBool::new(false);
static WAKE: (Mutex<()>, Condvar) = (Mutex::new(()), Condvar::new());
static XBOX_SLOT: AtomicU8 = AtomicU8::new(u8::MAX);

pub fn configure(app: &AppHandle, on: bool, combo: u8, custom: u16) {
    COMBO.store(combo.min(COMBOS.len() as u8 - 1), Ordering::Relaxed);
    CUSTOM.store(custom, Ordering::Relaxed);
    ENABLED.store(on, Ordering::Relaxed);
    if on && !RUNNING.swap(true, Ordering::Relaxed) {
        let app = app.clone();
        let _ = std::thread::Builder::new().name("mando".into()).spawn(move || watch(app));
    }
    WAKE.1.notify_all();
}

fn from_xinput(buttons: u16) -> u16 {
    [(0x1000, A), (0x2000, B), (0x4000, X), (0x8000, Y), (0x0100, LB), (0x0200, RB), (0x0020, SELECT), (0x0010, START), (0x0040, L3), (0x0080, R3)]
        .iter()
        .filter(|(theirs, _)| buttons & theirs != 0)
        .fold(0, |all, (_, ours)| all | ours)
}

fn from_sony(report: &[u8], at: usize) -> u16 {
    let (Some(shapes), Some(others), Some(extra)) = (report.get(at), report.get(at + 1), report.get(at + 2)) else { return 0 };
    let mut all = 0;
    for (bit, ours) in [(0x10, X), (0x20, A), (0x40, B), (0x80, Y)] {
        if shapes & bit != 0 {
            all |= ours;
        }
    }
    for (bit, ours) in [(0x01, LB), (0x02, RB), (0x04, LT), (0x08, RT), (0x10, SELECT), (0x20, START), (0x40, L3), (0x80, R3)] {
        if others & bit != 0 {
            all |= ours;
        }
    }
    if extra & 0x01 != 0 {
        all |= HOME;
    }
    all
}

struct Sony {
    handle: HANDLE,
    wired_dualsense: bool,
}

impl Drop for Sony {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}

impl Sony {
    fn find() -> Option<Sony> {
        unsafe {
            let size = std::mem::size_of::<RAWINPUTDEVICELIST>() as u32;
            let mut count = 0;
            GetRawInputDeviceList(None, &mut count, size);
            let mut devices = vec![RAWINPUTDEVICELIST::default(); count as usize];
            let found = GetRawInputDeviceList(Some(devices.as_mut_ptr()), &mut count, size);
            if found == u32::MAX {
                return None;
            }
            for device in devices.iter().take(found as usize).filter(|device| device.dwType == RIM_TYPEHID) {
                let mut name = [0u16; 512];
                let mut len = name.len() as u32;
                let got = GetRawInputDeviceInfoW(Some(device.hDevice), RIDI_DEVICENAME, Some(name.as_mut_ptr().cast()), &mut len);
                if got == u32::MAX || got == 0 {
                    continue;
                }
                let path = String::from_utf16_lossy(&name[..got as usize]).trim_end_matches('\0').to_lowercase();
                if !path.contains("054c") {
                    continue;
                }
                let Ok(handle) = CreateFileW(&HSTRING::from(path.as_str()), 0xC000_0000, FILE_SHARE_READ | FILE_SHARE_WRITE, None, OPEN_EXISTING, FILE_FLAGS_AND_ATTRIBUTES(0), None) else { continue };
                let mut about = HIDD_ATTRIBUTES { Size: std::mem::size_of::<HIDD_ATTRIBUTES>() as u32, ..Default::default() };
                let known = HidD_GetAttributes(handle, &mut about) && about.VendorID == SONY;
                let dualsense = matches!(about.ProductID, 0x0CE6 | 0x0DF2);
                if !known || !(dualsense || matches!(about.ProductID, 0x05C4 | 0x09CC | 0x0BA0)) {
                    let _ = CloseHandle(handle);
                    continue;
                }
                return Some(Sony { handle, wired_dualsense: dualsense && path.contains("vid_054c") });
            }
            None
        }
    }

    fn buttons(&self) -> Option<u16> {
        let mut report = [0u8; 96];
        report[0] = 0x01;
        unsafe { HidD_GetInputReport(self.handle, report.as_mut_ptr().cast(), report.len() as u32) }.then(|| from_sony(&report, if self.wired_dualsense { 8 } else { 5 }))
    }
}

fn xbox() -> Option<(u16, u8)> {
    let mut seen = None;
    for slot in 0..4 {
        let mut state = XINPUT_STATE::default();
        if unsafe { XInputGetState(slot, &mut state) } == 0 {
            let mut buttons = from_xinput(state.Gamepad.wButtons.0);
            if state.Gamepad.bLeftTrigger > 120 {
                buttons |= LT;
            }
            if state.Gamepad.bRightTrigger > 120 {
                buttons |= RT;
            }
            if buttons != 0 || seen.is_none() {
                seen = Some((buttons, slot as u8));
            }
        }
    }
    seen
}

fn buzz(slot: u8, times: u8) {
    if slot > 3 {
        return;
    }
    std::thread::spawn(move || unsafe {
        for _ in 0..times {
            let _ = XInputSetState(u32::from(slot), &XINPUT_VIBRATION { wLeftMotorSpeed: 0, wRightMotorSpeed: 22_000 });
            std::thread::sleep(Duration::from_millis(90));
            let _ = XInputSetState(u32::from(slot), &XINPUT_VIBRATION::default());
            std::thread::sleep(Duration::from_millis(90));
        }
    });
}

#[derive(Default)]
struct Hold {
    since: Option<Instant>,
    talking: bool,
}

impl Hold {
    fn step(&mut self, held: bool, now: Instant) -> Option<bool> {
        match (held, self.talking) {
            (true, false) => {
                let since = *self.since.get_or_insert(now);
                (now.duration_since(since) >= HOLD).then(|| {
                    self.talking = true;
                    true
                })
            }
            (false, true) => {
                *self = Hold::default();
                Some(false)
            }
            (false, false) => {
                self.since = None;
                None
            }
            (true, true) => None,
        }
    }
}

fn watch(app: AppHandle) {
    let mut sony: Option<Sony> = None;
    let mut hold = Hold::default();
    loop {
        if !ENABLED.load(Ordering::Relaxed) {
            sony = None;
            if hold.step(false, Instant::now()) == Some(false) {
                crate::hotkeys::from_pad(&app, false);
            }
            let guard = WAKE.0.lock().unwrap();
            let _unused = WAKE.1.wait(guard).unwrap();
            continue;
        }
        let xbox = xbox();
        if sony.is_none() && xbox.is_none() {
            sony = Sony::find();
        }
        let playstation = sony.as_ref().map(Sony::buttons);
        if playstation == Some(None) {
            sony = None;
        }
        let pressed = xbox.map_or(0, |(buttons, _)| buttons) | playstation.flatten().unwrap_or(0);
        let combo = match CUSTOM.load(Ordering::Relaxed) {
            0 => COMBOS[COMBO.load(Ordering::Relaxed) as usize % COMBOS.len()],
            custom => custom,
        };
        let held = pressed & combo == combo && !RECORDING.load(Ordering::Relaxed);
        match hold.step(held, Instant::now()) {
            Some(true) => {
                let slot = xbox.map_or(u8::MAX, |(_, slot)| slot);
                XBOX_SLOT.store(slot, Ordering::Relaxed);
                buzz(slot, 1);
                crate::hotkeys::from_pad(&app, true);
            }
            Some(false) => {
                buzz(XBOX_SLOT.swap(u8::MAX, Ordering::Relaxed), 2);
                crate::hotkeys::from_pad(&app, false);
            }
            None => {}
        }
        let connected = xbox.is_some() || sony.is_some();
        std::thread::sleep(if connected { LOOK_EVERY } else { SEARCH_EVERY });
    }
}

#[tauri::command]
pub async fn pad_record() -> Result<u16, String> {
    RECORDING.store(true, Ordering::Relaxed);
    let mut sony = Sony::find();
    let (mut best, mut seen_pad) = (0u16, false);
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        let xbox = xbox();
        let playstation = sony.as_ref().and_then(Sony::buttons);
        seen_pad |= xbox.is_some() || playstation.is_some();
        let pressed = xbox.map_or(0, |(buttons, _)| buttons) | playstation.unwrap_or(0);
        if pressed.count_ones() > best.count_ones() {
            best = pressed;
        }
        if pressed == 0 && best != 0 {
            break;
        }
        if sony.is_none() && xbox.is_none() {
            sony = Sony::find();
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    RECORDING.store(false, Ordering::Relaxed);
    match (best, seen_pad) {
        (0, false) => Err("No veo ningún mando conectado.".into()),
        (0, true) => Err("No pulsaste ningún botón del mando.".into()),
        (buttons, _) => Ok(buttons),
    }
}

#[tauri::command]
pub fn pad_label(buttons: u16) -> String {
    label(buttons)
}

#[tauri::command]
pub async fn pad_status() -> String {
    match (xbox().is_some(), Sony::find().is_some()) {
        (true, true) => "Veo un mando de Xbox y uno de PlayStation.".into(),
        (true, false) => "Veo un mando de Xbox (o uno que se presenta como tal).".into(),
        (false, true) => "Veo un mando de PlayStation.".into(),
        (false, false) => "No veo ningún mando conectado.".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pad_speaks_the_same_buttons() {
        assert_eq!(from_xinput(0x0040 | 0x0080), L3 | R3);
        assert_eq!(from_xinput(0x0020 | 0x0010 | 0x1000), SELECT | START | A);
        let mut report = [0u8; 16];
        report[5] = 0x08 | 0x20 | 0x40; // la cruceta suelta (8) no cuenta como botón
        report[6] = 0x40 | 0x80;
        report[7] = 0x01;
        assert_eq!(from_sony(&report, 5), A | B | L3 | R3 | HOME);
        let mut wired = [0u8; 16];
        wired[9] = 0x10 | 0x20;
        assert_eq!(from_sony(&wired, 8), SELECT | START);
        assert_eq!(from_sony(&[0u8; 4], 5), 0, "un informe corto no rompe nada");
        let mut triggers = [0u8; 16];
        triggers[6] = 0x04 | 0x08;
        assert_eq!(from_sony(&triggers, 5), LT | RT);
        assert_eq!(label(L3 | R3), "L3 + R3");
        assert_eq!(label(LB | RT | A), "A + LB + RT");
    }

    #[test]
    #[ignore]
    fn mando_de_verdad() {
        println!("xbox: {:?}", xbox());
        match Sony::find() {
            Some(pad) => {
                for _ in 0..3 {
                    println!("playstation (por cable con DualSense: {}): botones {:?}", pad.wired_dualsense, pad.buttons());
                    std::thread::sleep(Duration::from_millis(60));
                }
            }
            None => println!("playstation: no hay"),
        }
    }

    #[test]
    fn the_combo_must_be_held_and_ends_on_release() {
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        let mut hold = Hold::default();
        assert_eq!(hold.step(true, at(0)), None);
        assert_eq!(hold.step(true, at(120)), None);
        assert_eq!(hold.step(false, at(180)), None);
        assert_eq!(hold.step(true, at(1000)), None);
        assert_eq!(hold.step(true, at(1300)), Some(true));
        assert_eq!(hold.step(true, at(2000)), None);
        assert_eq!(hold.step(false, at(2060)), Some(false));
        assert_eq!(hold.step(false, at(2120)), None);
    }
}
