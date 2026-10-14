// sacar el texto de una imagen. lo hace windows solito

use std::io::Cursor;

use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine;
use windows::Storage::Streams::DataWriter;

pub fn read(bgra: &[u8], w: i32, h: i32) -> Result<String, String> {
    let fail = |e: windows::core::Error| e.to_string();
    let engine = OcrEngine::TryCreateFromUserProfileLanguages().map_err(|_| "Windows no tiene instalado el lector de texto para tu idioma.".to_string())?;
    let writer = DataWriter::new().map_err(fail)?;
    writer.WriteBytes(bgra).map_err(fail)?;
    let pixels = writer.DetachBuffer().map_err(fail)?;
    let bitmap = SoftwareBitmap::CreateCopyFromBuffer(&pixels, BitmapPixelFormat::Bgra8, w, h).map_err(fail)?;
    let seen = engine.RecognizeAsync(&bitmap).map_err(fail)?.join().map_err(fail)?;
    let lines: Vec<String> = seen.Lines().map_err(fail)?.into_iter().filter_map(|line| line.Text().ok().map(|text| text.to_string())).collect();
    Ok(lines.join("\r\n"))
}

pub fn read_png(png: &[u8]) -> Result<String, String> {
    let mut reader = png::Decoder::new(Cursor::new(png)).read_info().map_err(|e| e.to_string())?;
    let mut pixels = vec![0; reader.output_buffer_size().ok_or("La imagen es demasiado grande.")?];
    let info = reader.next_frame(&mut pixels).map_err(|e| e.to_string())?;
    let step = match info.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        _ => return Err("No sé leer ese tipo de imagen.".into()),
    };
    let bgra: Vec<u8> = pixels[..info.buffer_size()].chunks_exact(step).flat_map(|p| [p[2], p[1], p[0], 255]).collect();
    read(&bgra, info.width as i32, info.height as i32)
}

pub fn wants(spoken: &str) -> bool {
    let clean = crate::intent::normalize(spoken);
    ["copia el texto", "copiar el texto", "copiame el texto", "copia lo que dice", "saca el texto", "extrae el texto", "copia las letras", "copy the text", "copia o texto", "copie o texto"]
        .iter()
        .any(|ask| clean.contains(ask))
}

fn deliver(text: Result<String, String>) -> Result<String, String> {
    let text = text?;
    let words = text.split_whitespace().count();
    if words == 0 {
        return Err("No encontré texto en esa imagen.".into());
    }
    crate::clip::ignore_own_changes();
    if !crate::inject::set_clipboard_text(&text) {
        return Err("No se pudo usar el portapapeles.".into());
    }
    Ok(format!("Texto copiado: {words} palabras."))
}

pub fn copy_from_png(png: &[u8]) -> Result<String, String> {
    deliver(read_png(png))
}

pub fn spoken(spoken: &str) -> Option<Result<String, String>> {
    if !wants(spoken) {
        return None;
    }
    Some(match crate::clip::image_png(None) {
        Some(png) => copy_from_png(&png),
        None => Err("No hay ninguna imagen reciente. Rodea algo con el lazo y vuelve a pedirlo.".into()),
    })
}

#[tauri::command]
pub async fn ocr_clip(index: usize) -> Result<String, String> {
    let png = crate::clip::image_png(Some(index)).ok_or("Esa imagen ya no está.")?;
    copy_from_png(&png)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_knows_when_text_is_wanted() {
        assert!(wants("copia el texto de esto") && wants("Cópiame el texto porfa") && wants("copy the text"));
        assert!(!wants("copia esto") && !wants("pon música"));
    }
}
