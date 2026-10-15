// amf directo: el chip de video de las radeon por la dll que ya trae el driver (amfrt64.dll), sin media foundation en el medio

use std::collections::VecDeque;
use std::ffi::c_void;

use windows::core::{s, w, Interface, PCWSTR};
use windows::Win32::Foundation::{FreeLibrary, HMODULE};
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_NV12, DXGI_SAMPLE_DESC};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

use crate::codificador::{Encoder, Input, Tuning};
use crate::mp4::{Codec, Packet};

type Object = *mut c_void;

const OK: i32 = 0;
const EOF: i32 = 23;
const INPUT_FULL: i32 = 25;
const VERSION: u64 = 1 << 48 | 4 << 32;
const DX11: i32 = 110;
const NV12: i32 = 1;
const SLOTS: usize = 8;

#[repr(C)]
#[derive(Clone, Copy)]
struct Variant {
    kind: i32,
    value: [u64; 2],
}

fn int(value: i64) -> Variant {
    Variant { kind: 2, value: [value as u64, 0] }
}

fn flag(value: bool) -> Variant {
    Variant { kind: 1, value: [value as u64, 0] }
}

fn pair(kind: i32, a: u32, b: u32) -> Variant {
    Variant { kind, value: [a as u64 | (b as u64) << 32, 0] }
}

unsafe fn slot<F: Copy>(object: Object, index: usize) -> F {
    let table = *(object as *const *const usize);
    std::mem::transmute_copy(&*table.add(index))
}

unsafe fn release(object: Object) {
    if !object.is_null() {
        slot::<extern "system" fn(Object) -> i32>(object, 1)(object);
    }
}

unsafe fn set(object: Object, name: PCWSTR, value: Variant) -> i32 {
    slot::<extern "system" fn(Object, PCWSTR, Variant) -> i32>(object, 3)(object, name, value)
}

pub struct Amf {
    library: HMODULE,
    context: Object,
    component: Object,
    textures: Vec<ID3D11Texture2D>,
    busy: VecDeque<(i64, usize)>,
    parked: Vec<Packet>,
    codec: Codec,
    name: String,
    drained: bool,
}

unsafe impl Send for Amf {}

impl Amf {
    pub fn available() -> bool {
        unsafe {
            match LoadLibraryW(w!("amfrt64.dll")) {
                Ok(library) => {
                    let found = GetProcAddress(library, s!("AMFInit")).is_some();
                    let _ = FreeLibrary(library);
                    found
                }
                Err(_) => false,
            }
        }
    }

    pub unsafe fn open(device: &ID3D11Device, tuning: Tuning) -> Result<Amf, String> {
        let library = LoadLibraryW(w!("amfrt64.dll")).map_err(|_| "no está amfrt64.dll")?;
        let mut made = Amf { library, context: std::ptr::null_mut(), component: std::ptr::null_mut(), textures: Vec::new(), busy: VecDeque::new(), parked: Vec::new(), codec: tuning.codec, name: String::new(), drained: false };
        let init = GetProcAddress(library, s!("AMFInit")).ok_or("amfrt64.dll sin AMFInit")?;
        let init: extern "C" fn(u64, *mut Object) -> i32 = std::mem::transmute(init);
        if let Some(query) = GetProcAddress(library, s!("AMFQueryVersion")) {
            let query: extern "C" fn(*mut u64) -> i32 = std::mem::transmute(query);
            let mut runtime = 0u64;
            if query(&mut runtime) == OK && runtime < VERSION {
                return Err("el driver de AMD es muy viejo".into());
            }
            made.name = format!("AMF {}.{}.{}", runtime >> 48, (runtime >> 32) & 0xFFFF, (runtime >> 16) & 0xFFFF);
        }
        let mut factory: Object = std::ptr::null_mut();
        if init(VERSION, &mut factory) != OK || factory.is_null() {
            return Err("AMF no arrancó".into());
        }
        let create_context: extern "system" fn(Object, *mut Object) -> i32 = slot(factory, 0);
        if create_context(factory, &mut made.context) != OK || made.context.is_null() {
            return Err("AMF sin contexto".into());
        }
        let init_dx11: extern "system" fn(Object, *mut c_void, i32) -> i32 = slot(made.context, 18);
        if init_dx11(made.context, device.as_raw(), DX11) != OK {
            return Err("AMF no tomó la gráfica".into());
        }
        let id = match tuning.codec {
            Codec::Hevc => w!("AMFVideoEncoderHW_HEVC"),
            Codec::Av1 => w!("AMFVideoEncoderHW_AV1"),
            Codec::H264 => w!("AMFVideoEncoderVCE_AVC"),
        };
        let create_component: extern "system" fn(Object, Object, PCWSTR, *mut Object) -> i32 = slot(factory, 1);
        if create_component(factory, made.context, id, &mut made.component) != OK || made.component.is_null() {
            return Err(format!("esta gráfica no codifica {} con AMF", crate::mp4::label(tuning.codec)));
        }
        let c = made.component;
        let (w, h) = tuning.size;
        let bitrate = tuning.bitrate as i64;
        if tuning.codec == Codec::Av1 {
            set(c, w!("Av1Usage"), int(0));
            set(c, w!("Av1EncodingLatencyMode"), int(1));
            set(c, w!("Av1QualityPreset"), int(match tuning.speed { 67.. => 100, 34..=66 => 70, _ => 30 }));
            set(c, w!("Av1Profile"), int(1));
            set(c, w!("Av1FrameSize"), pair(5, w, h));
            set(c, w!("Av1FrameRate"), pair(7, tuning.fps, 1));
            set(c, w!("Av1RateControlMethod"), int(3));
            set(c, w!("Av1TargetBitrate"), int(bitrate));
            set(c, w!("Av1PeakBitrate"), int(bitrate));
            set(c, w!("Av1VBVBufferSize"), int(bitrate));
            set(c, w!("Av1GOPSize"), int(tuning.fps as i64));
            set(c, w!("Av1HeaderInsertionMode"), int(2));
            set(c, w!("Av1FillerData"), flag(false));
            set(c, w!("Av1BPicturesPattern"), int(0));
            set(c, w!("Av1AlignmentMode"), int(3));
        } else if tuning.codec == Codec::Hevc {
            set(c, w!("HevcUsage"), int(0));
            set(c, w!("HevcQualityPreset"), int(match tuning.speed { 67.. => 10, 34..=66 => 5, _ => 0 }));
            set(c, w!("HevcProfile"), int(1));
            set(c, w!("HevcFrameSize"), pair(5, w, h));
            set(c, w!("HevcFrameRate"), pair(7, tuning.fps, 1));
            set(c, w!("HevcRateControlMethod"), int(3));
            set(c, w!("HevcTargetBitrate"), int(bitrate));
            set(c, w!("HevcPeakBitrate"), int(bitrate));
            set(c, w!("HevcVBVBufferSize"), int(bitrate));
            set(c, w!("HevcGOPSize"), int(tuning.fps as i64));
            set(c, w!("HevcGOPSPerIDR"), int(1));
            set(c, w!("HevcHeaderInsertionMode"), int(2));
            set(c, w!("HevcFillerDataEnable"), flag(false));
        } else {
            set(c, w!("Usage"), int(0));
            set(c, w!("QualityPreset"), int(match tuning.speed { 67.. => 1, 34..=66 => 0, _ => 2 }));
            set(c, w!("Profile"), int(100));
            set(c, w!("FrameSize"), pair(5, w, h));
            set(c, w!("FrameRate"), pair(7, tuning.fps, 1));
            set(c, w!("RateControlMethod"), int(1));
            set(c, w!("TargetBitrate"), int(bitrate));
            set(c, w!("PeakBitrate"), int(bitrate));
            set(c, w!("VBVBufferSize"), int(bitrate));
            set(c, w!("IDRPeriod"), int(tuning.fps as i64));
            set(c, w!("HeaderInsertionSpacing"), int(tuning.fps as i64));
            set(c, w!("BPicturesPattern"), int(0));
            set(c, w!("FillerDataEnable"), flag(false));
        }
        let start: extern "system" fn(Object, i32, i32, i32) -> i32 = slot(c, 17);
        let mut started = start(c, NV12, w as i32, h as i32);
        if started != OK && tuning.codec == Codec::Av1 && h == 1080 {
            set(c, w!("Av1AlignmentMode"), int(2));
            started = start(c, NV12, w as i32, h as i32);
        }
        if started != OK && tuning.codec == Codec::Av1 && w % 64 == 0 && h % 16 == 0 {
            set(c, w!("Av1AlignmentMode"), int(1));
            started = start(c, NV12, w as i32, h as i32);
        }
        if started != OK {
            return Err(format!("AMF no aceptó {w}×{h} (código {started})"));
        }
        let description = D3D11_TEXTURE2D_DESC {
            Width: w,
            Height: h,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_NV12,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };
        for _ in 0..SLOTS {
            let mut texture = None;
            device.CreateTexture2D(&description, None, Some(&mut texture)).map_err(|e| format!("textura: {}", e.message()))?;
            made.textures.push(texture.ok_or("textura")?);
        }
        if made.name.is_empty() {
            made.name = "AMF".into();
        }
        made.name = format!("{} {}", made.name, crate::mp4::label(tuning.codec));
        Ok(made)
    }

    unsafe fn query(&mut self, out: &mut Vec<Packet>) -> i32 {
        let query: extern "system" fn(Object, *mut Object) -> i32 = slot(self.component, 23);
        loop {
            let mut data: Object = std::ptr::null_mut();
            let result = query(self.component, &mut data);
            if data.is_null() {
                return result;
            }
            let kind: extern "system" fn(Object) -> i32 = slot(data, 17);
            if kind(data) == 0 {
                let pts: extern "system" fn(Object) -> i64 = slot(data, 20);
                let size: extern "system" fn(Object) -> usize = slot(data, 24);
                let native: extern "system" fn(Object) -> *mut u8 = slot(data, 25);
                let at = pts(data);
                let (bytes, length) = (native(data), size(data));
                if !bytes.is_null() && length > 0 {
                    let data = std::slice::from_raw_parts(bytes, length);
                    out.push(Packet { key: crate::mp4::is_key(self.codec, data), data: data.into(), pts: at });
                }
                while self.busy.front().is_some_and(|(pts, _)| *pts <= at) {
                    self.busy.pop_front();
                }
            }
            release(data);
        }
    }
}

impl Encoder for Amf {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn hardware(&self) -> bool {
        true
    }

    fn input(&mut self) -> Result<Option<Input>, String> {
        let free = (0..self.textures.len()).find(|i| !self.busy.iter().any(|(_, used)| used == i));
        Ok(free.map(|slot| Input { texture: self.textures[slot].clone(), slice: 0, slot }))
    }

    fn submit(&mut self, input: Input, pts: i64, duration: i64) -> Result<(), String> {
        unsafe {
            let wrap: extern "system" fn(Object, *mut c_void, *mut Object, *mut c_void) -> i32 = slot(self.context, 49);
            let mut surface: Object = std::ptr::null_mut();
            let made = wrap(self.context, input.texture.as_raw(), &mut surface, std::ptr::null_mut());
            if made != OK || surface.is_null() {
                return Err(format!("AMF no envolvió la textura (código {made})"));
            }
            slot::<extern "system" fn(Object, i64)>(surface, 19)(surface, pts);
            slot::<extern "system" fn(Object, i64)>(surface, 21)(surface, duration);
            let submit: extern "system" fn(Object, Object) -> i32 = slot(self.component, 22);
            let mut result = submit(self.component, surface);
            let mut tries = 0;
            let mut spare = Vec::new();
            while result == INPUT_FULL && tries < 50 {
                self.query(&mut spare);
                std::thread::sleep(std::time::Duration::from_micros(500));
                result = submit(self.component, surface);
                tries += 1;
            }
            release(surface);
            if result != OK {
                return Err(format!("AMF rechazó el cuadro (código {result})"));
            }
            self.busy.push_back((pts, input.slot));
            self.parked.append(&mut spare);
            Ok(())
        }
    }

    fn collect(&mut self, out: &mut Vec<Packet>) -> Result<(), String> {
        out.append(&mut self.parked);
        unsafe { self.query(out) };
        Ok(())
    }

    fn finish(&mut self, out: &mut Vec<Packet>) {
        unsafe {
            out.append(&mut self.parked);
            let drain: extern "system" fn(Object) -> i32 = slot(self.component, 20);
            drain(self.component);
            let until = std::time::Instant::now() + std::time::Duration::from_secs(2);
            while !self.drained && std::time::Instant::now() < until {
                if self.query(out) == EOF {
                    self.drained = true;
                } else {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            }
        }
    }
}

impl Drop for Amf {
    fn drop(&mut self) {
        unsafe {
            if !self.component.is_null() {
                slot::<extern "system" fn(Object) -> i32>(self.component, 19)(self.component);
                release(self.component);
            }
            if !self.context.is_null() {
                slot::<extern "system" fn(Object) -> i32>(self.context, 13)(self.context);
                release(self.context);
            }
            self.textures.clear();
            let _ = FreeLibrary(self.library);
        }
    }
}
