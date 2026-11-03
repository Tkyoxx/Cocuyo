// el resumen del dia: la primera vez que prendes el pc, una tarjeta con el clima, lo que tienes hoy y quien te escribio. una vez al dia y nada mas

use std::time::Duration;

use serde_json::json;
use tauri::{AppHandle, Manager};
use windows::Win32::System::SystemInformation::GetLocalTime;

const MONTHS: [&str; 12] = ["enero", "febrero", "marzo", "abril", "mayo", "junio", "julio", "agosto", "septiembre", "octubre", "noviembre", "diciembre"];
const DAYS: [&str; 7] = ["domingo", "lunes", "martes", "miércoles", "jueves", "viernes", "sábado"];

pub fn greeting(hour: u16) -> &'static str {
    match hour {
        5..=11 => "Buenos días",
        12..=18 => "Buenas tardes",
        _ => "Buenas noches",
    }
}

pub fn start(app: AppHandle) {
    if !crate::settings::load(&app).morning_summary {
        return;
    }
    let Some(mark) = app.path().app_config_dir().ok().map(|dir| dir.join("resumen-dia")) else { return };
    let now = unsafe { GetLocalTime() };
    let today = format!("{}-{:02}-{:02}", now.wYear, now.wMonth, now.wDay);
    if std::fs::read_to_string(&mark).is_ok_and(|seen| seen.trim() == today) {
        return;
    }
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(35));
        let mut waited = 0;
        while crate::placement::fullscreen() {
            if waited > 120 {
                return;
            }
            std::thread::sleep(Duration::from_secs(60));
            waited += 1;
        }
        if !crate::settings::load(&app).morning_summary {
            return;
        }
        let _ = std::fs::write(&mark, &today);
        crate::scripts::show_card(&app, "Cocuyo", &card(&app));
    });
}

fn card(app: &AppHandle) -> serde_json::Value {
    let now = unsafe { GetLocalTime() };
    let mut rows = Vec::new();
    if let Ok(weather) = crate::weather::today(app) {
        rows.push(json!({ "texto": weather.place(), "valor": weather.short() }));
    }
    for (at, what) in crate::agenda::today().into_iter().take(4) {
        rows.push(json!({ "texto": what, "valor": at }));
    }
    let people = crate::whatsapp::unread_people();
    if !people.is_empty() {
        let shown: Vec<String> = people.iter().take(4).map(|(name, n)| if *n > 1 { format!("{name} ({n})") } else { name.clone() }).collect();
        rows.push(json!({ "texto": crate::i18n::tr("Te escribieron"), "valor": shown.join(", ") }));
    }
    let day = format!("{} {} de {}", DAYS[usize::from(now.wDayOfWeek % 7)], now.wDay, MONTHS[usize::from(now.wMonth.clamp(1, 12) - 1)]);
    let text = if rows.is_empty() { format!("{day}. Nada pendiente por hoy.") } else { day };
    json!({
        "titulo": crate::i18n::tr(greeting(now.wHour)),
        "texto": text,
        "icono": if (6..19).contains(&now.wHour) { "☀️" } else { "🌙" },
        "filas": rows,
        "segundos": 45
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greets_by_the_hour() {
        assert_eq!(greeting(7), "Buenos días");
        assert_eq!(greeting(15), "Buenas tardes");
        assert_eq!(greeting(23), "Buenas noches");
        assert_eq!(greeting(2), "Buenas noches");
    }
}
