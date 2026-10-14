// prueba de velocidad y dns. cloudflare no deja bajar 100 megas o mas, por eso son 90

use std::io::{BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use windows::core::{w, HSTRING};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

use crate::apps::CREATE_NO_WINDOW;
use crate::overlay;

const DOWN: &str = "https://speed.cloudflare.com/__down?bytes=90000000";
const DOWN_SMALL: &str = "https://speed.cloudflare.com/__down?bytes=20000000";
const UP: &str = "https://speed.cloudflare.com/__up";
const SECONDS: &str = "8";
const UPLOAD_BYTES: usize = 24_000_000;

static MEASURING: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Speed {
    phase: &'static str,
    ping_ms: Option<f32>,
    down: Option<f32>,
    up: Option<f32>,
    error: Option<String>,
}

fn curl() -> Command {
    let mut command = Command::new("curl.exe");
    command.creation_flags(CREATE_NO_WINDOW).stdout(Stdio::piped());
    command
}

fn ping() -> Option<f32> {
    let mut best: Option<f32> = None;
    for _ in 0..3 {
        let output = curl()
            .args(["--silent", "--max-time", "5", "--output", "NUL", "--write-out", "%{time_namelookup} %{time_connect}"])
            .arg("https://speed.cloudflare.com/__down?bytes=0")
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&output.stdout);
        let mut parts = text.split_whitespace().filter_map(|part| part.replace(',', ".").parse::<f32>().ok());
        if let (Some(lookup), Some(connect)) = (parts.next(), parts.next()) {
            let ms = (connect - lookup) * 1000.0;
            if ms > 0.0 {
                best = Some(best.map_or(ms, |b| b.min(ms)));
            }
        }
    }
    best
}

fn meter_speed(token: &str) -> Option<f32> {
    let (number, scale) = match token.chars().last()? {
        'k' => (&token[..token.len() - 1], 1024.0),
        'M' => (&token[..token.len() - 1], 1024.0 * 1024.0),
        'G' => (&token[..token.len() - 1], 1024.0 * 1024.0 * 1024.0),
        _ => (token, 1.0),
    };
    Some(number.parse::<f32>().ok()? * scale)
}

fn transfer(mut command: Command, upload: Option<usize>, report: &dyn Fn(f32)) -> Option<f32> {
    command.stderr(Stdio::piped());
    if upload.is_some() {
        command.stdin(Stdio::piped());
    }
    let mut child = command.spawn().ok()?;
    if let (Some(bytes), Some(mut stdin)) = (upload, child.stdin.take()) {
        std::thread::spawn(move || {
            let chunk = vec![0u8; 256 * 1024];
            let mut sent = 0;
            while sent < bytes && stdin.write_all(&chunk).is_ok() {
                sent += chunk.len();
            }
        });
    }
    let mut line = Vec::new();
    for byte in BufReader::new(child.stderr.take()?).bytes().map_while(Result::ok) {
        if byte == b'\r' || byte == b'\n' {
            let text = String::from_utf8_lossy(&line);
            let columns: Vec<&str> = text.split_whitespace().collect();
            if columns.len() == 12 {
                if let Some(speed) = meter_speed(columns[11]).filter(|speed| *speed > 0.0) {
                    report(speed * 8.0 / 1_000_000.0);
                }
            }
            line.clear();
        } else {
            line.push(byte);
        }
    }
    let output = child.wait_with_output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let numbers: Vec<f32> = text.split_whitespace().filter_map(|part| part.replace(',', ".").parse().ok()).collect();
    let (bytes, total, waited) = (*numbers.first()?, *numbers.get(1)?, *numbers.get(2)?);
    let seconds = total - waited;
    (bytes > 50_000.0 && seconds > 0.2).then(|| bytes * 8.0 / seconds / 1_000_000.0)
}

#[tauri::command]
pub fn net_speed(app: AppHandle) {
    if MEASURING.swap(true, Ordering::Relaxed) {
        return;
    }
    std::thread::spawn(move || {
        let emit = |speed: &Speed| {
            let _ = app.emit_to(overlay::LABEL, "net-speed", speed.clone());
        };
        let mut speed = Speed { phase: "ping", ..Default::default() };
        emit(&speed);
        speed.ping_ms = ping();
        if speed.ping_ms.is_none() {
            speed.error = Some("No hay conexión con el servidor de prueba. Revisa tu internet.".into());
        } else {
            speed.phase = "down";
            emit(&speed);
            let live = |mbps: f32| emit(&Speed { down: Some(mbps), ..speed.clone() });
            let download = |from: &str| {
                let mut down = curl();
                down.args(["--max-time", SECONDS, "--output", "NUL", "--write-out", "%{size_download} %{time_total} %{time_starttransfer}", from]);
                transfer(down, None, &live)
            };
            speed.down = download(DOWN).or_else(|| download(DOWN_SMALL));

            speed.phase = "up";
            emit(&speed);
            let mut up = curl();
            up.args(["--max-time", SECONDS, "--output", "NUL", "--request", "POST", "--data-binary", "@-"]);
            up.args(["--header", "Content-Type: application/octet-stream", "--write-out", "%{size_upload} %{time_total} %{time_pretransfer}", UP]);
            let live = |mbps: f32| emit(&Speed { up: Some(mbps), ..speed.clone() });
            speed.up = transfer(up, Some(UPLOAD_BYTES), &live);
            if speed.down.is_none() {
                speed.error = Some("La prueba se cortó antes de poder medir.".into());
            }
        }
        speed.phase = "done";
        emit(&speed);
        MEASURING.store(false, Ordering::Relaxed);
    });
}

const PROVIDERS: [(&str, [u8; 4], [u8; 4]); 5] = [
    ("Cloudflare", [1, 1, 1, 1], [1, 0, 0, 1]),
    ("Google", [8, 8, 8, 8], [8, 8, 4, 4]),
    ("Quad9", [9, 9, 9, 9], [149, 112, 112, 112]),
    ("OpenDNS", [208, 67, 222, 222], [208, 67, 220, 220]),
    ("AdGuard", [94, 140, 14, 14], [94, 140, 15, 15]),
];
const QUESTIONS: [&str; 5] = ["google.com", "youtube.com", "wikipedia.org", "github.com", "whatsapp.com"];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dns {
    name: String,
    address: String,
    ms: Option<f32>,
    current: bool,
    provider: Option<usize>,
}

fn question(domain: &str, id: u16) -> Vec<u8> {
    let mut packet = vec![(id >> 8) as u8, id as u8, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0];
    for label in domain.split('.') {
        packet.push(label.len() as u8);
        packet.extend_from_slice(label.as_bytes());
    }
    packet.extend_from_slice(&[0, 0, 1, 0, 1]);
    packet
}

fn measure(server: Ipv4Addr) -> Option<f32> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.set_read_timeout(Some(Duration::from_millis(900))).ok()?;
    let to = SocketAddr::from((server, 53));
    let mut times = Vec::new();
    let mut answer = [0u8; 512];
    for (index, domain) in QUESTIONS.iter().enumerate() {
        let id = 0x4300 + index as u16;
        let start = Instant::now();
        if socket.send_to(&question(domain, id), to).is_err() {
            continue;
        }
        while let Ok((size, _)) = socket.recv_from(&mut answer) {
            if size >= 2 && answer[0] == (id >> 8) as u8 && answer[1] == id as u8 {
                times.push(start.elapsed().as_secs_f32() * 1000.0);
                break;
            }
        }
    }
    if times.len() < 3 {
        return None;
    }
    times.sort_by(f32::total_cmp);
    Some(times[times.len() / 2])
}

fn current_dns() -> Vec<Ipv4Addr> {
    let script = "$i=(Get-NetRoute -DestinationPrefix '0.0.0.0/0' | Sort-Object RouteMetric | Select-Object -First 1).ifIndex; (Get-DnsClientServerAddress -InterfaceIndex $i -AddressFamily IPv4).ServerAddresses -join ','";
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    let Ok(output) = output else { return Vec::new() };
    String::from_utf8_lossy(&output.stdout).trim().split(',').filter_map(|address| address.trim().parse().ok()).collect()
}

#[tauri::command]
pub async fn dns_measure() -> Vec<Dns> {
    let current = std::thread::spawn(current_dns);
    let providers: Vec<_> = PROVIDERS
        .iter()
        .enumerate()
        .map(|(index, (name, primary, _))| {
            let address = Ipv4Addr::from(*primary);
            (index, *name, address, std::thread::spawn(move || measure(address)))
        })
        .collect();
    let current = current.join().unwrap_or_default();
    let mut list: Vec<Dns> = providers
        .into_iter()
        .map(|(index, name, address, timing)| Dns {
            name: name.into(),
            address: address.to_string(),
            ms: timing.join().ok().flatten(),
            current: current.contains(&address),
            provider: Some(index),
        })
        .collect();
    if let Some(first) = current.first().filter(|_| !list.iter().any(|dns| dns.current)) {
        list.push(Dns { name: "El de tu conexión".into(), address: first.to_string(), ms: measure(*first), current: true, provider: None });
    }
    list.sort_by(|a, b| a.ms.unwrap_or(f32::MAX).total_cmp(&b.ms.unwrap_or(f32::MAX)));
    list
}

fn elevated(change: &str) -> Result<(), String> {
    let script = format!(
        "$i=(Get-NetRoute -DestinationPrefix '0.0.0.0/0' | Sort-Object RouteMetric | Select-Object -First 1).ifIndex; Set-DnsClientServerAddress -InterfaceIndex $i {change}; Clear-DnsClientCache"
    );
    let arguments = HSTRING::from(format!("-NoProfile -WindowStyle Hidden -Command \"{script}\""));
    let result = unsafe { ShellExecuteW(None, w!("runas"), w!("powershell.exe"), &arguments, None, SW_HIDE) };
    if result.0 as isize > 32 {
        Ok(())
    } else {
        Err("Windows no dio el permiso para cambiar el DNS.".into())
    }
}

#[tauri::command]
pub fn dns_apply(provider: usize) -> Result<String, String> {
    let (name, primary, secondary) = PROVIDERS.get(provider).ok_or("Ese servidor no está en la lista.")?;
    elevated(&format!("-ServerAddresses '{}','{}'", Ipv4Addr::from(*primary), Ipv4Addr::from(*secondary)))?;
    Ok(format!("DNS cambiado a {name}."))
}

#[tauri::command]
pub fn dns_reset() -> Result<String, String> {
    elevated("-ResetServerAddresses")?;
    Ok("DNS en automático otra vez.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dns_questions_are_well_formed() {
        let packet = question("google.com", 0x4301);
        assert_eq!(&packet[..2], &[0x43, 0x01]);
        assert_eq!(&packet[2..6], &[0x01, 0x00, 0, 1]);
        assert_eq!(&packet[12..], b"\x06google\x03com\x00\x00\x01\x00\x01");
    }

    #[test]
    fn curl_meter_speeds_are_read() {
        assert_eq!(meter_speed("5731k"), Some(5731.0 * 1024.0));
        assert_eq!(meter_speed("12.5M"), Some(12.5 * 1024.0 * 1024.0));
        assert_eq!(meter_speed("900"), Some(900.0));
        assert_eq!(meter_speed("--:--:--"), None);
    }

    #[test]
    #[ignore]
    fn red_de_verdad() {
        println!("ping: {:?} ms", ping());
        for (name, primary, _) in PROVIDERS {
            println!("{name}: {:?} ms", measure(Ipv4Addr::from(primary)));
        }
        println!("dns del equipo: {:?}", current_dns());
        let mut down = curl();
        down.args(["--max-time", "5", "--output", "NUL", "--write-out", "%{size_download} %{time_total} %{time_starttransfer}", DOWN]);
        println!("bajada: {:?} Mbps", transfer(down, None, &|mbps| println!("  ...{mbps:.1}")));
        let mut up = curl();
        up.args(["--max-time", "5", "--output", "NUL", "--request", "POST", "--data-binary", "@-"]);
        up.args(["--header", "Content-Type: application/octet-stream", "--write-out", "%{size_upload} %{time_total} %{time_pretransfer}", UP]);
        println!("subida: {:?} Mbps", transfer(up, Some(UPLOAD_BYTES), &|mbps| println!("  ...{mbps:.1}")));
    }
}
