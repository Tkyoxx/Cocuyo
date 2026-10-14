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
} from "./bridge";
import { Caption, Pill } from "./caption";
import { Guide } from "./guide";
import { setLanguage } from "./i18n";
import { Installer } from "./install";
import { Island } from "./island";
import { Lasso } from "./lasso";
import { Menu } from "./menu";
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

function publish() {
  if (lasso.active || selector.active) return;
  const shapes: Shape[] = [];
  for (const shape of [island.shape(), caption.shape(), pill.shape()]) {
    if (shape) shapes.push(shape);
  }
  setRegion(shapes);
}

island.onShape = caption.onShape = pill.onShape = publish;

function closeMenu() {
  if (!menu.open) return;
  menu.hide();
  island.setBase("idle");
}

island.onOpen = () => {
  if (guide.open || installer.open) return;
  menu.show();
};
island.onLeave = () => {
  if (!menu.typing) closeMenu();
};
menu.onClose = closeMenu;
menu.onSay = (text) => caption.say("app", text);
menu.onSize = (size) => island.setBase("menu", size);
menu.onSettings = (s) => {
  guide.setChords(s.dictationKeys, s.lassoKeys, s.commandKeys);
  setLanguage(s.language);
};
void getSettings().then(menu.onSettings);
onSettingsRequested(() => {
  if (!guide.open && !installer.open) menu.show("ajustes");
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
  wanted = place;
  if (guide.open || installer.open) return;
  closeMenu();
  island.setPlace(place);
});

function announce(result: Captured) {
  const size = `${result.w} × ${result.h} px`;
  if (result.error) caption.say("app", `No se pudo guardar la captura: ${result.error}`, 5200);
  else if (result.path) caption.say("app", `Captura guardada${result.copied ? " y copiada" : ""}: ${size}.`);
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
  if (e.phase === "start") {
    if (e.chord === "lasso") {
      lasso.start(e.cursor);
      setRegion(null);
    } else pill.show();
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
    if (e.cancelled) pill.cancel();
    else if (guide.open) pill.done();
    else pill.think();
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
  try {
    const asked = await call<{ question: string; answer: string } | null>("lasso_ask");
    if (!asked) {
      pill.cancel();
      return announce(saved);
    }
    pill.done();
    caption.say("you", asked.question, 1600);
    setTimeout(() => caption.say("app", asked.answer, readingTime(asked.answer)), 1300);
  } catch (error) {
    pill.cancel();
    caption.say("app", String(error), 5200);
  }
});

const readingTime = (text: string) => Math.min(22000, Math.max(3400, 1800 + text.length * 55));

function setLevel(peak: number) {
  const level = Math.min(1, Math.sqrt(peak) * 1.7);
  document.documentElement.style.setProperty("--lv", level.toFixed(3));
}
on<number>("level", setLevel);

on<Heard>("heard", (heard) => {
  if (heard.error) {
    pill.cancel();
    if (heard.text) caption.say("you", heard.text, 1600);
    setTimeout(() => caption.say("app", heard.error!, 5200), heard.text ? 1300 : 0);
    return;
  }
  pill.done();
  if (heard.chord === "dictation") return;
  caption.say("you", heard.text, 1600);
  if (heard.reply) setTimeout(() => caption.say("app", heard.reply, readingTime(heard.reply)), 1300);
});

on<string>("note", (text) => caption.say("app", text, 5200));

on<{ version: string }>("update", (found) => caption.say("app", `Actualizando Cocuyo a la ${found.version}. Ya vuelvo.`, 4000));
void call<string | null>("update_done", undefined, null).then((version) => {
  if (version) setTimeout(() => caption.say("app", `Cocuyo se actualizó a la ${version}.`, 5000), 2500);
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
  else menu.show(step);
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
    if (e.isTrusted && e.detail === 0) {
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
});
