// lo que sueltas en la isla: se copia aca, el original no se toca, y a la semana se va

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use serde::Serialize;
use tauri::{AppHandle, Manager};

const KEEP: Duration = Duration::from_secs(7 * 24 * 3600);
const MAX_BYTES: u64 = 200 * 1024 * 1024;
const MAX_TEXT: usize = 200_000;
const ASK_WINDOW: Duration = Duration::from_secs(120);
const TEXTS: [&str; 22] = [
    "txt", "md", "csv", "json", "log", "ini", "toml", "yaml", "yml", "xml", "html", "css", "js", "ts", "rs", "py", "ps1", "bat", "c", "cpp", "java", "srt",
];
const IMAGES: [&str; 7] = ["png", "jpg", "jpeg", "bmp", "gif", "webp", "tif"];

static ASKING: Mutex<Option<(PathBuf, Instant)>> = Mutex::new(None);

#[derive(Clone, Serialize)]
pub struct Dropped {
    pub name: String,
    pub path: String,
    pub kind: &'static str,
    pub bytes: u64,
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
    } else {
        "otro"
    }
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
        let Some(name) = path.file_name().map(|n| n.to_string_lossy().to_string()) else { continue };
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

fn owned(app: &AppHandle, path: &str) -> Result<PathBuf, String> {
    let dir = dir(app).ok_or("No encontré la carpeta de Cocuyo.")?;
    let path = PathBuf::from(path);
    let inside = path.canonicalize().ok().zip(dir.canonicalize().ok()).is_some_and(|(file, dir)| file.starts_with(dir));
    let capture = crate::settings::capture_dir(app, &crate::settings::load(app)).and_then(|d| d.canonicalize().ok());
    let captured = capture.zip(path.canonicalize().ok()).is_some_and(|(dir, file)| file.starts_with(dir));
    if inside || captured {
        Ok(path)
    } else {
        Err("Ese archivo no llegó soltado ni es una captura.".into())
    }
}

pub fn as_png(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        return Ok(bytes.to_vec());
    }
    use windows::Graphics::Imaging::{BitmapDecoder, BitmapEncoder};
    use windows::Storage::Streams::{DataReader, DataWriter, InMemoryRandomAccessStream};
    let fail = |e: windows::core::Error| e.to_string();
    let input = InMemoryRandomAccessStream::new().map_err(fail)?;
    let writer = DataWriter::CreateDataWriter(&input).map_err(fail)?;
    writer.WriteBytes(bytes).map_err(fail)?;
    writer.StoreAsync().map_err(fail)?.join().map_err(fail)?;
    writer.DetachStream().map_err(fail)?;
    input.Seek(0).map_err(fail)?;
    let decoder = BitmapDecoder::CreateAsync(&input).map_err(|_| "No sé abrir esa imagen.".to_string())?.join().map_err(|_| "No sé abrir esa imagen.".to_string())?;
    let bitmap = decoder.GetSoftwareBitmapAsync().map_err(fail)?.join().map_err(fail)?;
    let output = InMemoryRandomAccessStream::new().map_err(fail)?;
    let encoder = BitmapEncoder::CreateAsync(BitmapEncoder::PngEncoderId().map_err(fail)?, &output).map_err(fail)?.join().map_err(fail)?;
    encoder.SetSoftwareBitmap(&bitmap).map_err(fail)?;
    encoder.FlushAsync().map_err(fail)?.join().map_err(fail)?;
    let size = output.Size().map_err(fail)? as u32;
    let reader = DataReader::CreateDataReader(&output.GetInputStreamAt(0).map_err(fail)?).map_err(fail)?;
    reader.LoadAsync(size).map_err(fail)?.join().map_err(fail)?;
    let mut png = vec![0u8; size as usize];
    reader.ReadBytes(&mut png).map_err(fail)?;
    Ok(png)
}

fn read_text(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_TEXT)]).to_string();
    if text.trim().is_empty() {
        return Err("El archivo está vacío.".into());
    }
    Ok(text)
}

#[tauri::command]
pub async fn bandeja_texto(app: AppHandle, path: String) -> Result<String, String> {
    let path = owned(&app, &path)?;
    let text = match kind_of(&path) {
        "imagen" => {
            let png = as_png(&std::fs::read(&path).map_err(|e| e.to_string())?)?;
            crate::ocr::read_png(&png)?
        }
        "texto" => read_text(&path)?,
        _ => return Err("De este tipo de archivo no sé sacar el texto. Sirven imágenes y archivos de texto.".into()),
    };
    if text.trim().is_empty() {
        return Err("No encontré texto ahí.".into());
    }
    crate::clip::ignore_own_changes();
    crate::inject::set_clipboard_text(&text);
    Ok(format!("Copié {} palabras.", text.split_whitespace().count()))
}

#[tauri::command]
pub fn bandeja_preguntar(app: AppHandle, path: String) -> Result<(), String> {
    let path = owned(&app, &path)?;
    if !matches!(kind_of(&path), "imagen" | "texto") {
        return Err("Por ahora solo puedo mirar imágenes y archivos de texto.".into());
    }
    if crate::ai::mode(&app).is_none() {
        return Err("Para preguntar por un archivo necesitas un modelo de IA (Ajustes → IA).".into());
    }
    *ASKING.lock().unwrap() = Some((path, Instant::now()));
    Ok(())
}

pub fn waiting() -> bool {
    ASKING.lock().unwrap().as_ref().is_some_and(|(_, since)| since.elapsed() < ASK_WINDOW)
}

pub fn answer(app: &AppHandle, question: &str) -> Option<Result<String, String>> {
    let (path, since) = ASKING.lock().unwrap().take()?;
    if since.elapsed() >= ASK_WINDOW {
        return None;
    }
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    Some(match kind_of(&path) {
        "imagen" => std::fs::read(&path).map_err(|e| e.to_string()).and_then(|bytes| as_png(&bytes)).and_then(|png| crate::ai::see(app, question, &png)),
        _ => read_text(&path).and_then(|text| crate::ai::about_file(app, question, &name, &text)),
    })
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
        said.push(format!("Guardada: {}", target.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()));
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
    fn each_file_gets_its_kind() {
        assert_eq!(kind_of(Path::new("a/foto.JPG")), "imagen");
        assert_eq!(kind_of(Path::new("notas.md")), "texto");
        assert_eq!(kind_of(Path::new("contrato.pdf")), "pdf");
        assert_eq!(kind_of(Path::new("juego.exe")), "otro");
        assert_eq!(kind_of(Path::new("sin_extension")), "otro");
    }
}
