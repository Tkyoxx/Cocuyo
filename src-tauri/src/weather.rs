// clima con open-meteo: gratis y sin cuenta

use std::os::windows::process::CommandExt;
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager};

use crate::settings;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const FRESH_FOR: Duration = Duration::from_secs(20 * 60);

#[derive(Clone, Serialize)]
pub struct Hour {
    hour: u8,
    temp: f64,
    rain: u8,
    code: u16,
}

#[derive(Clone, Serialize)]
pub struct Weather {
    place: String,
    temp: f64,
    feels: f64,
    code: u16,
    day: bool,
    max: f64,
    min: f64,
    rain: u8,
    hours: Vec<Hour>,
    text: String,
}

static LAST: Mutex<Option<(Instant, String, Weather)>> = Mutex::new(None);

fn get(url: &str) -> Option<Value> {
    let output = Command::new("curl.exe").args(["--silent", "--max-time", "8", url]).creation_flags(CREATE_NO_WINDOW).output().ok()?;
    serde_json::from_slice(&output.stdout).ok()
}

fn url_encode(text: &str) -> String {
    text.bytes().map(|b| if b.is_ascii_alphanumeric() { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

pub fn sky(code: u16) -> &'static str {
    match code {
        0 => "Despejado",
        1 => "Casi despejado",
        2 => "Algunas nubes",
        3 => "Nublado",
        45 | 48 => "Niebla",
        51..=57 => "Llovizna",
        61..=67 => "Lluvia",
        71..=77 => "Nieve",
        80..=82 => "Chubascos",
        85 | 86 => "Nevadas",
        95..=99 => "Tormenta",
        _ => "Variable",
    }
}

fn locate(app: &AppHandle, city: &str) -> Result<(f64, f64, String), String> {
    let file = app.path().app_config_dir().map_err(|e| e.to_string())?.join("clima.json");
    let saved: Option<Value> = std::fs::read(&file).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok());
    if let Some(saved) = saved.filter(|saved| saved["city"] == city) {
        if let (Some(lat), Some(lon), Some(name)) = (saved["lat"].as_f64(), saved["lon"].as_f64(), saved["name"].as_str()) {
            return Ok((lat, lon, name.to_string()));
        }
    }
    let found = get(&format!("https://geocoding-api.open-meteo.com/v1/search?name={}&count=1&language=es", url_encode(city)))
        .ok_or("No hay conexión con el servicio del clima.")?;
    let place = &found["results"][0];
    let (Some(lat), Some(lon), Some(name)) = (place["latitude"].as_f64(), place["longitude"].as_f64(), place["name"].as_str()) else {
        return Err(format!("No encontré ninguna ciudad llamada «{city}»."));
    };
    let _ = std::fs::write(&file, serde_json::json!({ "city": city, "lat": lat, "lon": lon, "name": name }).to_string());
    Ok((lat, lon, name.to_string()))
}

fn read(place: String, data: &Value) -> Option<Weather> {
    let now = &data["current"];
    let daily = &data["daily"];
    let hourly = &data["hourly"];
    let this_hour = now["time"].as_str()?.get(..13)?;
    let times = hourly["time"].as_array()?;
    let from = times.iter().position(|time| time.as_str().is_some_and(|time| time.starts_with(this_hour)))?;
    let hours = (1..=4)
        .filter_map(|step| {
            let at = from + step * 2;
            Some(Hour {
                hour: times.get(at)?.as_str()?.get(11..13)?.parse().ok()?,
                temp: hourly["temperature_2m"][at].as_f64()?,
                rain: hourly["precipitation_probability"][at].as_f64().unwrap_or(0.0) as u8,
                code: hourly["weather_code"][at].as_u64().unwrap_or(0) as u16,
            })
        })
        .collect();
    let code = now["weather_code"].as_u64()? as u16;
    Some(Weather {
        place,
        temp: now["temperature_2m"].as_f64()?,
        feels: now["apparent_temperature"].as_f64()?,
        code,
        day: now["is_day"].as_u64().unwrap_or(1) == 1,
        max: daily["temperature_2m_max"][0].as_f64()?,
        min: daily["temperature_2m_min"][0].as_f64()?,
        rain: daily["precipitation_probability_max"][0].as_f64().unwrap_or(0.0) as u8,
        hours,
        text: sky(code).to_string(),
    })
}

pub fn today(app: &AppHandle) -> Result<Weather, String> {
    let mut city = settings::load(app).city.trim().to_string();
    if city.is_empty() {
        let found = get("https://ipwho.is/?fields=success,city,latitude,longitude").filter(|found| found["success"] == true);
        let place = found.as_ref().and_then(|found| Some((found["city"].as_str()?.to_string(), found["latitude"].as_f64()?, found["longitude"].as_f64()?)));
        let Some((name, lat, lon)) = place.filter(|(name, _, _)| !name.is_empty()) else {
            return Err("Escribe tu ciudad en la pestaña Hoy para ver el clima.".into());
        };
        if let Ok(dir) = app.path().app_config_dir() {
            let _ = std::fs::write(dir.join("clima.json"), serde_json::json!({ "city": name, "lat": lat, "lon": lon, "name": name }).to_string());
        }
        let mut config = settings::load(app);
        config.city = name.clone();
        let _ = settings::set_settings(app.clone(), config);
        city = name;
    }
    if let Some((when, known, weather)) = LAST.lock().unwrap().as_ref() {
        if *known == city && when.elapsed() < FRESH_FOR {
            return Ok(weather.clone());
        }
    }
    let (lat, lon, name) = locate(app, &city)?;
    let data = get(&format!(
        "https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}&current=temperature_2m,apparent_temperature,weather_code,is_day\
         &hourly=temperature_2m,precipitation_probability,weather_code&daily=temperature_2m_max,temperature_2m_min,precipitation_probability_max\
         &timezone=auto&forecast_days=2"
    ))
    .ok_or("No hay conexión con el servicio del clima.")?;
    let weather = read(name, &data).ok_or("El servicio del clima respondió algo que no entiendo.")?;
    *LAST.lock().unwrap() = Some((Instant::now(), city, weather.clone()));
    Ok(weather)
}

#[tauri::command]
pub async fn weather(app: AppHandle) -> Result<Weather, String> {
    today(&app)
}

#[tauri::command]
pub async fn weather_city(app: AppHandle, city: String) -> Result<Weather, String> {
    let city = city.trim().to_string();
    if city.is_empty() {
        return Err("Escribe el nombre de tu ciudad.".into());
    }
    locate(&app, &city)?;
    let mut config = settings::load(&app);
    config.city = city;
    settings::set_settings(app.clone(), config)?;
    today(&app)
}

pub fn spoken(app: &AppHandle, spoken: &str) -> Option<Result<String, String>> {
    let clean = crate::intent::normalize(spoken);
    let first = clean.split_whitespace().next().unwrap_or("");
    if matches!(first, "pon" | "ponme" | "busca" | "abre" | "play" | "search" | "open" | "toca" | "anota" | "ejecuta") {
        return None;
    }
    let about_sky = ["clima", "temperatura", "llover", "lluvia", "llueve", "weather", "rain", "chuva", "frio", "calor"].iter().any(|word| clean.split_whitespace().any(|said| said == *word))
        || (clean.contains("tiempo") && ["hace", "hara", "va a hacer"].iter().any(|verb| clean.contains(verb)));
    if !about_sky {
        return None;
    }
    Some(today(app).map(|weather| {
        format!(
            "En {} hay {} grados, {}. Hoy, máxima de {} y mínima de {}; {} % de probabilidad de lluvia.",
            weather.place,
            weather.temp.round(),
            weather.text.to_lowercase(),
            weather.max.round(),
            weather.min.round(),
            weather.rain
        )
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_forecast_is_read_from_now_on() {
        let data = serde_json::json!({
            "current": { "time": "2026-10-13T19:15", "temperature_2m": 21.3, "apparent_temperature": 25.0, "weather_code": 3, "is_day": 0 },
            "daily": { "temperature_2m_max": [27.4, 28.0], "temperature_2m_min": [18.9, 19.0], "precipitation_probability_max": [60, 30] },
            "hourly": {
                "time": (0..48).map(|hour| format!("2026-10-{}T{:02}:00", 13 + hour / 24, hour % 24)).collect::<Vec<_>>(),
                "temperature_2m": (0..48).map(|hour| 15.0 + f64::from(hour) / 10.0).collect::<Vec<_>>(),
                "precipitation_probability": (0..48).collect::<Vec<_>>(),
                "weather_code": vec![61; 48],
            }
        });
        let weather = read("Caracas".into(), &data).unwrap();
        assert_eq!((weather.temp, weather.code, weather.day, weather.rain), (21.3, 3, false, 60));
        assert_eq!(weather.text, "Nublado");
        assert_eq!(weather.hours.iter().map(|hour| hour.hour).collect::<Vec<_>>(), [21, 23, 1, 3]);
        assert_eq!(weather.hours[0].rain, 21);
    }

    #[test]
    fn the_sky_has_a_name() {
        assert_eq!((sky(0), sky(2), sky(63), sky(81), sky(96)), ("Despejado", "Algunas nubes", "Lluvia", "Chubascos", "Tormenta"));
    }
}
