// fondos animados. antes era un navegador entero para un video, una locura; ahora decodifica la grafica y hay una ventana por monitor

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use windows::core::{implement, w, Interface, BOOL, BSTR, GUID, HSTRING};
use windows::Win32::Foundation::{HANDLE, HMODULE, HWND, LPARAM, LRESULT, POINT, RECT, S_OK, WPARAM};
use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, ScreenToClient, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Multithread, ID3D11Texture2D, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
    D3D11_CREATE_DEVICE_VIDEO_SUPPORT, D3D11_SDK_VERSION,
};
use windows::Win32::Graphics::Dwm::DwmFlush;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_ALPHA_MODE_IGNORE, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::{
    IDXGIDevice, IDXGIFactory2, IDXGISwapChain1, DXGI_PRESENT, DXGI_SCALING_STRETCH, DXGI_SWAP_CHAIN_DESC1,
    DXGI_SWAP_EFFECT_FLIP_DISCARD, DXGI_USAGE_RENDER_TARGET_OUTPUT,
};
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Power::{RegisterPowerSettingNotification, POWERBROADCAST_SETTING};
use windows::Win32::System::RemoteDesktop::WTSRegisterSessionNotification;
use windows::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetSystemMetrics, PeekMessageW, PostMessageW, RegisterClassW,
    SetParent, SetWindowLongPtrW, SetWindowPos, WaitMessage, DEVICE_NOTIFY_WINDOW_HANDLE, GWL_STYLE, MSG, PM_REMOVE, SM_CXSCREEN,
    SM_CYSCREEN, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_SHOWWINDOW, WINDOW_EX_STYLE, WM_DESTROY, WM_ERASEBKGND, WM_NULL,
    WM_POWERBROADCAST, WM_WTSSESSION_CHANGE, WNDCLASSW, WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_POPUP, WS_VISIBLE,
};

const GUID_CONSOLE_DISPLAY_STATE: GUID = GUID::from_u128(0x6fe69556_704a_47a0_8f24_c28d936fda47);
const PBT_POWERSETTINGCHANGE: usize = 0x8013;
const WTS_SESSION_LOCK: usize = 7;
const WTS_SESSION_UNLOCK: usize = 8;
const MAX_MISSES: u32 = 180;
const LOOK_EVERY: Duration = Duration::from_secs(2);

type OnFail = Arc<dyn Fn(PathBuf, String) + Send + Sync>;

static SOURCE: Mutex<Vec<(String, PathBuf)>> = Mutex::new(Vec::new());
static ON_FAIL: Mutex<Option<OnFail>> = Mutex::new(None);
static THREAD: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);
static HOST: AtomicIsize = AtomicIsize::new(0);
static CHANGED: AtomicBool = AtomicBool::new(false);
static QUIT: AtomicBool = AtomicBool::new(false);
static PAUSE: AtomicBool = AtomicBool::new(false);
static HIDDEN: AtomicU32 = AtomicU32::new(0);
static CLOSING: AtomicBool = AtomicBool::new(false);
static LOCKED: AtomicBool = AtomicBool::new(false);
static DARK: AtomicBool = AtomicBool::new(false);
static GONE: AtomicBool = AtomicBool::new(false);
pub static FRAMES: AtomicU32 = AtomicU32::new(0);

enum End {
    Unsupported(String),
    Lost(String),
    Moved,
}

impl From<windows::core::Error> for End {
    fn from(e: windows::core::Error) -> Self {
        End::Unsupported(e.to_string())
    }
}

fn wake() {
    let host = HOST.load(Ordering::Relaxed);
    if host != 0 {
        unsafe {
            let _ = PostMessageW(Some(HWND(host as _)), WM_NULL, WPARAM(0), LPARAM(0));
        }
    }
}

pub struct Screen {
    pub id: String,
    pub rect: RECT,
    pub main: bool,
}

impl Screen {
    pub fn number(&self) -> u32 {
        self.id.chars().filter(char::is_ascii_digit).collect::<String>().parse().unwrap_or(0)
    }

    pub fn size(&self) -> (u32, u32) {
        ((self.rect.right - self.rect.left).max(1) as u32, (self.rect.bottom - self.rect.top).max(1) as u32)
    }
}

unsafe extern "system" fn add_monitor(monitor: HMONITOR, _dc: HDC, _rect: *mut RECT, list: LPARAM) -> BOOL {
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    if GetMonitorInfoW(monitor, &mut info as *mut MONITORINFOEXW as *mut MONITORINFO).as_bool() {
        let name = info.szDevice;
        let len = name.iter().position(|c| *c == 0).unwrap_or(name.len());
        let found = Screen { id: String::from_utf16_lossy(&name[..len]), rect: info.monitorInfo.rcMonitor, main: info.monitorInfo.dwFlags & 1 != 0 };
        (*(list.0 as *mut Vec<Screen>)).push(found);
    }
    true.into()
}

pub fn screens() -> Vec<Screen> {
    let mut found: Vec<Screen> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(add_monitor), LPARAM(&mut found as *mut Vec<Screen> as isize));
        if found.is_empty() {
            let rect = RECT { left: 0, top: 0, right: GetSystemMetrics(SM_CXSCREEN), bottom: GetSystemMetrics(SM_CYSCREEN) };
            found.push(Screen { id: "\\\\.\\DISPLAY1".into(), rect, main: true });
        }
    }
    found.sort_by_key(|screen| (screen.number(), screen.id.clone()));
    found
}

pub fn hides(monitor: &RECT, window: &RECT, zoomed: bool) -> bool {
    let whole = window.left <= monitor.left && window.top <= monitor.top && window.right >= monitor.right && window.bottom >= monitor.bottom;
    let (x, y) = ((window.left + window.right) / 2, (window.top + window.bottom) / 2);
    whole || (zoomed && x >= monitor.left && x < monitor.right && y >= monitor.top && y < monitor.bottom)
}

fn groups(plan: &[(String, PathBuf)], ids: &[&str]) -> Vec<(PathBuf, Vec<usize>)> {
    let mut found: Vec<(PathBuf, Vec<usize>)> = Vec::new();
    for (index, id) in ids.iter().enumerate() {
        let Some((_, path)) = plan.iter().find(|(wanted, _)| wanted == id) else { continue };
        match found.iter_mut().find(|(known, _)| known == path) {
            Some((_, at)) => at.push(index),
            None => found.push((path.clone(), vec![index])),
        }
    }
    found
}

pub fn play(plan: Vec<(String, PathBuf)>, on_fail: impl Fn(PathBuf, String) + Send + Sync + 'static) {
    *ON_FAIL.lock().unwrap() = Some(Arc::new(on_fail));
    *SOURCE.lock().unwrap() = plan;
    CHANGED.store(true, Ordering::Relaxed);

    let mut thread = THREAD.lock().unwrap();
    if thread.as_ref().is_some_and(|t| !t.is_finished()) && !QUIT.load(Ordering::Relaxed) {
        wake();
        return;
    }
    if let Some(old) = thread.take() {
        let _ = old.join();
    }
    QUIT.store(false, Ordering::Relaxed);
    *thread = std::thread::Builder::new().name("fondo".into()).spawn(run).ok();
}

pub fn pause(on: bool) {
    if PAUSE.swap(on, Ordering::Relaxed) != on {
        wake();
    }
}

pub fn hide(mask: u32) {
    if HIDDEN.swap(mask, Ordering::Relaxed) != mask {
        wake();
    }
}

pub fn stop() {
    QUIT.store(true, Ordering::Relaxed);
    wake();
    if let Some(thread) = THREAD.lock().unwrap().take() {
        let _ = thread.join();
    }
    SOURCE.lock().unwrap().clear();
}

fn failed(path: PathBuf, why: String) {
    let on_fail = ON_FAIL.lock().unwrap().clone();
    if let Some(on_fail) = on_fail {
        on_fail(path, why);
    }
}

fn run() {
    let mut losses = 0;
    let failure = unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let failure = match MFStartup(MF_VERSION, MFSTARTUP_LITE) {
            Err(e) => Some(e.to_string()),
            Ok(()) => loop {
                match session() {
                    Ok(()) => break None,
                    Err(End::Unsupported(why)) => break Some(why),
                    Err(End::Moved) => {}
                    Err(End::Lost(why)) => {
                        losses += 1;
                        if losses > 4 || QUIT.load(Ordering::Relaxed) {
                            break Some(why);
                        }
                        std::thread::sleep(Duration::from_secs(3));
                    }
                }
            },
        };
        let _ = MFShutdown();
        CoUninitialize();
        failure
    };
    HOST.store(0, Ordering::Relaxed);
    if let Some(why) = failure.filter(|_| !QUIT.load(Ordering::Relaxed)) {
        let mut files: Vec<PathBuf> = SOURCE.lock().unwrap().iter().map(|(_, path)| path.clone()).collect();
        files.dedup();
        for file in files {
            failed(file, why.clone());
        }
    }
}

#[implement(IMFMediaEngineNotify)]
struct Notify(Arc<AtomicBool>);

impl IMFMediaEngineNotify_Impl for Notify_Impl {
    fn EventNotify(&self, event: u32, _param1: usize, _param2: u32) -> windows::core::Result<()> {
        if event == MF_MEDIA_ENGINE_EVENT_ERROR.0 as u32 {
            self.0.store(true, Ordering::Relaxed);
            wake();
        }
        Ok(())
    }
}

unsafe extern "system" fn host_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_WTSSESSION_CHANGE => match wparam.0 {
            WTS_SESSION_LOCK => LOCKED.store(true, Ordering::Relaxed),
            WTS_SESSION_UNLOCK => LOCKED.store(false, Ordering::Relaxed),
            _ => {}
        },
        WM_POWERBROADCAST if wparam.0 == PBT_POWERSETTINGCHANGE && lparam.0 != 0 => {
            let setting = &*(lparam.0 as *const POWERBROADCAST_SETTING);
            if setting.PowerSetting == GUID_CONSOLE_DISPLAY_STATE {
                DARK.store(setting.Data[0] == 0, Ordering::Relaxed);
            }
        }
        WM_ERASEBKGND => return LRESULT(1),
        WM_DESTROY if !CLOSING.load(Ordering::Relaxed) => GONE.store(true, Ordering::Relaxed),
        _ => {}
    }
    DefWindowProcW(hwnd, message, wparam, lparam)
}

struct Host(HWND);

impl Drop for Host {
    fn drop(&mut self) {
        CLOSING.store(true, Ordering::Relaxed);
        unsafe {
            let _ = DestroyWindow(self.0);
        }
        CLOSING.store(false, Ordering::Relaxed);
    }
}

pub(crate) unsafe fn place(hwnd: HWND, layer: HWND, on: &RECT) -> windows::core::Result<()> {
    SetParent(hwnd, Some(layer))?;
    SetWindowLongPtrW(hwnd, GWL_STYLE, (WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN | WS_CLIPSIBLINGS).0 as isize);
    let mut corner = POINT { x: on.left, y: on.top };
    let _ = ScreenToClient(layer, &mut corner);
    SetWindowPos(hwnd, None, corner.x, corner.y, on.right - on.left, on.bottom - on.top, SWP_NOACTIVATE | SWP_SHOWWINDOW | SWP_FRAMECHANGED)
}

unsafe fn host(on: &RECT) -> Result<Host, End> {
    let layer = crate::live::desktop_layer().ok_or_else(|| End::Unsupported("Windows no dejó colocar el fondo animado en el escritorio.".into()))?;
    let instance = GetModuleHandleW(None)?;
    let class = WNDCLASSW { lpfnWndProc: Some(host_proc), hInstance: instance.into(), lpszClassName: w!("CocuyoFondo"), ..Default::default() };
    RegisterClassW(&class);
    let hwnd = CreateWindowExW(WINDOW_EX_STYLE(0), w!("CocuyoFondo"), None, WS_POPUP, 0, 0, on.right - on.left, on.bottom - on.top, None, None, Some(instance.into()), None)?;
    let host = Host(hwnd);
    GONE.store(false, Ordering::Relaxed);
    place(hwnd, layer, on).map_err(|e| End::Lost(e.to_string()))?;
    let _ = WTSRegisterSessionNotification(hwnd, 0);
    let _ = RegisterPowerSettingNotification(HANDLE(hwnd.0), &GUID_CONSOLE_DISPLAY_STATE, DEVICE_NOTIFY_WINDOW_HANDLE);
    Ok(host)
}

fn cover(video: (u32, u32), screen: (i32, i32)) -> MFVideoNormalizedRect {
    let full = MFVideoNormalizedRect { left: 0.0, top: 0.0, right: 1.0, bottom: 1.0 };
    if video.0 == 0 || video.1 == 0 || screen.0 <= 0 || screen.1 <= 0 {
        return full;
    }
    let video_ratio = video.0 as f32 / video.1 as f32;
    let screen_ratio = screen.0 as f32 / screen.1 as f32;
    if video_ratio > screen_ratio {
        let keep = screen_ratio / video_ratio;
        MFVideoNormalizedRect { left: (1.0 - keep) / 2.0, right: (1.0 + keep) / 2.0, ..full }
    } else {
        let keep = video_ratio / screen_ratio;
        MFVideoNormalizedRect { top: (1.0 - keep) / 2.0, bottom: (1.0 + keep) / 2.0, ..full }
    }
}

struct Target {
    index: usize,
    size: (i32, i32),
    chain: IDXGISwapChain1,
    _host: Host,
}

struct Show {
    path: PathBuf,
    engine: IMFMediaEngine,
    broken: Arc<AtomicBool>,
    targets: Vec<Target>,
    paused: Option<bool>,
}

unsafe fn session() -> Result<(), End> {
    let screens = screens();
    let ids: Vec<&str> = screens.iter().map(|screen| screen.id.as_str()).collect();
    let wanted = groups(&SOURCE.lock().unwrap(), &ids);
    if wanted.is_empty() {
        return Ok(());
    }

    let mut device: Option<ID3D11Device> = None;
    let mut context: Option<ID3D11DeviceContext> = None;
    let flags = D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT;
    D3D11CreateDevice(None, D3D_DRIVER_TYPE_HARDWARE, HMODULE::default(), flags, None, D3D11_SDK_VERSION, Some(&mut device), None, Some(&mut context))?;
    let (Some(device), Some(_context)) = (device, context) else {
        return Err(End::Unsupported("Sin Direct3D 11.".into()));
    };
    let _ = device.cast::<ID3D11Multithread>()?.SetMultithreadProtected(true);

    let mut token = 0;
    let mut manager = None;
    MFCreateDXGIDeviceManager(&mut token, &mut manager)?;
    let manager = manager.ok_or_else(|| End::Unsupported("Sin gestor de video.".into()))?;
    manager.ResetDevice(&device, token)?;

    let factory: IDXGIFactory2 = device.cast::<IDXGIDevice>()?.GetAdapter()?.GetParent()?;
    let class: IMFMediaEngineClassFactory = CoCreateInstance(&CLSID_MFMediaEngineClassFactory, None, CLSCTX_INPROC_SERVER)?;

    let mut shows: Vec<Show> = Vec::new();
    for (path, at) in wanted {
        let broken = Arc::new(AtomicBool::new(false));
        let notify: IMFMediaEngineNotify = Notify(broken.clone()).into();
        let mut attributes = None;
        MFCreateAttributes(&mut attributes, 3)?;
        let attributes = attributes.ok_or_else(|| End::Unsupported("Sin atributos.".into()))?;
        attributes.SetUnknown(&MF_MEDIA_ENGINE_CALLBACK, &notify)?;
        attributes.SetUnknown(&MF_MEDIA_ENGINE_DXGI_MANAGER, &manager)?;
        attributes.SetUINT32(&MF_MEDIA_ENGINE_VIDEO_OUTPUT_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM.0 as u32)?;
        let engine = class.CreateInstance(MF_MEDIA_ENGINE_FORCEMUTE.0 as u32, &attributes)?;
        engine.SetLoop(true)?;
        engine.SetMuted(true)?;
        engine.SetSource(&BSTR::from(path.to_string_lossy().as_ref()))?;

        let mut targets = Vec::new();
        for index in at {
            let rect = screens[index].rect;
            let size = (rect.right - rect.left, rect.bottom - rect.top);
            let host = host(&rect)?;
            let description = DXGI_SWAP_CHAIN_DESC1 {
                Width: size.0 as u32,
                Height: size.1 as u32,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: 2,
                Scaling: DXGI_SCALING_STRETCH,
                SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
                AlphaMode: DXGI_ALPHA_MODE_IGNORE,
                ..Default::default()
            };
            let chain: IDXGISwapChain1 = factory.CreateSwapChainForHwnd(&device, host.0, &description, None, None)?;
            targets.push(Target { index, size, chain, _host: host });
        }
        shows.push(Show { path, engine, broken, targets, paused: None });
    }
    HOST.store(shows[0].targets[0]._host.0 .0 as isize, Ordering::Relaxed);

    let layout = |screens: &[Screen]| -> Vec<(String, RECT)> { screens.iter().map(|screen| (screen.id.clone(), screen.rect)).collect() };
    let built_for = layout(&screens);
    let mut misses = 0;
    let mut looked = Instant::now();
    let ended = 'frames: loop {
        let mut message = MSG::default();
        while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
            DispatchMessageW(&message);
        }
        if QUIT.load(Ordering::Relaxed) {
            break Ok(());
        }
        if GONE.load(Ordering::Relaxed) {
            break Err(End::Lost("El escritorio cerró la ventana del fondo.".into()));
        }
        let mut at = 0;
        while at < shows.len() {
            if shows[at].broken.swap(false, Ordering::Relaxed) {
                let gone = shows.remove(at);
                let code = gone.engine.GetError().map(|e| e.GetErrorCode()).unwrap_or(0);
                let _ = gone.engine.Shutdown();
                failed(gone.path.clone(), format!("Windows no pudo reproducir el video (error {code})."));
            } else {
                at += 1;
            }
        }
        if shows.is_empty() {
            break Ok(());
        }
        HOST.store(shows[0].targets[0]._host.0 .0 as isize, Ordering::Relaxed);
        if CHANGED.swap(false, Ordering::Relaxed) {
            let next = groups(&SOURCE.lock().unwrap(), &ids);
            let same = next.len() == shows.len() && next.iter().zip(&shows).all(|((_, at), show)| at.iter().copied().eq(show.targets.iter().map(|target| target.index)));
            if !same {
                break Err(End::Moved);
            }
            for ((path, _), show) in next.into_iter().zip(&mut shows) {
                if path != show.path {
                    show.engine.SetSource(&BSTR::from(path.to_string_lossy().as_ref()))?;
                    show.path = path;
                    show.paused = None;
                }
            }
        }

        let hold = PAUSE.load(Ordering::Relaxed) || LOCKED.load(Ordering::Relaxed) || DARK.load(Ordering::Relaxed);
        let hidden = HIDDEN.load(Ordering::Relaxed);
        let covered = |target: &Target| target.index < 32 && hidden & (1 << target.index) != 0;
        let mut playing = false;
        for show in &mut shows {
            let pause = hold || show.targets.iter().all(covered);
            if show.paused != Some(pause) {
                show.paused = Some(pause);
                let _ = if pause { show.engine.Pause() } else { show.engine.Play() };
            }
            playing |= !pause;
        }
        if !playing {
            let _ = WaitMessage();
            looked = Instant::now().checked_sub(LOOK_EVERY).unwrap_or(looked);
            continue;
        }
        if looked.elapsed() >= LOOK_EVERY {
            looked = Instant::now();
            if layout(&self::screens()) != built_for {
                break Err(End::Moved);
            }
        }

        for show in &shows {
            if show.paused == Some(true) {
                continue;
            }
            let mut time = 0i64;
            if (Interface::vtable(&show.engine).OnVideoStreamTick)(Interface::as_raw(&show.engine), &mut time) != S_OK {
                continue;
            }
            let (mut video_width, mut video_height) = (0, 0);
            let _ = show.engine.GetNativeVideoSize(Some(&mut video_width), Some(&mut video_height));
            for target in show.targets.iter().filter(|target| !covered(*target)) {
                let source = cover((video_width, video_height), target.size);
                let whole = RECT { left: 0, top: 0, right: target.size.0, bottom: target.size.1 };
                let drawn = target
                    .chain
                    .GetBuffer::<ID3D11Texture2D>(0)
                    .and_then(|buffer| show.engine.TransferVideoFrame(&buffer, Some(&source), &whole, None).map(|_| buffer));
                match drawn {
                    Ok(_buffer) => {
                        #[cfg(test)]
                        tests::snapshot(&device, &_context, &_buffer);
                        if target.chain.Present(0, DXGI_PRESENT(0)).is_err() {
                            break 'frames Err(End::Lost("Se perdió la tarjeta gráfica.".into()));
                        }
                        FRAMES.fetch_add(1, Ordering::Relaxed);
                        misses = 0;
                    }
                    Err(e) => {
                        misses += 1;
                        if misses > MAX_MISSES {
                            break 'frames Err(End::Lost(e.to_string()));
                        }
                    }
                }
            }
        }
        #[cfg(test)]
        tests::LOOPS.fetch_add(1, Ordering::Relaxed);
        let before = Instant::now();
        let _ = DwmFlush();
        if before.elapsed() < Duration::from_millis(2) {
            std::thread::sleep(Duration::from_millis(12));
        }
    };
    for show in &shows {
        let _ = show.engine.Shutdown();
    }
    HOST.store(0, Ordering::Relaxed);
    drop(shows);
    ended
}

fn pack(high: u32, low: u32) -> u64 {
    (u64::from(high) << 32) | u64::from(low)
}

fn even(value: f64) -> u32 {
    ((value / 2.0).round() as u32).max(1) * 2
}

fn plan(size: (u32, u32), fps: f64, screen: (u32, u32)) -> Option<((u32, u32), u32)> {
    let scale = f64::max(screen.0 as f64 / size.0 as f64, screen.1 as f64 / size.1 as f64).min(1.0);
    let target = (even(size.0 as f64 * scale), even(size.1 as f64 * scale));
    let smaller = (target.0 as f64 * target.1 as f64) < 0.8 * (size.0 as f64 * size.1 as f64);
    let keep_every = if fps > 40.0 { (fps / 30.0).round().max(1.0) as u32 } else { 1 };
    (smaller || keep_every > 1).then_some((if smaller { target } else { size }, keep_every))
}

pub fn optimize(source: &Path, target: &Path, screen: (u32, u32), keep_going: &dyn Fn() -> bool) -> Result<bool, String> {
    let draft = target.with_extension("tmp.mp4");
    let done = unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
        let done = match MFStartup(MF_VERSION, MFSTARTUP_LITE) {
            Ok(()) => {
                let done = transcode(source, &draft, screen, keep_going);
                let _ = MFShutdown();
                done
            }
            Err(e) => Err(e),
        };
        CoUninitialize();
        done
    };
    match done {
        Ok(true) => std::fs::rename(&draft, target).map(|_| true).map_err(|e| e.to_string()),
        other => {
            let _ = std::fs::remove_file(&draft);
            other.map_err(|e| e.to_string())
        }
    }
}

unsafe fn flags(pairs: &[(&GUID, u32)]) -> windows::core::Result<IMFAttributes> {
    let mut attributes = None;
    MFCreateAttributes(&mut attributes, pairs.len() as u32)?;
    let attributes = attributes.ok_or_else(windows::core::Error::empty)?;
    for (key, value) in pairs {
        attributes.SetUINT32(*key, *value)?;
    }
    Ok(attributes)
}

unsafe fn transcode(source: &Path, draft: &Path, screen: (u32, u32), keep_going: &dyn Fn() -> bool) -> windows::core::Result<bool> {
    const VIDEO: u32 = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
    const PROGRESSIVE: u32 = 2;
    const H264_HIGH: u32 = 100;

    let reading = flags(&[(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1), (&MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, 1)])?;
    let reader = MFCreateSourceReaderFromURL(&HSTRING::from(source.as_os_str()), &reading)?;
    let native = reader.GetNativeMediaType(VIDEO, 0)?;
    let size = native.GetUINT64(&MF_MT_FRAME_SIZE)?;
    let size = ((size >> 32) as u32, size as u32);
    let rate = native.GetUINT64(&MF_MT_FRAME_RATE).unwrap_or(pack(30, 1));
    let (rate_top, rate_bottom) = ((rate >> 32) as u32, (rate as u32).max(1));
    let fps = rate_top as f64 / rate_bottom as f64;
    let Some((target, keep_every)) = plan(size, fps, screen) else {
        return Ok(false);
    };

    reader.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false)?;
    reader.SetStreamSelection(VIDEO, true)?;
    let decoded = MFCreateMediaType()?;
    decoded.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
    decoded.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)?;
    decoded.SetUINT64(&MF_MT_FRAME_SIZE, pack(target.0, target.1))?;
    reader.SetCurrentMediaType(VIDEO, None, &decoded)?;
    let decoded = reader.GetCurrentMediaType(VIDEO)?;

    let out_fps = fps / keep_every as f64;
    let bitrate = (target.0 as f64 * target.1 as f64 * out_fps * 0.11).clamp(1_500_000.0, 12_000_000.0) as u32;
    let writing = flags(&[(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1), (&MF_SINK_WRITER_DISABLE_THROTTLING, 1)])?;
    let writer = MFCreateSinkWriterFromURL(&HSTRING::from(draft.as_os_str()), None, &writing)?;
    let encoded = MFCreateMediaType()?;
    encoded.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
    encoded.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
    encoded.SetUINT32(&MF_MT_AVG_BITRATE, bitrate)?;
    encoded.SetUINT32(&MF_MT_INTERLACE_MODE, PROGRESSIVE)?;
    encoded.SetUINT32(&MF_MT_MPEG2_PROFILE, H264_HIGH)?;
    encoded.SetUINT64(&MF_MT_FRAME_SIZE, pack(target.0, target.1))?;
    encoded.SetUINT64(&MF_MT_FRAME_RATE, pack(rate_top, rate_bottom * keep_every))?;
    encoded.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack(1, 1))?;
    let stream = writer.AddStream(&encoded)?;
    writer.SetInputMediaType(stream, &decoded, None)?;
    writer.BeginWriting()?;

    let frame = (10_000_000.0 / out_fps) as i64;
    let mut index = 0u32;
    loop {
        if !keep_going() {
            return Ok(false);
        }
        let (mut state, mut time, mut sample) = (0u32, 0i64, None);
        reader.ReadSample(VIDEO, 0, None, Some(&mut state), Some(&mut time), Some(&mut sample))?;
        if state & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
            break;
        }
        let Some(sample) = sample else { continue };
        if index % keep_every == 0 {
            sample.SetSampleTime(time)?;
            sample.SetSampleDuration(frame)?;
            writer.WriteSample(stream, &sample)?;
        }
        index += 1;
    }
    writer.Finalize()?;
    Ok(index > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Graphics::Direct3D11::{
        D3D11_CPU_ACCESS_READ, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

    static SHOT: Mutex<Option<PathBuf>> = Mutex::new(None);

    pub(super) unsafe fn snapshot(device: &ID3D11Device, context: &ID3D11DeviceContext, frame: &ID3D11Texture2D) {
        let Some(path) = SHOT.lock().unwrap().take() else { return };
        let mut description = D3D11_TEXTURE2D_DESC::default();
        frame.GetDesc(&mut description);
        description.Usage = D3D11_USAGE_STAGING;
        description.BindFlags = 0;
        description.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
        description.MiscFlags = 0;
        let mut copy = None;
        device.CreateTexture2D(&description, None, Some(&mut copy)).unwrap();
        let copy = copy.unwrap();
        context.CopyResource(&copy, frame);
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        context.Map(&copy, 0, D3D11_MAP_READ, 0, Some(&mut mapped)).unwrap();
        let (width, height) = (description.Width as usize, description.Height as usize);
        let mut rgba = Vec::with_capacity(width * height * 4);
        for y in 0..height {
            let row = std::slice::from_raw_parts((mapped.pData as *const u8).add(y * mapped.RowPitch as usize), width * 4);
            for pixel in row.chunks_exact(4) {
                rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
            }
        }
        context.Unmap(&copy, 0);
        let mut encoder = png::Encoder::new(std::fs::File::create(path).unwrap(), width as u32, height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(&rgba).unwrap();
    }

    pub static LOOPS: AtomicU32 = AtomicU32::new(0);

    fn cpu_seconds() -> f64 {
        use windows::Win32::Foundation::FILETIME;
        let (mut a, mut b, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
        unsafe { GetProcessTimes(GetCurrentProcess(), &mut a, &mut b, &mut kernel, &mut user).unwrap() };
        let ticks = |t: FILETIME| (u64::from(t.dwHighDateTime) << 32 | u64::from(t.dwLowDateTime)) as f64;
        (ticks(kernel) + ticks(user)) / 1e7
    }

    #[test]
    fn a_window_only_hides_the_monitor_it_is_on() {
        let left = RECT { left: -1920, top: 0, right: 0, bottom: 1080 };
        let main = RECT { left: 0, top: 0, right: 2560, bottom: 1440 };
        let maximized = RECT { left: -8, top: -8, right: 2568, bottom: 1408 };
        assert!(hides(&main, &maximized, true) && !hides(&left, &maximized, true));
        let game = RECT { left: -1920, top: 0, right: 0, bottom: 1080 };
        assert!(hides(&left, &game, false) && !hides(&main, &game, false));
        let loose = RECT { left: 100, top: 100, right: 900, bottom: 700 };
        assert!(!hides(&main, &loose, false) && !hides(&left, &loose, false));
        let across = RECT { left: -1920, top: 0, right: 2560, bottom: 1440 };
        assert!(hides(&main, &across, false) && hides(&left, &across, false));
    }

    #[test]
    fn monitors_with_the_same_video_share_one_decoder() {
        let plan = vec![("B".to_string(), PathBuf::from("mar.mp4")), ("A".to_string(), PathBuf::from("mar.mp4")), ("C".to_string(), PathBuf::from("lluvia.mp4"))];
        assert_eq!(groups(&plan, &["A", "B", "C"]), vec![(PathBuf::from("mar.mp4"), vec![0, 1]), (PathBuf::from("lluvia.mp4"), vec![2])]);
        assert_eq!(groups(&plan, &["C", "X"]), vec![(PathBuf::from("lluvia.mp4"), vec![0])]);
        assert!(groups(&plan, &["X"]).is_empty() && groups(&[], &["A"]).is_empty());
    }

    #[test]
    fn there_is_always_a_main_screen() {
        let found = screens();
        assert!(found.iter().filter(|screen| screen.main).count() == 1 && found.iter().all(|screen| screen.size().0 > 1));
    }

    #[test]
    fn cover_crops_without_stretching() {
        let wide = cover((2560, 1080), (1920, 1080));
        assert!((wide.right - wide.left - 0.75).abs() < 0.001 && wide.top == 0.0 && wide.bottom == 1.0);
        let tall = cover((1080, 1920), (1920, 1080));
        assert!(tall.left == 0.0 && (tall.bottom - tall.top) < 0.33);
        let same = cover((3840, 2160), (1920, 1080));
        assert!(same.left.abs() < 0.001 && same.top.abs() < 0.001);
    }

    #[test]
    fn plans_only_what_pays_off() {
        assert_eq!(plan((3840, 2160), 30.0, (1920, 1080)), Some(((1920, 1080), 1)));
        assert_eq!(plan((1920, 1080), 30.0, (1920, 1080)), None);
        assert_eq!(plan((1920, 1080), 60.0, (1920, 1080)), Some(((1920, 1080), 2)));
        assert_eq!(plan((1280, 720), 24.0, (1920, 1080)), None);
        assert_eq!(plan((5120, 2160), 30.0, (1920, 1080)), Some(((2560, 1080), 1)));
    }

    #[test]
    #[ignore]
    fn fondo_de_verdad() {
        let video = PathBuf::from(std::env::var("COCUYO_VIDEO").expect("falta COCUYO_VIDEO"));
        let out = std::env::temp_dir().join("cocuyo-fondo-prueba.mp4");
        let _ = std::fs::remove_file(&out);
        let started = Instant::now();
        let made = optimize(&video, &out, (1920, 1080), &|| true);
        println!("optimizar: {made:?} en {:.1} s, {} KB", started.elapsed().as_secs_f32(), std::fs::metadata(&out).map(|m| m.len() / 1024).unwrap_or(0));

        for (name, file) in [("original", video.clone()), ("optimizado", out.clone())] {
            if !file.is_file() {
                continue;
            }
            FRAMES.store(0, Ordering::Relaxed);
            LOOPS.store(0, Ordering::Relaxed);
            play(vec![(screens()[0].id.clone(), file.clone())], |_, why| println!("FALLÓ: {why}"));
            std::thread::sleep(Duration::from_secs(3));
            *SHOT.lock().unwrap() = Some(std::env::temp_dir().join(format!("cocuyo-fondo-{name}.png")));
            let (frames, cpu, at) = (FRAMES.load(Ordering::Relaxed), cpu_seconds(), Instant::now());
            std::thread::sleep(Duration::from_secs(6));
            let seconds = at.elapsed().as_secs_f64();
            println!("vueltas: {:.1}/s", LOOPS.swap(0, Ordering::Relaxed) as f64 / 9.0);
            println!(
                "{name}: {:.1} cuadros/s, {:.1} % de un núcleo",
                (FRAMES.load(Ordering::Relaxed) - frames) as f64 / seconds,
                (cpu_seconds() - cpu) / seconds * 100.0
            );
            pause(true);
            std::thread::sleep(Duration::from_secs(2));
            let (frames, cpu) = (FRAMES.load(Ordering::Relaxed), cpu_seconds());
            std::thread::sleep(Duration::from_secs(2));
            println!("{name} en pausa: {} cuadros, {:.2} % de un núcleo", FRAMES.load(Ordering::Relaxed) - frames, (cpu_seconds() - cpu) / 2.0 * 100.0);
            pause(false);
            stop();
        }
    }
}
