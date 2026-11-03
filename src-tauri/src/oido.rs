// el oido del pc para la grabadora: wasapi en loopback con el formato exacto que da windows, cualquier numero de canales,
// y si windows cambia de parlante (audifonos bluetooth, hdmi, enchufar algo) se pasa solo al nuevo. sin esto quedaba grabando silencio

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};

pub type Feed = Sender<(usize, u32, Vec<f32>)>;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    F32,
    I16,
    I24,
    I32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    kind: Kind,
    channels: usize,
    block: usize,
    rate: u32,
    left: Vec<(usize, f32)>,
    right: Vec<(usize, f32)>,
}

const FL: u32 = 0x1;
const FR: u32 = 0x2;
const FC: u32 = 0x4;
const BL: u32 = 0x10;
const BR: u32 = 0x20;
const FLC: u32 = 0x40;
const FRC: u32 = 0x80;
const BC: u32 = 0x100;
const SL: u32 = 0x200;
const SR: u32 = 0x400;
const PLAIN: [u32; 8] = [FL, FR, FC, 0x8, BL, BR, SL, SR];

impl Layout {
    pub fn new(tag: u16, bits: u16, block: u16, channels: u16, rate: u32, sub: u32, mask: u32) -> Option<Layout> {
        let float = tag == 3 || (tag == 0xFFFE && sub == 3);
        let kind = match (float, bits) {
            (true, 32) => Kind::F32,
            (false, 16) => Kind::I16,
            (false, 24) => Kind::I24,
            (false, 32) => Kind::I32,
            _ => return None,
        };
        let channels = channels as usize;
        let block = block as usize;
        if channels == 0 || rate == 0 || block < channels * bits as usize / 8 {
            return None;
        }
        if channels == 1 {
            return Some(Layout { kind, channels, block, rate, left: vec![(0, 1.0)], right: vec![(0, 1.0)] });
        }
        let speakers: Vec<u32> = if mask.count_ones() as usize >= channels { (0..32).map(|bit| 1u32 << bit).filter(|bit| mask & bit != 0).take(channels).collect() } else { (0..channels).map(|i| PLAIN.get(i).copied().unwrap_or(0)).collect() };
        let (mut left, mut right) = (Vec::new(), Vec::new());
        for (at, speaker) in speakers.into_iter().enumerate() {
            match speaker {
                FL => left.push((at, 1.0)),
                FR => right.push((at, 1.0)),
                FC => {
                    left.push((at, 0.707));
                    right.push((at, 0.707));
                }
                BL | SL | FLC => left.push((at, 0.707)),
                BR | SR | FRC => right.push((at, 0.707)),
                BC => {
                    left.push((at, 0.5));
                    right.push((at, 0.5));
                }
                _ => {}
            }
        }
        if left.is_empty() || right.is_empty() {
            left = vec![(0, 1.0)];
            right = vec![(1, 1.0)];
        }
        if channels > 2 {
            left.iter_mut().chain(right.iter_mut()).for_each(|(_, weight)| *weight *= 0.75);
        }
        Some(Layout { kind, channels, block, rate, left, right })
    }

    fn sample(&self, frame: &[u8], channel: usize) -> f32 {
        match self.kind {
            Kind::F32 => frame.get(channel * 4..channel * 4 + 4).map_or(0.0, |b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
            Kind::I16 => frame.get(channel * 2..channel * 2 + 2).map_or(0.0, |b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0),
            Kind::I24 => frame.get(channel * 3..channel * 3 + 3).map_or(0.0, |b| (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f32 / 8_388_608.0),
            Kind::I32 => frame.get(channel * 4..channel * 4 + 4).map_or(0.0, |b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32 / 2_147_483_648.0),
        }
    }

    pub fn stereo(&self, bytes: &[u8], out: &mut Vec<f32>) {
        out.reserve(bytes.len() / self.block * 2);
        for frame in bytes.chunks_exact(self.block) {
            let mix = |side: &[(usize, f32)]| side.iter().map(|(at, weight)| self.sample(frame, *at) * weight).sum::<f32>();
            out.push(mix(&self.left));
            out.push(mix(&self.right));
        }
    }
}

struct Tap {
    client: IAudioClient,
    capture: IAudioCaptureClient,
    event: HANDLE,
    layout: Layout,
    id: String,
}

impl Drop for Tap {
    fn drop(&mut self) {
        unsafe {
            let _ = self.client.Stop();
            let _ = CloseHandle(self.event);
        }
    }
}

unsafe fn default_id(devices: &IMMDeviceEnumerator) -> Option<String> {
    let device = devices.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
    let id = device.GetId().ok()?;
    let text = id.to_string().ok();
    CoTaskMemFree(Some(id.0 as *const _));
    text
}

unsafe fn open(devices: &IMMDeviceEnumerator) -> windows::core::Result<Tap> {
    let device = devices.GetDefaultAudioEndpoint(eRender, eConsole)?;
    let id = {
        let raw = device.GetId()?;
        let text = raw.to_string().unwrap_or_default();
        CoTaskMemFree(Some(raw.0 as *const _));
        text
    };
    let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
    let format = client.GetMixFormat()?;
    let plain = std::ptr::read_unaligned(format);
    let (sub, mask) = if plain.wFormatTag == 0xFFFE && plain.cbSize >= 22 {
        let wide = std::ptr::read_unaligned(format as *const WAVEFORMATEXTENSIBLE);
        let sub = wide.SubFormat;
        (sub.data1, wide.dwChannelMask)
    } else {
        (0, 0)
    };
    let layout = Layout::new(plain.wFormatTag, plain.wBitsPerSample, plain.nBlockAlign, plain.nChannels, plain.nSamplesPerSec, sub, mask);
    let started = client.Initialize(AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_EVENTCALLBACK, 2_000_000, 0, format, None);
    CoTaskMemFree(Some(format as *const _));
    started?;
    let layout = layout.ok_or_else(windows::core::Error::empty)?;
    let event = CreateEventW(None, false, false, None)?;
    let tap = Tap { capture: client.GetService()?, client, event, layout, id };
    tap.client.SetEventHandle(tap.event)?;
    tap.client.Start()?;
    Ok(tap)
}

impl Tap {
    unsafe fn drain(&self, who: usize, feed: &Feed, scratch: &mut Vec<f32>) -> windows::core::Result<()> {
        loop {
            if self.capture.GetNextPacketSize()? == 0 {
                return Ok(());
            }
            let mut data = std::ptr::null_mut();
            let (mut frames, mut flags) = (0u32, 0u32);
            self.capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
            scratch.clear();
            if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null() {
                scratch.resize(frames as usize * 2, 0.0);
            } else {
                self.layout.stereo(std::slice::from_raw_parts(data, frames as usize * self.layout.block), scratch);
            }
            self.capture.ReleaseBuffer(frames)?;
            if !scratch.is_empty() {
                let _ = feed.send((who, self.layout.rate, std::mem::take(scratch)));
            }
        }
    }
}

pub struct Oido {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Oido {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub fn listen(who: usize, feed: Feed) -> Option<Oido> {
    let stop = Arc::new(AtomicBool::new(false));
    let (ready, opened) = std::sync::mpsc::channel();
    let quit = stop.clone();
    let thread = std::thread::Builder::new()
        .name("grabadora-oido-pc".into())
        .spawn(move || unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            run(who, &feed, &quit, ready);
            CoUninitialize();
        })
        .ok()?;
    let oido = Oido { stop, thread: Some(thread) };
    opened.recv_timeout(Duration::from_secs(3)).unwrap_or(false).then_some(oido)
}

unsafe fn run(who: usize, feed: &Feed, stop: &AtomicBool, ready: Sender<bool>) {
    let Ok(devices) = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL) else {
        let _ = ready.send(false);
        return;
    };
    let mut ready = Some(ready);
    let mut misses = 0u64;
    let mut scratch = Vec::new();
    while !stop.load(Ordering::Relaxed) {
        let tap = match open(&devices) {
            Ok(tap) => tap,
            Err(_) => {
                if let Some(ready) = ready.take() {
                    let _ = ready.send(false);
                    return;
                }
                misses += 1;
                let until = Instant::now() + Duration::from_millis((250 * misses).min(3000));
                while Instant::now() < until && !stop.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(50));
                }
                continue;
            }
        };
        if let Some(ready) = ready.take() {
            let _ = ready.send(true);
        }
        misses = 0;
        let mut looked = Instant::now();
        loop {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            let woke = WaitForSingleObject(tap.event, 100) == WAIT_OBJECT_0;
            if tap.drain(who, feed, &mut scratch).is_err() {
                break;
            }
            if (!woke || looked.elapsed() > Duration::from_millis(500)) && looked.elapsed() > Duration::from_millis(250) {
                looked = Instant::now();
                if default_id(&devices).is_some_and(|now| now != tap.id) {
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(samples: &[f32]) -> Vec<u8> {
        samples.iter().flat_map(|s| s.to_le_bytes()).collect()
    }

    #[test]
    fn plain_stereo_passes_through() {
        let layout = Layout::new(0xFFFE, 32, 8, 2, 48_000, 3, FL | FR).unwrap();
        let mut out = Vec::new();
        layout.stereo(&bytes(&[0.5, -0.25, 0.1, 0.2]), &mut out);
        assert_eq!(out, vec![0.5, -0.25, 0.1, 0.2]);
    }

    #[test]
    fn seven_one_headsets_still_sound_on_both_sides() {
        let mask = FL | FR | FC | 0x8 | BL | BR | SL | SR;
        let layout = Layout::new(0xFFFE, 32, 32, 8, 48_000, 3, mask).unwrap();
        let mut out = Vec::new();
        layout.stereo(&bytes(&[0.4, 0.0, 0.0, 0.9, 0.0, 0.0, 0.0, 0.0]), &mut out);
        assert!((out[0] - 0.3).abs() < 1e-4 && out[1].abs() < 1e-6, "{out:?}");
        out.clear();
        layout.stereo(&bytes(&[0.0, 0.0, 0.5, 0.9, 0.0, 0.0, 0.0, 0.0]), &mut out);
        assert!((out[0] - out[1]).abs() < 1e-6 && out[0] > 0.2, "la voz del centro va a los dos lados: {out:?}");
    }

    #[test]
    fn without_a_mask_it_assumes_the_usual_order() {
        let layout = Layout::new(0xFFFE, 32, 24, 6, 48_000, 3, 0).unwrap();
        let mut out = Vec::new();
        layout.stereo(&bytes(&[0.0, 0.8, 0.0, 0.0, 0.0, 0.0]), &mut out);
        assert!(out[0].abs() < 1e-6 && out[1] > 0.5, "{out:?}");
    }

    #[test]
    fn reads_integer_formats_and_mono() {
        let layout = Layout::new(1, 16, 2, 1, 44_100, 0, 0).unwrap();
        let mut out = Vec::new();
        layout.stereo(&16384i16.to_le_bytes(), &mut out);
        assert_eq!(out, vec![0.5, 0.5]);
        let layout = Layout::new(0xFFFE, 24, 6, 2, 48_000, 1, FL | FR).unwrap();
        out.clear();
        let half: [u8; 3] = [0x00, 0x00, 0x40];
        layout.stereo(&[half, [0x00, 0x00, 0xC0]].concat(), &mut out);
        assert!((out[0] - 0.5).abs() < 1e-6 && (out[1] + 0.5).abs() < 1e-6, "{out:?}");
        assert!(Layout::new(0xFFFE, 64, 16, 2, 48_000, 3, FL | FR).is_none());
    }

    #[test]
    #[ignore]
    fn hears_the_pc() {
        let (feed, heard) = std::sync::mpsc::channel();
        let oido = listen(0, feed).expect("no abrió el parlante de Windows");
        let until = Instant::now() + Duration::from_secs(3);
        let mut samples = 0usize;
        let mut peak = 0f32;
        while Instant::now() < until {
            if let Ok((_, rate, chunk)) = heard.recv_timeout(Duration::from_millis(200)) {
                assert!(rate >= 8000);
                samples += chunk.len();
                peak = chunk.iter().fold(peak, |p, s| p.max(s.abs()));
            }
        }
        drop(oido);
        println!("[oido] {samples} muestras, pico {peak:.3}");
    }
}
