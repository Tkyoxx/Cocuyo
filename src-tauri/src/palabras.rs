// tus palabras: las que siempre van bien escritas, lo que se pega al decir algo y el estilo de cada app

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Snippet {
    pub said: String,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppStyle {
    pub app: String,
    pub name: String,
    pub style: String,
}

const OPEN: char = '⟦';
const CLOSE: char = '⟧';
const RECENT: usize = 10;

fn fold(c: char) -> char {
    match c.to_lowercase().next().unwrap_or(c) {
        'á' | 'à' | 'â' | 'ä' | 'ã' => 'a',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' => 'i',
        'ó' | 'ò' | 'ô' | 'ö' | 'õ' => 'o',
        'ú' | 'ù' | 'û' | 'ü' => 'u',
        'ç' => 'c',
        other => other,
    }
}

fn runs(chars: &[char]) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == OPEN {
            i = chars[i..].iter().position(|&c| c == CLOSE).map_or(chars.len(), |p| i + p + 1);
            continue;
        }
        if chars[i].is_alphanumeric() {
            let start = i;
            while i < chars.len() && chars[i].is_alphanumeric() {
                i += 1;
            }
            found.push((start, i));
        } else {
            i += 1;
        }
    }
    found
}

fn folded(chars: &[char]) -> String {
    chars.iter().map(|&c| fold(c)).collect()
}

pub fn tokens(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    runs(&chars).into_iter().map(|(a, b)| folded(&chars[a..b])).collect()
}

fn joins(gap: &[char]) -> bool {
    !gap.is_empty() && gap.iter().all(|&c| c.is_whitespace() || matches!(c, '-' | '\'' | '’'))
}

pub fn replacing(phrase: &str, with: &str, text: &str) -> String {
    let wanted = tokens(phrase);
    if wanted.is_empty() {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let words = runs(&chars);
    let mut out = String::with_capacity(text.len() + with.len());
    let (mut copied, mut i) = (0, 0);
    while i + wanted.len() <= words.len() {
        let hit = (0..wanted.len()).all(|k| {
            let (a, b) = words[i + k];
            folded(&chars[a..b]) == wanted[k] && (k == 0 || joins(&chars[words[i + k - 1].1..a]))
        });
        if hit {
            let (start, end) = (words[i].0, words[i + wanted.len() - 1].1);
            out.extend(&chars[copied..start]);
            out.push_str(with);
            copied = end;
            i += wanted.len();
        } else {
            i += 1;
        }
    }
    out.extend(&chars[copied..]);
    out
}

pub fn is_plain_word(word: &str) -> bool {
    let word = word.trim();
    !word.is_empty() && word.chars().all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '\'' | '’'))
}

pub struct Prepared {
    pub text: String,
    pieces: Vec<String>,
    words: Vec<String>,
}

fn mark(n: usize) -> String {
    format!("{OPEN}{n}{CLOSE}")
}

fn spell(text: &str, words: &[String]) -> String {
    words.iter().fold(text.to_string(), |text, word| replacing(word, word, &text))
}

pub fn prepare(heard: &str, words: &[String], snippets: &[Snippet]) -> Prepared {
    let words: Vec<String> = words.iter().map(|w| w.trim().to_string()).filter(|w| is_plain_word(w)).collect();
    let mut order: Vec<&Snippet> = snippets.iter().filter(|s| !tokens(&s.said).is_empty() && !s.text.is_empty()).collect();
    order.sort_by_key(|s| std::cmp::Reverse(tokens(&s.said).len()));
    let mut pieces = Vec::new();
    let mut text = heard.to_string();
    for snippet in order {
        let next = replacing(&snippet.said, &mark(pieces.len() + 1), &text);
        if next != text {
            pieces.push(snippet.text.clone());
            text = next;
        }
    }
    let text = spell(&text, &words);
    Prepared { text, pieces, words }
}

impl Prepared {
    pub fn keeps(&self, rewritten: &str) -> bool {
        (1..=self.pieces.len()).all(|n| rewritten.contains(&mark(n)))
    }

    pub fn finish(&self, text: &str) -> String {
        let text = spell(text, &self.words);
        self.pieces.iter().enumerate().fold(text, |text, (i, piece)| text.replace(&mark(i + 1), piece))
    }
}

pub fn style_for(settings: &crate::settings::Settings, app: Option<&str>) -> String {
    let chosen = app.and_then(|app| settings.dictation_styles.iter().find(|s| s.app == app && !s.style.is_empty()));
    match chosen {
        Some(s) => s.style.clone(),
        None if !settings.ai_dictation => "tal".into(),
        None if settings.dictation_style == "pulido" => "pulido".into(),
        None => "natural".into(),
    }
}

pub fn front_app() -> Option<(String, String)> {
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
    let path = unsafe {
        let window = GetForegroundWindow();
        if window.is_invalid() {
            return None;
        }
        let mut process = 0;
        GetWindowThreadProcessId(window, Some(&mut process));
        crate::apps::image_of(process)?
    };
    let file = std::path::Path::new(&path);
    let exe = file.file_name()?.to_string_lossy().to_lowercase();
    if exe == "cocuyo.exe" || exe == "explorer.exe" {
        return None;
    }
    let stem = file.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| exe.clone());
    let name = crate::system::description(&path).filter(|d| d.chars().count() <= 32).unwrap_or(stem);
    Some((exe, name))
}

fn recent_file(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_config_dir().ok()?.join("dictado.json"))
}

fn recent(app: &AppHandle) -> Vec<AppStyle> {
    recent_file(app).and_then(|path| std::fs::read(path).ok()).and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
}

pub fn seen(app: &AppHandle, exe: &str, name: &str) {
    let mut apps = recent(app);
    if apps.first().is_some_and(|first| first.app == exe) {
        return;
    }
    apps.retain(|a| a.app != exe);
    apps.insert(0, AppStyle { app: exe.into(), name: name.into(), style: String::new() });
    apps.truncate(RECENT);
    if let (Some(path), Ok(json)) = (recent_file(app), serde_json::to_vec(&apps)) {
        let _ = std::fs::write(path, json);
    }
}

#[tauri::command]
pub fn dictation_apps(app: AppHandle) -> Vec<AppStyle> {
    let styles = crate::settings::load(&app).dictation_styles;
    let mut apps: Vec<AppStyle> = recent(&app)
        .into_iter()
        .map(|mut a| {
            a.style = styles.iter().find(|s| s.app == a.app).map(|s| s.style.clone()).unwrap_or_default();
            a
        })
        .collect();
    for styled in styles.into_iter().filter(|s| !s.style.is_empty()) {
        if !apps.iter().any(|a| a.app == styled.app) {
            apps.push(styled);
        }
    }
    apps
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snippet(said: &str, text: &str) -> Snippet {
        Snippet { said: said.into(), text: text.into() }
    }

    #[test]
    fn whole_words_only_and_no_matter_the_accents() {
        assert_eq!(replacing("cloud code", "Claude Code", "abre cloud code ya"), "abre Claude Code ya");
        assert_eq!(replacing("cloud code", "Claude Code", "Abre Cloud-Code."), "Abre Claude Code.");
        assert_eq!(replacing("art", "ARTE", "voy a partir"), "voy a partir");
        assert_eq!(replacing("cancion", "canción", "la Canción buena"), "la canción buena");
        assert_eq!(replacing("mi firma", "X", "mi. Firma"), "mi. Firma", "una frase no se salta un punto");
        assert_eq!(replacing("", "X", "hola"), "hola");
    }

    #[test]
    fn the_dictionary_fixes_the_spelling() {
        let ready = prepare("abre cocuyo y busca a tkyoxx", &["Cocuyo".into(), "Tkyoxx".into(), "C++".into()], &[]);
        assert_eq!(ready.finish(&ready.text), "abre Cocuyo y busca a Tkyoxx");
    }

    #[test]
    fn snippets_are_kept_away_from_the_ai() {
        let snippets = [snippet("mi firma", "Saludos,\nYerik"), snippet("mi correo", "yo@ejemplo.com")];
        let ready = prepare("Te mando mi correo y mi firma.", &[], &snippets);
        assert_eq!(ready.text, "Te mando ⟦2⟧ y ⟦1⟧.");
        assert!(ready.keeps("Te envío ⟦2⟧ y ⟦1⟧."));
        assert!(!ready.keeps("Te envío mi correo y ⟦1⟧."));
        assert_eq!(ready.finish("Te envío ⟦2⟧ y ⟦1⟧."), "Te envío yo@ejemplo.com y Saludos,\nYerik.");
    }

    #[test]
    fn the_longest_snippet_wins() {
        let snippets = [snippet("correo", "A"), snippet("correo del trabajo", "B")];
        let ready = prepare("mi correo del trabajo", &[], &snippets);
        assert_eq!(ready.finish(&ready.text), "mi B");
    }

    #[test]
    fn the_dictionary_never_touches_a_snippet() {
        let ready = prepare("pon mi firma", &["Firma".into()], &[snippet("mi firma", "la firma de siempre")]);
        assert_eq!(ready.finish(&ready.text), "pon la firma de siempre");
    }

    #[test]
    fn nothing_to_do_leaves_it_alone() {
        let ready = prepare("hola, ¿qué tal?", &[], &[snippet("", "x"), snippet("algo", "")]);
        assert_eq!(ready.text, "hola, ¿qué tal?");
        assert!(ready.keeps("lo que sea"));
    }

    #[test]
    fn each_app_keeps_its_style() {
        let mut s = crate::settings::Settings::default();
        assert_eq!(style_for(&s, None), "natural");
        s.dictation_style = "pulido".into();
        assert_eq!(style_for(&s, Some("winword.exe")), "pulido");
        s.dictation_styles = vec![AppStyle { app: "whatsapp.exe".into(), name: "WhatsApp".into(), style: "tal".into() }];
        assert_eq!(style_for(&s, Some("whatsapp.exe")), "tal");
        s.ai_dictation = false;
        assert_eq!(style_for(&s, Some("winword.exe")), "tal");
    }
}
