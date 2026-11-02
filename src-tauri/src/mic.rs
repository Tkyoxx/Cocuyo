// el microfono. solo se abre mientras mantienes el atajo

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use tauri::{AppHandle, Emitter};

use crate::overlay;

pub struct Recording {
    pub samples: Vec<f32>,
    pub rate: u32,
}

enum Command {
    Start,
    Discard,
    Stop(Sender<Result<Recording, String>>),
}

static TX: OnceLock<Sender<Command>> = OnceLock::new();
static LEVEL: AtomicU32 = AtomicU32::new(0);
static TAP: Mutex<Option<Sender<(u32, Vec<f32>)>>> = Mutex::new(None);
static RATE: AtomicU32 = AtomicU32::new(0);
static RECENT: Mutex<Vec<f32>> = Mutex::new(Vec::new());
const WINDOW: usize = 1024;
const BANDS: [f32; 9] = [120.0, 200.0, 320.0, 500.0, 800.0, 1250.0, 2000.0, 3150.0, 5000.0];

pub fn bands(samples: &[f32], rate: u32) -> [f32; 9] {
    let mut out = [0f32; 9];
    let n = samples.len();
    if n < 64 || rate == 0 {
        return out;
    }
    let hann: Vec<f32> = (0..n).map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / (n - 1) as f32).cos()).collect();
    for (band, &freq) in BANDS.iter().enumerate() {
        let coeff = 2.0 * (std::f32::consts::TAU * freq / rate as f32).cos();
        let (mut s1, mut s2) = (0f32, 0f32);
        for (sample, w) in samples.iter().zip(&hann) {
            let s0 = sample * w + coeff * s1 - s2;
            s2 = s1;
            s1 = s0;
        }
        let power = (s1 * s1 + s2 * s2 - coeff * s1 * s2).max(0.0);
        let amp = power.sqrt() / (n as f32 / 4.0);
        let db = 20.0 * (amp + 1e-9).log10();
        out[band] = ((db + 72.0) / 56.0).clamp(0.0, 1.0);
    }
    out
}
static CHOSEN: Mutex<String> = Mutex::new(String::new());

pub fn choose(id: &str) {
    *CHOSEN.lock().unwrap() = id.trim().to_string();
}

pub(crate) fn device(host: &cpal::Host) -> Option<cpal::Device> {
    let wanted = CHOSEN.lock().unwrap().clone();
    if !wanted.is_empty() {
        let found = host.input_devices().ok().into_iter().flatten().find(|mic| mic.id().is_ok_and(|id| id.to_string() == wanted));
        if found.is_some() {
            return found;
        }
    }
    host.default_input_device()
}

#[derive(serde::Serialize)]
pub struct Mic {
    id: String,
    nombre: String,
    windows: bool,
}

#[tauri::command]
pub fn mic_list() -> Vec<Mic> {
    let host = cpal::default_host();
    let default = host.default_input_device().and_then(|mic| mic.id().ok()).map(|id| id.to_string());
    let mut mics: Vec<Mic> = host
        .input_devices()
        .into_iter()
        .flatten()
        .filter_map(|mic| {
            let id = mic.id().ok()?.to_string();
            let nombre = mic.description().map(|about| about.name().to_string()).unwrap_or_else(|_| mic.to_string());
            Some(Mic { windows: default.as_deref() == Some(id.as_str()), id, nombre })
        })
        .collect();
    mics.sort_by(|a, b| b.windows.cmp(&a.windows).then_with(|| a.nombre.cmp(&b.nombre)));
    mics
}

pub fn tap() -> mpsc::Receiver<(u32, Vec<f32>)> {
    let (tx, rx) = mpsc::channel();
    *TAP.lock().unwrap() = Some(tx);
    rx
}

pub fn untap() {
    *TAP.lock().unwrap() = None;
}

pub fn start() {
    if let Some(tx) = TX.get() {
        let _ = tx.send(Command::Start);
    }
}

pub fn discard() {
    if let Some(tx) = TX.get() {
        let _ = tx.send(Command::Discard);
    }
}

pub fn stop() -> Result<Recording, String> {
    let (reply, result) = mpsc::channel();
    TX.get()
        .ok_or("El micrófono no está listo.")?
        .send(Command::Stop(reply))
        .map_err(|_| "El micrófono no está listo.")?;
    result.recv().map_err(|_| "El micrófono no respondió.".to_string())?
}

type Buffer = Arc<Mutex<Vec<f32>>>;

fn push<T: Copy>(data: &[T], channels: usize, buffer: &Buffer, to_f32: impl Fn(T) -> f32) {
    let mut peak = 0f32;
    let mut out = buffer.lock().unwrap();
    let from = out.len();
    for frame in data.chunks(channels.max(1)) {
        let sample = frame.iter().map(|&s| to_f32(s)).sum::<f32>() / frame.len() as f32;
        peak = peak.max(sample.abs());
        out.push(sample);
    }
    LEVEL.store(peak.to_bits(), Ordering::Relaxed);
    {
        let mut recent = RECENT.lock().unwrap();
        recent.extend_from_slice(&out[from..]);
        let extra = recent.len().saturating_sub(WINDOW);
        if extra > 0 {
            recent.drain(..extra);
        }
    }
    if let Some(tap) = TAP.lock().unwrap().as_ref() {
        let _ = tap.send((RATE.load(Ordering::Relaxed), out[from..].to_vec()));
    }
}

fn open(buffer: Buffer) -> Result<(cpal::Stream, u32), String> {
    let device = device(&cpal::default_host()).ok_or("No hay ningún micrófono conectado.")?;
    let config = device.default_input_config().map_err(|e| format!("El micrófono no responde: {e}"))?;
    let rate = config.sample_rate();
    RATE.store(rate, Ordering::Relaxed);
    let channels = config.channels() as usize;
    let format = config.sample_format();
    let config: cpal::StreamConfig = config.into();
    let on_error = |_| {};

    let stream = match format {
        cpal::SampleFormat::F32 => device.build_input_stream(
            config.clone(),
            move |data: &[f32], _: &_| push(data, channels, &buffer, |s| s),
            on_error,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_input_stream(
            config.clone(),
            move |data: &[i16], _: &_| push(data, channels, &buffer, |s| s as f32 / 32768.0),
            on_error,
            None,
        ),
        cpal::SampleFormat::U16 => device.build_input_stream(
            config.clone(),
            move |data: &[u16], _: &_| push(data, channels, &buffer, |s| (s as f32 - 32768.0) / 32768.0),
            on_error,
            None,
        ),
        other => return Err(format!("Formato de micrófono no soportado: {other:?}")),
    }
    .map_err(|e| format!("No se pudo abrir el micrófono: {e}"))?;
    stream.play().map_err(|e| format!("No se pudo abrir el micrófono: {e}"))?;
    Ok((stream, rate))
}

pub fn spawn(app: AppHandle) {
    let (tx, rx) = mpsc::channel();
    let _ = TX.set(tx);

    std::thread::Builder::new()
        .name("microfono".into())
        .spawn(move || {
            let buffer: Buffer = Arc::default();
            let mut open_stream: Option<Result<(cpal::Stream, u32), String>> = None;

            loop {
                let command = if matches!(open_stream, Some(Ok(_))) {
                    match rx.recv_timeout(Duration::from_millis(50)) {
                        Ok(command) => command,
                        Err(RecvTimeoutError::Timeout) => {
                            let level = f32::from_bits(LEVEL.load(Ordering::Relaxed));
                            let _ = app.emit_to(overlay::LABEL, "level", level);
                            let heard = bands(&RECENT.lock().unwrap(), RATE.load(Ordering::Relaxed));
                            let _ = app.emit_to(overlay::LABEL, "bands", heard);
                            crate::gamelayer::level(level);
                            continue;
                        }
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                } else {
                    match rx.recv() {
                        Ok(command) => command,
                        Err(_) => break,
                    }
                };

                match command {
                    Command::Start if matches!(open_stream, Some(Ok(_))) => {}
                    Command::Start => {
                        buffer.lock().unwrap().clear();
                        LEVEL.store(0, Ordering::Relaxed);
                        open_stream = Some(open(buffer.clone()));
                    }
                    Command::Discard => {
                        open_stream = None;
                        RECENT.lock().unwrap().clear();
                        buffer.lock().unwrap().clear();
                        LEVEL.store(0, Ordering::Relaxed);
                        let _ = app.emit_to(overlay::LABEL, "level", 0f32);
                    }
                    Command::Stop(reply) => {
                        let result = match open_stream.take() {
                            Some(Ok((stream, rate))) => {
                                drop(stream);
                                Ok(Recording { samples: std::mem::take(&mut *buffer.lock().unwrap()), rate })
                            }
                            Some(Err(error)) => Err(error),
                            None => Err("No se estaba grabando.".into()),
                        };
                        let _ = reply.send(result);
                    }
                }
            }
        })
        .expect("hilo del micrófono");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(freq: f32, rate: u32) -> Vec<f32> {
        (0..WINDOW).map(|i| (std::f32::consts::TAU * freq * i as f32 / rate as f32).sin() * 0.3).collect()
    }

    #[test]
    fn each_band_hears_its_own_pitch() {
        for (band, &freq) in BANDS.iter().enumerate() {
            let heard = bands(&tone(freq, 48_000), 48_000);
            let loudest = heard.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).unwrap().0;
            assert_eq!(loudest, band, "{freq} Hz cayó en {loudest}: {heard:?}");
        }
    }

    #[test]
    fn silence_draws_nothing() {
        assert!(bands(&vec![0.0; WINDOW], 48_000).iter().all(|&b| b == 0.0));
        assert!(bands(&[0.1; 10], 48_000).iter().all(|&b| b == 0.0));
    }
}
