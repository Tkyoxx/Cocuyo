// los ajustes. todo lo que el usuario puede tocar pasa por aqui

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::hotkeys::{self, Hotkey, ALT, CTRL, DEFAULT_CAPTURES, SHIFT, WIN};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dock {
    Auto,
    Top,
    Right,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub capture_dir: Option<PathBuf>,
    pub ask_where: bool,
    pub copy_to_clipboard: bool,
    pub dock: Dock,
    pub hide_on_fullscreen: bool,
    pub cue_fullscreen: bool,
    pub speak_fullscreen: bool,
    pub language: String,
    pub game_overlay: bool,
    pub dictation_keys: u8,
    pub lasso_keys: u8,
    pub command_keys: u8,
    pub wallpaper_dir: Option<PathBuf>,
    pub live_wallpaper: Option<PathBuf>,
    pub live_screens: std::collections::BTreeMap<String, PathBuf>,
    pub autostart: bool,
    pub portable: bool,
    pub spotify_client_id: Option<String>,
    pub ai_on: bool,
    pub ai_format: String,
    pub ai_url: String,
    pub ai_model: String,
    pub ai_always: bool,
    pub ai_dictation: bool,
    pub ai_hear: bool,
    pub ai_persona: String,
    pub ai_daily: u32,
    pub voice: String,
    pub name: String,
    pub pad_on: bool,
    pub pad_combo: u8,
    pub pad_buttons: u16,
    pub bar_style: String,
    pub bar_opacity: u8,
    pub bar_center: bool,
    pub bar_solid_max: bool,
    pub dictation_into: String,
    pub extensions_on: bool,
    pub extensions: Vec<String>,
    pub check_updates: bool,
    pub city: String,
    pub capture_region: Hotkey,
    pub capture_window: Hotkey,
    pub capture_screen: Hotkey,
}

const AI_URL: &str = "https://generativelanguage.googleapis.com/v1beta/openai";
const AI_MODELS: &str = "gemini-3.5-flash-lite, gemini-3.1-flash-lite";

impl Default for Settings {
    fn default() -> Self {
        Self {
            capture_dir: None,
            ask_where: false,
            copy_to_clipboard: true,
            dock: Dock::Auto,
            hide_on_fullscreen: true,
            cue_fullscreen: true,
            speak_fullscreen: true,
            language: crate::cue::system_language().into(),
            game_overlay: false,
            dictation_keys: CTRL | WIN,
            lasso_keys: CTRL | SHIFT,
            command_keys: ALT | WIN,
            wallpaper_dir: None,
            live_wallpaper: None,
            live_screens: Default::default(),
            autostart: false,
            portable: false,
            spotify_client_id: None,
            city: String::new(),
            check_updates: true,
            name: String::new(),
            pad_on: false,
            pad_combo: 0,
            pad_buttons: 0,
            bar_style: "normal".into(),
            bar_opacity: 0,
            bar_center: false,
            bar_solid_max: true,
            dictation_into: String::new(),
            extensions_on: true,
            extensions: Vec::new(),
            ai_daily: 480,
            voice: String::new(),
            ai_on: false,
            ai_format: "openai".into(),
            ai_url: AI_URL.into(),
            ai_model: AI_MODELS.into(),
            ai_always: false,
            ai_dictation: false,
            ai_hear: false,
            ai_persona: String::new(),
            capture_region: DEFAULT_CAPTURES[0],
            capture_window: DEFAULT_CAPTURES[1],
            capture_screen: DEFAULT_CAPTURES[2],
        }
    }
}

impl Settings {
    fn captures(&self) -> [Hotkey; 3] {
        [self.capture_region, self.capture_window, self.capture_screen]
    }
}

fn check_keys(settings: &Settings) -> Result<(), String> {
    let chords = [settings.dictation_keys, settings.lasso_keys, settings.command_keys];
    if !chords.into_iter().all(hotkeys::valid_chord) {
        return Err("Un atajo de mantener necesita al menos dos teclas, y no puede ser solo Ctrl + Alt.".into());
    }
    if chords[0] == chords[1] || chords[0] == chords[2] || chords[1] == chords[2] {
        return Err("Dictar, rodear y dar órdenes necesitan atajos distintos.".into());
    }
    let [region, window, screen] = settings.captures();
    if ![region, window, screen].into_iter().all(hotkeys::valid_hotkey) {
        return Err("Una letra o un número necesita Ctrl, Shift, Alt o Win. Solas solo valen Impr Pant y las teclas F.".into());
    }
    if region == window || region == screen || window == screen {
        return Err("Cada captura necesita su propio atajo.".into());
    }
    Ok(())
}

fn file(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_config_dir().ok()?.join("ajustes.json"))
}

pub fn load(app: &AppHandle) -> Settings {
    let mut settings: Settings = file(app)
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    if !settings.ai_on && settings.ai_url == "http://127.0.0.1:11434/v1" && settings.ai_model == "llama3.2" {
        settings.ai_url = AI_URL.into();
        settings.ai_model = AI_MODELS.into();
    }
    if check_keys(&settings).is_err() {
        let safe = Settings::default();
        return Settings {
            dictation_keys: safe.dictation_keys,
            lasso_keys: safe.lasso_keys,
            command_keys: safe.command_keys,
            capture_region: safe.capture_region,
            capture_window: safe.capture_window,
            capture_screen: safe.capture_screen,
            ..settings
        };
    }
    settings
}

pub fn apply(app: &AppHandle) {
    let settings = load(app);
    hotkeys::configure(settings.dictation_keys, settings.lasso_keys, settings.command_keys, settings.captures());
    crate::placement::refresh(app);
    crate::cue::configure(settings.cue_fullscreen, settings.speak_fullscreen, &settings.language);
    crate::gamelayer::configure(settings.game_overlay);
    crate::tray::relabel();
    crate::cue::attach(app.clone(), &settings.voice);
    crate::pad::configure(app, settings.pad_on, settings.pad_combo, settings.pad_buttons);
    crate::taskbar::configure(crate::taskbar::Look {
        style: crate::taskbar::Style::from_name(&settings.bar_style),
        opacity: settings.bar_opacity.min(100),
        center: settings.bar_center,
        solid_when_max: settings.bar_solid_max,
    });
}

pub fn capture_dir(app: &AppHandle, settings: &Settings) -> Option<PathBuf> {
    match &settings.capture_dir {
        Some(dir) => Some(dir.clone()),
        None => Some(app.path().picture_dir().ok()?.join("Cocuyo")),
    }
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> Settings {
    load(&app)
}

#[tauri::command]
pub fn set_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    check_keys(&settings)?;
    if settings.autostart != load(&app).autostart {
        crate::system::set_autostart(settings.autostart)?;
    }
    let path = file(&app).ok_or("No se encontró la carpeta de configuración.")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_vec_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())?;
    apply(&app);
    Ok(())
}

#[tauri::command]
pub fn current_capture_dir(app: AppHandle) -> Option<String> {
    capture_dir(&app, &load(&app)).map(|p| p.display().to_string())
}

#[tauri::command]
pub async fn pick_capture_dir() -> Option<String> {
    crate::dialogs::pick_folder().map(|p| p.display().to_string())
}
