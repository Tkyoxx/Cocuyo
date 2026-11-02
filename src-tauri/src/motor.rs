// el motor de voz arranca apenas empiezas a hablar y espera el audio por una tuberia: el modelo carga mientras hablas
// medido con parakeet-cli: una frase pasa de ~990 ms a ~280 ms desde que sueltas las teclas

use std::io::Read;
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use windows::core::HSTRING;
use windows::Win32::Foundation::{CloseHandle, ERROR_IO_PENDING, ERROR_PIPE_CONNECTED, HANDLE, WAIT_OBJECT_0};
use windows::Win32::Storage::FileSystem::{FlushFileBuffers, WriteFile, FILE_FLAG_OVERLAPPED, PIPE_ACCESS_OUTBOUND};
use windows::Win32::System::Pipes::{ConnectNamedPipe, CreateNamedPipeW, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT};
use windows::Win32::System::Threading::{CreateEventW, ResetEvent, WaitForMultipleObjects};
use windows::Win32::System::IO::{GetOverlappedResult, OVERLAPPED};

use crate::apps::CREATE_NO_WINDOW;

const PATIENCE: u32 = 15_000;
const STALE: Duration = Duration::from_secs(240);

struct Warm {
    child: Child,
    pipe: HANDLE,
    since: Instant,
}

unsafe impl Send for Warm {}

impl Drop for Warm {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        unsafe {
            let _ = CloseHandle(self.pipe);
        }
    }
}

static WARM: Mutex<Option<Warm>> = Mutex::new(None);
static TURN: AtomicU32 = AtomicU32::new(0);

pub fn warm(engine: &Path, model: &Path, threads: usize) {
    let mut slot = WARM.lock().unwrap();
    if slot.as_mut().is_some_and(|warm| warm.since.elapsed() < STALE && matches!(warm.child.try_wait(), Ok(None))) {
        return;
    }
    *slot = None;
    let name = format!(r"\\.\pipe\cocuyo-voz-{}-{}", std::process::id(), TURN.fetch_add(1, Ordering::Relaxed));
    let pipe = unsafe {
        CreateNamedPipeW(&HSTRING::from(name.as_str()), PIPE_ACCESS_OUTBOUND | FILE_FLAG_OVERLAPPED, PIPE_TYPE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS, 1, 1 << 20, 0, 0, None)
    };
    if pipe.is_invalid() {
        return;
    }
    let child = Command::new(engine)
        .arg("-m")
        .arg(model)
        .arg("-f")
        .arg(&name)
        .args(["-np", "-t", &threads.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();
    match child {
        Ok(child) => *slot = Some(Warm { child, pipe, since: Instant::now() }),
        Err(_) => unsafe {
            let _ = CloseHandle(pipe);
        },
    }
}

pub fn cool() {
    WARM.lock().unwrap().take();
}

fn wait_for(event: HANDLE, child: HANDLE) -> bool {
    unsafe { WaitForMultipleObjects(&[event, child], false, PATIENCE) == WAIT_OBJECT_0 }
}

fn send(warm: &Warm, wav: &[u8]) -> Option<()> {
    let child = HANDLE(warm.child.as_raw_handle());
    let event = unsafe { CreateEventW(None, true, false, None) }.ok()?;
    let sent = (|| {
        let mut overlapped = OVERLAPPED { hEvent: event, ..Default::default() };
        match unsafe { ConnectNamedPipe(warm.pipe, Some(&mut overlapped)) } {
            Ok(()) => {}
            Err(e) if e.code() == ERROR_PIPE_CONNECTED.to_hresult() => {}
            Err(e) if e.code() == ERROR_IO_PENDING.to_hresult() => {
                if !wait_for(event, child) {
                    return None;
                }
            }
            Err(_) => return None,
        }
        let mut done = 0;
        while done < wav.len() {
            unsafe { ResetEvent(event) }.ok()?;
            let mut overlapped = OVERLAPPED { hEvent: event, ..Default::default() };
            match unsafe { WriteFile(warm.pipe, Some(&wav[done..]), None, Some(&mut overlapped)) } {
                Ok(()) => {}
                Err(e) if e.code() == ERROR_IO_PENDING.to_hresult() => {
                    if !wait_for(event, child) {
                        return None;
                    }
                }
                Err(_) => return None,
            }
            let mut moved = 0u32;
            unsafe { GetOverlappedResult(warm.pipe, &overlapped, &mut moved, false) }.ok()?;
            if moved == 0 {
                return None;
            }
            done += moved as usize;
        }
        unsafe { FlushFileBuffers(warm.pipe) }.ok()
    })();
    unsafe {
        let _ = CloseHandle(event);
    }
    sent
}

pub fn transcribe(wav: &[u8]) -> Option<Result<String, String>> {
    let mut warm = WARM.lock().unwrap().take()?;
    if !matches!(warm.child.try_wait(), Ok(None)) {
        return None;
    }
    send(&warm, wav)?;
    unsafe {
        let _ = CloseHandle(std::mem::replace(&mut warm.pipe, HANDLE::default()));
    }
    let mut out = String::new();
    warm.child.stdout.take()?.read_to_string(&mut out).ok()?;
    let ended = warm.child.wait().ok()?;
    Some(if ended.success() { Ok(out.split_whitespace().collect::<Vec<_>>().join(" ")) } else { Err("El motor de voz falló al transcribir.".into()) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "usa el motor y el modelo instalados de verdad"]
    fn hears_the_same_while_warm() {
        let voz = std::path::PathBuf::from(std::env::var("LOCALAPPDATA").unwrap()).join("io.github.tkyoxx.cocuyo").join("voz");
        let wav = std::fs::read(std::env::var("COCUYO_WAV").expect("COCUYO_WAV con un .wav de 16 kHz")).unwrap();
        warm(&voz.join("motor").join("parakeet-cli.exe"), &voz.join("parakeet-tdt-0.6b-v3-q4_k.bin"), 4);
        std::thread::sleep(Duration::from_millis(1500));
        let started = Instant::now();
        let heard = transcribe(&wav).expect("no estaba tibio").unwrap();
        eprintln!("tibio: {} ms -> {heard}", started.elapsed().as_millis());
        assert!(heard.split_whitespace().count() > 5);
        warm(&voz.join("motor").join("parakeet-cli.exe"), &voz.join("parakeet-tdt-0.6b-v3-q4_k.bin"), 4);
        cool();
        assert!(WARM.lock().unwrap().is_none());
    }
}
