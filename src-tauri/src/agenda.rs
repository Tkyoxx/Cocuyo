// recordatorios y rutinas. un hilo durmiendo hasta que toque, nada de timers dando vueltas

use std::path::PathBuf;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use windows::Win32::System::SystemInformation::GetLocalTime;

use crate::intent::normalize;

const DAY: i64 = 86_400;
const EVERY_DAY: u8 = 0b111_1111;
const WEEKDAYS: u8 = 0b001_1111;
const WEEKEND: u8 = 0b110_0000;
const LATE: i64 = 12 * 3600;
const DAY_NAMES: [&str; 7] = ["lunes", "martes", "miércoles", "jueves", "viernes", "sábado", "domingo"];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "lowercase")]
pub enum When {
    Once { at: i64 },
    Daily { minute: u16, days: u8 },
    Startup,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    id: u64,
    when: When,
    act: bool,
    what: String,
    #[serde(default)]
    done: i64,
}

#[derive(Clone, Copy)]
pub struct Now {
    unix: i64,
    second: i64,
    weekday: u8,
    date: (i64, i64, i64),
}

impl Now {
    fn read() -> Self {
        let local = unsafe { GetLocalTime() };
        Self {
            unix: SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64),
            second: i64::from(local.wHour) * 3600 + i64::from(local.wMinute) * 60 + i64::from(local.wSecond),
            weekday: ((local.wDayOfWeek + 6) % 7) as u8,
            date: (i64::from(local.wYear), i64::from(local.wMonth), i64::from(local.wDay)),
        }
    }

    fn midnight(&self) -> i64 {
        self.unix - self.second
    }
}

static BOOK: Mutex<Vec<Entry>> = Mutex::new(Vec::new());
static CHANGED: Condvar = Condvar::new();

fn number(word: &str) -> Option<i64> {
    if let Ok(digits) = word.parse::<i64>() {
        return Some(digits);
    }
    const WORDS: [(&str, i64); 44] = [
        ("un", 1), ("uno", 1), ("una", 1), ("dos", 2), ("tres", 3), ("cuatro", 4), ("cinco", 5), ("seis", 6), ("siete", 7), ("ocho", 8),
        ("nueve", 9), ("diez", 10), ("once", 11), ("doce", 12), ("quince", 15), ("veinte", 20), ("treinta", 30), ("cuarenta", 40), ("cincuenta", 50),
        ("one", 1), ("two", 2), ("three", 3), ("four", 4), ("five", 5), ("six", 6), ("seven", 7), ("eight", 8), ("nine", 9), ("ten", 10),
        ("eleven", 11), ("twelve", 12), ("fifteen", 15), ("twenty", 20), ("thirty", 30), ("um", 1), ("uma", 1), ("dois", 2), ("duas", 2),
        ("quatro", 4), ("sete", 7), ("oito", 8), ("nove", 9), ("dez", 10), ("vinte", 20),
    ];
    WORDS.iter().find(|(name, _)| *name == word).map(|(_, value)| *value)
}

fn day_of(word: &str) -> Option<u8> {
    const NAMES: [[&str; 3]; 7] = [
        ["lunes", "monday", "segunda"],
        ["martes", "tuesday", "terca"],
        ["miercoles", "wednesday", "quarta"],
        ["jueves", "thursday", "quinta"],
        ["viernes", "friday", "sexta"],
        ["sabado", "saturday", "sabados"],
        ["domingo", "sunday", "domingos"],
    ];
    NAMES.iter().position(|names| names.contains(&word)).map(|index| index as u8)
}

fn day_number((year, month, day): (i64, i64, i64)) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let of_era = year - era * 400;
    let of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    era * 146_097 + of_era * 365 + of_era / 4 - of_era / 100 + of_year
}

fn month_of(word: &str) -> Option<i64> {
    const MONTHS: [[&str; 3]; 12] = [
        ["enero", "january", "janeiro"], ["febrero", "february", "fevereiro"], ["marzo", "march", "marco"], ["abril", "april", "abril"],
        ["mayo", "may", "maio"], ["junio", "june", "junho"], ["julio", "july", "julho"], ["agosto", "august", "agosto"],
        ["septiembre", "september", "setembro"], ["octubre", "october", "outubro"], ["noviembre", "november", "novembro"], ["diciembre", "december", "dezembro"],
    ];
    MONTHS.iter().position(|names| names.contains(&word) || word == "setiembre" && names[0] == "septiembre").map(|index| index as i64 + 1)
}

fn date_at(words: &[String], index: usize, now: &Now) -> Option<(i64, usize)> {
    let word = |at: usize| words.get(at).map(String::as_str).unwrap_or("");
    let lead = usize::from(matches!(word(index), "el" | "dia" | "on" | "no"));
    let day = word(index + lead).parse::<i64>().ok().filter(|day| (1..=31).contains(day))?;
    if word(index + lead + 1) != "de" && word(index + lead + 1) != "of" {
        return None;
    }
    let month = month_of(word(index + lead + 2))?;
    let today = day_number(now.date);
    let ahead = [now.date.0, now.date.0 + 1].into_iter().map(|year| day_number((year, month, day)) - today).find(|ahead| *ahead >= 0)?;
    Some((ahead, lead + 3))
}

struct Found {
    when: When,
    from: usize,
    to: usize,
    timer: bool,
}

fn relative(words: &[String], now: &Now) -> Option<Found> {
    for from in 0..words.len() {
        let timer = matches!(words[from].as_str(), "temporizador" | "timer" | "alarma" | "cronometro");
        if !timer && !matches!(words[from].as_str(), "en" | "dentro" | "in" | "em" | "daqui") {
            continue;
        }
        let mut at = from + 1;
        while words.get(at).is_some_and(|word| matches!(word.as_str(), "de" | "a" | "por" | "para" | "for" | "of")) {
            at += 1;
        }
        let word = |index: usize| words.get(index).map(String::as_str).unwrap_or("");
        let (seconds, to) = if matches!(word(at), "media" | "meia" | "half") && word(at + 1).starts_with("hor") {
            (1800, at + 2)
        } else if let Some(amount) = number(word(at)) {
            let unit = word(at + 1);
            let each = if unit.starts_with("seg") || unit.starts_with("sec") {
                1
            } else if unit.starts_with("min") {
                60
            } else if unit.starts_with("hor") || unit.starts_with("hour") {
                3600
            } else {
                continue;
            };
            if each == 3600 && matches!(word(at + 2), "y" | "e" | "and") && matches!(word(at + 3), "media" | "meia" | "half") {
                (amount * 3600 + 1800, at + 4)
            } else {
                (amount * each, at + 2)
            }
        } else {
            continue;
        };
        if seconds > 0 {
            return Some(Found { when: When::Once { at: now.unix + seconds }, from, to, timer });
        }
    }
    None
}

fn absolute(words: &[String], now: &Now) -> Option<Found> {
    let word = |index: usize| words.get(index).map(String::as_str).unwrap_or("");
    for start in 0..words.len() {
        let hour_at = match (word(start), word(start + 1)) {
            ("a", "las" | "la") => start + 2,
            ("at" | "as", _) => start + 1,
            _ => continue,
        };
        let Some(mut hour) = number(word(hour_at)).filter(|hour| (0..24).contains(hour)) else { continue };
        let mut minute = 0;
        let mut to = hour_at + 1;
        if let Some(minutes) = word(to).parse::<i64>().ok().filter(|minutes| (0..60).contains(minutes)) {
            (minute, to) = (minutes, to + 1);
        } else if matches!(word(to), "y" | "e" | "and") {
            let extra = match word(to + 1) {
                "media" | "meia" | "half" => Some(30),
                "cuarto" | "quarto" | "quarter" => Some(15),
                other => number(other).filter(|minutes| (0..60).contains(minutes)),
            };
            if let Some(extra) = extra {
                (minute, to) = (extra, to + 2);
            }
        } else if word(to) == "menos" && word(to + 1) == "cuarto" {
            (hour, minute, to) = ((hour + 23) % 24, 45, to + 2);
        }
        let lead = match (word(to), word(to + 1)) {
            ("de" | "da" | "in" | "por", "la" | "the") => 2,
            ("de" | "da" | "in" | "por", _) => 1,
            _ => 0,
        };
        let part = to + lead;
        let period = match word(part) {
            "tarde" | "noche" | "noite" | "pm" | "evening" | "afternoon" | "night" => Some(true),
            "mañana" | "manana" | "madrugada" | "manha" | "am" | "morning" => Some(false),
            "p" if word(part + 1) == "m" => Some(true),
            "a" if word(part + 1) == "m" => Some(false),
            _ => None,
        };
        if period.is_some() {
            to = part + if matches!(word(part), "p" | "a") { 2 } else { 1 };
        }
        match period {
            Some(true) if hour < 12 => hour += 12,
            Some(false) if hour == 12 => hour = 0,
            _ => {}
        }

        let mut from = start;
        let mut days = None;
        let mut offset = None;
        let mut dated = false;
        let claim = |range: std::ops::Range<usize>, from: &mut usize, to: &mut usize| {
            if range.end == *from {
                *from = range.start;
            } else if range.start == *to {
                *to = range.end;
            }
        };
        for index in 0..words.len() {
            let here = index..index + 1;
            let tomorrow = matches!(word(index), "mañana" | "manana" | "tomorrow" | "amanha") && (index < start || index >= to);
            if tomorrow && index > 0 && word(index - 1) == "pasado" {
                offset = Some(2);
                claim(index - 1..index + 1, &mut from, &mut to);
            } else if tomorrow {
                offset = Some(1);
                claim(here, &mut from, &mut to);
            } else if let Some((ahead, length)) = date_at(words, index, now).filter(|_| index < start || index >= to) {
                offset = Some(ahead);
                dated = true;
                claim(index..index + length, &mut from, &mut to);
            } else if matches!(word(index), "hoy" | "today" | "hoje") {
                offset = Some(0);
                claim(here, &mut from, &mut to);
            } else if matches!(word(index), "todos" | "cada" | "every" | "todo")
                && (word(index + 1).starts_with("dia") || word(index + 1) == "day" || (matches!(word(index + 1), "los" | "os") && word(index + 2).starts_with("dia")))
            {
                let end = if matches!(word(index + 1), "los" | "os") { index + 3 } else { index + 2 };
                days = Some(EVERY_DAY);
                claim(index..end, &mut from, &mut to);
            } else if matches!(word(index), "diario" | "diariamente" | "daily") {
                days = Some(EVERY_DAY);
                claim(here, &mut from, &mut to);
            } else if word(index) == "de" && word(index + 1) == "lunes" && word(index + 2) == "a" && word(index + 3) == "viernes" {
                days = Some(WEEKDAYS);
                claim(index..index + 4, &mut from, &mut to);
            } else if (word(index) == "entre" && word(index + 1) == "semana") || word(index) == "weekdays" {
                days = Some(WEEKDAYS);
                claim(index..index + if word(index) == "entre" { 2 } else { 1 }, &mut from, &mut to);
            } else if word(index) == "fines" && word(index + 1) == "de" && word(index + 2) == "semana" {
                days = Some(WEEKEND);
                claim(index..index + 3, &mut from, &mut to);
            } else if let Some(day) = day_of(word(index)).filter(|_| index < start || index >= to) {
                let plural = matches!(word(index.wrapping_sub(1)), "los" | "cada" | "every");
                let mut article = usize::from(index > 0 && matches!(word(index - 1), "el" | "los" | "cada" | "on" | "every"));
                if article == 1 && index > 1 && word(index - 2) == "todos" {
                    article = 2;
                }
                if plural {
                    days = Some(days.unwrap_or(0) | 1 << day);
                } else {
                    offset = Some((i64::from(day) - i64::from(now.weekday)).rem_euclid(7));
                }
                claim(index - article..index + 1, &mut from, &mut to);
            }
        }

        let minute_of_day = hour * 60 + minute;
        let when = match days {
            Some(days) => When::Daily { minute: minute_of_day as u16, days },
            None => {
                let mut at = now.midnight() + minute_of_day * 60 + offset.unwrap_or(0) * DAY;
                if period.is_none() && hour < 12 && offset.is_none() && at <= now.unix && at + 12 * 3600 > now.unix {
                    at += 12 * 3600;
                }
                if at <= now.unix && !dated {
                    at += if offset == Some(0) || offset.is_none() { DAY } else { 7 * DAY };
                }
                When::Once { at }
            }
        };
        return Some(Found { when, from, to, timer: false });
    }
    None
}

fn day_only(words: &[String], now: &Now) -> Option<Found> {
    let word = |index: usize| words.get(index).map(String::as_str).unwrap_or("");
    for from in 0..words.len() {
        let (ahead, length) = if let Some(found) = date_at(words, from, now) {
            found
        } else if word(from) == "pasado" && matches!(word(from + 1), "mañana" | "manana") {
            (2, 2)
        } else if matches!(word(from), "mañana" | "manana" | "tomorrow" | "amanha") {
            (1, 1)
        } else if word(from) == "el" && day_of(word(from + 1)).is_some() {
            let ahead = (i64::from(day_of(word(from + 1))?) - i64::from(now.weekday)).rem_euclid(7);
            (if ahead == 0 { 7 } else { ahead }, 2)
        } else {
            continue;
        };
        let at = now.midnight() + ahead * DAY + 9 * 3600;
        if at > now.unix {
            return Some(Found { when: When::Once { at }, from, to: from + length, timer: false });
        }
    }
    None
}

fn startup(words: &[String]) -> Option<Found> {
    let word = |index: usize| words.get(index).map(String::as_str).unwrap_or("");
    for from in 0..words.len() {
        let verb_at = match word(from) {
            "al" | "cuando" | "ao" | "quando" | "when" => from + 1,
            "on" if word(from + 1) == "startup" => return Some(Found { when: When::Startup, from, to: from + 2, timer: false }),
            _ => continue,
        };
        let verb = word(verb_at);
        if !["encend", "encien", "inici", "prend", "arran", "ligar", "ligo"].iter().any(|stem| verb.starts_with(stem)) {
            continue;
        }
        let mut to = verb_at + 1;
        while matches!(word(to), "la" | "el" | "mi" | "o" | "a" | "pc" | "computadora" | "computador" | "compu" | "equipo" | "ordenador" | "windows" | "sesion" | "maquina" | "laptop") {
            to += 1;
        }
        return Some(Found { when: When::Startup, from, to, timer: false });
    }
    None
}

fn parse(spoken: &str, now: &Now) -> Option<(When, bool, String)> {
    let said: Vec<&str> = spoken.split(|c: char| c.is_whitespace() || c == ':').filter(|word| !word.is_empty()).collect();
    let mut original = Vec::new();
    let mut words = Vec::new();
    for word in said {
        for piece in normalize(word).split_whitespace() {
            words.push(piece.to_string());
            original.push(if normalize(word) == piece { word.trim_matches(|c: char| ",.;¿?¡!".contains(c)).to_string() } else { piece.to_string() });
        }
    }
    let asks_reminder = |word: &String| {
        ["recuerdame", "recuerda", "recordarme", "recuerdamelo", "avisame", "avisa", "acuerdame", "remind", "lembra", "lembre"].iter().any(|verb| crate::intent::close(word, verb))
    };
    let found = startup(&words)
        .or_else(|| relative(&words, now))
        .or_else(|| absolute(&words, now))
        .or_else(|| day_only(&words, now).filter(|_| words.iter().take(3).any(asks_reminder)))?;

    let mut rest: Vec<(String, String)> = (0..words.len()).filter(|index| *index < found.from || *index >= found.to).map(|index| (words[index].clone(), original[index].clone())).collect();
    let drop_while = |rest: &mut Vec<(String, String)>, fillers: &[&str]| {
        while rest.first().is_some_and(|(word, _)| fillers.contains(&word.as_str())) {
            rest.remove(0);
        }
    };
    drop_while(&mut rest, &["oye", "por", "favor", "y", "pon", "ponme", "un", "una", "set", "a"][..if found.timer { 10 } else { 4 }]);
    let reminder = rest.first().is_some_and(|(word, _)| asks_reminder(word));
    if reminder {
        rest.remove(0);
        drop_while(&mut rest, &["me", "que", "de", "to", "about", "lo"]);
    }
    while rest.last().is_some_and(|(word, _)| matches!(word.as_str(), "y" | "que" | "de" | "para" | "por" | "favor")) {
        rest.pop();
    }
    let what = rest.iter().map(|(_, original)| original.as_str()).collect::<Vec<_>>().join(" ");
    match (reminder || found.timer, what.is_empty()) {
        (true, true) if found.timer => Some((found.when, false, "Se cumplió el tiempo.".into())),
        (true, true) => Some((found.when, false, "Ya es la hora.".into())),
        (true, false) => Some((found.when, false, what)),
        (false, false) if found.from == 0 || found.to == words.len() || matches!(found.when, When::Startup) => Some((found.when, true, what)),
        _ => None,
    }
}

fn clock(minute: i64) -> String {
    format!("{}:{:02}", minute / 60 % 24, minute % 60)
}

fn describe(when: &When, now: &Now) -> String {
    match when {
        When::Startup => "al encender el equipo".into(),
        When::Daily { minute, days } => {
            let hour = clock(i64::from(*minute));
            match *days {
                EVERY_DAY => format!("cada día a las {hour}"),
                WEEKDAYS => format!("de lunes a viernes a las {hour}"),
                WEEKEND => format!("los fines de semana a las {hour}"),
                days => {
                    let names: Vec<&str> = (0..7).filter(|day| days & (1 << day) != 0).map(|day| DAY_NAMES[day]).collect();
                    format!("los {} a las {hour}", names.join(", "))
                }
            }
        }
        When::Once { at } => {
            let left = at - now.unix;
            let days_ahead = (at - now.midnight()).div_euclid(DAY);
            let hour = clock((at - now.midnight()).rem_euclid(DAY) / 60);
            match (left, days_ahead) {
                (..=89, _) => format!("en {} s", left.max(1)),
                (..=3599, _) => format!("en {} min", (left + 30) / 60),
                (_, 0) => format!("a las {hour}"),
                (_, 1) => format!("mañana a las {hour}"),
                (_, days) => format!("el {} a las {hour}", DAY_NAMES[((i64::from(now.weekday) + days).rem_euclid(7)) as usize]),
            }
        }
    }
}

fn file(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_config_dir().ok()?.join("agenda.json"))
}

fn save(app: &AppHandle, book: &[Entry]) {
    if let (Some(path), Ok(json)) = (file(app), serde_json::to_vec_pretty(book)) {
        let _ = std::fs::write(path, json);
    }
    let _ = app.emit_to(crate::overlay::LABEL, "agenda", ());
}

fn add(app: &AppHandle, when: When, act: bool, what: String) -> String {
    let now = Now::read();
    let said = describe(&when, &now);
    let reply = if act { format!("Programado {said}: «{what}».") } else { format!("Te aviso {said}: {what}") };
    let mut book = BOOK.lock().unwrap();
    let id = book.iter().map(|entry| entry.id).max().unwrap_or(0) + 1;
    book.push(Entry { id, when, act, what, done: 0 });
    save(app, &book);
    CHANGED.notify_all();
    reply
}

pub fn soon(spoken: &str) -> bool {
    let words: Vec<String> = normalize(spoken).split_whitespace().map(str::to_string).collect();
    relative(&words, &Now::read()).is_some()
}

pub fn manage(app: &AppHandle, spoken: &str) -> Option<Result<String, String>> {
    let clean = normalize(spoken);
    let about_plans = ["programad", "recordatorio", "rutina", "agenda", "pendiente", "temporizador", "alarma", "reminder", "scheduled", "lembrete"].iter().any(|word| clean.contains(word));
    if !about_plans {
        return None;
    }
    let first = clean.split_whitespace().next().unwrap_or("");
    if matches!(first, "que" | "cuales" | "cuantos" | "dime" | "what" | "quais" | "tengo" | "hay") {
        let planned = agenda_list();
        let said = match planned.len() {
            0 => "No tienes nada programado.".to_string(),
            count => {
                let listed: Vec<String> = planned.iter().take(3).map(|plan| format!("{}, {}", plan.when, plan.what)).collect();
                format!("Tienes {count}: {}{}", listed.join("; "), if count > 3 { "; y más en la pestaña Hoy." } else { "." })
            }
        };
        crate::cue::answer(&crate::i18n::tr(&said));
        return Some(Ok(said));
    }
    if !["cancela", "borra", "quita", "elimina", "cancel", "delete", "remove", "apaga"].iter().any(|verb| crate::intent::close(first, verb)) {
        return None;
    }
    let mut book = BOOK.lock().unwrap();
    let before = book.len();
    let everything = clean.split_whitespace().any(|word| matches!(word, "todo" | "todos" | "todas" | "all" | "tudo"));
    let about: Vec<&str> = clean
        .split_whitespace()
        .skip(1)
        .filter(|word| !["el", "la", "los", "las", "de", "del", "mi", "mis", "recordatorio", "recordatorios", "rutina", "rutinas", "temporizador", "alarma", "programado", "programada", "lo", "the", "reminder", "todo", "todos", "todas"].contains(word))
        .collect();
    let mut gone = Vec::new();
    book.retain(|entry| {
        let hit = everything || (!about.is_empty() && crate::intent::covers(&about.join(" "), &entry.what) >= 0.6);
        if hit {
            gone.push(entry.what.clone());
        }
        !hit
    });
    if book.len() != before {
        save(app, &book);
        CHANGED.notify_all();
    }
    Some(match gone.len() {
        0 if about.is_empty() => Err("Dime cuál: «cancela el recordatorio de la pizza», o «borra todos los recordatorios».".to_string()),
        0 => Err(format!("No encontré nada programado con «{}».", about.join(" "))),
        1 => Ok(format!("Quitado: {}.", gone[0])),
        count => Ok(format!("Quitados {count}.")),
    })
}

pub struct ForPhone {
    pub at: i64,
    pub what: String,
    pub alarm: bool,
    pub said: String,
}

pub fn for_phone(spoken: &str) -> Option<ForPhone> {
    const ALARM: [&str; 8] = ["alarma", "despiertame", "despertador", "levantame", "alarm", "wake me", "desperta", "acorda"];
    let clean = normalize(spoken);
    let alarm = ALARM.iter().any(|word| clean.contains(word));
    let text = if alarm {
        const SKIP: [&str; 14] = ["oye", "pon", "ponme", "poner", "una", "un", "me", "alarma", "despiertame", "despertador", "levantame", "set", "an", "alarm"];
        let rest: Vec<&str> = spoken.split_whitespace().skip_while(|word| SKIP.contains(&normalize(word).as_str())).collect();
        format!("recuérdame {}", rest.join(" "))
    } else {
        spoken.to_string()
    };
    let now = Now::read();
    let (when, act, what) = parse(&text, &now)?;
    if act || matches!(when, When::Startup) {
        return None;
    }
    let said = describe(&when, &now);
    let at = next_due(&Entry { id: 0, when, act: false, what: String::new(), done: 0 }, &now)?;
    let what = what.strip_prefix("para ").or_else(|| what.strip_prefix("for ")).unwrap_or(&what).to_string();
    let what = if what.starts_with("Ya es la hora") || what.starts_with("Se cumplió") { String::new() } else { what };
    Some(ForPhone { at, what, alarm, said })
}

pub fn schedule(app: &AppHandle, spoken: &str) -> Option<Result<String, String>> {
    let (when, act, what) = parse(spoken, &Now::read())?;
    Some(Ok(add(app, when, act, what)))
}

#[derive(Serialize)]
pub struct Planned {
    id: u64,
    what: String,
    act: bool,
    when: String,
}

#[tauri::command]
pub fn agenda_list() -> Vec<Planned> {
    let now = Now::read();
    let mut book = BOOK.lock().unwrap().clone();
    book.sort_by_key(|entry| next_due(entry, &now).unwrap_or(i64::MAX));
    book.iter().map(|entry| Planned { id: entry.id, what: entry.what.clone(), act: entry.act, when: describe(&entry.when, &now) }).collect()
}

#[tauri::command]
pub fn agenda_remove(app: AppHandle, id: u64) {
    let mut book = BOOK.lock().unwrap();
    book.retain(|entry| entry.id != id);
    save(&app, &book);
    CHANGED.notify_all();
}

#[tauri::command]
pub fn agenda_add(app: AppHandle, text: String) -> Option<String> {
    schedule(&app, &text).and_then(Result::ok)
}

fn next_due(entry: &Entry, now: &Now) -> Option<i64> {
    match entry.when {
        When::Startup => None,
        When::Once { at } => Some(at),
        When::Daily { minute, days } => (0..8).find_map(|ahead| {
            let at = now.midnight() + ahead * DAY + i64::from(minute) * 60;
            let day = (i64::from(now.weekday) + ahead).rem_euclid(7);
            (days & (1 << day) != 0 && at > entry.done && at + 60 > now.unix).then_some(at)
        }),
    }
}

fn fire(app: &AppHandle, entry: &Entry, late: bool) {
    let (app, entry) = (app.clone(), entry.clone());
    std::thread::spawn(move || {
        let text = if entry.act {
            match crate::voice::run_order(&app, &entry.what) {
                Ok(done) if done.is_empty() => format!("Hecho: {}.", entry.what),
                Ok(done) => done,
                Err(why) => why,
            }
        } else if late {
            format!("Se pasó la hora: {}", entry.what)
        } else {
            entry.what.clone()
        };
        let text = crate::i18n::tr(&text);
        if cfg!(debug_assertions) {
            eprintln!("[agenda] {:?} «{}» -> {text}", entry.when, entry.what);
        }
        let _ = app.emit_to(crate::overlay::LABEL, "note", text.clone());
        if !entry.act {
            crate::cue::alert(&text);
        }
    });
}

pub fn start(app: AppHandle, boot: bool) {
    let saved: Vec<Entry> = file(&app).and_then(|path| std::fs::read(path).ok()).and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
    *BOOK.lock().unwrap() = saved;
    let _ = std::thread::Builder::new().name("agenda".into()).spawn(move || {
        if boot {
            std::thread::sleep(Duration::from_secs(12));
            for entry in BOOK.lock().unwrap().iter().filter(|entry| entry.when == When::Startup) {
                fire(&app, entry, false);
            }
        }
        let mut book = BOOK.lock().unwrap();
        loop {
            let now = Now::read();
            let mut changed = false;
            for index in (0..book.len()).rev() {
                let Some(at) = next_due(&book[index], &now).filter(|at| *at <= now.unix) else { continue };
                changed = true;
                match book[index].when {
                    When::Once { .. } => {
                        let entry = book.remove(index);
                        if now.unix - at <= LATE {
                            fire(&app, &entry, now.unix - at > 120);
                        }
                    }
                    _ => {
                        book[index].done = at;
                        fire(&app, &book[index], false);
                    }
                }
            }
            if changed {
                save(&app, &book);
            }
            let next = book.iter().filter_map(|entry| next_due(entry, &now)).min();
            book = match next {
                None => CHANGED.wait(book).unwrap(),
                Some(at) => {
                    let left = (at - now.unix).clamp(1, if at - now.unix > 600 { 300 } else { 30 });
                    CHANGED.wait_timeout(book, Duration::from_secs(left as u64)).unwrap().0
                }
            };
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alarms_for_the_phone() {
        let found = for_phone("ponme una alarma a las 6:30 para el gimnasio").unwrap();
        assert!(found.alarm);
        assert_eq!(found.what, "el gimnasio");
        assert!(found.at > Now::read().unix);
        let reminder = for_phone("recuérdame en 10 minutos sacar la ropa").unwrap();
        assert!(!reminder.alarm);
        assert_eq!(reminder.what, "sacar la ropa");
        assert!(for_phone("pon música").is_none());
        assert!(for_phone("despiértame mañana a las 7").is_some());
    }

    fn tuesday_3pm() -> Now {
        Now { unix: 20_000 * DAY + 15 * 3600, second: 15 * 3600, weekday: 1, date: (2026, 10, 13) }
    }

    fn at(spoken: &str) -> (When, bool, String) {
        parse(spoken, &tuesday_3pm()).unwrap_or_else(|| panic!("«{spoken}» no se tomó como algo para más tarde"))
    }

    fn today(hour: i64, minute: i64) -> When {
        When::Once { at: tuesday_3pm().midnight() + hour * 3600 + minute * 60 }
    }

    #[test]
    fn reminders_in_a_while() {
        let now = tuesday_3pm();
        assert_eq!(at("recuérdame en 10 minutos sacar la pizza"), (When::Once { at: now.unix + 600 }, false, "sacar la pizza".into()));
        assert_eq!(at("Recuérdame sacar la pizza en diez minutos."), (When::Once { at: now.unix + 600 }, false, "sacar la pizza".into()));
        assert_eq!(at("avísame en una hora y media que llame a mamá"), (When::Once { at: now.unix + 5400 }, false, "llame a mamá".into()));
        assert_eq!(at("pon un temporizador de 5 minutos"), (When::Once { at: now.unix + 300 }, false, "Se cumplió el tiempo.".into()));
        assert_eq!(at("dentro de media hora recuérdame la reunión"), (When::Once { at: now.unix + 1800 }, false, "la reunión".into()));
        assert_eq!(at("remind me in 20 minutes to stretch"), (When::Once { at: now.unix + 1200 }, false, "stretch".into()));
    }

    #[test]
    fn orders_at_a_time() {
        assert_eq!(at("a las 6 de la tarde pon música"), (today(18, 0), true, "pon música".into()));
        assert_eq!(at("a las 6 abre Spotify"), (today(18, 0), true, "abre Spotify".into()));
        assert_eq!(at("abre Spotify a las 2"), (today(26, 0), true, "abre Spotify".into()));
        assert_eq!(at("a las 8:30 pm pon rock"), (today(20, 30), true, "pon rock".into()));
        assert_eq!(at("a las 7 y media de la mañana pon noticias"), (today(31, 30), true, "pon noticias".into()));
        assert_eq!(at("mañana a las 8 recuérdame la cita con el médico"), (today(32, 0), false, "la cita con el médico".into()));
        assert_eq!(at("el viernes a las 10 de la noche pon salsa"), (today(22 + 3 * 24, 0), true, "pon salsa".into()));
    }

    #[test]
    fn dates_and_days_without_a_time() {
        assert_eq!(at("el 24 de diciembre a las 9 de la noche recuérdame llamar a la abuela"), (today(21 + 72 * 24, 0), false, "llamar a la abuela".into()));
        assert_eq!(at("recuérdame el 5 de marzo el cumpleaños de Ana"), (today(9 + 143 * 24, 0), false, "el cumpleaños de Ana".into()));
        assert_eq!(at("mañana recuérdame pagar la luz"), (today(33, 0), false, "pagar la luz".into()));
        assert_eq!(at("recuérdame el viernes la reunión"), (today(9 + 3 * 24, 0), false, "la reunión".into()));
        assert_eq!(day_number((2026, 12, 24)) - day_number((2026, 10, 13)), 72);
        assert_eq!(day_number((2028, 3, 1)) - day_number((2028, 2, 28)), 2);
    }

    #[test]
    fn routines() {
        assert_eq!(at("todos los días a las 8 abre Spotify"), (When::Daily { minute: 480, days: EVERY_DAY }, true, "abre Spotify".into()));
        assert_eq!(at("de lunes a viernes a las 9 de la mañana abre Discord"), (When::Daily { minute: 540, days: WEEKDAYS }, true, "abre Discord".into()));
        assert_eq!(at("cada lunes a las 7 pm recuérdame sacar la basura"), (When::Daily { minute: 19 * 60, days: 1 }, false, "sacar la basura".into()));
        assert_eq!(at("al encender la PC abre Discord y Spotify"), (When::Startup, true, "abre Discord y Spotify".into()));
        assert_eq!(at("cuando inicie Windows pon música suave"), (When::Startup, true, "pon música suave".into()));
    }

    #[test]
    fn ordinary_orders_are_left_alone() {
        let now = tuesday_3pm();
        for spoken in ["pon rock en español", "busca vuelos a las vegas", "abre el bloc de notas", "sube el volumen", "pon la de las seis de la mañana de Joaquín Sabina ahora", "pon música", "pon mañana será bonito", "busca el 24 de diciembre en google"] {
            assert!(parse(spoken, &now).is_none(), "«{spoken}» no es para más tarde");
        }
    }

    #[test]
    fn it_says_when() {
        let now = tuesday_3pm();
        assert_eq!(describe(&When::Once { at: now.unix + 600 }, &now), "en 10 min");
        assert_eq!(describe(&today(18, 0), &now), "a las 18:00");
        assert_eq!(describe(&today(32, 5), &now), "mañana a las 8:05");
        assert_eq!(describe(&today(22 + 3 * 24, 0), &now), "el viernes a las 22:00");
        assert_eq!(describe(&When::Daily { minute: 480, days: EVERY_DAY }, &now), "cada día a las 8:00");
        assert_eq!(describe(&When::Daily { minute: 540, days: WEEKDAYS }, &now), "de lunes a viernes a las 9:00");
        assert_eq!(describe(&When::Daily { minute: 540, days: 0b101 }, &now), "los lunes, miércoles a las 9:00");
    }

    #[test]
    fn a_routine_comes_due_once_a_day_on_its_days() {
        let now = tuesday_3pm();
        let mut entry = Entry { id: 1, when: When::Daily { minute: 15 * 60, days: EVERY_DAY }, act: true, what: String::new(), done: 0 };
        assert_eq!(next_due(&entry, &now), Some(now.unix));
        entry.done = now.unix;
        assert_eq!(next_due(&entry, &now), Some(now.unix + DAY));
        entry.when = When::Daily { minute: 15 * 60, days: 1 };
        entry.done = 0;
        assert_eq!(next_due(&entry, &now), Some(now.unix + 6 * DAY));
        entry.when = When::Daily { minute: 9 * 60, days: EVERY_DAY };
        assert_eq!(next_due(&entry, &now), Some(now.midnight() + DAY + 9 * 3600));
    }
}
