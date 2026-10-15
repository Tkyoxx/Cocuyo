// el hook del teclado. si se te queda pegada la tecla windows, en la bandeja hay un boton para soltarla, no me escribas

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, RegisterHotKey, SendInput, UnregisterHotKey, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
    KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN,
    VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx,
    HC_ACTION, KBDLLHOOKSTRUCT, LLKHF_INJECTED, MSG, WH_KEYBOARD_LL, WM_APP, WM_CLIPBOARDUPDATE,
    WM_HOTKEY, WM_KEYDOWN, WM_SYSKEYDOWN, WM_TIMER,
};

use crate::{capture, clip, mic, overlay, placement, voice};

const ARM_DELAY: Duration = Duration::from_millis(140);

pub const CTRL: u8 = 1;
pub const SHIFT: u8 = 2;
pub const ALT: u8 = 4;
pub const WIN: u8 = 8;

const VK_ESCAPE: u32 = 0x1B;
const VK_SNAPSHOT: u32 = 0x2C;
const VK_MASK: u16 = 0xE8;

fn modifier(vk: u32) -> Option<u8> {
    match vk {
        0xA2 | 0xA3 => Some(CTRL),
        0xA0 | 0xA1 => Some(SHIFT),
        0xA4 | 0xA5 => Some(ALT),
        0x5B | 0x5C => Some(WIN),
        _ => None,
    }
}

pub fn valid_chord(mask: u8) -> bool {
    mask < 16 && mask.count_ones() >= 2 && mask != CTRL | ALT
}

struct KeyEdge {
    vk: u32,
    down: bool,
    held: u8,
}

const MAX_HOLD: Duration = Duration::from_secs(90);

static TX: OnceLock<Sender<KeyEdge>> = OnceLock::new();
static ECHO: AtomicBool = AtomicBool::new(false);
static PAUSED: AtomicBool = AtomicBool::new(false);
static ACCEPT_INJECTED: AtomicBool = AtomicBool::new(false);
static DICTATION_KEYS: AtomicU8 = AtomicU8::new(CTRL | WIN);
static LASSO_KEYS: AtomicU8 = AtomicU8::new(CTRL | SHIFT);
static COMMAND_KEYS: AtomicU8 = AtomicU8::new(ALT | WIN);

#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, Serialize)]
pub struct Hotkey {
    pub mods: u8,
    pub vk: u32,
}

pub const DEFAULT_CAPTURES: [Hotkey; 5] = [
    Hotkey { mods: 0, vk: VK_SNAPSHOT },
    Hotkey { mods: ALT, vk: VK_SNAPSHOT },
    Hotkey { mods: CTRL, vk: VK_SNAPSHOT },
    Hotkey { mods: CTRL | ALT, vk: 0x52 },
    Hotkey { mods: CTRL | ALT, vk: 0x47 },
];

pub fn valid_hotkey(key: Hotkey) -> bool {
    let standalone = matches!(key.vk, 0x2C | 0x13 | 0x91 | 0x70..=0x87);
    key.vk != 0 && key.mods < 16 && modifier(key.vk).is_none() && (key.mods != 0 || standalone)
}

static CAPTURE_KEYS: Mutex<[Hotkey; 5]> = Mutex::new(DEFAULT_CAPTURES);
static CAPTURE_OK: [AtomicBool; 5] = [AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false)];
static HOOK_THREAD: AtomicU32 = AtomicU32::new(0);
const WM_REBIND: u32 = WM_APP + 1;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Chord {
    Dictation,
    Lasso,
    Command,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Signal {
    Start(Chord),
    End { chord: Chord, cancelled: bool },
}

#[derive(Clone, Copy)]
enum Mode {
    Idle,
    Pending(Chord, Instant),
    Active(Chord, Instant),
}

pub struct ChordMachine {
    pub dictation: u8,
    pub lasso: u8,
    pub command: u8,
    held: u8,
    dirty: bool,
    mode: Mode,
}

impl ChordMachine {
    pub fn new() -> Self {
        Self {
            dictation: CTRL | WIN,
            lasso: CTRL | SHIFT,
            command: ALT | WIN,
            held: 0,
            dirty: false,
            mode: Mode::Idle,
        }
    }

    pub fn key(&mut self, vk: u32, down: bool) {
        match modifier(vk) {
            Some(bit) if down => self.held |= bit,
            Some(bit) => self.held &= !bit,
            None if down => self.dirty = true,
            None => {}
        }
    }

    pub fn held(&self) -> u8 {
        self.held
    }

    pub fn sync(&mut self, held: u8) {
        self.held = held;
    }

    pub fn deadline(&self) -> Option<Instant> {
        match self.mode {
            Mode::Pending(_, since) => Some(since + ARM_DELAY),
            Mode::Active(_, since) => Some(since + MAX_HOLD),
            Mode::Idle => None,
        }
    }

    pub fn poll(&mut self, now: Instant) -> Option<Signal> {
        let held = if self.dirty || self.held == 0 {
            None
        } else if self.held == self.dictation {
            Some(Chord::Dictation)
        } else if self.held == self.lasso {
            Some(Chord::Lasso)
        } else if self.held == self.command {
            Some(Chord::Command)
        } else {
            None
        };

        let mut signal = None;
        self.mode = match self.mode {
            Mode::Idle => match held {
                Some(chord) => Mode::Pending(chord, now),
                None => Mode::Idle,
            },
            Mode::Pending(chord, since) => {
                if held != Some(chord) {
                    Mode::Idle
                } else if now.duration_since(since) >= ARM_DELAY {
                    signal = Some(Signal::Start(chord));
                    Mode::Active(chord, now)
                } else {
                    self.mode
                }
            }
            Mode::Active(chord, since) => {
                if now.duration_since(since) >= MAX_HOLD {
                    self.dirty = true;
                    signal = Some(Signal::End { chord, cancelled: true });
                    Mode::Idle
                } else if held == Some(chord) {
                    self.mode
                } else {
                    signal = Some(Signal::End { chord, cancelled: self.dirty });
                    Mode::Idle
                }
            }
        };

        if self.held == 0 {
            self.dirty = false;
        }
        signal
    }
}

#[derive(Clone, Serialize)]
struct ChordEvent {
    chord: Chord,
    phase: &'static str,
    cancelled: bool,
    cursor: Option<(f64, f64)>,
    asking: bool,
}

#[tauri::command]
pub fn set_key_echo(on: bool) {
    ECHO.store(on, Ordering::Relaxed);
}

#[tauri::command]
pub fn set_chords_paused(on: bool) {
    PAUSED.store(on, Ordering::Relaxed);
}

#[tauri::command]
pub fn capture_keys_available() -> [bool; 5] {
    [0, 1, 2, 3, 4].map(|i| CAPTURE_OK[i].load(Ordering::Relaxed))
}

pub fn configure(dictation: u8, lasso: u8, command: u8, captures: [Hotkey; 5]) {
    DICTATION_KEYS.store(dictation, Ordering::Relaxed);
    LASSO_KEYS.store(lasso, Ordering::Relaxed);
    COMMAND_KEYS.store(command, Ordering::Relaxed);
    *CAPTURE_KEYS.lock().unwrap() = captures;
    let thread = HOOK_THREAD.load(Ordering::Relaxed);
    if thread != 0 {
        unsafe {
            let _ = PostThreadMessageW(thread, WM_REBIND, WPARAM(0), LPARAM(0));
        }
    }
}

unsafe fn bind_captures() {
    let keys = *CAPTURE_KEYS.lock().unwrap();
    for (i, key) in keys.iter().enumerate() {
        let id = i as i32 + 1;
        let _ = UnregisterHotKey(None, id);
        let mut mods = MOD_NOREPEAT;
        for (bit, flag) in [(CTRL, MOD_CONTROL), (SHIFT, MOD_SHIFT), (ALT, MOD_ALT), (WIN, MOD_WIN)] {
            if key.mods & bit != 0 {
                mods |= flag;
            }
        }
        CAPTURE_OK[i].store(RegisterHotKey(None, id, mods, key.vk).is_ok(), Ordering::Relaxed);
    }
}

pub fn spawn(app: AppHandle) {
    if cfg!(debug_assertions) && std::env::var_os("COCUYO_ACCEPT_INJECTED").is_some() {
        ACCEPT_INJECTED.store(true, Ordering::Relaxed);
    }

    let (tx, rx) = mpsc::channel();
    let _ = TX.set(tx);
    let hook_app = app.clone();

    std::thread::Builder::new()
        .name("acordes".into())
        .spawn(move || run_chords(app, rx))
        .expect("hilo de acordes");

    std::thread::Builder::new()
        .name("hook-teclado".into())
        .spawn(move || unsafe {
            let module: HINSTANCE = GetModuleHandleW(None).expect("módulo propio").into();
            let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), Some(module), 0)
                .expect("instalar WH_KEYBOARD_LL");

            HOOK_THREAD.store(GetCurrentThreadId(), Ordering::Relaxed);
            bind_captures();
            placement::install(hook_app.clone());
            clip::listen();

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
                if msg.message == WM_HOTKEY {
                    if !PAUSED.load(Ordering::Relaxed) {
                        let app = hook_app.clone();
                        let id = msg.wParam.0;
                        std::thread::spawn(move || capture::hotkey(&app, id));
                    }
                } else if msg.message == WM_TIMER {
                    placement::on_timer(msg.wParam.0);
                } else if msg.message == WM_REBIND {
                    bind_captures();
                } else if msg.message == WM_CLIPBOARDUPDATE {
                    clip::on_change();
                }
            }
            let _ = UnhookWindowsHookEx(hook);
        })
        .expect("hilo del hook");
}

static HOOK_MATCHED: AtomicBool = AtomicBool::new(false);

const MODIFIER_KEYS: [(u32, u8); 8] =
    [(0xA2, CTRL), (0xA3, CTRL), (0xA0, SHIFT), (0xA1, SHIFT), (0xA4, ALT), (0xA5, ALT), (0x5B, WIN), (0x5C, WIN)];

fn modifiers_after(vk: u32, down: bool) -> u8 {
    MODIFIER_KEYS.iter().fold(0, |held, (key, bit)| {
        let is_down = if *key == vk { down } else { (unsafe { GetAsyncKeyState(*key as i32) }) < 0 };
        if is_down {
            held | bit
        } else {
            held
        }
    })
}

fn guard_release(down: bool, held: u8) {
    if down {
        let chords = [&DICTATION_KEYS, &LASSO_KEYS, &COMMAND_KEYS];
        if !PAUSED.load(Ordering::Relaxed) && chords.iter().any(|c| c.load(Ordering::Relaxed) == held) {
            HOOK_MATCHED.store(true, Ordering::Relaxed);
        }
    } else if HOOK_MATCHED.load(Ordering::Relaxed) {
        mask_modifiers();
    }
    if held == 0 {
        HOOK_MATCHED.store(false, Ordering::Relaxed);
    }
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let injected = kb.flags.0 & LLKHF_INJECTED.0 != 0;
        let ours = kb.vkCode == VK_MASK as u32;
        if !ours && (!injected || ACCEPT_INJECTED.load(Ordering::Relaxed)) {
            let down = matches!(wparam.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
            let held = modifiers_after(kb.vkCode, down);
            if modifier(kb.vkCode).is_some() {
                guard_release(down, held);
            } else if held == 0 {
                HOOK_MATCHED.store(false, Ordering::Relaxed);
            }
            if let Some(tx) = TX.get() {
                let _ = tx.send(KeyEdge { vk: kb.vkCode, down, held });
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

pub fn release_modifiers() {
    let up = |vk: u32| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: VIRTUAL_KEY(vk as u16), wScan: 0, dwFlags: KEYEVENTF_KEYUP, time: 0, dwExtraInfo: 0 },
        },
    };
    mask_modifiers();
    let inputs = MODIFIER_KEYS.map(|(vk, _)| up(vk));
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
    HOOK_MATCHED.store(false, Ordering::Relaxed);
}

#[derive(Default)]
pub struct StuckWin {
    since: Option<Instant>,
    keys: u32,
}

impl StuckWin {
    const AFTER: Duration = Duration::from_secs(4);
    const KEYS: u32 = 3;

    pub fn stuck(&mut self, held: u8, typed: bool, now: Instant) -> bool {
        if held & WIN == 0 {
            *self = Self::default();
            return false;
        }
        let since = *self.since.get_or_insert(now);
        self.keys += u32::from(typed);
        if self.keys >= Self::KEYS && now.duration_since(since) >= Self::AFTER {
            *self = Self::default();
            return true;
        }
        false
    }
}

fn run_chords(app: AppHandle, rx: Receiver<KeyEdge>) {
    let mut machine = ChordMachine::new();
    let mut echoed = 0;
    let mut stuck_win = StuckWin::default();

    loop {
        let edge = match machine.deadline() {
            Some(deadline) => {
                match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                    Ok(edge) => Some(edge),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
            None => match rx.recv() {
                Ok(edge) => Some(edge),
                Err(_) => break,
            },
        };

        if let Some(edge) = edge {
            if edge.vk == VK_ESCAPE && edge.down && capture::selecting() {
                capture::cancel_selection(&app);
            }
            if edge.down && modifier(edge.vk).is_none() && ECHO.load(Ordering::Relaxed) {
                let _ = app.emit_to(overlay::LABEL, "key", edge.vk);
            }
            machine.key(edge.vk, edge.down);
            machine.sync(edge.held);
            let typed = edge.down && matches!(edge.vk, 0x30..=0x5A | 0x20 | 0x0D);
            if stuck_win.stuck(edge.held, typed, Instant::now()) {
                release_modifiers();
                machine.sync(0);
                let _ = app.emit_to(overlay::LABEL, "note", "La tecla Windows se había quedado pegada: la solté.");
            }
        }

        if ECHO.load(Ordering::Relaxed) && machine.held() != echoed {
            echoed = machine.held();
            let _ = app.emit_to(overlay::LABEL, "keys", echoed);
        }

        if PAUSED.load(Ordering::Relaxed) {
            machine.dictation = 0;
            machine.lasso = 0;
            machine.command = 0;
        } else {
            machine.dictation = DICTATION_KEYS.load(Ordering::Relaxed);
            machine.lasso = LASSO_KEYS.load(Ordering::Relaxed);
            machine.command = COMMAND_KEYS.load(Ordering::Relaxed);
            if placement::fullscreen() || placement::exclusive() {
                machine.lasso = 0;
            }
        }
        if let Some(signal) = machine.poll(Instant::now()) {
            fire(&app, signal);
        }
    }
}

fn mask_modifiers() {
    let key = |flags| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(VK_MASK),
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let inputs = [key(KEYBD_EVENT_FLAGS(0)), key(KEYEVENTF_KEYUP)];
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

pub fn from_pad(app: &AppHandle, down: bool) {
    fire(app, if down { Signal::Start(Chord::Command) } else { Signal::End { chord: Chord::Command, cancelled: false } });
}

fn fire(app: &AppHandle, signal: Signal) {
    let event = match signal {
        Signal::Start(chord) => {
            mask_modifiers();
            let mut asking = false;
            let cursor = if chord == Chord::Lasso {
                capture::begin(app);
                overlay::open_fully(app);
                asking = voice::lasso_listen(app);
                overlay::cursor_in_window(app)
            } else {
                mic::start();
                if chord == Chord::Command {
                    crate::talk::begin(app);
                }
                crate::cue::play(crate::cue::Cue::Listen);
                crate::gamelayer::listening(app);
                None
            };
            ChordEvent { chord, phase: "start", cancelled: false, cursor, asking }
        }
        Signal::End { chord, cancelled } => {
            if chord != Chord::Lasso {
                if cancelled {
                    crate::gamelayer::cancel();
                } else {
                    crate::gamelayer::thinking();
                }
                let app = app.clone();
                std::thread::spawn(move || voice::finish(&app, chord, cancelled));
            }
            let asking = chord == Chord::Lasso && voice::lasso_heard(app, cancelled);
            ChordEvent { chord, phase: "end", cancelled, cursor: None, asking }
        }
    };
    let _ = app.emit_to(overlay::LABEL, "chord", event);
}

#[cfg(test)]
mod tests {
    use super::*;

    const VK_LCONTROL: u32 = 0xA2;
    const VK_LSHIFT: u32 = 0xA0;
    const VK_LWIN: u32 = 0x5B;
    const VK_LMENU: u32 = 0xA4;
    const VK_RMENU: u32 = 0xA5;
    const VK_T: u32 = 0x54;

    fn after(start: Instant, ms: u64) -> Instant {
        start + Duration::from_millis(ms)
    }

    #[test]
    fn hold_ctrl_win_starts_and_ends_dictation() {
        let t0 = Instant::now();
        let mut m = ChordMachine::new();
        m.key(VK_LCONTROL, true);
        assert_eq!(m.poll(t0), None);
        m.key(VK_LWIN, true);
        assert_eq!(m.poll(after(t0, 10)), None);
        assert_eq!(m.poll(after(t0, 160)), Some(Signal::Start(Chord::Dictation)));
        m.key(VK_LWIN, false);
        assert_eq!(
            m.poll(after(t0, 900)),
            Some(Signal::End { chord: Chord::Dictation, cancelled: false })
        );
    }

    #[test]
    fn a_switched_off_chord_never_fires_with_nothing_held() {
        let t0 = Instant::now();
        let mut m = ChordMachine::new();
        m.lasso = 0;
        m.key(0x57, true);
        assert_eq!(m.poll(t0), None);
        m.key(0x57, false);
        assert_eq!(m.poll(t0 + Duration::from_millis(10)), None);
        assert_eq!(m.poll(t0 + Duration::from_millis(400)), None);
        assert_eq!(m.deadline(), None, "no queda nada armado esperando");

        m.dictation = 0;
        m.command = 0;
        assert_eq!(m.poll(t0 + Duration::from_millis(800)), None);
        assert_eq!(m.poll(t0 + Duration::from_millis(1200)), None);
    }

    #[test]
    fn a_chord_held_forever_is_given_up() {
        let t0 = Instant::now();
        let mut m = ChordMachine::new();
        m.key(VK_LMENU, true);
        m.key(VK_LWIN, true);
        m.poll(t0);
        assert_eq!(m.poll(after(t0, 200)), Some(Signal::Start(Chord::Command)));
        assert_eq!(m.poll(after(t0, 91_000)), Some(Signal::End { chord: Chord::Command, cancelled: true }));
        assert_eq!(m.poll(after(t0, 92_000)), None);
        assert_eq!(m.deadline(), None);
    }

    #[test]
    fn the_real_keyboard_state_overrides_a_stale_count() {
        let t0 = Instant::now();
        let mut m = ChordMachine::new();
        m.key(VK_LCONTROL, true);
        m.sync(0);
        m.poll(t0);
        m.key(VK_LWIN, true);
        m.sync(WIN);
        m.poll(after(t0, 10));
        assert_eq!(m.poll(after(t0, 400)), None);
    }

    #[test]
    fn a_stuck_windows_key_is_noticed() {
        let t0 = Instant::now();
        let mut stuck = StuckWin::default();
        assert!(!stuck.stuck(WIN, false, t0));
        assert!(!stuck.stuck(WIN, true, after(t0, 80)));
        assert!(!stuck.stuck(0, false, after(t0, 150)));
        assert!(!stuck.stuck(CTRL | WIN, false, after(t0, 1_000)));
        assert!(!stuck.stuck(CTRL | WIN, false, after(t0, 31_000)));
        assert!(!stuck.stuck(0, false, after(t0, 31_100)));
        assert!(!stuck.stuck(WIN, true, after(t0, 40_000)));
        assert!(!stuck.stuck(WIN, true, after(t0, 41_000)));
        assert!(!stuck.stuck(WIN, true, after(t0, 42_000)));
        assert!(stuck.stuck(WIN, true, after(t0, 44_500)));
        assert!(!stuck.stuck(WIN, true, after(t0, 44_600)));
    }

    #[test]
    fn quick_shortcut_never_arms() {
        let t0 = Instant::now();
        let mut m = ChordMachine::new();
        m.key(VK_LCONTROL, true);
        m.key(VK_LSHIFT, true);
        assert_eq!(m.poll(t0), None);
        m.key(VK_T, true);
        assert_eq!(m.poll(after(t0, 40)), None);
        m.key(VK_T, false);
        assert_eq!(m.poll(after(t0, 400)), None);
    }

    #[test]
    fn release_before_delay_does_nothing() {
        let t0 = Instant::now();
        let mut m = ChordMachine::new();
        m.key(VK_LCONTROL, true);
        m.key(VK_LSHIFT, true);
        assert_eq!(m.poll(t0), None);
        m.key(VK_LSHIFT, false);
        assert_eq!(m.poll(after(t0, 80)), None);
        assert_eq!(m.deadline(), None);
    }

    #[test]
    fn extra_key_cancels_active_chord_until_all_released() {
        let t0 = Instant::now();
        let mut m = ChordMachine::new();
        m.key(VK_LCONTROL, true);
        m.key(VK_LSHIFT, true);
        m.poll(t0);
        assert_eq!(m.poll(after(t0, 200)), Some(Signal::Start(Chord::Lasso)));
        m.key(VK_T, true);
        assert_eq!(
            m.poll(after(t0, 300)),
            Some(Signal::End { chord: Chord::Lasso, cancelled: true })
        );
        m.key(VK_T, false);
        assert_eq!(m.poll(after(t0, 800)), None);
        assert_eq!(m.deadline(), None);
        m.key(VK_LCONTROL, false);
        m.key(VK_LSHIFT, false);
        m.poll(after(t0, 900));
        m.key(VK_LCONTROL, true);
        m.key(VK_LSHIFT, true);
        m.poll(after(t0, 1000));
        assert_eq!(m.poll(after(t0, 1200)), Some(Signal::Start(Chord::Lasso)));
    }

    #[test]
    fn altgr_does_not_arm() {
        let t0 = Instant::now();
        let mut m = ChordMachine::new();
        m.key(VK_LCONTROL, true);
        m.key(VK_RMENU, true);
        assert_eq!(m.poll(t0), None);
        assert_eq!(m.poll(after(t0, 500)), None);
        m.key(VK_LSHIFT, true);
        assert_eq!(m.poll(after(t0, 600)), None);
        assert_eq!(m.poll(after(t0, 1100)), None);
    }

    #[test]
    fn an_extra_modifier_is_a_different_shortcut() {
        let t0 = Instant::now();
        let mut m = ChordMachine::new();
        m.key(VK_LCONTROL, true);
        m.key(VK_LSHIFT, true);
        m.key(VK_LWIN, true);
        assert_eq!(m.poll(t0), None);
        assert_eq!(m.poll(after(t0, 500)), None);
    }

    #[test]
    fn custom_chords_replace_the_defaults() {
        let t0 = Instant::now();
        let mut m = ChordMachine::new();
        m.dictation = ALT | SHIFT;
        m.key(VK_LMENU, true);
        m.key(VK_LSHIFT, true);
        m.poll(t0);
        assert_eq!(m.poll(after(t0, 200)), Some(Signal::Start(Chord::Dictation)));
        m.key(VK_LMENU, false);
        m.key(VK_LSHIFT, false);
        m.poll(after(t0, 300));
        m.key(VK_LCONTROL, true);
        m.key(VK_LWIN, true);
        m.poll(after(t0, 400));
        assert_eq!(m.poll(after(t0, 900)), None);
    }

    #[test]
    fn hotkey_rules() {
        assert!(valid_hotkey(Hotkey { mods: 0, vk: 0x2C }));
        assert!(valid_hotkey(Hotkey { mods: 0, vk: 0x78 }));
        assert!(valid_hotkey(Hotkey { mods: CTRL | SHIFT, vk: 0x53 }));
        assert!(!valid_hotkey(Hotkey { mods: 0, vk: 0x53 }));
        assert!(!valid_hotkey(Hotkey { mods: CTRL, vk: 0 }));
        assert!(!valid_hotkey(Hotkey { mods: CTRL, vk: 0xA0 }));
    }

    #[test]
    fn chord_rules() {
        assert!(valid_chord(CTRL | WIN));
        assert!(valid_chord(ALT | SHIFT));
        assert!(valid_chord(CTRL | SHIFT | ALT));
        assert!(!valid_chord(CTRL));
        assert!(!valid_chord(0));
        assert!(!valid_chord(CTRL | ALT));
    }
}
