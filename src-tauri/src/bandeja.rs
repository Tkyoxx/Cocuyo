// lo que sueltas en la isla: se copia aca, el original no se toca, y a la semana se va

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

const KEEP: Duration = Duration::from_secs(7 * 24 * 3600);
const MAX_BYTES: u64 = 200 * 1024 * 1024;
const MAX_TEXT: usize = 400_000;
const MAX_PAGES: u32 = 40;
const ASK_WINDOW: Duration = Duration::from_secs(600);
const TEXTS: [&str; 41] = [
    "txt", "md", "csv", "tsv", "json", "jsonl", "log", "ini", "cfg", "conf", "toml", "yaml", "yml", "xml", "html", "htm", "css", "js", "jsx", "ts", "tsx",
    "rs", "py", "ps1", "bat", "sh", "c", "h", "cpp", "hpp", "cs", "java", "kt", "go", "php", "rb", "lua", "sql", "srt", "vtt", "tex",
];
const IMAGES: [&str; 7] = ["png", "jpg", "jpeg", "bmp", "gif", "webp", "tif"];
const DOCS: [&str; 5] = ["docx", "pptx", "xlsx", "odt", "odp"];
const UNKNOWN: &str = "De ese tipo de archivo no sé leer. Sirven imágenes, PDF, Word, Excel, PowerPoint y archivos de texto.";

static ASKING: Mutex<Option<(PathBuf, Instant)>> = Mutex::new(None);
static TALK: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());
static READ: Mutex<Option<(PathBuf, Read)>> = Mutex::new(None);
static READING: Mutex<()> = Mutex::new(());

#[derive(Clone, Serialize)]
pub struct Dropped {
    pub name: String,
    pub path: String,
    pub kind: &'static str,
    pub bytes: u64,
}

#[derive(Clone)]
enum Read {
    Text(String),
    Image(Vec<u8>),
}

#[derive(Clone, Serialize)]
struct Answered {
    pregunta: String,
    respuesta: Option<String>,
    error: Option<String>,
}

#[derive(Clone, Serialize)]
struct Opened {
    archivo: Dropped,
    pregunta: String,
    respuesta: String,
}

fn dir(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_local_data_dir().ok()?.join("bandeja"))
}

fn kind_of(path: &Path) -> &'static str {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if IMAGES.contains(&ext.as_str()) {
        "imagen"
    } else if TEXTS.contains(&ext.as_str()) {
        "texto"
    } else if ext == "pdf" {
        "pdf"
    } else if DOCS.contains(&ext.as_str()) {
        "documento"
    } else {
        "otro"
    }
}

fn name_of(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

pub fn free_name(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let file = Path::new(name);
    let stem = file.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| name.to_string());
    let ext = file.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    (2..).map(|n| dir.join(format!("{stem} ({n}){ext}"))).find(|candidate| !candidate.exists()).unwrap_or(first)
}

fn sweep(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let old = entry.metadata().and_then(|m| m.modified()).ok().and_then(|at| now.duration_since(at).ok()).is_some_and(|age| age > KEEP);
        if old {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[tauri::command]
pub fn bandeja_recibir(app: AppHandle, paths: Vec<String>) -> Result<Vec<Dropped>, String> {
    let dir = dir(&app).ok_or("No encontré la carpeta de Cocuyo.")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    sweep(&dir);
    let mut kept = Vec::new();
    for path in paths.iter().map(PathBuf::from).take(10) {
        let Ok(meta) = std::fs::metadata(&path) else { continue };
        if !meta.is_file() || meta.len() > MAX_BYTES {
            continue;
        }
        let name = name_of(&path);
        if name.is_empty() {
            continue;
        }
        let copy = free_name(&dir, &name);
        if std::fs::copy(&path, &copy).is_err() {
            continue;
        }
        kept.push(Dropped { name, kind: kind_of(&copy), path: copy.display().to_string(), bytes: meta.len() });
    }
    if kept.is_empty() {
        return Err(if paths.is_empty() { "Eso no era un archivo.".into() } else { "No pude copiar lo que soltaste (¿es una carpeta o pesa más de 200 MB?).".into() });
    }
    Ok(kept)
}

fn under(path: &Path, dir: &Path) -> bool {
    let plain = path.components().all(|part| !matches!(part, std::path::Component::ParentDir)) && path.starts_with(dir);
    plain || path.canonicalize().ok().zip(dir.canonicalize().ok()).is_some_and(|(file, dir)| file.starts_with(dir))
}

fn owned(app: &AppHandle, path: &str) -> Result<PathBuf, String> {
    let dir = dir(app).ok_or("No encontré la carpeta de Cocuyo.")?;
    let path = PathBuf::from(path);
    let captured = crate::settings::capture_dir(app, &crate::settings::load(app)).is_some_and(|capture| under(&path, &capture));
    if path.is_file() && (under(&path, &dir) || captured) {
        Ok(path)
    } else {
        Err("Ese archivo no llegó soltado ni es una captura.".into())
    }
}

fn stream_of(bytes: &[u8]) -> Result<windows::Storage::Streams::InMemoryRandomAccessStream, String> {
    use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};
    let fail = |e: windows::core::Error| e.to_string();
    let stream = InMemoryRandomAccessStream::new().map_err(fail)?;
    let writer = DataWriter::CreateDataWriter(&stream).map_err(fail)?;
    writer.WriteBytes(bytes).map_err(fail)?;
    writer.StoreAsync().map_err(fail)?.join().map_err(fail)?;
    writer.DetachStream().map_err(fail)?;
    stream.Seek(0).map_err(fail)?;
    Ok(stream)
}

fn bytes_of(stream: &windows::Storage::Streams::InMemoryRandomAccessStream) -> Result<Vec<u8>, String> {
    use windows::Storage::Streams::DataReader;
    let fail = |e: windows::core::Error| e.to_string();
    let size = stream.Size().map_err(fail)? as u32;
    let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0).map_err(fail)?).map_err(fail)?;
    reader.LoadAsync(size).map_err(fail)?.join().map_err(fail)?;
    let mut bytes = vec![0u8; size as usize];
    reader.ReadBytes(&mut bytes).map_err(fail)?;
    Ok(bytes)
}

pub fn as_png(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        return Ok(bytes.to_vec());
    }
    use windows::Graphics::Imaging::{BitmapDecoder, BitmapEncoder};
    use windows::Storage::Streams::InMemoryRandomAccessStream;
    let fail = |e: windows::core::Error| e.to_string();
    let input = stream_of(bytes)?;
    let decoder = BitmapDecoder::CreateAsync(&input).map_err(|_| "No sé abrir esa imagen.".to_string())?.join().map_err(|_| "No sé abrir esa imagen.".to_string())?;
    let bitmap = decoder.GetSoftwareBitmapAsync().map_err(fail)?.join().map_err(fail)?;
    let output = InMemoryRandomAccessStream::new().map_err(fail)?;
    let encoder = BitmapEncoder::CreateAsync(BitmapEncoder::PngEncoderId().map_err(fail)?, &output).map_err(fail)?.join().map_err(fail)?;
    encoder.SetSoftwareBitmap(&bitmap).map_err(fail)?;
    encoder.FlushAsync().map_err(fail)?.join().map_err(fail)?;
    bytes_of(&output)
}

fn read_text(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_TEXT)]).to_string();
    if text.trim().is_empty() {
        return Err("El archivo está vacío.".into());
    }
    Ok(text)
}

fn pdf_text(app: &AppHandle, path: &Path) -> Result<String, String> {
    use windows::Data::Pdf::{PdfDocument, PdfPageRenderOptions};
    use windows::Graphics::Imaging::BitmapEncoder;
    use windows::Storage::Streams::InMemoryRandomAccessStream;
    let fail = |e: windows::core::Error| e.to_string();
    let stream = stream_of(&std::fs::read(path).map_err(|e| e.to_string())?)?;
    let doc = PdfDocument::LoadFromStreamAsync(&stream)
        .map_err(fail)?
        .join()
        .map_err(|_| "No pude abrir ese PDF. ¿Tiene contraseña o está dañado?".to_string())?;
    let pages = doc.PageCount().map_err(fail)?;
    let read = pages.min(MAX_PAGES);
    let mut text = String::new();
    for index in 0..read {
        let _ = app.emit_to(crate::overlay::LABEL, "bandeja-leyendo", (index + 1, read));
        let page = doc.GetPage(index).map_err(fail)?;
        let width = (page.Size().map_err(fail)?.Width * 2.0).clamp(900.0, 2000.0) as u32;
        let options = PdfPageRenderOptions::new().map_err(fail)?;
        options.SetDestinationWidth(width).map_err(fail)?;
        options.SetBitmapEncoderId(BitmapEncoder::PngEncoderId().map_err(fail)?).map_err(fail)?;
        let out = InMemoryRandomAccessStream::new().map_err(fail)?;
        page.RenderWithOptionsToStreamAsync(&out, &options).map_err(fail)?.join().map_err(fail)?;
        let words = crate::ocr::read_png(&bytes_of(&out)?).unwrap_or_default();
        if !words.trim().is_empty() {
            text.push_str(&format!("[Página {}]\n{}\n\n", index + 1, words.trim()));
        }
    }
    if text.trim().is_empty() {
        return Err("No encontré texto en ese PDF.".into());
    }
    if pages > read {
        text.push_str(&format!("[El PDF tiene {pages} páginas; aquí van las primeras {read}.]"));
    }
    Ok(text)
}

fn entries(zip: &[u8], wanted: impl Fn(&str) -> bool) -> Vec<(String, String)> {
    zip_files(zip, wanted, 64 << 20).into_iter().map(|(name, raw)| (name, String::from_utf8_lossy(&raw).into_owned())).collect()
}

pub fn zip_files(zip: &[u8], wanted: impl Fn(&str) -> bool, limit: usize) -> Vec<(String, Vec<u8>)> {
    let u16_at = |i: usize| zip.get(i..i + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize);
    let u32_at = |i: usize| zip.get(i..i + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
    let Some(end) = (0..zip.len().saturating_sub(21)).rev().take(70_000).find(|&i| u32_at(i) == Some(0x0605_4b50)) else { return Vec::new() };
    let (Some(count), Some(mut at)) = (u16_at(end + 10), u32_at(end + 16)) else { return Vec::new() };
    let mut found = Vec::new();
    for _ in 0..count {
        if u32_at(at) != Some(0x0201_4b50) {
            break;
        }
        let (Some(method), Some(size), Some(name_len), Some(extra_len), Some(comment_len), Some(local)) =
            (u16_at(at + 10), u32_at(at + 20), u16_at(at + 28), u16_at(at + 30), u16_at(at + 32), u32_at(at + 42))
        else {
            break;
        };
        let name = String::from_utf8_lossy(zip.get(at + 46..at + 46 + name_len).unwrap_or_default()).to_string();
        at += 46 + name_len + extra_len + comment_len;
        if !wanted(&name) {
            continue;
        }
        let (Some(local_name), Some(local_extra)) = (u16_at(local + 26), u16_at(local + 28)) else { continue };
        let start = local + 30 + local_name + local_extra;
        let Some(data) = zip.get(start..start + size) else { continue };
        let raw = match method {
            0 => data.to_vec(),
            8 => match miniz_oxide::inflate::decompress_to_vec_with_limit(data, limit) {
                Ok(raw) => raw,
                Err(_) => continue,
            },
            _ => continue,
        };
        found.push((name, raw));
    }
    found
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let Some(end) = rest.find(';').filter(|&end| end <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix("#x")
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|dec| dec.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

pub fn xml_text(xml: &str) -> String {
    let mut out = String::new();
    let mut skipping = false;
    let mut rest = xml;
    while let Some(open) = rest.find('<') {
        if !skipping {
            out.push_str(&unescape(&rest[..open]));
        }
        let Some(close) = rest[open..].find('>') else { break };
        let tag = &rest[open + 1..open + close];
        let name = tag.trim_start_matches('/').split(|c: char| c.is_whitespace() || c == '/').next().unwrap_or("");
        if matches!(name, "w:instrText" | "w:delText") {
            skipping = !tag.starts_with('/') && !tag.ends_with('/');
        } else if tag.starts_with('/') && matches!(name, "w:p" | "a:p" | "text:p" | "text:h") {
            out.push('\n');
        } else if matches!(name, "w:tab" | "text:tab") {
            out.push('\t');
        } else if matches!(name, "w:br" | "a:br" | "text:line-break") {
            out.push('\n');
        }
        rest = &rest[open + close + 1..];
    }
    if !skipping {
        out.push_str(&unescape(rest));
    }
    let mut tidy = String::new();
    let mut blank = 0;
    for line in out.lines().map(str::trim_end) {
        blank = if line.trim().is_empty() { blank + 1 } else { 0 };
        if blank < 2 {
            tidy.push_str(line);
            tidy.push('\n');
        }
    }
    tidy.trim().to_string()
}

fn number_in(name: &str) -> u32 {
    name.chars().filter(char::is_ascii_digit).collect::<String>().parse().unwrap_or(0)
}

fn between<'a>(text: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let start = text.find(open)? + open.len();
    let end = text[start..].find(close)?;
    Some(&text[start..start + end])
}

fn sheets_text(zip: &[u8]) -> String {
    let shared: Vec<String> = entries(zip, |n| n == "xl/sharedStrings.xml")
        .first()
        .map(|(_, xml)| xml.split("<si").skip(1).map(|si| xml_text(&format!("<si{si}")).replace('\n', " ")).collect())
        .unwrap_or_default();
    let mut sheets = entries(zip, |n| n.starts_with("xl/worksheets/sheet") && n.ends_with(".xml"));
    sheets.sort_by_key(|(name, _)| number_in(name));
    let mut out = String::new();
    for (index, (_, xml)) in sheets.iter().enumerate().take(20) {
        out.push_str(&format!("[Hoja {}]\n", index + 1));
        let mut row = 0;
        let mut line: Vec<String> = Vec::new();
        let mut rows = 0;
        for cell in xml.split("<c ").skip(1) {
            let head = &cell[..cell.find('>').unwrap_or(cell.len())];
            let at = between(head, "r=\"", "\"").map(number_in).unwrap_or(row);
            if at != row && !line.is_empty() {
                out.push_str(&line.join(" | "));
                out.push('\n');
                line.clear();
                rows += 1;
            }
            row = at;
            if rows >= 3000 {
                break;
            }
            if head.ends_with('/') {
                continue;
            }
            let body = &cell[..cell.find("</c>").unwrap_or(cell.len())];
            let value = between(body, "<v>", "</v>").unwrap_or("");
            let kind = between(head, "t=\"", "\"").unwrap_or("");
            let shown = match kind {
                "s" => value.parse::<usize>().ok().and_then(|i| shared.get(i).cloned()).unwrap_or_default(),
                "inlineStr" => xml_text(body),
                "b" => if value == "1" { "VERDADERO".into() } else { "FALSO".into() },
                _ => unescape(value),
            };
            if !shown.trim().is_empty() {
                line.push(shown.trim().to_string());
            }
        }
        if !line.is_empty() {
            out.push_str(&line.join(" | "));
            out.push('\n');
        }
        out.push('\n');
    }
    out.trim().to_string()
}

pub fn office_text(zip: &[u8], ext: &str) -> String {
    match ext {
        "docx" => entries(zip, |n| n == "word/document.xml").iter().map(|(_, xml)| xml_text(xml)).collect::<Vec<_>>().join("\n"),
        "pptx" => {
            let mut slides = entries(zip, |n| n.starts_with("ppt/slides/slide") && n.ends_with(".xml"));
            slides.sort_by_key(|(name, _)| number_in(name));
            slides.iter().enumerate().map(|(i, (_, xml))| format!("[Diapositiva {}]\n{}", i + 1, xml_text(xml))).collect::<Vec<_>>().join("\n\n")
        }
        "xlsx" => sheets_text(zip),
        _ => entries(zip, |n| n == "content.xml").iter().map(|(_, xml)| xml_text(xml)).collect::<Vec<_>>().join("\n"),
    }
}

fn doc_text(path: &Path) -> Result<String, String> {
    let zip = std::fs::read(path).map_err(|e| e.to_string())?;
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let text = office_text(&zip, &ext);
    if text.trim().is_empty() {
        return Err("No encontré texto en ese documento.".into());
    }
    Ok(text.chars().take(MAX_TEXT).collect())
}

fn read(app: &AppHandle, path: &Path) -> Result<Read, String> {
    let _busy = READING.lock().unwrap();
    if let Some((cached, read)) = READ.lock().unwrap().as_ref() {
        if cached == path {
            return Ok(read.clone());
        }
    }
    let read = match kind_of(path) {
        "imagen" => Read::Image(as_png(&std::fs::read(path).map_err(|e| e.to_string())?)?),
        "texto" => Read::Text(read_text(path)?),
        "pdf" => Read::Text(pdf_text(app, path)?),
        "documento" => Read::Text(doc_text(path)?),
        _ => return Err(UNKNOWN.into()),
    };
    *READ.lock().unwrap() = Some((path.to_path_buf(), read.clone()));
    Ok(read)
}

#[tauri::command]
pub async fn bandeja_texto(app: AppHandle, path: String) -> Result<String, String> {
    let path = owned(&app, &path)?;
    let text = tauri::async_runtime::spawn_blocking(move || match read(&app, &path)? {
        Read::Text(text) => Ok(text),
        Read::Image(png) => crate::ocr::read_png(&png),
    })
    .await
    .map_err(|e| e.to_string())??;
    if text.trim().is_empty() {
        return Err("No encontré texto ahí.".into());
    }
    crate::clip::ignore_own_changes();
    crate::inject::set_clipboard_text(&text);
    Ok(format!("Copié {} palabras.", text.split_whitespace().count()))
}

fn start(path: &Path) {
    *ASKING.lock().unwrap() = Some((path.to_path_buf(), Instant::now()));
    TALK.lock().unwrap().clear();
}

#[tauri::command]
pub async fn bandeja_elegir(app: AppHandle) -> Result<Option<Dropped>, String> {
    let Some(path) = tauri::async_runtime::spawn_blocking(crate::dialogs::pick_document).await.map_err(|e| e.to_string())? else { return Ok(None) };
    bandeja_recibir(app, vec![path.display().to_string()]).map(|kept| kept.into_iter().next())
}

#[tauri::command]
pub fn bandeja_abrir(app: AppHandle, path: String) -> Result<(), String> {
    let path = owned(&app, &path)?;
    if kind_of(&path) == "otro" {
        return Err(UNKNOWN.into());
    }
    if crate::ai::mode(&app).is_none() {
        return Err("Para preguntar por un archivo necesitas un modelo de IA (Ajustes → IA).".into());
    }
    start(&path);
    std::thread::spawn(move || {
        let _ = read(&app, &path);
    });
    Ok(())
}

#[tauri::command]
pub fn bandeja_cerrar() {
    *ASKING.lock().unwrap() = None;
    TALK.lock().unwrap().clear();
}

#[tauri::command]
pub async fn bandeja_preguntar(app: AppHandle, path: String, pregunta: String) -> Result<String, String> {
    let path = owned(&app, &path)?;
    tauri::async_runtime::spawn_blocking(move || ask(&app, &path, pregunta.trim())).await.map_err(|e| e.to_string())?
}

fn ask(app: &AppHandle, path: &Path, question: &str) -> Result<String, String> {
    if question.is_empty() {
        return Err("Escribe algo.".into());
    }
    let ours = {
        let mut asking = ASKING.lock().unwrap();
        match asking.as_mut().filter(|(asked, _)| asked == path) {
            Some((_, since)) => {
                *since = Instant::now();
                true
            }
            None => false,
        }
    };
    let history = if ours { TALK.lock().unwrap().clone() } else { Vec::new() };
    let name = name_of(path);
    let answer = match read(app, path)? {
        Read::Text(text) => crate::ai::ask_file(app, question, &name, Some(&text), None, &history),
        Read::Image(png) => crate::ai::ask_file(app, question, &name, None, Some(&png), &history).or_else(|seeing| {
            let text = crate::ocr::read_png(&png).ok().filter(|text| text.split_whitespace().count() > 3).ok_or(seeing)?;
            crate::ai::ask_file(app, question, &name, Some(&text), None, &history)
        }),
    }?;
    if ours {
        let mut talk = TALK.lock().unwrap();
        talk.push((question.to_string(), answer.clone()));
        let extra = talk.len().saturating_sub(6);
        talk.drain(..extra);
    }
    Ok(answer)
}

pub fn waiting() -> bool {
    ASKING.lock().unwrap().as_ref().is_some_and(|(_, since)| since.elapsed() < ASK_WINDOW)
}

pub fn answer(app: &AppHandle, question: &str) -> Option<Result<String, String>> {
    let path = ASKING.lock().unwrap().as_ref().filter(|(_, since)| since.elapsed() < ASK_WINDOW).map(|(path, _)| path.clone())?;
    let done = ask(app, &path, question);
    let (respuesta, error) = match done {
        Ok(answer) => (Some(answer), None),
        Err(why) => (None, Some(crate::i18n::tr(&why))),
    };
    let _ = app.emit_to(crate::overlay::LABEL, "bandeja-respuesta", Answered { pregunta: question.to_string(), respuesta, error });
    Some(Ok(String::new()))
}

pub fn about_text(app: &AppHandle, text: &str, question: &str) -> Result<String, String> {
    let dir = dir(app).ok_or("No encontré la carpeta de Cocuyo.")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = free_name(&dir, "Texto seleccionado.txt");
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    start(&path);
    let answer = ask(app, &path, question).inspect_err(|_| bandeja_cerrar())?;
    let archivo = Dropped { name: name_of(&path), path: path.display().to_string(), kind: "texto", bytes: text.len() as u64 };
    let _ = app.emit_to(crate::overlay::LABEL, "bandeja-abrir", Opened { archivo, pregunta: question.to_string(), respuesta: answer });
    Ok(String::new())
}

#[tauri::command]
pub async fn anotar_abrir(app: AppHandle, path: String) -> Result<String, String> {
    let path = owned(&app, &path)?;
    let png = as_png(&std::fs::read(&path).map_err(|e| e.to_string())?)?;
    Ok(format!("data:image/png;base64,{}", crate::ai::base64(&png)))
}

#[tauri::command]
pub async fn anotar_guardar(app: AppHandle, path: String, png: String, copiar: bool, guardar: bool) -> Result<String, String> {
    let path = owned(&app, &path)?;
    let bytes = crate::ai::unbase64(&png);
    if !bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        return Err("La imagen anotada llegó dañada.".into());
    }
    let mut said = Vec::new();
    if guardar {
        let dir = path.parent().ok_or("No encontré la carpeta de la captura.")?;
        let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Captura".into());
        let target = free_name(dir, &format!("{stem} anotada.png"));
        std::fs::write(&target, &bytes).map_err(|e| e.to_string())?;
        said.push(format!("Guardada: {}", name_of(&target)));
    }
    if copiar {
        if !crate::clip::copy_png(&bytes) {
            return Err("No pude dejarla copiada.".into());
        }
        said.push("Copiada.".to_string());
    }
    Ok(said.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_repeated_name_never_overwrites() {
        let dir = std::env::temp_dir().join("cocuyo-prueba-bandeja");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(free_name(&dir, "factura.pdf"), dir.join("factura.pdf"));
        std::fs::write(dir.join("factura.pdf"), b"1").unwrap();
        assert_eq!(free_name(&dir, "factura.pdf"), dir.join("factura (2).pdf"));
        std::fs::write(dir.join("factura (2).pdf"), b"2").unwrap();
        assert_eq!(free_name(&dir, "factura.pdf"), dir.join("factura (3).pdf"));
        assert_eq!(free_name(&dir, "LEEME"), dir.join("LEEME"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_files_inside_the_tray_count() {
        let tray = Path::new(r"C:\Users\Ana\AppData\Local\cocuyo\bandeja");
        assert!(under(Path::new(r"C:\Users\Ana\AppData\Local\cocuyo\bandeja\factura.pdf"), tray));
        assert!(!under(Path::new(r"C:\Users\Ana\AppData\Local\cocuyo\bandeja\..\ajustes.json"), tray));
        assert!(!under(Path::new(r"C:\Users\Ana\Documents\factura.pdf"), tray));
    }

    #[test]
    fn each_file_gets_its_kind() {
        assert_eq!(kind_of(Path::new("a/foto.JPG")), "imagen");
        assert_eq!(kind_of(Path::new("notas.md")), "texto");
        assert_eq!(kind_of(Path::new("contrato.pdf")), "pdf");
        assert_eq!(kind_of(Path::new("informe.DOCX")), "documento");
        assert_eq!(kind_of(Path::new("cuentas.xlsx")), "documento");
        assert_eq!(kind_of(Path::new("juego.exe")), "otro");
        assert_eq!(kind_of(Path::new("sin_extension")), "otro");
    }

    #[test]
    fn word_xml_keeps_paragraphs_and_skips_field_codes() {
        let xml = r#"<w:document><w:body><w:p><w:r><w:t>Hola &amp; adiós</w:t></w:r></w:p><w:p><w:r><w:instrText> HYPERLINK "x" </w:instrText><w:t xml:space="preserve">Segunda</w:t><w:tab/><w:t>línea</w:t></w:r></w:p></w:body></w:document>"#;
        assert_eq!(xml_text(xml), "Hola & adiós\nSegunda\tlínea");
        assert_eq!(unescape("a &#241; b &#xF1; &raro c"), "a ñ b ñ &raro c");
    }

    fn zip_of(files: &[(&str, &str)]) -> Vec<u8> {
        let mut zip = Vec::new();
        let mut central = Vec::new();
        for (name, body) in files {
            let packed = miniz_oxide::deflate::compress_to_vec(body.as_bytes(), 6);
            let local = zip.len() as u32;
            zip.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            zip.extend_from_slice(&[20, 0, 0, 0, 8, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            zip.extend_from_slice(&(packed.len() as u32).to_le_bytes());
            zip.extend_from_slice(&(body.len() as u32).to_le_bytes());
            zip.extend_from_slice(&(name.len() as u16).to_le_bytes());
            zip.extend_from_slice(&[0, 0]);
            zip.extend_from_slice(name.as_bytes());
            zip.extend_from_slice(&packed);
            central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            central.extend_from_slice(&[20, 0, 20, 0, 0, 0, 8, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            central.extend_from_slice(&(packed.len() as u32).to_le_bytes());
            central.extend_from_slice(&(body.len() as u32).to_le_bytes());
            central.extend_from_slice(&(name.len() as u16).to_le_bytes());
            central.extend_from_slice(&[0; 12]);
            central.extend_from_slice(&local.to_le_bytes());
            central.extend_from_slice(name.as_bytes());
        }
        let at = zip.len() as u32;
        zip.extend_from_slice(&central);
        zip.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        zip.extend_from_slice(&[0, 0, 0, 0]);
        zip.extend_from_slice(&(files.len() as u16).to_le_bytes());
        zip.extend_from_slice(&(files.len() as u16).to_le_bytes());
        zip.extend_from_slice(&(central.len() as u32).to_le_bytes());
        zip.extend_from_slice(&at.to_le_bytes());
        zip.extend_from_slice(&[0, 0]);
        zip
    }

    #[test]
    fn reads_word_powerpoint_and_excel() {
        let docx = zip_of(&[("word/document.xml", "<w:p><w:t>Contrato de arriendo</w:t></w:p><w:p><w:t>Valor: 900.000</w:t></w:p>")]);
        assert_eq!(office_text(&docx, "docx"), "Contrato de arriendo\nValor: 900.000");
        let pptx = zip_of(&[("ppt/slides/slide2.xml", "<a:p><a:t>Cierre</a:t></a:p>"), ("ppt/slides/slide1.xml", "<a:p><a:t>Portada</a:t></a:p>")]);
        assert_eq!(office_text(&pptx, "pptx"), "[Diapositiva 1]\nPortada\n\n[Diapositiva 2]\nCierre");
        let xlsx = zip_of(&[
            ("xl/sharedStrings.xml", "<sst><si><t>Nombre</t></si><si><t>Ana</t></si></sst>"),
            ("xl/worksheets/sheet1.xml", r#"<row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1"><v>Edad</v></c></row><row r="2"><c r="A2" t="s"><v>1</v></c><c r="B2"><v>31</v></c><c r="C2" s="1"/></row>"#),
        ]);
        assert_eq!(office_text(&xlsx, "xlsx"), "[Hoja 1]\nNombre | Edad\nAna | 31");
    }
}
