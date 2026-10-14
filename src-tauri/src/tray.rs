// el icono junto al reloj

use std::sync::OnceLock;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter};

use crate::i18n::tr;
use crate::overlay;

static ITEMS: OnceLock<Vec<(MenuItem<tauri::Wry>, &'static str)>> = OnceLock::new();

pub fn relabel() {
    for (item, text) in ITEMS.get().into_iter().flatten() {
        let _ = item.set_text(tr(text));
    }
}

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let guide = MenuItem::with_id(app, "guide", "Ver la guía otra vez", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Ajustes…", true, None::<&str>)?;
    let unstick = MenuItem::with_id(app, "unstick", "Soltar teclas atascadas", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Salir de Cocuyo", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&settings, &guide, &unstick, &quit])?;
    let _ = ITEMS.set(vec![
        (guide, "Ver la guía otra vez"),
        (settings, "Ajustes…"),
        (unstick, "Soltar teclas atascadas"),
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
            "quit" => app.exit(0),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}
