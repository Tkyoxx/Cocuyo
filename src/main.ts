// el arranque de la interfaz

import {
  call,
  captureCommit,
  captureDiscard,
  getSettings,
  on,
  onCaptured,
  onChord,
  onDemo,
  onGuideRequested,
  onKey,
  onKeys,
  onPlace,
  onSelect,
  onSettingsRequested,
  setRegion,
  type Captured,
  type Heard,
  type Place,
  type Shape,
  type Stats,
} from "./bridge";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { Caption, Pill } from "./caption";
import { Guide } from "./guide";
import { setLanguage, tr } from "./i18n";
import { Installer } from "./install";
import { Island } from "./island";
import { Lasso } from "./lasso";
import { Menu } from "./menu";
import { emit, loadExtensions } from "./extensiones";
import { discordPage, settingsPage } from "./pages";
import { Anotador } from "./anotar";
import { NOVEDADES } from "./novedades";
import { Aviso, type Update } from "./version";
import { Burbuja, type Linea } from "./burbuja";
import { Preguntas, SE_PREGUNTA, type Archivo } from "./preguntar";
import { Cards, type Card, type CardButton, type Progress } from "./tarjeta";
import type { RecState } from "./pages";
import { Selector } from "./selector";

const memory = (navigator as { deviceMemory?: number }).deviceMemory ?? 8;
if (navigator.hardwareConcurrency <= 2 || memory <= 2) {
  document.documentElement.dataset.lite = "";
}

const island = new Island();
const lasso = new Lasso();
const selector = new Selector();
const caption = new Caption();
const pill = new Pill();
const guide = new Guide();
const installer = new Installer();
const menu = new Menu();
const addDiscord = () => menu.addPage(discordPage);
window.addEventListener("cocuyo-discord", addDiscord);
void call<{ conectado: boolean }>("discord_estado", undefined, { conectado: false }).then((state) => state.conectado && addDiscord());
const cards = new Cards();
const burbuja = new Burbuja();
burbuja.onFocus = (on) => void call("set_focusable", { on });
window.addEventListener("cocuyo-burbuja", (e) => {
  const what = (e as CustomEvent<{ probar?: boolean; oir?: string; hilo?: Linea[]; adjuntar?: string[] }>).detail;
  if (what.probar) burbuja.probar();
  if (what.adjuntar?.length) void burbuja.attach(what.adjuntar);
  if (what.hilo?.length) burbuja.probarHilo(what.hilo);
  if (what.oir) burbuja.oir(what.oir);
});
const floating = new Set<HTMLElement>();
const floatWatch = new ResizeObserver(() => publish());

let publishing = false;

function publish() {
  if (publishing) return;
  publishing = true;
  queueMicrotask(() => {
    publishing = false;
    publishNow();
  });
}

function publishNow() {
  if (lasso.active || selector.active) return;
  const shapes: Shape[] = [];
  for (const shape of [island.shape(), caption.shape(), pill.shape()]) {
    if (shape) shapes.push(shape);
  }
  shapes.push(...cards.shapes());
  shapes.push(...burbuja.shapes());
  for (const el of floating) {
    const r = el.getBoundingClientRect();
    if (r.width && r.height) shapes.push({ x: r.left, y: r.top, w: r.width, h: r.height, r: 0 });
  }
  if (!recMini.hidden) {
    const r = recMini.getBoundingClientRect();
    shapes.push({ x: r.left, y: r.top, w: r.width, h: r.height, r: r.height / 2 });
  }
  setRegion(shapes);
}

island.onShape = caption.onShape = pill.onShape = cards.onShape = burbuja.onShape = publish;

const islandEl = document.getElementById("island")!;
const recMini = document.getElementById("rec-mini")!;
const recTexts = [document.getElementById("rec-time")!, document.getElementById("rec-mini-time")!];
let recSince = 0;
let recTimer = 0;
let recOn = false;
let placeNow: Place = "top";

function recTick() {
  const total = Math.max(0, Math.floor((Date.now() - recSince) / 1000));
  const [h, m, s] = [Math.floor(total / 3600), Math.floor(total / 60) % 60, total % 60];
  const text = h ? `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}` : `${m}:${String(s).padStart(2, "0")}`;
  for (const el of recTexts) el.textContent = text;
}

function paintRecording(state: RecState) {
  recOn = state.grabando;
  islandEl.toggleAttribute("data-rec", recOn);
  window.clearInterval(recTimer);
  if (recOn) {
    recSince = state.desde || Date.now();
    recTick();
    recTimer = window.setInterval(recTick, 1000);
  }
  const mini = recOn && placeNow === "hidden";
  if (recMini.hidden === mini) {
    recMini.hidden = !mini;
    publish();
  }
}

async function stopRecording(e: Event) {
  e.stopPropagation();
  try {
    await call<string>("grabar_parar");
    caption.say("app", "Grabación guardada.", 3200);
  } catch (error) {
    caption.say("app", String(error), 4200);
  }
}

document.getElementById("rec")!.addEventListener("click", stopRecording);
recMini.addEventListener("click", stopRecording);
on<RecState>("grabadora", paintRecording);
void call<RecState | null>("grabar_estado", undefined, null).then((state) => state && paintRecording(state));
cards.onSay = (text) => caption.say("app", text, 4200);

function closeMenu() {
  if (!menu.open) return;
  menu.hide();
  island.setBase("idle");
  cards.behind = false;
  emit("menu", false);
}

island.onOpen = () => {
  if (guide.open || installer.open) return;
  menu.show();
  cards.behind = true;
  emit("menu", true);
};
island.onLeave = () => {
  if (!menu.typing && !menu.pinned) closeMenu();
};
function reply(text: string, ms?: number) {
  if (text.length <= 260 && !text.includes("\n")) return caption.say("app", text, ms ?? readingTime(text));
  const first = text.split(/(?<=[.!?])\s/)[0].slice(0, 180);
  caption.say("app", `${first}…`, 3600);
  cards.show({ script: "Cocuyo", titulo: "Cocuyo", texto: text, md: true, segundos: 90, botones: [{ texto: "Copiar", copiar: text }] });
}

menu.onAttach = async () => {
  try {
    const file = await call<Dropped | null>("bandeja_elegir");
    if (!file) return;
    closeMenu();
    if (SE_PREGUNTA.includes(file.kind)) await preguntas.start(file);
    else offerFile(file, file.name, 30);
  } catch (error) {
    caption.say("app", String(error), 5200);
  }
};

menu.onAsk = async (text) => {
  caption.say("you", text, 1600);
  island.setState("piensa");
  const waiting = window.setTimeout(() => caption.typing(), 900);
  try {
    const said = await call<string>("orden_escrita", { text });
    clearTimeout(waiting);
    settled(false);
    if (said) reply(said);
    else caption.hide();
  } catch (error) {
    clearTimeout(waiting);
    settled(true);
    caption.say("app", String(error), 5200);
  }
};

menu.onMeter = () => {
  settingsPage.goTo("ia");
  menu.show("ajustes");
};
island.onPeek = async () => {
  const s = await call<Stats | null>("system_stats", undefined, null);
  if (!s || island.base !== "idle") return;
  const memory = s.memoryTotal ? s.memoryUsed / s.memoryTotal : 0;
  const top = s.top[0];
  if (s.cpu >= 85) island.vitals(`${tr("Procesador al")} ${Math.round(s.cpu)} %`, true);
  else if (memory >= 0.9) island.vitals(`${tr("Memoria al")} ${Math.round(memory * 100)} %${top ? ` · ${top.name}` : ""}`, true);
  else island.vitals("", false);
};
menu.onClose = closeMenu;
menu.onSay = (text) => caption.say("app", text);
menu.onSize = (size) => island.setBase("menu", size);
menu.onSettings = (s) => {
  burbuja.setGustos(s);
  if (s.statusLine) document.documentElement.dataset.rayita = "";
  else delete document.documentElement.dataset.rayita;
  if (s.accent) document.documentElement.style.setProperty("--accent", s.accent);
  else document.documentElement.style.removeProperty("--accent");
  menu.setAi(s.aiOn);
  menu.setName(s.name);
  guide.setChords(s.dictationKeys, s.lassoKeys, s.commandKeys);
  setLanguage(s.language);
};
void getSettings().then(menu.onSettings);
onSettingsRequested(() => {
  if (!guide.open && !installer.open) {
    menu.show("ajustes");
    cards.behind = true;
  }
});

installer.onOpen = (size) => {
  menu.hide();
  island.setBase("install", size);
};
installer.onClose = () => {
  island.setBase("idle");
  if (guide.seen) island.setPlace(wanted);
  else setTimeout(() => guide.start(), 450);
};

guide.onOpen = (size) => {
  menu.hide();
  island.setBase("guide", size);
  void call("set_voice_practice", { on: true });
};
guide.onResize = (size) => island.setPanel(size);
guide.onVoice = () =>
  setTimeout(() => {
    settingsPage.goTo("voz");
    menu.show("ajustes");
    cards.behind = true;
    emit("menu", true);
  }, 420);
let wanted: Place = "top";
guide.onClose = () => {
  island.setBase("idle");
  island.setPlace(wanted);
  void call("set_voice_practice", { on: false });
};

onKeys((mask) => {
  guide.keys(mask);
  menu.keys(mask);
});
onKey((vk) => menu.key(vk));
onGuideRequested(() => {
  island.setPlace("top", true);
  guide.start();
});
onPlace((place) => {
  placeNow = place;
  if (recOn) paintRecording({ grabando: true, desde: recSince, repeticion: false, archivo: null });
  wanted = place;
  if (guide.open || installer.open) return;
  closeMenu();
  island.setPlace(place);
});

const anotador = new Anotador();
anotador.onOpen = (size) => {
  if (preguntas.open) preguntas.close();
  closeMenu();
  cards.behind = true;
  island.setBase("anota", size);
};
anotador.onClose = () => {
  island.setBase("idle");
  cards.behind = false;
};
anotador.onSay = (text) => caption.say("app", text, 3600);

type Dropped = Archivo;

const ICON_OF: Record<Dropped["kind"], string> = { imagen: "🖼️", texto: "📄", pdf: "📕", documento: "📘", otro: "📦" };

const preguntas = new Preguntas();
preguntas.onOpen = (size) => {
  if (anotador.open) anotador.close();
  closeMenu();
  cards.behind = true;
  island.setBase("pregunta", size);
};
preguntas.onClose = () => {
  island.setBase("idle");
  cards.behind = false;
};
on<{ archivo: Dropped; pregunta: string; respuesta: string }>("bandeja-abrir", (opened) => preguntas.show(opened.archivo, opened.pregunta, opened.respuesta));

function offerFile(file: Dropped, title: string, seconds: number) {
  const buttons: CardButton[] = [];
  const failed = (error: unknown) => caption.say("app", String(error), 5200);
  if (SE_PREGUNTA.includes(file.kind)) {
    buttons.push({ texto: "¿De qué trata?", accion: () => void preguntas.start(file, tr("¿De qué trata?")).catch(failed) });
    buttons.push({ texto: "Preguntar", accion: () => void preguntas.start(file).catch(failed) });
    buttons.push({
      texto: "Sacar el texto",
      accion: () =>
        void call<string>("bandeja_texto", { path: file.path })
          .then((said) => caption.say("app", said, 3600))
          .catch((error) => caption.say("app", String(error), 5200)),
    });
  }
  if (file.kind === "imagen") buttons.push({ texto: "Anotar", accion: () => void anotador.start(file.path).catch(failed) });
  buttons.push({ texto: "Abrir", abrir: file.path });
  if (file.kind === "otro") buttons.push({ texto: "Copiar la ruta", copiar: file.path });
  cards.show({ script: "Cocuyo", titulo: title, texto: "¿Qué hago con esto?", icono: ICON_OF[file.kind], segundos: seconds, botones: buttons });
}

if ("__TAURI_INTERNALS__" in window) void getCurrentWebview().onDragDropEvent(async (event) => {
  const kind = event.payload.type;
  const at = "position" in event.payload ? { x: event.payload.position.x / devicePixelRatio, y: event.payload.position.y / devicePixelRatio } : null;
  const toChat = !!at && burbuja.takes(at.x, at.y);
  if (kind === "enter" || kind === "over") {
    burbuja.hovering(toChat);
    island.dropping(!toChat);
    return;
  }
  burbuja.hovering(false);
  island.dropping(false);
  if (kind !== "drop" || !event.payload.paths.length) return;
  if (toChat) {
    void burbuja.attach(event.payload.paths);
    return;
  }
  island.gulp();
  try {
    const kept = await call<Dropped[]>("bandeja_recibir", { paths: event.payload.paths });
    offerFile(kept[0], kept.length > 1 ? `${kept[0].name} y ${kept.length - 1} más` : kept[0].name, 30);
  } catch (error) {
    caption.say("app", String(error), 5200);
  }
});

function announce(result: Captured) {
  const size = `${result.w} × ${result.h} px`;
  if (result.error) caption.say("app", `No se pudo guardar la captura: ${result.error}`, 5200);
  else if (result.path) {
    caption.say("app", `Captura guardada${result.copied ? " y copiada" : ""}: ${size}.`);
  }
  else if (result.copied) caption.say("app", `Captura copiada: ${size}.`);
  else caption.say("app", "Captura sin guardar.");
}

onSelect((phase) => {
  if (phase === "cancel") {
    if (selector.active) selector.cancel();
    else captureDiscard();
    return;
  }
  selector.start();
  setRegion(null);
  closeMenu();
  caption.hide();
});
selector.onDone = async (box) => {
  publish();
  if (box) announce(await captureCommit(box));
  else captureDiscard();
};

onCaptured((result) => {
  publish();
  announce(result);
});

onChord(async (e) => {
  if (e.phase === "locked") {
    caption.say("app", "Manos libres. Habla tranquilo: tócalas otra vez para pegar o Esc para cancelar.", 4200);
    return;
  }
  if (e.phase === "start") {
    if (e.chord === "lasso") {
      lasso.start(e.cursor);
      setRegion(null);
    } else {
      pill.show();
    }
    closeMenu();
    caption.hide();
    island.setVoice(true);
    guide.chord(e, true);
    return;
  }

  island.setVoice(false);
  setLevel(0);

  if (e.chord !== "lasso") {
    guide.chord(e, !e.cancelled);
    if (e.cancelled) {
      pill.cancel();
      island.setState(null);
    } else if (guide.open) {
      pill.done();
      island.setState(null);
    } else {
      pill.think();
      island.setState("piensa");
      if (e.chord === "command") typingSoon();
    }
    return;
  }

  const box = await lasso.end(e.cancelled);
  guide.chord(e, box !== null);
  publish();
  if (!box || guide.open) {
    captureDiscard();
    if (!guide.open && !e.cancelled) caption.say("app", "Dibuja un círculo más grande.");
    return;
  }
  const saved = await captureCommit(box);
  if (!e.asking) return announce(saved);
  pill.show();
  pill.think();
  island.setState("piensa");
  try {
    const asked = await call<{ question: string; answer: string; live: boolean } | null>("lasso_ask");
    if (!asked) {
      pill.cancel();
      island.setState(null);
      return announce(saved);
    }
    pill.done();
    island.setState("bien", 1200);
    if (asked.live) {
      if (asked.answer) caption.stream("app", asked.answer, readingTime(asked.answer));
      return;
    }
    caption.say("you", asked.question, 1600);
    setTimeout(() => reply(asked.answer), 1300);
  } catch (error) {
    pill.cancel();
    island.setState("mal", 2600);
    caption.say("app", String(error), 5200);
  }
});

const readingTime = (text: string) => Math.min(22000, Math.max(3400, 1800 + text.length * 55));

function setLevel(peak: number) {
  const level = Math.min(1, Math.sqrt(peak) * 1.7);
  document.documentElement.style.setProperty("--lv", level.toFixed(3));
}
on<number>("level", setLevel);
on<"on" | "pensando" | "nada">("oido-corto", (state) => {
  island.setVoice(state === "on");
  if (state === "on") {
    caption.hide();
    pill.show();
    return;
  }
  setLevel(0);
  if (state === "pensando") pill.think();
  else pill.cancel();
});

on<{ who: "you" | "app"; text: string }>("live-text", (live) => {
  if (!live.text) return;
  if (live.who === "app") {
    clearTimeout(typingTimer);
    pill.done();
    if (island.state === "piensa") island.setState(null);
  }
  caption.stream(live.who, live.text, readingTime(live.text) + 1500);
});

let typingTimer = 0;
function typingSoon() {
  clearTimeout(typingTimer);
  typingTimer = window.setTimeout(() => island.state === "piensa" && caption.typing(), 1200);
}

function settled(error: boolean) {
  clearTimeout(typingTimer);
  if (caption.isTyping) caption.hide();
  island.setState(error ? "mal" : "bien", error ? 2600 : 1200);
}

on<Heard>("heard", (heard) => {
  settled(!!heard.error);
  if (heard.live && !heard.error) {
    pill.done();
    if (heard.reply) caption.stream("app", heard.reply, readingTime(heard.reply));
    else if (heard.text) caption.stream("you", heard.text, 1600);
    return;
  }
  if (heard.error) {
    pill.cancel();
    if (heard.text) caption.say("you", heard.text, 1600);
    setTimeout(() => caption.say("app", heard.error!, 5200), heard.text ? 1300 : 0);
    return;
  }
  pill.done();
  if (heard.chord === "dictation") {
    if (heard.reply) caption.say("app", heard.reply, 4200);
    return;
  }
  caption.say("you", heard.text, 1600);
  if (heard.reply) setTimeout(() => reply(heard.reply), 1300);
});

on<string>("note", (text) => caption.say("app", text, 5200));
cards.onWaiting = (waiting) => {
  if (waiting) island.setState("espera");
  else if (island.state === "espera") island.setState(null);
};
on<Card>("script-card", (card) => cards.show(card));
on<number>("script-card-drop", (id) => cards.dropId(id));
on<Progress>("script-progress", (update) => cards.progress(update));
on("extensions-off", () => location.reload());

void loadExtensions({
  island: document.getElementById("island")!,
  say: (text, ms) => caption.say("app", text, ms ?? readingTime(text)),
  card: (card) => cards.show(card),
  progress: (script, valor, texto) => cards.progress({ script, valor, texto }),
  addPage: (page) => menu.addPage(page),
  openMenu: (id) => {
    menu.show(id);
    cards.behind = true;
    emit("menu", true);
  },
  closeMenu,
  float: (el) => {
    if (!el.isConnected) document.body.append(el);
    floating.add(el);
    floatWatch.observe(el);
    publish();
    return () => {
      floating.delete(el);
      floatWatch.unobserve(el);
      el.remove();
      publish();
    };
  },
}).then(() => {
  let reopen: string | null = null;
  try {
    reopen = sessionStorage.getItem("cocuyo.reabrir");
    sessionStorage.removeItem("cocuyo.reabrir");
  } catch {
    reopen = null;
  }
  if (reopen) {
    settingsPage.goTo(reopen);
    menu.show("ajustes");
    cards.behind = true;
  }
});

const aviso = new Aviso(cards);
on<Update>("update-offer", (found) => aviso.offer(found));
on<{ version: string }>("update", (found) => {
  island.setState("nueva");
  if (!aviso.installing()) caption.say("app", `Actualizando Cocuyo a la ${found.version}. Ya vuelvo.`, 4000);
});
void call<string | null>("update_done", undefined, null).then((version) => {
  if (!version) return;
  setTimeout(() => aviso.welcome(version, NOVEDADES[version] ?? []), 2200);
});
window.addEventListener("cocuyo-aviso", (e) => {
  const what = (e as CustomEvent<{ oferta?: Update; hola?: string; instalando?: boolean }>).detail;
  if (what.oferta) aviso.offer(what.oferta);
  if (what.hola) aviso.welcome(what.hola, NOVEDADES[what.hola] ?? []);
  if (what.instalando) aviso.installing();
});

onDemo((step) => {
  if (step === "cerrar") closeMenu();
  else if (step === "guia") guide.start();
  else if (step === "sin-guia") guide.close();
  else if (step === "instalar-voz") void call("voice_install");
  else if (step === "instalador") installer.start();
  else if (step === "conectar-spotify") void call("spotify_connect");
  else if (step.startsWith("idioma=")) setLanguage(step.slice(7) as "es" | "en" | "pt");
  else if (step === "derecha" || step === "arriba") island.setPlace(step === "derecha" ? "right" : "top");
  else if (step.startsWith("ajustes=")) {
    settingsPage.goTo(step.slice(8));
    menu.show("ajustes");
    cards.behind = true;
  } else {
    menu.show(step);
    cards.behind = true;
  }
});

const writable = (target: EventTarget | null) => target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement;
for (const type of ["keydown", "keyup", "keypress"] as const) {
  window.addEventListener(
    type,
    (e) => {
      if (!writable(e.target) && e.key !== "Escape") e.preventDefault();
    },
    true,
  );
}
window.addEventListener(
  "click",
  (e) => {
    const submitting = e.target instanceof HTMLButtonElement && e.target.type === "submit" && !!e.target.form?.contains(document.activeElement) && writable(document.activeElement);
    if (e.isTrusted && e.detail === 0 && !submitting) {
      e.preventDefault();
      e.stopPropagation();
    }
  },
  true,
);
publish();
void call<string>("install_state", undefined, "portable").then((state) => {
  if (state === "fresh") setTimeout(() => installer.start(), 500);
  else if (!guide.seen) setTimeout(() => guide.start(), 500);
  else setTimeout(() => island.greet(), 700);
});
