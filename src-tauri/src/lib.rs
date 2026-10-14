// aqui se arma todo. si algo no arranca, empieza a buscar por aca

mod agenda;
mod ai;
mod apps;
mod capture;
mod clip;
mod cue;
mod agent;
mod dialogs;
mod extensions;
mod gamelayer;
mod hotkeys;
mod i18n;
mod inject;
mod install;
mod intent;
mod live;
mod mcp;
mod minutes;
mod media;
mod mic;
mod net;
mod notes;
mod ocr;
mod overlay;
mod pad;
mod pc;
mod placement;
mod player;
mod scripts;
mod settings;
mod spotify;
mod system;
mod talk;
mod taskbar;
mod tray;
mod tts;
mod update;
mod voice;
mod weather;

use tauri::Manager;

pub(crate) const BROWSER_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,CalculateNativeWinOcclusion --in-process-gpu --enable-features=NetworkServiceInProcess2 --disable-background-networking --disable-component-update --disable-extensions";

#[cfg(debug_assertions)]
fn demo(app: tauri::AppHandle) {
    use tauri::Emitter;
    let Ok(steps) = std::env::var("COCUYO_DEMO") else { return };
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(4));
        for step in steps.split(',') {
            if let Some(query) = step.strip_prefix("spotify=") {
                eprintln!("[spotify] {}", media::probe(&app, query));
            } else if let Some(path) = step.strip_prefix("api=") {
                eprintln!("[api] {path} -> {}", media::raw(&app, path));
            } else if step == "capa" {
                eprintln!("[capa] {}", gamelayer::probe(&app));
            } else if step == "sonando" {
                let now = tauri::async_runtime::block_on(media::media_now(None));
                eprintln!("[sonando] {:?}", now.map(|n| serde_json::to_string(&n).map(|s| s.chars().take(260).collect::<String>())));
            } else if let Some(index) = step.strip_prefix("paso=") {
                let _ = app.emit_to(overlay::LABEL, "install-step", index.parse::<u8>().unwrap_or(0));
            } else if let Some(dir) = step.strip_prefix("instalar=") {
                let dir = std::path::Path::new(dir);
                eprintln!("[instalar] {:?}", install::copy_and_link(dir, &dir.join("Cocuyo.lnk")));
            } else if let Some(file) = step.strip_prefix("fondo=") {
                let path = (!file.is_empty()).then(|| std::path::Path::new(file));
                eprintln!("[fondo] {file:?} -> {:?}", live::choose(&app, path, None));
            } else if let Some(wav) = step.strip_prefix("dictado=") {
                let heard = voice::transcribe_file(&app, std::path::Path::new(wav));
                let pasted = heard.as_ref().map(|text| inject::paste(text));
                eprintln!("[voz] dictado {heard:?} -> {pasted:?}");
            } else if let Some(choice) = step.strip_prefix("vivo-modelo=") {
                let (model, think) = choice.split_once('|').unwrap_or((choice, ""));
                talk::try_model(model, think);
                eprintln!("[vivo] probando {model} pensando «{think}»");
            } else if step == "memoria-prueba" {
                let charla = [
                    ("oye, ¿me recuerdas a qué hora es el partido de mi hermano Mateo?".to_string(), "No lo tengo anotado, ¿a qué hora es?".to_string()),
                    ("a las 4, él juega de portero en el equipo del colegio".to_string(), "Listo, a las 4. ¡Que tape mucho!".to_string()),
                    ("y pon algo de salsa que ando cocinando".to_string(), "Sonando salsa.".to_string()),
                    ("es que los domingos siempre hago el almuerzo para mi mamá".to_string(), "Qué buen plan. ¡Buen provecho!".to_string()),
                ];
                eprintln!("[memoria] {:?}", ai::learned(&app, &charla));
            } else if step == "vivo-modelos" {
                eprintln!("[vivo] modelos: {:?}", ai::live_models(&app));
            } else if let Some(file) = step.strip_prefix("resumir=") {
                let text = std::fs::read_to_string(file).unwrap_or_default();
                let heard = text.split("## Lo que se dijo").nth(1).unwrap_or(&text).to_string();
                eprintln!("[apuntes] resumen:
{}", ai::summarize_minutes(&app, &heard).unwrap_or_else(|e| e));
            } else if step == "apuntes-empezar" {
                eprintln!("[apuntes] {:?}", minutes::start(&app));
            } else if step == "apuntes-terminar" {
                eprintln!("[apuntes] {:?}", minutes::finish(&app));
            } else if let Some(seconds) = step.strip_prefix("esperar=") {
                std::thread::sleep(std::time::Duration::from_secs(seconds.parse().unwrap_or(1)));
            } else if let Some(wav) = step.strip_prefix("apuntes-oir=") {
                eprintln!("[apuntes] {:?}", minutes::probe(&app, std::path::Path::new(wav)));
            } else if let Some(wav) = step.strip_prefix("vivo-cortar=") {
                eprintln!("[vivo] cortar {:?}", talk::probe_barge(&app, std::path::Path::new(wav)));
                talk::forget();
            } else if let Some(wav) = step.strip_prefix("vivo-lazo=") {
                eprintln!("[vivo] lazo {:?}", talk::probe_lasso(&app, std::path::Path::new(wav)));
                talk::forget();
            } else if let Some(wav) = step.strip_prefix("vivo-oir=") {
                eprintln!("[vivo] {:?}", talk::probe_audio(&app, std::path::Path::new(wav)));
                talk::forget();
            } else if let Some(text) = step.strip_prefix("vivo=") {
                let started = std::time::Instant::now();
                eprintln!("[vivo] {:?} en {:?}", talk::probe(&app, text), started.elapsed());
                talk::forget();
            } else if step == "ia" {
                let started = std::time::Instant::now();
                let order = ai::rewrite(&app, "oye ponme la de beibi dol de dominic faik porfa", &scripts::list(&app));
                eprintln!("[ia] {order:?} en {:?}", started.elapsed());
                eprintln!("[ia] modelos: {:?}", tauri::async_runtime::block_on(ai::ai_models(app.clone())));
            } else if let Some(image) = step.strip_prefix("ver=") {
                let started = std::time::Instant::now();
                let seen = std::fs::read(image).map_err(|e| e.to_string()).and_then(|png| ai::see(&app, "que hay en esta parte de la pantalla", &png));
                eprintln!("[ver] {seen:?} en {:?}", started.elapsed());
            } else if let Some(wav) = step.strip_prefix("oir-ia=") {
                let started = std::time::Instant::now();
                let heard = std::fs::read(wav).map_err(|e| e.to_string()).and_then(|bytes| ai::hear(&app, &bytes, false, &scripts::list(&app)));
                eprintln!("[oir-ia] {heard:?} en {:?}", started.elapsed());
            } else if let Some(text) = step.strip_prefix("di=") {
                for turn in 1..=2 {
                    let started = std::time::Instant::now();
                    let said = ai::voice_wav(&app, text, "Kore").map(|file| std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0));
                    eprintln!("[di] vez {turn}: {said:?} bytes en {:?}", started.elapsed());
                }
            } else if let Some(id) = step.strip_prefix("voz-local=") {
                eprintln!("[voz-local] {id}: {}", tts::try_out(&app, id, "Listo. Sonando Baby Doll de Dominic Fike."));
            } else if step == "mando" {
                eprintln!("[mando] {}", tauri::async_runtime::block_on(pad::pad_status()));
            } else if let Some(text) = step.strip_prefix("loquendero=") {
                for turn in 1..=2 {
                    let started = std::time::Instant::now();
                    let said = tts::loquendero(&app, text).map(|file| std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0));
                    eprintln!("[loquendero] vez {turn}: {said:?} bytes en {:?}", started.elapsed());
                }
                eprintln!("[loquendero] voces de Windows: {:?}", tauri::async_runtime::block_on(tts::system_voices()).len());
            } else if let Some(text) = step.strip_prefix("pulir=") {
                eprintln!("[pulir] {:?}", ai::polish(&app, text));
            } else if step == "atrapar" {
                eprintln!("[fondo] atento a las descargas: {:?}", live::catch_downloads(&app));
            } else if let Some(phrase) = step.strip_prefix("frase=") {
                let started = std::time::Instant::now();
                eprintln!("[frase] {phrase:?} = {:?} -> {:?} en {:?}", intent::parse(phrase), voice::run_order(&app, phrase), started.elapsed());
            } else if let Some(wav) = step.strip_prefix("oir=") {
                let heard = voice::transcribe_clip(&app, std::path::Path::new(wav)).map(Option::unwrap_or_default);
                let understood = heard.as_ref().ok().map(|text| intent::parse(text));
                let choice = match &understood {
                    Some(intent::Intent::Spotify(query)) => media::probe(&app, query).split(';').nth(1).unwrap_or_default().to_string(),
                    _ => String::new(),
                };
                eprintln!("[oir] {heard:?} = {understood:?} ->{choice}");
            } else if let Some(wav) = step.strip_prefix("orden=") {
                let started = std::time::Instant::now();
                let heard = voice::transcribe_file(&app, std::path::Path::new(wav));
                let done = heard.as_ref().map(|text| voice::run_order(&app, text));
                eprintln!("[voz] {wav}: {heard:?} -> {done:?} en {:?}", started.elapsed());
            } else {
                let _ = app.emit_to(overlay::LABEL, "demo", step.to_string());
            }
            std::thread::sleep(std::time::Duration::from_secs(3));
        }
    });
}

pub fn run() {
    if std::env::args().any(|arg| arg == "--uninstall") {
        return install::uninstall();
    }
    install::wait_for_previous();
    update::tidy();
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|_app, _args, _cwd| {}))
        .invoke_handler(tauri::generate_handler![
            overlay::set_region,
            overlay::set_focusable,
            hotkeys::set_key_echo,
            hotkeys::set_chords_paused,
            hotkeys::capture_keys_available,
            capture::capture_commit,
            capture::capture_discard,
            placement::get_place,
            settings::get_settings,
            settings::set_settings,
            settings::current_capture_dir,
            settings::pick_capture_dir,
            voice::voice_status,
            voice::voice_install,
            voice::set_voice_practice,
            apps::apps_list,
            apps::app_launch,
            apps::app_add,
            apps::app_remove,
            apps::app_rename,
            system::app_icon,
            system::program_info,
            net::net_speed,
            net::dns_measure,
            net::dns_apply,
            net::dns_reset,
            system::power,
            system::open_settings_page,
            system::open_capture_dir,
            system::volume_get,
            system::volume_set,
            system::media_key,
            media::media_now,
            media::media_seek,
            spotify::spotify_status,
            spotify::spotify_connect,
            spotify::spotify_disconnect,
            system::system_stats,
            system::wallpapers_list,
            system::wallpaper_thumb,
            system::wallpaper_set,
            system::wallpaper_pick_dir,
            system::todos_get,
            system::todos_set,
            clip::clip_list,
            clip::clip_use,
            clip::clip_clear,
            live::live_wallpaper_set,
            live::live_wallpaper_get,
            live::screens_list,
            live::wallpaper_browse,
            voice::lasso_ask,
            weather::weather,
            weather::weather_city,
            agenda::agenda_list,
            agenda::agenda_remove,
            agenda::agenda_add,
            ai::ai_key_set,
            ai::ai_key_saved,
            ai::ai_test,
            ai::ai_models,
            ai::ai_key_page,
            ai::ai_usage,
            ai::ai_budget,
            cue::voice_try,
            tts::tts_voices,
            tts::system_voices,
            pad::pad_status,
            pad::pad_record,
            pad::pad_label,
            ocr::ocr_clip,
            update::version,
            update::update_check,
            update::update_install,
            update::update_done,
            taskbar::taskbar_state,
            taskbar::taskbar_autohide,
            taskbar::taskbar_close_rival,
            scripts::gallery_publish,
            scripts::gallery_list,
            scripts::gallery_install,
            ai::memory_list,
            ai::memory_add,
            ai::memory_remove,
            system::mic_muted,
            system::mic_mute,
            tts::tts_install,
            scripts::scripts_list,
            scripts::scripts_open,
            scripts::script_action,
            scripts::script_run,
            scripts::say_aloud,
            extensions::extensions_list,
            extensions::extensions_open,
            extensions::extension_set,
            extensions::extensions_master,
            extensions::extensions_gallery,
            extensions::extension_install,
            extensions::extension_commands,
            mcp::mcp_status,
            agent::extension_tools,
            agent::extension_tool_done,
            mcp::mcp_open,
            mcp::mcp_reload,
            install::install_state,
            install::install_run,
            install::install_relaunch,
            install::install_skip,
        ])
        .setup(|app| {
            let window = app
                .get_webview_window(overlay::LABEL)
                .expect("la ventana overlay está declarada en tauri.conf.json");
            overlay::prepare(&window)?;
            tray::build(app.handle())?;
            mic::spawn(app.handle().clone());
            extensions::allow(app.handle());
            settings::apply(app.handle());
            if settings::load(app.handle()).autostart {
                let _ = system::set_autostart(true);
            }
            agenda::start(app.handle().clone(), std::env::args().any(|arg| arg == "--inicio"));
            update::watch(app.handle().clone());
            hotkeys::spawn(app.handle().clone());
            let handle = app.handle().clone();
            std::thread::spawn(move || live::restore(&handle));
            #[cfg(debug_assertions)]
            demo(app.handle().clone());
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error al iniciar Cocuyo")
        .run(|_app, event| {
            if let tauri::RunEvent::Exit = event {
                taskbar::restore();
                mcp::shutdown();
                talk::forget();
            }
        });
}
