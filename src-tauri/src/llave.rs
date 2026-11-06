// la llave de acceso de whatsapp: la firma solo sale de la pagina real de whatsapp web, asi que abro chrome o edge con un perfil
// de usar y botar, tu confirmas con tu telefono y la firma vuelve a gowa sin guardarse en ninguna parte

use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

use crate::apps::CREATE_NO_WINDOW;
use crate::talk::Socket;

const ORIGIN: &str = "https://web.whatsapp.com";
const PROFILE: &str = "Cocuyo-Llave-";

static SIGNING: AtomicBool = AtomicBool::new(false);
static CANCEL: AtomicBool = AtomicBool::new(false);

pub fn cancel() {
    CANCEL.store(true, Ordering::Relaxed);
}

struct Signing;

impl Drop for Signing {
    fn drop(&mut self) {
        SIGNING.store(false, Ordering::Relaxed);
    }
}

pub fn browser() -> Option<PathBuf> {
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    const APP_PATHS: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths\\";
    for exe in ["chrome.exe", "msedge.exe"] {
        for root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
            if let Some(path) = crate::apps::reg_string(root, &format!("{APP_PATHS}{exe}"), "").map(PathBuf::from).filter(|path| path.is_file()) {
                return Some(path);
            }
        }
    }
    let places = [
        ("PROGRAMFILES", "Google\\Chrome\\Application\\chrome.exe"),
        ("PROGRAMFILES(X86)", "Google\\Chrome\\Application\\chrome.exe"),
        ("LOCALAPPDATA", "Google\\Chrome\\Application\\chrome.exe"),
        ("PROGRAMFILES(X86)", "Microsoft\\Edge\\Application\\msedge.exe"),
        ("PROGRAMFILES", "Microsoft\\Edge\\Application\\msedge.exe"),
    ];
    places.iter().filter_map(|(base, rest)| std::env::var_os(base).map(|dir| PathBuf::from(dir).join(rest))).find(|path| path.is_file())
}

fn port_of(active: &str) -> Option<u16> {
    active.lines().next()?.trim().parse().ok().filter(|port| *port != 0)
}

fn page_path(list: &Value) -> Option<String> {
    let page = list.as_array()?.iter().find(|target| target["type"] == "page" && target["url"].as_str().is_some_and(|url| url.strip_prefix(ORIGIN).is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))))?;
    let socket = page["webSocketDebuggerUrl"].as_str()?;
    let after = socket.strip_prefix("ws://")?;
    Some(after[after.find('/')?..].to_string())
}

// gowa manda null lo que whatsapp no trajo y chrome se niega a leerlo
fn without_nulls(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(map.iter().filter(|(_, inner)| !inner.is_null()).map(|(key, inner)| (key.clone(), without_nulls(inner))).collect()),
        Value::Array(list) => Value::Array(list.iter().map(without_nulls).collect()),
        other => other.clone(),
    }
}

fn script(options: &Value, note: &str, click: &str, limit: Duration) -> String {
    let mut options = without_nulls(options);
    options["timeout"] = json!(limit.as_millis() as u64);
    let note = Value::from(note);
    let click = Value::from(click);
    let wait = limit.as_millis() as u64;
    format!(
        r#"(async () => {{
  const note = document.createElement("div");
  note.style.cssText = "position:fixed;inset:0;z-index:2147483647;display:flex;align-items:center;justify-content:center;padding:48px;text-align:center;background:#0b0b0b;color:#fff;font:600 20px/1.5 system-ui";
  document.documentElement.append(note);
  const until = Date.now() + {wait};
  while (!document.hasFocus() && Date.now() < until) {{
    note.textContent = {click};
    await new Promise((done) => setTimeout(done, 200));
  }}
  note.textContent = {note};
  const stop = new AbortController();
  setTimeout(() => stop.abort(), Math.max(0, until - Date.now()));
  try {{
    const opts = PublicKeyCredential.parseRequestOptionsFromJSON({options});
    const cred = await navigator.credentials.get({{ publicKey: opts, signal: stop.signal }});
    return JSON.stringify({{ ok: cred.toJSON() }});
  }} catch (e) {{
    return JSON.stringify({{ error: e.name, detail: e.message }});
  }}
}})()"#
    )
}

fn answer_of(said: &str) -> Result<Value, String> {
    let said: Value = serde_json::from_str(said).map_err(|_| crate::i18n::tr("El navegador no devolvió la firma."))?;
    if said["ok"].is_object() {
        return Ok(said["ok"].clone());
    }
    let name = said["error"].as_str().unwrap_or("");
    let detail = said["detail"].as_str().unwrap_or("");
    Err(match name {
        "NotAllowedError" if detail.contains("focus") => crate::i18n::tr("La ventana del navegador no quedó al frente."),
        "NotAllowedError" | "AbortError" => crate::i18n::tr("Cancelaste la llave o se acabó el tiempo."),
        "TypeError" if detail.contains("not a function") => crate::i18n::tr("Tu Chrome o Edge está muy viejo para la llave de acceso: actualízalo."),
        _ => crate::i18n::tr(&format!("El navegador no pudo usar tu llave de acceso ({name}). Deja el Bluetooth prendido en el PC y en el celular.")),
    })
}

struct Cdp {
    socket: Socket,
    next: u64,
}

impl Cdp {
    fn ask(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.next += 1;
        self.socket.send(&json!({ "id": self.next, "method": method, "params": params }))?;
        loop {
            let said = self.socket.receive()?;
            if said["id"] == self.next {
                return Ok(said);
            }
        }
    }

    fn evaluate(&mut self, expression: &str, wait: bool) -> Result<Value, String> {
        let said = self.ask("Runtime.evaluate", json!({ "expression": expression, "awaitPromise": wait, "returnByValue": true, "userGesture": true }))?;
        Ok(said["result"]["result"]["value"].clone())
    }

    fn page_ready(&mut self) -> bool {
        wait_for(Duration::from_secs(30), || {
            let Ok(at) = self.evaluate("location.origin + ' ' + document.readyState", false) else { return Some(false) };
            let at = at.as_str()?;
            (at.strip_prefix(ORIGIN).is_some_and(|rest| rest.starts_with(' ')) && !at.ends_with("loading")).then_some(true)
        })
        .unwrap_or(false)
    }
}

fn wait_for<T>(limit: Duration, mut found: impl FnMut() -> Option<T>) -> Option<T> {
    let started = Instant::now();
    while started.elapsed() < limit {
        if let Some(value) = found() {
            return Some(value);
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    None
}

fn bring_forward(pid: u32) {
    wait_for(Duration::from_secs(5), || unsafe {
        let window = crate::pc::app_windows().into_iter().find(|hwnd| {
            let mut owner = 0;
            GetWindowThreadProcessId(*hwnd, Some(&mut owner));
            owner == pid
        })?;
        crate::pc::bring(window);
        Some(())
    });
}

fn run(child: &Arc<Mutex<Child>>, pid: u32, profile: &std::path::Path, options: &Value, limit: Duration) -> Result<Value, String> {
    let started = Instant::now();
    let gone = || child.lock().unwrap().try_wait().ok().flatten().is_some();
    let lost = || crate::i18n::tr(if started.elapsed() >= limit || CANCEL.load(Ordering::Relaxed) { "Cancelaste la llave o se acabó el tiempo." } else { "El navegador se cerró antes de terminar." });
    let no_page = || crate::i18n::tr("No pude abrir WhatsApp Web en el navegador. ¿Hay internet?");
    let port = wait_for(Duration::from_secs(25), || if gone() { Some(None) } else { std::fs::read_to_string(profile.join("DevToolsActivePort")).ok().and_then(|text| port_of(&text)).map(Some) }).flatten().ok_or_else(lost)?;
    let path = wait_for(Duration::from_secs(25), || {
        if gone() {
            return Some(None);
        }
        let out = Command::new("curl.exe").args(["-s", "--max-time", "3"]).arg(format!("http://127.0.0.1:{port}/json")).creation_flags(CREATE_NO_WINDOW).stderr(Stdio::null()).output().ok()?;
        page_path(&serde_json::from_slice(&out.stdout).ok()?).map(Some)
    })
    .flatten()
    .ok_or_else(|| if gone() { lost() } else { no_page() })?;
    let socket = Socket::connect("127.0.0.1", port, &path, "", false).map_err(|_| crate::i18n::tr("No pude hablar con el navegador."))?;
    let mut cdp = Cdp { socket, next: 0 };
    let note = crate::i18n::tr("Elige «Usar un teléfono o una tablet», escanea el código con la cámara del celular y confirma con tu huella. Deja el Bluetooth prendido en los dos. Después toca la notificación de WhatsApp que te llega al celular, algo como «¿Estás intentando iniciar sesión?». Cocuyo cierra esta ventana al terminar.");
    let click = crate::i18n::tr("Haz clic en esta ventana para seguir.");
    for _ in 0..3 {
        if !cdp.page_ready() {
            return Err(if gone() { lost() } else { no_page() });
        }
        let _ = cdp.ask("Page.bringToFront", json!({}));
        bring_forward(pid);
        let left = limit.saturating_sub(started.elapsed());
        let said = cdp.evaluate(&script(options, &note, &click, left), true).map_err(|_| lost())?;
        if let Some(said) = said.as_str() {
            let answer = answer_of(said);
            if answer.is_err() {
                crate::errores::note("whatsapp", &format!("llave de acceso: {said}"));
            }
            return answer;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    crate::errores::note("whatsapp", "llave de acceso: la pagina se recargo sola y no devolvio nada");
    Err(crate::i18n::tr("El navegador no devolvió la firma."))
}

pub fn sign(options: &Value, limit: Duration) -> Result<Value, String> {
    let browser = browser().ok_or_else(|| crate::i18n::tr("Para la llave de acceso necesito Chrome o Edge instalado."))?;
    if SIGNING.swap(true, Ordering::Relaxed) {
        return Err(crate::i18n::tr("Ya hay una ventana del navegador abierta para la llave."));
    }
    let _signing = Signing;
    CANCEL.store(false, Ordering::Relaxed);
    let temp = std::env::temp_dir();
    for old in std::fs::read_dir(&temp).into_iter().flatten().flatten().filter(|entry| entry.file_name().to_string_lossy().starts_with(PROFILE)) {
        let _ = std::fs::remove_dir_all(old.path());
    }
    let profile = temp.join(format!("{PROFILE}{}", std::process::id()));
    std::fs::create_dir_all(&profile).map_err(|e| e.to_string())?;
    let child = Command::new(browser)
        .arg(format!("--user-data-dir={}", profile.display()))
        .args(["--remote-debugging-port=0", "--no-first-run", "--no-default-browser-check", "--disable-sync", "--window-size=900,680", "--window-position=120,80", ORIGIN])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| crate::i18n::tr("No pude abrir el navegador."))?;
    let pid = child.id();
    let job = crate::whatsapp::job_for(&child);
    let child = Arc::new(Mutex::new(child));
    let finished = Arc::new(AtomicBool::new(false));
    let watchdog = {
        let (child, finished) = (child.clone(), finished.clone());
        std::thread::spawn(move || {
            let started = Instant::now();
            while !finished.load(Ordering::Relaxed) {
                if CANCEL.load(Ordering::Relaxed) || started.elapsed() > limit + Duration::from_secs(10) {
                    let _ = child.lock().unwrap().kill();
                    return;
                }
                std::thread::sleep(Duration::from_millis(200));
            }
        })
    };
    let result = run(&child, pid, &profile, options, limit);
    finished.store(true, Ordering::Relaxed);
    let _ = watchdog.join();
    drop(job);
    {
        let mut child = child.lock().unwrap();
        let _ = child.kill();
        let _ = child.wait();
    }
    wait_for(Duration::from_secs(4), || std::fs::remove_dir_all(&profile).ok().or_else(|| (!profile.exists()).then_some(())));
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_port_the_browser_picked() {
        assert_eq!(port_of("54321\n/devtools/browser/abc\n"), Some(54321));
        assert_eq!(port_of("0\n"), None);
        assert_eq!(port_of(""), None);
    }

    #[test]
    fn only_trusts_the_real_whatsapp_page() {
        let list = json!([
            { "type": "page", "url": "https://web.whatsapp.com.mala.example/", "webSocketDebuggerUrl": "ws://127.0.0.1:9/devtools/page/MALA" },
            { "type": "iframe", "url": "https://web.whatsapp.com/", "webSocketDebuggerUrl": "ws://127.0.0.1:9/devtools/page/FRAME" },
            { "type": "page", "url": "https://web.whatsapp.com/", "webSocketDebuggerUrl": "ws://127.0.0.1:9/devtools/page/BUENA" },
        ]);
        assert_eq!(page_path(&list).as_deref(), Some("/devtools/page/BUENA"));
        assert_eq!(page_path(&json!([{ "type": "page", "url": "chrome://newtab/", "webSocketDebuggerUrl": "ws://127.0.0.1:9/x" }])), None);
    }

    #[test]
    fn the_script_uses_what_whatsapp_asked_for_and_gives_more_time() {
        let options = json!({ "challenge": "abc", "rpId": "whatsapp.com", "timeout": 60000, "userVerification": "required" });
        let code = script(&options, "Elige «uno»", "Haz clic", Duration::from_secs(150));
        assert!(code.contains("\"challenge\":\"abc\"") && code.contains("\"rpId\":\"whatsapp.com\""));
        assert!(code.contains("\"timeout\":150000") && !code.contains("60000"));
        assert!(code.contains("\"Elige «uno»\"") && code.contains("\"Haz clic\""));
    }

    #[test]
    fn the_nulls_gowa_sends_never_reach_chrome() {
        let options = json!({ "challenge": "abc", "allowCredentials": null, "extensions": null });
        let code = script(&options, "", "", Duration::from_secs(1));
        assert!(!code.contains("allowCredentials") && !code.contains("extensions"));
        let options = json!({ "challenge": "abc", "allowCredentials": [{ "id": "x", "type": "public-key", "transports": null }], "extensions": { "prf": { "eval": null, "keep": 1 } } });
        let clean = without_nulls(&options);
        assert_eq!(clean["allowCredentials"][0], json!({ "id": "x", "type": "public-key" }));
        assert_eq!(clean["extensions"]["prf"], json!({ "keep": 1 }));
    }

    #[test]
    fn tells_each_refusal_apart() {
        let signed = answer_of(r#"{"ok":{"id":"x","rawId":"x","type":"public-key","response":{}}}"#).unwrap();
        assert_eq!(signed["type"], "public-key");
        assert!(answer_of(r#"{"error":"NotAllowedError","detail":"timed out or was not allowed"}"#).unwrap_err().contains("Cancelaste"));
        assert!(answer_of(r#"{"error":"AbortError","detail":"aborted"}"#).unwrap_err().contains("Cancelaste"));
        assert!(answer_of(r#"{"error":"NotAllowedError","detail":"The document is not focused."}"#).unwrap_err().contains("al frente"));
        assert!(answer_of(r#"{"error":"TypeError","detail":"PublicKeyCredential.parseRequestOptionsFromJSON is not a function"}"#).unwrap_err().contains("viejo"));
        let other = answer_of(r#"{"error":"SecurityError","detail":"bad rp"}"#).unwrap_err();
        assert!(other.contains("SecurityError") && other.contains("Bluetooth"));
        assert!(answer_of("").is_err());
    }

    #[test]
    #[ignore]
    fn opens_the_real_page_and_asks_for_the_key() {
        let options = json!({ "challenge": "c29sb191bmFfcHJ1ZWJhX3Npbl92YWxvcl9uaW5ndW5v", "timeout": 0, "rpId": "whatsapp.com", "allowCredentials": null, "userVerification": "preferred", "extensions": null });
        let started = Instant::now();
        let said = sign(&options, Duration::from_secs(15));
        println!("[llave] {:?} en {:?}", said.as_ref().map(|_| "firmado").map_err(String::as_str), started.elapsed());
        assert!(!std::env::temp_dir().join(format!("Cocuyo-Llave-{}", std::process::id())).exists());
    }
}
