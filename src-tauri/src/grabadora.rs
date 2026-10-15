// la grabadora: la pantalla se copia en la grafica, se pasa a nv12 ahi mismo y la codifica el chip de video. ningun cuadro toca la cpu
// y si la maquina no da, aprende sola a cuantos fps grabar

use std::collections::VecDeque;
use std::mem::ManuallyDrop;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use windows::core::{Interface, BOOL, HSTRING};
use windows::Win32::Foundation::{HMODULE, RECT};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_UNKNOWN, D3D11_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP};
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_MODE_ROTATION, DXGI_MODE_ROTATION_ROTATE180, DXGI_MODE_ROTATION_ROTATE270, DXGI_MODE_ROTATION_ROTATE90, DXGI_RATIONAL, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::Graphics::Gdi::{EnumDisplaySettingsW, DEVMODEW, ENUM_CURRENT_SETTINGS};
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};
use windows::Win32::System::Threading::{GetCurrentProcess, GetCurrentThread, GetProcessTimes, SetThreadPriority, THREAD_PRIORITY_ABOVE_NORMAL};

use crate::codificador::{Aac, Encoder, MfVideo, Pcm, Tuning};
use crate::mp4::{Codec, Mp4, Packet, Sound, TICKS};

#[derive(Debug, Default, Clone, Serialize)]
pub struct Stats {
    pub grafica: String,
    pub codificador: String,
    pub por_hardware: bool,
    pub tamano: (u32, u32),
    pub fps: u32,
    pub cuadros: u64,
    pub nuevos: u64,
    pub tarde: u64,
    pub ocupado: u64,
    pub ms_por_cuadro: f64,
    pub audio: Vec<String>,
    pub cpu_pct: f64,
    pub segundos: f64,
}

const PROFILES: [(&str, u32, u32, f64, u32); 4] = [
    ("ahorro", 720, 30, 0.07, 100),
    ("equilibrado", 1080, 60, 0.09, 100),
    ("alta", 0, 60, 0.12, 50),
    ("maxima", 0, 0, 0.16, 0),
];
const FPS_STEPS: [u32; 12] = [240, 165, 144, 120, 100, 90, 75, 60, 50, 40, 30, 24];
const MOST_FPS: u32 = 144;

#[derive(Clone, Debug, PartialEq)]
pub struct Wanted {
    pub monitor: String,
    pub profile: String,
    pub height: u32,
    pub fps: u32,
    pub kbps: u32,
    pub speed: u32,
    pub codec: Codec,
    pub system: Option<f32>,
    pub mic: Option<(String, f32)>,
    pub audio_kbps: u32,
    pub cursor: bool,
    pub replay: u32,
    pub noise: u8,
}

impl Wanted {
    pub fn from(settings: &crate::settings::Settings) -> Wanted {
        Wanted {
            monitor: settings.rec_monitor.clone(),
            profile: settings.rec_profile.clone(),
            height: settings.rec_height,
            fps: settings.rec_fps,
            kbps: settings.rec_kbps,
            speed: settings.rec_speed.min(100),
            codec: match settings.rec_codec.as_str() {
                "hevc" => Codec::Hevc,
                "av1" => Codec::Av1,
                _ => Codec::H264,
            },
            system: settings.rec_system.then_some(settings.rec_system_vol.min(200) as f32 / 100.0),
            mic: settings.rec_mic.then(|| (settings.rec_mic_id.clone(), settings.rec_mic_vol.min(200) as f32 / 100.0)),
            audio_kbps: settings.rec_audio_kbps,
            cursor: settings.rec_cursor,
            replay: settings.rec_replay.then_some(settings.rec_replay_secs.clamp(5, 600)).unwrap_or(0),
            noise: match settings.rec_noise.as_str() {
                "suave" => 1,
                "fuerte" => 2,
                _ => 0,
            },
        }
    }

    fn engine_part(&self) -> (String, String, u32, u32, u32, u32, Codec, bool, u32, bool) {
        (self.monitor.clone(), self.profile.clone(), self.height, self.fps, self.kbps, self.speed, self.codec, self.cursor, self.audio_kbps, self.system.is_some() || self.mic.is_some())
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Machine {
    pub screen: (u32, u32),
    pub refresh: u32,
    pub hardware: Vec<Codec>,
    pub integrated: bool,
    pub threads: u32,
    pub vram_mb: u64,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Plan {
    pub perfil: String,
    pub tamano: (u32, u32),
    pub alto: u32,
    pub fps: u32,
    pub kbps: u32,
    pub rapidez: u32,
    pub codec: Codec,
    pub hardware: bool,
    pub topes: Vec<String>,
    pub aprendido: Option<u32>,
    pub mb_por_minuto: f64,
}

pub fn auto_profile(machine: &Machine) -> &'static str {
    if !machine.hardware.contains(&Codec::H264) {
        "ahorro"
    } else if machine.integrated {
        "equilibrado"
    } else if machine.vram_mb >= 6000 && machine.threads >= 8 {
        "alta"
    } else {
        "equilibrado"
    }
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

pub fn bitrate(size: (u32, u32), fps: u32, bits_per_pixel: f64, codec: Codec) -> u32 {
    let thrift = if codec == Codec::H264 { 1.0 } else { 0.7 };
    (size.0 as f64 * size.1 as f64 * fps as f64 * bits_per_pixel * thrift).clamp(1_500_000.0, 60_000_000.0) as u32
}

pub fn plan(wanted: &Wanted, machine: &Machine, learned: Option<u32>) -> Plan {
    let chosen = if wanted.profile == "auto" || wanted.profile.is_empty() { auto_profile(machine) } else { wanted.profile.as_str() };
    let preset = PROFILES.iter().find(|p| p.0 == chosen).copied();
    let (mut height, mut fps, bits, speed) = match preset {
        Some((_, h, f, b, s)) => (h, f, b, s),
        None => (wanted.height, wanted.fps, 0.1, wanted.speed),
    };
    let refresh = if machine.refresh >= 24 { machine.refresh } else { 60 };
    if fps == 0 {
        fps = refresh.min(MOST_FPS);
    }
    let mut topes = Vec::new();
    let mut codec = wanted.codec;
    let hardware = machine.hardware.contains(&codec);
    if !hardware && codec != Codec::H264 {
        topes.push(format!("tu gráfica no codifica {} por hardware: va en H.264", if codec == Codec::Hevc { "HEVC" } else { "AV1" }));
        codec = Codec::H264;
    }
    let hardware = machine.hardware.contains(&codec);
    let screen_height = machine.screen.1.max(1);
    if height == 0 || height > screen_height {
        height = screen_height;
    }
    let mut cap = |max_height: u32, max_fps: u32, why: &str| {
        if height > max_height || fps > max_fps {
            topes.push(format!("{why}: {}p a {} fps", max_height.min(height), max_fps.min(fps)));
            height = height.min(max_height);
            fps = fps.min(max_fps);
        }
    };
    if !hardware {
        match machine.threads {
            0..=2 => cap(480, 30, "sin codificador en la gráfica y con 2 hilos"),
            3..=4 => cap(720, 30, "sin codificador en la gráfica y con 4 hilos"),
            5..=8 => cap(720, 60, "sin codificador en la gráfica"),
            _ => {}
        }
    } else if machine.integrated {
        cap(720, 60, "gráfica integrada");
    }
    if fps > refresh {
        fps = refresh;
    }
    let follows_learning = preset.is_some() || wanted.fps == 0;
    let aprendido = learned.filter(|l| follows_learning && *l < fps);
    if let Some(learned) = aprendido {
        fps = learned;
    }
    let size = fit(machine.screen, height);
    let kbps = if preset.is_none() && wanted.kbps > 0 { wanted.kbps } else { bitrate(size, fps, bits, codec) / 1000 };
    let audio = if wanted.system.is_some() || wanted.mic.is_some() { crate::codificador::aac_rate(wanted.audio_kbps) / 1000 } else { 0 };
    Plan {
        perfil: chosen.to_string(),
        tamano: size,
        alto: size.1,
        fps,
        kbps,
        rapidez: if hardware { speed } else { 100 },
        codec,
        hardware,
        topes,
        aprendido,
        mb_por_minuto: ((kbps + audio) as f64 * 60.0 / 8.0 / 1000.0 * 10.0).round() / 10.0,
    }
}

pub fn learn(fps: u32, stats: &Stats, clean_runs: u32) -> (Option<u32>, u32) {
    let total = stats.cuadros + stats.tarde;
    if stats.segundos < 5.0 || total == 0 {
        return (None, clean_runs);
    }
    let lost = stats.tarde as f64 / total as f64;
    if lost > 0.02 {
        let held = (stats.cuadros as f64 / stats.segundos) as u32;
        let lower = FPS_STEPS.iter().copied().find(|step| *step < fps && *step <= held.max(24)).unwrap_or(24);
        return (Some(lower), 0);
    }
    if lost < 0.003 && stats.segundos >= 15.0 && stats.ms_por_cuadro < 1000.0 / fps as f64 * 0.4 {
        if clean_runs + 1 >= 2 {
            let higher = FPS_STEPS.iter().rev().copied().find(|step| *step > fps).unwrap_or(fps);
            return (Some(higher), 0);
        }
        return (Some(fps), clean_runs + 1);
    }
    (Some(fps), 0)
}

#[derive(Clone, Debug, Serialize)]
pub struct Monitor {
    pub id: String,
    pub nombre: String,
    pub ancho: u32,
    pub alto: u32,
    pub hz: u32,
    pub principal: bool,
    pub grafica: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Grafica {
    pub nombre: String,
    pub marca: String,
    pub integrada: bool,
    pub memoria_mb: u64,
}

struct Card {
    adapter: IDXGIAdapter1,
    name: String,
    vendor: u32,
    integrated: bool,
    vram_mb: u64,
    luid: i64,
}

fn brand(vendor: u32) -> &'static str {
    match vendor {
        0x10DE => "NVIDIA",
        0x1002 | 0x1022 => "AMD",
        0x8086 => "Intel",
        _ => "",
    }
}

unsafe fn cards() -> Vec<(Card, Vec<IDXGIOutput>)> {
    let Ok(factory) = CreateDXGIFactory1::<IDXGIFactory1>() else { return Vec::new() };
    let mut out = Vec::new();
    let mut index = 0;
    while let Ok(adapter) = factory.EnumAdapters1(index) {
        index += 1;
        let Ok(about) = adapter.GetDesc1() else { continue };
        if about.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
            continue;
        }
        let name = String::from_utf16_lossy(&about.Description).trim_end_matches('\0').trim().to_string();
        let vram_mb = about.DedicatedVideoMemory as u64 / (1024 * 1024);
        let lower = name.to_lowercase();
        let integrated = (about.VendorId == 0x8086 && !lower.contains("arc")) || vram_mb < 1500 || lower.contains("vega 8") || lower.ends_with("radeon(tm) graphics") || lower.ends_with("radeon graphics");
        let luid = ((about.AdapterLuid.HighPart as i64) << 32) | about.AdapterLuid.LowPart as i64;
        let mut outputs = Vec::new();
        let mut slot = 0;
        while let Ok(output) = adapter.EnumOutputs(slot) {
            slot += 1;
            if output.GetDesc().is_ok_and(|d| d.AttachedToDesktop.as_bool()) {
                outputs.push(output);
            }
        }
        out.push((Card { adapter, name, vendor: about.VendorId, integrated, vram_mb, luid }, outputs));
    }
    out
}

fn device_name(about: &DXGI_OUTPUT_DESC) -> String {
    String::from_utf16_lossy(&about.DeviceName).trim_end_matches('\0').to_string()
}

fn refresh_of(device: &str) -> u32 {
    unsafe {
        let mut mode = DEVMODEW { dmSize: std::mem::size_of::<DEVMODEW>() as u16, ..Default::default() };
        if EnumDisplaySettingsW(&HSTRING::from(device), ENUM_CURRENT_SETTINGS, &mut mode).as_bool() {
            mode.dmDisplayFrequency
        } else {
            60
        }
    }
}

pub fn monitors() -> Vec<Monitor> {
    let mut out = Vec::new();
    unsafe {
        for (card, outputs) in cards() {
            for output in outputs {
                let Ok(about) = output.GetDesc() else { continue };
                let RECT { left, top, right, bottom } = about.DesktopCoordinates;
                let id = device_name(&about);
                let number = id.trim_start_matches("\\\\.\\DISPLAY").to_string();
                out.push(Monitor {
                    nombre: format!("Pantalla {number}"),
                    hz: refresh_of(&id),
                    id,
                    ancho: (right - left) as u32,
                    alto: (bottom - top) as u32,
                    principal: left == 0 && top == 0,
                    grafica: card.name.trim_end_matches(" Series").to_string(),
                });
            }
        }
    }
    out.sort_by_key(|m| (!m.principal, m.id.clone()));
    out
}

unsafe fn find_output(monitor: &str) -> Option<(Card, IDXGIOutput1, DXGI_OUTPUT_DESC)> {
    let mut first = None;
    for (card, outputs) in cards() {
        for output in outputs {
            let Ok(about) = output.GetDesc() else { continue };
            let Ok(output) = output.cast::<IDXGIOutput1>() else { continue };
            let wanted = !monitor.is_empty() && device_name(&about) == monitor;
            let primary = about.DesktopCoordinates.left == 0 && about.DesktopCoordinates.top == 0;
            if wanted {
                return Some((card, output, about));
            }
            if primary && first.is_none() {
                first = Some((Card { adapter: card.adapter.clone(), name: card.name.clone(), vendor: card.vendor, integrated: card.integrated, vram_mb: card.vram_mb, luid: card.luid }, output, about));
            }
        }
    }
    first
}

fn threads() -> u32 {
    std::thread::available_parallelism().map_or(1, |n| n.get() as u32)
}

fn processor() -> String {
    use windows::Win32::System::Registry::HKEY_LOCAL_MACHINE;
    let raw = crate::apps::reg_string(HKEY_LOCAL_MACHINE, "HARDWARE\\DESCRIPTION\\System\\CentralProcessor\\0", "ProcessorNameString").unwrap_or_default();
    tidy_cpu(&raw)
}

pub fn tidy_cpu(raw: &str) -> String {
    let clean = raw.replace("(R)", "").replace("(TM)", "").replace("(tm)", "");
    let clean = clean.split('@').next().unwrap_or("");
    let clean = clean.split(" with ").next().unwrap_or("");
    clean.split_whitespace().filter(|w| !matches!(*w, "CPU" | "Processor" | "Intel")).collect::<Vec<_>>().join(" ")
}

fn hardware_codecs(card: &Card) -> Vec<Codec> {
    let mut found: Vec<Codec> = [Codec::H264, Codec::Hevc, Codec::Av1].into_iter().filter(|codec| crate::codificador::has_hardware(*codec, Some(card.luid))).collect();
    let direct = (card.vendor == 0x1002 && crate::amf::Amf::available()) || (card.vendor == 0x10DE && crate::nvenc::Nvenc::available());
    if direct && !found.contains(&Codec::H264) {
        found.insert(0, Codec::H264);
    }
    found
}

pub fn machine(monitor: &str) -> Machine {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let Some((card, _, about)) = find_output(monitor) else {
            return Machine { screen: (1920, 1080), refresh: 60, threads: threads(), ..Default::default() };
        };
        let RECT { left, top, right, bottom } = about.DesktopCoordinates;
        Machine {
            screen: ((right - left) as u32, (bottom - top) as u32),
            refresh: refresh_of(&device_name(&about)),
            hardware: hardware_codecs(&card),
            integrated: card.integrated,
            threads: threads(),
            vram_mb: card.vram_mb,
        }
    }
}

fn memory_file(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_local_data_dir().ok()?.join("grabadora.json"))
}

fn memory(app: &AppHandle) -> serde_json::Value {
    memory_file(app).and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_else(|| serde_json::json!({}))
}

fn remember(app: &AppHandle, change: impl FnOnce(&mut serde_json::Value)) {
    let mut all = memory(app);
    change(&mut all);
    if let Some(path) = memory_file(app) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, serde_json::to_vec_pretty(&all).unwrap_or_default());
    }
}

fn learn_key(wanted: &Wanted, plan: &Plan) -> String {
    format!("{}|{}x{}|{:?}|{}", wanted.monitor, plan.tamano.0, plan.tamano.1, plan.codec, plan.hardware)
}

fn learned(app: &AppHandle, key: &str) -> Option<u32> {
    memory(app)["aprendido"][key].as_u64().map(|n| n as u32)
}

const RATE: u32 = 48_000;
const LAG: u64 = RATE as u64 / 10;
const MOST_QUEUED: usize = RATE as usize;

struct Ear {
    queue: VecDeque<f32>,
    left: crate::talk::Resampler,
    right: crate::talk::Resampler,
    rate: u32,
    gain: f32,
    clean: Option<crate::ruido::Denoiser>,
}

struct Ears {
    _streams: Vec<cpal::Stream>,
    arriving: Receiver<(usize, Vec<f32>)>,
    ears: Vec<Ear>,
    written: u64,
}

fn listen_to(device: &cpal::Device, who: usize, feed: Sender<(usize, Vec<f32>)>) -> Option<(cpal::Stream, u32)> {
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
            let _ = feed.send((who, stereo(&mut data.iter().copied())));
        }, |_| {}, None),
        cpal::SampleFormat::I16 => device.build_input_stream(config, move |data: &[i16], _: &_| {
            let _ = feed.send((who, stereo(&mut data.iter().map(|s| *s as f32 / 32768.0))));
        }, |_| {}, None),
        _ => return None,
    }
    .ok()?;
    stream.play().ok()?;
    Some((stream, rate))
}

impl Ears {
    fn open(system: Option<f32>, mic: Option<(String, f32)>, noise: u8, names: &mut Vec<String>) -> Option<Ears> {
        let host = cpal::default_host();
        let (feed, arriving) = channel();
        let mut streams = Vec::new();
        let mut ears = Vec::new();
        let chosen_mic = mic.as_ref().and_then(|(id, _)| {
            if id.is_empty() {
                return crate::mic::device(&host);
            }
            host.input_devices().ok().into_iter().flatten().find(|d| d.id().is_ok_and(|found| found.to_string() == *id)).or_else(|| crate::mic::device(&host))
        });
        let wanted = [(system, host.default_output_device(), "PC"), (mic.map(|(_, gain)| gain), chosen_mic, "micrófono")];
        for (gain, device, what) in wanted {
            let (Some(gain), Some(device)) = (gain, device) else { continue };
            if let Some((stream, rate)) = listen_to(&device, ears.len(), feed.clone()) {
                let name = device.description().map(|about| about.name().to_string()).unwrap_or_default();
                let clean = (what == "micrófono" && noise > 0).then(|| crate::ruido::Denoiser::new(noise > 1));
                names.push(format!("{what}: {name}{}", if clean.is_some() { " (sin ruido)" } else { "" }));
                streams.push(stream);
                ears.push(Ear { queue: VecDeque::new(), left: crate::talk::Resampler::new(rate as f64, RATE as f64), right: crate::talk::Resampler::new(rate as f64, RATE as f64), rate, gain, clean });
            }
        }
        (!ears.is_empty()).then(|| Ears { _streams: streams, arriving, ears, written: 0 })
    }

    fn take(&mut self, elapsed: Duration, lag: u64) -> Option<Vec<i16>> {
        while let Ok((who, samples)) = self.arriving.try_recv() {
            let ear = &mut self.ears[who];
            if let Some(clean) = ear.clean.as_mut() {
                let mono: Vec<f32> = samples.iter().step_by(2).copied().collect();
                let mono = if ear.rate == RATE { mono } else { ear.left.feed(&mono) };
                for sample in clean.process(&mono) {
                    ear.queue.push_back(sample);
                    ear.queue.push_back(sample);
                }
            } else if ear.rate == RATE {
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
        if frames < 1024 {
            return None;
        }
        let frames = frames / 1024 * 1024;
        let mut mixed = vec![0f32; frames * 2];
        for ear in &mut self.ears {
            let have = ear.queue.len().min(frames * 2);
            for (slot, sample) in mixed.iter_mut().zip(ear.queue.drain(..have)) {
                *slot += sample * ear.gain;
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

enum Voice {
    Aac(Aac),
    Pcm(Pcm),
}

impl Voice {
    fn sound(&self) -> Sound {
        match self {
            Voice::Aac(aac) => Sound::Aac(aac.bitrate),
            Voice::Pcm(_) => Sound::Pcm,
        }
    }

    fn encode(&mut self, pcm: &[i16], out: &mut Vec<Packet>) -> Result<(), String> {
        match self {
            Voice::Aac(aac) => aac.encode(pcm, out),
            Voice::Pcm(raw) => {
                raw.encode(pcm, out);
                Ok(())
            }
        }
    }
}

const VS: &[u8] = include_bytes!("../shaders/cursor_vs.cso");
const PS: &[u8] = include_bytes!("../shaders/cursor_ps.cso");

struct Pointer {
    vs: ID3D11VertexShader,
    ps: ID3D11PixelShader,
    sampler: ID3D11SamplerState,
    over: ID3D11BlendState,
    invert: ID3D11BlendState,
    color: Option<ID3D11ShaderResourceView>,
    xor: Option<ID3D11ShaderResourceView>,
    size: (u32, u32),
    at: (i32, i32),
    visible: bool,
    shape: Vec<u8>,
}

fn blend(device: &ID3D11Device, source: D3D11_BLEND, target: D3D11_BLEND) -> windows::core::Result<ID3D11BlendState> {
    let mut description = D3D11_BLEND_DESC::default();
    description.RenderTarget[0] = D3D11_RENDER_TARGET_BLEND_DESC {
        BlendEnable: true.into(),
        SrcBlend: source,
        DestBlend: target,
        BlendOp: D3D11_BLEND_OP_ADD,
        SrcBlendAlpha: D3D11_BLEND_ZERO,
        DestBlendAlpha: D3D11_BLEND_ONE,
        BlendOpAlpha: D3D11_BLEND_OP_ADD,
        RenderTargetWriteMask: D3D11_COLOR_WRITE_ENABLE_ALL.0 as u8,
    };
    let mut state = None;
    unsafe { device.CreateBlendState(&description, Some(&mut state))? };
    state.ok_or_else(windows::core::Error::empty)
}

fn picture(device: &ID3D11Device, pixels: &[u8], size: (u32, u32)) -> Option<ID3D11ShaderResourceView> {
    let description = D3D11_TEXTURE2D_DESC {
        Width: size.0,
        Height: size.1,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        Usage: D3D11_USAGE_IMMUTABLE,
        BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
        CPUAccessFlags: 0,
        MiscFlags: 0,
    };
    let data = D3D11_SUBRESOURCE_DATA { pSysMem: pixels.as_ptr() as *const _, SysMemPitch: size.0 * 4, SysMemSlicePitch: 0 };
    unsafe {
        let mut texture = None;
        device.CreateTexture2D(&description, Some(&data), Some(&mut texture)).ok()?;
        let mut view = None;
        device.CreateShaderResourceView(&texture?, None, Some(&mut view)).ok()?;
        view
    }
}

pub fn shape_pixels(kind: u32, width: u32, height: u32, pitch: u32, raw: &[u8]) -> (Vec<u8>, Option<Vec<u8>>, u32) {
    let pitch = pitch as usize;
    match kind {
        1 => {
            let rows = height / 2;
            let mut color = vec![0u8; (width * rows * 4) as usize];
            let mut xor = vec![0u8; color.len()];
            let mut inverts = false;
            for y in 0..rows as usize {
                for x in 0..width as usize {
                    let bit = 0x80 >> (x % 8);
                    let and = raw.get(y * pitch + x / 8).is_some_and(|b| b & bit != 0);
                    let flip = raw.get((y + rows as usize) * pitch + x / 8).is_some_and(|b| b & bit != 0);
                    let at = (y * width as usize + x) * 4;
                    match (and, flip) {
                        (false, false) => color[at..at + 4].copy_from_slice(&[0, 0, 0, 255]),
                        (false, true) => color[at..at + 4].copy_from_slice(&[255, 255, 255, 255]),
                        (true, true) => {
                            xor[at..at + 4].copy_from_slice(&[255, 255, 255, 255]);
                            inverts = true;
                        }
                        _ => {}
                    }
                }
            }
            (color, inverts.then_some(xor), rows)
        }
        4 => {
            let mut color = vec![0u8; (width * height * 4) as usize];
            let mut xor = vec![0u8; color.len()];
            let mut inverts = false;
            for y in 0..height as usize {
                for x in 0..width as usize {
                    let from = y * pitch + x * 4;
                    let Some(px) = raw.get(from..from + 4) else { continue };
                    let at = (y * width as usize + x) * 4;
                    if px[3] == 0 {
                        color[at..at + 4].copy_from_slice(&[px[0], px[1], px[2], 255]);
                    } else if px[0] | px[1] | px[2] != 0 {
                        xor[at..at + 4].copy_from_slice(&[px[0], px[1], px[2], 255]);
                        inverts = true;
                    }
                }
            }
            (color, inverts.then_some(xor), height)
        }
        _ => {
            let mut color = vec![0u8; (width * height * 4) as usize];
            for y in 0..height as usize {
                for x in 0..width as usize {
                    let from = y * pitch + x * 4;
                    let Some(px) = raw.get(from..from + 4) else { continue };
                    let a = px[3] as u32;
                    let at = (y * width as usize + x) * 4;
                    color[at..at + 4].copy_from_slice(&[(px[0] as u32 * a / 255) as u8, (px[1] as u32 * a / 255) as u8, (px[2] as u32 * a / 255) as u8, px[3]]);
                }
            }
            (color, None, height)
        }
    }
}

impl Pointer {
    fn new(device: &ID3D11Device) -> Option<Pointer> {
        unsafe {
            let mut vs = None;
            device.CreateVertexShader(VS, None, Some(&mut vs)).ok()?;
            let mut ps = None;
            device.CreatePixelShader(PS, None, Some(&mut ps)).ok()?;
            let sampler_desc = D3D11_SAMPLER_DESC {
                Filter: D3D11_FILTER_MIN_MAG_MIP_POINT,
                AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
                AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
                AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
                MaxLOD: f32::MAX,
                ..Default::default()
            };
            let mut sampler = None;
            device.CreateSamplerState(&sampler_desc, Some(&mut sampler)).ok()?;
            Some(Pointer {
                vs: vs?,
                ps: ps?,
                sampler: sampler?,
                over: blend(device, D3D11_BLEND_ONE, D3D11_BLEND_INV_SRC_ALPHA).ok()?,
                invert: blend(device, D3D11_BLEND_INV_DEST_COLOR, D3D11_BLEND_INV_SRC_COLOR).ok()?,
                color: None,
                xor: None,
                size: (0, 0),
                at: (0, 0),
                visible: false,
                shape: Vec::new(),
            })
        }
    }

    unsafe fn update(&mut self, device: &ID3D11Device, duplication: &IDXGIOutputDuplication, info: &DXGI_OUTDUPL_FRAME_INFO) -> bool {
        let mut changed = false;
        if info.LastMouseUpdateTime != 0 {
            let visible = info.PointerPosition.Visible.as_bool();
            let at = (info.PointerPosition.Position.x, info.PointerPosition.Position.y);
            changed |= visible != self.visible || (visible && at != self.at);
            self.visible = visible;
            if visible {
                self.at = at;
            }
        }
        if info.PointerShapeBufferSize > 0 {
            self.shape.resize(info.PointerShapeBufferSize as usize, 0);
            let mut needed = 0;
            let mut about = DXGI_OUTDUPL_POINTER_SHAPE_INFO::default();
            if duplication.GetFramePointerShape(self.shape.len() as u32, self.shape.as_mut_ptr() as *mut _, &mut needed, &mut about).is_ok() && about.Width > 0 && about.Height > 0 {
                let (color, xor, rows) = shape_pixels(about.Type, about.Width, about.Height, about.Pitch, &self.shape);
                self.size = (about.Width, rows);
                self.color = picture(device, &color, self.size);
                self.xor = xor.and_then(|xor| picture(device, &xor, self.size));
                changed = true;
            }
        }
        changed
    }

    unsafe fn draw(&self, context: &ID3D11DeviceContext, target: &ID3D11RenderTargetView) {
        let Some(color) = &self.color else { return };
        if !self.visible {
            return;
        }
        context.OMSetRenderTargets(Some(&[Some(target.clone())]), None);
        let viewport = D3D11_VIEWPORT { TopLeftX: self.at.0 as f32, TopLeftY: self.at.1 as f32, Width: self.size.0 as f32, Height: self.size.1 as f32, MinDepth: 0.0, MaxDepth: 1.0 };
        context.RSSetViewports(Some(&[viewport]));
        context.IASetInputLayout(None);
        context.IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP);
        context.VSSetShader(&self.vs, None);
        context.PSSetShader(&self.ps, None);
        context.PSSetSamplers(0, Some(&[Some(self.sampler.clone())]));
        context.PSSetShaderResources(0, Some(&[Some(color.clone())]));
        context.OMSetBlendState(&self.over, None, u32::MAX);
        context.Draw(4, 0);
        if let Some(xor) = &self.xor {
            context.PSSetShaderResources(0, Some(&[Some(xor.clone())]));
            context.OMSetBlendState(&self.invert, None, u32::MAX);
            context.Draw(4, 0);
        }
        context.PSSetShaderResources(0, Some(&[None]));
        context.OMSetRenderTargets(None, None);
    }
}

struct Screen {
    card: Card,
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    duplication: IDXGIOutputDuplication,
    output: IDXGIOutput1,
    source: (u32, u32),
    rotation: DXGI_MODE_ROTATION,
}

unsafe fn open_screen(monitor: &str) -> Result<Screen, String> {
    let (card, output, _) = find_output(monitor).ok_or("no encontré la pantalla")?;
    let mut device = None;
    let mut context = None;
    let flags = D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT;
    D3D11CreateDevice(&card.adapter, D3D_DRIVER_TYPE_UNKNOWN, HMODULE::default(), flags, None, D3D11_SDK_VERSION, Some(&mut device), None, Some(&mut context)).map_err(|e| format!("gráfica: {}", e.message()))?;
    let (Some(device), Some(context)) = (device, context) else { return Err("gráfica".into()) };
    let _ = device.cast::<ID3D11Multithread>().map(|m| m.SetMultithreadProtected(true));
    let duplication = output.DuplicateOutput(&device).map_err(|e| format!("captura: {}", e.message()))?;
    let about = duplication.GetDesc();
    Ok(Screen { card, device, context, duplication, output, source: (about.ModeDesc.Width, about.ModeDesc.Height), rotation: about.Rotation })
}

fn rotated(size: (u32, u32), rotation: DXGI_MODE_ROTATION) -> (u32, u32) {
    if rotation == DXGI_MODE_ROTATION_ROTATE90 || rotation == DXGI_MODE_ROTATION_ROTATE270 {
        (size.1, size.0)
    } else {
        size
    }
}

fn process_cpu() -> f64 {
    let (mut created, mut ended, mut kernel, mut user) = Default::default();
    unsafe {
        let _ = GetProcessTimes(GetCurrentProcess(), &mut created, &mut ended, &mut kernel, &mut user);
    }
    let ticks = |t: windows::Win32::Foundation::FILETIME| ((t.dwHighDateTime as u64) << 32 | t.dwLowDateTime as u64) as f64 / 10_000_000.0;
    ticks(kernel) + ticks(user)
}

trait Step<T> {
    fn at(self, what: &str) -> Result<T, String>;
}

impl<T> Step<T> for windows::core::Result<T> {
    fn at(self, what: &str) -> Result<T, String> {
        self.map_err(|e| format!("{what}: {}", e.message()))
    }
}

fn probe_key(kind: &str, codec: Codec, card: &Card) -> String {
    let version = unsafe { card.adapter.CheckInterfaceSupport(&IDXGIDevice::IID) }.unwrap_or(0);
    format!("{kind}-{codec:?}-{:x}-{version:x}", card.luid)
}

fn probed(app: Option<&AppHandle>, kind: &str, codec: Codec, card: &Card) -> bool {
    let Some(app) = app else { return true };
    let key = probe_key(kind, codec, card);
    if let Some(known) = memory(app)["pruebas"][&key].as_bool() {
        return known;
    }
    let passed = std::env::current_exe()
        .ok()
        .and_then(|exe| {
            use std::os::windows::process::CommandExt;
            let mut child = std::process::Command::new(exe)
                .args(["--probar-codificador", kind, &format!("{codec:?}"), &card.luid.to_string()])
                .creation_flags(0x0800_0000)
                .spawn()
                .ok()?;
            let until = Instant::now() + Duration::from_secs(15);
            loop {
                if let Ok(Some(status)) = child.try_wait() {
                    return Some(status.success());
                }
                if Instant::now() > until {
                    let _ = child.kill();
                    return Some(false);
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        })
        .unwrap_or(false);
    remember(app, |all| all["pruebas"][&key] = serde_json::json!(passed));
    passed
}

unsafe fn open_encoder(app: Option<&AppHandle>, screen: &Screen, manager: Option<&IMFDXGIDeviceManager>, tuning: Tuning, notes: &mut Vec<String>) -> Result<Box<dyn Encoder>, String> {
    let direct = tuning.codec != Codec::Av1 && !(cfg!(debug_assertions) && std::env::var_os("COCUYO_SIN_DIRECTO").is_some());
    if direct && screen.card.vendor == 0x1002 && crate::amf::Amf::available() && probed(app, "amf", tuning.codec, &screen.card) {
        match crate::amf::Amf::open(&screen.device, tuning) {
            Ok(encoder) => return Ok(Box::new(encoder)),
            Err(e) => notes.push(e),
        }
    }
    if direct && screen.card.vendor == 0x10DE && crate::nvenc::Nvenc::available() && probed(app, "nvenc", tuning.codec, &screen.card) {
        match crate::nvenc::Nvenc::open(&screen.device, tuning) {
            Ok(encoder) => return Ok(Box::new(encoder)),
            Err(e) => notes.push(e),
        }
    }
    if let Some(manager) = manager {
        match MfVideo::open(&screen.device, manager, Some(screen.card.luid), tuning, true) {
            Ok(encoder) => return Ok(Box::new(encoder)),
            Err(e) => notes.push(e),
        }
        if tuning.codec == Codec::H264 {
            match MfVideo::open(&screen.device, manager, None, tuning, false) {
                Ok(encoder) => return Ok(Box::new(encoder)),
                Err(e) => notes.push(e),
            }
        }
    }
    Err(format!("no hay con qué codificar: {}", notes.join("; ")))
}

pub fn probe_main(args: &[String]) -> i32 {
    let kind = args.first().map(String::as_str).unwrap_or("");
    let codec = if args.get(1).is_some_and(|c| c == "Hevc") { Codec::Hevc } else { Codec::H264 };
    let luid: i64 = args.get(2).and_then(|l| l.parse().ok()).unwrap_or(0);
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let Some((card, _)) = cards().into_iter().find(|(card, _)| card.luid == luid) else { return 2 };
        let mut device = None;
        let mut context = None;
        if D3D11CreateDevice(&card.adapter, D3D_DRIVER_TYPE_UNKNOWN, HMODULE::default(), D3D11_CREATE_DEVICE_VIDEO_SUPPORT, None, D3D11_SDK_VERSION, Some(&mut device), None, Some(&mut context)).is_err() {
            return 3;
        }
        let Some(device) = device else { return 3 };
        let tuning = Tuning { codec, size: (640, 360), fps: 30, bitrate: 2_000_000, speed: 100 };
        let mut encoder: Box<dyn Encoder> = match kind {
            "amf" => match crate::amf::Amf::open(&device, tuning) {
                Ok(e) => Box::new(e),
                Err(_) => return 4,
            },
            "nvenc" => match crate::nvenc::Nvenc::open(&device, tuning) {
                Ok(e) => Box::new(e),
                Err(_) => return 4,
            },
            _ => return 5,
        };
        let mut packets = Vec::new();
        for i in 0..6i64 {
            match encoder.input() {
                Ok(Some(input)) => {
                    if encoder.submit(input, i * TICKS / 30, TICKS / 30).is_err() {
                        return 6;
                    }
                }
                _ => return 6,
            }
            let _ = encoder.collect(&mut packets);
        }
        encoder.finish(&mut packets);
        let good = packets.iter().any(|p| p.key && crate::mp4::Video::from_key(codec, (640, 360), &p.data).is_some());
        if good { 0 } else { 7 }
    }
}

struct Ring {
    seconds: u32,
    video: VecDeque<Packet>,
    audio: VecDeque<Packet>,
}

impl Ring {
    fn push_video(&mut self, packet: Packet) {
        let newest = packet.pts;
        self.video.push_back(packet);
        let limit = newest - self.seconds as i64 * TICKS;
        loop {
            let next = self.video.iter().skip(1).position(|p| p.key).map(|i| i + 1);
            match next {
                Some(i) if self.video[i].pts <= limit => {
                    self.video.drain(..i);
                }
                _ => break,
            }
        }
        if let Some(first) = self.video.front().map(|p| p.pts) {
            while self.audio.front().is_some_and(|a| a.pts < first - TICKS) {
                self.audio.pop_front();
            }
        }
    }

    fn from_last_key(&self) -> (Vec<Packet>, Vec<Packet>) {
        let Some(at) = self.video.iter().rposition(|p| p.key) else { return (Vec::new(), Vec::new()) };
        let start = self.video[at].pts;
        (self.video.iter().skip(at).cloned().collect(), self.audio.iter().filter(|a| a.pts >= start).cloned().collect())
    }
}

enum Write {
    Video(Packet),
    Audio(Packet),
}

struct FileSink {
    path: PathBuf,
    feed: Sender<Write>,
    writer: std::thread::JoinHandle<Result<f64, String>>,
    zero: Option<i64>,
}

impl FileSink {
    fn open(path: PathBuf, codec: Codec, size: (u32, u32), fps: u32, sound: Option<Sound>) -> Result<FileSink, String> {
        let mut mp4 = Mp4::create(&path, codec, size, fps, sound, true).map_err(|e| format!("No pude crear el archivo: {e}"))?;
        let (feed, inbox) = channel::<Write>();
        let writer = std::thread::Builder::new()
            .name("grabadora-disco".into())
            .spawn(move || {
                for item in inbox {
                    let done = match item {
                        Write::Video(p) => mp4.video(&p),
                        Write::Audio(p) => mp4.audio(&p),
                    };
                    done.map_err(|e| format!("disco: {e}"))?;
                }
                mp4.finish().map_err(|e| format!("disco: {e}"))
            })
            .map_err(|e| e.to_string())?;
        Ok(FileSink { path, feed, writer, zero: None })
    }

    fn video(&mut self, packet: &Packet) {
        if self.zero.is_none() {
            if !packet.key {
                return;
            }
            self.zero = Some(packet.pts);
        }
        let zero = self.zero.unwrap_or(0);
        let _ = self.feed.send(Write::Video(Packet { pts: packet.pts - zero, ..packet.clone() }));
    }

    fn audio(&mut self, packet: &Packet) {
        if let Some(zero) = self.zero.filter(|zero| packet.pts >= *zero) {
            let _ = self.feed.send(Write::Audio(Packet { pts: packet.pts - zero, ..packet.clone() }));
        }
    }

    fn close(self) -> Result<(PathBuf, f64), String> {
        drop(self.feed);
        let seconds = self.writer.join().map_err(|_| "el disco se cayó".to_string())??;
        Ok((self.path, seconds))
    }
}

enum Order {
    Record(PathBuf, Sender<Result<(), String>>),
    StopRecord(Sender<Result<(PathBuf, f64), String>>),
    Save(PathBuf, Sender<Result<(), String>>),
    Quit,
}

struct Engine {
    orders: Sender<Order>,
    thread: std::thread::JoinHandle<Result<Stats, String>>,
    wanted: Wanted,
    recording: Option<(PathBuf, u64)>,
}

static ENGINE: Mutex<Option<Engine>> = Mutex::new(None);
static STATE: Mutex<Option<Estado>> = Mutex::new(None);
static APP: std::sync::OnceLock<AppHandle> = std::sync::OnceLock::new();

#[derive(Clone, Serialize, Default)]
pub struct Estado {
    pub grabando: bool,
    pub desde: u64,
    pub repeticion: bool,
    pub archivo: Option<String>,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

fn estado_of(engine: Option<&Engine>) -> Estado {
    match engine {
        Some(engine) => Estado {
            grabando: engine.recording.is_some(),
            desde: engine.recording.as_ref().map_or(0, |(_, since)| *since),
            repeticion: engine.wanted.replay > 0,
            archivo: engine.recording.as_ref().map(|(path, _)| path.display().to_string()),
        },
        None => Estado::default(),
    }
}

fn announce() {
    let state = estado_of(ENGINE.lock().unwrap().as_ref());
    *STATE.lock().unwrap() = Some(state.clone());
    if let Some(app) = APP.get() {
        let _ = app.emit_to(crate::overlay::LABEL, "grabadora", state);
    }
}

fn hide_overlay(app: &AppHandle, hide: bool) {
    use windows::Win32::UI::WindowsAndMessaging::{SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE, WDA_NONE};
    if let Some(window) = app.get_webview_window(crate::overlay::LABEL) {
        if let Ok(hwnd) = window.hwnd() {
            unsafe {
                let _ = SetWindowDisplayAffinity(windows::Win32::Foundation::HWND(hwnd.0 as _), if hide { WDA_EXCLUDEFROMCAPTURE } else { WDA_NONE });
            }
        }
    }
}

struct Run<'a> {
    app: Option<&'a AppHandle>,
    wanted: Wanted,
    plan: Plan,
    orders: Receiver<Order>,
    limit: Option<Duration>,
}

fn run(job: Run) -> Result<Stats, String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let media = MFStartup(MF_VERSION, MFSTARTUP_LITE).is_ok();
        let done = engine(job, media);
        if media {
            let _ = MFShutdown();
        }
        CoUninitialize();
        done
    }
}

unsafe fn engine(job: Run, media: bool) -> Result<Stats, String> {
    let Run { app, wanted, plan, orders, limit } = job;
    let mut screen = open_screen(&wanted.monitor)?;
    let fps = plan.fps.clamp(10, 240);
    let shown_size = rotated(screen.source, screen.rotation);
    let size = fit(shown_size, plan.alto);
    let video: ID3D11VideoDevice = screen.device.cast().at("video")?;
    let video_context: ID3D11VideoContext = screen.context.cast().at("video")?;
    let content = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
        InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
        InputFrameRate: DXGI_RATIONAL { Numerator: fps, Denominator: 1 },
        InputWidth: screen.source.0,
        InputHeight: screen.source.1,
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
    let turn = match screen.rotation {
        DXGI_MODE_ROTATION_ROTATE90 => Some(D3D11_VIDEO_PROCESSOR_ROTATION_90),
        DXGI_MODE_ROTATION_ROTATE180 => Some(D3D11_VIDEO_PROCESSOR_ROTATION_180),
        DXGI_MODE_ROTATION_ROTATE270 => Some(D3D11_VIDEO_PROCESSOR_ROTATION_270),
        _ => None,
    };
    if let Some(turn) = turn {
        video_context.VideoProcessorSetStreamRotation(&processor, 0, true, turn);
    }

    let canvas = D3D11_TEXTURE2D_DESC {
        Width: screen.source.0,
        Height: screen.source.1,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
        CPUAccessFlags: 0,
        MiscFlags: 0,
    };
    let make = |what: &str| -> Result<ID3D11Texture2D, String> {
        let mut texture = None;
        screen.device.CreateTexture2D(&canvas, None, Some(&mut texture)).at(what)?;
        texture.ok_or_else(|| what.to_string())
    };
    let last = make("textura")?;
    let input_desc = D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {
        FourCC: 0,
        ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
        Anonymous: D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0 { Texture2D: D3D11_TEX2D_VPIV { MipSlice: 0, ArraySlice: 0 } },
    };
    let view_of = |texture: &ID3D11Texture2D| -> Result<ID3D11VideoProcessorInputView, String> {
        let mut view = None;
        video.CreateVideoProcessorInputView(texture, &enumerator, &input_desc, Some(&mut view)).at("vista")?;
        view.ok_or_else(|| "vista".to_string())
    };
    let last_view = view_of(&last)?;
    let mut pointer = if wanted.cursor && turn.is_none() { Pointer::new(&screen.device) } else { None };
    let mut shown = None;
    if pointer.is_some() {
        let texture = make("textura")?;
        let mut target = None;
        screen.device.CreateRenderTargetView(&texture, None, Some(&mut target)).at("cursor")?;
        shown = Some((view_of(&texture)?, target.ok_or("cursor")?, texture));
    }

    let manager = if media {
        let mut token = 0;
        let mut manager = None;
        MFCreateDXGIDeviceManager(&mut token, &mut manager).at("gestor")?;
        let manager = manager.ok_or("gestor")?;
        manager.ResetDevice(&screen.device, token).at("gestor")?;
        Some(manager)
    } else {
        None
    };
    let tuning = Tuning { codec: plan.codec, size, fps, bitrate: plan.kbps * 1000, speed: plan.rapidez };
    let mut notes = Vec::new();
    let mut encoder = open_encoder(app, &screen, manager.as_ref(), tuning, &mut notes)?;
    if encoder.hardware() {
        let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_ABOVE_NORMAL);
    }
    let mut names = Vec::new();
    let mut ears = Ears::open(wanted.system, wanted.mic.clone(), wanted.noise, &mut names);
    let mut voice = None;
    if ears.is_some() {
        voice = match media.then(|| Aac::open(wanted.audio_kbps)) {
            Some(Ok(aac)) => Some(Voice::Aac(aac)),
            other => {
                if let Some(Err(e)) = other {
                    names.push(format!("sin AAC ({e}): sonido sin comprimir"));
                }
                Some(Voice::Pcm(Pcm::new()))
            }
        };
    }
    let sound = voice.as_ref().map(Voice::sound);
    let mut stats = Stats { grafica: screen.card.name.clone(), codificador: encoder.name(), por_hardware: encoder.hardware(), tamano: size, fps, audio: names, ..Default::default() };
    if !notes.is_empty() && cfg!(debug_assertions) {
        eprintln!("[grabar] respaldos: {}", notes.join(" | "));
    }

    let mut ring = (wanted.replay > 0).then(|| Ring { seconds: wanted.replay, video: VecDeque::new(), audio: VecDeque::new() });
    let mut file: Option<FileSink> = None;
    let mut views: Vec<(usize, u32, ID3D11VideoProcessorOutputView)> = Vec::new();
    let frame = TICKS / fps as i64;
    let step = Duration::from_nanos(1_000_000_000 / fps as u64);
    let begin = Instant::now();
    let cpu_before = process_cpu();
    let mut spent = Duration::ZERO;
    let mut index: u64 = 0;
    let mut packets = Vec::new();
    let mut sounds = Vec::new();
    let mut dirty = true;
    let mut quitting = false;
    let mut fatal = None;
    loop {
        while let Ok(order) = orders.try_recv() {
            match order {
                Order::Record(path, reply) => {
                    let opened = FileSink::open(path, plan.codec, size, fps, sound);
                    match opened {
                        Ok(mut sink) => {
                            if let Some(ring) = &ring {
                                let (video, audio) = ring.from_last_key();
                                for p in &video {
                                    sink.video(p);
                                }
                                for p in &audio {
                                    sink.audio(p);
                                }
                            }
                            file = Some(sink);
                            let _ = reply.send(Ok(()));
                        }
                        Err(e) => {
                            let _ = reply.send(Err(e));
                        }
                    }
                }
                Order::StopRecord(reply) => {
                    let closed = file.take().map(FileSink::close).unwrap_or_else(|| Err("No estaba grabando.".into()));
                    let _ = reply.send(closed);
                }
                Order::Save(path, reply) => match &ring {
                    Some(ring) if !ring.video.is_empty() => {
                        let video: Vec<Packet> = ring.video.iter().cloned().collect();
                        let audio: Vec<Packet> = ring.audio.iter().cloned().collect();
                        let codec = plan.codec;
                        let _ = std::thread::Builder::new().name("grabadora-clip".into()).spawn(move || {
                            let done = crate::mp4::write_clip(&path, codec, size, fps, sound, &video, &audio).map(|_| ()).map_err(|e| e.to_string());
                            let _ = reply.send(done);
                        });
                    }
                    _ => {
                        let _ = reply.send(Err("Todavía no hay nada guardado en la repetición.".into()));
                    }
                },
                Order::Quit => quitting = true,
            }
        }
        if quitting || limit.is_some_and(|limit| begin.elapsed() >= limit) {
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
                        dirty = true;
                    }
                }
                if let Some(pointer) = pointer.as_mut() {
                    dirty |= pointer.update(&screen.device, &screen.duplication, &info);
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
            Err(e) => {
                fatal = Some(format!("captura: {}", e.message()));
                break;
            }
        }
        let work = Instant::now();
        let source = match (&pointer, &shown) {
            (Some(pointer), Some((view, target, texture))) if pointer.visible => {
                if dirty {
                    screen.context.CopyResource(texture, &last);
                    pointer.draw(&screen.context, target);
                    dirty = false;
                }
                view
            }
            _ => &last_view,
        };
        let input = match encoder.input() {
            Ok(Some(input)) => input,
            Ok(None) => {
                stats.ocupado += 1;
                stats.tarde += 1;
                index += 1;
                let _ = encoder.collect(&mut packets);
                route(&mut packets, &mut sounds, &mut ring, &mut file);
                continue;
            }
            Err(e) => {
                fatal = Some(e);
                break;
            }
        };
        let key = input.texture.as_raw() as usize;
        let view = match views.iter().find(|(at, of, _)| *at == key && *of == input.slice) {
            Some((_, _, view)) => view.clone(),
            None => {
                let mut description = D3D11_TEXTURE2D_DESC::default();
                input.texture.GetDesc(&mut description);
                let view_desc = if description.ArraySize > 1 {
                    D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
                        ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2DARRAY,
                        Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 { Texture2DArray: D3D11_TEX2D_ARRAY_VPOV { MipSlice: 0, FirstArraySlice: input.slice, ArraySize: 1 } },
                    }
                } else {
                    D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC { ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D, Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 { Texture2D: D3D11_TEX2D_VPOV { MipSlice: 0 } } }
                };
                let mut view = None;
                video.CreateVideoProcessorOutputView(&input.texture, &enumerator, &view_desc, Some(&mut view)).at("vista de salida")?;
                let view = view.ok_or("vista de salida")?;
                views.push((key, input.slice, view.clone()));
                view
            }
        };
        let streams = [D3D11_VIDEO_PROCESSOR_STREAM { Enable: BOOL::from(true), pInputSurface: ManuallyDrop::new(Some(source.clone())), ..Default::default() }];
        let blitted = video_context.VideoProcessorBlt(&processor, &view, 0, &streams);
        let [stream_state] = streams;
        drop(ManuallyDrop::into_inner(stream_state.pInputSurface));
        if let Err(e) = blitted {
            fatal = Some(format!("conversión: {}", e.message()));
            break;
        }
        if let Err(e) = encoder.submit(input, index as i64 * frame, frame) {
            fatal = Some(e);
            break;
        }
        if let Err(e) = encoder.collect(&mut packets) {
            fatal = Some(e);
            break;
        }
        let mut deaf = false;
        if let (Some(ears), Some(voice)) = (ears.as_mut(), voice.as_mut()) {
            if let Some(pcm) = ears.take(begin.elapsed(), LAG) {
                if let Err(e) = voice.encode(&pcm, &mut sounds) {
                    stats.audio.push(e);
                    deaf = true;
                }
            }
        }
        if deaf {
            ears = None;
        }
        route(&mut packets, &mut sounds, &mut ring, &mut file);
        spent += work.elapsed();
        stats.cuadros += 1;
        index += 1;
    }
    encoder.finish(&mut packets);
    if let (Some(ears), Some(voice)) = (ears.as_mut(), voice.as_mut()) {
        let end = Duration::from_nanos(index * 1_000_000_000 / fps as u64);
        if let Some(pcm) = ears.take(end, 0) {
            let _ = voice.encode(&pcm, &mut sounds);
        }
        if let Voice::Aac(aac) = voice {
            aac.finish(&mut sounds);
        }
    }
    route(&mut packets, &mut sounds, &mut ring, &mut file);
    let closed = file.take().map(FileSink::close);
    drop(orders);
    stats.segundos = begin.elapsed().as_secs_f64();
    stats.cpu_pct = (process_cpu() - cpu_before) / stats.segundos.max(0.001) / threads() as f64 * 100.0;
    stats.ms_por_cuadro = spent.as_secs_f64() * 1000.0 / stats.cuadros.max(1) as f64;
    if let Some(Err(e)) = closed {
        return Err(e);
    }
    match fatal {
        Some(e) => Err(e),
        None => Ok(stats),
    }
}

fn route(packets: &mut Vec<Packet>, sounds: &mut Vec<Packet>, ring: &mut Option<Ring>, file: &mut Option<FileSink>) {
    for packet in packets.drain(..) {
        if let Some(file) = file.as_mut() {
            file.video(&packet);
        }
        if let Some(ring) = ring.as_mut() {
            ring.push_video(packet);
        }
    }
    for packet in sounds.drain(..) {
        if let Some(file) = file.as_mut() {
            file.audio(&packet);
        }
        if let Some(ring) = ring.as_mut() {
            ring.audio.push_back(packet);
        }
    }
}

pub fn record(path: &Path, wanted: Wanted, seconds: u64) -> Result<Stats, String> {
    let machine = machine(&wanted.monitor);
    let plan = plan(&wanted, &machine, None);
    let (orders, inbox) = channel();
    let (reply, answer) = channel();
    let _ = orders.send(Order::Record(path.to_path_buf(), reply));
    let job = Run { app: None, wanted, plan, orders: inbox, limit: Some(Duration::from_secs(seconds)) };
    let done = run(job);
    answer.try_recv().unwrap_or(Ok(()))?;
    drop(orders);
    done
}

fn dir_for(app: &AppHandle) -> Result<PathBuf, String> {
    let settings = crate::settings::load(app);
    let dir = crate::settings::record_dir(app, &settings).ok_or("No encontré la carpeta de videos.")?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("No pude crear {}: {e}", dir.display()))?;
    Ok(dir)
}

fn stamp(prefix: &str) -> String {
    let now = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    format!("{prefix} {:04}-{:02}-{:02} {:02}-{:02}-{:02}.mp4", now.wYear, now.wMonth, now.wDay, now.wHour, now.wMinute, now.wSecond)
}

fn spawn_engine(app: &AppHandle, wanted: Wanted) -> Result<Engine, String> {
    let machine = machine(&wanted.monitor);
    let key_plan = plan(&wanted, &machine, None);
    let key = learn_key(&wanted, &key_plan);
    let plan = plan(&wanted, &machine, learned(app, &key));
    let (orders, inbox) = channel();
    let handle = app.clone();
    let job_wanted = wanted.clone();
    let thread = std::thread::Builder::new()
        .name("grabadora".into())
        .spawn(move || {
            hide_overlay(&handle, true);
            let fps = plan.fps;
            let done = run(Run { app: Some(&handle), wanted: job_wanted, plan, orders: inbox, limit: None });
            hide_overlay(&handle, false);
            if let Ok(stats) = &done {
                let clean = memory(&handle)["limpias"][&key].as_u64().unwrap_or(0) as u32;
                let (next, clean) = learn(fps, stats, clean);
                remember(&handle, |all| {
                    if let Some(next) = next {
                        all["aprendido"][&key] = serde_json::json!(next);
                    }
                    all["limpias"][&key] = serde_json::json!(clean);
                    all["ultima"] = serde_json::to_value(stats).unwrap_or_default();
                });
                if cfg!(debug_assertions) {
                    eprintln!("[grabar] {}", serde_json::to_string(stats).unwrap_or_default());
                }
            }
            let ended_alone = {
                let mut engine = ENGINE.lock().unwrap();
                let alone = engine.as_ref().is_some_and(|e| e.thread.thread().id() == std::thread::current().id());
                if alone {
                    *engine = None;
                }
                alone
            };
            if ended_alone {
                if let Err(e) = &done {
                    let _ = handle.emit_to(crate::overlay::LABEL, "note", crate::i18n::tr(&format!("La grabadora se detuvo: {e}")));
                }
                announce();
            }
            done
        })
        .map_err(|e| e.to_string())?;
    Ok(Engine { orders, thread, wanted, recording: None })
}

fn stop_engine(engine: Engine) -> Result<Stats, String> {
    let _ = engine.orders.send(Order::Quit);
    engine.thread.join().map_err(|_| "La grabadora se cayó.".to_string())?
}

pub fn start(app: &AppHandle) -> Result<String, String> {
    let _ = APP.set(app.clone());
    let path = dir_for(app)?.join(stamp("Grabación"));
    let mut slot = ENGINE.lock().unwrap();
    if slot.as_ref().is_some_and(|e| e.recording.is_some()) {
        return Err("Ya estoy grabando.".into());
    }
    if slot.as_ref().is_some_and(|e| e.thread.is_finished()) {
        *slot = None;
    }
    if slot.is_none() {
        let wanted = Wanted::from(&crate::settings::load(app));
        *slot = Some(spawn_engine(app, wanted)?);
    }
    let engine = slot.as_mut().ok_or("La grabadora no arrancó.")?;
    let (reply, answer) = channel();
    engine.orders.send(Order::Record(path.clone(), reply)).map_err(|_| "La grabadora no arrancó.".to_string())?;
    match answer.recv_timeout(Duration::from_secs(20)) {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return Err(e),
        Err(_) => {
            let broken = slot.take();
            drop(slot);
            let failed = broken.map(stop_engine);
            announce();
            return Err(match failed {
                Some(Err(e)) => e,
                _ => "La grabadora no respondió.".into(),
            });
        }
    }
    engine.recording = Some((path.clone(), now_ms()));
    drop(slot);
    announce();
    Ok(path.display().to_string())
}

pub fn stop(app: &AppHandle) -> Result<String, String> {
    let _ = APP.set(app.clone());
    let mut slot = ENGINE.lock().unwrap();
    let engine = slot.as_mut().filter(|e| e.recording.is_some()).ok_or("No estaba grabando.")?;
    let (reply, answer) = channel();
    let _ = engine.orders.send(Order::StopRecord(reply));
    let closed = answer.recv_timeout(Duration::from_secs(30)).map_err(|_| "La grabadora no respondió.".to_string()).and_then(|r| r);
    engine.recording = None;
    let finished = if engine.wanted.replay == 0 { slot.take() } else { None };
    drop(slot);
    if let Some(engine) = finished {
        let _ = stop_engine(engine);
    }
    announce();
    closed.map(|(path, _)| path.display().to_string())
}

pub fn recording() -> bool {
    ENGINE.lock().unwrap().as_ref().is_some_and(|e| e.recording.is_some() && !e.thread.is_finished())
}

pub fn save_last(app: &AppHandle) -> Result<String, String> {
    let path = dir_for(app)?.join(stamp("Repetición"));
    let slot = ENGINE.lock().unwrap();
    let engine = slot.as_ref().filter(|e| e.wanted.replay > 0).ok_or("Enciende la repetición instantánea en Ajustes → Capturas.")?;
    let (reply, answer) = channel();
    engine.orders.send(Order::Save(path.clone(), reply)).map_err(|_| "La grabadora no responde.".to_string())?;
    drop(slot);
    answer.recv_timeout(Duration::from_secs(60)).map_err(|_| "Guardar tardó demasiado.".to_string())??;
    Ok(path.display().to_string())
}

pub fn configure(app: &AppHandle) {
    let _ = APP.set(app.clone());
    let wanted = Wanted::from(&crate::settings::load(app));
    let app = app.clone();
    std::thread::spawn(move || {
        let mut slot = ENGINE.lock().unwrap();
        let busy = slot.as_ref().is_some_and(|e| e.recording.is_some());
        let same = slot.as_ref().is_some_and(|e| e.wanted.engine_part() == wanted.engine_part() && e.wanted.replay == wanted.replay && e.wanted.system == wanted.system && e.wanted.mic == wanted.mic && e.wanted.noise == wanted.noise);
        if busy || same {
            return;
        }
        if let Some(engine) = slot.take() {
            drop(slot);
            let _ = stop_engine(engine);
            slot = ENGINE.lock().unwrap();
            if slot.is_some() {
                return;
            }
        }
        if wanted.replay > 0 {
            match spawn_engine(&app, wanted) {
                Ok(engine) => *slot = Some(engine),
                Err(e) => {
                    let _ = app.emit_to(crate::overlay::LABEL, "note", crate::i18n::tr(&format!("La repetición instantánea no arrancó: {e}")));
                }
            }
        }
        drop(slot);
        announce();
    });
}

#[derive(Serialize)]
pub struct Equipo {
    pub graficas: Vec<Grafica>,
    pub procesador: String,
    pub hilos: u32,
    pub monitores: Vec<Monitor>,
    pub codecs: Vec<Codec>,
    pub automatico: String,
    pub plan: Plan,
    pub codificador: String,
    pub ultima: serde_json::Value,
}

#[tauri::command]
pub async fn grabar_equipo(app: AppHandle) -> Equipo {
    tauri::async_runtime::spawn_blocking(move || {
        let settings = crate::settings::load(&app);
        let wanted = Wanted::from(&settings);
        let machine = machine(&wanted.monitor);
        let key = learn_key(&wanted, &plan(&wanted, &machine, None));
        let chosen = plan(&wanted, &machine, learned(&app, &key));
        let graficas = unsafe { cards() }
            .into_iter()
            .map(|(card, _)| Grafica { marca: brand(card.vendor).into(), nombre: card.name.trim_end_matches(" Series").to_string(), integrada: card.integrated, memoria_mb: card.vram_mb })
            .fold(Vec::<Grafica>::new(), |mut all, card| {
                if !all.iter().any(|seen| seen.nombre == card.nombre && seen.memoria_mb == card.memoria_mb) {
                    all.push(card);
                }
                all
            });
        let card = unsafe { find_output(&wanted.monitor) }.map(|(card, _, _)| card);
        let codificador = match &card {
            Some(card) if card.vendor == 0x1002 && crate::amf::Amf::available() => "AMF (AMD)".to_string(),
            Some(card) if card.vendor == 0x10DE && crate::nvenc::Nvenc::available() => "NVENC (NVIDIA)".to_string(),
            Some(card) if card.vendor == 0x8086 && !machine.hardware.is_empty() => "Quick Sync (Intel)".to_string(),
            _ if !machine.hardware.is_empty() => "el de tu gráfica".to_string(),
            _ => "el procesador".to_string(),
        };
        if let Some(card) = &card {
            for codec in [Codec::H264, Codec::Hevc] {
                if card.vendor == 0x1002 && crate::amf::Amf::available() && machine.hardware.contains(&codec) {
                    probed(Some(&app), "amf", codec, card);
                }
                if card.vendor == 0x10DE && crate::nvenc::Nvenc::available() && machine.hardware.contains(&codec) {
                    probed(Some(&app), "nvenc", codec, card);
                }
            }
        }
        let mut codecs = machine.hardware.clone();
        if !codecs.contains(&Codec::H264) {
            codecs.insert(0, Codec::H264);
        }
        Equipo {
            graficas,
            procesador: processor(),
            hilos: machine.threads,
            monitores: monitors(),
            codecs,
            automatico: auto_profile(&machine).into(),
            plan: chosen,
            codificador,
            ultima: memory(&app)["ultima"].clone(),
        }
    })
    .await
    .unwrap_or_else(|_| Equipo { graficas: Vec::new(), procesador: String::new(), hilos: threads(), monitores: Vec::new(), codecs: vec![Codec::H264], automatico: "equilibrado".into(), plan: plan(&Wanted::from(&crate::settings::Settings::default()), &Machine::default(), None), codificador: String::new(), ultima: serde_json::Value::Null })
}

#[tauri::command]
pub async fn grabar_empezar(app: AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || start(&app)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn grabar_parar(app: AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || stop(&app)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn grabar_ultimo(app: AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || save_last(&app)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn grabar_estado() -> Estado {
    STATE.lock().unwrap().clone().unwrap_or_default()
}

pub fn toggle(app: &AppHandle) -> String {
    let said = if recording() {
        stop(app).map(|file| format!("Grabación guardada: {}", Path::new(&file).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(file)))
    } else {
        start(app).map(|_| "Grabando la pantalla. Pulsa el mismo atajo para parar.".to_string())
    };
    said.unwrap_or_else(|why| why)
}

pub fn replay_hotkey(app: &AppHandle) -> String {
    save_last(app).map(|file| format!("Guardé lo último: {}", Path::new(&file).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(file))).unwrap_or_else(|why| why)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wanted(profile: &str) -> Wanted {
        Wanted { monitor: String::new(), profile: profile.into(), height: 0, fps: 0, kbps: 0, speed: 100, codec: Codec::H264, system: Some(1.0), mic: None, audio_kbps: 160, cursor: true, replay: 0, noise: 0 }
    }

    fn rx580() -> Machine {
        Machine { screen: (1920, 1080), refresh: 60, hardware: vec![Codec::H264, Codec::Hevc], integrated: false, threads: 4, vram_mb: 8192 }
    }

    #[test]
    fn sizes_keep_the_shape_and_stay_even() {
        assert_eq!(fit((1920, 1080), 720), (1280, 720));
        assert_eq!(fit((1920, 1080), 0), (1920, 1080));
        assert_eq!(fit((1366, 768), 720), (1280, 720));
        assert_eq!(fit((2560, 1080), 1080), (2560, 1080));
        assert_eq!(fit((1365, 767), 2000), (1364, 766));
        assert_eq!(bitrate((640, 360), 30, 0.07, Codec::H264), 1_500_000);
        assert_eq!(soft(0.5), 0.5);
        assert_eq!(tidy_cpu("Intel(R) Core(TM) i3-7100 CPU @ 3.90GHz"), "Core i3-7100");
        assert_eq!(tidy_cpu("AMD Ryzen 5 5600G with Radeon Graphics"), "AMD Ryzen 5 5600G");
        assert!(soft(3.0) <= 1.0 && soft(3.0) > 0.99 && soft(-3.0) >= -1.0 && soft(0.95) < 0.95);
    }

    #[test]
    fn the_automatic_profile_reads_the_machine() {
        let mine = plan(&wanted("auto"), &rx580(), None);
        assert_eq!(mine.perfil, "equilibrado");
        assert_eq!((mine.tamano, mine.fps), ((1920, 1080), 60));
        assert!(mine.hardware && mine.topes.is_empty());
        let strong = Machine { threads: 16, ..rx580() };
        assert_eq!(plan(&wanted("auto"), &strong, None).perfil, "alta");
        let laptop = Machine { integrated: true, vram_mb: 512, ..rx580() };
        let small = plan(&wanted("auto"), &laptop, None);
        assert_eq!(small.tamano, (1280, 720));
        assert!(!small.topes.is_empty());
    }

    #[test]
    fn without_a_video_chip_it_records_less() {
        let old = Machine { hardware: vec![], threads: 4, ..rx580() };
        let capped = plan(&wanted("maxima"), &old, None);
        assert_eq!((capped.alto, capped.fps), (720, 30));
        assert!(!capped.hardware && capped.rapidez == 100);
        let tiny = Machine { hardware: vec![], threads: 2, ..rx580() };
        assert_eq!(plan(&wanted("alta"), &tiny, None).alto, 480);
        let hevc = Wanted { codec: Codec::Av1, ..wanted("alta") };
        let fallback = plan(&hevc, &rx580(), None);
        assert_eq!(fallback.codec, Codec::H264);
        assert!(fallback.topes[0].contains("AV1"));
    }

    #[test]
    fn it_learns_the_fps_it_can_hold() {
        let mut stats = Stats { segundos: 30.0, cuadros: 1700, tarde: 100, ms_por_cuadro: 2.0, ..Default::default() };
        assert_eq!(learn(60, &stats, 0), (Some(50), 0));
        stats.cuadros = 900;
        assert_eq!(learn(60, &stats, 0), (Some(30), 0));
        stats.tarde = 0;
        assert_eq!(learn(50, &stats, 0), (Some(50), 1));
        assert_eq!(learn(50, &stats, 1), (Some(60), 0));
        assert_eq!(learn(120, &stats, 1), (Some(144), 0));
        let learned = plan(&wanted("equilibrado"), &rx580(), Some(50));
        assert_eq!((learned.fps, learned.aprendido), (50, Some(50)));
        let fast = Machine { refresh: 144, ..rx580() };
        assert_eq!(plan(&wanted("maxima"), &fast, None).fps, 144);
        assert_eq!(plan(&wanted("maxima"), &rx580(), None).fps, 60);
        assert_eq!(plan(&wanted("maxima"), &fast, Some(120)).fps, 120);
        let fixed = Wanted { profile: "propio".into(), height: 1080, fps: 60, kbps: 12_000, ..wanted("propio") };
        assert_eq!(plan(&fixed, &rx580(), Some(48)).fps, 60);
        assert_eq!(plan(&fixed, &rx580(), None).kbps, 12_000);
    }

    #[test]
    fn cursor_shapes_become_pictures() {
        let raw = [0b1000_0000u8, 0b0100_0000, 0b0000_0000, 0b0100_0000];
        let (color, xor, rows) = shape_pixels(1, 2, 4, 1, &raw);
        assert_eq!(rows, 2);
        assert_eq!(&color[4..8], &[0, 0, 0, 255]);
        assert_eq!(color[3], 0);
        assert_eq!(&color[8..12], &[0, 0, 0, 255]);
        assert_eq!(&color[12..16], &[0, 0, 0, 0]);
        assert_eq!(&xor.unwrap()[12..16], &[255, 255, 255, 255]);
        let (color, _, _) = shape_pixels(2, 1, 1, 4, &[200, 100, 50, 128]);
        assert_eq!(color, vec![100, 50, 25, 128]);
    }

    #[test]
    fn the_ring_keeps_whole_seconds_from_a_key_frame() {
        let mut ring = Ring { seconds: 2, video: VecDeque::new(), audio: VecDeque::new() };
        for i in 0..300i64 {
            ring.push_video(Packet { data: vec![0u8].into(), pts: i * TICKS / 60, key: i % 60 == 0 });
        }
        let first = ring.video.front().unwrap();
        assert!(first.key);
        let span = ring.video.back().unwrap().pts - first.pts;
        assert!(span >= 2 * TICKS && span < 3 * TICKS, "{span}");
        let (video, _) = ring.from_last_key();
        assert!(video[0].key && video.len() == 60);
    }

    #[test]
    #[ignore]
    fn records_the_screen() {
        let path = std::env::var("COCUYO_GRABAR").unwrap_or_else(|_| std::env::temp_dir().join("cocuyo-prueba.mp4").display().to_string());
        let seconds = std::env::var("COCUYO_GRABAR_SEG").ok().and_then(|n| n.parse().ok()).unwrap_or(10);
        let codec = if std::env::var("COCUYO_GRABAR_HEVC").is_ok() { Codec::Hevc } else { Codec::H264 };
        let stats = record(Path::new(&path), Wanted { codec, ..wanted("equilibrado") }, seconds).unwrap();
        println!("[grabar] {}", serde_json::to_string(&stats).unwrap());
        assert!(stats.cuadros > 0);
    }
}
