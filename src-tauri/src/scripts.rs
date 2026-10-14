// scripts del usuario por voz y la galeria. lo que se dice llega como dato, nunca como codigo

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::intent;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const PATIENCE: Duration = Duration::from_secs(20);
const KINDS: [&str; 5] = ["ps1", "py", "bat", "cmd", "sh"];
const RUN: [&str; 12] = ["ejecuta", "ejecutar", "corre", "correr", "lanza", "script", "run", "execute", "launch", "roda", "rodar", "executa"];

const README: &str = "Scripts de Cocuyo\r\n\
=================\r\n\r\n\
Cada archivo de esta carpeta es una orden nueva para Cocuyo.\r\n\r\n\
- Vale PowerShell (.ps1), Python (.py), por lotes (.bat, .cmd) y Bash (.sh).\r\n\
- El nombre del archivo es el nombre de la orden: \"copia de seguridad.ps1\" se\r\n  ejecuta diciendo \"ejecuta copia de seguridad\".\r\n\
- La primera línea de comentario cuenta para qué sirve. Si conectas un modelo de\r\n  IA en Ajustes, con decir algo que encaje con esa descripción ya lo encuentra.\r\n\
- Lo último que el script escriba en pantalla es lo que Cocuyo te responde.\r\n\
- Lo que digas después del nombre le llega como primer argumento, y también en\r\n  la variable COCUYO_DATO. La frase entera va en COCUYO_FRASE.\r\n\r\n\
Cocuyo solo ejecuta lo que tú pongas aquí. No pongas scripts que no entiendas.\r\n";

const EXAMPLE: &str = "# Dice qué hora es.\r\n\"Son las \" + (Get-Date -Format \"h:mm\") + \".\"\r\n";

#[derive(Clone, Serialize)]
pub struct Script {
    pub name: String,
    pub about: String,
    #[serde(skip)]
    pub path: PathBuf,
}

pub fn dir(app: &AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_config_dir().ok()?.join("scripts");
    if !dir.is_dir() {
        std::fs::create_dir_all(&dir).ok()?;
        let _ = std::fs::write(dir.join("LEEME.txt"), README);
        let _ = std::fs::write(dir.join("di la hora.ps1"), EXAMPLE);
    }
    Some(dir)
}

fn about(text: &str) -> String {
    for line in text.lines().take(12) {
        let line = line.trim();
        let lower = line.to_lowercase();
        if line.starts_with("#!") || lower.starts_with("@echo") || lower.starts_with("# -*-") {
            continue;
        }
        let comment = ["#", "::", "//", "\"\"\"", "'''"].iter().find_map(|mark| line.strip_prefix(mark)).or_else(|| {
            lower.starts_with("rem ").then(|| &line[4..])
        });
        match comment.map(|c| c.trim().trim_end_matches("\"\"\"").trim()) {
            Some(text) if !text.is_empty() => return text.chars().take(160).collect(),
            Some(_) => continue,
            None if line.is_empty() => continue,
            None => break,
        }
    }
    String::new()
}

fn scripts_in(dir: &Path) -> Vec<Script> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut found: Vec<Script> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let kind = path.extension()?.to_string_lossy().to_lowercase();
            if !KINDS.contains(&kind.as_str()) || !path.is_file() {
                return None;
            }
            let name = path.file_stem()?.to_string_lossy().replace(['_', '-'], " ").trim().to_string();
            let head = std::fs::read(&path).ok().map(|bytes| String::from_utf8_lossy(&bytes[..bytes.len().min(2048)]).into_owned());
            Some(Script { name, about: about(head.as_deref().unwrap_or("").trim_start_matches('\u{feff}')), path })
        })
        .collect();
    found.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    found
}

pub fn list(app: &AppHandle) -> Vec<Script> {
    dir(app).map(|dir| scripts_in(&dir)).unwrap_or_default()
}

fn pick<'a>(scripts: &'a [Script], text: &str) -> Option<(&'a Script, String)> {
    let said: Vec<&str> = text.split_whitespace().collect();
    let mut best: Option<(&Script, usize, f32)> = None;
    for script in scripts {
        let words = script.name.split_whitespace().count().max(1);
        for take in [words, words + 1, words.saturating_sub(1)] {
            if take == 0 || take > said.len() {
                continue;
            }
            let score = intent::similarity(&said[..take].join(" "), &script.name);
            if score >= 0.8 && best.is_none_or(|(_, _, top)| score > top) {
                best = Some((script, take, score));
            }
        }
    }
    best.map(|(script, take, _)| (script, said[take..].join(" ")))
}

pub fn by_name(app: &AppHandle, spoken: &str) -> Option<Result<String, String>> {
    let clean = intent::normalize(spoken);
    let words: Vec<&str> = clean.split_whitespace().collect();
    let asked = words.first().is_some_and(|first| RUN.iter().any(|verb| intent::close(first, verb)));
    let rest = if asked { words[1..].join(" ") } else { clean.clone() };
    let rest = rest.trim_start_matches("el script ").trim_start_matches("script ").trim();
    if rest.is_empty() {
        return None;
    }
    let scripts = list(app);
    let (script, data) = pick(&scripts, rest)?;
    if !asked && !(data.is_empty() && intent::similarity(rest, &script.name) >= 0.92) {
        return None;
    }
    Some(run(script, &data, spoken))
}

pub fn named(app: &AppHandle, text: &str, spoken: &str) -> Result<String, String> {
    let scripts = list(app);
    match pick(&scripts, text.trim()) {
        Some((script, data)) => run(script, &data, spoken),
        None => Err(format!("No tengo ningún script llamado «{}».", text.trim())),
    }
}

fn command_for(script: &Script, data: &str) -> Command {
    let kind = script.path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let mut command = match kind.as_str() {
        "ps1" => {
            let literal = |text: &str| format!("'{}'", text.replace('\'', "''"));
            let mut line = format!(
                "[Console]::OutputEncoding = [Text.Encoding]::UTF8; & ([scriptblock]::Create([IO.File]::ReadAllText({}))) ",
                literal(&script.path.to_string_lossy())
            );
            if !data.is_empty() {
                line += &literal(data);
            }
            let mut command = Command::new("powershell.exe");
            command.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &line]);
            return command;
        }
        "py" => {
            let mut command = Command::new("python.exe");
            command.arg(&script.path).env("PYTHONUTF8", "1");
            command
        }
        "sh" => {
            let mut command = Command::new("bash.exe");
            command.arg(&script.path);
            command
        }
        _ => Command::new(&script.path),
    };
    if !data.is_empty() {
        command.arg(data);
    }
    command
}

pub fn run(script: &Script, data: &str, spoken: &str) -> Result<String, String> {
    let name = script.name.clone();
    let mut command = command_for(script, data);
    command
        .env("COCUYO_FRASE", spoken)
        .env("COCUYO_DATO", data)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW);
    if let Some(dir) = script.path.parent() {
        command.current_dir(dir);
    }
    let child = command.spawn().map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => format!("Para ejecutar «{name}» falta instalar el programa que lo interpreta."),
        _ => format!("No pude ejecutar «{name}»: {e}"),
    })?;

    let (done, wait) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = done.send(child.wait_with_output());
    });
    let Ok(output) = wait.recv_timeout(PATIENCE) else {
        return Ok(format!("«{name}» sigue trabajando."));
    };
    let output = output.map_err(|e| format!("No pude ejecutar «{name}»: {e}"))?;
    let last_line = |bytes: &[u8]| {
        let text = String::from_utf8_lossy(bytes);
        text.lines().map(str::trim).filter(|line| !line.is_empty()).last().map(|line| line.chars().take(220).collect::<String>())
    };
    if output.status.success() {
        Ok(last_line(&output.stdout).unwrap_or_else(|| format!("Hecho: {name}.")))
    } else {
        Err(last_line(&output.stderr).or_else(|| last_line(&output.stdout)).map_or_else(|| format!("«{name}» terminó con un error."), |why| format!("«{name}» falló: {why}")))
    }
}

const GALLERY: &str = "https://raw.githubusercontent.com/Tkyoxx/Cocuyo/main/galeria/";

#[derive(Clone, Serialize, serde::Deserialize)]
pub struct Shared {
    name: String,
    about: String,
    file: String,
    #[serde(default)]
    author: String,
    #[serde(default, skip_deserializing)]
    have: bool,
}

const PUBLISH: &str = "https://github.com/Tkyoxx/Cocuyo/issues/new?template=script.yml";
const LONGEST_LINK: usize = 1900;

fn a_login(name: &str) -> bool {
    !name.is_empty() && name.len() <= 39 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn escaped(text: &str) -> String {
    text.bytes().map(|b| if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.') { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

fn publish_link(file: &str, about: &str, code: &str) -> (String, bool) {
    let name = file.rsplit_once('.').map_or(file, |(name, _)| name);
    let head = format!("{PUBLISH}&title={}&archivo={}&que-hace={}", escaped(&format!("Script: {name}")), escaped(file), escaped(about));
    let whole = format!("{head}&codigo={}", escaped(code));
    if whole.len() <= LONGEST_LINK {
        (whole, true)
    } else {
        (head, false)
    }
}

fn fetch(url: &str) -> Result<Vec<u8>, String> {
    let output = Command::new("curl.exe")
        .args(["--silent", "--location", "--fail", "--max-time", "15", url])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err("No se pudo llegar a la galería. Revisa tu conexión.".into())
    }
}

fn safe_file(file: &str) -> bool {
    let kind = file.rsplit('.').next().unwrap_or("").to_lowercase();
    !file.is_empty()
        && file.len() <= 80
        && KINDS.contains(&kind.as_str())
        && !file.starts_with('.')
        && file.chars().all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.'))
        && !file.contains("..")
}

#[tauri::command]
pub async fn gallery_list(app: AppHandle) -> Result<Vec<Shared>, String> {
    let listed: Vec<Shared> = serde_json::from_slice(&fetch(&format!("{GALLERY}indice.json"))?).map_err(|_| "La galería respondió algo que no entiendo.".to_string())?;
    let mine = dir(&app);
    Ok(listed
        .into_iter()
        .filter(|shared| safe_file(&shared.file))
        .map(|mut shared| {
            if !a_login(&shared.author) {
                shared.author.clear();
            }
            shared.have = mine.as_ref().is_some_and(|dir| dir.join(&shared.file).is_file());
            shared
        })
        .collect())
}

#[tauri::command]
pub async fn gallery_publish(app: AppHandle, name: String) -> Result<bool, String> {
    let script = list(&app).into_iter().find(|script| script.name == name).ok_or("Ese script ya no está en tu carpeta.")?;
    let file = script.path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
    if !safe_file(&file) {
        return Err("Para subirlo, el nombre del archivo solo puede llevar letras, números, espacios y guiones.".into());
    }
    let code = std::fs::read_to_string(&script.path).map_err(|_| "No pude leer ese script.".to_string())?;
    let code = code.trim_start_matches('\u{feff}');
    if code.trim().is_empty() || code.len() > 20_000 {
        return Err("Ese script está vacío o es demasiado largo para la galería.".into());
    }
    let (link, whole) = publish_link(&file, &script.about, code);
    if !whole {
        crate::inject::set_clipboard_text(code);
    }
    crate::apps::open(&link)?;
    Ok(whole)
}

#[tauri::command]
pub async fn gallery_install(app: AppHandle, file: String) -> Result<String, String> {
    if !safe_file(&file) {
        return Err("Ese archivo no es de la galería.".into());
    }
    let target = dir(&app).ok_or("No se encontró la carpeta de configuración.")?.join(&file);
    if target.exists() {
        return Err("Ya tienes un script con ese nombre.".into());
    }
    let content = fetch(&format!("{GALLERY}{}", escaped(&file)))?;
    if content.is_empty() || content.len() > 200_000 {
        return Err("Ese script no se pudo descargar.".into());
    }
    std::fs::write(&target, content).map_err(|e| e.to_string())?;
    Ok(file.rsplit_once('.').map_or(file.clone(), |(name, _)| name.replace(['_', '-'], " ")))
}

#[tauri::command]
pub async fn scripts_list(app: AppHandle) -> Vec<Script> {
    list(&app)
}

#[tauri::command]
pub fn scripts_open(app: AppHandle) -> Result<(), String> {
    let dir = dir(&app).ok_or("No se encontró la carpeta de configuración.")?;
    crate::apps::open(&dir.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn script(name: &str) -> Script {
        Script { name: name.into(), about: String::new(), path: PathBuf::new() }
    }

    #[test]
    fn the_gallery_only_takes_plain_script_files() {
        assert!(safe_file("di la hora.ps1") && safe_file("modo_cine-2.py") && safe_file("café.bat"));
        for bad in ["..\\x.ps1", "../x.ps1", "a/b.ps1", "x.exe", "x", ".ps1", "x.ps1;calc", "C:\\x.ps1", ""] {
            assert!(!safe_file(bad), "{bad} no debería valer");
        }
    }

    #[test]
    fn publishing_opens_the_form_already_filled_in() {
        let (link, whole) = publish_link("di la hora.ps1", "Dice qué hora es.", "\"Son las \" + (Get-Date -Format \"h:mm\")\r\n");
        assert!(whole && link.starts_with("https://github.com/Tkyoxx/Cocuyo/issues/new?template=script.yml&title=Script%3A%20di%20la%20hora&archivo=di%20la%20hora.ps1&que-hace=Dice%20qu%C3%A9%20hora%20es."));
        assert!(link.contains("&codigo=%22Son%20las%20%22%20%2B%20%28Get-Date") && !link.contains(' ') && !link.contains('"'));
        let (link, whole) = publish_link("largo.ps1", "x", &"a = 1\n".repeat(600));
        assert!(!whole && !link.contains("codigo=") && link.len() < LONGEST_LINK);
        assert!(a_login("Tkyoxx") && a_login("un-nombre-9") && !a_login("") && !a_login("dos palabras") && !a_login("a/b") && !a_login(&"x".repeat(40)));
    }

    #[test]
    fn the_first_comment_says_what_it_does() {
        assert_eq!(about("# Dice qué hora es.\n\"Son las\""), "Dice qué hora es.");
        assert_eq!(about("#!/usr/bin/env python\n# -*- coding: utf-8 -*-\n\n# Copia mis documentos.\nimport os"), "Copia mis documentos.");
        assert_eq!(about("@echo off\r\nREM Apaga las luces\r\ncurl x"), "Apaga las luces");
        assert_eq!(about(":: Sube el brillo\r\n"), "Sube el brillo");
        assert_eq!(about("\"\"\"Resume el portapapeles.\"\"\"\nimport sys"), "Resume el portapapeles.");
        assert_eq!(about("import os\n# tarde"), "");
    }

    #[test]
    fn a_script_is_found_by_its_name_and_keeps_its_data() {
        let scripts = [script("copia de seguridad"), script("di la hora"), script("luces")];
        let (found, data) = pick(&scripts, "copia de seguridad fotos del viaje").unwrap();
        assert_eq!((found.name.as_str(), data.as_str()), ("copia de seguridad", "fotos del viaje"));
        let (found, data) = pick(&scripts, "di la ora").unwrap();
        assert_eq!((found.name.as_str(), data.as_str()), ("di la hora", ""));
        let (found, data) = pick(&scripts, "luces rojo").unwrap();
        assert_eq!((found.name.as_str(), data.as_str()), ("luces", "rojo"));
        assert!(pick(&scripts, "pon rock en español").is_none());
    }

    #[test]
    fn a_powershell_script_runs_and_answers() {
        let dir = std::env::temp_dir().join(format!("cocuyo-scripts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("eco de prueba.ps1");
        std::fs::write(&path, "# Repite lo que le dicen.\r\nparam($dato)\r\n\"ruido\"\r\n\"Dijiste: $dato / $env:COCUYO_DATO / año\"\r\n").unwrap();
        let scripts = scripts_in(&dir);
        assert_eq!((scripts[0].name.as_str(), scripts[0].about.as_str()), ("eco de prueba", "Repite lo que le dicen."));

        let tricky = "it's $(Get-Date); rm x";
        assert_eq!(run(&scripts[0], tricky, "frase").unwrap(), format!("Dijiste: {tricky} / {tricky} / año"));

        std::fs::write(dir.join("roto.ps1"), "Write-Error 'no hay disco'; exit 3").unwrap();
        let broken = scripts_in(&dir).into_iter().find(|s| s.name == "roto").unwrap();
        assert!(run(&broken, "", "").unwrap_err().starts_with("«roto» falló"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
