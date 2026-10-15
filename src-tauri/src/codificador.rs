// los codificadores: el chip de video de la grafica por media foundation, el de la cpu si no hay otro, y el aac del sonido.
// amf y nvenc directos viven en sus archivos pero hablan igual que estos
#![allow(non_snake_case)]

use std::mem::ManuallyDrop;
use std::sync::Arc;

use windows::core::{Interface, GUID, HRESULT, PWSTR};
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_NV12, DXGI_SAMPLE_DESC};
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Variant::VARIANT;

use crate::mp4::{Codec, Packet, AUDIO_RATE, TICKS};

#[derive(Clone, Copy, Debug)]
pub struct Tuning {
    pub codec: Codec,
    pub size: (u32, u32),
    pub fps: u32,
    pub bitrate: u32,
    pub speed: u32,
}

pub struct Input {
    pub texture: ID3D11Texture2D,
    pub slice: u32,
    pub slot: usize,
}

pub trait Encoder {
    fn name(&self) -> String;
    fn hardware(&self) -> bool;
    fn input(&mut self) -> Result<Option<Input>, String>;
    fn submit(&mut self, input: Input, pts: i64, duration: i64) -> Result<(), String>;
    fn collect(&mut self, out: &mut Vec<Packet>) -> Result<(), String>;
    fn finish(&mut self, out: &mut Vec<Packet>);
}

#[windows_core::interface("901db4c7-31ce-41a2-85dc-8fa0bf41b8da")]
unsafe trait ICodecAPI: windows_core::IUnknown {
    fn IsSupported(&self, api: *const GUID) -> HRESULT;
    fn IsModifiable(&self, api: *const GUID) -> HRESULT;
    fn GetParameterRange(&self, api: *const GUID, low: *mut VARIANT, high: *mut VARIANT, step: *mut VARIANT) -> HRESULT;
    fn GetParameterValues(&self, api: *const GUID, values: *mut *mut VARIANT, count: *mut u32) -> HRESULT;
    fn GetDefaultValue(&self, api: *const GUID, value: *mut VARIANT) -> HRESULT;
    fn GetValue(&self, api: *const GUID, value: *mut VARIANT) -> HRESULT;
    fn SetValue(&self, api: *const GUID, value: *const VARIANT) -> HRESULT;
}

trait Step<T> {
    fn at(self, what: &str) -> Result<T, String>;
}

impl<T> Step<T> for windows::core::Result<T> {
    fn at(self, what: &str) -> Result<T, String> {
        self.map_err(|e| format!("{what}: {}", e.message()))
    }
}

const fn pack(high: u32, low: u32) -> u64 {
    ((high as u64) << 32) | low as u64
}

pub fn subtype(codec: Codec) -> GUID {
    match codec {
        Codec::H264 => MFVideoFormat_H264,
        Codec::Hevc => MFVideoFormat_HEVC,
        Codec::Av1 => MFVideoFormat_AV1,
    }
}

pub unsafe fn video_type(kind: &GUID, size: (u32, u32), fps: u32) -> windows::core::Result<IMFMediaType> {
    let media = MFCreateMediaType()?;
    media.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
    media.SetGUID(&MF_MT_SUBTYPE, kind)?;
    media.SetUINT64(&MF_MT_FRAME_SIZE, pack(size.0, size.1))?;
    media.SetUINT64(&MF_MT_FRAME_RATE, pack(fps, 1))?;
    media.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack(1, 1))?;
    media.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
    media.SetUINT32(&MF_MT_YUV_MATRIX, MFVideoTransferMatrix_BT709.0 as u32)?;
    media.SetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_16_235.0 as u32)?;
    Ok(media)
}

unsafe fn attributes(pairs: &[(&GUID, u32)]) -> windows::core::Result<IMFAttributes> {
    let mut store = None;
    MFCreateAttributes(&mut store, pairs.len() as u32 + 1)?;
    let store = store.ok_or_else(windows::core::Error::empty)?;
    for (key, value) in pairs {
        store.SetUINT32(*key, *value)?;
    }
    Ok(store)
}

unsafe fn text(store: &IMFAttributes, key: &GUID) -> String {
    let mut value = PWSTR::null();
    let mut length = 0;
    if store.GetAllocatedString(key, &mut value, &mut length).is_err() || value.is_null() {
        return String::new();
    }
    let read = value.to_string().unwrap_or_default();
    CoTaskMemFree(Some(value.0 as *const _));
    read
}

unsafe fn blob(store: &IMFAttributes, key: &GUID) -> Vec<u8> {
    let Ok(size) = store.GetBlobSize(key) else { return Vec::new() };
    let mut out = vec![0u8; size as usize];
    if store.GetBlob(key, &mut out, None).is_err() {
        return Vec::new();
    }
    out
}

pub struct Found {
    pub activate: IMFActivate,
    pub name: String,
}

pub unsafe fn find(category: GUID, flags: MFT_ENUM_FLAG, input: GUID, output: GUID, major: GUID, adapter: Option<i64>) -> Vec<Found> {
    let input = MFT_REGISTER_TYPE_INFO { guidMajorType: major, guidSubtype: input };
    let output = MFT_REGISTER_TYPE_INFO { guidMajorType: major, guidSubtype: output };
    let mut list: *mut Option<IMFActivate> = std::ptr::null_mut();
    let mut count = 0u32;
    let mut done = false;
    if let Some(luid) = adapter {
        if let Ok(store) = attributes(&[]) {
            if store.SetUINT64(&MFT_ENUM_ADAPTER_LUID, luid as u64).is_ok() {
                done = MFTEnum2(category, flags, Some(&input), Some(&output), &store, &mut list, &mut count).is_ok();
            }
        }
    }
    if !done && MFTEnumEx(category, flags, Some(&input), Some(&output), &mut list, &mut count).is_err() {
        return Vec::new();
    }
    let mut out = Vec::new();
    if list.is_null() {
        return out;
    }
    for i in 0..count as usize {
        if let Some(activate) = (*list.add(i)).take() {
            let name = text(&activate, &MFT_FRIENDLY_NAME_Attribute);
            out.push(Found { activate, name });
        }
    }
    CoTaskMemFree(Some(list as *const _));
    out
}

pub fn has_hardware(codec: Codec, adapter: Option<i64>) -> bool {
    unsafe {
        !find(MFT_CATEGORY_VIDEO_ENCODER, MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER, MFVideoFormat_NV12, subtype(codec), MFMediaType_Video, adapter).is_empty()
    }
}

unsafe fn tune(transform: &IMFTransform, tuning: &Tuning) {
    let Ok(api) = transform.cast::<ICodecAPI>() else { return };
    let set = |key: &GUID, value: VARIANT| {
        let _ = api.SetValue(key, &value);
    };
    set(&CODECAPI_AVEncCommonRateControlMode, VARIANT::from(eAVEncCommonRateControlMode_CBR.0 as u32));
    set(&CODECAPI_AVEncCommonMeanBitRate, VARIANT::from(tuning.bitrate));
    set(&CODECAPI_AVEncCommonQualityVsSpeed, VARIANT::from(100 - tuning.speed.min(100)));
    set(&CODECAPI_AVEncMPVGOPSize, VARIANT::from(tuning.fps));
    set(&CODECAPI_AVEncMPVDefaultBPictureCount, VARIANT::from(0u32));
    set(&CODECAPI_AVLowLatencyMode, VARIANT::from(true));
}

pub struct MfVideo {
    transform: IMFTransform,
    events: Option<IMFMediaEventGenerator>,
    allocator: Option<IMFVideoSampleAllocatorEx>,
    context: ID3D11DeviceContext,
    own: Option<(ID3D11Texture2D, ID3D11Texture2D)>,
    slots: Vec<Option<IMFSample>>,
    need: u32,
    ready: Vec<Packet>,
    header: Vec<u8>,
    provides: bool,
    out_size: u32,
    name: String,
    hardware: bool,
    tuning: Tuning,
    drained: bool,
}

impl MfVideo {
    pub unsafe fn open(device: &ID3D11Device, manager: &IMFDXGIDeviceManager, adapter: Option<i64>, tuning: Tuning, hardware: bool, skip: &[String]) -> Result<MfVideo, String> {
        let flags = if hardware { MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER } else { MFT_ENUM_FLAG_SYNCMFT | MFT_ENUM_FLAG_SORTANDFILTER };
        let candidates = find(MFT_CATEGORY_VIDEO_ENCODER, flags, MFVideoFormat_NV12, subtype(tuning.codec), MFMediaType_Video, adapter.filter(|_| hardware));
        let mut why = String::from("no hay codificador");
        for found in candidates.into_iter().filter(|found| !skip.contains(&found.name)) {
            match Self::try_one(device, manager, &found, tuning, hardware) {
                Ok(encoder) => return Ok(encoder),
                Err(e) => why = format!("{}: {e}", found.name),
            }
        }
        Err(why)
    }

    unsafe fn try_one(device: &ID3D11Device, manager: &IMFDXGIDeviceManager, found: &Found, tuning: Tuning, hardware: bool) -> Result<MfVideo, String> {
        let transform: IMFTransform = found.activate.ActivateObject().at("activar")?;
        let mut events = None;
        let mut allocator = None;
        let mut own = None;
        if hardware {
            let about = transform.GetAttributes().at("atributos")?;
            if about.GetUINT32(&MF_TRANSFORM_ASYNC).unwrap_or(0) != 0 {
                about.SetUINT32(&MF_TRANSFORM_ASYNC_UNLOCK, 1).at("desbloquear")?;
                events = Some(transform.cast::<IMFMediaEventGenerator>().at("eventos")?);
            }
            transform.ProcessMessage(MFT_MESSAGE_SET_D3D_MANAGER, manager.as_raw() as usize).at("gráfica")?;
        }
        tune(&transform, &tuning);
        let output = video_type(&subtype(tuning.codec), tuning.size, tuning.fps).at("tipo")?;
        output.SetUINT32(&MF_MT_AVG_BITRATE, tuning.bitrate).at("tipo")?;
        let profile = match tuning.codec {
            Codec::H264 => eAVEncH264VProfile_High.0 as u32,
            Codec::Hevc => eAVEncH265VProfile_Main_420_8.0 as u32,
            Codec::Av1 => 0,
        };
        if profile != 0 {
            output.SetUINT32(&MF_MT_MPEG2_PROFILE, profile).at("tipo")?;
        }
        transform.SetOutputType(0, &output, 0).at("salida")?;
        let input = video_type(&MFVideoFormat_NV12, tuning.size, tuning.fps).at("tipo nv12")?;
        transform.SetInputType(0, &input, 0).at("entrada nv12")?;
        tune(&transform, &tuning);
        let context = device.GetImmediateContext().at("contexto")?;
        if hardware {
            let pool: IMFVideoSampleAllocatorEx = {
                let mut made: Option<IMFVideoSampleAllocatorEx> = None;
                MFCreateVideoSampleAllocatorEx(&IMFVideoSampleAllocatorEx::IID, &mut made as *mut _ as *mut _).at("asignador")?;
                made.ok_or("asignador")?
            };
            pool.SetDirectXManager(manager).at("asignador")?;
            let both = attributes(&[(&MF_SA_D3D11_BINDFLAGS, (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_VIDEO_ENCODER.0) as u32), (&MF_SA_D3D11_USAGE, D3D11_USAGE_DEFAULT.0 as u32)]).at("asignador")?;
            if pool.InitializeSampleAllocatorEx(4, 10, &both, &input).is_err() {
                let target = attributes(&[(&MF_SA_D3D11_BINDFLAGS, D3D11_BIND_RENDER_TARGET.0 as u32), (&MF_SA_D3D11_USAGE, D3D11_USAGE_DEFAULT.0 as u32)]).at("asignador")?;
                pool.InitializeSampleAllocatorEx(4, 10, &target, &input).at("asignador")?;
            }
            allocator = Some(pool);
        } else {
            let mut description = D3D11_TEXTURE2D_DESC {
                Width: tuning.size.0,
                Height: tuning.size.1,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_NV12,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
                CPUAccessFlags: 0,
                MiscFlags: 0,
            };
            let mut target = None;
            device.CreateTexture2D(&description, None, Some(&mut target)).at("textura")?;
            description.Usage = D3D11_USAGE_STAGING;
            description.BindFlags = 0;
            description.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
            let mut staging = None;
            device.CreateTexture2D(&description, None, Some(&mut staging)).at("textura")?;
            own = Some((target.ok_or("textura")?, staging.ok_or("textura")?));
        }
        let info = transform.GetOutputStreamInfo(0).at("salida")?;
        let header = transform.GetOutputCurrentType(0).map(|t| blob(&t, &MF_MT_MPEG_SEQUENCE_HEADER)).unwrap_or_default();
        let _ = transform.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0);
        let _ = transform.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0);
        let need = u32::from(events.is_none());
        Ok(MfVideo {
            transform,
            events,
            allocator,
            context,
            own,
            slots: Vec::new(),
            need,
            ready: Vec::new(),
            header,
            provides: info.dwFlags & MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32 != 0,
            out_size: info.cbSize.max(1 << 20),
            name: found.name.clone(),
            hardware,
            tuning,
            drained: false,
        })
    }

    unsafe fn pump(&mut self) -> Result<(), String> {
        let Some(events) = self.events.clone() else { return Ok(()) };
        loop {
            match events.GetEvent(MF_EVENT_FLAG_NO_WAIT) {
                Ok(event) => {
                    let kind = event.GetType().unwrap_or(0) as i32;
                    if kind == METransformNeedInput.0 {
                        self.need += 1;
                    } else if kind == METransformHaveOutput.0 {
                        self.output()?;
                    } else if kind == METransformDrainComplete.0 {
                        self.drained = true;
                    }
                }
                Err(e) if e.code() == MF_E_NO_EVENTS_AVAILABLE => return Ok(()),
                Err(e) => return Err(format!("codificador: {}", e.message())),
            }
        }
    }

    unsafe fn output(&mut self) -> Result<bool, String> {
        loop {
            let mut buffer = MFT_OUTPUT_DATA_BUFFER { dwStreamID: 0, ..Default::default() };
            if !self.provides {
                let sample = MFCreateSample().at("salida")?;
                sample.AddBuffer(&MFCreateMemoryBuffer(self.out_size).at("salida")?).at("salida")?;
                buffer.pSample = ManuallyDrop::new(Some(sample));
            }
            let mut status = 0;
            let done = self.transform.ProcessOutput(0, std::slice::from_mut(&mut buffer), &mut status);
            let sample = ManuallyDrop::take(&mut buffer.pSample);
            drop(ManuallyDrop::take(&mut buffer.pEvents));
            match done {
                Ok(()) => {
                    if let Some(sample) = sample {
                        self.packet(&sample)?;
                    }
                    if self.events.is_some() {
                        return Ok(true);
                    }
                }
                Err(e) if e.code() == MF_E_TRANSFORM_STREAM_CHANGE => {
                    let kind = self.transform.GetOutputAvailableType(0, 0).at("cambio de formato")?;
                    self.transform.SetOutputType(0, &kind, 0).at("cambio de formato")?;
                    let header = blob(&kind, &MF_MT_MPEG_SEQUENCE_HEADER);
                    if !header.is_empty() {
                        self.header = header;
                    }
                    if self.events.is_some() {
                        return Ok(false);
                    }
                }
                Err(e) if e.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => return Ok(false),
                Err(e) => return Err(format!("codificar: {}", e.message())),
            }
        }
    }

    unsafe fn packet(&mut self, sample: &IMFSample) -> Result<(), String> {
        let buffer = sample.ConvertToContiguousBuffer().at("salida")?;
        let mut data = std::ptr::null_mut();
        let mut length = 0;
        buffer.Lock(&mut data, None, Some(&mut length)).at("salida")?;
        let mut bytes = std::slice::from_raw_parts(data, length as usize).to_vec();
        let _ = buffer.Unlock();
        let key = sample.GetUINT32(&MFSampleExtension_CleanPoint).unwrap_or(0) != 0 || crate::mp4::is_key(self.tuning.codec, &bytes);
        if key && !self.header.is_empty() && crate::mp4::Video::from_key(self.tuning.codec, self.tuning.size, &bytes).is_none() {
            let mut joined = self.header.clone();
            joined.extend_from_slice(&bytes);
            bytes = joined;
        }
        let pts = sample.GetSampleTime().unwrap_or(0);
        self.ready.push(Packet { data: bytes.into(), pts, key });
        Ok(())
    }
}

impl Encoder for MfVideo {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn hardware(&self) -> bool {
        self.hardware
    }

    fn input(&mut self) -> Result<Option<Input>, String> {
        unsafe {
            if let Some((target, _)) = &self.own {
                return Ok(Some(Input { texture: target.clone(), slice: 0, slot: 0 }));
            }
            self.pump()?;
            if self.need == 0 {
                return Ok(None);
            }
            let Some(allocator) = &self.allocator else { return Ok(None) };
            let sample = match allocator.AllocateSample() {
                Ok(sample) => sample,
                Err(e) if e.code() == MF_E_SAMPLEALLOCATOR_EMPTY => return Ok(None),
                Err(e) => return Err(format!("muestra: {}", e.message())),
            };
            let media = sample.GetBufferByIndex(0).at("muestra")?;
            if let Ok(full) = media.GetMaxLength() {
                let _ = media.SetCurrentLength(full);
            }
            let surface: IMFDXGIBuffer = media.cast().at("muestra")?;
            let mut texture: Option<ID3D11Texture2D> = None;
            surface.GetResource(&ID3D11Texture2D::IID, &mut texture as *mut _ as *mut _).at("muestra")?;
            let slice = surface.GetSubresourceIndex().at("muestra")?;
            let slot = match self.slots.iter().position(Option::is_none) {
                Some(free) => free,
                None => {
                    self.slots.push(None);
                    self.slots.len() - 1
                }
            };
            self.slots[slot] = Some(sample);
            Ok(Some(Input { texture: texture.ok_or("muestra")?, slice, slot }))
        }
    }

    fn submit(&mut self, input: Input, pts: i64, duration: i64) -> Result<(), String> {
        unsafe {
            let sample = match &self.own {
                Some((target, staging)) => {
                    self.context.CopyResource(staging, target);
                    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                    self.context.Map(staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped)).at("copiar")?;
                    let (w, h) = (self.tuning.size.0 as usize, self.tuning.size.1 as usize);
                    let total = w * h * 3 / 2;
                    let media = MFCreateMemoryBuffer(total as u32).at("copiar")?;
                    let mut into = std::ptr::null_mut();
                    media.Lock(&mut into, None, None).at("copiar")?;
                    let from = mapped.pData as *const u8;
                    let pitch = mapped.RowPitch as usize;
                    for row in 0..h + h / 2 {
                        std::ptr::copy_nonoverlapping(from.add(row * pitch), into.add(row * w), w);
                    }
                    let _ = media.Unlock();
                    self.context.Unmap(staging, 0);
                    media.SetCurrentLength(total as u32).at("copiar")?;
                    let sample = MFCreateSample().at("copiar")?;
                    sample.AddBuffer(&media).at("copiar")?;
                    sample
                }
                None => self.slots.get_mut(input.slot).and_then(Option::take).ok_or("muestra perdida")?,
            };
            sample.SetSampleTime(pts).at("muestra")?;
            sample.SetSampleDuration(duration).at("muestra")?;
            self.transform.ProcessInput(0, &sample, 0).at("codificar")?;
            if self.events.is_some() {
                self.need = self.need.saturating_sub(1);
            } else {
                while self.output()? {}
            }
            Ok(())
        }
    }

    fn collect(&mut self, out: &mut Vec<Packet>) -> Result<(), String> {
        unsafe { self.pump()? };
        out.append(&mut self.ready);
        Ok(())
    }

    fn finish(&mut self, out: &mut Vec<Packet>) {
        unsafe {
            let _ = self.transform.ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0);
            let _ = self.transform.ProcessMessage(MFT_MESSAGE_COMMAND_DRAIN, 0);
            if self.events.is_some() {
                let until = std::time::Instant::now() + std::time::Duration::from_secs(2);
                while !self.drained && std::time::Instant::now() < until {
                    if self.pump().is_err() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
            } else {
                while matches!(self.output(), Ok(true)) {}
            }
            out.append(&mut self.ready);
            let _ = self.transform.ProcessMessage(MFT_MESSAGE_NOTIFY_END_STREAMING, 0);
        }
    }
}

pub struct Aac {
    transform: IMFTransform,
    out_size: u32,
    taken: u64,
    made: u64,
    pub bitrate: u32,
}

unsafe fn audio_type(kind: &GUID, bitrate: u32) -> windows::core::Result<IMFMediaType> {
    let media = MFCreateMediaType()?;
    media.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
    media.SetGUID(&MF_MT_SUBTYPE, kind)?;
    media.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, 2)?;
    media.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, AUDIO_RATE)?;
    media.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
    if *kind == MFAudioFormat_AAC {
        media.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, bitrate / 8)?;
        media.SetUINT32(&MF_MT_AAC_PAYLOAD_TYPE, 0)?;
        media.SetUINT32(&MF_MT_AAC_AUDIO_PROFILE_LEVEL_INDICATION, 0x29)?;
    } else {
        media.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, 4)?;
        media.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, AUDIO_RATE * 4)?;
    }
    Ok(media)
}

pub fn aac_rate(kbps: u32) -> u32 {
    [96u32, 128, 160, 192].into_iter().min_by_key(|k| k.abs_diff(kbps)).unwrap_or(192) * 1000
}

impl Aac {
    pub unsafe fn open(kbps: u32) -> Result<Aac, String> {
        let bitrate = aac_rate(kbps);
        let found = find(MFT_CATEGORY_AUDIO_ENCODER, MFT_ENUM_FLAG_SYNCMFT | MFT_ENUM_FLAG_SORTANDFILTER, MFAudioFormat_PCM, MFAudioFormat_AAC, MFMediaType_Audio, None);
        let first = found.first().ok_or("no hay codificador aac")?;
        let transform: IMFTransform = first.activate.ActivateObject().at("aac")?;
        transform.SetOutputType(0, &audio_type(&MFAudioFormat_AAC, bitrate).at("aac")?, 0).at("aac")?;
        transform.SetInputType(0, &audio_type(&MFAudioFormat_PCM, bitrate).at("aac")?, 0).at("aac")?;
        let info = transform.GetOutputStreamInfo(0).at("aac")?;
        let _ = transform.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0);
        let _ = transform.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0);
        Ok(Aac { transform, out_size: info.cbSize.max(8192), taken: 0, made: 0, bitrate })
    }

    pub fn encode(&mut self, pcm: &[i16], out: &mut Vec<Packet>) -> Result<(), String> {
        unsafe {
            let bytes = pcm.len() * 2;
            let media = MFCreateMemoryBuffer(bytes as u32).at("aac")?;
            let mut into = std::ptr::null_mut();
            media.Lock(&mut into, None, None).at("aac")?;
            std::ptr::copy_nonoverlapping(pcm.as_ptr() as *const u8, into, bytes);
            let _ = media.Unlock();
            media.SetCurrentLength(bytes as u32).at("aac")?;
            let sample = MFCreateSample().at("aac")?;
            sample.AddBuffer(&media).at("aac")?;
            let frames = (pcm.len() / 2) as u64;
            sample.SetSampleTime((self.taken * TICKS as u64 / AUDIO_RATE as u64) as i64).at("aac")?;
            sample.SetSampleDuration((frames * TICKS as u64 / AUDIO_RATE as u64) as i64).at("aac")?;
            self.taken += frames;
            self.transform.ProcessInput(0, &sample, 0).at("aac")?;
            self.drain(out)
        }
    }

    unsafe fn drain(&mut self, out: &mut Vec<Packet>) -> Result<(), String> {
        loop {
            let sample = MFCreateSample().at("aac")?;
            sample.AddBuffer(&MFCreateMemoryBuffer(self.out_size).at("aac")?).at("aac")?;
            let mut buffer = MFT_OUTPUT_DATA_BUFFER { dwStreamID: 0, pSample: ManuallyDrop::new(Some(sample)), ..Default::default() };
            let mut status = 0;
            let done = self.transform.ProcessOutput(0, std::slice::from_mut(&mut buffer), &mut status);
            let sample = ManuallyDrop::take(&mut buffer.pSample);
            drop(ManuallyDrop::take(&mut buffer.pEvents));
            match done {
                Ok(()) => {
                    let Some(sample) = sample else { continue };
                    let media = sample.ConvertToContiguousBuffer().at("aac")?;
                    let mut data = std::ptr::null_mut();
                    let mut length = 0;
                    media.Lock(&mut data, None, Some(&mut length)).at("aac")?;
                    let bytes: Arc<[u8]> = std::slice::from_raw_parts(data, length as usize).into();
                    let _ = media.Unlock();
                    if !bytes.is_empty() {
                        out.push(Packet { data: bytes, pts: (self.made * TICKS as u64 / AUDIO_RATE as u64) as i64, key: true });
                        self.made += 1024;
                    }
                }
                Err(e) if e.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => return Ok(()),
                Err(e) => return Err(format!("aac: {}", e.message())),
            }
        }
    }

    pub fn finish(&mut self, out: &mut Vec<Packet>) {
        unsafe {
            let _ = self.transform.ProcessMessage(MFT_MESSAGE_COMMAND_DRAIN, 0);
            let _ = self.drain(out);
        }
    }
}

pub struct Pcm {
    made: u64,
}

impl Pcm {
    pub fn new() -> Pcm {
        Pcm { made: 0 }
    }

    pub fn encode(&mut self, pcm: &[i16], out: &mut Vec<Packet>) {
        let bytes: Vec<u8> = pcm.iter().flat_map(|s| s.to_le_bytes()).collect();
        out.push(Packet { data: bytes.into(), pts: (self.made * TICKS as u64 / AUDIO_RATE as u64) as i64, key: true });
        self.made += (pcm.len() / 2) as u64;
    }
}
