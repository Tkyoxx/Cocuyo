// extensiones y temas: como spicetify pero para la isla. corren con el mismo poder que la app, asi que ojo con lo que instalas

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::settings;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const GALLERY: &str = "https://raw.githubusercontent.com/Tkyoxx/Cocuyo/main/galeria/extensiones/";
const FILES: [&str; 9] = ["json", "css", "js", "png", "jpg", "webp", "svg", "woff2", "md"];
pub const PERMISSIONS: [&str; 10] = ["red", "portapapeles", "pantalla", "voz", "equipo", "datos", "scripts", "agente", "escritorio", "comandos"];

const README: &str = "Extensiones de Cocuyo\r\n=====================\r\n\r\n\
Cada carpeta de aquí es un tema o una extensión para la isla, al estilo de\r\n\
Spicetify. Dentro va un manifest.json que dice cómo se llama y qué archivos\r\n\
cargar: un estilo.css (los temas) y/o un index.js (las extensiones).\r\n\r\n\
Se activan en Ajustes, sección Extensiones. Si alguna rompe la isla, el\r\n\
icono de Cocuyo junto al reloj tiene «Desactivar extensiones».\r\n\r\n\
Todo explicado en: https://github.com/Tkyoxx/Cocuyo/tree/main/sdk\r\n\r\n\
Una extensión puede hacer lo mismo que Cocuyo. Instala solo las que entiendas.\r\n";

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Texts {
    #[serde(default)]
    nombre: String,
    #[serde(default)]
    descripcion: String,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Manifest {
    #[serde(default)]
    nombre: String,
    #[serde(default)]
    descripcion: String,
    #[serde(default)]
    autor: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    tipo: String,
    #[serde(default)]
    estilo: Option<String>,
    #[serde(default)]
    codigo: Option<String>,
    #[serde(default)]
    vista: Option<String>,
    #[serde(default)]
    permisos: Option<Vec<String>>,
    #[serde(default)]
    traduccion: std::collections::HashMap<String, Texts>,
}

#[derive(Serialize)]
pub struct Extension {
    id: String,
    nombre: String,
    descripcion: String,
    autor: String,
    version: String,
    tipo: String,
    estilo: Option<String>,
    codigo: Option<String>,
    vista: Option<String>,
    permisos: Option<Vec<String>>,
    activa: bool,
}

fn known(permissions: &Option<Vec<String>>) -> Option<Vec<String>> {
    permissions.as_ref().map(|asked| {
        let mut clean: Vec<String> = PERMISSIONS.iter().filter(|known| asked.iter().any(|p| p.trim() == **known)).map(|p| p.to_string()).collect();
        clean.dedup();
        clean
    })
}

fn translated(name: &str, about: &str, texts: &std::collections::HashMap<String, Texts>) -> (String, String) {
    let language = crate::cue::language().0;
    match texts.get(language) {
        Some(texts) if language != "es" => (
            if texts.nombre.trim().is_empty() { name.to_string() } else { texts.nombre.clone() },
            if texts.descripcion.trim().is_empty() { about.to_string() } else { texts.descripcion.clone() },
        ),
        _ => (name.to_string(), about.to_string()),
    }
}

pub fn dir(app: &AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_config_dir().ok()?.join("extensiones");
    std::fs::create_dir_all(&dir).ok()?;
    if std::fs::read_to_string(dir.join("LEEME.txt")).ok().as_deref() != Some(README) {
        let _ = std::fs::write(dir.join("LEEME.txt"), README);
    }
    Some(dir)
}

pub fn allow(app: &AppHandle) {
    let Some(dir) = dir(app) else { return };
    let scope = app.asset_protocol_scope();
    let mut folders = vec![(dir, 0)];
    while let Some((folder, depth)) = folders.pop() {
        for entry in std::fs::read_dir(&folder).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() && depth < 3 {
                folders.push((path, depth + 1));
            } else if path.is_file() {
                let _ = scope.allow_file(&path);
            }
        }
    }
}

fn plain_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 60 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn plain_file(name: &str) -> bool {
    let kind = name.rsplit('.').next().unwrap_or("").to_lowercase();
    !name.is_empty()
        && name.len() <= 80
        && name.contains('.')
        && !name.starts_with('.')
        && !name.contains("..")
        && FILES.contains(&kind.as_str())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

fn inside(folder: &Path, file: &Option<String>) -> Option<String> {
    let file = file.as_deref().filter(|file| plain_file(file))?;
    let path = folder.join(file);
    path.is_file().then(|| path.to_string_lossy().into_owned())
}

pub fn list(app: &AppHandle) -> Vec<Extension> {
    let Some(dir) = dir(app) else { return Vec::new() };
    let enabled = settings::load(app).extensions;
    let Ok(entries) = std::fs::read_dir(&dir) else { return Vec::new() };
    let mut found: Vec<Extension> = entries
        .flatten()
        .filter_map(|entry| {
            let folder = entry.path();
            let id = entry.file_name().to_string_lossy().into_owned();
            if !folder.is_dir() || !plain_id(&id) {
                return None;
            }
            let manifest: Manifest = serde_json::from_slice(&std::fs::read(folder.join("manifest.json")).ok()?).ok()?;
            let tipo = if manifest.tipo == "tema" { "tema" } else { "extension" };
            let (nombre, descripcion) = translated(&manifest.nombre, &manifest.descripcion, &manifest.traduccion);
            Some(Extension {
                nombre: if nombre.trim().is_empty() { id.clone() } else { nombre.chars().take(60).collect() },
                descripcion: descripcion.chars().take(200).collect(),
                autor: manifest.autor.chars().take(40).collect(),
                version: manifest.version.chars().take(20).collect(),
                tipo: tipo.into(),
                estilo: inside(&folder, &manifest.estilo),
                codigo: inside(&folder, &manifest.codigo),
                vista: inside(&folder, &manifest.vista),
                permisos: if tipo == "tema" { Some(Vec::new()) } else { known(&manifest.permisos) },
                activa: enabled.contains(&id),
                id,
            })
        })
        .collect();
    found.sort_by(|a, b| a.tipo.cmp(&b.tipo).then(a.nombre.to_lowercase().cmp(&b.nombre.to_lowercase())));
    found
}

#[tauri::command]
pub fn extensions_list(app: AppHandle) -> Vec<Extension> {
    allow(&app);
    list(&app)
}

#[tauri::command]
pub fn extensions_open(app: AppHandle) -> Result<(), String> {
    let dir = dir(&app).ok_or("No se encontró la carpeta de extensiones.")?;
    crate::apps::open(&dir.to_string_lossy())
}

#[tauri::command]
pub fn extension_set(app: AppHandle, id: String, on: bool) -> Result<(), String> {
    let mut config = settings::load(&app);
    config.extensions.retain(|known| *known != id);
    if on && plain_id(&id) {
        config.extensions.push(id);
    }
    if on {
        config.extensions_on = true;
    }
    settings::set_settings(app, config)
}

#[tauri::command]
pub fn extensions_master(app: AppHandle, on: bool) -> Result<(), String> {
    let mut config = settings::load(&app);
    config.extensions_on = on;
    settings::set_settings(app, config)
}

pub fn safe_mode(app: &AppHandle) {
    let mut config = settings::load(app);
    config.extensions_on = false;
    let _ = settings::set_settings(app.clone(), config);
    let _ = app.emit_to(crate::overlay::LABEL, "extensions-off", ());
}

static COMMANDS: std::sync::Mutex<Vec<Phrases>> = std::sync::Mutex::new(Vec::new());

#[derive(Clone, Deserialize)]
pub struct Phrases {
    id: String,
    frases: Vec<String>,
}

#[tauri::command]
pub fn extension_commands(commands: Vec<Phrases>) {
    let clean: Vec<Phrases> = commands
        .into_iter()
        .take(200)
        .map(|phrases| Phrases {
            id: phrases.id.chars().take(80).collect(),
            frases: phrases.frases.iter().map(|f| crate::intent::normalize(f)).filter(|f| !f.is_empty()).take(20).collect(),
        })
        .filter(|phrases| !phrases.frases.is_empty())
        .collect();
    *COMMANDS.lock().unwrap() = clean;
}

fn matching(commands: &[Phrases], spoken: &str) -> Option<String> {
    let said = crate::intent::normalize(spoken);
    let words: Vec<&str> = said.split_whitespace().collect();
    commands.iter().find_map(|phrases| {
        phrases.frases.iter().any(|phrase| {
            let take = phrase.split_whitespace().count();
            take > 0 && take <= words.len() && crate::intent::similarity(&words[..take].join(" "), phrase) >= 0.86
        })
        .then(|| phrases.id.clone())
    })
}

pub fn spoken(app: &AppHandle, spoken: &str) -> Option<Result<String, String>> {
    if !settings::load(app).extensions_on {
        return None;
    }
    let id = matching(&COMMANDS.lock().unwrap(), spoken)?;
    let _ = app.emit_to(crate::overlay::LABEL, "extension-command", serde_json::json!({ "id": id, "texto": spoken }));
    Some(Ok(String::new()))
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SharedExtension {
    id: String,
    nombre: String,
    #[serde(default)]
    descripcion: String,
    #[serde(default)]
    autor: String,
    #[serde(default)]
    tipo: String,
    #[serde(default)]
    archivos: Vec<String>,
    #[serde(default)]
    vista: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    permisos: Option<Vec<String>>,
    #[serde(default)]
    traduccion: std::collections::HashMap<String, Texts>,
    #[serde(default, skip_deserializing)]
    have: bool,
    #[serde(default, skip_deserializing)]
    newer: bool,
}

fn installed_version(dir: &Path, id: &str) -> Option<String> {
    let manifest: Manifest = serde_json::from_slice(&std::fs::read(dir.join(id).join("manifest.json")).ok()?).ok()?;
    Some(manifest.version)
}

fn fetch(url: &str) -> Result<Vec<u8>, String> {
    let output = Command::new("curl.exe")
        .args(["--silent", "--location", "--fail", "--max-time", "20", url])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err("No se pudo llegar a la galería. Revisa tu conexión.".into())
    }
}

#[tauri::command]
pub async fn extensions_gallery(app: AppHandle) -> Result<Vec<SharedExtension>, String> {
    let listed: Vec<SharedExtension> = serde_json::from_slice(&fetch(&format!("{GALLERY}indice.json"))?).map_err(|_| "La galería respondió algo que no entiendo.".to_string())?;
    let mine = dir(&app);
    Ok(listed
        .into_iter()
        .filter(|shared| plain_id(&shared.id) && shared.archivos.iter().all(|file| plain_file(file)))
        .map(|mut shared| {
            let installed = mine.as_ref().and_then(|dir| installed_version(dir, &shared.id));
            shared.have = installed.is_some();
            shared.newer = installed.is_some_and(|version| !shared.version.is_empty() && version != shared.version);
            shared.permisos = if shared.tipo == "tema" { Some(Vec::new()) } else { known(&shared.permisos) };
            let (nombre, descripcion) = translated(&shared.nombre, &shared.descripcion, &shared.traduccion);
            (shared.nombre, shared.descripcion) = (nombre, descripcion);
            if !plain_file(&shared.vista) {
                shared.vista.clear();
            } else {
                shared.vista = format!("{GALLERY}{}/{}", shared.id, shared.vista);
            }
            shared
        })
        .collect())
}

#[tauri::command]
pub async fn extension_install(app: AppHandle, id: String) -> Result<String, String> {
    if !plain_id(&id) {
        return Err("Esa extensión no es de la galería.".into());
    }
    let manifest_bytes = fetch(&format!("{GALLERY}{id}/manifest.json"))?;
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes).map_err(|_| "Esa extensión trae un manifest roto.".to_string())?;
    if manifest.tipo != "tema" {
        let listed: Vec<SharedExtension> = serde_json::from_slice(&fetch(&format!("{GALLERY}indice.json"))?).map_err(|_| "La galería respondió algo que no entiendo.".to_string())?;
        let shown = listed.iter().find(|shared| shared.id == id).and_then(|shared| known(&shared.permisos)).unwrap_or_default();
        let asked = known(&manifest.permisos).unwrap_or_else(|| PERMISSIONS.iter().map(|p| p.to_string()).collect());
        if asked.iter().any(|permission| !shown.contains(permission)) {
            return Err("Esa extensión pide más permisos de los que muestra la galería. No se instala.".into());
        }
    }
    let mut files: Vec<String> = [&manifest.estilo, &manifest.codigo, &manifest.vista].into_iter().flatten().cloned().collect();
    files.dedup();
    if files.iter().any(|file| !plain_file(file)) {
        return Err("Esa extensión pide archivos con nombres raros. No se instala.".into());
    }
    let folder = dir(&app).ok_or("No se encontró la carpeta de extensiones.")?.join(&id);
    let staging = folder.with_extension("nueva");
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    std::fs::write(staging.join("manifest.json"), &manifest_bytes).map_err(|e| e.to_string())?;
    for file in &files {
        let bytes = fetch(&format!("{GALLERY}{id}/{file}"))?;
        if bytes.len() > 3_000_000 {
            let _ = std::fs::remove_dir_all(&staging);
            return Err("Esa extensión pesa demasiado.".into());
        }
        std::fs::write(staging.join(file), bytes).map_err(|e| e.to_string())?;
    }
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::rename(&staging, &folder).map_err(|e| e.to_string())?;
    extension_set(app, id, true)?;
    Ok(if manifest.nombre.trim().is_empty() { "la extensión".into() } else { manifest.nombre })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_phrases_match_the_start_of_what_was_said() {
        let commands = vec![Phrases { id: "foco:empezar".into(), frases: vec!["empieza el foco".into(), "modo pomodoro".into()] }];
        assert_eq!(matching(&commands, "Empieza el foco ya mismo").as_deref(), Some("foco:empezar"));
        assert_eq!(matching(&commands, "empiesa el foco").as_deref(), Some("foco:empezar"));
        assert_eq!(matching(&commands, "pon música").as_deref(), None);
        assert_eq!(matching(&commands, "el").as_deref(), None);
    }

    #[test]
    fn only_known_permissions_count() {
        let asked = Some(vec!["red".into(), "agente".into(), "lanzar cohetes".into(), "red".into()]);
        assert_eq!(known(&asked), Some(vec!["red".to_string(), "agente".to_string()]));
        assert_eq!(known(&None), None);
        assert_eq!(known(&Some(Vec::new())), Some(Vec::new()));
    }

    #[test]
    fn a_manifest_without_translation_keeps_its_spanish() {
        let manifest: Manifest = serde_json::from_str(r#"{"nombre":"Foco","descripcion":"Pomodoro","permisos":["agente"],"traduccion":{"en":{"nombre":"Focus"}}}"#).unwrap();
        assert_eq!(manifest.permisos.as_deref(), Some(&["agente".to_string()][..]));
        assert_eq!(manifest.traduccion["en"].nombre, "Focus");
        assert_eq!(translated("Foco", "Pomodoro", &Default::default()), ("Foco".to_string(), "Pomodoro".to_string()));
    }

    #[test]
    fn only_plain_names_get_in() {
        assert!(plain_id("aurora") && plain_id("foco-pomodoro_2") && !plain_id("../x") && !plain_id("a b") && !plain_id(""));
        assert!(plain_file("estilo.css") && plain_file("index.js") && plain_file("vista.webp"));
        for bad in ["../x.js", "a/b.js", "x.exe", ".css", "x.js;rm", "C:\\x.js", "", "noext"] {
            assert!(!plain_file(bad), "{bad}");
        }
    }
}
