// youtube music sin clave ni cuenta extra: busca en su pagina de busqueda (su robots.txt la deja; /youtubei/ no, y no la tocamos),
// abre la cancion en tu youtube music y si no arranca sola le da play por los controles de windows

use std::os::windows::process::CommandExt;
use std::process::Command;
use std::time::Duration;

use serde_json::Value;
use tauri::AppHandle;

use crate::apps::CREATE_NO_WINDOW;

const AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36";

#[derive(Debug, PartialEq)]
pub struct Found {
    pub id: String,
    pub list: Option<String>,
    pub title: String,
    pub artist: String,
}

pub fn wanted(app: &AppHandle) -> bool {
    match crate::settings::load(app).music_service.as_str() {
        "youtube" => true,
        "spotify" => false,
        _ => crate::spotify::token(app).is_none(),
    }
}

pub fn js_string(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('x') => {
                let hex: String = chars.by_ref().take(2).collect();
                if let Some(c) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    out.push(c);
                }
            }
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                let mut code = u32::from_str_radix(&hex, 16).unwrap_or(0xFFFD);
                if (0xD800..0xDC00).contains(&code) && chars.peek() == Some(&'\\') {
                    let mut ahead = chars.clone();
                    ahead.next();
                    if ahead.next() == Some('u') {
                        let low: String = ahead.by_ref().take(4).collect();
                        if let Ok(low) = u32::from_str_radix(&low, 16) {
                            code = 0x10000 + ((code - 0xD800) << 10) + (low - 0xDC00);
                            chars = ahead;
                        }
                    }
                }
                out.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
            }
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => {}
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

pub fn results(html: &str) -> Option<Value> {
    let at = html.find("path: '\\/search'")?;
    let data = html[at..].find("data: '").map(|i| at + i + 7)?;
    let mut end = data;
    let bytes = html.as_bytes();
    while end < bytes.len() {
        match bytes[end] {
            b'\\' => end += 2,
            b'\'' => break,
            _ => end += 1,
        }
    }
    serde_json::from_str(&js_string(html.get(data..end)?)).ok()
}

fn runs(value: &Value) -> Vec<String> {
    value["runs"].as_array().map(|all| all.iter().filter_map(|run| run["text"].as_str().map(str::to_string)).collect()).unwrap_or_default()
}

fn artist_of(subtitle: &[String]) -> String {
    let parts: Vec<&str> = subtitle.iter().map(String::as_str).filter(|p| p.trim() != "•" && !p.trim().is_empty()).collect();
    let kinds = ["canción", "video", "song", "álbum", "album", "sencillo", "single", "ep", "playlist", "lista"];
    parts.iter().find(|p| !kinds.contains(&p.trim().to_lowercase().as_str()) && !p.contains(':')).map(|p| p.trim().to_string()).unwrap_or_default()
}

fn walk<'a>(value: &'a Value, found: &mut Vec<&'a Value>) {
    match value {
        Value::Object(map) => {
            for (key, inner) in map {
                if key == "musicCardShelfRenderer" || key == "musicResponsiveListItemRenderer" {
                    found.push(value);
                }
                walk(inner, found);
            }
        }
        Value::Array(all) => all.iter().for_each(|inner| walk(inner, found)),
        _ => {}
    }
}

pub fn first_song(data: &Value) -> Option<Found> {
    let mut found = Vec::new();
    walk(data, &mut found);
    for item in found {
        if let Some(card) = item.get("musicCardShelfRenderer") {
            let watch = &card["title"]["runs"][0]["navigationEndpoint"]["watchEndpoint"];
            if let Some(id) = watch["videoId"].as_str() {
                let title = runs(&card["title"]).join("");
                return Some(Found { id: id.into(), list: watch["playlistId"].as_str().map(str::to_string), title, artist: artist_of(&runs(&card["subtitle"])) });
            }
            continue;
        }
        let row = &item["musicResponsiveListItemRenderer"];
        let Some(id) = row["playlistItemData"]["videoId"].as_str().or(row["overlay"]["musicItemThumbnailOverlayRenderer"]["content"]["musicPlayButtonRenderer"]["playNavigationEndpoint"]["watchEndpoint"]["videoId"].as_str()) else {
            continue;
        };
        let column = |i: usize| runs(&row["flexColumns"][i]["musicResponsiveListItemFlexColumnRenderer"]["text"]);
        return Some(Found { id: id.into(), list: None, title: column(0).join(""), artist: artist_of(&column(1)) });
    }
    None
}

fn query_of(text: &str) -> String {
    text.bytes().map(|b| if b.is_ascii_alphanumeric() { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

pub fn search(query: &str) -> Result<Found, String> {
    let url = format!("https://music.youtube.com/search?q={}", query_of(query.trim()));
    let out = Command::new("curl.exe")
        .args(["-sL", "--max-time", "12", "--compressed", "-A", AGENT, "-H", "Accept-Language: es-419,es;q=0.9,en;q=0.6", &url])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| e.to_string())?;
    let html = String::from_utf8_lossy(&out.stdout);
    let data = results(&html).ok_or("YouTube Music no respondió como esperaba.")?;
    first_song(&data).ok_or_else(|| format!("No encontré «{query}» en YouTube Music."))
}

pub fn play(app: &AppHandle, query: &str) -> Result<String, String> {
    let _ = app;
    let found = search(query)?;
    let url = match &found.list {
        Some(list) => format!("https://music.youtube.com/watch?v={}&list={list}", found.id),
        None => format!("https://music.youtube.com/watch?v={}", found.id),
    };
    crate::media::control(crate::media::Transport::Pause);
    crate::apps::open(&url)?;
    let title = found.title.clone();
    std::thread::spawn(move || crate::media::play_titled(&title, Duration::from_secs(12)));
    Ok(if found.artist.is_empty() { format!("Sonando «{}» en YouTube Music.", found.title) } else { format!("Sonando «{}» de {} en YouTube Music.", found.title, found.artist) })
}

pub fn asked_by_name(spoken: &str) -> Option<String> {
    let clean = crate::intent::normalize(spoken);
    let tails = [" en youtube music", " en yt music", " por youtube music", " en youtube musica"];
    let tail = tails.iter().find(|tail| clean.ends_with(**tail))?;
    let heads = ["pon ", "ponme ", "reproduce ", "toca ", "play ", "quiero oir ", "quiero escuchar "];
    let head = heads.iter().find(|head| clean.starts_with(**head))?;
    let words: Vec<&str> = spoken.split_whitespace().collect();
    let skip = head.split_whitespace().count();
    let cut = tail.split_whitespace().count();
    (words.len() > skip + cut).then(|| words[skip..words.len() - cut].join(" "))
}

pub fn spoken(app: &AppHandle, spoken: &str) -> Option<Result<String, String>> {
    let query = asked_by_name(spoken)?;
    Some(play(app, &query))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_the_strings_youtube_music_embeds() {
        assert_eq!(js_string(r"\x7b\x22a\x22:\x22Tití\x22\x7d"), r#"{"a":"Tití"}"#);
        assert_eq!(js_string(r"https:\/\/i.ytimg.com"), "https://i.ytimg.com");
        assert_eq!(js_string(r"😀"), "😀");
    }

    #[test]
    fn finds_the_first_song_in_the_results() {
        let html = r"x initialData.push({path: '\/search', params: JSON.parse('\x7b\x7d'), data: '\x7b\x22contents\x22:\x5b\x7b\x22musicResponsiveListItemRenderer\x22:\x7b\x22playlistItemData\x22:\x7b\x22videoId\x22:\x22abcdefghijk\x22\x7d,\x22flexColumns\x22:\x5b\x7b\x22musicResponsiveListItemFlexColumnRenderer\x22:\x7b\x22text\x22:\x7b\x22runs\x22:\x5b\x7b\x22text\x22:\x22Tití Me Preguntó\x22\x7d\x5d\x7d\x7d\x7d,\x7b\x22musicResponsiveListItemFlexColumnRenderer\x22:\x7b\x22text\x22:\x7b\x22runs\x22:\x5b\x7b\x22text\x22:\x22Canción\x22\x7d,\x7b\x22text\x22:\x22 • \x22\x7d,\x7b\x22text\x22:\x22Bad Bunny\x22\x7d,\x7b\x22text\x22:\x22 • \x22\x7d,\x7b\x22text\x22:\x224:03\x22\x7d\x5d\x7d\x7d\x7d\x5d\x7d\x7d\x5d\x7d'}); y";
        let found = first_song(&results(html).unwrap()).unwrap();
        assert_eq!(found, Found { id: "abcdefghijk".into(), list: None, title: "Tití Me Preguntó".into(), artist: "Bad Bunny".into() });
    }

    #[test]
    fn hears_youtube_music_by_name() {
        assert_eq!(asked_by_name("pon Tití me preguntó en YouTube Music").as_deref(), Some("Tití me preguntó"));
        assert_eq!(asked_by_name("reproduce música para estudiar en yt music").as_deref(), Some("música para estudiar"));
        assert_eq!(asked_by_name("pon Bad Bunny"), None);
        assert_eq!(asked_by_name("abre YouTube Music"), None);
    }

    #[test]
    #[ignore]
    fn searches_youtube_music_for_real() {
        let found = search("bad bunny titi me pregunto").unwrap();
        println!("[ytmusic] {found:?}");
        assert_eq!(found.id.len(), 11);
    }
}
