// buscar donde tu digas: steam con precios, internet en una tarjeta y unas cuantas paginas mas

use std::os::windows::process::CommandExt;
use std::process::Command;

use serde_json::{json, Value};
use tauri::AppHandle;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const VERBS: [&str; 18] = [
    "busca", "buscame", "buscar", "busque", "buscale", "encuentra", "encuentrame", "mira", "mirame", "search", "find", "look", "procura", "procure", "pesquisa",
    "pesquise", "consulta", "chequea",
];
const FILLERS: [&str; 7] = ["oye", "cocuyo", "porfa", "por", "favor", "please", "hey"];
const ON: [&str; 6] = ["en", "on", "in", "no", "na", "por"];

#[derive(Clone, Copy, PartialEq, Debug)]
enum Place {
    Steam,
    Web,
    Page(&'static str, &'static str),
}

const PLACES: &[(&[&str], Place)] = &[
    (&["steam", "stim", "estim", "la tienda de steam", "tienda de steam"], Place::Steam),
    (&["internet", "la web", "la red", "la internet", "web", "the web", "the internet", "duckduckgo"], Place::Web),
    (&["wikipedia", "wiki", "la wikipedia"], Place::Page("Wikipedia", "https://{lang}.wikipedia.org/w/index.php?search={q}")),
    (&["amazon"], Place::Page("Amazon", "https://www.amazon.com/s?k={q}")),
    (&["github", "guijab", "gitjab"], Place::Page("GitHub", "https://github.com/search?q={q}")),
    (&["twitch"], Place::Page("Twitch", "https://www.twitch.tv/search?term={q}")),
    (&["reddit", "redit"], Place::Page("Reddit", "https://www.reddit.com/search/?q={q}")),
    (&["epic games", "epic", "la tienda de epic", "epic store"], Place::Page("Epic Games", "https://store.epicgames.com/browse?q={q}&sortBy=relevancy")),
    (&["gog"], Place::Page("GOG", "https://www.gog.com/games?query={q}")),
    (&["google maps", "maps", "mapas", "el mapa", "mapa"], Place::Page("Mapas", "https://www.google.com/maps/search/{q}")),
    (&["imagenes", "fotos", "images", "pictures", "imagens"], Place::Page("Imágenes", "https://www.google.com/search?tbm=isch&q={q}")),
    (&["noticias", "las noticias", "news", "the news"], Place::Page("Noticias", "https://news.google.com/search?q={q}")),
    (&["tiktok"], Place::Page("TikTok", "https://www.tiktok.com/search?q={q}")),
    (&["mercado libre", "mercadolibre"], Place::Page("Mercado Libre", "https://listado.mercadolibre.com.co/{q}")),
];

fn words_of(spoken: &str) -> (Vec<String>, Vec<String>) {
    let mut original = Vec::new();
    let mut plain = Vec::new();
    for word in spoken.split_whitespace() {
        let flat = crate::intent::normalize(word).split_whitespace().collect::<Vec<_>>().join(" ");
        if flat.is_empty() {
            continue;
        }
        original.push(word.trim_matches(|c: char| matches!(c, ',' | '.' | '?' | '¿' | '!' | '¡' | ':' | ';' | '«' | '»' | '"')).to_string());
        plain.push(flat);
    }
    (original, plain)
}

fn place_at(plain: &[String], at: usize) -> Option<(Place, usize)> {
    PLACES
        .iter()
        .flat_map(|(names, place)| names.iter().map(move |name| (name, *place)))
        .filter_map(|(name, place)| {
            let parts: Vec<&str> = name.split(' ').collect();
            let fits = plain.len() >= at + parts.len() && parts.iter().enumerate().all(|(index, part)| plain[at + index] == *part);
            fits.then_some((place, parts.len()))
        })
        .max_by_key(|(_, size)| *size)
}

fn parse(spoken: &str) -> Option<(Place, String)> {
    let (original, plain) = words_of(spoken);
    let mut at = plain.iter().take_while(|word| FILLERS.contains(&word.as_str())).count();
    if !plain.get(at).is_some_and(|word| VERBS.contains(&word.as_str())) {
        return None;
    }
    at += 1;
    if plain.get(at).is_some_and(|word| matches!(word.as_str(), "me" | "up" | "for")) {
        at += 1;
    }
    if plain.get(at).is_some_and(|word| ON.contains(&word.as_str())) {
        if let Some((place, size)) = place_at(&plain, at + 1) {
            let query = original[at + 1 + size..].join(" ");
            return (!query.trim().is_empty()).then(|| (place, clean_query(&query)));
        }
    }
    for on in (at + 1..plain.len()).rev() {
        if !ON.contains(&plain[on].as_str()) {
            continue;
        }
        if let Some((place, size)) = place_at(&plain, on + 1) {
            if on + 1 + size == plain.len() {
                let query = original[at..on].join(" ");
                return (!query.trim().is_empty()).then(|| (place, clean_query(&query)));
            }
        }
    }
    None
}

fn clean_query(query: &str) -> String {
    let mut words: Vec<&str> = query.split_whitespace().collect();
    while words.first().is_some_and(|word| matches!(crate::intent::normalize(word).trim(), "el" | "la" | "los" | "las" | "un" | "una" | "juego" | "the" | "a" | "o")) && words.len() > 1 {
        words.remove(0);
    }
    words.join(" ")
}

pub fn spoken(app: &AppHandle, spoken: &str) -> Option<Result<String, String>> {
    let (place, query) = parse(spoken)?;
    let done = match place {
        Place::Steam => steam(app, &query),
        Place::Web => web(app, &query),
        Place::Page(name, address) => {
            let lang = crate::cue::language().0;
            let url = address.replace("{lang}", lang).replace("{q}", &crate::voice::url_encode(&query));
            crate::apps::open(&url).map(|()| format!("Buscando «{query}» en {name}."))
        }
    };
    match &done {
        Ok(said) => crate::cue::answer(said),
        Err(why) => crate::cue::answer(why),
    }
    Some(done)
}

fn curl_json(url: &str) -> Result<Value, String> {
    let output = Command::new("curl.exe")
        .args(["--silent", "--compressed", "--max-time", "10", "--connect-timeout", "5", "--user-agent", "Cocuyo/0.2 (https://github.com/Tkyoxx/Cocuyo)", url])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| e.to_string())?;
    serde_json::from_slice(&output.stdout).map_err(|_| "Steam no respondió. ¿Hay conexión?".to_string())
}

fn steam_installed() -> bool {
    use windows::Win32::System::Registry::HKEY_CURRENT_USER;
    crate::apps::reg_string(HKEY_CURRENT_USER, "Software\\Valve\\Steam", "SteamExe").is_some_and(|exe| std::path::Path::new(&exe).exists())
}

pub(crate) fn money(cents: u64, currency: &str) -> String {
    let whole = cents / 100;
    let rest = cents % 100;
    let english = crate::cue::language().0 == "en";
    let digits = whole.to_string();
    let mut grouped = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            grouped.push(if english { ',' } else { '.' });
        }
        grouped.push(digit);
    }
    let decimals = if rest == 0 { String::new() } else { format!("{}{rest:02}", if english { '.' } else { ',' }) };
    let symbol = match currency {
        "EUR" => "€",
        "GBP" => "£",
        "BRL" => "R$",
        _ => "$",
    };
    format!("{symbol}{grouped}{decimals} {currency}")
}

fn price_of(item: &Value) -> (String, bool) {
    let Some(price) = item.get("price").filter(|price| price.is_object()) else {
        return (crate::i18n::tr("Gratis"), false);
    };
    let currency = price["currency"].as_str().unwrap_or("USD");
    let last = price["final"].as_u64().unwrap_or(0);
    let first = price["initial"].as_u64().unwrap_or(last);
    if first > last && first > 0 {
        let off = 100 - last * 100 / first;
        (format!("−{off} % · {}", money(last, currency)), true)
    } else {
        (money(last, currency), false)
    }
}

fn country() -> String {
    use windows::Win32::Globalization::{GetGeoInfoW, GetUserGeoID, GEOCLASS_NATION, GEO_ISO2};
    let mut name = [0u16; 8];
    let size = unsafe { GetGeoInfoW(GetUserGeoID(GEOCLASS_NATION), GEO_ISO2, Some(&mut name), 0) };
    let code = String::from_utf16_lossy(&name[..(size.max(1) as usize - 1).min(name.len())]);
    if code.len() == 2 && code.chars().all(|c| c.is_ascii_alphabetic()) { code.to_ascii_uppercase() } else { "US".into() }
}

fn steam(app: &AppHandle, query: &str) -> Result<String, String> {
    let language = match crate::cue::language().0 {
        "en" => "english",
        "pt" => "brazilian",
        _ => "spanish",
    };
    let found = curl_json(&format!("https://store.steampowered.com/api/storesearch/?term={}&l={language}&cc={}", crate::voice::url_encode(query), country()))?;
    let items: Vec<&Value> = found["items"].as_array().map(|items| items.iter().take(6).collect()).unwrap_or_default();
    let installed = steam_installed();
    let all = format!("https://store.steampowered.com/search/?term={}", crate::voice::url_encode(query));
    let open = |url: String| if installed { format!("steam://openurl/{url}") } else { url };
    if items.is_empty() {
        return Err(format!("No encontré «{query}» en Steam."));
    }
    let rows: Vec<Value> = items.iter().map(|item| json!({ "texto": item["name"].as_str().unwrap_or(""), "valor": price_of(item).0 })).collect();
    let top = items[0];
    let name = top["name"].as_str().unwrap_or(query);
    let id = top["id"].as_u64().unwrap_or(0);
    let mut buttons = vec![json!({ "texto": crate::i18n::tr("Ver en Steam"), "abrir": open(format!("https://store.steampowered.com/app/{id}")) })];
    if let Some(owned) = crate::apps::find(app, name, 0.9).filter(|owned| owned.id.contains(&format!("rungameid/{id}"))) {
        buttons.insert(0, json!({ "texto": crate::i18n::tr("Jugar"), "abrir": owned.id }));
    }
    buttons.push(json!({ "texto": crate::i18n::tr("Ver todo"), "abrir": open(all) }));
    crate::scripts::show_card(app, "Steam", &json!({
        "titulo": format!("«{query}» en Steam"), "icono": "🎮", "filas": rows, "botones": buttons, "segundos": 40
    }));
    let (price, sale) = price_of(top);
    Ok(if sale { format!("{name} está en oferta en Steam: {price}.") } else { format!("{name} en Steam: {price}.") })
}

fn web(app: &AppHandle, query: &str) -> Result<String, String> {
    let page = format!("https://duckduckgo.com/?q={}", crate::voice::url_encode(query));
    let hits = match crate::investiga::quick_search(query) {
        Some(hits) if !hits.is_empty() => hits,
        _ => {
            crate::apps::open(&page)?;
            return Ok(format!("Buscando «{query}» en internet."));
        }
    };
    let rows: Vec<Value> = hits.iter().take(6).map(|(_, title, site)| json!({ "texto": title, "valor": site })).collect();
    let mut buttons: Vec<Value> = hits.iter().take(2).map(|(url, _, site)| json!({ "texto": site, "abrir": url })).collect();
    buttons.push(json!({ "texto": crate::i18n::tr("Ver todo"), "abrir": page }));
    crate::scripts::show_card(app, "Cocuyo", &json!({ "titulo": format!("«{query}» en internet"), "icono": "🔎", "filas": rows, "botones": buttons, "segundos": 45 }));
    Ok(format!("Esto salió en internet sobre «{query}»."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_place_and_the_query_come_apart() {
        assert_eq!(parse("busca Hollow Knight en Steam"), Some((Place::Steam, "Hollow Knight".into())));
        assert_eq!(parse("oye búscame en steam el juego Celeste"), Some((Place::Steam, "Celeste".into())));
        assert_eq!(parse("busca en la tienda de steam juegos de terror"), Some((Place::Steam, "juegos de terror".into())));
        assert_eq!(parse("busca cómo cambiar la pasta térmica en internet"), Some((Place::Web, "cómo cambiar la pasta térmica".into())));
        assert_eq!(parse("busca en la web precio de la RX 7600"), Some((Place::Web, "precio de la RX 7600".into())));
        assert_eq!(parse("search portal 2 on steam"), Some((Place::Steam, "portal 2".into())));
        assert!(matches!(parse("busca Simón Bolívar en Wikipedia"), Some((Place::Page("Wikipedia", _), q)) if q == "Simón Bolívar"));
        assert!(matches!(parse("busca un restaurante en el mapa"), Some((Place::Page("Mapas", _), q)) if q == "restaurante"));
        assert!(matches!(parse("busca restaurantes en mapas"), Some((Place::Page("Mapas", _), q)) if q == "restaurantes"));
        assert_eq!(parse("busca gatos en youtube"), None);
        assert_eq!(parse("busca el clima de mañana"), None);
        assert_eq!(parse("pon Hollow Knight en Steam"), None);
        assert_eq!(parse("busca en steam"), None);
    }

    #[test]
    fn prices_read_like_prices() {
        assert_eq!(money(3_750_000, "COP"), "$37.500 COP");
        assert_eq!(money(1999, "USD"), "$19,99 USD");
        assert_eq!(money(5_970_000, "COP"), "$59.700 COP");
        let (sale, on) = price_of(&json!({ "price": { "currency": "COP", "initial": 19_900_000, "final": 5_970_000 } }));
        assert!(on && sale.starts_with("−70 %"));
        assert!(!price_of(&json!({ "name": "gratis" })).1);
    }

    #[test]
    #[ignore]
    fn steam_de_verdad() {
        println!("país: {}", country());
        let found = curl_json(&format!("https://store.steampowered.com/api/storesearch/?term=hollow%20knight&l=spanish&cc={}", country())).unwrap();
        for item in found["items"].as_array().unwrap() {
            println!("{} -> {:?}", item["name"], price_of(item));
        }
    }
}
