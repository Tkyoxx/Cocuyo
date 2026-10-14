// capturas: rectangulo, ventana, pantalla y el lazo

use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use windows::Win32::Foundation::{HANDLE, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, CAPTUREBLT, DIB_RGB_COLORS, SRCCOPY,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN,
};

use crate::{dialogs, overlay, settings};

const SETTLE: Duration = Duration::from_millis(48);
const CF_DIB: u32 = 8;
const HEADER: usize = std::mem::size_of::<BITMAPINFOHEADER>();

pub struct Frame {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    bgra: Vec<u8>,
}

static FRAME: Mutex<Option<Frame>> = Mutex::new(None);
static SELECTING: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Captured {
    path: Option<String>,
    w: i32,
    h: i32,
    copied: bool,
    error: Option<String>,
}

fn header(w: i32, h: i32) -> BITMAPINFOHEADER {
    BITMAPINFOHEADER {
        biSize: HEADER as u32,
        biWidth: w,
        biHeight: h,
        biPlanes: 1,
        biBitCount: 32,
        ..Default::default()
    }
}

fn grab(x: i32, y: i32, w: i32, h: i32) -> Option<Frame> {
    if w <= 0 || h <= 0 {
        return None;
    }
    unsafe {
        let screen = GetDC(None);
        let memory = CreateCompatibleDC(Some(screen));
        let bitmap = CreateCompatibleBitmap(screen, w, h);
        let previous = SelectObject(memory, bitmap.into());
        let copied = BitBlt(memory, 0, 0, w, h, Some(screen), x, y, SRCCOPY | CAPTUREBLT).is_ok();
        SelectObject(memory, previous);

        let mut info = BITMAPINFO { bmiHeader: header(w, -h), ..Default::default() };
        let mut bgra = vec![0u8; w as usize * h as usize * 4];
        let lines = GetDIBits(
            memory,
            bitmap,
            0,
            h as u32,
            Some(bgra.as_mut_ptr().cast()),
            &mut info,
            DIB_RGB_COLORS,
        );

        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(memory);
        ReleaseDC(None, screen);
        (copied && lines == h).then_some(Frame { x, y, w, h, bgra })
    }
}

impl Frame {
    fn crop(&self, x: i32, y: i32, w: i32, h: i32) -> Option<Frame> {
        let x0 = x.max(self.x);
        let y0 = y.max(self.y);
        let x1 = (x + w).min(self.x + self.w);
        let y1 = (y + h).min(self.y + self.h);
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        let (w, h) = (x1 - x0, y1 - y0);
        let mut bgra = Vec::with_capacity(w as usize * h as usize * 4);
        for row in 0..h {
            let start = ((y0 - self.y + row) * self.w + (x0 - self.x)) as usize * 4;
            bgra.extend_from_slice(&self.bgra[start..start + w as usize * 4]);
        }
        Some(Frame { x: x0, y: y0, w, h, bgra })
    }

    fn png_for_asking(&self) -> Option<Vec<u8>> {
        const LONGEST: i32 = 1568;
        let step = (self.w.max(self.h) + LONGEST - 1) / LONGEST;
        let (w, h) = ((self.w / step).max(1), (self.h / step).max(1));
        let mut rgb = Vec::with_capacity((w * h * 3) as usize);
        for y in 0..h {
            for x in 0..w {
                let mut sum = [0u32; 3];
                let mut count = 0;
                for sy in (y * step)..((y + 1) * step).min(self.h) {
                    for sx in (x * step)..((x + 1) * step).min(self.w) {
                        let at = ((sy * self.w + sx) * 4) as usize;
                        sum[0] += u32::from(self.bgra[at + 2]);
                        sum[1] += u32::from(self.bgra[at + 1]);
                        sum[2] += u32::from(self.bgra[at]);
                        count += 1;
                    }
                }
                rgb.extend(sum.map(|channel| (channel / count.max(1)) as u8));
            }
        }
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, w as u32, h as u32);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().ok()?.write_image_data(&rgb).ok()?;
        Some(out)
    }

    fn save_png(&self, path: &Path) -> Result<(), String> {
        let rgb: Vec<u8> = self.bgra.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0]]).collect();
        let file = File::create(path).map_err(|e| e.to_string())?;
        let mut encoder = png::Encoder::new(BufWriter::new(file), self.w as u32, self.h as u32);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(&rgb).map_err(|e| e.to_string())
    }

    fn copy_to_clipboard(&self) -> bool {
        let row = self.w as usize * 4;
        unsafe {
            let Ok(memory) = GlobalAlloc(GMEM_MOVEABLE, HEADER + self.bgra.len()) else {
                return false;
            };
            let target = GlobalLock(memory) as *mut u8;
            if target.is_null() {
                return false;
            }
            let head = header(self.w, self.h);
            std::ptr::copy_nonoverlapping((&head as *const BITMAPINFOHEADER).cast::<u8>(), target, HEADER);
            for (i, line) in self.bgra.chunks_exact(row).rev().enumerate() {
                std::ptr::copy_nonoverlapping(line.as_ptr(), target.add(HEADER + i * row), row);
            }
            let _ = GlobalUnlock(memory);

            if OpenClipboard(None).is_err() {
                return false;
            }
            let _ = EmptyClipboard();
            let placed = SetClipboardData(CF_DIB, Some(HANDLE(memory.0))).is_ok();
            let _ = CloseClipboard();
            placed
        }
    }
}

fn file_name() -> String {
    let t = unsafe { GetLocalTime() };
    format!(
        "Cocuyo {:04}-{:02}-{:02} {:02}.{:02}.{:02}.png",
        t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond
    )
}

fn target(app: &AppHandle, settings: &settings::Settings) -> Result<Option<PathBuf>, String> {
    if settings.ask_where {
        return Ok(dialogs::save_png(file_name()));
    }
    let dir = settings::capture_dir(app, settings).ok_or("No se encontró la carpeta de imágenes")?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("No se pudo crear {}: {e}", dir.display()))?;
    Ok(Some(dir.join(file_name())))
}

fn deliver(app: &AppHandle, frame: Frame) -> Captured {
    let settings = settings::load(app);
    let copied = settings.copy_to_clipboard && frame.copy_to_clipboard();
    let mut out = Captured { w: frame.w, h: frame.h, copied, ..Default::default() };
    match target(app, &settings) {
        Ok(Some(path)) => match frame.save_png(&path) {
            Ok(()) => out.path = Some(path.display().to_string()),
            Err(e) => out.error = Some(e),
        },
        Ok(None) => {}
        Err(e) => out.error = Some(e),
    }
    out
}

fn overlay_rect(app: &AppHandle) -> Option<(i32, i32, i32, i32)> {
    let window = app.get_webview_window(overlay::LABEL)?;
    let origin = window.outer_position().ok()?;
    let size = window.outer_size().ok()?;
    Some((origin.x, origin.y, size.width as i32, size.height as i32))
}

fn grab_clean(app: &AppHandle, rect: (i32, i32, i32, i32)) -> Option<Frame> {
    overlay::hide(app);
    std::thread::sleep(SETTLE);
    grab(rect.0, rect.1, rect.2, rect.3)
}

pub fn begin(app: &AppHandle) {
    let started = std::time::Instant::now();
    let rect = overlay_rect(app);
    let frame = rect.and_then(|rect| grab_clean(app, rect));
    if cfg!(debug_assertions) {
        eprintln!("[captura] zona {rect:?}, foto {} en {:?}", frame.is_some(), started.elapsed());
    }
    *FRAME.lock().unwrap() = frame;
}

pub fn selecting() -> bool {
    SELECTING.load(Ordering::Relaxed)
}

pub fn cancel_selection(app: &AppHandle) {
    let _ = app.emit_to(overlay::LABEL, "select", "cancel");
}

pub const HOTKEY_REGION: usize = 1;
pub const HOTKEY_WINDOW: usize = 2;

pub fn hotkey(app: &AppHandle, id: usize) {
    if selecting() {
        cancel_selection(app);
        return;
    }
    if id == HOTKEY_REGION {
        begin(app);
        SELECTING.store(true, Ordering::Relaxed);
        overlay::open_fully(app);
        let _ = app.emit_to(overlay::LABEL, "select", "start");
        return;
    }

    let rect = unsafe {
        if id == HOTKEY_WINDOW {
            let mut r = RECT::default();
            let found = DwmGetWindowAttribute(
                GetForegroundWindow(),
                DWMWA_EXTENDED_FRAME_BOUNDS,
                (&mut r as *mut RECT).cast(),
                std::mem::size_of::<RECT>() as u32,
            );
            found.is_ok().then_some((r.left, r.top, r.right - r.left, r.bottom - r.top))
        } else {
            Some((0, 0, GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)))
        }
    };
    let result = match rect.and_then(|rect| grab_clean(app, rect)) {
        Some(frame) => deliver(app, frame),
        None => Captured { error: Some("No se pudo capturar".into()), ..Default::default() },
    };
    let _ = app.emit_to(overlay::LABEL, "captured", result);
}

#[tauri::command]
pub async fn capture_commit(app: AppHandle, x: f64, y: f64, w: f64, h: f64) -> Captured {
    SELECTING.store(false, Ordering::Relaxed);
    let frame = FRAME.lock().unwrap().take();
    let scale = app
        .get_webview_window(overlay::LABEL)
        .and_then(|window| window.scale_factor().ok())
        .unwrap_or(1.0);
    let px = |v: f64| (v * scale).round() as i32;

    let cropped = frame.and_then(|f| f.crop(f.x + px(x), f.y + px(y), px(w), px(h)));
    let result = match cropped {
        Some(frame) => {
            *ASKED_ABOUT.lock().unwrap() = if crate::voice::question_pending() { frame.png_for_asking() } else { None };
            deliver(&app, frame)
        }
        None => Captured { error: Some("No había nada que capturar".into()), ..Default::default() },
    };
    if cfg!(debug_assertions) {
        eprintln!("[captura] recorte {x},{y} {w}x{h} -> {:?} {:?}", result.path, result.error);
    }
    result
}

pub fn screen_text() -> Result<String, String> {
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
    let (w, h) = unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
    let frame = grab(0, 0, w, h).ok_or("No pude mirar la pantalla.")?;
    crate::ocr::read(&frame.bgra, frame.w, frame.h)
}

pub fn asked_about() -> Option<Vec<u8>> {
    ASKED_ABOUT.lock().unwrap().take()
}

static ASKED_ABOUT: Mutex<Option<Vec<u8>>> = Mutex::new(None);

#[tauri::command]
pub fn capture_discard() {
    SELECTING.store(false, Ordering::Relaxed);
    *FRAME.lock().unwrap() = None;
}
