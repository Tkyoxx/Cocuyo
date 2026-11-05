// el microfono limpio para las otras apps: tu micro de verdad pasa por el quitarruido y sale por el cable virtual (vb-cable).
// en discord, zoom o whatsapp eliges «CABLE Output». solo trabaja mientras alguna app escucha el cable; si no, ni abre el micro

use std::collections::{HashMap, HashSet, VecDeque};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait};
use serde::Serialize;
use tauri::AppHandle;
use windows::core::{implement, w, Interface, HSTRING};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, WAIT_OBJECT_0};
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED};
use windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
use windows::Win32::System::Threading::{AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsW, CreateEventW, GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, SetEvent, WaitForMultipleObjects, WaitForSingleObject, INFINITE, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use crate::apps::CREATE_NO_WINDOW;
use crate::profundo::{Level, Profundo, DELAY, HOP};
use crate::settings::Settings;

const RATE: u32 = 48_000;
const QUEUE: usize = HOP;
const AT_MOST: u32 = HOP as u32 * 2;
const LIGHTEN_AT: Duration = Duration::from_micros(6_000);
const RAW_AT: Duration = Duration::from_micros(8_500);
const FIRST_WAIT: Duration = Duration::from_secs(30);
const LONGEST_WAIT: Duration = Duration::from_secs(600);
const CABLE_ZIP: &str = "https://download.vb-audio.com/Download_CABLE/VBCABLE_Driver_Pack45.zip";
const CABLE_SHA: &str = "b950e39f01af1d04ea623c8f6d8eb9b6ea5c477c637295fabf20631c85116bfb";
const CABLE_SETUP: &str = "VBCABLE_Setup_x64.exe";

#[derive(Clone, Debug, PartialEq)]
pub struct Tune {
    pub limit_db: f32,
    pub between: bool,
    pub gate: u32,
    pub gain_db: f32,
    pub level: bool,
    pub light: bool,
}

impl Tune {
    pub fn of(s: &Settings) -> Tune {
        let (limit_db, between, gate) = match s.clean_mic_mode.as_str() {
            "natural" => (18.0, false, 0),
            "tope" => (100.0, true, 2),
            "propio" => (if s.clean_mic_amount >= 100 { 100.0 } else { s.clean_mic_amount as f32 * 0.45 }, s.clean_mic_between, s.clean_mic_gate.min(3)),
            _ => (100.0, false, 0),
        };
        Tune { limit_db, between, gate, gain_db: (s.clean_mic_gain as f32).clamp(-12.0, 18.0), level: s.clean_mic_level, light: s.clean_mic_light }
    }
}

pub struct Chain {
    engine: Profundo,
    tune: Tune,
    gate: f32,
    hold: u32,
    level: f32,
    applied: f32,
    pub lsnr: f32,
}

impl Chain {
    pub fn new(tune: &Tune) -> Chain {
        let mut engine = Profundo::with(Level::Normal, !tune.light);
        Chain::setup(&mut engine, tune);
        Chain { engine, tune: tune.clone(), gate: 1.0, hold: 0, level: 1.0, applied: 10f32.powf(tune.gain_db / 20.0), lsnr: -15.0 }
    }

    fn setup(engine: &mut Profundo, tune: &Tune) {
        engine.tune(tune.limit_db, tune.between);
        engine.set_thresholds(-15.0, 35.0, 35.0);
    }

    pub fn retune(&mut self, tune: &Tune) {
        if tune.light != self.tune.light {
            self.engine = Profundo::with(Level::Normal, !tune.light);
        }
        Chain::setup(&mut self.engine, tune);
        self.tune = tune.clone();
    }

    pub fn frame(&mut self, input: &[f32], out: &mut [f32]) {
        self.lsnr = self.engine.frame(input, out);
        let open = if self.tune.gate == 0 {
            true
        } else {
            let threshold = [-8.0, -3.0, 2.0][(self.tune.gate - 1) as usize];
            if self.lsnr > threshold {
                self.hold = 25;
            } else {
                self.hold = self.hold.saturating_sub(1);
            }
            self.hold > 0
        };
        self.finish(out, open, self.tune.level && self.lsnr > 5.0);
    }

    pub fn pass(&mut self, input: &[f32], out: &mut [f32]) {
        out.copy_from_slice(input);
        self.finish(out, true, false);
    }

    fn adopt(&mut self, mut engine: Profundo) {
        Chain::setup(&mut engine, &self.tune);
        self.engine = engine;
        self.tune.light = true;
    }

    fn finish(&mut self, out: &mut [f32], open: bool, learn: bool) {
        if learn {
            let rms = (out.iter().map(|v| v * v).sum::<f32>() / out.len() as f32).sqrt();
            if rms > 1e-4 {
                let want = (0.08 / rms).clamp(0.25, 6.0);
                self.level += (want - self.level) * if want < self.level { 0.15 } else { 0.03 };
            }
        } else if !self.tune.level {
            self.level = 1.0;
        }
        let gain = 10f32.powf(self.tune.gain_db / 20.0) * self.level;
        let (from, step) = (self.applied, (gain - self.applied) / out.len() as f32);
        let target = if open { 1.0 } else { 0.0 };
        let ramp = if open { 1.0 / 144.0 } else { 1.0 / 1920.0 };
        for (i, v) in out.iter_mut().enumerate() {
            self.gate = if target > self.gate { (self.gate + ramp).min(1.0) } else { (self.gate - ramp).max(0.0) };
            *v = soft(*v * (from + step * i as f32) * self.gate);
        }
        self.applied = gain;
    }
}

fn soft(x: f32) -> f32 {
    let a = x.abs();
    if a <= 0.85 {
        x
    } else {
        x.signum() * (0.85 + 0.15 * ((a - 0.85) / 0.15).tanh())
    }
}

#[derive(Debug, PartialEq)]
enum Pace {
    Lighten,
    Raw,
}

struct Guard {
    spent: Duration,
    frames: u32,
    skip: u32,
    strikes: u32,
    lowered: bool,
    raw_until: Option<Instant>,
    wait: Duration,
}

impl Guard {
    fn new() -> Guard {
        Guard { spent: Duration::ZERO, frames: 0, skip: 10, strikes: 0, lowered: false, raw_until: None, wait: FIRST_WAIT }
    }

    fn restart(&mut self) {
        (self.spent, self.frames, self.skip, self.strikes) = (Duration::ZERO, 0, 10, 0);
    }

    fn shape(&self, tune: &Tune) -> Tune {
        Tune { light: tune.light || self.lowered, ..tune.clone() }
    }

    fn raw(&mut self, now: Instant) -> bool {
        match self.raw_until {
            Some(until) if now < until => true,
            Some(_) => {
                self.raw_until = None;
                self.restart();
                self.strikes = 1;
                false
            }
            None => false,
        }
    }

    fn note(&mut self, took: Duration, light: bool, now: Instant) -> Option<Pace> {
        if self.skip > 0 {
            self.skip -= 1;
            return None;
        }
        if took < Duration::from_micros(30) {
            return None;
        }
        self.spent += took;
        self.frames += 1;
        if self.frames < 25 && self.spent < Duration::from_millis(250) {
            return None;
        }
        let mean = self.spent / self.frames;
        (self.spent, self.frames) = (Duration::ZERO, 0);
        if mean < if light { RAW_AT } else { LIGHTEN_AT } {
            self.strikes = 0;
            self.wait = FIRST_WAIT;
            return None;
        }
        self.strikes += 1;
        if self.strikes < 2 {
            return None;
        }
        self.strikes = 0;
        if light {
            self.raw_until = Some(now + self.wait);
            self.wait = (self.wait * 2).min(LONGEST_WAIT);
            Some(Pace::Raw)
        } else {
            self.lowered = true;
            Some(Pace::Lighten)
        }
    }
}

struct Boost(HANDLE);

impl Boost {
    unsafe fn new() -> Option<Boost> {
        let mut task = 0;
        AvSetMmThreadCharacteristicsW(w!("Audio"), &mut task).ok().map(Boost)
    }
}

impl Drop for Boost {
    fn drop(&mut self) {
        unsafe {
            let _ = AvRevertMmThreadCharacteristics(self.0);
        }
    }
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Aparato {
    pub id: String,
    pub nombre: String,
    pub cable: bool,
    pub windows: bool,
}

fn is_cable(name: &str) -> bool {
    let lower = name.to_lowercase();
    ["cable", "vb-audio", "voicemeeter", "virtual"].iter().any(|w| lower.contains(w))
}

fn list(inputs: bool) -> Vec<Aparato> {
    let host = cpal::default_host();
    let default = if inputs { host.default_input_device() } else { host.default_output_device() }.and_then(|d| d.id().ok()).map(|id| id.to_string());
    let all = if inputs { host.input_devices().ok().map(|d| d.collect::<Vec<_>>()) } else { host.output_devices().ok().map(|d| d.collect::<Vec<_>>()) };
    all.unwrap_or_default()
        .into_iter()
        .filter_map(|device| {
            let id = device.id().ok()?.to_string();
            let nombre = device.description().map(|about| about.name().to_string()).unwrap_or_else(|_| device.to_string());
            Some(Aparato { cable: is_cable(&nombre), windows: default.as_deref() == Some(id.as_str()), id, nombre })
        })
        .collect()
}

fn pick_output(wanted: &str, outputs: &[Aparato]) -> Option<Aparato> {
    if let Some(found) = outputs.iter().find(|o| !wanted.is_empty() && o.id == wanted) {
        return Some(found.clone());
    }
    let lower = |o: &Aparato| o.nombre.to_lowercase();
    outputs
        .iter()
        .find(|o| lower(o).contains("cable input"))
        .or_else(|| outputs.iter().find(|o| o.cable && lower(o).contains("input")))
        .or_else(|| outputs.iter().find(|o| o.cable))
        .cloned()
}

fn suffix(name: &str) -> String {
    name.find('(').map(|at| name[at..].to_lowercase()).unwrap_or_default()
}

fn pair(output: &Aparato, inputs: &[Aparato]) -> Option<Aparato> {
    let swapped = output.nombre.to_lowercase().replace("input", "output");
    let cables: Vec<&Aparato> = inputs.iter().filter(|i| i.cable).collect();
    cables
        .iter()
        .find(|i| i.nombre.to_lowercase() == swapped)
        .or_else(|| cables.iter().find(|i| !suffix(&i.nombre).is_empty() && suffix(&i.nombre) == suffix(&output.nombre)))
        .or_else(|| (cables.len() == 1).then(|| &cables[0]))
        .map(|i| (*i).clone())
}

fn pick_input(wanted: &str, mic: &str, inputs: &[Aparato]) -> Option<Aparato> {
    let real = |id: &str| inputs.iter().find(|i| !id.is_empty() && i.id == id && !i.cable);
    real(wanted)
        .or_else(|| real(mic))
        .or_else(|| inputs.iter().find(|i| i.windows && !i.cable))
        .or_else(|| inputs.iter().find(|i| !i.cable))
        .cloned()
}

#[derive(Clone, PartialEq, Debug)]
struct Want {
    input: String,
    mic: String,
    output: String,
    tune: Tune,
}

#[derive(Serialize, Clone, Default)]
pub struct Estado {
    prendido: bool,
    estado: String,
    cable: String,
    elige: String,
    entrada: String,
    escuchan: Vec<String>,
    limpiando: bool,
    cpu: f32,
    voz: bool,
    entra: f32,
    sale: f32,
    latencia: u32,
    error: String,
    bajo: bool,
    ahora: Vec<String>,
}

static WANT: Mutex<Option<Want>> = Mutex::new(None);
static STATUS: Mutex<Option<Estado>> = Mutex::new(None);
static WORKER: Mutex<Option<Worker>> = Mutex::new(None);
static ACTIVE: AtomicBool = AtomicBool::new(true);
static WAKE: AtomicIsize = AtomicIsize::new(0);
static WHEN: Mutex<(String, Vec<String>)> = Mutex::new((String::new(), Vec::new()));
static RECENT: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());
static WATCHED: Mutex<Vec<(String, bool)>> = Mutex::new(Vec::new());
static INSTALLED: Mutex<Option<Vec<(String, String)>>> = Mutex::new(None);
static KINDS: Mutex<Option<HashMap<String, bool>>> = Mutex::new(None);
static NAMES: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);
static EVERYTHING: Mutex<Option<(Instant, Vec<Juego>)>> = Mutex::new(None);
static SCANNING: AtomicBool = AtomicBool::new(false);
const BROWSERS: [&str; 12] = ["chrome", "msedge", "firefox", "opera", "brave", "vivaldi", "vlc", "mpc-hc64", "mpc-be64", "potplayermini64", "netflix", "wmplayer"];
const NOT_APPS: [&str; 8] = ["cocuyo", "explorer", "searchapp", "shellexperiencehost", "startmenuexperiencehost", "applicationframehost", "lockapp", "textinputhost"];
const NOT_VOICE: [&str; 14] = ["amdrsserv", "audiodg", "ffmpeg", "msedgewebview2", "nvcontainer", "nvsphelper64", "python", "pythonw", "rundll32", "speechuxwiz", "steam", "steamwebhelper", "wallpaper32", "wallpaper64"];
const CALL_APPS: [&str; 19] = ["discord", "discordptb", "discordcanary", "zoom", "teams", "ms-teams", "whatsapp", "whatsapp.root", "telegram", "skype", "slack", "signal", "viber", "ciscocollabhost", "ts3client_win64", "mumble", "obs64", "obs32", "streamlabs obs"];
const LAUNCHERS: [(&str, &str); 2] = [("robloxplayerlauncher", "robloxplayerbeta"), ("robloxplayerinstaller", "robloxplayerbeta")];
const STORE_APPS: [(&str, &str); 5] = [("whatsappdesktop", "whatsapp.root"), ("msteams", "ms-teams"), ("telegram", "telegram"), ("discord", "discord"), ("zoom", "zoom")];
const KNOWN_GAMES: [(&str, &str); 16] = [
    ("robloxplayerbeta", "Roblox"),
    ("rocketleague", "Rocket League"),
    ("gta_sa", "GTA San Andreas"),
    ("gta5", "GTA V"),
    ("gtaiv", "GTA IV"),
    ("minecraft.windows", "Minecraft"),
    ("minecraftlauncher", "Minecraft"),
    ("valorant-win64-shipping", "Valorant"),
    ("league of legends", "League of Legends"),
    ("fortniteclient-win64-shipping", "Fortnite"),
    ("cs2", "Counter-Strike 2"),
    ("dota2", "Dota 2"),
    ("r5apex", "Apex Legends"),
    ("overwatch", "Overwatch"),
    ("fifa23", "FIFA 23"),
    ("eldenring", "Elden Ring"),
];

fn status(change: impl FnOnce(&mut Estado)) {
    let mut guard = STATUS.lock().unwrap_or_else(|e| e.into_inner());
    change(guard.get_or_insert_with(Estado::default));
}

fn wake() {
    let event = WAKE.load(Ordering::Relaxed);
    if event != 0 {
        unsafe {
            let _ = SetEvent(HANDLE(event as _));
        }
    }
}

struct Worker {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        wake();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub fn sync(app: &AppHandle) {
    let s = crate::settings::load(app);
    *WHEN.lock().unwrap_or_else(|e| e.into_inner()) = (s.clean_mic_when.clone(), s.clean_mic_games.iter().map(|g| plain_exe(g)).collect());
    if s.clean_mic && s.clean_mic_when != "siempre" {
        let _ = std::thread::Builder::new().name("microfono-juegos".into()).spawn(scan_running);
    }
    decide_active();
    if !s.clean_mic {
        *WANT.lock().unwrap_or_else(|e| e.into_inner()) = None;
        let worker = WORKER.lock().unwrap_or_else(|e| e.into_inner()).take();
        drop(worker);
        status(|st| *st = Estado { estado: "apagado".into(), ..Default::default() });
        return;
    }
    let want = Want { input: s.clean_mic_in.clone(), mic: s.mic.clone(), output: s.clean_mic_out.clone(), tune: Tune::of(&s) };
    *WANT.lock().unwrap_or_else(|e| e.into_inner()) = Some(want);
    let mut worker = WORKER.lock().unwrap_or_else(|e| e.into_inner());
    if worker.as_ref().is_some_and(|w| w.thread.as_ref().is_some_and(|t| !t.is_finished())) {
        wake();
        return;
    }
    let stop = Arc::new(AtomicBool::new(false));
    let quit = stop.clone();
    let thread = std::thread::Builder::new()
        .name("microfono-limpio".into())
        .spawn(move || unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            run(&quit);
            CoUninitialize();
        })
        .ok();
    *worker = Some(Worker { stop, thread });
}

pub fn shutdown() {
    let worker = WORKER.lock().unwrap_or_else(|e| e.into_inner()).take();
    drop(worker);
}

fn plain_exe(name: &str) -> String {
    let lower = name.trim().to_lowercase();
    let file = lower.rsplit(['\\', '/']).next().unwrap_or("").to_string();
    file.strip_suffix(".exe").unwrap_or(&file).to_string()
}

fn triggered(when: &str, games: &[String], watched: &[(String, bool)]) -> Vec<String> {
    watched
        .iter()
        .filter(|(exe, game)| match when {
            "jugando" => *game,
            "elegidos" => games.contains(exe),
            _ => false,
        })
        .map(|(exe, _)| exe.clone())
        .collect()
}

fn triggered_now() -> Vec<String> {
    let (when, games) = WHEN.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let watched = WATCHED.lock().unwrap_or_else(|e| e.into_inner());
    triggered(&when, &games, &watched)
}

fn decide_active() {
    let when = WHEN.lock().unwrap_or_else(|e| e.into_inner()).0.clone();
    let active = !matches!(when.as_str(), "jugando" | "elegidos") || !triggered_now().is_empty();
    if ACTIVE.swap(active, Ordering::Relaxed) != active {
        wake();
    }
}

fn processes() -> Vec<(String, u32)> {
    let mut found = Vec::new();
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return found };
        let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut more = Process32FirstW(snapshot, &mut entry).is_ok();
        while more {
            let len = entry.szExeFile.iter().position(|c| *c == 0).unwrap_or(entry.szExeFile.len());
            found.push((plain_exe(&String::from_utf16_lossy(&entry.szExeFile[..len])), entry.th32ProcessID));
            more = Process32NextW(snapshot, &mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
    }
    found
}

fn watch(exe: &str, pid: u32, game: bool) {
    let mut watched = WATCHED.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(entry) = watched.iter_mut().find(|(name, _)| name == exe) {
        entry.1 |= game;
        return;
    }
    let Ok(process) = (unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) }) else { return };
    watched.push((exe.to_string(), game));
    let (exe, first) = (exe.to_string(), process.0 as isize);
    let _ = std::thread::Builder::new().name("microfono-juego".into()).stack_size(64 * 1024).spawn(move || unsafe {
        let mut handle = HANDLE(first as _);
        loop {
            WaitForSingleObject(handle, INFINITE);
            let _ = CloseHandle(handle);
            let next = processes().into_iter().find(|(name, _)| *name == exe).and_then(|(_, pid)| OpenProcess(PROCESS_SYNCHRONIZE, false, pid).ok());
            match next {
                Some(process) => handle = process,
                None => break,
            }
        }
        WATCHED.lock().unwrap_or_else(|e| e.into_inner()).retain(|(name, _)| *name != exe);
        decide_active();
    });
}

fn installed() -> Vec<(String, String)> {
    if let Some(list) = INSTALLED.lock().unwrap_or_else(|e| e.into_inner()).clone() {
        return list;
    }
    let mut seen = HashSet::new();
    let list: Vec<(String, String)> = crate::apps::installed_games()
        .into_iter()
        .map(|(name, path)| {
            let exe = plain_exe(&path);
            let name = KNOWN_GAMES.iter().find(|(known, _)| *known == exe).map(|(_, nice)| nice.to_string()).unwrap_or_else(|| capitalized(&name));
            (exe, name)
        })
        .filter(|(exe, _)| !exe.is_empty() && seen.insert(exe.clone()))
        .collect();
    *INSTALLED.lock().unwrap_or_else(|e| e.into_inner()) = Some(list.clone());
    list
}

fn mic_users() -> Vec<String> {
    use windows::core::PWSTR;
    use windows::Win32::System::Registry::{RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, KEY_READ, RRF_RT_QWORD};
    let mut found: Vec<(u64, String)> = Vec::new();
    unsafe {
        let mut key = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, w!("Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\microphone\\NonPackaged"), Some(0), KEY_READ, &mut key).is_err() {
            return Vec::new();
        }
        let mut index = 0;
        loop {
            let mut name = [0u16; 512];
            let mut len = name.len() as u32;
            if RegEnumKeyExW(key, index, Some(PWSTR(name.as_mut_ptr())), &mut len, None, None, None, None).is_err() {
                break;
            }
            index += 1;
            let entry = String::from_utf16_lossy(&name[..len as usize]);
            let (mut last, mut size) = (0u64, 8u32);
            let _ = RegGetValueW(key, &HSTRING::from(entry.as_str()), w!("LastUsedTimeStop"), RRF_RT_QWORD, None, Some(&mut last as *mut u64 as *mut _), Some(&mut size));
            found.push((last, entry.replace('#', "\\")));
        }
        let _ = RegCloseKey(key);
    }
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found.into_iter().map(|(_, path)| path).filter(|path| Path::new(path).is_file()).collect()
}

fn capitalized(name: &str) -> String {
    if name.chars().any(char::is_uppercase) {
        return name.to_string();
    }
    let mut chars = name.chars();
    chars.next().map(|first| first.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_default()
}

fn same_name(name: &str) -> String {
    name.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect()
}

fn skip_app(exe: &str) -> bool {
    exe.is_empty() || exe.starts_with("cocuyo") || NOT_APPS.contains(&exe) || NOT_VOICE.contains(&exe) || crate::apps::NOT_THE_GAME.iter().any(|junk| exe.contains(junk))
}

fn program_of(id: &str) -> Option<String> {
    let lower = id.to_lowercase();
    if lower.ends_with(".exe") {
        let exe = plain_exe(id);
        let real = LAUNCHERS.iter().find(|(launcher, _)| *launcher == exe).map_or(exe.clone(), |(_, game)| game.to_string());
        return (!lower.contains("{1ac14e77-") && !lower.contains("{d65231b0-")).then_some(real);
    }
    STORE_APPS.iter().find(|(word, _)| lower.contains(word)).map(|(_, exe)| exe.to_string())
}

fn scan_everything(app: &AppHandle) -> Vec<Juego> {
    let known = installed();
    let name = |exe: &str, path: Option<&str>| {
        if path.is_some_and(|p| p.to_lowercase().contains("minecraft")) {
            return "Minecraft".to_string();
        }
        known.iter().find(|(found, _)| found == exe).map(|(_, nombre)| nombre.clone()).unwrap_or_else(|| name_of(exe, path))
    };
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for path in mic_users() {
        let exe = plain_exe(&path);
        let lower = path.to_lowercase();
        if !skip_app(&exe) && !lower.contains("\\temp\\") && !lower.contains("\\.claude\\") && seen.insert(exe.clone()) {
            let grupo = if !CALL_APPS.contains(&exe.as_str()) && is_game(&exe, &path) { "juego" } else { "voz" };
            out.push(Juego { nombre: name(&exe, Some(&path)), exe, grupo });
        }
    }
    for (exe, nombre) in &known {
        if seen.insert(exe.clone()) {
            out.push(Juego { exe: exe.clone(), nombre: nombre.clone(), grupo: "juego" });
        }
    }
    for program in crate::apps::list(app) {
        let Some(exe) = program_of(&program.id) else { continue };
        let named = same_name(&program.name);
        if skip_app(&exe) || out.iter().any(|game| same_name(&game.nombre) == named) || !seen.insert(exe.clone()) {
            continue;
        }
        let grupo = if CALL_APPS.contains(&exe.as_str()) {
            "voz"
        } else if KNOWN_GAMES.iter().any(|(known, _)| *known == exe) || (program.id.contains(":\\") && is_game(&exe, &program.id)) {
            "juego"
        } else {
            "programa"
        };
        out.push(Juego { exe, nombre: program.name, grupo });
    }
    out
}

fn everything(app: &AppHandle) -> (Vec<Juego>, bool) {
    let cached = EVERYTHING.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let stale = cached.as_ref().is_none_or(|(at, _)| at.elapsed() > Duration::from_secs(600));
    if stale && !SCANNING.swap(true, Ordering::Relaxed) {
        let app = app.clone();
        let _ = std::thread::Builder::new().name("microfono-lista".into()).spawn(move || {
            let list = scan_everything(&app);
            *EVERYTHING.lock().unwrap_or_else(|e| e.into_inner()) = Some((Instant::now(), list));
            SCANNING.store(false, Ordering::Relaxed);
        });
    }
    let loading = cached.is_none();
    (cached.map(|(_, list)| list).unwrap_or_default(), loading)
}

fn is_game(exe: &str, path: &str) -> bool {
    if let Some(kind) = KINDS.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|kinds| kinds.get(exe).copied()) {
        return kind;
    }
    let kind = KNOWN_GAMES.iter().any(|(known, _)| *known == exe) || installed().iter().any(|(known, _)| known == exe) || crate::apps::looks_like_a_game(Path::new(path));
    KINDS.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(exe.to_string(), kind);
    kind
}

fn open_apps() -> Vec<(String, u32, String)> {
    let mut seen = HashSet::new();
    let mut found = Vec::new();
    for hwnd in unsafe { crate::pc::app_windows() } {
        let mut pid = 0;
        unsafe { windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        let Some(path) = crate::apps::image_of(pid) else { continue };
        let exe = plain_exe(&path);
        if !exe.is_empty() && !NOT_APPS.contains(&exe.as_str()) && seen.insert(exe.clone()) {
            found.push((exe, pid, path));
        }
    }
    found
}

fn scan_running() {
    let (when, games) = WHEN.lock().unwrap_or_else(|e| e.into_inner()).clone();
    match when.as_str() {
        "elegidos" => {
            for (exe, pid) in processes() {
                if games.contains(&exe) {
                    watch(&exe, pid, false);
                }
            }
        }
        "jugando" => {
            for (exe, pid, path) in open_apps() {
                if is_game(&exe, &path) {
                    watch(&exe, pid, true);
                }
            }
        }
        _ => return,
    }
    decide_active();
}

fn name_of(exe: &str, path: Option<&str>) -> String {
    if let Some((_, name)) = KNOWN_GAMES.iter().find(|(known, _)| *known == exe) {
        return name.to_string();
    }
    if let Some(name) = NAMES.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|names| names.get(exe).cloned()) {
        return name;
    }
    let found = path.and_then(crate::system::description);
    if let Some(name) = &found {
        NAMES.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(exe.to_string(), name.clone());
    }
    found.unwrap_or_else(|| friendly(exe))
}

pub fn foreground(hwnd: HWND) {
    if WANT.lock().unwrap_or_else(|e| e.into_inner()).is_none() {
        return;
    }
    let mut pid = 0;
    unsafe { windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    let path = crate::apps::image_of(pid).unwrap_or_default();
    let exe = plain_exe(&path);
    if exe.is_empty() || NOT_APPS.contains(&exe.as_str()) {
        decide_active();
        return;
    }
    {
        let mut recent = RECENT.lock().unwrap_or_else(|e| e.into_inner());
        recent.retain(|e| *e != exe);
        recent.push_front(exe.clone());
        recent.truncate(10);
    }
    let (when, games) = WHEN.lock().unwrap_or_else(|e| e.into_inner()).clone();
    match when.as_str() {
        "elegidos" if games.contains(&exe) => watch(&exe, pid, false),
        "jugando" if crate::placement::fullscreen() && !BROWSERS.contains(&exe.as_str()) => watch(&exe, pid, true),
        "jugando" => {
            let _ = std::thread::Builder::new().name("microfono-mira".into()).spawn(move || {
                if is_game(&exe, &path) {
                    watch(&exe, pid, true);
                    decide_active();
                }
            });
        }
        _ => {}
    }
    decide_active();
}

#[implement(IAudioSessionNotification)]
struct Created(isize);

impl IAudioSessionNotification_Impl for Created_Impl {
    fn OnSessionCreated(&self, _session: windows_core::Ref<IAudioSessionControl>) -> windows_core::Result<()> {
        unsafe {
            let _ = SetEvent(HANDLE(self.0 as _));
        }
        Ok(())
    }
}

#[implement(IAudioSessionEvents)]
struct Changed(isize);

impl IAudioSessionEvents_Impl for Changed_Impl {
    fn OnDisplayNameChanged(&self, _: &windows_core::PCWSTR, _: *const windows_core::GUID) -> windows_core::Result<()> {
        Ok(())
    }
    fn OnIconPathChanged(&self, _: &windows_core::PCWSTR, _: *const windows_core::GUID) -> windows_core::Result<()> {
        Ok(())
    }
    fn OnSimpleVolumeChanged(&self, _: f32, _: windows_core::BOOL, _: *const windows_core::GUID) -> windows_core::Result<()> {
        Ok(())
    }
    fn OnChannelVolumeChanged(&self, _: u32, _: *const f32, _: u32, _: *const windows_core::GUID) -> windows_core::Result<()> {
        Ok(())
    }
    fn OnGroupingParamChanged(&self, _: *const windows_core::GUID, _: *const windows_core::GUID) -> windows_core::Result<()> {
        Ok(())
    }
    fn OnStateChanged(&self, _: AudioSessionState) -> windows_core::Result<()> {
        unsafe {
            let _ = SetEvent(HANDLE(self.0 as _));
        }
        Ok(())
    }
    fn OnSessionDisconnected(&self, _: AudioSessionDisconnectReason) -> windows_core::Result<()> {
        unsafe {
            let _ = SetEvent(HANDLE(self.0 as _));
        }
        Ok(())
    }
}

struct Ears {
    manager: IAudioSessionManager2,
    created: IAudioSessionNotification,
    changed: IAudioSessionEvents,
    watched: Vec<IAudioSessionControl>,
}

impl Ears {
    unsafe fn open(devices: &IMMDeviceEnumerator, id: &str, event: HANDLE) -> Option<Ears> {
        let device = devices.GetDevice(&HSTRING::from(endpoint(id))).ok()?;
        let manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None).ok()?;
        let created: IAudioSessionNotification = Created(event.0 as isize).into();
        let changed: IAudioSessionEvents = Changed(event.0 as isize).into();
        manager.RegisterSessionNotification(&created).ok()?;
        let mut ears = Ears { manager, created, changed, watched: Vec::new() };
        ears.rewatch();
        Some(ears)
    }

    unsafe fn rewatch(&mut self) {
        for session in self.watched.drain(..) {
            let _ = session.UnregisterAudioSessionNotification(&self.changed);
        }
        let Ok(all) = self.manager.GetSessionEnumerator() else { return };
        for index in 0..all.GetCount().unwrap_or(0) {
            if let Ok(session) = all.GetSession(index) {
                if session.RegisterAudioSessionNotification(&self.changed).is_ok() {
                    self.watched.push(session);
                }
            }
        }
    }

    unsafe fn listeners(&self) -> Vec<String> {
        let own = GetCurrentProcessId();
        let mut names = Vec::new();
        for session in &self.watched {
            if session.GetState().ok() != Some(AudioSessionStateActive) {
                continue;
            }
            let pid = session.cast::<IAudioSessionControl2>().ok().and_then(|s| s.GetProcessId().ok()).unwrap_or(0);
            if pid == own {
                continue;
            }
            let name = process_name(pid);
            if !names.contains(&name) {
                names.push(name);
            }
        }
        names
    }
}

impl Drop for Ears {
    fn drop(&mut self) {
        unsafe {
            for session in self.watched.drain(..) {
                let _ = session.UnregisterAudioSessionNotification(&self.changed);
            }
            let _ = self.manager.UnregisterSessionNotification(&self.created);
        }
    }
}

fn process_name(pid: u32) -> String {
    let path = unsafe {
        let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return "Una app".into() };
        let mut buffer = [0u16; 512];
        let mut len = buffer.len() as u32;
        let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, windows::core::PWSTR(buffer.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(process);
        if !ok {
            return "Una app".into();
        }
        String::from_utf16_lossy(&buffer[..len as usize])
    };
    friendly(&plain_exe(&path))
}

fn friendly(exe: &str) -> String {
    match exe {
        "discord" | "discordptb" | "discordcanary" => "Discord".into(),
        "zoom" => "Zoom".into(),
        "whatsapp" | "whatsapp.root" => "WhatsApp".into(),
        "robloxplayerbeta" => "Roblox".into(),
        "rocketleague" => "Rocket League".into(),
        "teams" | "ms-teams" => "Teams".into(),
        "obs64" | "obs32" => "OBS".into(),
        "chrome" => "Chrome".into(),
        "msedge" => "Edge".into(),
        "opera" => "Opera".into(),
        "firefox" => "Firefox".into(),
        "steam" | "steamwebhelper" => "Steam".into(),
        "telegram" => "Telegram".into(),
        "skype" => "Skype".into(),
        _ => {
            let mut chars = exe.chars();
            chars.next().map(|first| first.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_else(|| "Una app".into())
        }
    }
}

fn endpoint(id: &str) -> &str {
    id.strip_prefix("wasapi:").unwrap_or(id)
}

struct Stream {
    client: IAudioClient,
    event: HANDLE,
    size: u32,
}

impl Drop for Stream {
    fn drop(&mut self) {
        unsafe {
            let _ = self.client.Stop();
            let _ = CloseHandle(self.event);
        }
    }
}

unsafe fn open_stream(devices: &IMMDeviceEnumerator, id: &str, millis: i64) -> windows::core::Result<Stream> {
    let device = devices.GetDevice(&HSTRING::from(endpoint(id)))?;
    let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
    let format = WAVEFORMATEX { wFormatTag: 3, nChannels: 1, nSamplesPerSec: RATE, nAvgBytesPerSec: RATE * 4, nBlockAlign: 4, wBitsPerSample: 32, cbSize: 0 };
    let flags = AUDCLNT_STREAMFLAGS_EVENTCALLBACK | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;
    client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, millis * 10_000, 0, &format, None)?;
    let event = CreateEventW(None, false, false, None)?;
    client.SetEventHandle(event)?;
    let size = client.GetBufferSize()?;
    Ok(Stream { client, event, size })
}

unsafe fn drain(capture: &IAudioCaptureClient, into: &mut Vec<f32>) -> windows::core::Result<()> {
    loop {
        if capture.GetNextPacketSize()? == 0 {
            return Ok(());
        }
        let mut data = std::ptr::null_mut();
        let (mut frames, mut flags) = (0u32, 0u32);
        capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
        if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null() {
            into.resize(into.len() + frames as usize, 0.0);
        } else {
            into.extend_from_slice(std::slice::from_raw_parts(data as *const f32, frames as usize));
        }
        capture.ReleaseBuffer(frames)?;
    }
}

struct Drift {
    queue: VecDeque<f32>,
    pos: f64,
    target: usize,
}

impl Drift {
    fn take(&mut self, count: usize, out: &mut [f32]) -> usize {
        let excess = self.queue.len() as f64 - self.target as f64;
        let ratio = 1.0 + (excess / RATE as f64 * 0.5).clamp(-0.003, 0.003);
        let mut made = 0;
        while made < count && self.pos + 1.0 < self.queue.len() as f64 {
            let at = self.pos as usize;
            let frac = (self.pos - at as f64) as f32;
            out[made] = self.queue[at] * (1.0 - frac) + self.queue[at + 1] * frac;
            made += 1;
            self.pos += ratio;
        }
        let used = (self.pos as usize).min(self.queue.len());
        self.queue.drain(..used);
        self.pos -= used as f64;
        if self.queue.len() > self.target * 6 {
            let extra = self.queue.len() - self.target;
            self.queue.drain(..extra);
        }
        made
    }
}

fn db(rms: f32) -> f32 {
    if rms <= 1e-6 {
        -120.0
    } else {
        20.0 * rms.log10()
    }
}

unsafe fn run(stop: &AtomicBool) {
    let Ok(devices) = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL) else {
        status(|st| st.estado = "error".into());
        return;
    };
    let (Ok(wake_event), Ok(sessions)) = (CreateEventW(None, false, false, None), CreateEventW(None, false, false, None)) else { return };
    WAKE.store(wake_event.0 as isize, Ordering::Relaxed);
    let mut ears: Option<(String, Ears)> = None;
    let mut guard = Guard::new();
    while !stop.load(Ordering::Relaxed) {
        let Some(want) = WANT.lock().unwrap_or_else(|e| e.into_inner()).clone() else { break };
        let (inputs, outputs) = (list(true), list(false));
        let output = pick_output(&want.output, &outputs);
        let input = pick_input(&want.input, &want.mic, &inputs);
        let Some(output) = output else {
            ears = None;
            status(|st| *st = Estado { prendido: true, estado: "sin_cable".into(), entrada: input.map(|i| i.nombre).unwrap_or_default(), ..Default::default() });
            WaitForMultipleObjects(&[wake_event], false, 30_000);
            continue;
        };
        let Some(input) = input else {
            status(|st| *st = Estado { prendido: true, estado: "sin_micro".into(), cable: output.nombre.clone(), ..Default::default() });
            WaitForMultipleObjects(&[wake_event], false, 30_000);
            continue;
        };
        let listen = pair(&output, &inputs);
        let elige = listen.as_ref().map(|l| l.nombre.clone()).unwrap_or_else(|| "CABLE Output".into());
        if let Some(listen) = &listen {
            if ears.as_ref().map(|(id, _)| id != &listen.id).unwrap_or(true) {
                ears = Ears::open(&devices, &listen.id, sessions).map(|e| (listen.id.clone(), e));
            }
        } else {
            ears = None;
        }
        let heard = ears.as_ref().map(|(_, e)| e.listeners());
        if heard.as_ref().is_some_and(|h| h.is_empty()) {
            status(|st| *st = Estado { prendido: true, estado: "esperando".into(), cable: output.nombre.clone(), elige: elige.clone(), entrada: input.nombre.clone(), ..Default::default() });
            let woke = WaitForMultipleObjects(&[wake_event, sessions], false, 120_000);
            if woke.0 == WAIT_OBJECT_0.0 + 1 {
                if let Some((_, e)) = ears.as_mut() {
                    e.rewatch();
                }
            }
            continue;
        }
        scan_running();
        let ended = stream(&devices, stop, &want, &input, &output, &elige, ears.as_mut().map(|(_, e)| e), wake_event, sessions, &mut guard);
        if let Err(problem) = ended {
            status(|st| {
                st.estado = "error".into();
                st.limpiando = false;
                st.error = problem;
            });
            WaitForMultipleObjects(&[wake_event], false, 5_000);
        }
    }
    drop(ears);
    WAKE.store(0, Ordering::Relaxed);
    let _ = CloseHandle(wake_event);
    let _ = CloseHandle(sessions);
}

#[allow(clippy::too_many_arguments)]
unsafe fn stream(devices: &IMMDeviceEnumerator, stop: &AtomicBool, want: &Want, input: &Aparato, output: &Aparato, elige: &str, mut ears: Option<&mut Ears>, wake_event: HANDLE, sessions: HANDLE, guard: &mut Guard) -> Result<(), String> {
    let mic = open_stream(devices, &input.id, 20).map_err(|e| format!("No pude abrir {}: {}", input.nombre, e.message()))?;
    let out = open_stream(devices, &output.id, 40).map_err(|e| format!("No pude abrir {}: {}", output.nombre, e.message()))?;
    let capture: IAudioCaptureClient = mic.client.GetService().map_err(|e| e.message())?;
    let render: IAudioRenderClient = out.client.GetService().map_err(|e| e.message())?;
    let _boost = Boost::new();
    guard.restart();
    let mut chain = Chain::new(&guard.shape(&want.tune));
    let mut tune = want.tune.clone();
    let mut lighter: Option<Receiver<Profundo>> = None;
    let mut pending: Vec<f32> = Vec::with_capacity(HOP * 8);
    let mut drift = Drift { queue: VecDeque::from(vec![0.0; QUEUE]), pos: 0.0, target: QUEUE };
    let opened = Instant::now();
    let (mut queued, mut buffered, mut turns) = (0usize, 0usize, 0usize);
    let mut frame = [0.0f32; HOP];
    let mut chunk = vec![0.0f32; out.size as usize];
    mic.client.Start().map_err(|e| e.message())?;
    out.client.Start().map_err(|e| e.message())?;
    let mut quiet_since: Option<Instant> = None;
    let mut heard = ears.as_ref().map(|e| e.listeners()).unwrap_or_default();
    let (mut spent, mut frames, mut energy_in, mut energy_out, mut talk) = (Duration::ZERO, 0u32, 0f32, 0f32, 0u32);
    let mut reported = Instant::now();
    let mut active = ACTIVE.load(Ordering::Relaxed);
    let mut clean = active && !guard.raw(Instant::now());
    let mut fade = 0usize;
    loop {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        let woke = WaitForMultipleObjects(&[mic.event, out.event, wake_event, sessions], false, 200).0.wrapping_sub(WAIT_OBJECT_0.0);
        if woke == 2 {
            let now = WANT.lock().unwrap_or_else(|e| e.into_inner()).clone();
            match now {
                None => return Ok(()),
                Some(now) if now.input != want.input || now.output != want.output || now.mic != want.mic => return Ok(()),
                Some(now) if now.tune != tune => {
                    if now.tune.light != tune.light {
                        *guard = Guard::new();
                        lighter = None;
                    }
                    chain.retune(&guard.shape(&now.tune));
                    tune = now.tune;
                }
                _ => {}
            }
            active = ACTIVE.load(Ordering::Relaxed);
        }
        if let Some(engine) = lighter.as_ref().and_then(|ready| ready.try_recv().ok()) {
            chain.adopt(engine);
            lighter = None;
            guard.restart();
            fade = HOP * 2;
        }
        if woke == 3 {
            if let Some(e) = ears.as_deref_mut() {
                e.rewatch();
                heard = e.listeners();
            }
        }
        if ears.is_some() && heard.is_empty() {
            let since = *quiet_since.get_or_insert_with(Instant::now);
            if since.elapsed() > Duration::from_secs(5) {
                return Ok(());
            }
        } else {
            quiet_since = None;
        }
        drain(&capture, &mut pending).map_err(|e| format!("El micrófono se desconectó: {}", e.message()))?;
        let mut at = 0;
        while pending.len() - at >= HOP {
            let piece = &pending[at..at + HOP];
            let started = Instant::now();
            let now_clean = active && !guard.raw(started);
            if now_clean != clean {
                clean = now_clean;
                fade = HOP * 2;
            }
            if clean {
                chain.frame(piece, &mut frame);
            } else {
                chain.pass(piece, &mut frame);
            }
            let took = started.elapsed();
            spent += took;
            frames += 1;
            if clean && lighter.is_none() && guard.note(took, chain.tune.light, started) == Some(Pace::Lighten) {
                let (send, ready) = channel();
                lighter = Some(ready);
                let _ = std::thread::Builder::new().name("microfono-ligero".into()).spawn(move || {
                    let _ = send.send(Profundo::with(Level::Normal, false));
                });
            }
            energy_in += piece.iter().map(|v| v * v).sum::<f32>();
            energy_out += frame.iter().map(|v| v * v).sum::<f32>();
            if chain.lsnr > 3.0 {
                talk += 1;
            }
            if fade > 0 {
                for v in frame.iter_mut() {
                    *v *= 1.0 - fade as f32 / (HOP * 2) as f32;
                    fade = fade.saturating_sub(1);
                }
            }
            drift.queue.extend(frame.iter());
            at += HOP;
        }
        pending.drain(..at);
        let padding = out.client.GetCurrentPadding().map_err(|e| format!("El cable se desconectó: {}", e.message()))?;
        let room = out.size.saturating_sub(padding).min(AT_MOST.saturating_sub(padding));
        queued += drift.queue.len();
        buffered += padding as usize;
        turns += 1;
        if room > 0 {
            let made = drift.take(room as usize, &mut chunk);
            let starving = made == 0 && padding < HOP as u32 / 2;
            if starving && opened.elapsed() > Duration::from_secs(1) {
                drift.target = (drift.target + HOP / 2).min(HOP * 6);
            }
            let count = if starving { (HOP / 2).min(room as usize) } else { made };
            if count > 0 {
                let buffer = render.GetBuffer(count as u32).map_err(|e| e.message())?;
                let target = std::slice::from_raw_parts_mut(buffer as *mut f32, count);
                if made == 0 {
                    target.fill(0.0);
                } else {
                    target.copy_from_slice(&chunk[..count]);
                }
                render.ReleaseBuffer(count as u32, 0).map_err(|e| e.message())?;
            }
        }
        if reported.elapsed() > Duration::from_millis(250) && frames > 0 {
            let seconds = frames as f32 * HOP as f32 / RATE as f32;
            let samples = (frames as usize * HOP) as f32;
            let snapshot = Estado {
                prendido: true,
                estado: if !active {
                    "pasando".into()
                } else if clean {
                    "limpiando".into()
                } else {
                    "no_da".into()
                },
                cable: output.nombre.clone(),
                elige: elige.to_string(),
                entrada: input.nombre.clone(),
                escuchan: heard.clone(),
                limpiando: clean,
                cpu: spent.as_secs_f32() / seconds * 100.0,
                voz: talk * 3 > frames,
                entra: db((energy_in / samples).sqrt()),
                sale: db((energy_out / samples).sqrt()),
                latencia: ((if clean { DELAY } else { 0 } + HOP + (queued + buffered) / turns.max(1)) as f32 / RATE as f32 * 1000.0) as u32,
                error: String::new(),
                bajo: guard.lowered && !tune.light,
                ahora: Vec::new(),
            };
            status(|st| *st = snapshot);
            reported = Instant::now();
            (spent, frames, energy_in, energy_out, talk) = (Duration::ZERO, 0, 0.0, 0.0, 0);
            (queued, buffered, turns) = (0, 0, 0);
        }
    }
}

pub struct Prueba {
    crudo: Vec<f32>,
    limpio: Vec<f32>,
}

static PRUEBA: Mutex<Option<Prueba>> = Mutex::new(None);

#[derive(Serialize)]
pub struct Resumen {
    segundos: f32,
    ruido_antes: f32,
    ruido_despues: f32,
    voz_antes: f32,
    voz_despues: f32,
    cpu: f32,
    picos_crudo: Vec<f32>,
    picos_limpio: Vec<f32>,
}

fn clean_all(raw: &[f32], tune: &Tune) -> (Vec<f32>, f32) {
    let mut chain = Chain::new(tune);
    let mut out = vec![0.0f32; raw.len() + DELAY + HOP];
    let mut padded = raw.to_vec();
    padded.resize(raw.len() + DELAY + HOP, 0.0);
    let started = Instant::now();
    let mut frames = 0;
    for (input, output) in padded.chunks_exact(HOP).zip(out.chunks_exact_mut(HOP)) {
        chain.frame(input, output);
        frames += 1;
    }
    let cpu = started.elapsed().as_secs_f32() / (frames as f32 * HOP as f32 / RATE as f32) * 100.0;
    out.drain(..DELAY);
    out.truncate(raw.len());
    (out, cpu)
}

fn peaks(samples: &[f32], count: usize) -> Vec<f32> {
    let size = (samples.len() / count).max(1);
    samples.chunks(size).take(count).map(|c| c.iter().fold(0.0f32, |m, v| m.max(v.abs()))).collect()
}

fn floor_and_voice(samples: &[f32]) -> (f32, f32) {
    let mut levels: Vec<f32> = samples.chunks_exact(HOP).map(|c| (c.iter().map(|v| v * v).sum::<f32>() / HOP as f32).sqrt()).collect();
    if levels.is_empty() {
        return (-120.0, -120.0);
    }
    levels.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let low = levels[levels.len() / 10];
    let high = levels[levels.len() * 9 / 10];
    (db(low), db(high))
}

fn summary(prueba: &Prueba, cpu: f32) -> Resumen {
    let (ruido_antes, voz_antes) = floor_and_voice(&prueba.crudo);
    let (ruido_despues, voz_despues) = floor_and_voice(&prueba.limpio);
    Resumen {
        segundos: prueba.crudo.len() as f32 / RATE as f32,
        ruido_antes,
        ruido_despues,
        voz_antes,
        voz_despues,
        cpu,
        picos_crudo: peaks(&prueba.crudo, 160),
        picos_limpio: peaks(&prueba.limpio, 160),
    }
}

fn record(input: &Aparato, seconds: f32) -> Result<Vec<f32>, String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|e| e.message())?;
        let mic = open_stream(&devices, &input.id, 20).map_err(|e| format!("No pude abrir {}: {}", input.nombre, e.message()))?;
        let capture: IAudioCaptureClient = mic.client.GetService().map_err(|e| e.message())?;
        let wanted = (seconds * RATE as f32) as usize;
        let mut samples = Vec::with_capacity(wanted + RATE as usize);
        mic.client.Start().map_err(|e| e.message())?;
        let until = Instant::now() + Duration::from_secs_f32(seconds + 2.0);
        while samples.len() < wanted && Instant::now() < until {
            WaitForSingleObject(mic.event, 200);
            drain(&capture, &mut samples).map_err(|e| e.message())?;
        }
        samples.truncate(wanted);
        Ok(samples)
    }
}

#[tauri::command]
pub async fn limpio_grabar(app: AppHandle, segundos: f32) -> Result<Resumen, String> {
    let s = crate::settings::load(&app);
    let tune = Tune::of(&s);
    tauri::async_runtime::spawn_blocking(move || {
        let input = pick_input(&s.clean_mic_in, &s.mic, &list(true)).ok_or("No encontré ningún micrófono.")?;
        let crudo = record(&input, segundos.clamp(2.0, 15.0))?;
        if crudo.len() < HOP * 20 {
            return Err("El micrófono no mandó nada. Revisa que no esté silenciado.".into());
        }
        let (limpio, cpu) = clean_all(&crudo, &tune);
        let prueba = Prueba { crudo, limpio };
        let resumen = summary(&prueba, cpu);
        *PRUEBA.lock().unwrap_or_else(|e| e.into_inner()) = Some(prueba);
        Ok(resumen)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn limpio_reprocesar(app: AppHandle) -> Result<Resumen, String> {
    let tune = Tune::of(&crate::settings::load(&app));
    tauri::async_runtime::spawn_blocking(move || {
        let raw = PRUEBA.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|p| p.crudo.clone()).ok_or("Primero grábate.")?;
        let (limpio, cpu) = clean_all(&raw, &tune);
        let prueba = Prueba { crudo: raw, limpio };
        let resumen = summary(&prueba, cpu);
        *PRUEBA.lock().unwrap_or_else(|e| e.into_inner()) = Some(prueba);
        Ok(resumen)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn limpio_audio(cual: String) -> tauri::ipc::Response {
    let guard = PRUEBA.lock().unwrap_or_else(|e| e.into_inner());
    let bytes = guard.as_ref().map(|p| if cual == "limpio" { &p.limpio } else { &p.crudo }).map(|samples| samples.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>()).unwrap_or_default();
    tauri::ipc::Response::new(bytes)
}

#[tauri::command]
pub fn limpio_estado() -> Estado {
    let mut estado = STATUS.lock().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_else(|| Estado { estado: "apagado".into(), ..Default::default() });
    let known = INSTALLED.lock().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_default();
    estado.ahora = triggered_now().into_iter().map(|exe| known.iter().find(|(name, _)| *name == exe).map(|(_, nombre)| nombre.clone()).unwrap_or_else(|| name_of(&exe, None))).collect();
    estado
}

fn fetch_cable() -> Result<PathBuf, String> {
    let output = Command::new("curl.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["--silent", "--location", "--fail", "--proto", "=https", "--max-time", "180", CABLE_ZIP])
        .output()
        .map_err(|_| "No pude abrir curl.exe.".to_string())?;
    if !output.status.success() || output.stdout.is_empty() {
        return Err("No pude bajar VB-Cable. Revisa tu internet o bájalo de su página.".into());
    }
    let hash: String = crate::candado::sha256(&output.stdout).iter().map(|b| format!("{b:02x}")).collect();
    if hash != CABLE_SHA {
        return Err("VB-Audio cambió el paquete y no lo pude comprobar. Bájalo de su página.".into());
    }
    let dir = std::env::temp_dir().join("Cocuyo-VBCable");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for (name, raw) in crate::bandeja::zip_files(&output.stdout, |name| !name.contains(['/', '\\', ':']) && !name.contains(".."), 8 << 20) {
        std::fs::write(dir.join(name), raw).map_err(|e| e.to_string())?;
    }
    let setup = dir.join(CABLE_SETUP);
    if setup.is_file() {
        Ok(setup)
    } else {
        Err("El paquete de VB-Cable vino incompleto. Bájalo de su página.".into())
    }
}

#[tauri::command]
pub async fn limpio_instalar_cable() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| {
        let setup = fetch_cable()?;
        let folder = setup.parent().map(|dir| HSTRING::from(dir.as_os_str())).unwrap_or_default();
        let result = unsafe { ShellExecuteW(None, w!("runas"), &HSTRING::from(setup.as_os_str()), None, &folder, SW_SHOWNORMAL) };
        if result.0 as isize > 32 {
            Ok(())
        } else {
            Err("Windows no dio el permiso para abrir el instalador.".to_string())
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
pub struct Aparatos {
    entradas: Vec<Aparato>,
    salidas: Vec<Aparato>,
}

#[tauri::command]
pub async fn limpio_aparatos() -> Aparatos {
    tauri::async_runtime::spawn_blocking(|| Aparatos { entradas: list(true).into_iter().filter(|a| !a.cable).collect(), salidas: list(false).into_iter().filter(|a| a.cable).collect() }).await.unwrap_or(Aparatos { entradas: Vec::new(), salidas: Vec::new() })
}

#[derive(Serialize, Clone)]
pub struct Juego {
    exe: String,
    nombre: String,
    grupo: &'static str,
}

#[derive(Serialize, Default)]
pub struct Lista {
    lista: Vec<Juego>,
    cargando: bool,
}

#[tauri::command]
pub async fn limpio_juegos(app: AppHandle) -> Lista {
    let chosen = crate::settings::load(&app).clean_mic_games;
    tauri::async_runtime::spawn_blocking(move || {
        let (rest, cargando) = everything(&app);
        let label = |exe: &str, path: Option<&str>| rest.iter().find(|game| game.exe == exe).map(|game| game.nombre.clone()).unwrap_or_else(|| name_of(exe, path));
        let mut seen = HashSet::new();
        let mut lista = Vec::new();
        for (exe, _, path) in open_apps() {
            if seen.insert(exe.clone()) {
                lista.push(Juego { nombre: label(&exe, Some(&path)), exe, grupo: "abierta" });
            }
        }
        for game in &rest {
            if seen.insert(game.exe.clone()) {
                lista.push(game.clone());
            }
        }
        for exe in chosen.iter().map(|exe| plain_exe(exe)).chain(RECENT.lock().unwrap_or_else(|e| e.into_inner()).iter().cloned()) {
            if seen.insert(exe.clone()) {
                lista.push(Juego { nombre: label(&exe, None), exe, grupo: "otra" });
            }
        }
        Lista { lista, cargando }
    })
    .await
    .unwrap_or_default()
}

#[tauri::command]
pub async fn limpio_buscar_programa() -> Option<String> {
    tauri::async_runtime::spawn_blocking(|| {
        let path = crate::dialogs::pick_exe()?;
        let text = path.to_string_lossy().to_string();
        let exe = plain_exe(&text);
        name_of(&exe, Some(&text));
        (!exe.is_empty()).then_some(exe)
    })
    .await
    .ok()
    .flatten()
}

fn has(text: &str, words: &[&str]) -> bool {
    words.iter().any(|w| text.contains(w))
}

pub fn spoken(app: &AppHandle, spoken: &str) -> Option<Result<String, String>> {
    let text = spoken.to_lowercase();
    let about = has(&text, &["ruido", "micrófono limpio", "microfono limpio", "micro limpio", "noise"]);
    if !about || !has(&text, &["micr", "mic ", "voz", "noise", "fondo"]) {
        return None;
    }
    let off = has(&text, &["apaga", "quita el micr", "desactiva", "ya no", "deja de", "turn off"]);
    let mut change = serde_json::Map::new();
    let said = if off {
        change.insert("cleanMic".into(), false.into());
        "Listo, tu micrófono sale tal cual."
    } else {
        change.insert("cleanMic".into(), true.into());
        let mode = if has(&text, &["natural", "suave"]) {
            Some("natural")
        } else if has(&text, &["a tope", "fuerte", "máximo", "maximo", "todo el ruido"]) {
            Some("tope")
        } else if has(&text, &["llamada", "normal"]) {
            Some("llamada")
        } else {
            None
        };
        if let Some(mode) = mode {
            change.insert("cleanMicMode".into(), mode.into());
        }
        match mode {
            Some("natural") => "Listo: le quito el ruido a tu micrófono y dejo un poco de ambiente.",
            Some("tope") => "Listo: le quito todo el ruido a tu micrófono, a tope.",
            _ => "Listo: le quito el ruido a tu micrófono. En la app de la llamada elige «CABLE Output».",
        }
    };
    let mut current = serde_json::to_value(crate::settings::load(app)).ok()?;
    for (key, value) in change {
        current[key] = value;
    }
    let updated: Settings = serde_json::from_value(current).ok()?;
    Some(crate::settings::set_settings(app.clone(), updated).map(|_| said.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aparato(nombre: &str) -> Aparato {
        Aparato { id: format!("wasapi:{nombre}"), nombre: nombre.into(), cable: is_cable(nombre), windows: false }
    }

    #[test]
    fn finds_the_cable_and_its_other_end() {
        let outputs = [aparato("Altavoces (Realtek(R) Audio)"), aparato("CABLE Input (VB-Audio Virtual Cable)")];
        let inputs = [aparato("Micrófono (Fifine Microphone)"), aparato("CABLE Output (VB-Audio Virtual Cable)")];
        let out = pick_output("", &outputs).unwrap();
        assert_eq!(out.nombre, "CABLE Input (VB-Audio Virtual Cable)");
        assert_eq!(pair(&out, &inputs).unwrap().nombre, "CABLE Output (VB-Audio Virtual Cable)");
        assert_eq!(pick_input("", "", &inputs).unwrap().nombre, "Micrófono (Fifine Microphone)");
        assert_eq!(pick_input("wasapi:CABLE Output (VB-Audio Virtual Cable)", "", &inputs).unwrap().nombre, "Micrófono (Fifine Microphone)");
        assert!(pick_output("", &[aparato("Altavoces (Realtek(R) Audio)")]).is_none());
    }

    #[test]
    fn the_modes_mean_something() {
        let mut s = Settings::default();
        s.clean_mic_mode = "natural".into();
        assert_eq!(Tune::of(&s).limit_db, 18.0);
        s.clean_mic_mode = "tope".into();
        let tope = Tune::of(&s);
        assert!(tope.between && tope.gate > 0 && tope.limit_db >= 100.0);
        s.clean_mic_mode = "propio".into();
        s.clean_mic_amount = 50;
        assert!((Tune::of(&s).limit_db - 22.5).abs() < 0.01);
    }

    #[test]
    fn the_chain_keeps_the_length_and_never_clips() {
        let mut s = Settings::default();
        s.clean_mic_gain = 18;
        s.clean_mic_level = true;
        let tune = Tune::of(&s);
        let raw: Vec<f32> = (0..RATE as usize).map(|i| (i as f32 * 0.03).sin() * 0.9).collect();
        let (clean, _) = clean_all(&raw, &tune);
        assert_eq!(clean.len(), raw.len());
        assert!(clean.iter().all(|v| v.abs() <= 1.0), "{}", clean.iter().fold(0.0f32, |m, v| m.max(v.abs())));
    }

    #[test]
    fn drift_keeps_the_queue_near_its_target() {
        let mut drift = Drift { queue: VecDeque::from(vec![0.1; QUEUE * 3]), pos: 0.0, target: QUEUE };
        let mut out = vec![0.0; 480];
        for _ in 0..2000 {
            drift.queue.extend(std::iter::repeat_n(0.1, 480));
            drift.take(480, &mut out);
        }
        assert!(drift.queue.len() < QUEUE + QUEUE / 2, "{}", drift.queue.len());
        drift.queue.extend(std::iter::repeat_n(0.1, QUEUE * 8));
        drift.take(480, &mut out);
        assert!(drift.queue.len() <= QUEUE, "lo que sobra de golpe se tira: {}", drift.queue.len());
    }

    fn feed(guard: &mut Guard, millis: f32, light: bool, frames: usize, now: Instant) -> Vec<Pace> {
        (0..frames).filter_map(|_| guard.note(Duration::from_secs_f32(millis / 1000.0), light, now)).collect()
    }

    #[test]
    fn a_fast_pc_never_gets_touched() {
        let mut guard = Guard::new();
        let now = Instant::now();
        assert!(feed(&mut guard, 0.5, false, 20_000, now).is_empty());
        assert!(feed(&mut guard, 5.9, false, 20_000, now).is_empty());
        assert!(!guard.lowered && guard.raw_until.is_none());
    }

    #[test]
    fn a_hiccup_is_not_a_slow_pc() {
        let mut guard = Guard::new();
        let now = Instant::now();
        feed(&mut guard, 0.5, false, 10, now);
        assert!(feed(&mut guard, 9.0, false, 25, now).is_empty());
        assert!(feed(&mut guard, 0.5, false, 25, now).is_empty());
        assert!(feed(&mut guard, 9.0, false, 25, now).is_empty());
        assert!(!guard.lowered);
    }

    #[test]
    fn a_slow_pc_goes_light_and_then_raw() {
        let mut guard = Guard::new();
        let now = Instant::now();
        feed(&mut guard, 7.0, false, 10, now);
        assert_eq!(feed(&mut guard, 7.0, false, 50, now), vec![Pace::Lighten]);
        assert!(guard.lowered);
        assert!(guard.shape(&Tune::of(&Settings::default())).light);
        guard.restart();
        feed(&mut guard, 7.0, true, 10, now);
        assert!(feed(&mut guard, 7.0, true, 2_000, now).is_empty(), "ligera a 7 ms todavía llega");
        assert_eq!(feed(&mut guard, 9.0, true, 50, now), vec![Pace::Raw]);
        assert!(guard.raw(now));
        assert!(guard.raw(now + FIRST_WAIT - Duration::from_secs(1)));
    }

    #[test]
    fn it_tries_again_later_and_waits_longer_each_time() {
        let mut guard = Guard::new();
        let now = Instant::now();
        feed(&mut guard, 9.0, true, 10, now);
        assert_eq!(feed(&mut guard, 9.0, true, 50, now), vec![Pace::Raw]);
        let later = now + FIRST_WAIT + Duration::from_secs(1);
        assert!(!guard.raw(later));
        feed(&mut guard, 9.0, true, 10, later);
        assert_eq!(feed(&mut guard, 9.0, true, 25, later), vec![Pace::Raw]);
        assert!(guard.raw(later + FIRST_WAIT + Duration::from_secs(20)));
        assert!(!guard.raw(later + FIRST_WAIT * 2 + Duration::from_secs(1)));
        feed(&mut guard, 1.0, true, 10, later);
        assert!(feed(&mut guard, 1.0, true, 25, later).is_empty());
        assert_eq!(guard.wait, FIRST_WAIT);
    }

    #[test]
    fn very_slow_frames_do_not_make_you_wait_25_of_them() {
        let mut guard = Guard::new();
        let now = Instant::now();
        feed(&mut guard, 40.0, false, 10, now);
        assert_eq!(feed(&mut guard, 40.0, false, 14, now), vec![Pace::Lighten]);
    }

    #[test]
    fn silence_does_not_hide_a_slow_pc() {
        let mut guard = Guard::new();
        let now = Instant::now();
        feed(&mut guard, 7.0, false, 10, now);
        for _ in 0..50 {
            guard.note(Duration::from_micros(2), false, now);
            guard.note(Duration::from_millis(7), false, now);
        }
        assert!(guard.lowered);
    }

    #[test]
    #[ignore]
    fn the_cable_package_is_the_one_we_checked() {
        let setup = fetch_cable().unwrap();
        assert!(setup.ends_with(CABLE_SETUP));
        let dir = setup.parent().unwrap();
        for file in ["vbMmeCable64_win10.inf", "vbaudio_cable64_win10.sys", "vbaudio_cable64_win10.cat", "readme.txt"] {
            assert!(dir.join(file).is_file(), "{file}");
        }
        assert_eq!(std::fs::read_dir(dir).unwrap().count(), 31);
    }

    #[test]
    fn each_mode_listens_to_its_own_list() {
        let watched = vec![("discord".to_string(), false), ("gta_sa".to_string(), true)];
        assert_eq!(triggered("jugando", &[], &watched), vec!["gta_sa"]);
        assert_eq!(triggered("elegidos", &["discord".to_string()], &watched), vec!["discord"]);
        assert!(triggered("elegidos", &[], &watched).is_empty());
        assert!(triggered("siempre", &[], &watched).is_empty());
    }

    #[test]
    fn sees_what_is_running_right_now() {
        let own = plain_exe(&std::env::current_exe().unwrap().to_string_lossy());
        assert!(processes().iter().any(|(exe, pid)| *exe == own && *pid == std::process::id()));
    }

    #[test]
    #[ignore]
    fn lists_the_games_on_this_pc() {
        for (name, exe) in crate::apps::installed_games() {
            println!("[juego] {name} -> {exe}");
        }
        for path in mic_users() {
            let exe = plain_exe(&path);
            println!("[micro] {} {exe} = {} -> {path}", if skip_app(&exe) { "fuera" } else if is_game(&exe, &path) { "juego" } else { "voz" }, name_of(&exe, Some(&path)));
        }
    }

    #[test]
    fn knows_what_a_program_is_from_its_id() {
        assert_eq!(program_of("C:\\Games\\bloodstrike\\launcher.exe").as_deref(), Some("launcher"));
        assert_eq!(program_of("{6D809377-6AF0-444B-8957-A3773F02200E}\\Zoom\\Zoom.exe").as_deref(), Some("zoom"));
        assert_eq!(program_of("{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\\notepad.exe"), None);
        assert_eq!(program_of("5319275A.WhatsAppDesktop_cv1g1gvanyjgm!App").as_deref(), Some("whatsapp.root"));
        assert_eq!(program_of("Microsoft.AutoGenerated.{28E39414}"), None);
        assert_eq!(program_of("C:\\Roblox\\RobloxPlayerLauncher.exe").as_deref(), Some("robloxplayerbeta"));
        assert_eq!(same_name("Blood Strike"), same_name("bloodstrike"));
        assert_eq!(capitalized("bloodstrike"), "Bloodstrike");
        assert_eq!(capitalized("F1 22"), "F1 22");
        assert!(skip_app("cocuyo_lib-58cf31dac383bea8") && skip_app("steamwebhelper") && skip_app("unins000"));
        assert!(!skip_app("discord") && !skip_app("robloxplayerbeta"));
    }

    #[test]
    fn understands_what_you_say() {
        assert!(has(&"quítale el ruido a mi micrófono".to_lowercase(), &["ruido"]));
        assert_eq!(plain_exe("C:\\Games\\RocketLeague.exe"), "rocketleague");
        assert_eq!(friendly("discord"), "Discord");
    }
}
