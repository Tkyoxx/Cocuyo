// nvenc directo: el chip de video de las nvidia por nvEncodeAPI64.dll, la que trae el driver. api 12.1 (drivers de 2023 en adelante)

use std::collections::VecDeque;
use std::ffi::c_void;

use windows::core::{s, w, Interface, GUID};
use windows::Win32::Foundation::{FreeLibrary, HMODULE};
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_NV12, DXGI_SAMPLE_DESC};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

use crate::codificador::{Encoder, Input, Tuning};
use crate::mp4::{Codec, Packet};

const API: u32 = 12 | (1 << 24);
const fn ver(v: u32) -> u32 {
    API | (v << 16) | (7 << 28)
}
const FUNCTIONS: u32 = ver(2);
const OPEN: u32 = ver(1);
const CONFIG: u32 = ver(8) | (1 << 31);
const PRESET: u32 = ver(4) | (1 << 31);
const INIT: u32 = ver(6) | (1 << 31);
const REGISTER: u32 = ver(4);
const MAP: u32 = ver(4);
const BITSTREAM: u32 = ver(1);
const PICTURE: u32 = ver(6) | (1 << 31);
const LOCK: u32 = ver(1) | (1 << 31);
const SEQUENCE: u32 = ver(1);

const SUCCESS: i32 = 0;
const LOCK_BUSY: i32 = 13;
const NV12: u32 = 1;
const CBR: u32 = 2;
const SLOTS: usize = 6;

const H264: GUID = GUID::from_u128(0x6bc82762_4e63_4ca4_aa85_1e50f321f6bf);
const HEVC: GUID = GUID::from_u128(0x790cdc88_4522_4d7b_9425_bda9975f7603);
const P3: GUID = GUID::from_u128(0x36850110_3a07_441f_94d5_3670631f91f6);
const P4: GUID = GUID::from_u128(0x90a7b826_df06_4862_b9d2_cd6d73a08681);
const P6: GUID = GUID::from_u128(0x8e75c279_6299_4ab6_8302_0b215a335cf5);

type Handle = *mut c_void;

#[repr(C)]
struct Functions {
    version: u32,
    reserved: u32,
    calls: [*const c_void; 43],
    rest: [*const c_void; 275],
}

#[repr(C)]
struct OpenParams {
    version: u32,
    device_type: u32,
    device: Handle,
    reserved: Handle,
    api: u32,
    reserved1: [u32; 253],
    reserved2: [Handle; 64],
}

#[repr(C, align(8))]
struct Config([u8; 3584]);

#[repr(C)]
struct PresetConfig {
    version: u32,
    config: Config,
    reserved1: [u32; 255],
    reserved2: [Handle; 64],
}

#[repr(C)]
struct InitParams {
    version: u32,
    encode: GUID,
    preset: GUID,
    width: u32,
    height: u32,
    dar_width: u32,
    dar_height: u32,
    rate_num: u32,
    rate_den: u32,
    asynchronous: u32,
    ptd: u32,
    bits: u32,
    private_size: u32,
    private_data: Handle,
    config: *mut Config,
    max_width: u32,
    max_height: u32,
    hints: [u32; 8],
    tuning: u32,
    format: u32,
    state_buffers: u32,
    stats_level: u32,
    reserved: [u32; 285],
    reserved2: [Handle; 64],
}

#[repr(C)]
struct Register {
    version: u32,
    kind: u32,
    width: u32,
    height: u32,
    pitch: u32,
    sub: u32,
    resource: Handle,
    registered: Handle,
    format: u32,
    usage: u32,
    fence: Handle,
    chroma: [u32; 2],
    reserved1: [u32; 245],
    reserved2: [Handle; 61],
}

#[repr(C)]
struct MapInput {
    version: u32,
    sub: u32,
    input: Handle,
    registered: Handle,
    mapped: Handle,
    format: u32,
    reserved1: [u32; 251],
    reserved2: [Handle; 63],
}

#[repr(C)]
struct CreateBitstream {
    version: u32,
    size: u32,
    heap: u32,
    reserved: u32,
    buffer: Handle,
    pointer: Handle,
    reserved1: [u32; 58],
    reserved2: [Handle; 64],
}

#[repr(C, align(8))]
struct CodecPicture([u32; 388]);

#[repr(C)]
struct Picture {
    version: u32,
    width: u32,
    height: u32,
    pitch: u32,
    flags: u32,
    frame: u32,
    timestamp: u64,
    duration: u64,
    input: Handle,
    output: Handle,
    completion: Handle,
    format: u32,
    structure: u32,
    kind: u32,
    codec: CodecPicture,
    hints: [u32; 8],
    me_hints: Handle,
    reserved1: [u32; 6],
    reserved2: [Handle; 2],
    qp_map: Handle,
    qp_map_size: u32,
    reserved_bits: u32,
    me_distance: [u16; 2],
    alpha: Handle,
    sb_hints: Handle,
    sb_count: u32,
    state_index: u32,
    recon: Handle,
    reserved3: [u32; 284],
    reserved4: [Handle; 57],
}

#[repr(C)]
struct LockBitstream {
    version: u32,
    bits: u32,
    output: Handle,
    slices: *mut u32,
    frame: u32,
    status: u32,
    slice_count: u32,
    size: u32,
    timestamp: u64,
    duration: u64,
    pointer: *mut u8,
    picture_type: u32,
    structure: u32,
    qp: u32,
    satd: u32,
    ltr_index: u32,
    ltr_bitmap: u32,
    temporal: u32,
    intra: u32,
    inter: u32,
    mv_x: i32,
    mv_y: i32,
    alpha_size: u32,
    stats_size: u32,
    stats: Handle,
    display: u32,
    reserved1: [u32; 220],
    reserved2: [Handle; 63],
    internal: [u32; 8],
}

#[repr(C)]
struct Sequence {
    version: u32,
    size: u32,
    sps: u32,
    pps: u32,
    buffer: *mut u8,
    out: *mut u32,
    reserved: [u32; 250],
    reserved2: [Handle; 64],
}

fn zeroed<T>() -> Box<T> {
    unsafe { Box::new(std::mem::zeroed()) }
}

struct Slot {
    texture: ID3D11Texture2D,
    registered: Handle,
    bitstream: Handle,
    mapped: Handle,
    busy: bool,
}

pub struct Nvenc {
    library: HMODULE,
    calls: Box<Functions>,
    encoder: Handle,
    slots: Vec<Slot>,
    order: VecDeque<usize>,
    header: Vec<u8>,
    tuning: Tuning,
    frame: u32,
}

unsafe impl Send for Nvenc {}

impl Nvenc {
    pub fn available() -> bool {
        unsafe {
            match LoadLibraryW(w!("nvEncodeAPI64.dll")) {
                Ok(library) => {
                    let found = GetProcAddress(library, s!("NvEncodeAPICreateInstance")).is_some();
                    let _ = FreeLibrary(library);
                    found
                }
                Err(_) => false,
            }
        }
    }

    unsafe fn call<F: Copy>(&self, index: usize) -> F {
        std::mem::transmute_copy(&self.calls.calls[index])
    }

    pub unsafe fn open(device: &ID3D11Device, tuning: Tuning) -> Result<Nvenc, String> {
        let guid = match tuning.codec {
            Codec::H264 => H264,
            Codec::Hevc => HEVC,
            Codec::Av1 => return Err("NVENC directo hace H.264 y HEVC".into()),
        };
        let library = LoadLibraryW(w!("nvEncodeAPI64.dll")).map_err(|_| "no está nvEncodeAPI64.dll")?;
        let mut made = Nvenc { library, calls: zeroed(), encoder: std::ptr::null_mut(), slots: Vec::new(), order: VecDeque::new(), header: Vec::new(), tuning, frame: 0 };
        if let Some(max) = GetProcAddress(library, s!("NvEncodeAPIGetMaxSupportedVersion")) {
            let max: extern "system" fn(*mut u32) -> i32 = std::mem::transmute(max);
            let mut version = 0;
            if max(&mut version) == SUCCESS && version < (12 << 4 | 1) {
                return Err("el driver de NVIDIA es muy viejo para NVENC 12.1".into());
            }
        }
        let create = GetProcAddress(library, s!("NvEncodeAPICreateInstance")).ok_or("sin NvEncodeAPICreateInstance")?;
        let create: extern "system" fn(*mut Functions) -> i32 = std::mem::transmute(create);
        made.calls.version = FUNCTIONS;
        if create(&mut *made.calls) != SUCCESS {
            return Err("NVENC no arrancó".into());
        }
        let mut open = zeroed::<OpenParams>();
        open.version = OPEN;
        open.device = device.as_raw();
        open.api = API;
        let session: extern "system" fn(*mut OpenParams, *mut Handle) -> i32 = made.call(29);
        let opened = session(&mut *open, &mut made.encoder);
        if opened != SUCCESS || made.encoder.is_null() {
            made.encoder = std::ptr::null_mut();
            return Err(format!("NVENC no abrió sesión (código {opened})"));
        }
        let preset_guid = match tuning.speed {
            67.. => P3,
            34..=66 => P4,
            _ => P6,
        };
        let mut preset = zeroed::<PresetConfig>();
        preset.version = PRESET;
        preset.config.0[..4].copy_from_slice(&CONFIG.to_le_bytes());
        let get_preset: extern "system" fn(Handle, GUID, GUID, u32, *mut PresetConfig) -> i32 = made.call(39);
        let got = get_preset(made.encoder, guid, preset_guid, 1, &mut *preset);
        if got != SUCCESS {
            return Err(format!("NVENC sin preset (código {got})"));
        }
        let config = &mut preset.config.0;
        let put = |config: &mut [u8; 3584], at: usize, value: u32| config[at..at + 4].copy_from_slice(&value.to_le_bytes());
        config[..4].copy_from_slice(&CONFIG.to_le_bytes());
        put(config, 20, tuning.fps);
        put(config, 24, 1);
        put(config, 40 + 4, CBR);
        put(config, 40 + 20, tuning.bitrate);
        put(config, 40 + 24, tuning.bitrate);
        put(config, 40 + 28, tuning.bitrate);
        put(config, 40 + 32, tuning.bitrate);
        let codec = 168;
        if tuning.codec == Codec::H264 {
            let bits = u32::from_le_bytes(config[codec..codec + 4].try_into().unwrap_or_default()) | 1 << 12;
            put(config, codec, bits);
            put(config, codec + 8, tuning.fps);
        } else {
            let bits = u32::from_le_bytes(config[codec + 16..codec + 20].try_into().unwrap_or_default()) | 1 << 7;
            put(config, codec + 16, bits);
            put(config, codec + 20, tuning.fps);
        }
        let mut init = zeroed::<InitParams>();
        init.version = INIT;
        init.encode = guid;
        init.preset = preset_guid;
        init.width = tuning.size.0;
        init.height = tuning.size.1;
        init.dar_width = tuning.size.0;
        init.dar_height = tuning.size.1;
        init.rate_num = tuning.fps;
        init.rate_den = 1;
        init.ptd = 1;
        init.config = &mut preset.config;
        init.max_width = tuning.size.0;
        init.max_height = tuning.size.1;
        init.tuning = 1;
        let initialize: extern "system" fn(Handle, *mut InitParams) -> i32 = made.call(11);
        let ready = initialize(made.encoder, &mut *init);
        if ready != SUCCESS {
            return Err(format!("NVENC no aceptó la configuración (código {ready})"));
        }
        let mut header = vec![0u8; 1024];
        let mut length = 0u32;
        let mut sequence = zeroed::<Sequence>();
        sequence.version = SEQUENCE;
        sequence.size = header.len() as u32;
        sequence.buffer = header.as_mut_ptr();
        sequence.out = &mut length;
        let get_sequence: extern "system" fn(Handle, *mut Sequence) -> i32 = made.call(22);
        if get_sequence(made.encoder, &mut *sequence) == SUCCESS {
            header.truncate(length as usize);
            made.header = header;
        }
        let description = D3D11_TEXTURE2D_DESC {
            Width: tuning.size.0,
            Height: tuning.size.1,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_NV12,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };
        let register: extern "system" fn(Handle, *mut Register) -> i32 = made.call(30);
        let create_bitstream: extern "system" fn(Handle, *mut CreateBitstream) -> i32 = made.call(14);
        for _ in 0..SLOTS {
            let mut texture = None;
            device.CreateTexture2D(&description, None, Some(&mut texture)).map_err(|e| format!("textura: {}", e.message()))?;
            let texture = texture.ok_or("textura")?;
            let mut record = zeroed::<Register>();
            record.version = REGISTER;
            record.width = tuning.size.0;
            record.height = tuning.size.1;
            record.resource = texture.as_raw();
            record.format = NV12;
            let registered = register(made.encoder, &mut *record);
            if registered != SUCCESS {
                return Err(format!("NVENC no registró la textura (código {registered})"));
            }
            let mut stream = zeroed::<CreateBitstream>();
            stream.version = BITSTREAM;
            let created = create_bitstream(made.encoder, &mut *stream);
            if created != SUCCESS {
                return Err(format!("NVENC sin búfer de salida (código {created})"));
            }
            made.slots.push(Slot { texture, registered: record.registered, bitstream: stream.buffer, mapped: std::ptr::null_mut(), busy: false });
        }
        Ok(made)
    }

    unsafe fn take(&mut self, wait: bool, out: &mut Vec<Packet>) -> Result<(), String> {
        let lock: extern "system" fn(Handle, *mut LockBitstream) -> i32 = self.call(17);
        let unlock: extern "system" fn(Handle, Handle) -> i32 = self.call(18);
        let unmap: extern "system" fn(Handle, Handle) -> i32 = self.call(26);
        while let Some(&index) = self.order.front() {
            let mut locked = zeroed::<LockBitstream>();
            locked.version = LOCK;
            locked.bits = u32::from(!wait);
            locked.output = self.slots[index].bitstream;
            let result = lock(self.encoder, &mut *locked);
            if result == LOCK_BUSY {
                return Ok(());
            }
            if result != SUCCESS {
                return Err(format!("NVENC no entregó el cuadro (código {result})"));
            }
            let mut bytes = std::slice::from_raw_parts(locked.pointer, locked.size as usize).to_vec();
            unlock(self.encoder, self.slots[index].bitstream);
            unmap(self.encoder, self.slots[index].mapped);
            self.slots[index].mapped = std::ptr::null_mut();
            self.slots[index].busy = false;
            self.order.pop_front();
            let key = crate::mp4::is_key(self.tuning.codec, &bytes);
            if key && !self.header.is_empty() && crate::mp4::Video::from_key(self.tuning.codec, self.tuning.size, &bytes).is_none() {
                let mut joined = self.header.clone();
                joined.append(&mut bytes);
                bytes = joined;
            }
            out.push(Packet { data: bytes.into(), pts: locked.timestamp as i64, key });
        }
        Ok(())
    }
}

impl Encoder for Nvenc {
    fn name(&self) -> String {
        format!("NVENC {}", if self.tuning.codec == Codec::Hevc { "HEVC" } else { "H.264" })
    }

    fn hardware(&self) -> bool {
        true
    }

    fn input(&mut self) -> Result<Option<Input>, String> {
        Ok(self.slots.iter().position(|s| !s.busy).map(|slot| Input { texture: self.slots[slot].texture.clone(), slice: 0, slot }))
    }

    fn submit(&mut self, input: Input, pts: i64, duration: i64) -> Result<(), String> {
        unsafe {
            let map: extern "system" fn(Handle, *mut MapInput) -> i32 = self.call(25);
            let encode: extern "system" fn(Handle, *mut Picture) -> i32 = self.call(16);
            let mut mapping = zeroed::<MapInput>();
            mapping.version = MAP;
            mapping.registered = self.slots[input.slot].registered;
            let mapped = map(self.encoder, &mut *mapping);
            if mapped != SUCCESS {
                return Err(format!("NVENC no tomó la textura (código {mapped})"));
            }
            let mut picture = zeroed::<Picture>();
            picture.version = PICTURE;
            picture.width = self.tuning.size.0;
            picture.height = self.tuning.size.1;
            picture.frame = self.frame;
            picture.timestamp = pts as u64;
            picture.duration = duration as u64;
            picture.input = mapping.mapped;
            picture.output = self.slots[input.slot].bitstream;
            picture.format = NV12;
            picture.structure = 1;
            let done = encode(self.encoder, &mut *picture);
            if done != SUCCESS {
                let unmap: extern "system" fn(Handle, Handle) -> i32 = self.call(26);
                unmap(self.encoder, mapping.mapped);
                return Err(format!("NVENC no codificó (código {done})"));
            }
            self.frame += 1;
            self.slots[input.slot].mapped = mapping.mapped;
            self.slots[input.slot].busy = true;
            self.order.push_back(input.slot);
            Ok(())
        }
    }

    fn collect(&mut self, out: &mut Vec<Packet>) -> Result<(), String> {
        unsafe { self.take(false, out) }
    }

    fn finish(&mut self, out: &mut Vec<Packet>) {
        unsafe {
            let _ = self.take(true, out);
            let encode: extern "system" fn(Handle, *mut Picture) -> i32 = self.call(16);
            let mut end = zeroed::<Picture>();
            end.version = PICTURE;
            end.flags = 0x8;
            encode(self.encoder, &mut *end);
        }
    }
}

impl Drop for Nvenc {
    fn drop(&mut self) {
        unsafe {
            if !self.encoder.is_null() {
                let unmap: extern "system" fn(Handle, Handle) -> i32 = self.call(26);
                let unregister: extern "system" fn(Handle, Handle) -> i32 = self.call(31);
                let destroy_bitstream: extern "system" fn(Handle, Handle) -> i32 = self.call(15);
                let destroy: extern "system" fn(Handle) -> i32 = self.call(27);
                for slot in &self.slots {
                    if !slot.mapped.is_null() {
                        unmap(self.encoder, slot.mapped);
                    }
                    unregister(self.encoder, slot.registered);
                    destroy_bitstream(self.encoder, slot.bitstream);
                }
                destroy(self.encoder);
            }
            self.slots.clear();
            let _ = FreeLibrary(self.library);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_structs_match_the_sdk() {
        assert_eq!(std::mem::size_of::<Functions>(), 2552);
        assert_eq!(std::mem::size_of::<OpenParams>(), 1552);
        assert_eq!(std::mem::size_of::<PresetConfig>(), 5128);
        assert_eq!(std::mem::size_of::<Config>(), 3584);
        assert_eq!(std::mem::size_of::<InitParams>(), 1808);
        assert_eq!(std::mem::offset_of!(InitParams, config), 88);
        assert_eq!(std::mem::offset_of!(InitParams, tuning), 136);
        assert_eq!(std::mem::size_of::<Register>(), 1536);
        assert_eq!(std::mem::offset_of!(Register, registered), 32);
        assert_eq!(std::mem::offset_of!(Picture, codec), 80);
        assert_eq!(std::mem::offset_of!(Picture, recon), 1760);
        assert_eq!(std::mem::size_of::<Picture>(), 3360);
        assert_eq!(std::mem::size_of::<LockBitstream>(), 1552);
        assert_eq!(std::mem::offset_of!(LockBitstream, timestamp), 40);
        assert_eq!(std::mem::size_of::<MapInput>(), 1544);
        assert_eq!(std::mem::size_of::<CreateBitstream>(), 776);
        assert_eq!(std::mem::size_of::<Sequence>(), 1544);
        assert_eq!(std::mem::offset_of!(LockBitstream, pointer), 56);
        assert_eq!(std::mem::offset_of!(LockBitstream, display), 128);
        assert_eq!(std::mem::offset_of!(MapInput, mapped), 24);
        assert_eq!(std::mem::offset_of!(CreateBitstream, buffer), 16);
    }
}
