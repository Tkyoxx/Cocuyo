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
    Stop(Sender<Result<Recording, String>>),
}

static TX: OnceLock<Sender<Command>> = OnceLock::new();
static LEVEL: AtomicU32 = AtomicU32::new(0);
static TAP: Mutex<Option<Sender<(u32, Vec<f32>)>>> = Mutex::new(None);
static RATE: AtomicU32 = AtomicU32::new(0);

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
    if let Some(tap) = TAP.lock().unwrap().as_ref() {
        let _ = tap.send((RATE.load(Ordering::Relaxed), out[from..].to_vec()));
    }
}

fn open(buffer: Buffer) -> Result<(cpal::Stream, u32), String> {
    let device = cpal::default_host()
        .default_input_device()
        .ok_or("No hay ningún micrófono conectado.")?;
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
                    Command::Start => {
                        buffer.lock().unwrap().clear();
                        LEVEL.store(0, Ordering::Relaxed);
                        open_stream = Some(open(buffer.clone()));
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
