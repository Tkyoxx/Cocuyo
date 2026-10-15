// la grabadora: la pantalla se copia en la grafica, se pasa a nv12 ahi mismo y la codifica el chip de video. ningun cuadro toca la cpu

use std::collections::VecDeque;
use std::mem::ManuallyDrop;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use windows::core::{Interface, BOOL, GUID, HSTRING};
use windows::Win32::Foundation::{HMODULE, RECT};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_RATIONAL, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIAdapter1, IDXGIFactory1, IDXGIOutput1, IDXGIOutputDuplication, IDXGIResource, DXGI_ERROR_ACCESS_LOST,
    DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO,
};
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};
use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct Stats {
    pub grafica: String,
    pub codificador: String,
    pub por_hardware: bool,
    pub tamano: (u32, u32),
    pub fps: u32,
    pub cuadros: u64,
    pub nuevos: u64,
    pub tarde: u64,
    pub ms_por_cuadro: f64,
    pub audio: Vec<String>,
    pub cpu_pct: f64,
    pub segundos: f64,
}

struct Screen {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    duplication: IDXGIOutputDuplication,
    output: IDXGIOutput1,
    size: (u32, u32),
    name: String,
}

const RATE: u32 = 48_000;
const LAG: u64 = RATE as u64 / 10;
const MOST_QUEUED: usize = RATE as usize;

struct Ear {
    queue: VecDeque<f32>,
    left: crate::talk::Resampler,
    right: crate::talk::Resampler,
    rate: u32,
}

struct Ears {
    _streams: Vec<cpal::Stream>,
    arriving: Receiver<(usize, u32, Vec<f32>)>,
    ears: Vec<Ear>,
    written: u64,
}

fn listen_to(device: &cpal::Device, who: usize, feed: Sender<(usize, u32, Vec<f32>)>) -> Option<(cpal::Stream, u32)> {
    let config = device.default_input_config().or_else(|_| device.default_output_config()).ok()?;
    let rate = config.sample_rate();
    let channels = config.channels().max(1) as usize;
    let format = config.sample_format();
    let config: cpal::StreamConfig = config.into();
    let stereo = move |data: &mut dyn Iterator<Item = f32>| {
        let all: Vec<f32> = data.collect();
        let mut out = Vec::with_capacity(all.len() / channels * 2);
        for frame in all.chunks(channels) {
            let left = frame[0];
            out.push(left);
            out.push(*frame.get(1).unwrap_or(&left));
        }
        out
    };
    let stream = match format {
        cpal::SampleFormat::F32 => device.build_input_stream(config, move |data: &[f32], _: &_| {
            let _ = feed.send((who, rate, stereo(&mut data.iter().copied())));
        }, |_| {}, None),
        cpal::SampleFormat::I16 => device.build_input_stream(config, move |data: &[i16], _: &_| {
            let _ = feed.send((who, rate, stereo(&mut data.iter().map(|s| *s as f32 / 32768.0))));
        }, |_| {}, None),
        _ => return None,
    }
    .ok()?;
    stream.play().ok()?;
    Some((stream, rate))
}

impl Ears {
    fn open(system: bool, mic: bool, names: &mut Vec<String>) -> Option<Ears> {
        let host = cpal::default_host();
        let (feed, arriving) = channel();
        let mut streams = Vec::new();
        let mut ears = Vec::new();
        let wanted = [(system, host.default_output_device(), "PC"), (mic, crate::mic::device(&host), "micrófono")];
        for (on, device, what) in wanted {
            let Some(device) = device.filter(|_| on) else { continue };
            if let Some((stream, rate)) = listen_to(&device, ears.len(), feed.clone()) {
                let name = device.description().map(|about| about.name().to_string()).unwrap_or_default();
                names.push(format!("{what}: {name}"));
                streams.push(stream);
                ears.push(Ear { queue: VecDeque::new(), left: crate::talk::Resampler::new(rate as f64, RATE as f64), right: crate::talk::Resampler::new(rate as f64, RATE as f64), rate });
            }
        }
        (!ears.is_empty()).then(|| Ears { _streams: streams, arriving, ears, written: 0 })
    }

    fn take(&mut self, elapsed: Duration) -> Option<Vec<i16>> {
        self.take_with(elapsed, LAG)
    }

    fn take_with(&mut self, elapsed: Duration, lag: u64) -> Option<Vec<i16>> {
        while let Ok((who, _, samples)) = self.arriving.try_recv() {
            let ear = &mut self.ears[who];
            if ear.rate == RATE {
                ear.queue.extend(samples);
            } else {
                let left: Vec<f32> = samples.iter().step_by(2).copied().collect();
                let right: Vec<f32> = samples.iter().skip(1).step_by(2).copied().collect();
                let (left, right) = (ear.left.feed(&left), ear.right.feed(&right));
                for (l, r) in left.iter().zip(right.iter()) {
                    ear.queue.push_back(*l);
                    ear.queue.push_back(*r);
                }
            }
            let extra = ear.queue.len().saturating_sub(MOST_QUEUED * 2);
            ear.queue.drain(..extra & !1);
        }
        let due = (elapsed.as_secs_f64() * RATE as f64) as u64;
        let frames = due.saturating_sub(lag).saturating_sub(self.written) as usize;
        if frames < RATE as usize / 100 {
            return None;
        }
        let mut mixed = vec![0f32; frames * 2];
        for ear in &mut self.ears {
            let have = ear.queue.len().min(frames * 2);
            for (slot, sample) in mixed.iter_mut().zip(ear.queue.drain(..have)) {
                *slot += sample;
            }
        }
        self.written += frames as u64;
        Some(mixed.iter().map(|s| (soft(*s) * 32767.0) as i16).collect())
    }
}

fn soft(sample: f32) -> f32 {
    let size = sample.abs();
    if size <= 0.9 {
        return sample;
    }
    sample.signum() * (0.9 + 0.1 * ((size - 0.9) / 0.1).tanh())
}

unsafe fn audio_type(subtype: &GUID) -> windows::core::Result<IMFMediaType> {
    let kind = MFCreateMediaType()?;
    kind.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
    kind.SetGUID(&MF_MT_SUBTYPE, subtype)?;
    kind.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, 2)?;
    kind.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, RATE)?;
    kind.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
    if *subtype == MFAudioFormat_AAC {
        kind.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, 24_000)?;
        kind.SetUINT32(&MF_MT_AAC_PAYLOAD_TYPE, 0)?;
    } else {
        kind.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, 4)?;
        kind.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, RATE * 4)?;
        kind.SetUINT32(&MF_MT_ALL_SAMPLES_INDEPENDENT, 1)?;
    }
    Ok(kind)
}

unsafe fn audio_sample(pcm: &[i16], first_frame: u64) -> windows::core::Result<IMFSample> {
    let bytes = pcm.len() * 2;
    let buffer = MFCreateMemoryBuffer(bytes as u32)?;
    let mut data = std::ptr::null_mut();
    buffer.Lock(&mut data, None, None)?;
    std::ptr::copy_nonoverlapping(pcm.as_ptr() as *const u8, data, bytes);
    buffer.Unlock()?;
    buffer.SetCurrentLength(bytes as u32)?;
    let sample = MFCreateSample()?;
    sample.AddBuffer(&buffer)?;
    sample.SetSampleTime((first_frame * 10_000_000 / RATE as u64) as i64)?;
    sample.SetSampleDuration(((pcm.len() / 2) as u64 * 10_000_000 / RATE as u64) as i64)?;
    Ok(sample)
}

fn index_frames(stats: &Stats) -> u64 {
    stats.cuadros + stats.tarde
}

fn index_seconds(frames: u64, fps: u32) -> f64 {
    frames as f64 / fps as f64
}

fn even(n: u32) -> u32 {
    n & !1
}

pub fn fit(screen: (u32, u32), height: u32) -> (u32, u32) {
    if height == 0 || height >= screen.1 {
        return (even(screen.0), even(screen.1));
    }
    (even((screen.0 as u64 * height as u64 / screen.1 as u64) as u32), even(height))
}

pub fn bitrate(size: (u32, u32), fps: u32) -> u32 {
    (size.0 as f64 * size.1 as f64 * fps as f64 * 0.12).clamp(2_000_000.0, 40_000_000.0) as u32
}

const fn pack(high: u32, low: u32) -> u64 {
    ((high as u64) << 32) | low as u64
}

unsafe fn open_screen() -> windows::core::Result<Screen> {
    let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
    let mut index = 0;
    loop {
        let adapter: IDXGIAdapter1 = factory.EnumAdapters1(index)?;
        index += 1;
        let mut slot = 0;
        while let Ok(output) = adapter.EnumOutputs(slot) {
            slot += 1;
            let about = output.GetDesc()?;
            let RECT { left, top, right, bottom } = about.DesktopCoordinates;
            if !about.AttachedToDesktop.as_bool() || left != 0 || top != 0 {
                continue;
            }
            let mut device = None;
            let mut context = None;
            let flags = D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT;
            D3D11CreateDevice(&adapter, D3D_DRIVER_TYPE_UNKNOWN, HMODULE::default(), flags, None, D3D11_SDK_VERSION, Some(&mut device), None, Some(&mut context))?;
            let (Some(device), Some(context)) = (device, context) else { return Err(windows::core::Error::empty()) };
            let _ = device.cast::<ID3D11Multithread>()?.SetMultithreadProtected(true);
            let output: IDXGIOutput1 = output.cast()?;
            let duplication = output.DuplicateOutput(&device)?;
            let card = adapter.GetDesc1()?;
            let name = String::from_utf16_lossy(&card.Description).trim_end_matches('\0').to_string();
            return Ok(Screen { device, context, duplication, output, size: ((right - left) as u32, (bottom - top) as u32), name });
        }
    }
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

unsafe fn video_type(subtype: &GUID, size: (u32, u32), fps: u32) -> windows::core::Result<IMFMediaType> {
    let kind = MFCreateMediaType()?;
    kind.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
    kind.SetGUID(&MF_MT_SUBTYPE, subtype)?;
    kind.SetUINT64(&MF_MT_FRAME_SIZE, pack(size.0, size.1))?;
    kind.SetUINT64(&MF_MT_FRAME_RATE, pack(fps, 1))?;
    kind.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack(1, 1))?;
    kind.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
    kind.SetUINT32(&MF_MT_YUV_MATRIX, MFVideoTransferMatrix_BT709.0 as u32)?;
    kind.SetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_16_235.0 as u32)?;
    Ok(kind)
}

fn process_cpu() -> f64 {
    let (mut created, mut ended, mut kernel, mut user) = Default::default();
    unsafe {
        let _ = GetProcessTimes(GetCurrentProcess(), &mut created, &mut ended, &mut kernel, &mut user);
    }
    let ticks = |t: windows::Win32::Foundation::FILETIME| ((t.dwHighDateTime as u64) << 32 | t.dwLowDateTime as u64) as f64 / 10_000_000.0;
    ticks(kernel) + ticks(user)
}

pub fn record(path: &Path, height: u32, fps: u32, sound: (bool, bool), stop: &AtomicBool, limit: Option<Duration>) -> Result<Stats, String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let started = MFStartup(MF_VERSION, MFSTARTUP_LITE).map_err(|e| format!("Media Foundation no arrancó: {e}"));
        let done = started.and_then(|_| {
            let done = run(path, height, fps.clamp(10, 120), sound, stop, limit);
            let _ = MFShutdown();
            done
        });
        CoUninitialize();
        done
    }
}

trait Step<T> {
    fn at(self, what: &str) -> Result<T, String>;
}

impl<T> Step<T> for windows::core::Result<T> {
    fn at(self, what: &str) -> Result<T, String> {
        self.map_err(|e| format!("{what}: {}", e.message()))
    }
}

unsafe fn run(path: &Path, height: u32, fps: u32, sound: (bool, bool), stop: &AtomicBool, limit: Option<Duration>) -> Result<Stats, String> {
    let mut screen = open_screen().at("pantalla")?;
    let size = fit(screen.size, height);
    let video: ID3D11VideoDevice = screen.device.cast().at("video")?;
    let video_context: ID3D11VideoContext = screen.context.cast().at("video")?;
    let content = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
        InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
        InputFrameRate: DXGI_RATIONAL { Numerator: fps, Denominator: 1 },
        InputWidth: screen.size.0,
        InputHeight: screen.size.1,
        OutputFrameRate: DXGI_RATIONAL { Numerator: fps, Denominator: 1 },
        OutputWidth: size.0,
        OutputHeight: size.1,
        Usage: D3D11_VIDEO_USAGE_PLAYBACK_NORMAL,
    };
    let enumerator = video.CreateVideoProcessorEnumerator(&content).at("procesador")?;
    let processor = video.CreateVideoProcessor(&enumerator, 0).at("procesador")?;
    video_context.VideoProcessorSetStreamColorSpace(&processor, 0, &D3D11_VIDEO_PROCESSOR_COLOR_SPACE { _bitfield: 0 });
    video_context.VideoProcessorSetOutputColorSpace(&processor, &D3D11_VIDEO_PROCESSOR_COLOR_SPACE { _bitfield: (1 << 2) | (1 << 4) });
    video_context.VideoProcessorSetStreamAutoProcessingMode(&processor, 0, false);
    video_context.VideoProcessorSetStreamFrameFormat(&processor, 0, D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE);

    let still = D3D11_TEXTURE2D_DESC {
        Width: screen.size.0,
        Height: screen.size.1,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
        CPUAccessFlags: 0,
        MiscFlags: 0,
    };
    let mut last = None;
    screen.device.CreateTexture2D(&still, None, Some(&mut last)).at("textura")?;
    let last: ID3D11Texture2D = last.ok_or("textura")?;
    let input_view_desc = D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {
        FourCC: 0,
        ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
        Anonymous: D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0 { Texture2D: D3D11_TEX2D_VPIV { MipSlice: 0, ArraySlice: 0 } },
    };
    let mut input_view = None;
    video.CreateVideoProcessorInputView(&last, &enumerator, &input_view_desc, Some(&mut input_view)).at("vista de entrada")?;
    let input_view = input_view.ok_or("vista de entrada")?;

    let mut token = 0;
    let mut manager = None;
    MFCreateDXGIDeviceManager(&mut token, &mut manager).at("gestor")?;
    let manager = manager.ok_or("gestor")?;
    manager.ResetDevice(&screen.device, token).at("gestor")?;

    let nv12 = video_type(&MFVideoFormat_NV12, size, fps).at("tipo nv12")?;
    let mut allocator: Option<IMFVideoSampleAllocatorEx> = None;
    MFCreateVideoSampleAllocatorEx(&IMFVideoSampleAllocatorEx::IID, &mut allocator as *mut _ as *mut _).at("asignador")?;
    let allocator = allocator.ok_or("asignador")?;
    allocator.SetDirectXManager(&manager).at("asignador")?;
    let pool = attributes(&[(&MF_SA_D3D11_BINDFLAGS, (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_VIDEO_ENCODER.0) as u32), (&MF_SA_D3D11_USAGE, D3D11_USAGE_DEFAULT.0 as u32)]).at("asignador")?;
    if allocator.InitializeSampleAllocatorEx(4, 12, &pool, &nv12).is_err() {
        let pool = attributes(&[(&MF_SA_D3D11_BINDFLAGS, D3D11_BIND_RENDER_TARGET.0 as u32), (&MF_SA_D3D11_USAGE, D3D11_USAGE_DEFAULT.0 as u32)]).at("asignador")?;
        allocator.InitializeSampleAllocatorEx(4, 12, &pool, &nv12).at("asignador")?;
    }

    let writing = attributes(&[(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1)]).at("escritor")?;
    writing.SetUnknown(&MF_SINK_WRITER_D3D_MANAGER, &manager).at("escritor")?;
    let writer = MFCreateSinkWriterFromURL(&HSTRING::from(path.as_os_str()), None, &writing).at("escritor")?;
    let h264 = video_type(&MFVideoFormat_H264, size, fps).at("tipo h264")?;
    let rate = bitrate(size, fps);
    h264.SetUINT32(&MF_MT_AVG_BITRATE, rate).at("tipo h264")?;
    h264.SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_High.0 as u32).at("tipo h264")?;
    let stream = writer.AddStream(&h264).at("salida h264")?;
    let tuning = attributes(&[
        (&CODECAPI_AVEncCommonRateControlMode, eAVEncCommonRateControlMode_CBR.0 as u32),
        (&CODECAPI_AVEncCommonMeanBitRate, rate),
        (&CODECAPI_AVEncCommonQualityVsSpeed, 20),
        (&CODECAPI_AVEncMPVGOPSize, fps * 2),
    ]).at("ajustes")?;
    writer.SetInputMediaType(stream, &nv12, &tuning).at("entrada nv12 al codificador")?;
    let mut names = Vec::new();
    let mut ears = Ears::open(sound.0, sound.1, &mut names);
    let mut sound_stream = None;
    if ears.is_some() {
        let added = audio_type(&MFAudioFormat_AAC).and_then(|aac| writer.AddStream(&aac));
        match added.and_then(|index| audio_type(&MFAudioFormat_PCM).and_then(|pcm| writer.SetInputMediaType(index, &pcm, None)).map(|_| index)) {
            Ok(index) => sound_stream = Some(index),
            Err(e) => {
                names.push(format!("sin audio: {}", e.message()));
                ears = None;
            }
        }
    }

    let mut stats = Stats { grafica: screen.name.clone(), tamano: size, fps, audio: names, ..Default::default() };
    let mut encoder: Option<IMFTransform> = None;
    if writer.GetServiceForStream(stream, &GUID::zeroed(), &IMFTransform::IID, &mut encoder as *mut _ as *mut _).is_ok() {
        if let Some(about) = encoder.as_ref().and_then(|encoder| encoder.GetAttributes().ok()) {
            let mut name = [0u16; 256];
            let mut used = 0;
            if about.GetString(&MFT_FRIENDLY_NAME_Attribute, &mut name, Some(&mut used)).is_ok() {
                stats.codificador = String::from_utf16_lossy(&name[..used as usize]);
            }
            stats.por_hardware = about.GetUINT32(&MF_SA_D3D11_AWARE).unwrap_or(0) != 0 || about.GetItem(&MFT_ENUM_HARDWARE_URL_Attribute, None).is_ok();
        }
    }
    writer.BeginWriting().at("empezar")?;

    let mut views: Vec<(usize, u32, ID3D11VideoProcessorOutputView)> = Vec::new();
    let frame = 10_000_000i64 / fps as i64;
    let step = Duration::from_nanos(1_000_000_000 / fps as u64);
    let begin = Instant::now();
    let cpu_before = process_cpu();
    let mut spent = Duration::ZERO;
    let mut index: u64 = 0;
    loop {
        if stop.load(Ordering::Relaxed) || limit.is_some_and(|limit| begin.elapsed() >= limit) {
            break;
        }
        let due = begin + step * index as u32;
        let now = Instant::now();
        if now > due + step {
            let behind = ((now - due).as_nanos() / step.as_nanos()) as u64;
            stats.tarde += behind;
            index += behind;
            continue;
        }
        if due > now {
            std::thread::sleep(due - now);
        }
        let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut resource: Option<IDXGIResource> = None;
        match screen.duplication.AcquireNextFrame(0, &mut info, &mut resource) {
            Ok(()) => {
                if info.LastPresentTime != 0 {
                    if let Some(texture) = resource.as_ref().and_then(|resource| resource.cast::<ID3D11Texture2D>().ok()) {
                        screen.context.CopyResource(&last, &texture);
                        stats.nuevos += 1;
                    }
                }
                drop(resource);
                let _ = screen.duplication.ReleaseFrame();
            }
            Err(e) if e.code() == DXGI_ERROR_WAIT_TIMEOUT => {}
            Err(e) if e.code() == DXGI_ERROR_ACCESS_LOST => {
                if let Ok(again) = screen.output.DuplicateOutput(&screen.device) {
                    screen.duplication = again;
                }
                continue;
            }
            Err(e) => return Err(format!("captura: {}", e.message())),
        }
        let work = Instant::now();
        let sample = allocator.AllocateSample().at("muestra")?;
        let media = sample.GetBufferByIndex(0).at("muestra")?;
        if let Ok(full) = media.GetMaxLength() {
            let _ = media.SetCurrentLength(full);
        }
        let buffer: IMFDXGIBuffer = media.cast().at("muestra")?;
        let mut texture: Option<ID3D11Texture2D> = None;
        buffer.GetResource(&ID3D11Texture2D::IID, &mut texture as *mut _ as *mut _).at("muestra")?;
        let texture = texture.ok_or("muestra")?;
        let slice = buffer.GetSubresourceIndex().at("muestra")?;
        let key = texture.as_raw() as usize;
        let view = match views.iter().find(|(at, of, _)| *at == key && *of == slice) {
            Some((_, _, view)) => view.clone(),
            None => {
                let mut description = D3D11_TEXTURE2D_DESC::default();
                texture.GetDesc(&mut description);
                let view_desc = if description.ArraySize > 1 {
                    D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
                        ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2DARRAY,
                        Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 { Texture2DArray: D3D11_TEX2D_ARRAY_VPOV { MipSlice: 0, FirstArraySlice: slice, ArraySize: 1 } },
                    }
                } else {
                    D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
                        ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
                        Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 { Texture2D: D3D11_TEX2D_VPOV { MipSlice: 0 } },
                    }
                };
                let mut view = None;
                video.CreateVideoProcessorOutputView(&texture, &enumerator, &view_desc, Some(&mut view)).at("vista de salida")?;
                let view = view.ok_or("vista de salida")?;
                views.push((key, slice, view.clone()));
                view
            }
        };
        let streams = [D3D11_VIDEO_PROCESSOR_STREAM { Enable: BOOL::from(true), pInputSurface: ManuallyDrop::new(Some(input_view.clone())), ..Default::default() }];
        let blitted = video_context.VideoProcessorBlt(&processor, &view, 0, &streams);
        let [stream_state] = streams;
        drop(ManuallyDrop::into_inner(stream_state.pInputSurface));
        blitted.at("conversión")?;
        sample.SetSampleTime(index as i64 * frame).at("muestra")?;
        sample.SetSampleDuration(frame).at("muestra")?;
        writer.WriteSample(stream, &sample).at("codificar")?;
        if let (Some(ears), Some(index)) = (ears.as_mut(), sound_stream) {
            let first = ears.written;
            if let Some(pcm) = ears.take(begin.elapsed()) {
                writer.WriteSample(index, &audio_sample(&pcm, first).at("audio")?).at("audio")?;
            }
        }
        spent += work.elapsed();
        stats.cuadros += 1;
        index += 1;
    }
    if let (Some(ears), Some(index)) = (ears.as_mut(), sound_stream) {
        let first = ears.written;
        if let Some(pcm) = ears.take_with(Duration::from_secs_f64(index_seconds(index_frames(&stats), fps)), 0) {
            let _ = audio_sample(&pcm, first).and_then(|sample| writer.WriteSample(index, &sample));
        }
    }
    writer.Finalize().at("cerrar")?;
    stats.segundos = begin.elapsed().as_secs_f64();
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get()) as f64;
    stats.cpu_pct = (process_cpu() - cpu_before) / stats.segundos.max(0.001) / cores * 100.0;
    stats.ms_por_cuadro = spent.as_secs_f64() * 1000.0 / stats.cuadros.max(1) as f64;
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_keep_the_shape_and_stay_even() {
        assert_eq!(fit((1920, 1080), 720), (1280, 720));
        assert_eq!(fit((1920, 1080), 0), (1920, 1080));
        assert_eq!(fit((1366, 768), 720), (1280, 720));
        assert_eq!(fit((2560, 1080), 1080), (2560, 1080));
        assert_eq!(fit((1365, 767), 2000), (1364, 766));
        assert_eq!(bitrate((1920, 1080), 60), 14_929_920);
        assert_eq!(bitrate((640, 360), 30), 2_000_000);
        assert_eq!(soft(0.5), 0.5);
        assert!(soft(3.0) <= 1.0 && soft(3.0) > 0.99 && soft(-3.0) >= -1.0 && soft(0.95) < 0.95);
    }

    #[test]
    #[ignore]
    fn records_the_screen() {
        let path = std::env::var("COCUYO_GRABAR").unwrap_or_else(|_| std::env::temp_dir().join("cocuyo-prueba.mp4").display().to_string());
        let seconds = std::env::var("COCUYO_GRABAR_SEG").ok().and_then(|n| n.parse().ok()).unwrap_or(10);
        let stats = record(Path::new(&path), 1080, 60, (true, true), &AtomicBool::new(false), Some(Duration::from_secs(seconds))).unwrap();
        println!("[grabar] {}", serde_json::to_string(&stats).unwrap());
        assert!(stats.cuadros > 0);
    }
}
