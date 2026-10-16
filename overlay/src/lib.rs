// la capa que se mete en el juego. con anticheat ni lo intentes, te pueden banear

mod look;
mod shared;

use std::ffi::c_void;
use std::sync::atomic::{AtomicPtr, AtomicU32, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use look::{Paint, PillLook, OUT, POP, UI};
use shared::{Shared, CAPTION_APP, CAPTION_NONE, MAP_NAME, PILL_DONE, PILL_HIDDEN, PILL_LISTENING, PILL_THINKING};
use windows::core::{w, Interface, BOOL, HRESULT, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HMODULE, LPARAM, LRESULT, SIZE, WPARAM};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDeviceAndSwapChain, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, D3D11_BIND_SHADER_RESOURCE, D3D11_BOX,
    D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_FLAG, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_SDK_VERSION,
    D3D11_SUBRESOURCE_DATA, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_TYPELESS, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_B8G8R8A8_UNORM_SRGB,
    DXGI_FORMAT_B8G8R8X8_UNORM, DXGI_FORMAT_R10G10B10A2_UNORM, DXGI_FORMAT_R8G8B8A8_TYPELESS, DXGI_FORMAT_R8G8B8A8_UNORM,
    DXGI_FORMAT_R8G8B8A8_UNORM_SRGB, DXGI_MODE_DESC, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{IDXGISwapChain, DXGI_PRESENT_TEST, DXGI_SWAP_CHAIN_DESC, DXGI_USAGE_RENDER_TARGET_OUTPUT};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject, GdiFlush, GetTextExtentPoint32W, GetTextFaceW,
    GetTextMetricsW, SelectObject, SetBkMode, SetTextCharacterExtra, SetTextColor, TextOutW, ANTIALIASED_QUALITY, BITMAPINFO,
    BITMAPINFOHEADER, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DIB_RGB_COLORS, FW_MEDIUM, HDC, HFONT, HGDIOBJ, OUT_DEFAULT_PRECIS,
    TEXTMETRICW, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::{
    DisableThreadLibraryCalls, GetModuleHandleExW, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_PIN,
};
use windows::Win32::System::Memory::{
    MapViewOfFile, OpenFileMappingW, VirtualProtect, FILE_MAP_READ, PAGE_PROTECTION_FLAGS, PAGE_READWRITE,
};
use windows::Win32::UI::WindowsAndMessaging::{CallNextHookEx, CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WS_OVERLAPPED};

static SHARED: AtomicPtr<Shared> = AtomicPtr::new(std::ptr::null_mut());
static PRESENT: AtomicUsize = AtomicUsize::new(0);
static PRESENT1: AtomicUsize = AtomicUsize::new(0);

type PresentFn = unsafe extern "system" fn(*mut c_void, u32, u32) -> HRESULT;
type Present1Fn = unsafe extern "system" fn(*mut c_void, u32, u32, *const c_void) -> HRESULT;

#[no_mangle]
pub unsafe extern "system" fn cocuyo_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    CallNextHookEx(None, code, wparam, lparam)
}

#[no_mangle]
unsafe extern "system" fn DllMain(module: HINSTANCE, reason: u32, _reserved: *mut c_void) -> BOOL {
    const DLL_PROCESS_ATTACH: u32 = 1;
    if reason == DLL_PROCESS_ATTACH {
        let _ = DisableThreadLibraryCalls(HMODULE(module.0));
        let host = std::env::current_exe().ok().and_then(|exe| Some(exe.file_stem()?.to_string_lossy().to_lowercase()));
        if !matches!(host.as_deref(), Some("cocuyo" | "enganchar")) {
            let mut pinned = HMODULE::default();
            let _ = GetModuleHandleExW(
                GET_MODULE_HANDLE_EX_FLAG_PIN | GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
                PCWSTR(DllMain as *const u16),
                &mut pinned,
            );
            std::thread::spawn(|| unsafe { start() });
        }
    }
    BOOL(1)
}

unsafe fn start() {
    let name: Vec<u16> = MAP_NAME.encode_utf16().chain(Some(0)).collect();
    let Ok(mapping) = OpenFileMappingW(FILE_MAP_READ.0, false, PCWSTR(name.as_ptr())) else { return };
    let view = MapViewOfFile(mapping, FILE_MAP_READ, 0, 0, std::mem::size_of::<Shared>());
    if view.Value.is_null() {
        return;
    }
    SHARED.store(view.Value.cast(), Ordering::Release);
    hook_presents();
}

unsafe fn hook_presents() {
    let Ok(window) = CreateWindowExW(WINDOW_EX_STYLE(0), w!("STATIC"), w!(""), WS_OVERLAPPED, 0, 0, 16, 16, None, None, None, None) else {
        return;
    };
    let desc = DXGI_SWAP_CHAIN_DESC {
        BufferDesc: DXGI_MODE_DESC { Width: 16, Height: 16, Format: DXGI_FORMAT_R8G8B8A8_UNORM, ..Default::default() },
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
        BufferCount: 1,
        OutputWindow: window,
        Windowed: true.into(),
        ..Default::default()
    };
    let mut chain: Option<IDXGISwapChain> = None;
    for driver in [D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP] {
        let made = D3D11CreateDeviceAndSwapChain(
            None,
            driver,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_FLAG(0),
            None,
            D3D11_SDK_VERSION,
            Some(&desc),
            Some(&mut chain),
            None,
            None,
            None,
        );
        if made.is_ok() && chain.is_some() {
            break;
        }
    }
    if let Some(chain) = &chain {
        let table = *(chain.as_raw() as *const *mut usize);
        swap_entry(table.add(8), present as *const () as usize, &PRESENT);
        swap_entry(table.add(22), present1 as *const () as usize, &PRESENT1);
    }
    drop(chain);
    let _ = DestroyWindow(window);
}

unsafe fn swap_entry(entry: *mut usize, ours: usize, original: &AtomicUsize) {
    let mut old = PAGE_PROTECTION_FLAGS(0);
    if VirtualProtect(entry.cast(), std::mem::size_of::<usize>(), PAGE_READWRITE, &mut old).is_err() {
        return;
    }
    original.store(*entry, Ordering::Release);
    *entry = ours;
    let _ = VirtualProtect(entry.cast(), std::mem::size_of::<usize>(), old, &mut old);
}

unsafe extern "system" fn present(chain: *mut c_void, sync: u32, flags: u32) -> HRESULT {
    if flags & DXGI_PRESENT_TEST.0 == 0 {
        draw(chain);
    }
    let original: PresentFn = std::mem::transmute(PRESENT.load(Ordering::Acquire));
    original(chain, sync, flags)
}

unsafe extern "system" fn present1(chain: *mut c_void, sync: u32, flags: u32, parameters: *const c_void) -> HRESULT {
    if flags & DXGI_PRESENT_TEST.0 == 0 {
        draw(chain);
    }
    let original: Present1Fn = std::mem::transmute(PRESENT1.load(Ordering::Acquire));
    original(chain, sync, flags, parameters)
}

#[derive(Clone, Copy, PartialEq)]
enum Layout {
    Rgba,
    Bgra,
    Rgb10,
}

fn layout_of(format: DXGI_FORMAT) -> Option<Layout> {
    match format {
        DXGI_FORMAT_R8G8B8A8_UNORM | DXGI_FORMAT_R8G8B8A8_UNORM_SRGB | DXGI_FORMAT_R8G8B8A8_TYPELESS => Some(Layout::Rgba),
        DXGI_FORMAT_B8G8R8A8_UNORM | DXGI_FORMAT_B8G8R8A8_UNORM_SRGB | DXGI_FORMAT_B8G8R8A8_TYPELESS | DXGI_FORMAT_B8G8R8X8_UNORM => {
            Some(Layout::Bgra)
        }
        DXGI_FORMAT_R10G10B10A2_UNORM => Some(Layout::Rgb10),
        _ => None,
    }
}

struct Slot {
    device: usize,
    format: DXGI_FORMAT,
    size: (u32, u32),
    read: ID3D11Texture2D,
    write: ID3D11Texture2D,
}

struct Text {
    width: usize,
    height: usize,
    cover: Vec<u8>,
    word: Vec<u8>,
    words: usize,
    dark: bool,
}

struct Stage {
    born: Instant,
    last_frame: f32,
    pill: u32,
    pill_shown: bool,
    pill_since: f32,
    pill_state_since: f32,
    level: f32,
    pill_slot: Option<Slot>,
    caption_seq: u32,
    caption_shown: bool,
    caption_since: f32,
    words_since: f32,
    text: Option<Text>,
    settled: Option<(u32, Vec<Paint>)>,
    caption_slot: Option<Slot>,
    badge: Option<Badge>,
}
unsafe impl Send for Stage {}

struct Badge {
    device: usize,
    format: DXGI_FORMAT,
    second: u64,
    size: (u32, u32),
    texture: ID3D11Texture2D,
}

static STAGE: Mutex<Option<Stage>> = Mutex::new(None);

unsafe fn draw(chain: *mut c_void) {
    let shared = SHARED.load(Ordering::Acquire);
    if shared.is_null() {
        return;
    }
    AtomicU32::from_ptr(std::ptr::addr_of_mut!((*shared).frames)).fetch_add(1, Ordering::Relaxed);
    let shared = &*shared;
    let Ok(mut guard) = STAGE.try_lock() else { return };
    if shared.enabled == 0 {
        *guard = None;
        return;
    }
    if guard.is_none() {
        if shared.pill == PILL_HIDDEN && shared.caption == CAPTION_NONE && shared.recording == 0 {
            return;
        }
        *guard = Some(Stage {
            born: Instant::now(),
            last_frame: 0.0,
            pill: PILL_HIDDEN,
            pill_shown: false,
            pill_since: -10.0,
            pill_state_since: 0.0,
            level: 0.0,
            pill_slot: None,
            caption_seq: shared.caption_seq.wrapping_sub(1),
            caption_shown: false,
            caption_since: -10.0,
            words_since: 0.0,
            text: None,
            settled: None,
            caption_slot: None,
            badge: None,
        });
    }
    let Some(stage) = guard.as_mut() else { return };
    let now = stage.born.elapsed().as_secs_f32();
    let step = (now - stage.last_frame).clamp(0.0, 0.25);
    stage.last_frame = now;

    let wanted = shared.pill != PILL_HIDDEN;
    if wanted != stage.pill_shown {
        stage.pill_shown = wanted;
        stage.pill_since = now;
    }
    if wanted && shared.pill != stage.pill {
        stage.pill = shared.pill;
        stage.pill_state_since = now;
    }
    if stage.pill_shown && stage.pill != PILL_DONE && now - stage.pill_state_since > 120.0 {
        stage.pill_shown = false;
        stage.pill_since = now;
    }
    stage.level += (shared.level.clamp(0.0, 1.0) - stage.level) * (step / 0.09).min(1.0);
    let pill_alive = stage.pill_shown || now - stage.pill_since < 0.26;

    if shared.caption_seq != stage.caption_seq {
        stage.caption_seq = shared.caption_seq;
        stage.settled = None;
        if shared.caption == CAPTION_NONE {
            if stage.caption_shown {
                stage.caption_shown = false;
                stage.caption_since = now;
            }
        } else {
            stage.text = None; // Se compone más abajo, cuando se sabe el tamaño del juego.
            stage.words_since = now;
            if !stage.caption_shown {
                stage.caption_shown = true;
                stage.caption_since = now;
            }
        }
    }
    let caption_alive = stage.caption_shown || now - stage.caption_since < 0.24;
    let recording = shared.recording != 0;
    if !recording {
        stage.badge = None;
    }
    if !pill_alive && !caption_alive && !recording {
        return;
    }

    let Some(chain) = IDXGISwapChain::from_raw_borrowed(&chain) else { return };
    let Ok(device) = chain.GetDevice::<ID3D11Device>() else { return };
    let Ok(back) = chain.GetBuffer::<ID3D11Texture2D>(0) else { return };
    let mut target = D3D11_TEXTURE2D_DESC::default();
    back.GetDesc(&mut target);
    let Some(layout) = layout_of(target.Format) else { return };
    if target.SampleDesc.Count != 1 || target.Width < 320 || target.Height < 200 {
        return;
    }
    let Ok(context) = device.GetImmediateContext() else { return };
    let (screen_w, screen_h) = (target.Width as f32, target.Height as f32);

    let fit = if shared.screen_h > 100.0 { screen_h / shared.screen_h } else { 1.0 };
    let scale = shared.scale.clamp(0.5, 4.0) * fit;
    let inset = shared.inset_bottom.max(0.0) * fit;
    if recording {
        badge(&device, &context, &back, &target, layout, &mut stage.badge, shared.rec_since, scale);
    }

    if pill_alive {
        let since = now - stage.pill_since;
        let (opacity, drop, size) = if stage.pill_shown {
            let moved = look::ease(POP, since / 0.55);
            (look::ease(UI, since / 0.2), 14.0 * (1.0 - moved), 0.6 + 0.4 * moved)
        } else {
            let moved = look::ease(UI, since / 0.24);
            (1.0 - look::ease(UI, since / 0.2), 14.0 * moved, 1.0 - 0.4 * moved)
        };
        let state = now - stage.pill_state_since;
        let done = stage.pill == PILL_DONE;
        let pill = PillLook {
            opacity,
            drop,
            scale: size.max(0.05),
            bars: match stage.pill {
                PILL_THINKING => look::thinking_bars(state),
                PILL_LISTENING => look::listening_bars(stage.level),
                _ => look::listening_bars(0.0),
            },
            wave: if done { 1.0 - look::ease(UI, state / 0.18) } else { 1.0 },
            wave_scale: if done { (1.0 - 0.6 * look::ease(POP, state / 0.4)).max(0.05) } else { 1.0 },
            check: if done { look::ease(UI, state / 0.15) } else { 0.0 },
            check_scale: if done { (0.5 + 0.5 * look::ease(POP, state / 0.45)).max(0.05) } else { 1.0 },
            check_drawn: if done { look::ease(OUT, (state - 0.05) / 0.32) } else { 0.0 },
        };
        let center_x = screen_w / 2.0;
        let center_y = screen_h - inset - (34.0 + look::PILL_H / 2.0) * scale;
        let half_w = (look::PILL_W / 2.0 + look::PILL_MARGIN) * scale;
        let half_h = (look::PILL_H / 2.0 + look::PILL_MARGIN) * scale;
        let area = clamp_area(center_x - half_w, center_y - half_h, half_w * 2.0, half_h * 2.0, target.Width, target.Height);
        blend(&device, &context, &back, &target, layout, &mut stage.pill_slot, area, |x, y| {
            look::pill(&pill, (x - center_x) / scale, (y - center_y) / scale)
        });
    }

    if caption_alive {
        if stage.text.is_none() {
            let who = text_of(&shared.who);
            let said = text_of(&shared.text);
            stage.text = compose(&who, &said, shared.caption == CAPTION_APP, scale, screen_w * 0.88);
        }
        let Some(text) = stage.text.as_ref() else { return };
        let border = if text.dark { 1.0 } else { 1.2 };
        let width = text.width as f32 + (look::SAID_PAD_X + border) * 2.0 * scale;
        let height = text.height as f32 + (look::SAID_PAD_Y + border) * 2.0 * scale;
        let view = if shared.view_h > 100.0 { shared.view_h * fit } else { screen_h - inset };
        let bottom = screen_h - inset - 0.17 * view;
        let (center_x, center_y) = (screen_w / 2.0, bottom - height / 2.0);
        let margin = look::SAID_MARGIN * scale;
        let area = clamp_area(
            center_x - width / 2.0 - margin,
            center_y - height / 2.0 - margin,
            width + margin * 2.0,
            height + margin * 2.0,
            target.Width,
            target.Height,
        );

        let since = now - stage.caption_since;
        let (opacity, rise, size) = if stage.caption_shown {
            let moved = look::ease(POP, since / 0.6);
            (look::ease(UI, since / 0.3), 10.0 * (1.0 - moved), 0.94 + 0.06 * moved)
        } else {
            let moved = look::ease(UI, since / 0.22);
            (1.0 - moved, 10.0 * moved, 1.0 - 0.06 * moved)
        };
        let words = now - stage.words_since;
        let ink = look::ink(text.dark);
        let point = |x: f32, y: f32| -> Paint {
            let qx = (x - center_x) / size;
            let qy = (y - bottom - rise * scale) / size + height / 2.0;
            let mut out = look::bubble(text.dark, qx / scale, qy / scale, width / scale, height / scale);
            let (tx, ty) = (qx + text.width as f32 / 2.0, qy + text.height as f32 / 2.0);
            if tx >= 0.0 && ty >= 0.0 && (tx as usize) < text.width && (ty as usize) < text.height {
                let word = text.word[ty as usize * text.width + tx as usize];
                if word != 0 {
                    let entered = if word == 255 { 1.0 } else { look::ease(UI, (words - 0.034 * (word - 1) as f32) / 0.42) };
                    let lifted = ty - 4.0 * scale * (1.0 - entered);
                    if lifted >= 0.0 && (lifted as usize) < text.height {
                        let cover = text.cover[lifted as usize * text.width + tx as usize] as f32 / 255.0;
                        out = out.under(Paint::rgba(ink.0, ink.1, ink.2, 1.0).fade(cover * entered));
                    }
                }
            }
            out.fade(opacity)
        };

        let still = stage.caption_shown && since > 0.62 && words > 0.45 + 0.034 * text.words as f32;
        if still {
            let (w, h) = (area.2 as usize, area.3 as usize);
            if stage.settled.as_ref().is_none_or(|(seq, layer)| *seq != stage.caption_seq || layer.len() != w * h) {
                let mut layer = Vec::with_capacity(w * h);
                for y in 0..h {
                    for x in 0..w {
                        layer.push(point(area.0 as f32 + x as f32 + 0.5, area.1 as f32 + y as f32 + 0.5));
                    }
                }
                stage.settled = Some((stage.caption_seq, layer));
            }
            let layer = &stage.settled.as_ref().unwrap().1;
            let (left, top) = (area.0 as f32, area.1 as f32);
            blend(&device, &context, &back, &target, layout, &mut stage.caption_slot, area, |x, y| {
                layer[(y - top) as usize * w + (x - left) as usize]
            });
        } else {
            blend(&device, &context, &back, &target, layout, &mut stage.caption_slot, area, point);
        }
    }
}

fn text_of(units: &[u16]) -> String {
    String::from_utf16_lossy(&units.iter().copied().take_while(|unit| *unit != 0).collect::<Vec<u16>>())
}

fn clamp_area(x: f32, y: f32, w: f32, h: f32, max_w: u32, max_h: u32) -> (u32, u32, u32, u32) {
    let left = x.floor().clamp(0.0, max_w as f32) as u32;
    let top = y.floor().clamp(0.0, max_h as f32) as u32;
    let right = (x + w).ceil().clamp(0.0, max_w as f32) as u32;
    let bottom = (y + h).ceil().clamp(0.0, max_h as f32) as u32;
    (left, top, right.saturating_sub(left), bottom.saturating_sub(top))
}

#[allow(clippy::too_many_arguments)]
unsafe fn blend(
    device: &ID3D11Device,
    context: &ID3D11DeviceContext,
    back: &ID3D11Texture2D,
    target: &D3D11_TEXTURE2D_DESC,
    layout: Layout,
    slot: &mut Option<Slot>,
    area: (u32, u32, u32, u32),
    paint: impl Fn(f32, f32) -> Paint,
) {
    let (left, top, width, height) = area;
    if width == 0 || height == 0 {
        return;
    }
    let fits = slot.as_ref().is_some_and(|s| s.device == device.as_raw() as usize && s.format == target.Format && s.size == (width, height));
    if !fits {
        *slot = None;
        let mut desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: target.Format,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_STAGING,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            ..Default::default()
        };
        let mut read = None;
        if device.CreateTexture2D(&desc, None, Some(&mut read)).is_err() {
            return;
        }
        desc.Usage = D3D11_USAGE_DEFAULT;
        desc.CPUAccessFlags = 0;
        desc.BindFlags = D3D11_BIND_SHADER_RESOURCE.0 as u32;
        let mut write = None;
        if device.CreateTexture2D(&desc, None, Some(&mut write)).is_err() {
            return;
        }
        let (Some(read), Some(write)) = (read, write) else { return };
        *slot = Some(Slot { device: device.as_raw() as usize, format: target.Format, size: (width, height), read, write });
    }
    let Some(slot) = slot.as_ref() else { return };

    let region = D3D11_BOX { left, top, front: 0, right: left + width, bottom: top + height, back: 1 };
    context.CopySubresourceRegion(&slot.read, 0, 0, 0, 0, back, 0, Some(&region));
    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    if context.Map(&slot.read, 0, D3D11_MAP_READ, 0, Some(&mut mapped)).is_err() {
        return;
    }
    let mut pixels = vec![0u32; (width * height) as usize];
    for row in 0..height as usize {
        let source = (mapped.pData as *const u8).add(row * mapped.RowPitch as usize) as *const u32;
        std::ptr::copy_nonoverlapping(source, pixels.as_mut_ptr().add(row * width as usize), width as usize);
    }
    context.Unmap(&slot.read, 0);

    for row in 0..height {
        for column in 0..width {
            let over = paint(left as f32 + column as f32 + 0.5, top as f32 + row as f32 + 0.5);
            if over.a <= 0.0 {
                continue;
            }
            let pixel = &mut pixels[(row * width + column) as usize];
            let (was, keep) = (*pixel, 1.0 - over.a);
            *pixel = match layout {
                Layout::Rgba | Layout::Bgra => {
                    let channel = |shift: u32, value: f32| -> u32 {
                        let under = ((was >> shift) & 0xFF) as f32 / 255.0;
                        (((value + under * keep).clamp(0.0, 1.0) * 255.0 + 0.5) as u32) << shift
                    };
                    let (first, third) = if layout == Layout::Rgba { (over.r, over.b) } else { (over.b, over.r) };
                    channel(0, first) | channel(8, over.g) | channel(16, third) | (was & 0xFF00_0000)
                }
                Layout::Rgb10 => {
                    let channel = |shift: u32, value: f32| -> u32 {
                        let under = ((was >> shift) & 0x3FF) as f32 / 1023.0;
                        (((value + under * keep).clamp(0.0, 1.0) * 1023.0 + 0.5) as u32) << shift
                    };
                    channel(0, over.r) | channel(10, over.g) | channel(20, over.b) | (was & 0xC000_0000)
                }
            };
        }
    }

    context.UpdateSubresource(&slot.write, 0, None, pixels.as_ptr().cast(), width * 4, 0);
    context.CopySubresourceRegion(back, 0, left, top, 0, &slot.write, 0, None);
}

fn pack(layout: Layout, r: f32, g: f32, b: f32) -> u32 {
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
    let ten = |v: f32| (v.clamp(0.0, 1.0) * 1023.0 + 0.5) as u32;
    match layout {
        Layout::Rgba => byte(r) | byte(g) << 8 | byte(b) << 16 | 0xFF00_0000,
        Layout::Bgra => byte(b) | byte(g) << 8 | byte(r) << 16 | 0xFF00_0000,
        Layout::Rgb10 => ten(r) | ten(g) << 10 | ten(b) << 20 | 0xC000_0000,
    }
}

unsafe fn label(text: &str, pixels: f32) -> Option<(u32, u32, Vec<u8>)> {
    let dc = CreateCompatibleDC(None);
    if dc.is_invalid() {
        return None;
    }
    let face = font(dc, pixels);
    let old_font = SelectObject(dc, HGDIOBJ(face.0));
    let units: Vec<u16> = text.encode_utf16().collect();
    let mut size = SIZE::default();
    let _ = GetTextExtentPoint32W(dc, &units, &mut size);
    let (width, height) = (size.cx.max(1), size.cy.max(1));
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: width, biHeight: -height, biPlanes: 1, biBitCount: 32, ..Default::default() },
        ..Default::default()
    };
    let mut bits: *mut c_void = std::ptr::null_mut();
    let Ok(bitmap) = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0) else {
        SelectObject(dc, old_font);
        let _ = DeleteObject(HGDIOBJ(face.0));
        let _ = DeleteDC(dc);
        return None;
    };
    let old_bitmap = SelectObject(dc, HGDIOBJ(bitmap.0));
    std::ptr::write_bytes(bits as *mut u8, 0, (width * height * 4) as usize);
    SetBkMode(dc, TRANSPARENT);
    SetTextColor(dc, COLORREF(0x00FF_FFFF));
    let _ = TextOutW(dc, 0, 0, &units);
    let _ = GdiFlush();
    let cover = std::slice::from_raw_parts(bits as *const u32, (width * height) as usize).iter().map(|p| ((p >> 8) & 0xFF) as u8).collect();
    SelectObject(dc, old_bitmap);
    SelectObject(dc, old_font);
    let _ = DeleteObject(HGDIOBJ(bitmap.0));
    let _ = DeleteObject(HGDIOBJ(face.0));
    let _ = DeleteDC(dc);
    Some((width as u32, height as u32, cover))
}

unsafe fn badge(device: &ID3D11Device, context: &ID3D11DeviceContext, back: &ID3D11Texture2D, target: &D3D11_TEXTURE2D_DESC, layout: Layout, slot: &mut Option<Badge>, since: u64, scale: f32) {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
    let second = now.saturating_sub(since) / 1000;
    let same = slot.as_ref().is_some_and(|b| b.device == device.as_raw() as usize && b.format == target.Format);
    if !same || slot.as_ref().is_some_and(|b| b.second != second) {
        let clock = if second >= 3600 { format!("{}:{:02}:{:02}", second / 3600, second / 60 % 60, second % 60) } else { format!("{}:{:02}", second / 60, second % 60) };
        let Some((text_w, text_h, cover)) = label(&clock, 12.5 * scale) else { return };
        let (pad, dot, gap) = ((8.0 * scale).round() as u32, 8.0 * scale, (6.0 * scale).round() as u32);
        let height = ((22.0 * scale).round() as u32).max(text_h + 4);
        let width = pad + dot.ceil() as u32 + gap + text_w + pad;
        let (cx, cy) = (pad as f32 + dot / 2.0, height as f32 / 2.0);
        let top = (height - text_h) / 2;
        let left = pad + dot.ceil() as u32 + gap;
        let mut pixels = Vec::with_capacity((width * height) as usize);
        for y in 0..height {
            for x in 0..width {
                let (mut r, mut g, mut b) = (0.07, 0.07, 0.08);
                let distance = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                let red = (dot / 2.0 - distance + 0.5).clamp(0.0, 1.0);
                r += (1.0 - r) * red;
                g += (0.27 - g) * red;
                b += (0.23 - b) * red;
                if x >= left && y >= top && x - left < text_w && y - top < text_h {
                    let ink = cover[((y - top) * text_w + x - left) as usize] as f32 / 255.0 * 0.92;
                    r += (1.0 - r) * ink;
                    g += (1.0 - g) * ink;
                    b += (1.0 - b) * ink;
                }
                pixels.push(pack(layout, r, g, b));
            }
        }
        let fits = same && slot.as_ref().is_some_and(|b| b.size == (width, height));
        if fits {
            if let Some(made) = slot.as_mut() {
                context.UpdateSubresource(&made.texture, 0, None, pixels.as_ptr().cast(), width * 4, 0);
                made.second = second;
            }
        } else {
            let desc = D3D11_TEXTURE2D_DESC {
                Width: width,
                Height: height,
                MipLevels: 1,
                ArraySize: 1,
                Format: target.Format,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                Usage: D3D11_USAGE_DEFAULT,
                ..Default::default()
            };
            let data = D3D11_SUBRESOURCE_DATA { pSysMem: pixels.as_ptr().cast(), SysMemPitch: width * 4, SysMemSlicePitch: 0 };
            let mut texture = None;
            if device.CreateTexture2D(&desc, Some(&data), Some(&mut texture)).is_err() {
                return;
            }
            let Some(texture) = texture else { return };
            *slot = Some(Badge { device: device.as_raw() as usize, format: target.Format, second, size: (width, height), texture });
        }
    }
    let Some(made) = slot.as_ref() else { return };
    let margin = (14.0 * scale).round() as u32;
    if margin + made.size.0 <= target.Width && margin + made.size.1 <= target.Height {
        context.CopySubresourceRegion(back, 0, margin, margin, 0, &made.texture, 0, None);
    }
}

unsafe fn font(dc: HDC, pixels: f32) -> HFONT {
    for face in [w!("Segoe UI Variable Display"), w!("Segoe UI")] {
        let font = CreateFontW(
            -(pixels.round() as i32),
            0,
            0,
            0,
            FW_MEDIUM.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            ANTIALIASED_QUALITY,
            0,
            face,
        );
        let old = SelectObject(dc, HGDIOBJ(font.0));
        let mut name = [0u16; 64];
        let length = (GetTextFaceW(dc, Some(&mut name)).max(1) as usize - 1).min(63);
        SelectObject(dc, old);
        if String::from_utf16_lossy(&name[..length]) == face.to_string().unwrap_or_default() {
            return font;
        }
        let _ = DeleteObject(HGDIOBJ(font.0));
    }
    HFONT::default()
}

unsafe fn width_of(dc: HDC, text: &[u16]) -> i32 {
    let mut size = SIZE::default();
    let _ = GetTextExtentPoint32W(dc, text, &mut size);
    size.cx
}

const SMOOTH: usize = 4;

unsafe fn compose(who: &str, said: &str, dark: bool, scale: f32, most: f32) -> Option<Text> {
    let big = compose_raw(who, said, dark, scale * SMOOTH as f32, most * SMOOTH as f32)?;
    let (width, height) = (big.width.div_ceil(SMOOTH), big.height.div_ceil(SMOOTH));
    let mut cover = vec![0u8; width * height];
    let mut word = vec![0u8; width * height];
    for y in 0..height {
        for x in 0..width {
            let (mut sum, mut owner) = (0u32, 0u8);
            for sy in y * SMOOTH..((y + 1) * SMOOTH).min(big.height) {
                for sx in x * SMOOTH..((x + 1) * SMOOTH).min(big.width) {
                    sum += u32::from(big.cover[sy * big.width + sx]);
                    owner = owner.max(big.word[sy * big.width + sx]);
                }
            }
            cover[y * width + x] = (sum / (SMOOTH * SMOOTH) as u32) as u8;
            word[y * width + x] = owner;
        }
    }
    Some(Text { width, height, cover, word, words: big.words, dark })
}

unsafe fn compose_raw(who: &str, said: &str, dark: bool, scale: f32, most: f32) -> Option<Text> {
    let dc = CreateCompatibleDC(None);
    if dc.is_invalid() {
        return None;
    }
    let body = font(dc, look::SAID_FONT * scale);
    let label = font(dc, look::SAID_FONT * 0.72 * scale);
    let label_gap = (look::SAID_FONT * 0.72 * 0.12 * scale).round() as i32;
    let line = (look::SAID_LINE * scale).round() as i32;

    let old_font = SelectObject(dc, HGDIOBJ(body.0));
    let mut metrics = TEXTMETRICW::default();
    let _ = GetTextMetricsW(dc, &mut metrics);
    let zero: Vec<u16> = "0".encode_utf16().collect();
    let border = if dark { 1.0 } else { 1.2 };
    let chrome = (look::SAID_PAD_X + border) * 2.0 * scale;
    let limit = ((64 * width_of(dc, &zero)) as f32 - chrome).min(most - chrome).max(80.0) as i32;

    SelectObject(dc, HGDIOBJ(label.0));
    SetTextCharacterExtra(dc, label_gap);
    let who: Vec<u16> = who.to_uppercase().encode_utf16().collect();
    let mut label_metrics = TEXTMETRICW::default();
    let _ = GetTextMetricsW(dc, &mut label_metrics);
    let who_width = if who.is_empty() { 0 } else { width_of(dc, &who) + (8.0 * scale).round() as i32 };
    SetTextCharacterExtra(dc, 0);
    SelectObject(dc, HGDIOBJ(body.0));

    let mut words: Vec<Vec<u16>> = Vec::new();
    let mut current = String::new();
    for character in said.chars() {
        if !character.is_whitespace() && current.ends_with(char::is_whitespace) {
            words.push(current.encode_utf16().collect());
            current.clear();
        }
        current.push(if character.is_whitespace() { ' ' } else { character });
    }
    if !current.is_empty() {
        words.push(current.encode_utf16().collect());
    }
    words.truncate(250);

    struct Piece {
        line: usize,
        x: i32,
        solid: i32,
    }
    let mut pieces: Vec<Piece> = Vec::new();
    let mut lines: Vec<i32> = vec![who_width];
    let mut x = who_width;
    for word in &words {
        let width = width_of(dc, word);
        let trimmed = word.iter().rposition(|unit| *unit != b' ' as u16).map_or(0, |last| last + 1);
        let solid = width_of(dc, &word[..trimmed]);
        if x + solid > limit && x > 0 {
            lines.push(0);
            x = 0;
        }
        pieces.push(Piece { line: lines.len() - 1, x, solid });
        x += width;
        *lines.last_mut().unwrap() = x - (width - solid);
    }
    let width = (*lines.iter().max().unwrap_or(&0)).max(1);
    let height = (lines.len() as i32 * line).max(1);

    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height, // Negativo: la primera fila es la de arriba.
            biPlanes: 1,
            biBitCount: 32,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits: *mut c_void = std::ptr::null_mut();
    let Ok(bitmap) = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0) else {
        SelectObject(dc, old_font);
        let _ = DeleteObject(HGDIOBJ(body.0));
        let _ = DeleteObject(HGDIOBJ(label.0));
        let _ = DeleteDC(dc);
        return None;
    };
    let old_bitmap = SelectObject(dc, HGDIOBJ(bitmap.0));
    std::ptr::write_bytes(bits as *mut u8, 0, (width * height * 4) as usize);
    SetBkMode(dc, TRANSPARENT);
    SetTextColor(dc, COLORREF(0x00FF_FFFF));

    let mut word_of = vec![0u8; (width * height) as usize];
    let mut mark = |left: i32, top: i32, wide: i32, tall: i32, value: u8| {
        for y in top.max(0)..(top + tall).min(height) {
            for x in left.max(0)..(left + wide).min(width) {
                word_of[(y * width + x) as usize] = value;
            }
        }
    };
    let start = |index: usize| (width - lines[index]) / 2;
    let text_top = (line - metrics.tmHeight) / 2;
    if !who.is_empty() {
        SelectObject(dc, HGDIOBJ(label.0));
        SetTextCharacterExtra(dc, label_gap);
        let _ = TextOutW(dc, start(0), text_top + metrics.tmAscent - label_metrics.tmAscent, &who);
        SetTextCharacterExtra(dc, 0);
        SelectObject(dc, HGDIOBJ(body.0));
        mark(start(0), 0, who_width, line, 255);
    }
    for (index, (piece, word)) in pieces.iter().zip(&words).enumerate() {
        let left = start(piece.line) + piece.x;
        let _ = TextOutW(dc, left, piece.line as i32 * line + text_top, word);
        mark(left, piece.line as i32 * line, piece.solid.max(1), line, (index % 253) as u8 + 1);
    }
    let _ = GdiFlush();

    let source = std::slice::from_raw_parts(bits as *const u32, (width * height) as usize);
    let cover: Vec<u8> = source
        .iter()
        .zip(&word_of)
        .map(|(pixel, word)| {
            let value = (pixel >> 8) & 0xFF;
            if *word == 255 {
                (value * 55 / 100) as u8
            } else {
                value as u8
            }
        })
        .collect();

    SelectObject(dc, old_bitmap);
    SelectObject(dc, old_font);
    let _ = DeleteObject(HGDIOBJ(bitmap.0));
    let _ = DeleteObject(HGDIOBJ(body.0));
    let _ = DeleteObject(HGDIOBJ(label.0));
    let _ = DeleteDC(dc);
    Some(Text { width: width as usize, height: height as usize, cover, word: word_of, words: pieces.len(), dark })
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Graphics::Direct3D11::D3D11CreateDevice;

    #[test]
    fn the_recording_badge_lands_on_the_frame() {
        unsafe {
            let (mut device, mut context) = (None, None);
            D3D11CreateDevice(None, D3D_DRIVER_TYPE_WARP, HMODULE::default(), D3D11_CREATE_DEVICE_FLAG(0), None, D3D11_SDK_VERSION, Some(&mut device), None, Some(&mut context)).unwrap();
            let (device, context): (ID3D11Device, ID3D11DeviceContext) = (device.unwrap(), context.unwrap());
            let target = D3D11_TEXTURE2D_DESC {
                Width: 640,
                Height: 360,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                Usage: D3D11_USAGE_DEFAULT,
                ..Default::default()
            };
            let mut back = None;
            device.CreateTexture2D(&target, None, Some(&mut back)).unwrap();
            let back = back.unwrap();
            let since = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64 - 75_000;
            let mut slot = None;
            badge(&device, &context, &back, &target, Layout::Rgba, &mut slot, since, 1.0);
            let made = slot.as_ref().unwrap();
            assert_eq!(made.second, 75);
            let mut read = None;
            let staging = D3D11_TEXTURE2D_DESC { Usage: D3D11_USAGE_STAGING, CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32, ..target };
            device.CreateTexture2D(&staging, None, Some(&mut read)).unwrap();
            let read = read.unwrap();
            context.CopyResource(&read, &back);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            context.Map(&read, 0, D3D11_MAP_READ, 0, Some(&mut mapped)).unwrap();
            let pixel = |x: usize, y: usize| *((mapped.pData as *const u8).add(y * mapped.RowPitch as usize + x * 4) as *const u32);
            let dot = pixel(14 + 8 + 4, 14 + 11);
            assert!(dot & 0xFF > 200 && (dot >> 8) & 0xFF < 100, "punto {dot:08x}");
            let text = (14 + 30..14 + made.size.0 as usize).flat_map(|x| (14..14 + 22).map(move |y| (x, y))).filter(|(x, y)| pixel(*x, *y) & 0xFF > 180).count();
            assert!(text > 20, "texto {text}");
            assert_eq!(pixel(5, 5), 0);
            context.Unmap(&read, 0);
        }
    }
}
