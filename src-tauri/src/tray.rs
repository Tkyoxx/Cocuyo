// el icono junto al reloj

use std::sync::OnceLock;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter};

use crate::i18n::tr;
use crate::overlay;

static ITEMS: OnceLock<Vec<(MenuItem<tauri::Wry>, &'static str)>> = OnceLock::new();
static MUTE: OnceLock<MenuItem<tauri::Wry>> = OnceLock::new();

fn mute_label() -> &'static str {
    if crate::cue::muted() {
        "Quitarle el silencio a Cocuyo"
    } else {
        "Silenciar a Cocuyo"
    }
}

pub fn relabel() {
    for (item, text) in ITEMS.get().into_iter().flatten() {
        let _ = item.set_text(tr(text));
    }
    if let Some(item) = MUTE.get() {
        let _ = item.set_text(tr(mute_label()));
    }
}

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let guide = MenuItem::with_id(app, "guide", "Ver la guía otra vez", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Ajustes…", true, None::<&str>)?;
    let unstick = MenuItem::with_id(app, "unstick", "Soltar teclas atascadas", true, None::<&str>)?;
    let plain = MenuItem::with_id(app, "plain", "Desactivar extensiones", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Salir de Cocuyo", true, None::<&str>)?;
    let mute = MenuItem::with_id(app, "mute", tr(mute_label()), true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&settings, &mute, &guide, &unstick, &plain, &quit])?;
    let _ = MUTE.set(mute);
    let _ = ITEMS.set(vec![
        (guide, "Ver la guía otra vez"),
        (settings, "Ajustes…"),
        (unstick, "Soltar teclas atascadas"),
        (plain, "Desactivar extensiones"),
        (quit, "Salir de Cocuyo"),
    ]);

    let mut tray = TrayIconBuilder::with_id("cocuyo")
        .tooltip("Cocuyo")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "guide" => {
                let _ = app.emit_to(overlay::LABEL, "guide", ());
            }
            "settings" => {
                let _ = app.emit_to(overlay::LABEL, "settings", ());
            }
            "unstick" => crate::hotkeys::release_modifiers(),
            "mute" => {
                let now = !crate::cue::muted();
                crate::cue::set_muted(now);
                relabel();
                let said = if now { "Cocuyo en silencio: no suena ni habla hasta que se lo quites en este menú." } else { "Cocuyo vuelve a sonar y a hablar." };
                let _ = app.emit_to(overlay::LABEL, "note", tr(said));
            }
            "plain" => crate::extensions::safe_mode(app),
            "quit" => app.exit(0),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}
