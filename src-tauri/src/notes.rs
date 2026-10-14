// mandar notas a Mis Notas

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::json;
use tauri::AppHandle;

use crate::apps;

const PORTS: [u16; 4] = [5173, 5273, 5373, 5473];

pub fn request(port: u16, method: &str, path: &str, body: Option<&str>) -> Option<String> {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_millis(400)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(4))).ok()?;
    stream.set_write_timeout(Some(Duration::from_secs(4))).ok()?;

    let mut head = format!("{method} {path} HTTP/1.0\r\nHost: 127.0.0.1:{port}\r\n");
    if let Some(body) = body {
        head += &format!("Content-Type: application/json\r\nX-Notas: 1\r\nContent-Length: {}\r\n", body.len());
    }
    head += "\r\n";
    stream.write_all(head.as_bytes()).ok()?;
    if let Some(body) = body {
        stream.write_all(body.as_bytes()).ok()?;
    }

    let mut response = String::new();
    stream.read_to_string(&mut response).ok()?;
    let (status, rest) = response.split_once("\r\n")?;
    if !status.contains(" 200") {
        return None;
    }
    rest.split_once("\r\n\r\n").map(|(_, body)| body.to_string())
}

pub fn find_port() -> Option<u16> {
    PORTS.into_iter().find(|&port| {
        request(port, "GET", "/api/ping", None).is_some_and(|body| body.contains("\"mis-notas\""))
    })
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

pub fn add_note(app: &AppHandle, text: &str) -> Result<String, String> {
    let mut opened = false;
    let port = match find_port() {
        Some(port) => port,
        None => {
            let installed = apps::find(app, "Mis Notas", 0.72).ok_or("No encontré Mis Notas instalada en este equipo.")?;
            apps::launch(&installed)?;
            opened = true;
            let mut found = None;
            for _ in 0..30 {
                std::thread::sleep(Duration::from_millis(500));
                found = find_port();
                if found.is_some() {
                    break;
                }
            }
            found.ok_or("Mis Notas se abrió, pero no respondió a tiempo. Inténtalo otra vez.")?
        }
    };

    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
    let note = json!({
        "id": format!("cocuyo-{now}"),
        "title": "",
        "html": escape(text),
        "folder": null,
        "emoji": "🎙️",
        "paper": "crema",
        "font": "clasica",
        "pattern": "none",
        "align": "left",
        "stickers": [],
        "song": null,
        "pinned": false,
        "fav": false,
        "created": now,
        "updated": now,
    });
    let body = json!({
        "base": null,
        "savedAt": now,
        "data": { "notas.notes": serde_json::to_string(&[note]).map_err(|e| e.to_string())? },
    })
    .to_string();

    request(port, "PUT", "/api/data", Some(&body)).ok_or("Mis Notas no aceptó la nota.")?;
    Ok(if opened { "Abrí Mis Notas y guardé tu nota.".into() } else { "Anotado en Mis Notas.".into() })
}

pub fn note_text(spoken: &str) -> Option<String> {
    let lower = spoken.to_lowercase();
    let trigger = ["toma nota", "anota", "apunta"].into_iter().find(|t| lower.starts_with(t))?;
    let rest = &spoken[trigger.len()..];
    if rest.chars().next().is_some_and(char::is_alphanumeric) {
        return None;
    }
    let skip = |text: &str| text.trim_start_matches(|c: char| c.is_whitespace() || ",:;.-".contains(c)).to_string();
    let mut text = skip(rest);
    for link in ["de que ", "de ", "que "] {
        if text.to_lowercase().starts_with(link) {
            text = skip(&text[link.len()..]);
            break;
        }
    }
    Some(text)
        .filter(|text| !text.is_empty())
        .map(|text| {
            let mut chars = text.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => text,
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_orders_keep_what_was_said() {
        assert_eq!(note_text("Anota comprar pan y leche mañana.").as_deref(), Some("Comprar pan y leche mañana."));
        assert_eq!(note_text("Toma nota de que Juan llega el martes").as_deref(), Some("Juan llega el martes"));
        assert_eq!(note_text("apunta: llamar a mamá").as_deref(), Some("Llamar a mamá"));
        assert_eq!(note_text("Anota, nota de prueba.").as_deref(), Some("Nota de prueba."));
    }

    #[test]
    fn other_phrases_are_not_notes() {
        assert_eq!(note_text("Abre el bloc de notas"), None);
        assert_eq!(note_text("anota"), None);
        assert_eq!(note_text("anotaciones del día"), None);
    }

    #[test]
    fn html_is_escaped() {
        assert_eq!(escape("a < b & c"), "a &lt; b &amp; c");
    }
}
