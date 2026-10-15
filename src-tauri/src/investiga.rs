// investigar a fondo: busca en duckduckgo con curl, lee las paginas, cruza lo que dicen y arma un informe con sus fuentes

use std::collections::HashMap;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const BROWSER: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36 Cocuyo/0.2";
const ROBOT: &str = "cocuyo";
const MARK: &[u8] = b"\n<<cocuyo>>";
const SEARCHES: usize = 5;
const PAGES: usize = 8;
const TRIES: usize = 13;
const EXTRA_PAGES: usize = 3;
const PAGE_TEXT: usize = 7000;
const ENOUGH_TEXT: usize = 700;
const SKIP: [&str; 16] = [
    "duckduckgo.com", "youtube.com", "youtu.be", "facebook.com", "instagram.com", "tiktok.com", "pinterest.", "twitter.com", "x.com",
    "linkedin.com", "amazon.", "play.google.com", "apps.apple.com", "quora.com", "translate.google", "bing.com",
];

static BUSY: AtomicBool = AtomicBool::new(false);
static ROBOTS: Mutex<Vec<(String, Option<String>)>> = Mutex::new(Vec::new());
static LAST_SEARCH: Mutex<Option<Instant>> = Mutex::new(None);
const SEARCH_GAP: Duration = Duration::from_secs(8);
const BREATHER: Duration = Duration::from_secs(20);

const ASKS: [&str; 22] = [
    "investiga a fondo", "investigame a fondo", "investiga bien", "investiga", "investigame", "haz una investigacion a fondo", "hazme una investigacion a fondo",
    "haz una investigacion", "hazme una investigacion", "averigua a fondo", "averiguame a fondo", "busca a fondo", "deep research", "do a deep research on",
    "research", "investigate", "dig into", "pesquisa a fundo", "pesquise a fundo", "faca uma pesquisa sobre", "pesquise sobre", "pesquise",
];
const ABOUT: [&str; 8] = ["sobre", "acerca de", "de", "about", "on", "into", "sobre o", "sobre a"];

pub fn topic(spoken: &str) -> Option<String> {
    let spoken = crate::agent::called(spoken).unwrap_or_else(|| spoken.to_string());
    let mut clean = crate::intent::normalize(&spoken);
    let mut skip = 0;
    for lead in ["oye ", "cocuyo ", "porfa ", "por favor "] {
        if clean.starts_with(lead) {
            clean = clean[lead.len()..].trim_start().to_string();
            skip += lead.split_whitespace().count();
        }
    }
    let lead = ASKS.iter().filter(|lead| clean.starts_with(&format!("{lead} "))).max_by_key(|lead| lead.len())?;
    skip += lead.split_whitespace().count();
    let rest = clean[lead.len()..].trim_start();
    if let Some(about) = ABOUT.iter().filter(|about| rest.starts_with(&format!("{about} "))).max_by_key(|about| about.len()) {
        skip += about.split_whitespace().count();
    }
    let words: Vec<&str> = spoken.split_whitespace().skip(skip).collect();
    let topic = words.join(" ").trim_matches(|c: char| c.is_whitespace() || matches!(c, ',' | ':' | '.' | ';')).to_string();
    (topic.split_whitespace().count() >= 2).then_some(topic)
}

pub fn spoken(app: &AppHandle, said: &str) -> Option<Result<String, String>> {
    let topic = topic(said)?;
    let outcome = start(app, &topic);
    match &outcome {
        Ok(said) | Err(said) => crate::cue::answer(said),
    }
    Some(outcome)
}

pub fn start(app: &AppHandle, topic: &str) -> Result<String, String> {
    if crate::ai::mode(app).is_none() {
        return Err("Para investigar a fondo hace falta la IA: conéctala en Ajustes, sección IA.".into());
    }
    if BUSY.swap(true, Ordering::SeqCst) {
        return Ok("Ya estoy investigando otra cosa. Cuando acabe te aviso.".into());
    }
    let (app, topic) = (app.clone(), topic.trim().to_string());
    let said = format!("Me pongo a investigar «{topic}». Cuando tenga el informe te lo abro.");
    std::thread::spawn(move || {
        let outcome = run(&app, &topic);
        BUSY.store(false, Ordering::SeqCst);
        match outcome {
            Ok(done) => {
                step(&app, 1.0, "Informe listo");
                let _ = crate::apps::open(&done.path.to_string_lossy());
                let calls = format!("Gasté {} llamadas a la IA.", done.calls);
                crate::scripts::show_card(&app, "Cocuyo", &json!({
                    "titulo": done.title, "texto": format!("{} fuentes leídas. {calls}", done.sources), "icono": "🔎", "segundos": 40,
                    "botones": [{ "texto": "Abrir", "abrir": done.path.to_string_lossy() }]
                }));
                crate::cue::answer(&format!("{} {calls}", done.said.trim()));
            }
            Err(why) => {
                step(&app, 1.0, "No pude terminar");
                crate::cue::answer(&why);
            }
        }
    });
    Ok(said)
}

fn step(app: &AppHandle, value: f64, label: &str) {
    let _ = app.emit_to(crate::overlay::LABEL, "script-progress", json!({ "script": crate::i18n::tr("Investigación"), "valor": value, "texto": label }));
}

struct Hit {
    url: String,
    title: String,
    snippet: String,
    score: f32,
}

struct Source {
    url: String,
    title: String,
    site: String,
    text: String,
}

struct Done {
    path: PathBuf,
    title: String,
    said: String,
    sources: usize,
    calls: usize,
}

fn language() -> (&'static str, &'static str) {
    match crate::cue::language().0 {
        "en" => ("en", "inglés"),
        "pt" => ("pt", "portugués"),
        _ => ("es", "español"),
    }
}

fn run(app: &AppHandle, topic: &str) -> Result<Done, String> {
    let began = Instant::now();
    let spent_before = crate::ai::calls();
    let (_, tongue) = language();
    step(app, 0.03, "Pensando qué buscar");
    let plan = plan(app, topic, tongue);
    let mut hits: Vec<Hit> = Vec::new();
    let mut searched = Vec::new();
    let mut missed = Vec::new();
    let mut stopped = false;
    for (index, query) in plan.queries.iter().enumerate() {
        step(app, 0.06 + 0.2 * index as f64 / plan.queries.len() as f64, &format!("Buscando «{query}»"));
        match search_patiently(app, query, &mut stopped) {
            Some(found) => {
                merge(&mut hits, found);
                searched.push(query.clone());
            }
            None if stopped => missed.push(query.clone()),
            None => {}
        }
    }
    if hits.is_empty() && stopped {
        return Err("DuckDuckGo me pidió una pausa por buscar tanto. Prueba otra vez en unos minutos.".into());
    }
    if hits.is_empty() {
        return Err("No pude buscar en internet: DuckDuckGo no respondió. ¿Hay conexión?".into());
    }
    let mut sources = read_best(app, &hits, &[], PAGES, (0.28, 0.5));
    if sources.len() < 2 {
        return Err("Casi ninguna página se dejó leer. Prueba con otras palabras.".into());
    }
    step(app, 0.52, "Sacando lo importante");
    let mut notes = take_notes(app, topic, &plan.aspects, &sources, 0, tongue);
    step(app, 0.66, "Mirando qué falta");
    let missing = gaps(app, topic, &plan.aspects, &notes, tongue);
    if !missing.is_empty() {
        let mut more = Vec::new();
        for query in &missing {
            step(app, 0.68, &format!("Buscando «{query}»"));
            match search_patiently(app, query, &mut stopped) {
                Some(found) => {
                    merge(&mut more, found);
                    searched.push(query.clone());
                }
                None if stopped => missed.push(query.clone()),
                None => {}
            }
        }
        let seen: Vec<String> = sources.iter().map(|source| key(&source.url)).collect();
        let extra = read_best(app, &more, &seen, EXTRA_PAGES, (0.7, 0.76));
        if !extra.is_empty() {
            step(app, 0.78, "Sacando lo importante");
            let first = sources.len();
            sources.extend(extra);
            notes.extend(take_notes(app, topic, &plan.aspects, &sources[first..], first, tongue));
        }
    }
    step(app, 0.85, "Escribiendo el informe");
    let report = write(app, topic, &plan, &sources, &notes, tongue)?;
    let calls = crate::ai::calls().saturating_sub(spent_before);
    let title = report["titulo"].as_str().filter(|title| !title.trim().is_empty()).unwrap_or(&plan.title).trim().to_string();
    let said = report["dicho"].as_str().unwrap_or("").trim().to_string();
    let said = if said.is_empty() { format!("Listo el informe sobre {topic}.") } else { said };
    let page = render(topic, &title, &report, &sources, (&searched, &missed), calls, began.elapsed());
    let path = save(app, &title, &page)?;
    Ok(Done { path, title, said, sources: sources.len(), calls })
}

struct Plan {
    title: String,
    queries: Vec<String>,
    aspects: Vec<String>,
}

fn json_in(reply: &str) -> Option<Value> {
    let start = reply.find('{')?;
    let end = reply.rfind('}')?;
    (end > start).then(|| serde_json::from_str(&reply[start..=end]).ok()).flatten()
}

fn strings(value: &Value, limit: usize) -> Vec<String> {
    value.as_array().map_or_else(Vec::new, |list| {
        list.iter().filter_map(Value::as_str).map(|text| text.trim().to_string()).filter(|text| !text.is_empty()).take(limit).collect()
    })
}

fn plan(app: &AppHandle, topic: &str, tongue: &str) -> Plan {
    let now = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    let system = format!(
        "Eres un investigador. Hoy es {:02}/{:02}/{}. Te dan un tema y tú decides cómo investigarlo en un buscador web. \
         Parte la pregunta en 3 a 6 búsquedas distintas que juntas la cubran entera: la idea central, datos y cifras, lo más reciente, \
         opiniones a favor y en contra, y lo oficial o técnico si aplica. Búsquedas cortas, de 3 a 8 palabras, como las escribiría un experto; \
         en {tongue} y, si el tema se investiga mejor en inglés, alguna en inglés. Responde SOLO con JSON: \
         {{\"titulo\": \"título corto del informe en {tongue}\", \"busquedas\": [\"...\"], \"aspectos\": [\"lo que el informe tiene que responder\"]}}",
        now.wDay, now.wMonth, now.wYear
    );
    let reply = crate::ai::ask(app, &system, topic, 600).ok().and_then(|reply| json_in(&reply)).unwrap_or(Value::Null);
    let mut queries = strings(&reply["busquedas"], SEARCHES);
    queries.dedup();
    if queries.is_empty() {
        queries.push(topic.to_string());
    }
    let title = reply["titulo"].as_str().map(str::trim).filter(|title| !title.is_empty()).unwrap_or(topic).to_string();
    Plan { title, queries, aspects: strings(&reply["aspectos"], 8) }
}

fn encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'%' if at + 2 < bytes.len() => match std::str::from_utf8(&bytes[at + 1..at + 3]).ok().and_then(|hex| u8::from_str_radix(hex, 16).ok()) {
                Some(byte) => {
                    out.push(byte);
                    at += 3;
                }
                None => {
                    out.push(b'%');
                    at += 1;
                }
            },
            b'+' => {
                out.push(b' ');
                at += 1;
            }
            byte => {
                out.push(byte);
                at += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

struct Got {
    status: u16,
    kind: String,
    url: String,
    body: Vec<u8>,
}

fn get(url: &str, seconds: u32) -> Result<Got, String> {
    fetch(url, seconds, None)
}

fn fetch(url: &str, seconds: u32, form: Option<&str>) -> Result<Got, String> {
    let mut curl = Command::new("curl.exe");
    if let Some(form) = form {
        curl.args(["--data", form]);
    }
    let output = curl
        .args(["--silent", "--location", "--max-redirs", "5", "--compressed", "--connect-timeout", "6", "--max-filesize", "3000000"])
        .args(["--max-time", &seconds.to_string(), "--user-agent", BROWSER, "--header", "Accept-Language: es,en;q=0.8,pt;q=0.6"])
        .args(["--write-out", "\n<<cocuyo>>%{http_code}<<cocuyo>>%{content_type}<<cocuyo>>%{url_effective}", "--url", url])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| e.to_string())?;
    let all = output.stdout;
    let at = all.windows(MARK.len()).rposition(|window| window == MARK).ok_or("curl no respondió")?;
    let tail = String::from_utf8_lossy(&all[at + MARK.len()..]).into_owned();
    let mut parts = tail.split("<<cocuyo>>");
    let status = parts.next().unwrap_or("0").trim().parse().unwrap_or(0);
    let kind = parts.next().unwrap_or("").to_ascii_lowercase();
    let url = parts.next().unwrap_or(url).trim().to_string();
    let mut body = all;
    body.truncate(at);
    Ok(Got { status, kind, url, body })
}

enum Searched {
    Found(Vec<Hit>),
    Nothing,
    Stopped,
}

fn polite() {
    let mut last = LAST_SEARCH.lock().unwrap();
    if let Some(wait) = last.and_then(|at| SEARCH_GAP.checked_sub(at.elapsed())) {
        std::thread::sleep(wait);
    }
    *last = Some(Instant::now());
}

fn search(query: &str) -> Searched {
    polite();
    let form = format!("q={}&b=", encode(query));
    let Ok(got) = fetch("https://html.duckduckgo.com/html/", 15, Some(&form)) else { return Searched::Nothing };
    let page = String::from_utf8_lossy(&got.body).into_owned();
    if got.status == 202 || page.contains("anomaly-modal") {
        return Searched::Stopped;
    }
    let mut hits = if got.status == 200 { results(&page, "class=\"result__a\"", "result__snippet") } else { Vec::new() };
    if hits.is_empty() && got.status == 200 {
        polite();
        if let Ok(got) = fetch("https://lite.duckduckgo.com/lite/", 15, Some(&form)) {
            hits = results(&String::from_utf8_lossy(&got.body), "class='result-link'", "result-snippet");
        }
    }
    if hits.is_empty() { Searched::Nothing } else { Searched::Found(hits) }
}

pub fn quick_search(query: &str) -> Option<Vec<(String, String, String)>> {
    match search(query) {
        Searched::Found(hits) => Some(hits.into_iter().map(|hit| (host(&hit.url), hit.title, hit.url)).map(|(site, title, url)| (url, title, site)).collect()),
        _ => None,
    }
}

fn search_patiently(app: &AppHandle, query: &str, stopped: &mut bool) -> Option<Vec<Hit>> {
    if *stopped {
        return None;
    }
    match search(query) {
        Searched::Found(hits) => return Some(hits),
        Searched::Nothing => return None,
        Searched::Stopped => {}
    }
    step(app, 0.2, "DuckDuckGo pidió una pausa");
    std::thread::sleep(BREATHER);
    match search(query) {
        Searched::Found(hits) => Some(hits),
        Searched::Nothing => None,
        Searched::Stopped => {
            *stopped = true;
            None
        }
    }
}

fn results(page: &str, link: &str, snippet: &str) -> Vec<Hit> {
    let mut hits = Vec::new();
    let marks: Vec<usize> = page.match_indices(link).map(|(at, _)| at).collect();
    for (index, &at) in marks.iter().enumerate() {
        let next = marks.get(index + 1).copied().unwrap_or(page.len());
        let Some(open) = page[..at].rfind("<a ") else { continue };
        let Some(close) = page[at..].find('>').map(|end| at + end) else { continue };
        let tag = &page[open..close];
        let card = page[..open].rfind("<div class=\"result").map(|start| &page[start..page[start..].find('>').map_or(open, |end| start + end)]);
        if card.is_some_and(|card| card.contains("result--ad")) {
            continue;
        }
        let Some(href) = tag.split("href=\"").nth(1).and_then(|rest| rest.split('"').next()) else { continue };
        let href = unescape(href);
        let url = match href.split_once("uddg=") {
            Some((_, rest)) => decode(rest.split('&').next().unwrap_or("")),
            None if href.starts_with("//") => format!("https:{href}"),
            None => href,
        };
        if !url.starts_with("http") || SKIP.iter().any(|skip| host(&url).contains(skip)) || url.ends_with(".pdf") {
            continue;
        }
        let title = page[close + 1..].split("</a>").next().map(|raw| unescape(&strip(raw))).unwrap_or_default();
        let text = page[close..next]
            .find(snippet)
            .and_then(|from| {
                let from = close + from;
                let start = page[from..next].find('>')? + from + 1;
                let end = ["</a>", "</td>", "</div>"].iter().filter_map(|tag| page[start..next].find(tag)).min().map_or(next, |end| start + end);
                Some(unescape(&strip(&page[start..end])))
            })
            .unwrap_or_default();
        hits.push(Hit { url, title: title.split_whitespace().collect::<Vec<_>>().join(" "), snippet: text.split_whitespace().collect::<Vec<_>>().join(" "), score: 0.0 });
    }
    let total = hits.len().max(1) as f32;
    for (rank, hit) in hits.iter_mut().enumerate() {
        hit.score = 1.0 + (total - rank as f32) / total;
    }
    hits
}

fn key(url: &str) -> String {
    let url = url.split('#').next().unwrap_or(url).trim_end_matches('/');
    url.trim_start_matches("https://").trim_start_matches("http://").trim_start_matches("www.").to_lowercase()
}

fn host(url: &str) -> String {
    let rest = url.split("://").nth(1).unwrap_or(url);
    rest.split(['/', '?', '#']).next().unwrap_or("").trim_start_matches("www.").to_lowercase()
}

fn merge(all: &mut Vec<Hit>, found: Vec<Hit>) {
    for hit in found {
        match all.iter_mut().find(|known| key(&known.url) == key(&hit.url)) {
            Some(known) => {
                known.score += hit.score + 0.5;
                if known.snippet.len() < hit.snippet.len() {
                    known.snippet = hit.snippet;
                }
            }
            None => all.push(hit),
        }
    }
}

fn read_best(app: &AppHandle, hits: &[Hit], seen: &[String], want: usize, (from, to): (f64, f64)) -> Vec<Source> {
    let mut ranked: Vec<&Hit> = hits.iter().filter(|hit| !seen.contains(&key(&hit.url))).collect();
    ranked.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut per_site: HashMap<String, usize> = HashMap::new();
    let picked: Vec<&Hit> = ranked
        .into_iter()
        .filter(|hit| {
            let count = per_site.entry(host(&hit.url)).or_insert(0);
            *count += 1;
            *count <= 2
        })
        .take(if want == PAGES { TRIES } else { want + 3 })
        .collect();
    let done = Mutex::new(0usize);
    step(app, from, "Leyendo las páginas");
    let total = picked.len().max(1);
    let done = &done;
    let pages: Vec<Option<Source>> = std::thread::scope(|scope| {
        let jobs: Vec<_> = picked
            .iter()
            .map(|&hit| {
                scope.spawn(move || {
                    let source = read(hit);
                    let mut count = done.lock().unwrap();
                    *count += 1;
                    step(app, from + (to - from) * *count as f64 / total as f64, &format!("Leyendo páginas ({} de {total})", *count));
                    source
                })
            })
            .collect();
        jobs.into_iter().map(|job| job.join().ok().flatten()).collect()
    });
    pages.into_iter().flatten().take(want).collect()
}

fn read(hit: &Hit) -> Option<Source> {
    if !allowed(&hit.url) {
        return None;
    }
    let got = get(&hit.url, 15).ok()?;
    if got.status != 200 || !(got.kind.is_empty() || got.kind.contains("html") || got.kind.contains("text/plain")) {
        return None;
    }
    let html = text_of(&got.body, &got.kind);
    let (title, text) = page_text(&html);
    if text.chars().count() < ENOUGH_TEXT {
        return None;
    }
    let title = if title.trim().is_empty() { hit.title.clone() } else { title };
    Some(Source { site: host(&got.url), url: got.url, title, text })
}

fn text_of(body: &[u8], kind: &str) -> String {
    let head = String::from_utf8_lossy(&body[..body.len().min(3000)]).to_ascii_lowercase();
    let latin = ["iso-8859-1", "windows-1252", "latin1", "iso-8859-15"].iter().any(|name| kind.contains(name) || head.contains(&format!("charset={name}")) || head.contains(&format!("charset=\"{name}")));
    match std::str::from_utf8(body) {
        Ok(text) => text.to_string(),
        Err(_) if latin => body.iter().map(|&byte| byte as char).collect(),
        Err(_) => {
            let lossy = String::from_utf8_lossy(body);
            if lossy.matches('\u{FFFD}').count() > 30 { body.iter().map(|&byte| byte as char).collect() } else { lossy.into_owned() }
        }
    }
}

fn allowed(url: &str) -> bool {
    let site = host(url);
    let scheme = if url.starts_with("http://") { "http" } else { "https" };
    let known = ROBOTS.lock().unwrap().iter().find(|(name, _)| *name == site).map(|(_, rules)| rules.clone());
    let rules = match known {
        Some(rules) => rules,
        None => {
            let rules = get(&format!("{scheme}://{site}/robots.txt"), 6)
                .ok()
                .filter(|got| got.status == 200 && !got.kind.contains("html"))
                .map(|got| String::from_utf8_lossy(&got.body).into_owned());
            ROBOTS.lock().unwrap().push((site.clone(), rules.clone()));
            rules
        }
    };
    let path = url.split("://").nth(1).and_then(|rest| rest.find('/').map(|at| rest[at..].to_string())).unwrap_or_else(|| "/".into());
    rules.is_none_or(|rules| robots_allow(&rules, &path))
}

fn robots_allow(robots: &str, path: &str) -> bool {
    let mut groups: Vec<(Vec<String>, Vec<(bool, String)>)> = Vec::new();
    let mut taking_agents = false;
    for line in robots.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let Some((field, value)) = line.split_once(':') else { continue };
        let (field, value) = (field.trim().to_ascii_lowercase(), value.trim().to_string());
        match field.as_str() {
            "user-agent" => {
                if !taking_agents {
                    groups.push((Vec::new(), Vec::new()));
                }
                taking_agents = true;
                if let Some(group) = groups.last_mut() {
                    group.0.push(value.to_ascii_lowercase());
                }
            }
            "allow" | "disallow" => {
                taking_agents = false;
                if let Some(group) = groups.last_mut() {
                    if !value.is_empty() {
                        group.1.push((field == "allow", value));
                    }
                }
            }
            _ => taking_agents = false,
        }
    }
    let mine = groups.iter().filter(|(agents, _)| agents.iter().any(|agent| agent.contains(ROBOT))).flat_map(|(_, rules)| rules.iter()).collect::<Vec<_>>();
    let rules = if mine.is_empty() { groups.iter().filter(|(agents, _)| agents.iter().any(|agent| agent == "*")).flat_map(|(_, rules)| rules.iter()).collect() } else { mine };
    let best = rules.iter().filter(|(_, pattern)| matches(pattern, path)).max_by_key(|(allow, pattern)| (pattern.len(), *allow));
    best.is_none_or(|(allow, _)| *allow)
}

fn matches(pattern: &str, path: &str) -> bool {
    let (pattern, anchored) = match pattern.strip_suffix('$') {
        Some(pattern) => (pattern, true),
        None => (pattern, false),
    };
    let pieces: Vec<&str> = pattern.split('*').collect();
    if !path.starts_with(pieces[0]) {
        return false;
    }
    let mut at = pieces[0].len();
    for (index, piece) in pieces.iter().enumerate().skip(1) {
        let last = index == pieces.len() - 1;
        if last && anchored {
            return path.len() >= at + piece.len() && path.ends_with(piece);
        }
        match path[at..].find(piece) {
            Some(found) => at += found + piece.len(),
            None => return false,
        }
    }
    !anchored || at == path.len()
}

const DROP: [&str; 16] = ["script", "style", "noscript", "svg", "template", "iframe", "head", "nav", "footer", "aside", "form", "button", "select", "figure", "dialog", "menu"];
const RAW: [&str; 4] = ["script", "style", "noscript", "template"];
const BLOCKS: [&str; 20] = ["p", "div", "br", "li", "tr", "section", "article", "main", "blockquote", "pre", "table", "ul", "ol", "dd", "dt", "header", "td", "th", "hr", "figcaption"];

fn page_text(html: &str) -> (String, String) {
    let lower = html.to_ascii_lowercase();
    let title = lower.find("<title").and_then(|at| {
        let start = lower[at..].find('>')? + at + 1;
        let end = lower[start..].find("</title")? + start;
        Some(unescape(&unescape(&html[start..end])).split_whitespace().collect::<Vec<_>>().join(" "))
    });
    let mut best = String::new();
    for tag in ["article", "main"] {
        if let (Some(start), Some(end)) = (lower.find(&format!("<{tag}")), lower.rfind(&format!("</{tag}>"))) {
            if end > start {
                let text = flatten(&html[start..end], &lower[start..end]);
                if text.chars().count() >= 1500 {
                    best = text;
                    break;
                }
            }
        }
    }
    if best.is_empty() {
        let start = lower.find("<body").unwrap_or(0);
        best = flatten(&html[start..], &lower[start..]);
    }
    (title.unwrap_or_default(), best)
}

fn flatten(html: &str, lower: &str) -> String {
    let mut out = String::with_capacity(html.len() / 4);
    let mut dropping: Vec<String> = Vec::new();
    let mut at = 0;
    while at < html.len() {
        let Some(open) = html[at..].find('<').map(|found| at + found) else {
            if dropping.is_empty() {
                out.push_str(&html[at..]);
            }
            break;
        };
        if dropping.is_empty() {
            out.push_str(&html[at..open]);
        }
        if lower[open..].starts_with("<!--") {
            at = lower[open..].find("-->").map_or(html.len(), |end| open + end + 3);
            continue;
        }
        let Some(close) = tag_end(html, open) else { break };
        let inside = &lower[open + 1..close];
        let closing = inside.starts_with('/');
        let name: String = inside.trim_start_matches('/').chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
        at = close + 1;
        if name.is_empty() {
            continue;
        }
        if !closing && RAW.contains(&name.as_str()) {
            at = lower[at..].find(&format!("</{name}")).map_or(html.len(), |end| at + end);
            continue;
        }
        if DROP.contains(&name.as_str()) {
            if closing {
                if let Some(last) = dropping.iter().rposition(|open| *open == name) {
                    dropping.truncate(last);
                }
            } else if !inside.ends_with('/') && !matches!(name.as_str(), "hr" | "br") {
                dropping.push(name);
            }
            continue;
        }
        if !dropping.is_empty() {
            continue;
        }
        let heading = matches!(name.as_str(), "h1" | "h2" | "h3" | "h4");
        if heading && !closing {
            out.push_str("\n## ");
        } else if heading || BLOCKS.contains(&name.as_str()) {
            out.push('\n');
            if name == "li" && !closing {
                out.push_str("- ");
            }
        } else if matches!(name.as_str(), "td" | "th" | "span" | "a") {
            out.push(' ');
        }
    }
    tidy(&unescape(&out))
}

fn tag_end(html: &str, open: usize) -> Option<usize> {
    let mut quote = 0u8;
    for (at, &byte) in html.as_bytes()[open..].iter().enumerate() {
        match byte {
            b'"' | b'\'' if quote == 0 && at > 1 && html.as_bytes()[open + at - 1] == b'=' => quote = byte,
            byte if byte == quote => quote = 0,
            b'>' if quote == 0 => return Some(open + at),
            _ => {}
        }
    }
    None
}

fn tidy(text: &str) -> String {
    let mut kept: Vec<String> = Vec::new();
    let mut size = 0;
    for line in text.lines() {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        let words = line.split_whitespace().count();
        let heading = line.starts_with("## ") && (3..=16).contains(&words);
        let item = line.starts_with("- ") && line.chars().count() >= 30;
        let sentence = line.chars().count() >= 60 || (words >= 6 && line.ends_with(['.', ':', '?', '!', '»', '"']));
        if !(heading || item || sentence) || kept.iter().any(|seen| *seen == line) {
            continue;
        }
        if heading && kept.last().is_some_and(|last| last.starts_with("## ")) {
            kept.pop();
        }
        size += line.chars().count() + 1;
        kept.push(line);
        if size >= PAGE_TEXT {
            break;
        }
    }
    if kept.last().is_some_and(|last| last.starts_with("## ")) {
        kept.pop();
    }
    let joined = kept.join("\n");
    match joined.char_indices().nth(PAGE_TEXT) {
        Some((cut, _)) => joined[..cut].to_string(),
        None => joined,
    }
}

fn floor(text: &str, mut at: usize) -> usize {
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

fn strip(html: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for c in html.chars() {
        match c {
            '<' => inside = true,
            '>' => inside = false,
            c if !inside => out.push(c),
            _ => {}
        }
    }
    out
}

const NAMED: [(&str, &str); 44] = [
    ("nbsp", " "), ("amp", "&"), ("lt", "<"), ("gt", ">"), ("quot", "\""), ("apos", "'"), ("aacute", "á"), ("eacute", "é"), ("iacute", "í"),
    ("oacute", "ó"), ("uacute", "ú"), ("Aacute", "Á"), ("Eacute", "É"), ("Iacute", "Í"), ("Oacute", "Ó"), ("Uacute", "Ú"), ("ntilde", "ñ"),
    ("Ntilde", "Ñ"), ("uuml", "ü"), ("ccedil", "ç"), ("atilde", "ã"), ("otilde", "õ"), ("acirc", "â"), ("ecirc", "ê"), ("ocirc", "ô"),
    ("agrave", "à"), ("iexcl", "¡"), ("iquest", "¿"), ("laquo", "«"), ("raquo", "»"), ("hellip", "…"), ("mdash", "—"), ("ndash", "–"),
    ("rsquo", "’"), ("lsquo", "‘"), ("rdquo", "”"), ("ldquo", "“"), ("copy", "©"), ("reg", "®"), ("euro", "€"), ("deg", "°"),
    ("middot", "·"), ("bull", "•"), ("times", "×"),
];

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let end = rest[..floor(rest, rest.len().min(12))].find(';');
        let replaced = end.and_then(|end| {
            let name = &rest[1..end];
            let found = if let Some(number) = name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")) {
                u32::from_str_radix(number, 16).ok().and_then(char::from_u32).map(String::from)
            } else if let Some(number) = name.strip_prefix('#') {
                number.parse::<u32>().ok().and_then(char::from_u32).map(String::from)
            } else {
                NAMED.iter().find(|(known, _)| *known == name).map(|(_, value)| value.to_string())
            };
            found.map(|value| (value, end + 1))
        });
        match replaced {
            Some((value, used)) => {
                out.push_str(&value);
                rest = &rest[used..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn take_notes(app: &AppHandle, topic: &str, aspects: &[String], sources: &[Source], first: usize, tongue: &str) -> Vec<String> {
    let system = format!(
        "Eres un investigador minucioso. Te paso páginas web numeradas que salieron al investigar un tema. De cada una saca, en {tongue}, todo lo que \
         sirve para responderlo: hechos, cifras con sus unidades, fechas, nombres, comparaciones, y las opiniones marcadas como opinión. Di qué tipo de \
         fuente es (oficial, estudio, prensa, blog, tienda, foro, enciclopedia) y su fecha si aparece. Copia los datos tal cual; no inventes nada que no \
         esté en el texto y no mezcles una página con otra. Si una página no sirve, escribe solo que no sirve.\n\
         Formato exacto, una página tras otra:\n[n] tipo · fecha\n- dato\n- dato"
    );
    let aspects = if aspects.is_empty() { String::new() } else { format!("\nLo que hay que responder: {}.", aspects.join("; ")) };
    let groups: Vec<(usize, &[Source])> = sources.chunks(2).enumerate().map(|(index, chunk)| (first + index * 2, chunk)).collect();
    std::thread::scope(|scope| {
        let jobs: Vec<_> = groups
            .iter()
            .map(|(start, chunk)| {
                let system = &system;
                let aspects = &aspects;
                scope.spawn(move || {
                    let pages: String = chunk
                        .iter()
                        .enumerate()
                        .map(|(offset, source)| format!("\n\n=== [{}] {} ({})\n{}", start + offset + 1, source.title, source.site, source.text))
                        .collect();
                    let said = format!("Tema: {topic}{aspects}{pages}");
                    let notes = crate::ai::ask(app, system, &said, 1800).unwrap_or_default();
                    if notes.trim().is_empty() {
                        chunk.iter().enumerate().map(|(offset, source)| format!("[{}] {}\n- {}", start + offset + 1, source.title, source.text.chars().take(900).collect::<String>())).collect::<Vec<_>>().join("\n\n")
                    } else {
                        notes.trim().to_string()
                    }
                })
            })
            .collect();
        jobs.into_iter().filter_map(|job| job.join().ok()).collect()
    })
}

fn gaps(app: &AppHandle, topic: &str, aspects: &[String], notes: &[String], tongue: &str) -> Vec<String> {
    let system = format!(
        "Eres un investigador revisando sus notas. Mira si con ellas se puede responder bien el tema y cada aspecto. Si falta algo importante (un dato \
         clave, la otra versión de una contradicción, algo más reciente), escribe hasta 2 búsquedas web cortas para conseguirlo, una por línea, en {tongue} \
         o en inglés, sin numerar ni comillas. Si no falta nada, escribe solo: NADA"
    );
    let said = format!("Tema: {topic}\nAspectos: {}\n\nNotas:\n{}", aspects.join("; "), notes.join("\n\n"));
    let Ok(reply) = crate::ai::ask(app, &system, &said, 200) else { return Vec::new() };
    if crate::intent::normalize(&reply).trim().starts_with("nada") || reply.trim().is_empty() {
        return Vec::new();
    }
    reply
        .lines()
        .map(|line| line.trim().trim_start_matches(['-', '*', '•', '1', '2', '.', ')', ' ']).trim_matches(['"', '«', '»', '“', '”']).trim().to_string())
        .filter(|line| (2..=12).contains(&line.split_whitespace().count()) && !crate::intent::normalize(line).starts_with("nada"))
        .take(2)
        .collect()
}

fn write(app: &AppHandle, topic: &str, plan: &Plan, sources: &[Source], notes: &[String], tongue: &str) -> Result<Value, String> {
    let now = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    let system = format!(
        "Eres un investigador serio y escribes informes a fondo. Hoy es {:02}/{:02}/{}. Con las notas de las fuentes numeradas, escribe el informe \
         sobre el tema en {tongue}, claro y directo, para alguien listo que no es experto.\n\
         Reglas:\n\
         - Cada dato lleva su fuente entre corchetes al final de la frase: [2] o [1][4]. Solo números de fuentes que existen.\n\
         - Nada que no esté en las notas. Si lo deduces tú, dilo («parece», «se deduce»).\n\
         - Cruza las fuentes: si coinciden, dilo; si se contradicen, ponlo en «contradicciones» con quién dice qué y cuál parece más creíble y por qué \
           (fecha, tipo de fuente, si es oficial o un estudio). Separa hechos de opiniones.\n\
         - Las secciones van de lo más importante a lo menos; cada una con 2 a 4 párrafos, separados por una línea en blanco; puedes usar listas con «- ».\n\
         - Las conclusiones responden la pregunta de frente, con la seguridad que de verdad dan las fuentes.\n\
         Responde SOLO con JSON:\n\
         {{\"titulo\": \"...\", \"resumen\": \"3 a 5 frases que respondan lo principal, con fuentes\", \"puntos\": [\"punto clave con fuente\"], \
         \"secciones\": [{{\"titulo\": \"...\", \"texto\": \"...\"}}], \"contradicciones\": [{{\"tema\": \"...\", \"detalle\": \"[2] dice … y [5] dice …\", \
         \"veredicto\": \"cuál creer y por qué\"}}], \"conclusiones\": [\"...\"], \"limites\": \"lo que no se pudo confirmar o falta\", \
         \"dicho\": \"una o dos frases para decirle en voz alta lo principal, sin números de fuente\"}}",
        now.wDay, now.wMonth, now.wYear
    );
    let list: String = sources.iter().enumerate().map(|(index, source)| format!("[{}] {} — {}\n", index + 1, source.title, source.site)).collect();
    let said = format!(
        "Tema: {topic}\nTítulo pensado: {}\nAspectos a responder: {}\n\nFuentes:\n{list}\nNotas:\n{}",
        plan.title,
        plan.aspects.join("; "),
        notes.join("\n\n")
    );
    let reply = crate::ai::ask(app, &system, &said, 8000)?;
    if let Some(report) = json_in(&reply).filter(|report| report.get("secciones").is_some() || report.get("resumen").is_some()) {
        return Ok(report);
    }
    if reply.trim().len() < 200 {
        return Err("La IA no me devolvió el informe. Prueba otra vez en un rato.".into());
    }
    Ok(json!({ "titulo": plan.title, "resumen": "", "secciones": [{ "titulo": plan.title, "texto": reply.trim() }] }))
}

fn esc(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn cited(text: &str, sources: &[Source]) -> String {
    let safe = esc(text.trim());
    let mut out = String::with_capacity(safe.len());
    let mut rest = safe.as_str();
    while let Some(at) = rest.find('[') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let close = rest.find(']').filter(|close| *close <= 12);
        let numbers: Option<Vec<usize>> = close.and_then(|close| rest[1..close].split([',', ' ']).filter(|piece| !piece.is_empty()).map(|piece| piece.parse().ok()).collect());
        match (close, numbers) {
            (Some(close), Some(numbers)) if !numbers.is_empty() && numbers.iter().all(|n| (1..=sources.len()).contains(n)) => {
                for n in numbers {
                    out.push_str(&format!("<a class=\"cite\" href=\"#f{n}\" title=\"{}\">{n}</a>", esc(&sources[n - 1].title)));
                }
                rest = &rest[close + 1..];
            }
            _ => {
                out.push('[');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn blocks(text: &str, sources: &[Source]) -> String {
    let mut out = String::new();
    for block in text.split("\n\n").map(str::trim).filter(|block| !block.is_empty()) {
        let lines: Vec<&str> = block.lines().map(str::trim).filter(|line| !line.is_empty()).collect();
        if lines.iter().all(|line| line.starts_with("- ") || line.starts_with("* ") || line.starts_with("• ")) {
            out.push_str("<ul>");
            for line in lines {
                out.push_str(&format!("<li>{}</li>", cited(&line[line.char_indices().nth(2).map_or(line.len(), |(at, _)| at)..], sources)));
            }
            out.push_str("</ul>");
        } else {
            out.push_str(&format!("<p>{}</p>", lines.iter().map(|line| cited(line, sources)).collect::<Vec<_>>().join("<br>")));
        }
    }
    out
}

fn words(language: &str) -> [&'static str; 14] {
    match language {
        "en" => ["Research", "Summary", "Key points", "Where the sources disagree", "Verdict", "Conclusions", "What couldn't be confirmed", "Sources", "Searches", "sources read", "AI calls", "searches", "Made by Cocuyo with your own key. Check the sources before relying on it.", "DuckDuckGo asked for a break and these were left out"],
        "pt" => ["Pesquisa", "Resumo", "Pontos-chave", "Onde as fontes não batem", "Veredito", "Conclusões", "O que não deu para confirmar", "Fontes", "Buscas", "fontes lidas", "chamadas à IA", "buscas", "Feito pelo Cocuyo com a sua chave. Confira as fontes antes de confiar.", "O DuckDuckGo pediu uma pausa e estas ficaram de fora"],
        _ => ["Investigación", "Resumen", "Puntos clave", "Donde las fuentes no coinciden", "Qué creer", "Conclusiones", "Lo que no se pudo confirmar", "Fuentes", "Búsquedas", "fuentes leídas", "llamadas a la IA", "búsquedas", "Hecho por Cocuyo con tu propia clave. Revisa las fuentes antes de fiarte.", "DuckDuckGo pidió una pausa y estas quedaron por fuera"],
    }
}

fn render(topic: &str, title: &str, report: &Value, sources: &[Source], (searched, missed): (&[String], &[String]), calls: usize, took: Duration) -> String {
    let (language, _) = language();
    let w = words(language);
    let now = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    let date = format!("{:02}/{:02}/{} {:02}:{:02}", now.wDay, now.wMonth, now.wYear, now.wHour, now.wMinute);
    let mut body = String::new();
    let summary = report["resumen"].as_str().unwrap_or("");
    if !summary.trim().is_empty() {
        body.push_str(&format!("<section class=\"card lead\"><h2>{}</h2>{}</section>", w[1], blocks(summary, sources)));
    }
    let points = strings(&report["puntos"], 12);
    if !points.is_empty() {
        body.push_str(&format!("<section><h2>{}</h2><ul class=\"points\">{}</ul></section>", w[2], points.iter().map(|point| format!("<li>{}</li>", cited(point, sources))).collect::<String>()));
    }
    for section in report["secciones"].as_array().into_iter().flatten() {
        let heading = section["titulo"].as_str().unwrap_or("");
        let text = section["texto"].as_str().unwrap_or("");
        if text.trim().is_empty() {
            continue;
        }
        body.push_str(&format!("<section><h2>{}</h2>{}</section>", esc(heading), blocks(text, sources)));
    }
    let clashes: Vec<&Value> = report["contradicciones"].as_array().into_iter().flatten().filter(|clash| clash["detalle"].as_str().is_some_and(|text| !text.trim().is_empty())).collect();
    if !clashes.is_empty() {
        body.push_str(&format!("<section><h2>{}</h2>", w[3]));
        for clash in clashes {
            body.push_str(&format!(
                "<div class=\"card clash\"><h3>{}</h3>{}<p class=\"verdict\"><b>{}:</b> {}</p></div>",
                esc(clash["tema"].as_str().unwrap_or("")),
                blocks(clash["detalle"].as_str().unwrap_or(""), sources),
                w[4],
                cited(clash["veredicto"].as_str().unwrap_or(""), sources)
            ));
        }
        body.push_str("</section>");
    }
    let ends = strings(&report["conclusiones"], 10);
    if !ends.is_empty() {
        body.push_str(&format!("<section class=\"card\"><h2>{}</h2><ul class=\"points\">{}</ul></section>", w[5], ends.iter().map(|end| format!("<li>{}</li>", cited(end, sources))).collect::<String>()));
    }
    let limits = report["limites"].as_str().unwrap_or("");
    if !limits.trim().is_empty() {
        body.push_str(&format!("<section><h2>{}</h2>{}</section>", w[6], blocks(limits, sources)));
    }
    let list: String = sources
        .iter()
        .enumerate()
        .map(|(index, source)| format!("<li id=\"f{n}\"><span class=\"n\">{n}</span><div><a href=\"{url}\">{title}</a><small>{site}</small></div></li>", n = index + 1, url = esc(&source.url), title = esc(&source.title), site = esc(&source.site)))
        .collect();
    body.push_str(&format!("<section><h2>{}</h2><ol class=\"sources\">{list}</ol></section>", w[7]));
    let quoted = |queries: &[String]| queries.iter().map(|query| format!("«{}»", esc(query))).collect::<Vec<_>>().join(" · ");
    let missed = if missed.is_empty() { String::new() } else { format!("<p>{}: {}</p>", w[13], quoted(missed)) };
    body.push_str(&format!("<section class=\"quiet\"><h2>{}</h2><p>{}</p>{missed}</section>", w[8], quoted(searched)));
    let minutes = took.as_secs() / 60;
    let seconds = took.as_secs() % 60;
    format!(
        "<!doctype html><html lang=\"{language}\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <title>{title_text}</title><style>{STYLE}</style></head><body><main>\
         <header><p class=\"kicker\">Cocuyo · {kind}</p><h1>{title_text}</h1><p class=\"ask\">{topic_text}</p>\
         <div class=\"chips\"><span>{date}</span><span>{sources_count} {w9}</span><span>{searches} {w11}</span><span>{calls} {w10}</span><span>{minutes}:{seconds:02}</span></div></header>\
         {body}<footer>{w12}</footer></main></body></html>",
        title_text = esc(title),
        topic_text = esc(topic),
        kind = w[0],
        sources_count = sources.len(),
        searches = searched.len(),
        w9 = w[9],
        w10 = w[10],
        w11 = w[11],
        w12 = w[12],
    )
}

const STYLE: &str = ":root{color-scheme:dark;--bg:#000;--card:#0f0f11;--line:#1d1d21;--ink:rgb(244 245 248);--gray:rgb(140 135 145);--blue:#5cb8ff;--gold:#f2b84b}\
*{box-sizing:border-box}html{background:var(--bg)}body{margin:0;background:var(--bg);color:var(--ink);font:16px/1.65 'Segoe UI',system-ui,sans-serif;-webkit-font-smoothing:antialiased}\
main{max-width:760px;margin:0 auto;padding:56px 20px 80px}header{margin-bottom:36px}.kicker{margin:0 0 10px;color:var(--gold);font-size:13px;font-weight:600;letter-spacing:.06em;text-transform:uppercase}\
h1{margin:0;font-size:clamp(28px,5vw,40px);line-height:1.15;font-weight:700;letter-spacing:-.01em;text-wrap:balance}.ask{margin:14px 0 0;color:var(--gray);font-size:17px}\
.chips{display:flex;flex-wrap:wrap;gap:8px;margin-top:20px}.chips span{padding:5px 12px;border-radius:999px;background:var(--card);border:1px solid var(--line);color:var(--gray);font-size:13px;font-variant-numeric:tabular-nums}\
section{margin:0 0 34px}h2{margin:0 0 12px;font-size:21px;line-height:1.3;font-weight:650}h3{margin:0 0 8px;font-size:16px;font-weight:650}p{margin:0 0 14px}\
ul{margin:0 0 14px;padding-left:22px}li{margin:0 0 8px}li::marker{color:var(--gold)}.card{background:var(--card);border:1px solid var(--line);border-radius:20px;padding:22px 24px}\
.card>:last-child,.card p:last-child{margin-bottom:0}.lead{border-color:#2c2614}.lead h2{color:var(--gold)}.clash{margin-bottom:12px}.verdict{color:var(--ink)}.verdict b{color:var(--gold)}\
a{color:var(--blue);text-decoration:none}a:hover{text-decoration:underline}.cite{display:inline-block;min-width:18px;margin-left:2px;padding:0 5px;border-radius:6px;background:#10263a;color:var(--blue);font-size:11.5px;line-height:18px;text-align:center;vertical-align:2px;font-variant-numeric:tabular-nums}\
.cite:hover{text-decoration:none;background:#16344f}.sources{list-style:none;padding:0;margin:0}.sources li{display:flex;gap:12px;align-items:flex-start;padding:10px 0;border-top:1px solid var(--line);margin:0}\
.sources li:target{background:#10263a;border-radius:12px}.sources .n{flex:none;width:26px;height:26px;border-radius:8px;background:var(--card);border:1px solid var(--line);color:var(--gold);font-size:13px;line-height:24px;text-align:center}\
.sources div{min-width:0}.sources a{overflow-wrap:anywhere}.sources small{display:block;color:var(--gray);font-size:13px}.quiet p{color:var(--gray);font-size:14px}\
footer{margin-top:48px;padding-top:18px;border-top:1px solid var(--line);color:var(--gray);font-size:13px}\
@media print{:root{--bg:#fff;--card:#fff;--line:#ddd;--ink:#111;--gray:#555;--blue:#0b5394;--gold:#8a5a00}.cite{background:none}}";

fn save(app: &AppHandle, title: &str, page: &str) -> Result<PathBuf, String> {
    let dir = app.path().document_dir().map_err(|e| e.to_string())?.join("Cocuyo").join("Investigaciones");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let now = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    let name: String = title.chars().map(|c| if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | ',') { c } else { ' ' }).collect();
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    let name: String = name.chars().take(80).collect();
    let path = dir.join(format!("{:04}-{:02}-{:02} {:02}-{:02} {}.html", now.wYear, now.wMonth, now.wDay, now.wHour, now.wMinute, name.trim()));
    std::fs::write(&path, page).map_err(|e| e.to_string())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_topic_comes_out_of_the_order() {
        assert_eq!(topic("Investiga a fondo las baterías de estado sólido").as_deref(), Some("las baterías de estado sólido"));
        assert_eq!(topic("Cocuyo, investiga a fondo sobre el cambio climático en Colombia").as_deref(), Some("el cambio climático en Colombia"));
        assert_eq!(topic("jarvis haz una investigación sobre los mejores SSD de 2026").as_deref(), Some("los mejores SSD de 2026"));
        assert_eq!(topic("research the best budget GPUs").as_deref(), Some("the best budget GPUs"));
        assert_eq!(topic("pesquise sobre carros elétricos baratos").as_deref(), Some("carros elétricos baratos"));
        assert_eq!(topic("investiga esto"), None);
        assert_eq!(topic("pon música"), None);
    }

    #[test]
    fn duckduckgo_results_are_read() {
        let page = r#"<div class="result results_links"><h2 class="result__title"><a rel="nofollow" class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fes.wikipedia.org%2Fwiki%2FBater%C3%ADa&amp;rut=abc">Batería &amp; más</a></h2>
            <a class="result__snippet" href="x">Una <b>batería</b> de estado sólido.</a></div>
            <div class="result result--ad"><a class="result__a" href="https://duckduckgo.com/y.js?ad=1">Anuncio</a></div>
            <div class="result"><a class="result__a" href="https://www.youtube.com/watch?v=1">Video</a></div>
            <div class="result"><a class="result__a" href="https://ejemplo.com/a">Otra</a><a class="result__snippet" href="y">Texto dos</a></div>"#;
        let hits = results(page, "class=\"result__a\"", "result__snippet");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].url, "https://es.wikipedia.org/wiki/Batería");
        assert_eq!(hits[0].title, "Batería & más");
        assert_eq!(hits[0].snippet, "Una batería de estado sólido.");
        assert_eq!(hits[1].snippet, "Texto dos");
        let lite = "<a rel=\"nofollow\" href=\"//duckduckgo.com/l/?uddg=https%3A%2F%2Fen.wikipedia.org%2Fwiki%2FSolid%2Dstate&amp;rut=1\" class='result-link'>Solid-state</a></td></tr><tr><td class='result-snippet'>A <b>solid</b> battery</td>";
        let hits = results(lite, "class='result-link'", "result-snippet");
        assert_eq!(hits[0].url, "https://en.wikipedia.org/wiki/Solid-state");
        assert_eq!(hits[0].snippet, "A solid battery");
    }

    #[test]
    fn robots_rules_are_respected() {
        let robots = "User-agent: Googlebot\nDisallow: /\n\nUser-agent: *\nDisallow: /privado/\nDisallow: /*.pdf$\nAllow: /privado/abierto\n";
        assert!(robots_allow(robots, "/articulo"));
        assert!(!robots_allow(robots, "/privado/x"));
        assert!(robots_allow(robots, "/privado/abierto/y"));
        assert!(!robots_allow(robots, "/docs/a.pdf"));
        assert!(robots_allow(robots, "/docs/a.pdf?x=1"));
        assert!(!robots_allow("User-agent: cocuyo\nDisallow: /\n\nUser-agent: *\nAllow: /\n", "/x"));
        assert!(robots_allow("User-agent: *\nDisallow:\n", "/x"));
        assert!(!robots_allow("User-agent: *\nDisallow: /\n", "/x"));
    }

    #[test]
    fn the_main_text_is_pulled_out_of_the_page() {
        let html = "<html><head><title>Baterías &amp; futuro</title><style>p{}</style></head><body><nav><a>Inicio</a><a>Contacto</a></nav>\
            <header><div>Menú</div></header><article><h2>Qué son</h2><p>Las baterías de estado sólido usan un electrolito sólido en vez de líquido, y eso cambia todo.</p>\
            <script>var x = '<p>no</p>';</script><div class=\"[&>*]:pointer-events-auto\" data-x='a>b'><p>Un párrafo con atributos raros que no debe dejar basura en el texto.</p></div><ul><li>Más densidad de energía por kilo que las de litio de hoy.</li><li>corto</li></ul>\
            <p>Toyota dijo que las tendrá en 2027 o 2028, aunque otros fabricantes son más prudentes con las fechas.</p></article>\
            <footer><p>Todos los derechos reservados a esta página tan larga que aburre a cualquiera.</p></footer></body></html>";
        let (title, text) = page_text(html);
        assert_eq!(title, "Baterías & futuro");
        assert!(text.contains("## Qué son"));
        assert!(text.contains("electrolito sólido en vez de líquido"));
        assert!(text.contains("- Más densidad de energía"));
        assert!(text.contains("Toyota dijo"));
        assert!(text.contains("Un párrafo con atributos raros") && !text.contains("pointer-events"));
        assert!(!text.contains("no</p>") && !text.contains("Contacto") && !text.contains("derechos reservados") && !text.contains("corto"));
    }

    #[test]
    fn entities_and_addresses_come_back_clean() {
        assert_eq!(unescape("a&nbsp;b &amp; c &#233; &#xE9; &raquo; &foo; & d"), "a b & c é é » &foo; & d");
        assert_eq!(encode("baterías de estado"), "bater%C3%ADas+de+estado");
        assert_eq!(decode("https%3A%2F%2Fa.com%2Fb%C3%AD"), "https://a.com/bí");
        assert_eq!(host("https://www.Ejemplo.com/a?b"), "ejemplo.com");
        assert_eq!(key("https://www.a.com/b/#x"), "a.com/b");
    }

    #[test]
    fn citations_turn_into_links() {
        let sources = vec![
            Source { url: "https://a.com".into(), title: "A".into(), site: "a.com".into(), text: String::new() },
            Source { url: "https://b.com".into(), title: "B <b>".into(), site: "b.com".into(), text: String::new() },
        ];
        let html = cited("Dato <x> [1][2] y [3] y [1, 2] y [nota]", &sources);
        assert!(html.starts_with("Dato &lt;x&gt; <a class=\"cite\" href=\"#f1\" title=\"A\">1</a><a class=\"cite\" href=\"#f2\" title=\"B &lt;b&gt;\">2</a>"));
        assert!(html.contains(" y [3] y "));
        assert!(html.contains("href=\"#f2\" title=\"B &lt;b&gt;\">2</a> y [nota]"));
        let list = blocks("- uno [1]\n- dos\n\nPárrafo\nseguido", &sources);
        assert!(list.starts_with("<ul><li>uno <a"));
        assert!(list.ends_with("<p>Párrafo<br>seguido</p>"));
    }

    #[test]
    #[ignore]
    fn busqueda_de_verdad() {
        let mut hits = Vec::new();
        for query in ["baterías de estado sólido 2026", "solid state battery production timeline"] {
            match search(query) {
                Searched::Found(found) => merge(&mut hits, found),
                Searched::Nothing => panic!("nada para {query}"),
                Searched::Stopped => panic!("duckduckgo pidió pausa"),
            }
        }
        hits.sort_by(|a, b| b.score.total_cmp(&a.score));
        let started = Instant::now();
        let pages: Vec<Option<Source>> = std::thread::scope(|scope| {
            let jobs: Vec<_> = hits.iter().take(TRIES).map(|hit| scope.spawn(move || read(hit))).collect();
            jobs.into_iter().map(|job| job.join().ok().flatten()).collect()
        });
        for (hit, page) in hits.iter().zip(&pages) {
            println!("{:5.2} {} -> {}", hit.score, hit.url, page.as_ref().map_or("no".to_string(), |page| format!("{} letras «{}»", page.text.chars().count(), page.title)));
        }
        println!("{:?}", started.elapsed());
        if let Some(page) = pages.iter().flatten().next() {
            println!("{}", page.text.chars().take(1500).collect::<String>());
        }
        assert!(pages.iter().flatten().count() >= 4);
    }

    #[test]
    fn a_report_renders_with_its_sources() {
        let sources = vec![Source { url: "https://a.com/x".into(), title: "Fuente A".into(), site: "a.com".into(), text: String::new() }];
        let report = json!({ "resumen": "Todo bien [1].", "puntos": ["Uno [1]"], "secciones": [{ "titulo": "Parte", "texto": "Texto [1]." }],
            "contradicciones": [{ "tema": "Fechas", "detalle": "[1] dice 2027", "veredicto": "Creerle a [1]" }], "conclusiones": ["Sí [1]"], "limites": "Poco." });
        let page = render("¿Sirve?", "Informe", &report, &sources, (&["a b".into()], &["c d".into()]), 9, Duration::from_secs(75));
        assert!(page.contains("quedaron por fuera: «c d»"));
        assert!(page.contains("<h1>Informe</h1>") && page.contains("id=\"f1\"") && page.contains("9 ") && page.contains("1:15"));
        assert!(page.matches("href=\"#f1\"").count() >= 6);
    }
}
