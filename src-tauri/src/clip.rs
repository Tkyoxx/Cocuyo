// historial del portapapeles, solo en memoria. se cierra la app y se fue

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use windows::core::w;
use windows::Win32::Foundation::{HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{
    AddClipboardFormatListener, CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::Win32::UI::WindowsAndMessaging::{CreateWindowExW, HWND_MESSAGE, WINDOW_EX_STYLE, WINDOW_STYLE};

use crate::inject;
use crate::system::base64;

const KEEP: usize = 40;
const MAX_CHARS: usize = 20_000;
const KEEP_IMAGES: usize = 8;
const MAX_PIXELS: usize = 16_000_000;
const MAX_IMAGE_BYTES: usize = 24_000_000;
const THUMB: usize = 360;
const CF_DIB: u32 = 8;
const HEADER: usize = 40;

#[derive(Clone, Serialize)]
pub struct Image {
    w: u32,
    h: u32,
    thumb: String,
    #[serde(skip)]
    png: Arc<Vec<u8>>,
    #[serde(skip)]
    hash: u64,
}

#[derive(Clone, Serialize)]
pub struct Clip {
    text: String,
    image: Option<Image>,
    at: u64,
}

static HISTORY: Mutex<Vec<Clip>> = Mutex::new(Vec::new());
static OWN_UNTIL: AtomicU64 = AtomicU64::new(0);

fn now() -> u64 {
    unsafe { GetTickCount64() }
}

pub fn ignore_own_changes() {
    OWN_UNTIL.store(now() + 600, Ordering::Relaxed);
}

pub unsafe fn listen() {
    let window = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        w!("STATIC"),
        w!("cocuyo-portapapeles"),
        WINDOW_STYLE(0),
        0,
        0,
        0,
        0,
        Some(HWND_MESSAGE),
        None,
        None,
        None,
    );
    if let Ok(window) = window {
        let _ = AddClipboardFormatListener(window);
    }
}

pub fn on_change() {
    if now() < OWN_UNTIL.load(Ordering::Relaxed) {
        return;
    }
    unsafe {
        let private = RegisterClipboardFormatW(w!("ExcludeClipboardContentFromMonitorProcessing"));
        if private != 0 && IsClipboardFormatAvailable(private).is_ok() {
            return;
        }
    }
    if let Some(text) = inject::clipboard_text() {
        if text.trim().is_empty() || text.chars().count() > MAX_CHARS {
            return;
        }
        let mut history = HISTORY.lock().unwrap();
        history.retain(|c| c.image.is_some() || c.text != text);
        history.insert(0, Clip { text, image: None, at: now() });
        trim(&mut history);
    } else if unsafe { IsClipboardFormatAvailable(CF_DIB).is_ok() } {
        std::thread::spawn(|| {
            let Some(image) = read_image() else { return };
            let mut history = HISTORY.lock().unwrap();
            history.retain(|c| c.image.as_ref().is_none_or(|other| other.hash != image.hash));
            history.insert(0, Clip { text: format!("Imagen de {} × {}", image.w, image.h), image: Some(image), at: now() });
            trim(&mut history);
        });
    }
}

fn trim(history: &mut Vec<Clip>) {
    history.truncate(KEEP);
    loop {
        let images: Vec<usize> = history.iter().enumerate().filter(|(_, c)| c.image.is_some()).map(|(i, _)| i).collect();
        let weight: usize = history.iter().filter_map(|c| c.image.as_ref()).map(|image| image.png.len()).sum();
        match images.last() {
            Some(oldest) if images.len() > KEEP_IMAGES || (weight > MAX_IMAGE_BYTES && images.len() > 1) => {
                history.remove(*oldest);
            }
            _ => return,
        }
    }
}

fn png(rgba: &[u8], w: u32, h: u32) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    encoder.write_header().ok()?.write_image_data(rgba).ok()?;
    Some(bytes)
}

fn shrink(rgba: &[u8], w: usize, h: usize, side: usize) -> (Vec<u8>, usize, usize) {
    let step = w.max(h).div_ceil(side).max(1);
    let (tw, th) = ((w / step).max(1), (h / step).max(1));
    let mut out = Vec::with_capacity(tw * th * 4);
    for ty in 0..th {
        for tx in 0..tw {
            let mut sum = [0u32; 4];
            let mut count = 0;
            for y in (ty * step..(ty * step + step).min(h)).step_by(step.div_ceil(4).max(1)) {
                for x in (tx * step..(tx * step + step).min(w)).step_by(step.div_ceil(4).max(1)) {
                    let pixel = &rgba[(y * w + x) * 4..][..4];
                    for (total, channel) in sum.iter_mut().zip(pixel) {
                        *total += u32::from(*channel);
                    }
                    count += 1;
                }
            }
            out.extend(sum.map(|total| (total / count.max(1)) as u8));
        }
    }
    (out, tw, th)
}

fn read_image() -> Option<Image> {
    let dib = unsafe {
        if OpenClipboard(None).is_err() {
            return None;
        }
        let bytes = GetClipboardData(CF_DIB).ok().and_then(|handle| {
            let memory = HGLOBAL(handle.0);
            let pointer = GlobalLock(memory) as *const u8;
            if pointer.is_null() {
                return None;
            }
            let bytes = std::slice::from_raw_parts(pointer, GlobalSize(memory)).to_vec();
            let _ = GlobalUnlock(memory);
            Some(bytes)
        });
        let _ = CloseClipboard();
        bytes?
    };
    decode(&dib)
}

fn decode(dib: &[u8]) -> Option<Image> {
    let field = |at: usize| Some(i32::from_le_bytes(dib.get(at..at + 4)?.try_into().ok()?));
    let (size, width, height) = (field(0)? as usize, field(4)?, field(8)?);
    let bits = u16::from_le_bytes(dib.get(14..16)?.try_into().ok()?) as usize;
    let compression = field(16)?;
    let (w, h) = (width as usize, height.unsigned_abs() as usize);
    if width <= 0 || h == 0 || w * h > MAX_PIXELS || !matches!(bits, 24 | 32) || !matches!(compression, 0 | 3) {
        return None;
    }
    let start = size + if compression == 3 && size == HEADER { 12 } else { 0 };
    let row = (w * bits / 8).div_ceil(4) * 4;
    let pixels = dib.get(start..start + row * h)?;
    let step = bits / 8;
    let mut rgba = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        let line = &pixels[(if height > 0 { h - 1 - y } else { y }) * row..][..w * step];
        for pixel in line.chunks_exact(step) {
            rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
        }
    }
    let hash = rgba.iter().step_by(97).fold(0xcbf29ce484222325u64 ^ (w as u64) << 32 ^ h as u64, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3));
    let (small, tw, th) = shrink(&rgba, w, h, THUMB);
    let thumb = format!("data:image/png;base64,{}", base64(&png(&small, tw as u32, th as u32)?));
    Some(Image { w: w as u32, h: h as u32, thumb, png: Arc::new(png(&rgba, w as u32, h as u32)?), hash })
}

fn write_image(image: &Image) -> bool {
    let decoder = png::Decoder::new(std::io::Cursor::new(image.png.as_slice()));
    let Ok(mut reader) = decoder.read_info() else { return false };
    let Some(size) = reader.output_buffer_size() else { return false };
    let mut rgba = vec![0u8; size];
    if reader.next_frame(&mut rgba).is_err() {
        return false;
    }
    let (w, h) = (image.w as usize, image.h as usize);
    unsafe {
        let Ok(memory) = GlobalAlloc(GMEM_MOVEABLE, HEADER + w * h * 4) else { return false };
        let target = GlobalLock(memory) as *mut u8;
        if target.is_null() {
            return false;
        }
        let mut header = [0u8; HEADER];
        header[0..4].copy_from_slice(&(HEADER as u32).to_le_bytes());
        header[4..8].copy_from_slice(&(w as i32).to_le_bytes());
        header[8..12].copy_from_slice(&(h as i32).to_le_bytes());
        header[12..14].copy_from_slice(&1u16.to_le_bytes());
        header[14..16].copy_from_slice(&32u16.to_le_bytes());
        std::ptr::copy_nonoverlapping(header.as_ptr(), target, HEADER);
        let mut offset = HEADER;
        for line in rgba.chunks_exact(w * 4).rev() {
            for pixel in line.chunks_exact(4) {
                *target.add(offset) = pixel[2];
                *target.add(offset + 1) = pixel[1];
                *target.add(offset + 2) = pixel[0];
                *target.add(offset + 3) = 255;
                offset += 4;
            }
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

#[derive(Serialize)]
pub struct Listing {
    now: u64,
    clips: Vec<Clip>,
}

#[tauri::command]
pub fn clip_list() -> Listing {
    Listing { now: now(), clips: HISTORY.lock().unwrap().clone() }
}

#[tauri::command]
pub async fn clip_use(index: usize) -> Result<(), String> {
    let clip = HISTORY.lock().unwrap().get(index).cloned().ok_or("Ese elemento ya no está.")?;
    ignore_own_changes();
    let placed = match &clip.image {
        Some(image) => write_image(image),
        None => inject::set_clipboard_text(&clip.text),
    };
    if placed {
        Ok(())
    } else {
        Err("No se pudo usar el portapapeles.".into())
    }
}

#[tauri::command]
pub fn clip_clear() {
    HISTORY.lock().unwrap().clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(weight: usize, hash: u64) -> Clip {
        Clip { text: String::new(), image: Some(Image { w: 1, h: 1, thumb: String::new(), png: Arc::new(vec![0; weight]), hash }), at: hash }
    }

    #[test]
    fn old_images_leave_first() {
        let mut history: Vec<Clip> = (0..12).map(|i| image(1000, i)).collect();
        history.insert(3, Clip { text: "texto".into(), image: None, at: 99 });
        trim(&mut history);
        assert_eq!(history.iter().filter(|c| c.image.is_some()).count(), KEEP_IMAGES);
        assert!(history.iter().any(|c| c.text == "texto"));
        assert_eq!(history.iter().filter_map(|c| c.image.as_ref()).map(|i| i.hash).max(), Some(7));
    }

    #[test]
    fn heavy_images_respect_the_weight_cap() {
        let mut history: Vec<Clip> = (0..4).map(|i| image(10_000_000, i)).collect();
        trim(&mut history);
        assert_eq!(history.len(), 2, "24 MB dan para dos imágenes de 10 MB");
        let mut one = vec![image(40_000_000, 1)];
        trim(&mut one);
        assert_eq!(one.len(), 1);
    }

    fn bitmap(bits: u16, bottom_up: bool) -> Vec<u8> {
        let step = bits as usize / 8;
        let row = (3 * step).div_ceil(4) * 4;
        let mut dib = vec![0u8; HEADER];
        dib[0..4].copy_from_slice(&(HEADER as u32).to_le_bytes());
        dib[4..8].copy_from_slice(&3i32.to_le_bytes());
        dib[8..12].copy_from_slice(&(if bottom_up { 2i32 } else { -2 }).to_le_bytes());
        dib[14..16].copy_from_slice(&bits.to_le_bytes());
        let top: [[u8; 3]; 3] = [[0, 0, 255], [0, 255, 0], [255, 0, 0]];
        let bottom: [[u8; 3]; 3] = [[255; 3]; 3];
        for line in if bottom_up { [bottom, top] } else { [top, bottom] } {
            let mut bytes = Vec::new();
            for pixel in line {
                bytes.extend_from_slice(&pixel);
                bytes.resize(bytes.len() + step - 3, 0);
            }
            bytes.resize(row, 0);
            dib.extend(bytes);
        }
        dib
    }

    #[test]
    fn windows_bitmaps_are_read_in_every_layout() {
        for (bits, bottom_up) in [(24, true), (24, false), (32, true), (32, false)] {
            let image = decode(&bitmap(bits, bottom_up)).expect("se lee");
            assert_eq!((image.w, image.h), (3, 2), "{bits} bits");
            let mut reader = png::Decoder::new(std::io::Cursor::new(image.png.as_slice())).read_info().unwrap();
            let mut rgba = vec![0u8; reader.output_buffer_size().unwrap()];
            reader.next_frame(&mut rgba).unwrap();
            assert_eq!(&rgba[..12], &[255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255], "{bits} bits, de abajo arriba: {bottom_up}");
            assert!(rgba[12..].iter().all(|value| *value == 255));
            assert!(image.thumb.starts_with("data:image/png;base64,"));
        }
        assert!(decode(&bitmap(24, true)[..50]).is_none());
        assert!(decode(&[]).is_none());
    }

    #[test]
    fn thumbnails_keep_the_proportions() {
        let rgba = vec![200u8; 1920 * 1080 * 4];
        let (small, w, h) = shrink(&rgba, 1920, 1080, THUMB);
        assert_eq!((w, h), (320, 180));
        assert_eq!(small.len(), 320 * 180 * 4);
        assert!(small.iter().all(|value| *value == 200));
    }
}

pub fn image_png(index: Option<usize>) -> Option<Arc<Vec<u8>>> {
    let history = HISTORY.lock().unwrap();
    match index {
        Some(index) => history.get(index)?.image.as_ref().map(|image| image.png.clone()),
        None => history.iter().find_map(|clip| clip.image.as_ref().map(|image| image.png.clone())),
    }
}
