// datos inventados para la vitrina: nada de esto sale de un pc de verdad
(() => {
  localStorage.setItem("cocuyo.guide", "done");

  const paint = (size, draw) => {
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = size;
    draw(canvas.getContext("2d"), size);
    return canvas.toDataURL("image/png");
  };
  const rounded = (g, x, y, w, h, r) => {
    g.beginPath();
    g.roundRect(x, y, w, h, r);
  };
  const tile = (from, to, glyph) =>
    paint(64, (g, s) => {
      const fill = g.createLinearGradient(0, 0, s, s);
      fill.addColorStop(0, from);
      fill.addColorStop(1, to);
      g.fillStyle = fill;
      rounded(g, 4, 4, s - 8, s - 8, 14);
      g.fill();
      g.fillStyle = "rgba(255,255,255,0.95)";
      g.strokeStyle = "rgba(255,255,255,0.95)";
      g.lineWidth = 5;
      g.lineCap = "round";
      glyph(g, s);
    });
  const glyphs = {
    music: (g) => { g.beginPath(); g.arc(24, 42, 7, 0, 7); g.fill(); g.beginPath(); g.moveTo(30, 42); g.lineTo(30, 18); g.lineTo(44, 15); g.stroke(); },
    chat: (g) => { rounded(g, 16, 18, 32, 22, 8); g.fill(); g.beginPath(); g.moveTo(22, 38); g.lineTo(20, 47); g.lineTo(30, 39); g.fill(); },
    globe: (g) => { g.beginPath(); g.arc(32, 32, 14, 0, 7); g.stroke(); g.beginPath(); g.ellipse(32, 32, 6, 14, 0, 0, 7); g.stroke(); g.beginPath(); g.moveTo(18, 32); g.lineTo(46, 32); g.stroke(); },
    code: (g) => { g.beginPath(); g.moveTo(26, 22); g.lineTo(17, 32); g.lineTo(26, 42); g.moveTo(38, 22); g.lineTo(47, 32); g.lineTo(38, 42); g.stroke(); },
    play: (g) => { g.beginPath(); g.moveTo(25, 19); g.lineTo(45, 32); g.lineTo(25, 45); g.closePath(); g.fill(); },
    folder: (g) => { rounded(g, 15, 22, 34, 24, 4); g.fill(); rounded(g, 15, 18, 14, 8, 3); g.fill(); },
    camera: (g) => { rounded(g, 15, 22, 34, 22, 6); g.fill(); g.fillStyle = "rgba(0,0,0,0.35)"; g.beginPath(); g.arc(32, 33, 7, 0, 7); g.fill(); },
    note: (g) => { rounded(g, 19, 15, 26, 34, 5); g.fill(); g.strokeStyle = "rgba(0,0,0,0.3)"; g.lineWidth = 3; for (const y of [25, 32, 39]) { g.beginPath(); g.moveTo(24, y); g.lineTo(40, y); g.stroke(); } },
    pad: (g) => { rounded(g, 13, 23, 38, 20, 10); g.fill(); g.fillStyle = "rgba(0,0,0,0.35)"; g.fillRect(20, 31, 9, 3); g.fillRect(23, 28, 3, 9); g.beginPath(); g.arc(41, 30, 2.5, 0, 7); g.arc(45, 35, 2.5, 0, 7); g.fill(); },
    pen: (g) => { g.beginPath(); g.moveTo(20, 44); g.lineTo(44, 20); g.stroke(); g.beginPath(); g.moveTo(18, 47); g.lineTo(21, 40); g.lineTo(25, 44); g.closePath(); g.fill(); },
  };
  const icons = {
    musica: tile("#1ed760", "#119c47", glyphs.music),
    chat: tile("#7b8cff", "#4f5fe0", glyphs.chat),
    web: tile("#ffb347", "#f0642a", glyphs.globe),
    codigo: tile("#3ea8ff", "#1666c5", glyphs.code),
    video: tile("#ff6a5c", "#d63a2f", glyphs.play),
    carpeta: tile("#ffd76a", "#e9a93a", glyphs.folder),
    fotos: tile("#c08bff", "#7a45e0", glyphs.camera),
    notas: tile("#ff8fb3", "#e2507f", glyphs.note),
    juegos: tile("#3b3f4a", "#1b1d23", glyphs.pad),
    dibujo: tile("#5be0c2", "#1b9f86", glyphs.pen),
  };

  const scenery = (top, middle, bottom, sun, ridge) =>
    paint(320, (g, s) => {
      const sky = g.createLinearGradient(0, 0, 0, s);
      sky.addColorStop(0, top);
      sky.addColorStop(0.6, middle);
      sky.addColorStop(1, bottom);
      g.fillStyle = sky;
      g.fillRect(0, 0, s, s);
      const glow = g.createRadialGradient(s * 0.68, s * 0.5, 4, s * 0.68, s * 0.5, s * 0.36);
      glow.addColorStop(0, sun);
      glow.addColorStop(1, "rgba(0,0,0,0)");
      g.fillStyle = glow;
      g.fillRect(0, 0, s, s);
      for (const [lift, alpha] of [[0.62, 0.55], [0.74, 0.8], [0.86, 1]]) {
        g.fillStyle = ridge.replace("A", String(alpha));
        g.beginPath();
        g.moveTo(0, s);
        for (let x = 0; x <= s; x += 20) g.lineTo(x, s * lift + Math.sin(x / 31 + lift * 9) * 14 + Math.cos(x / 13) * 5);
        g.lineTo(s, s);
        g.fill();
      }
    });
  const walls = [
    ["Atardecer en Guatapé.mp4", scenery("#1d2b64", "#c06c84", "#f8b195", "rgba(255,226,170,0.95)", "rgba(40,20,60,A)")],
    ["Bosque con niebla.jpg", scenery("#0f2027", "#2c5364", "#9fb8ad", "rgba(220,240,230,0.7)", "rgba(10,30,30,A)")],
    ["Ciudad de neón.mp4", scenery("#120c2c", "#4b1d6e", "#ff4fa0", "rgba(255,120,220,0.9)", "rgba(15,8,35,A)")],
    ["Playa al mediodía.jpg", scenery("#2193b0", "#6dd5ed", "#f6e3b4", "rgba(255,255,230,0.95)", "rgba(10,70,110,A)")],
    ["Aurora.gif", scenery("#020c1b", "#0b6e4f", "#21d4a8", "rgba(160,255,210,0.8)", "rgba(2,15,25,A)")],
    ["Volcán dormido.jpg", scenery("#2b1b17", "#8c3b2a", "#f2a65a", "rgba(255,190,120,0.9)", "rgba(30,15,12,A)")],
    ["Lluvia en la ventana.mp4", scenery("#232526", "#414345", "#8e9eab", "rgba(200,220,240,0.6)", "rgba(20,22,26,A)")],
  ];
  const folder = "C:\\Users\\Valentina\\Pictures\\Fondos";
  const art = paint(240, (g, s) => {
    const fill = g.createLinearGradient(0, 0, s, s);
    fill.addColorStop(0, "#ff7a59");
    fill.addColorStop(0.55, "#d6409f");
    fill.addColorStop(1, "#5b2bd8");
    g.fillStyle = fill;
    g.fillRect(0, 0, s, s);
    g.fillStyle = "rgba(255,240,200,0.9)";
    g.beginPath();
    g.arc(s * 0.5, s * 0.58, s * 0.2, Math.PI, 0);
    g.fill();
    g.fillStyle = "rgba(20,10,40,0.55)";
    for (let i = 0; i < 6; i++) g.fillRect(0, s * 0.6 + i * 12, s, 5);
  });

  const programs = {
    "C:\\Program Files\\Navegador\\navegador.exe": ["Navegador", icons.web],
    "C:\\Users\\Valentina\\AppData\\Roaming\\Musica\\musica.exe": ["Música", icons.musica],
    "C:\\Users\\Valentina\\AppData\\Local\\Charla\\charla.exe": ["Charla", icons.chat],
    "C:\\Program Files\\Editor de código\\editor.exe": ["Editor de código", icons.codigo],
    "C:\\Juegos\\Lanzador\\lanzador.exe": ["Lanzador de juegos", icons.juegos],
  };
  const apps = [
    ["Música", "musica", icons.musica],
    ["Charla", "charla", icons.chat],
    ["Navegador", "navegador", icons.web],
    ["Editor de código", "editor", icons.codigo],
    ["Reproductor", "video", icons.video],
    ["Explorador", "carpeta", icons.carpeta],
    ["Fotos", "fotos", icons.fotos],
    ["Mis Notas", "notas", icons.notas],
    ["Lanzador de juegos", "juegos", icons.juegos],
    ["Pizarra", "dibujo", icons.dibujo],
  ];

  const settings = {
    name: "Valentina",
    city: "Medellín",
    language: "es",
    aiOn: true,
    aiUrl: "https://generativelanguage.googleapis.com/v1beta/openai",
    aiModel: "gemini-3.5-flash-lite, gemini-3.1-flash-lite",
    barStyle: "acrylic",
    barOpacity: 30,
    barCenter: true,
    barSolidMax: true,
    padOn: true,
    voice: "local:davefx",
    wallpaperDir: folder,
    liveWallpaper: `${folder}\\${walls[0][0]}`,
    autostart: true,
  };

  const data = {
    install_state: "installed",
    update_done: null,
    version: "0.1.1",
    get_settings: settings,
    set_settings: (args) => {
      Object.assign(settings, args.settings);
      window.__vitrinaBarra?.(settings);
    },
    weather: {
      place: "Medellín",
      temp: 24,
      feels: 25,
      code: 2,
      day: true,
      max: 27,
      min: 17,
      rain: 30,
      text: "Parcialmente nublado",
      hours: [
        { hour: 14, temp: 25, rain: 10, code: 2 },
        { hour: 15, temp: 26, rain: 20, code: 3 },
        { hour: 16, temp: 25, rain: 40, code: 61 },
        { hour: 17, temp: 23, rain: 60, code: 63 },
      ],
    },
    weather_city: () => data.weather,
    todos_get: [
      { text: "Comprar el regalo de mamá", done: false },
      { text: "Pagar el internet", done: true },
    ],
    agenda_list: [
      { id: 1, what: "Sacar la pizza del horno", act: false, when: "en 12 min" },
      { id: 2, what: "abre la música", act: true, when: "a diario 7:30" },
    ],
    media_now: { title: "Neón en la playa", artist: "Marea Alta", source: "Música", playing: true, position: 72, duration: 214, key: "neon", art },
    volume_get: { level: 0.62, muted: false },
    mic_muted: false,
    system_stats: {
      cpu: 21,
      threads: 8,
      memoryUsed: 7.6 * 2 ** 30,
      memoryTotal: 16 * 2 ** 30,
      diskUsed: 318 * 2 ** 30,
      diskTotal: 476 * 2 ** 30,
      uptimeSecs: 3 * 3600 + 22 * 60,
      top: Object.keys(programs).map((path, i) => ({ name: programs[path][0], bytes: [1.9, 0.82, 0.61, 0.44, 0.31][i] * 2 ** 30, path })),
    },
    program_info: (args) => {
      const known = programs[args.path];
      return known ? { name: known[0], icon: known[1] } : null;
    },
    apps_list: apps.map(([name, id]) => ({ name, id, custom: false })),
    app_icon: (args) => apps.find((app) => app[1] === args.id)?.[2] ?? null,
    wallpapers_list: { dir: folder, files: walls.map(([name]) => `${folder}\\${name}`) },
    wallpaper_thumb: (args) => walls.find(([name]) => args.path.endsWith(name))?.[1] ?? null,
    screens_list: [
      { id: "\\\\.\\DISPLAY1", number: 1, main: true, live: `${folder}\\${walls[0][0]}` },
      { id: "\\\\.\\DISPLAY2", number: 2, main: false, live: `${folder}\\${walls[2][0]}` },
    ],
    clip_list: {
      now: Date.now(),
      clips: [
        { text: "La reunión quedó para el jueves a las 3", at: Date.now() - 60000, image: null },
        { text: "", at: Date.now() - 300000, image: { w: 1280, h: 720, thumb: walls[3][1] } },
        { text: "calle 10 # 43-25, El Poblado", at: Date.now() - 1800000, image: null },
        { text: "https://github.com/Tkyoxx/Cocuyo", at: Date.now() - 3600000, image: null },
      ],
    },
    spotify_status: { source: "cocuyo", hasClientId: true, connecting: false },
    voice_status: { installed: true, installing: false, megabytes: 470 },
    tts_voices: [
      { id: "davefx", label: "Davefx", megabytes: 65, installed: true, installing: false, slow: false },
      { id: "claude", label: "Claude", megabytes: 65, installed: false, installing: false, slow: false },
      { id: "daniela", label: "Daniela", megabytes: 115, installed: false, installing: false, slow: false },
      { id: "dora", label: "Dora", megabytes: 130, installed: false, installing: false, slow: true },
    ],
    system_voices: [{ name: "Microsoft Pablo", language: "es-ES" }, { name: "Microsoft Laura", language: "es-ES" }],
    pad_status: "Mando de Xbox conectado. Mantén L3 + R3 para hablar.",
    pad_label: "L3 + R3",
    taskbar_state: { autohide: false, windows11: true, rival: null, deep: true },
    gallery_list: [
      { name: "organiza descargas", about: "Ordena tu carpeta de Descargas en carpetas por tipo.", file: "organiza descargas.ps1", author: "Tkyoxx", have: true },
      { name: "como esta mi internet", about: "Mide el ping y los paquetes perdidos. Para saber si el lag es tuyo.", file: "como esta mi internet.ps1", author: "Tkyoxx", have: false },
      { name: "modo foco", about: "Cierra lo que distrae. A trabajar, que el día no se hace solo.", file: "modo foco.ps1", author: "luna-cocuyera", have: false },
      { name: "dolar hoy", about: "Dice a cómo está el dólar en tu moneda.", file: "dolar hoy.ps1", author: "pacho-del-barrio", have: false },
      { name: "limpia temporales", about: "Borra temporales viejos y dice cuánto liberó.", file: "limpia temporales.ps1", author: "mafe-scripts-co", have: true },
      { name: "cuenta un chiste", about: "Cuenta un chiste malo. Malo de verdad.", file: "cuenta un chiste.ps1", author: "profe-de-cocuyo", have: false },
      { name: "resumen del pc", about: "Procesador, gráfica, memoria y disco en una frase.", file: "resumen del pc.ps1", author: "nico-hace-scripts", have: false },
      { name: "piedra papel o tijera", about: "Juega una ronda. Pierdes, pero con estilo.", file: "piedra papel o tijera.ps1", author: "tati-cocuyera", have: false },
    ],
    scripts_list: [
      { name: "organiza descargas", about: "Ordena tu carpeta de Descargas en carpetas por tipo." },
      { name: "limpia temporales", about: "Borra temporales viejos y dice cuánto liberó." },
      { name: "luces del cuarto", about: "Prende o apaga las luces inteligentes." },
    ],
    memory_list: ["Mi perro se llama Toby", "Trabajo de 8 a 5", "Mi talla de zapatos es 38"],
    ai_key_saved: true,
    ai_budget: [37, 960],
    ai_models: ["gemini-3.5-flash-lite", "gemini-3.1-flash-lite", "gemma-4-27b-it"],
    update_check: null,
    extensions_list: [
      { id: "cocuyos", nombre: "Cocuyos", descripcion: "Luciérnagas alrededor de la isla al ritmo de tu voz.", autor: "Tkyoxx", version: "1.0", tipo: "extension", estilo: null, codigo: `${location.origin}/galeria/extensiones/cocuyos/index.js`, vista: null, activa: true },
      { id: "foco", nombre: "Foco", descripcion: "Pestaña Pomodoro: di «empieza el foco».", autor: "Tkyoxx", version: "1.0", tipo: "extension", estilo: null, codigo: `${location.origin}/galeria/extensiones/foco/index.js`, vista: null, activa: true },
      { id: "aurora", nombre: "Aurora", descripcion: "La isla con un borde de aurora boreal que respira.", autor: "Tkyoxx", version: "1.0", tipo: "tema", estilo: `${location.origin}/galeria/extensiones/aurora/estilo.css`, codigo: null, vista: null, activa: new URLSearchParams(location.search).has("aurora") },
      { id: "neon", nombre: "Neón", descripcion: "Cyberpunk: magenta, cian y letra de terminal.", autor: "Tkyoxx", version: "1.0", tipo: "tema", estilo: `${location.origin}/galeria/extensiones/neon/estilo.css`, codigo: null, vista: null, activa: new URLSearchParams(location.search).has("neon") },
    ],
    extensions_gallery: [
      { id: "cocuyos", nombre: "Cocuyos", descripcion: "Luciérnagas que revolotean alrededor de la isla al ritmo de tu voz.", autor: "Tkyoxx", tipo: "extension", vista: "", have: true },
      { id: "foco", nombre: "Foco", descripcion: "Pestaña Pomodoro: 25 de trabajo, 5 de descanso.", autor: "Tkyoxx", tipo: "extension", vista: "", have: true },
      { id: "aurora", nombre: "Aurora", descripcion: "La isla con un borde de aurora boreal que respira.", autor: "Tkyoxx", tipo: "tema", vista: "", have: true },
      { id: "medianoche", nombre: "Medianoche", descripcion: "Azul noche y lila. Para los que programan a las 3 a. m.", autor: "Tkyoxx", tipo: "tema", vista: "", have: false },
      { id: "neon", nombre: "Neón", descripcion: "Cyberpunk: magenta, cian y letra de terminal.", autor: "Tkyoxx", tipo: "tema", vista: "", have: true },
    ],
    extension_commands: null,
    script_action: "",
    say_aloud: null,
    lasso_ask: { question: "¿Qué es esta planta?", answer: "Es una monstera. Le gusta la luz indirecta y regarla cuando la tierra está seca." },
  };

  addEventListener("load", () => window.__vitrinaBarra?.(settings));
  const listeners = {};
  window.__vitrina = {
    data,
    call(command, args) {
      const value = data[command];
      return typeof value === "function" ? value(args ?? {}) : value;
    },
    on(event, handler) {
      (listeners[event] ||= []).push(handler);
    },
    emit(event, payload) {
      for (const handler of listeners[event] ?? []) handler(payload);
    },
  };
})();
