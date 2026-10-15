// la pagina del celular. la vista, el microfono y lo que llega del pc

import { tarjeta } from "./acciones.js";
import { borrarInforme, fecha, guardarInforme, informes, pintarInforme } from "./informes.js";
import { idioma, t, traducirPagina } from "./idioma.js";

const $ = (id) => document.getElementById(id);
const isla = $("isla");
const islaTexto = $("isla-texto");
const charla = $("charla");
const vacio = $("vacio");
const mic = $("mic");
const muestra = new URLSearchParams(location.search).has("muestra");

let enlace = null;
let pcNombre = "tu PC";
let esperando = null;
let porDecir = "";
const altavoz = new Audio();
let oido = null;
let siguiente = 0;
let sonando = [];
let yoVivo = null;
let cocuyoVivo = null;

function oidoListo() {
  try {
    oido ??= new AudioContext();
  } catch {
    return null;
  }
  if (oido.state === "suspended") void oido.resume().catch(() => {});
  return oido;
}
addEventListener("pointerdown", () => oidoListo(), { capture: true });

function callarTrozos() {
  for (const fuente of sonando) {
    try {
      fuente.stop();
    } catch {}
  }
  sonando = [];
  siguiente = 0;
}

function sonarTrozo(pcm) {
  porDecir = "";
  if (!$("hablar").checked) return;
  const ctx = oidoListo();
  if (!ctx) return;
  speechSynthesis?.cancel();
  const muestras = new Int16Array(pcm.buffer, pcm.byteOffset, pcm.byteLength >> 1);
  if (!muestras.length) return;
  const buffer = ctx.createBuffer(1, muestras.length, 24000);
  const canal = buffer.getChannelData(0);
  for (let i = 0; i < muestras.length; i++) canal[i] = muestras[i] / 32768;
  const fuente = ctx.createBufferSource();
  fuente.buffer = buffer;
  fuente.connect(ctx.destination);
  if (siguiente < ctx.currentTime + 0.02) siguiente = ctx.currentTime + 0.08;
  fuente.start(siguiente);
  siguiente += buffer.duration;
  sonando.push(fuente);
  fuente.onended = () => (sonando = sonando.filter((f) => f !== fuente));
}

function despertarAltavoz() {
  if (altavoz.dataset.listo) return;
  altavoz.dataset.listo = "1";
  const silencio = Uint8Array.from(atob("UklGRiQAAABXQVZFZm10IBAAAAABAAEAQB8AAIA+AAACABAAZGF0YQAAAAA="), (c) => c.charCodeAt(0));
  altavoz.src = URL.createObjectURL(new Blob([silencio], { type: "audio/wav" }));
  void altavoz.play().catch(() => {});
}
addEventListener("pointerdown", despertarAltavoz, { once: true, capture: true });

try {
  if (localStorage.getItem("cocuyo.hablar") === "0") $("hablar").checked = false;
} catch {}

function preferencias() {
  void enlace?.enviar({ tipo: "prefs", voz: $("hablar").checked, trozos: true }).catch(() => {});
}

$("hablar").addEventListener("change", () => {
  try {
    localStorage.setItem("cocuyo.hablar", $("hablar").checked ? "1" : "0");
  } catch {}
  if (!$("hablar").checked) {
    altavoz.pause();
    callarTrozos();
    speechSynthesis?.cancel();
  }
  preferencias();
});

function estado(cual, texto) {
  isla.dataset.estado = cual;
  islaTexto.textContent = t(texto);
}

let ultimoNivel = "";
function nivel(peak) {
  const lv = Math.min(1, Math.sqrt(peak) * 1.7).toFixed(2);
  if (lv === ultimoNivel) return;
  ultimoNivel = lv;
  isla.style.setProperty("--lv", lv);
  mic.style.setProperty("--lv", lv);
}

function bajar() {
  requestAnimationFrame(() => charla.scrollTo({ top: charla.scrollHeight, behavior: "smooth" }));
}

function dicho(quien, texto, extra = "") {
  vacio.hidden = true;
  const el = document.createElement("div");
  el.className = `dicho ${quien}`;
  el.textContent = quien.includes("error") ? t(texto) : texto;
  if (extra) {
    const small = document.createElement("small");
    small.textContent = t(extra);
    el.append(small);
  }
  charla.append(el);
  bajar();
  return el;
}

function pintarTarjeta(t) {
  const el = document.createElement("div");
  el.className = "sc";
  if (t.color) el.style.setProperty("--tint", t.color);
  const head = document.createElement("div");
  head.className = "sc-head";
  const emoji = document.createElement("span");
  emoji.className = "sc-emoji";
  emoji.textContent = t.icono ?? "✨";
  const title = document.createElement("div");
  title.className = "sc-title";
  const b = document.createElement("b");
  b.textContent = t.titulo ?? "";
  const small = document.createElement("small");
  small.textContent = t.sub ?? "";
  title.append(b, small);
  head.append(emoji, title);
  el.append(head);
  if (t.texto) {
    const p = document.createElement("p");
    p.className = "sc-text";
    p.textContent = t.texto;
    el.append(p);
  }
  if (t.imagen) {
    const img = document.createElement("img");
    img.className = "sc-img";
    img.src = t.imagen;
    img.alt = "";
    el.append(img);
  }
  if (t.botones?.length) {
    const fila = document.createElement("div");
    fila.className = "sc-botones";
    for (const [texto, hacer] of t.botones) {
      const boton = document.createElement("button");
      boton.textContent = texto;
      boton.addEventListener("click", hacer);
      fila.append(boton);
    }
    el.append(fila);
  }
  vacio.hidden = true;
  charla.append(el);
  bajar();
}

function hablar(texto) {
  if (!$("hablar").checked || !("speechSynthesis" in window) || !texto) return;
  const voz = new SpeechSynthesisUtterance(texto.replace(/[«»]/g, ""));
  voz.lang = { es: "es-419", en: "en-US", pt: "pt-BR" }[idioma];
  speechSynthesis.cancel();
  speechSynthesis.speak(voz);
}

function sonar(wav) {
  porDecir = "";
  if (!$("hablar").checked) return;
  speechSynthesis?.cancel();
  if (altavoz.src.startsWith("blob:")) URL.revokeObjectURL(altavoz.src);
  altavoz.src = URL.createObjectURL(new Blob([wav], { type: "audio/wav" }));
  void altavoz.play().catch(() => hablar(porDecir));
}

function listo() {
  clearTimeout(esperando);
  esperando = null;
  estado("conectado", `Conectado a ${pcNombre}`);
}

function recibir(mensaje) {
  if (mensaje.tipo === "oido") {
    if (!mensaje.texto) return;
    if (yoVivo) yoVivo.firstChild.textContent = mensaje.texto;
    else yoVivo = dicho("yo", mensaje.texto, "por voz");
    if (!mensaje.parcial) yoVivo = null;
    bajar();
    return;
  }
  if (mensaje.tipo === "diciendo") {
    if (!mensaje.texto) return;
    if (cocuyoVivo) cocuyoVivo.textContent = mensaje.texto;
    else cocuyoVivo = dicho("cocuyo", mensaje.texto);
    bajar();
    return;
  }
  if (mensaje.tipo === "trozo") return sonarTrozo(mensaje.pcm);
  if (mensaje.tipo === "voz-fin") {
    porDecir = "";
    return;
  }
  if (mensaje.tipo === "voz") return sonar(mensaje.wav);
  if (mensaje.tipo === "sin-voz") {
    hablar(porDecir);
    porDecir = "";
    return;
  }
  if (mensaje.tipo === "informe") return void llegoInforme(mensaje.informe ?? {});
  if (mensaje.tipo !== "respuesta") return;
  listo();
  yoVivo = null;
  const vivo = cocuyoVivo;
  cocuyoVivo = null;
  if (mensaje.error) dicho("cocuyo error", mensaje.error);
  else if (mensaje.texto && vivo) vivo.textContent = mensaje.texto;
  else if (mensaje.texto) dicho("cocuyo", mensaje.texto);
  for (const t of mensaje.tarjetas ?? []) pintarTarjeta({ ...t, botones: (t.botones ?? []).map((b) => [b.texto, () => b.url && window.open(b.url, "_blank", "noopener")]) });
  for (const accion of mensaje.acciones ?? []) {
    const t = tarjeta(accion);
    if (t) pintarTarjeta(t);
  }
  if (mensaje.hablada) porDecir = "";
  else if (mensaje.voz) porDecir = mensaje.error || mensaje.texto || "";
  else hablar(mensaje.error || mensaje.texto);
}

function aguardar() {
  estado("pensando", "Pensando…");
  clearTimeout(esperando);
  esperando = setTimeout(() => {
    listo();
    dicho("cocuyo error", `${pcNombre} no respondió. Puede que siga trabajando; mira tu PC.`);
  }, 120000);
}

async function pedir(texto) {
  texto = texto.trim();
  if (!texto || !enlace) return;
  dicho("yo", texto);
  aguardar();
  try {
    await enlace.enviar({ tipo: "orden", texto });
  } catch (error) {
    listo();
    dicho("cocuyo error", String(error?.message ?? error));
  }
}

$("escribir").addEventListener("submit", (e) => {
  e.preventDefault();
  const input = $("texto");
  void pedir(input.value);
  input.value = "";
});

for (const idea of $("ideas").querySelectorAll("button")) idea.addEventListener("click", () => void pedir(idea.textContent ?? ""));

function moverLuz(boton) {
  const luz = $("pestanas").querySelector(".pestana-luz");
  luz.style.setProperty("--x", `${boton.offsetLeft}px`);
  luz.style.setProperty("--w", `${boton.offsetWidth}px`);
}

let leyendo = null;

function irA(vista) {
  for (const otro of $("pestanas").querySelectorAll("button")) otro.classList.toggle("on", otro.dataset.vista === vista);
  for (const cual of ["charla", "informes", "ajustes"]) $(cual).hidden = cual !== vista;
  $("lector").hidden = true;
  document.querySelector(".dock").hidden = vista !== "charla";
  if (vista === "informes") void pintarInformes();
}

async function pintarInformes() {
  let todos = [];
  try {
    todos = await informes();
  } catch {}
  $("informes-vacio").hidden = todos.length > 0;
  const caja = $("lista-informes");
  caja.replaceChildren();
  for (const informe of todos) {
    const fila = document.createElement("button");
    fila.className = "informe-fila";
    const b = document.createElement("b");
    b.textContent = informe.titulo || informe.tema || t("Informe");
    const small = document.createElement("small");
    small.textContent = `${fecha(informe.en)} · ${(informe.fuentes ?? []).length} ${t("fuentes")}`;
    fila.append(b, small);
    fila.addEventListener("click", () => leer(informe));
    caja.append(fila);
  }
}

function leer(informe) {
  leyendo = informe;
  for (const cual of ["charla", "informes", "ajustes"]) $(cual).hidden = true;
  document.querySelector(".dock").hidden = true;
  pintarInforme($("lector-texto"), informe);
  $("lector").hidden = false;
  $("lector").scrollTop = 0;
}

$("lector-volver").addEventListener("click", () => irA("informes"));
$("lector-borrar").addEventListener("click", async () => {
  if (!leyendo || !confirm(t("¿Borrar este informe del celular?"))) return;
  await borrarInforme(leyendo.en).catch(() => {});
  leyendo = null;
  irA("informes");
});

async function llegoInforme(informe) {
  if (informe.error) {
    dicho("cocuyo error", informe.error);
    return;
  }
  let guardado = informe;
  try {
    guardado = await guardarInforme(informe);
  } catch {}
  const resumen = String(guardado.resumen ?? "").replace(/\s*\[\d+\]/g, "").trim();
  pintarTarjeta({
    icono: "🔎",
    titulo: guardado.titulo || guardado.tema || t("Informe"),
    sub: t("Informe listo"),
    texto: resumen.length > 320 ? `${resumen.slice(0, 317)}…` : resumen,
    botones: [[t("Leer el informe"), () => leer(guardado)]],
  });
  navigator.vibrate?.([20, 60, 20]);
}

for (const boton of $("pestanas").querySelectorAll("button")) {
  boton.addEventListener("click", () => {
    for (const otro of $("pestanas").querySelectorAll("button")) otro.classList.toggle("on", otro === boton);
    irA(boton.dataset.vista);
    moverLuz(boton);
  });
}

let grabando = null;
let micro = null;
const TIBIO = 60000;
const COLA = 150;

class Bajador {
  constructor(rate) {
    this.paso = rate / 16000;
    this.medio = Math.ceil(16 * this.paso);
    this.fases = 128;
    const corte = 7300 / rate;
    this.tabla = Array.from({ length: this.fases }, (_, f) => {
      const h = new Float32Array(2 * this.medio + 1);
      let suma = 0;
      for (let i = 0; i < h.length; i++) {
        const t = i - this.medio - f / this.fases;
        const x = (t / (this.medio + 1) + 1) / 2;
        const ventana = 0.42 - 0.5 * Math.cos(2 * Math.PI * x) + 0.08 * Math.cos(4 * Math.PI * x);
        h[i] = (t === 0 ? 1 : Math.sin(2 * Math.PI * corte * t) / (2 * Math.PI * corte * t)) * ventana;
        suma += h[i];
      }
      return h.map((v) => v / suma);
    });
    this.resto = new Float32Array(0);
    this.pos = 0;
  }

  meter(datos) {
    const todo = new Float32Array(this.resto.length + datos.length);
    todo.set(this.resto);
    todo.set(datos, this.resto.length);
    const salida = [];
    let p = this.pos;
    while (Math.floor(p) + this.medio + 1 < todo.length) {
      let k0 = Math.floor(p);
      let f = Math.round((p - k0) * this.fases);
      if (f === this.fases) {
        k0++;
        f = 0;
      }
      const h = this.tabla[f];
      const desde = k0 - this.medio;
      let s = 0;
      for (let i = Math.max(0, -desde); i < h.length; i++) s += todo[desde + i] * h[i];
      salida.push(Math.max(-32768, Math.min(32767, Math.round(s * 32767))));
      p += this.paso;
    }
    const guardar = Math.max(0, Math.floor(p) - this.medio - 1);
    this.resto = todo.slice(guardar);
    this.pos = p - guardar;
    return Int16Array.from(salida);
  }

  vaciar() {
    return this.meter(new Float32Array(this.medio + 2));
  }
}

function juntar(trozos) {
  const todo = new Int16Array(trozos.reduce((n, t) => n + t.length, 0));
  let i = 0;
  for (const t of trozos) {
    todo.set(t, i);
    i += t.length;
  }
  return todo;
}

function alOir(datos) {
  const g = grabando;
  if (!g?.bajador) return;
  if (!g.oyendo && !g.soltando) {
    g.oyendo = true;
    mic.classList.replace("abriendo", "oyendo");
    estado("escuchando", "Te escucho…");
    navigator.vibrate?.(12);
  }
  g.largo += datos.length;
  const pcm = g.bajador.meter(datos);
  if (g.enVivo) {
    g.porMandar.push(pcm);
    g.cuantos += pcm.length;
    if (g.cuantos >= 4000) {
      const junto = juntar(g.porMandar);
      g.porMandar = [];
      g.cuantos = 0;
      void enlace.enviar({ tipo: "trozo", pcm: new Uint8Array(junto.buffer) }).catch(() => {});
    }
  } else g.listos.push(pcm);
  let pico = 0;
  for (const v of datos) pico = Math.max(pico, Math.abs(v));
  g.recientes = [g.recientes[1], pico];
  g.maximo = Math.max(g.maximo, pico);
  if (!g.soltando) nivel(pico);
}

async function abrirMicro() {
  if (micro) {
    clearTimeout(micro.cierre);
    if (micro.ctx.state === "suspended") await micro.ctx.resume().catch(() => {});
    return micro;
  }
  const stream = await navigator.mediaDevices.getUserMedia({ audio: { echoCancellation: false, noiseSuppression: true, autoGainControl: true, channelCount: 1 } });
  const ctx = new AudioContext();
  const fuente = ctx.createMediaStreamSource(stream);
  const proceso = ctx.createScriptProcessor(2048, 1, 1);
  proceso.onaudioprocess = (e) => alOir(e.inputBuffer.getChannelData(0));
  fuente.connect(proceso);
  proceso.connect(ctx.destination);
  micro = { stream, ctx, proceso, cierre: null };
  return micro;
}

function cerrarMicro() {
  if (!micro || grabando) return;
  clearTimeout(micro.cierre);
  micro.proceso.disconnect();
  micro.stream.getTracks().forEach((t) => t.stop());
  void micro.ctx.close();
  micro = null;
}

function enfriarMicro() {
  if (!micro) return;
  clearTimeout(micro.cierre);
  micro.cierre = setTimeout(cerrarMicro, TIBIO);
}

document.addEventListener("visibilitychange", () => {
  if (document.hidden) cerrarMicro();
});

async function empezarVoz() {
  if (grabando || !enlace) return;
  mic.classList.add("abriendo");
  estado("escuchando", "Abriendo el micrófono…");
  callarTrozos();
  altavoz.pause();
  speechSynthesis?.cancel();
  const enVivo = enlace.sabe("oido-vivo");
  if (enVivo) void enlace.enviar({ tipo: "voz-empieza" }).catch(() => {});
  grabando = { cargando: true, enVivo };
  try {
    const m = await abrirMicro();
    const antes = grabando;
    grabando = { enVivo, bajador: new Bajador(m.ctx.sampleRate), listos: [], porMandar: [], cuantos: 0, largo: 0, recientes: [0, 0], maximo: 0, rate: m.ctx.sampleRate, tope: setTimeout(() => void terminarVoz(false), 55000) };
    if (antes?.soltado) void terminarVoz(antes.soltado === "cancelada");
  } catch {
    grabando = null;
    if (enVivo) void enlace.enviar({ tipo: "voz-cancelada" }).catch(() => {});
    mic.classList.remove("abriendo", "oyendo");
    estado("conectado", `Conectado a ${pcNombre}`);
    dicho("cocuyo error", "No me dejaron usar el micrófono. Dale permiso en los ajustes del navegador.");
  }
}

async function terminarVoz(cancelada) {
  const g = grabando;
  if (g?.cargando) {
    g.soltado = cancelada ? "cancelada" : "lista";
    return;
  }
  if (!g || g.soltando) return;
  mic.classList.remove("abriendo", "oyendo");
  nivel(0);
  if (!cancelada && Math.max(...g.recientes) > g.maximo * 0.15) {
    g.soltando = true;
    await new Promise((listo) => setTimeout(listo, COLA));
  }
  grabando = null;
  clearTimeout(g.tope);
  enfriarMicro();
  if (cancelada || g.largo / g.rate < 0.3) {
    if (g.enVivo) void enlace.enviar({ tipo: "voz-cancelada" }).catch(() => {});
    estado("conectado", `Conectado a ${pcNombre}`);
    return;
  }
  aguardar();
  try {
    const cola = g.bajador.vaciar();
    if (g.enVivo) {
      const resto = juntar([...g.porMandar, cola]);
      if (resto.length) await enlace.enviar({ tipo: "trozo", pcm: new Uint8Array(resto.buffer) });
      await enlace.enviar({ tipo: "voz-termina" });
      return;
    }
    const pcm = juntar([...g.listos, cola]);
    await enlace.enviar({ tipo: "voz", pcm: new Uint8Array(pcm.buffer) });
  } catch (error) {
    listo();
    dicho("cocuyo error", String(error?.message ?? error));
  }
}

mic.addEventListener("pointerdown", (e) => {
  mic.setPointerCapture(e.pointerId);
  void empezarVoz();
});
mic.addEventListener("pointerup", () => void terminarVoz(false));
mic.addEventListener("pointercancel", () => void terminarVoz(true));
mic.addEventListener("contextmenu", (e) => e.preventDefault());

function mostrar(vista) {
  $("emparejar").hidden = vista !== "emparejar";
  $("charla-vista").hidden = vista !== "charla";
  if (vista === "charla") requestAnimationFrame(() => moverLuz($("pestanas").querySelector("button.on")));
}

function saludar(nombre) {
  const h = new Date().getHours();
  $("saludo").textContent = `${t(h < 12 ? "Buenos días" : h < 19 ? "Buenas tardes" : "Buenas noches")}${nombre ? `, ${nombre}` : ""}`;
}

async function arrancarMuestra() {
  pcNombre = "DESKTOP-S0305RJ";
  $("pc-nombre").textContent = pcNombre;
  mostrar("charla");
  saludar("Tkyoxx");
  estado("conectado", `Conectado a ${pcNombre}`);
  const respuestas = {
    "pon algo de salsa": { texto: "Sonando salsa en Spotify, en tu PC.", tarjetas: [] },
    "ponme una alarma a las 6:30": { texto: "Listo, ahí la tienes para ponerla en el celular.", acciones: [{ tipo: "alarma", hora: "06:30", titulo: "Levantarse" }] },
    "busca vuelos a cartagena": { texto: "Te la dejo para abrir aquí.", acciones: [{ tipo: "buscar", texto: "vuelos a Cartagena" }] },
  };
  enlace = {
    async enviar(m) {
      await new Promise((r) => setTimeout(r, 700));
      const r = respuestas[m.texto?.toLowerCase()] ?? { texto: "Hecho en tu PC." };
      recibir({ tipo: "respuesta", ...r });
    },
  };
  if (new URLSearchParams(location.search).get("muestra") === "informe") {
    dicho("yo", "Investiga a fondo si conviene un carro eléctrico en Colombia");
    dicho("cocuyo", "Me pongo a investigar «si conviene un carro eléctrico en Colombia». Cuando acabe te llega aquí el informe.");
    void llegoInforme({
      en: Date.now(),
      titulo: "¿Conviene un carro eléctrico en Colombia?",
      tema: "carro eléctrico en Colombia",
      resumen: "Sale más barato de mantener y no paga pico y placa en varias ciudades [1][2]. El precio de compra sigue siendo más alto [3].",
      puntos: ["Exentos de pico y placa en Bogotá y Medellín [1]", "Arancel del 0 % para importarlos [2]"],
      secciones: [{ titulo: "Costos", texto: "Cargar en casa cuesta menos que la gasolina [3].\n\n- Mantenimiento más simple [2]\n- Seguro algo más caro [3]" }],
      contradicciones: [{ tema: "Autonomía", detalle: "[1] dice 400 km y [3] dice 300 km en uso real.", veredicto: "Creerle a [3]: mide en ciudad [3]." }],
      conclusiones: ["Conviene si manejas en ciudad y puedes cargar en casa [1][3]."],
      limites: "No encontré cifras oficiales de 2026.",
      fuentes: [{ titulo: "Movilidad Bogotá", url: "https://example.com/1", sitio: "example.com" }, { titulo: "DIAN aranceles", url: "https://example.com/2", sitio: "example.com" }, { titulo: "Prueba real", url: "https://example.com/3", sitio: "example.com" }],
      llamadas: 9,
    });
  }
  if (new URLSearchParams(location.search).get("muestra") === "llena") {
    dicho("yo", "¿Qué hay en mi pantalla?");
    dicho("cocuyo", "Tienes abierto Visual Studio Code con el proyecto Cocuyo, y Spotify suena de fondo.");
    dicho("yo", "Ponme una alarma a las 6:30 para el gimnasio");
    dicho("cocuyo", "Listo. Tócala para dejarla en el celular.");
    pintarTarjeta(tarjeta({ tipo: "alarma", hora: "06:30", titulo: "Gimnasio" }));
    dicho("yo", "Busca cómo llegar al estadio", "por voz");
    pintarTarjeta(tarjeta({ tipo: "mapa", texto: "Estadio Atanasio Girardot" }));
  }
}

const instalada = matchMedia("(display-mode: standalone)").matches || navigator.standalone === true;
const ios = /iPhone|iPad|iPod/.test(navigator.userAgent) || (navigator.platform === "MacIntel" && navigator.maxTouchPoints > 1);
let pedirInstalar = null;

function mostrarInstalar() {
  const puede = !instalada && (pedirInstalar || ios);
  for (const el of document.querySelectorAll(".instalar")) el.hidden = !puede || ios;
  for (const el of document.querySelectorAll(".ios-ayuda")) el.hidden = !puede || !ios;
  for (const el of document.querySelectorAll(".instalar-tarjeta")) el.hidden = !puede;
}

addEventListener("beforeinstallprompt", (e) => {
  e.preventDefault();
  pedirInstalar = e;
  mostrarInstalar();
});
addEventListener("appinstalled", () => {
  pedirInstalar = null;
  mostrarInstalar();
});
for (const boton of document.querySelectorAll(".instalar")) {
  boton.addEventListener("click", async () => {
    if (!pedirInstalar) return;
    const pedido = pedirInstalar;
    pedirInstalar = null;
    await pedido.prompt();
    mostrarInstalar();
  });
}

async function arrancar() {
  traducirPagina();
  mostrarInstalar();
  if (muestra) return arrancarMuestra();
  const { abrirEnlace } = await import("./enlace.js");
  const guardado = await abrirEnlace(recibir, (cual, nombre) => {
    if (nombre) pcNombre = nombre;
    $("pc-estado").textContent = t(cual === "conectado" ? "Conectado" : cual === "dormido" ? "Apagado o sin Cocuyo" : cual === "en-otra" ? "Abierto en otra ventana" : "Buscando…");
    if (cual === "conectado") setTimeout(preferencias);
    if (esperando && cual === "conectado") return;
    if (cual === "conectado") estado("conectado", `Conectado a ${pcNombre}`);
    else if (cual === "buscando") estado("buscando", `Buscando a ${pcNombre}…`);
    else if (cual === "dormido") estado("buscando", `${pcNombre} está apagado o sin Cocuyo`);
    else if (cual === "en-otra") estado("buscando", "Abierto en otra ventana");
  });
  $("olvidar").addEventListener("click", async () => {
    if (!confirm(t(`¿Olvidar ${pcNombre}? Tendrás que emparejar otra vez.`))) return;
    await guardado.olvidar();
    location.reload();
  });
  if (!guardado.emparejado) {
    mostrar("emparejar");
    estado("buscando", "Sin emparejar");
    if (guardado.codigo) $("codigo").value = `${guardado.codigo.slice(0, 3)}-${guardado.codigo.slice(3, 6)}-${guardado.codigo.slice(6)}`;
    if (guardado.sinPunto) {
      $("codigo-form").hidden = true;
      $("emparejar-nota").textContent = t("Escanea con la cámara del celular el QR que te muestra Cocuyo en tu PC. Así esta página sabe dónde encontrarlo.");
      return;
    }
    $("codigo-form").addEventListener("submit", async (e) => {
      e.preventDefault();
      const nota = $("emparejar-nota");
      nota.classList.remove("mal");
      nota.textContent = t("Hablando con tu PC…");
      $("codigo-ir").disabled = true;
      try {
        const hecho = await guardado.emparejar($("codigo").value);
        pcNombre = hecho.nombre;
        enlace = hecho.enlace;
        $("pc-nombre").textContent = pcNombre;
        saludar("");
        mostrar("charla");
      } catch (error) {
        nota.classList.add("mal");
        nota.textContent = t(String(error?.message ?? error));
      } finally {
        $("codigo-ir").disabled = false;
      }
    });
    if (guardado.codigo) $("codigo-form").requestSubmit();
    return;
  }
  enlace = guardado.enlace;
  pcNombre = guardado.nombre;
  $("pc-nombre").textContent = pcNombre;
  saludar("");
  mostrar("charla");
}

void arrancar();

if ("serviceWorker" in navigator && !muestra && location.hostname !== "localhost") {
  addEventListener("load", () => void navigator.serviceWorker.register("sw.js", { updateViaCache: "none" }).catch(() => {}), { once: true });
}
