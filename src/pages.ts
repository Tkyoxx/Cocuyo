// las pestañas del menu. es largo, si, algun dia lo parto

import { convertFileSrc } from "@tauri-apps/api/core";
import { PERMISOS, type ExtensionInfo, type Permiso } from "./extensiones";
import { locale, tr, weekLetters } from "./i18n";
import { qrSvg } from "./qr";
import {
  call,
  captureKeysAvailable,
  currentCaptureDir,
  getSettings,
  on,
  pickCaptureDir,
  setChordsPaused,
  setKeyEcho,
  setSettings,
  type AppEntry,
  type ClipListing,
  type NowPlaying,
  type Dns,
  type Settings,
  type Speed,
  type SpotifyStatus,
  type Stats,
  type Todo,
  type VoiceProgress,
  type VoiceStatus,
} from "./bridge";
import { countKeys, keyLabel, keysOf } from "./keys";

export interface Host {
  say(text: string): void;
  close(): void;
  focus(input: HTMLInputElement): void;
  settingsChanged(settings: Settings): void;
}

export interface Page {
  id: string;
  label: string;
  w: number;
  h: number;
  html(): string;
  mount?(el: HTMLElement, host: Host): void;
  show?(el: HTMLElement, host: Host): void;
  hide?(): void;
}

const ICONS: Record<string, string> = {
  wifi: "M4 10a12 12 0 0116 0M7 13.5a7.5 7.5 0 0110 0M10 17a3 3 0 014 0",
  bluetooth: "M7 7l10 10-5 4V3l5 4L7 17",
  moon: "M20 14.5A8 8 0 019.5 4 8 8 0 1020 14.5z",
  plane: "M3 13l18-8-6 16-3-7-9-1z",
  cast: "M4 6h16v12h-6M4 14a4 4 0 014 4M4 10a8 8 0 018 8",
  volume: "M4 10v4h3l5 4V6L7 10H4zM16 9a4 4 0 010 6M18.5 6.5a8 8 0 010 11",
  mute: "M4 10v4h3l5 4V6L7 10H4zM16 9.5l5 5M21 9.5l-5 5",
  search: "M11 4a7 7 0 100 14 7 7 0 000-14zM20 20l-4-4",
  mic: "M12 3a3 3 0 00-3 3v5a3 3 0 006 0V6a3 3 0 00-3-3zM6 11a6 6 0 0012 0M12 17v4M9 21h6",
  headset: "M4 15v-3a8 8 0 0116 0v3M4 15a2 2 0 002 2h1v-5H6a2 2 0 00-2 2zM20 15a2 2 0 01-2 2h-1v-5h1a2 2 0 012 2z",
  leave: "M3 14c5-5 13-5 18 0l-2 3-4-1v-3a10 10 0 00-6 0v3l-4 1-2-3z",
  wave: "M3 12h2l2-5 3 10 3-12 3 9 2-2h3",
  echo: "M4 15a8 8 0 0116 0M7 15a5 5 0 0110 0M10 15a2 2 0 014 0",
  hand: "M8 13V6a1.5 1.5 0 013 0v5M11 11V4.5a1.5 1.5 0 013 0V11M14 11V6a1.5 1.5 0 013 0v8a6 6 0 01-6 6h-1a6 6 0 01-5-2.7L3 14a1.5 1.5 0 012.5-1.6L8 15",
  sun: "M12 7a5 5 0 100 10 5 5 0 000-10zM12 2v2M12 20v2M2 12h2M20 12h2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4",
  cloud: "M7 18a4 4 0 01-.5-7.97A5.5 5.5 0 0117.2 11.5 3.3 3.3 0 0117 18H7z",
  rain: "M7 15a4 4 0 01-.5-7.97A5.5 5.5 0 0117.2 8.5 3.3 3.3 0 0117 15H7zM8 18l-1 3M12 18l-1 3M16 18l-1 3",
  storm: "M7 15a4 4 0 01-.5-7.97A5.5 5.5 0 0117.2 8.5 3.3 3.3 0 0117 15H7zM13 16l-2.5 3.5h3L11 23",
  snow: "M7 15a4 4 0 01-.5-7.97A5.5 5.5 0 0117.2 8.5 3.3 3.3 0 0117 15H7zM8 19v.01M12 19v.01M16 19v.01M10 22v.01M14 22v.01",
  fog: "M4 8h16M4 12h16M6 16h12M8 20h8",
  lock: "M6 11h12v9H6zM8.5 11V8a3.5 3.5 0 017 0v3",
  restart: "M20 12a8 8 0 11-2.6-5.9M20 4v5h-5",
  power: "M12 3v9M6.3 6.3a8 8 0 1011.4 0",
  text: "M5 6h14M5 12h14M5 18h9",
  folder: "M3 7h6l2 2h10v10H3z",
  plus: "M12 5v14M5 12h14",
  close: "M6 6l12 12M18 6L6 18",
  image: "M4 5h16v14H4zM4 16l5-5 4 4 3-3 4 4",
  down: "M12 4v12M6 11l6 6 6-6M5 20h14",
  up: "M12 20V8M6 13l6-6 6 6M5 4h14",
};
const SOLID: Record<string, string> = {
  play: "M8 5v14l11-7z",
  pause: "M7 5h4v14H7zM13 5h4v14h-4z",
  prev: "M6 5h2v14H6zM20 5v14l-10-7z",
  next: "M16 5h2v14h-2zM4 5v14l10-7z",
};

const ic = (name: string) =>
  name in SOLID
    ? `<svg class="ic solid" viewBox="0 0 24 24"><path d="${SOLID[name]}"/></svg>`
    : `<svg class="ic" viewBox="0 0 24 24"><path d="${ICONS[name]}"/></svg>`;

const esc = (text: string) =>
  text.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);

const gb = (bytes: number) => (bytes / 2 ** 30).toFixed(1).replace(".", ",");

const SHORTCUTS: [string, string, string][] = [
  ["wifi", "Wi‑Fi", "network-wifi"],
  ["bluetooth", "Bluetooth", "bluetooth"],
  ["moon", "No molestar", "quiethours"],
  ["plane", "Modo avión", "network-airplanemode"],
];

const controles: Page = {
  id: "controles",
  label: "Controles",
  w: 700,
  h: 260,
  html: () =>
    SHORTCUTS.map(
      ([icon, title, page]) =>
        `<button class="tile" data-page-link="${page}"><span class="ti">${ic(icon)}</span><span><b>${title}</b><small>Abrir ajustes</small></span></button>`,
    ).join("") +
    `<button class="tile" id="c-mic"><span class="ti">${ic("mic")}</span><span><b>Micrófono</b><small></small></span></button>
     <button class="tile" id="c-mute"><span class="ti">${ic("mute")}</span><span><b>Silencio</b><small></small></span></button>
     <div class="vslider" id="c-vol"><span class="fill"></span><em></em>${ic("volume")}</div>
     <div class="card media">
       <span class="cover" id="m-art"></span>
       <span class="meta"><b id="m-title"></b><small id="m-artist"></small></span>
       <span class="progress"><i id="m-bar"></i></span>
       <span class="times"><span id="m-pos"></span><span id="m-dur"></span></span>
       <span class="transport"><button data-media="previous">${ic("prev")}</button><button class="play" data-media="play" id="m-play">${ic("play")}</button><button data-media="next">${ic("next")}</button></span>
     </div>`,
  mount(el, host) {
    for (const tile of el.querySelectorAll<HTMLElement>("[data-page-link]")) {
      tile.addEventListener("click", () => {
        void call("open_settings_page", { page: tile.dataset.pageLink });
        host.close();
      });
    }
    for (const button of el.querySelectorAll<HTMLElement>("[data-media]")) {
      button.addEventListener("click", async () => {
        await call("media_key", { action: button.dataset.media });
        setTimeout(() => void paintMedia(el), 350);
      });
    }

    const mic = el.querySelector<HTMLElement>("#c-mic")!;
    mic.addEventListener("click", async () => {
      await call("mic_mute", { on: !mic.classList.contains("on") }).catch((error) => host.say(String(error)));
      void paintMic(el);
    });

    const mute = el.querySelector<HTMLElement>("#c-mute")!;
    mute.addEventListener("click", async () => {
      const muted = !mute.classList.contains("on");
      await call("volume_set", { muted });
      paintVolume(el);
    });

    const track = el.querySelector<HTMLElement>(".progress")!;
    const fill = el.querySelector<HTMLElement>("#m-bar")!;
    const point = (e: PointerEvent) => {
      const r = track.getBoundingClientRect();
      return Math.min(1, Math.max(0, (e.clientX - r.left) / r.width));
    };
    const preview = (e: PointerEvent) => {
      fill.style.transition = "none";
      fill.style.width = `${point(e) * 100}%`;
      el.querySelector("#m-pos")!.textContent = clock(point(e) * mediaLength);
    };
    track.addEventListener("pointerdown", (e) => {
      if (!mediaLength) return;
      track.setPointerCapture(e.pointerId);
      seeking = true;
      track.classList.add("drag");
      preview(e);
    });
    track.addEventListener("pointermove", (e) => {
      if (seeking) preview(e);
    });
    const release = async (e: PointerEvent) => {
      if (!seeking) return;
      track.classList.remove("drag");
      await call("media_seek", { seconds: point(e) * mediaLength });
      seeking = false;
      setTimeout(() => void paintMedia(el), 400);
    };
    track.addEventListener("pointerup", (e) => void release(e));
    track.addEventListener("pointercancel", () => {
      seeking = false;
      track.classList.remove("drag");
    });

    const slider = el.querySelector<HTMLElement>("#c-vol")!;
    const drag = (e: PointerEvent) => {
      const r = slider.getBoundingClientRect();
      const level = Math.min(1, Math.max(0, 1 - (e.clientY - r.top) / r.height));
      setVolumeUi(el, level);
      void call("volume_set", { level });
    };
    slider.addEventListener("pointerdown", (e) => {
      slider.setPointerCapture(e.pointerId);
      drag(e);
    });
    slider.addEventListener("pointermove", (e) => {
      if (slider.hasPointerCapture(e.pointerId)) drag(e);
    });
  },
  show(el) {
    void paintVolume(el);
    void paintMedia(el);
    mediaTimer = window.setInterval(() => void paintMedia(el), 2000);
  },
  hide: () => clearInterval(mediaTimer),
};

let mediaTimer = 0;
let mediaKey: string | null = null;
let mediaLength = 0;
let seeking = false;

const clock = (seconds: number) => `${Math.floor(seconds / 60)}:${String(Math.floor(seconds % 60)).padStart(2, "0")}`;

async function paintMedia(el: HTMLElement) {
  const now = await call<NowPlaying | null>("media_now", { known: mediaKey }, null);
  const q = (id: string) => el.querySelector<HTMLElement>(`#${id}`)!;
  mediaLength = now?.duration ?? 0;
  if (seeking) return;
  if (!now) {
    mediaKey = null;
    q("m-title").textContent = "Nada sonando";
    q("m-artist").textContent = "Abre Spotify o cualquier reproductor";
    q("m-art").style.backgroundImage = "";
    q("m-bar").style.width = "0";
    q("m-pos").textContent = q("m-dur").textContent = "";
    q("m-play").innerHTML = ic("play");
    return;
  }
  q("m-title").textContent = now.title;
  q("m-artist").textContent = [now.artist, now.source].filter(Boolean).join(" · ");
  if (now.key !== mediaKey) {
    mediaKey = now.key;
    q("m-art").style.backgroundImage = now.art ? `url(${now.art})` : "";
  }
  const bar = q("m-bar");
  const share = (seconds: number) => (now.duration ? `${Math.min(100, (seconds / now.duration) * 100)}%` : "0");
  bar.style.transition = "none";
  bar.style.width = share(now.position);
  if (now.playing && now.duration) {
    void bar.offsetWidth;
    bar.style.transition = "width 2s linear";
    bar.style.width = share(now.position + 2);
  }
  q("m-pos").textContent = now.duration ? clock(now.position) : "";
  q("m-dur").textContent = now.duration ? clock(now.duration) : "";
  q("m-play").innerHTML = ic(now.playing ? "pause" : "play");
}

function setVolumeUi(el: HTMLElement, level: number) {
  const slider = el.querySelector<HTMLElement>("#c-vol")!;
  slider.style.setProperty("--v", String(level));
  slider.querySelector("em")!.textContent = String(Math.round(level * 100));
}

async function paintVolume(el: HTMLElement) {
  const volume = await call<{ level: number; muted: boolean } | null>("volume_get", undefined, { level: 0.5, muted: false });
  if (!volume) return;
  setVolumeUi(el, volume.level);
  const mute = el.querySelector<HTMLElement>("#c-mute")!;
  mute.classList.toggle("on", volume.muted);
  mute.querySelector("small")!.textContent = volume.muted ? "Activado" : "Apagado";
}

async function paintMic(el: HTMLElement) {
  const muted = await call<boolean | null>("mic_muted", undefined, null);
  const mic = el.querySelector<HTMLElement>("#c-mic")!;
  mic.classList.toggle("on", muted === true);
  mic.querySelector("small")!.textContent = muted === null ? "Sin micrófono" : muted ? "Silenciado" : "Abierto";
}

let clockTimer = 0;
let who = "";
let todos: Todo[] = [];
let planned: { id: number; what: string; act: boolean; when: string }[] = [];

function paintClock(el: HTMLElement) {
  const now = new Date();
  const hh = String(now.getHours()).padStart(2, "0");
  const mm = String(now.getMinutes()).padStart(2, "0");
  el.querySelector("#clock-time")!.innerHTML = `${hh}<i>:</i>${mm}`;
  el.querySelector("#clock-date")!.textContent = now.toLocaleDateString(locale(), { weekday: "long", day: "numeric", month: "long" });
  const greeting = now.getHours() < 5 ? "Buenas noches" : now.getHours() < 12 ? "Buenos días" : now.getHours() < 19 ? "Buenas tardes" : "Buenas noches";
  el.querySelector("#clock-hi")!.textContent = who ? `${tr(greeting)}, ${who}` : greeting;

  const monday = new Date(now);
  monday.setDate(now.getDate() - ((now.getDay() + 6) % 7));
  const jan4 = new Date(now.getFullYear(), 0, 4);
  const week = 1 + Math.round((monday.getTime() - jan4.getTime()) / 6048e5 + ((jan4.getDay() + 6) % 7) / 7);
  el.querySelector("#clock-week")!.textContent = `Semana ${week}`;
  el.querySelector("#week")!.innerHTML = [...weekLetters()]
    .map((letter, i) => {
      const day = new Date(monday);
      day.setDate(monday.getDate() + i);
      return `<span class="${day.toDateString() === now.toDateString() ? "today" : ""}">${letter}<b>${day.getDate()}</b></span>`;
    })
    .join("");

  clockTimer = window.setTimeout(() => paintClock(el), 60000 - (now.getSeconds() * 1000 + now.getMilliseconds()) + 50);
}

interface Weather {
  place: string;
  temp: number;
  feels: number;
  code: number;
  day: boolean;
  max: number;
  min: number;
  rain: number;
  hours: { hour: number; temp: number; rain: number; code: number }[];
  text: string;
}

const skyIcon = (code: number, day = true) =>
  code <= 1 ? (day ? "sun" : "moon") : code <= 3 ? "cloud" : code <= 48 ? "fog" : code <= 67 || (code >= 80 && code <= 82) ? "rain" : code <= 86 ? "snow" : "storm";

function paintWeather(el: HTMLElement, weather: Weather | null | undefined, problem = "") {
  const q = <T extends HTMLElement>(sel: string) => el.querySelector<T>(sel)!;
  const asking = !weather;
  q("#wx").classList.toggle("asking", asking);
  q("#wx-note").textContent = asking ? problem || "Escribe tu ciudad y pulsa Enter." : "";
  if (!weather) return;
  const deg = (value: number) => `${Math.round(value)}°`;
  q("#wx-place").textContent = weather.place;
  q("#wx-ic").innerHTML = ic(skyIcon(weather.code, weather.day));
  q("#wx-temp").textContent = deg(weather.temp);
  q("#wx-desc").textContent = weather.text;
  q("#wx-feels").textContent = `Sensación de ${deg(weather.feels)}`;
  q("#wx-day").innerHTML = `<span>${ic("up")} ${deg(weather.max)}</span><span>${ic("down")} ${deg(weather.min)}</span><span>${ic("rain")} ${weather.rain} %</span>`;
  q("#wx-hours").innerHTML = weather.hours
    .map((hour) => `<div><small>${String(hour.hour).padStart(2, "0")}:00</small>${ic(skyIcon(hour.code, hour.hour >= 6 && hour.hour < 19))}<b>${deg(hour.temp)}</b><small>${hour.rain} %</small></div>`)
    .join("");
}

function paintTodos(el: HTMLElement) {
  const done = todos.filter((t) => t.done).length;
  el.querySelector("#todo-count")!.textContent = [todos.length ? `${done}/${todos.length}` : "", planned.length ? `${planned.length} con hora` : ""]
    .filter(Boolean)
    .join(" · ");
  const timed = planned
    .map(
      (plan) =>
        `<div class="todo-row plan"><span class="what"><i>${esc(plan.when)}</i>${esc(plan.act ? `«${plan.what}»` : plan.what)}</span><button class="x" data-plan="${plan.id}" aria-label="Quitar">${ic("close")}</button></div>`,
    )
    .join("");
  const loose = todos
    .map(
      (t, i) =>
        `<div class="todo-row ${t.done ? "done" : ""}"><button data-toggle="${i}">${esc(t.text)}</button><button class="x" data-remove="${i}" aria-label="Quitar">${ic("close")}</button></div>`,
    )
    .join("");
  el.querySelector("#todo-list")!.innerHTML = timed || loose ? timed + loose : `<small>Nada pendiente. Escribe una tarea, o algo con hora: «a las 6 abre Spotify».</small>`;
}

const hoy: Page = {
  id: "hoy",
  label: "Hoy",
  w: 740,
  h: 330,
  html: () =>
    `<div class="card clock"><span class="lbl" id="clock-hi">Ahora</span><div class="time" id="clock-time"></div><div class="date" id="clock-date"></div><small class="foot" id="clock-week"></small></div>
     <div class="card wx" id="wx">
       <header><span class="lbl">Clima</span><button class="wx-place" id="wx-place" title="Cambiar de ciudad"></button></header>
       <div class="wx-now"><span class="wx-ic" id="wx-ic"></span><b id="wx-temp"></b><div><span id="wx-desc"></span><small id="wx-feels"></small></div></div>
       <div class="wx-day" id="wx-day"></div>
       <div class="wx-hours" id="wx-hours"></div>
       <input id="wx-city" placeholder="Tu ciudad…" maxlength="60" spellcheck="false" />
       <small class="wx-note" id="wx-note"></small>
     </div>
     <div class="daycol">
       <div class="card week" id="week"></div>
       <div class="card todo"><header><span class="lbl">Tareas</span><small id="todo-count"></small></header>
         <div id="todo-list"></div>
         <input id="todo-add" placeholder="Tarea, o «en 10 minutos recuérdame…»" maxlength="120" />
       </div>
     </div>`,
  mount(el, host) {
    const city = el.querySelector<HTMLInputElement>("#wx-city")!;
    city.addEventListener("pointerdown", () => host.focus(city));
    city.addEventListener("keydown", async (e) => {
      if (e.key !== "Enter" || !city.value.trim()) return;
      el.querySelector("#wx-note")!.textContent = "Buscando…";
      try {
        paintWeather(el, await call<Weather>("weather_city", { city: city.value }));
        city.value = "";
      } catch (error) {
        paintWeather(el, null, String(error));
      }
    });
    el.querySelector("#wx-place")!.addEventListener("click", () => {
      paintWeather(el, null);
      host.focus(city);
    });
    const save = () => {
      paintTodos(el);
      void call("todos_set", { todos });
    };
    el.querySelector("#todo-list")!.addEventListener("click", (e) => {
      const target = (e.target as HTMLElement).closest<HTMLElement>("[data-toggle],[data-remove],[data-plan]");
      if (!target) return;
      if (target.dataset.plan) {
        const id = Number(target.dataset.plan);
        planned = planned.filter((plan) => plan.id !== id);
        paintTodos(el);
        return void call("agenda_remove", { id });
      }
      if (target.dataset.remove) todos.splice(Number(target.dataset.remove), 1);
      else todos[Number(target.dataset.toggle)].done = !todos[Number(target.dataset.toggle)].done;
      save();
    });
    const input = el.querySelector<HTMLInputElement>("#todo-add")!;
    input.addEventListener("pointerdown", () => host.focus(input));
    input.addEventListener("keydown", async (e) => {
      const text = input.value.trim();
      if (e.key !== "Enter" || !text) return;
      input.value = "";
      const scheduled = await call<string | null>("agenda_add", { text }, null);
      if (scheduled) return host.say(scheduled);
      todos.push({ text, done: false });
      save();
    });
    on("agenda", async () => {
      planned = await call<typeof planned>("agenda_list", undefined, []);
      paintTodos(el);
    });
  },
  async show(el) {
    who = (await getSettings()).name.trim();
    paintClock(el);
    void call<Weather>("weather")
      .then((weather) => paintWeather(el, weather))
      .catch((error) => {
        const why = String(error);
        paintWeather(el, null, why.startsWith("Escribe") ? "" : why);
      });
    todos = await call<Todo[]>("todos_get", undefined, []);
    planned = await call<typeof planned>("agenda_list", undefined, []);
    paintTodos(el);
  },
  hide: () => clearTimeout(clockTimer),
};

let statsTimer = 0;

const gauge = (id: string, color: string, title: string) =>
  `<div class="card gauge" id="${id}" style="--p:0;--c:${color}">
     <svg viewBox="0 0 68 68"><circle class="track" cx="34" cy="34" r="26"/><circle class="arc" cx="34" cy="34" r="26"/></svg>
     <span class="pct"></span><b>${title}</b><small></small>
   </div>`;

const mini = (id: string, label: string) => `<div class="card mini"><span class="lbl">${label}</span><b id="${id}"></b></div>`;

function paintGauge(el: HTMLElement, id: string, fraction: number, sub: string) {
  const card = el.querySelector<HTMLElement>(`#${id}`)!;
  card.style.setProperty("--p", String(fraction));
  card.querySelector(".pct")!.textContent = `${Math.round(fraction * 100)}%`;
  card.querySelector("small")!.textContent = sub;
}

async function paintStats(el: HTMLElement) {
  const s = await call<Stats | null>("system_stats", undefined, null);
  if (!s) return;
  paintGauge(el, "g-cpu", s.cpu / 100, `${s.threads} hilos`);
  paintGauge(el, "g-ram", s.memoryUsed / s.memoryTotal, `${gb(s.memoryUsed)} de ${gb(s.memoryTotal)} GB`);
  paintGauge(el, "g-disk", s.diskTotal ? s.diskUsed / s.diskTotal : 0, "Disco C:");
  el.querySelector("#m-ram")!.textContent = `${gb(s.memoryTotal - s.memoryUsed)} GB`;
  el.querySelector("#m-disk")!.textContent = `${gb(s.diskTotal - s.diskUsed)} GB`;
  const hours = Math.floor(s.uptimeSecs / 3600);
  el.querySelector("#m-up")!.textContent = `${hours} h ${Math.floor((s.uptimeSecs % 3600) / 60)} min`;
  paintTop(el, s.top);
}

function paintTop(el: HTMLElement, top: Stats["top"]) {
  const most = top[0]?.bytes ?? 1;
  el.querySelector("#top-rows")!.innerHTML = top
    .map(({ name, bytes, path }) => {
      const known = programs.get(path);
      if (path && known === undefined) {
        programs.set(path, null);
        void call<Program | null>("program_info", { path }, null).then((info) => {
          if (!info?.icon) {
            programs.delete(path);
            if (!info) return;
          }
          if (info.icon) programs.set(path, info);
          paintTop(el, top);
        });
      }
      const icon = known?.icon ? `<img src="${known.icon}" alt="" />` : `<em style="background:${tint(name)}">${esc(name[0]?.toUpperCase() ?? "")}</em>`;
      return `<div class="row">${icon}<span>${esc(known?.name ?? name)}</span><i style="--p:${bytes / most}"></i><small>${gb(bytes)} GB</small></div>`;
    })
    .join("");
}

interface Program {
  name: string | null;
  icon: string | null;
}
const programs = new Map<string, Program | null>();

const equipo: Page = {
  id: "equipo",
  label: "Equipo",
  w: 760,
  h: 294,
  html: () =>
    gauge("g-cpu", "#f2a23a", "CPU") +
    gauge("g-ram", "#8ed46a", "RAM") +
    gauge("g-disk", "#b48cf0", "Disco") +
    `<div class="sidecol"><div class="card top"><header><span class="lbl">Lo que más memoria ocupa</span></header><div id="top-rows"></div></div></div>` +
    mini("m-ram", "Memoria libre") +
    mini("m-disk", "Libre en C:") +
    mini("m-up", "Encendido hace"),
  show(el) {
    void paintStats(el).then(() => setTimeout(() => void paintStats(el), 500));
    statsTimer = window.setInterval(() => void paintStats(el), 2000);
  },
  hide: () => clearInterval(statsTimer),
};

let dnsList: Dns[] = [];
let dnsBusy = false;

const mbps = (value: number | null) => (value === null ? "—" : value >= 100 ? value.toFixed(0) : value.toFixed(1));

function paintSpeed(el: HTMLElement, speed: Speed) {
  const q = (id: string) => el.querySelector<HTMLElement>(`#${id}`)!;
  q("net-down").textContent = mbps(speed.down);
  q("net-up").textContent = mbps(speed.up);
  q("net-ping").textContent = speed.pingMs === null ? "—" : `${Math.round(speed.pingMs)} ms`;
  const card = q("net-speed");
  card.dataset.phase = speed.phase;
  const shown = speed.phase === "up" ? speed.up : speed.down;
  card.style.setProperty("--p", String(shown ? Math.min(1, Math.log10(1 + shown) / 3) : 0));
  q("net-note").textContent =
    speed.error ??
    { ping: "Buscando el servidor…", down: "Midiendo la bajada…", up: "Midiendo la subida…", done: "Medido contra los servidores de Cloudflare." }[speed.phase];
  (q("net-go") as HTMLButtonElement).disabled = speed.phase !== "done";
  q("net-go").textContent = speed.phase === "done" ? "Medir otra vez" : "Midiendo…";
}

function paintDns(el: HTMLElement, note = "") {
  const slowest = Math.max(1, ...dnsList.map((d) => d.ms ?? 0));
  el.querySelector("#dns-rows")!.innerHTML = dnsList.length
    ? dnsList
        .map(
          (d) =>
            `<div class="row${d.current ? " on" : ""}"><span><b>${esc(d.name)}</b><small>${d.address}${d.current ? " · en uso" : ""}</small></span><i style="--p:${
              d.ms === null ? 0 : d.ms / slowest
            }"></i><small>${d.ms === null ? "sin respuesta" : `${Math.round(d.ms)} ms`}</small></div>`,
        )
        .join("")
    : `<small class="empty">${dnsBusy ? "Preguntándole a cada servidor…" : "Sin medir todavía."}</small>`;
  const best = dnsList.find((d) => d.provider !== null && d.ms !== null);
  const current = dnsList.find((d) => d.current);
  el.querySelector("#dns-note")!.textContent =
    note ||
    (dnsBusy
      ? "Midiendo…"
      : best && current && best !== current && current.ms !== null
        ? `${best.name} te responde ${Math.round(current.ms - best.ms!)} ms antes.`
        : best && best === current
          ? "Ya usas el más rápido."
          : "");
  el.querySelector<HTMLButtonElement>("#dns-best")!.disabled = dnsBusy || !best || best === current;
}

async function measureDns(el: HTMLElement) {
  if (dnsBusy) return;
  dnsBusy = true;
  paintDns(el);
  dnsList = await call<Dns[]>("dns_measure", undefined, []);
  dnsBusy = false;
  paintDns(el);
}

const red: Page = {
  id: "red",
  label: "Red",
  w: 760,
  h: 356,
  html: () =>
    `<div class="card netspeed" id="net-speed" style="--p:0">
       <span class="lbl">Velocidad</span>
       <div class="dial"><svg viewBox="0 0 120 120"><circle class="track" cx="60" cy="60" r="50"/><circle class="arc" cx="60" cy="60" r="50"/></svg><b id="net-down">—</b><small>Mbps de bajada</small></div>
       <div class="pair"><span>${ic("up")}<small>Subida</small><b id="net-up">—</b></span><span>${ic("wifi")}<small>Ping</small><b id="net-ping">—</b></span></div>
       <button class="ghost" id="net-go">Medir</button>
       <small id="net-note">Gasta datos: hasta unos 100 MB en conexiones rápidas.</small>
     </div>
     <div class="card dns">
       <header><span class="lbl">Servidores DNS</span><small id="dns-note"></small></header>
       <div id="dns-rows"></div>
       <footer><button class="ghost" id="dns-best">Usar el más rápido</button><button class="ghost" id="dns-auto">Automático</button><button class="ghost" id="dns-again">Medir</button></footer>
     </div>`,
  mount(el, host) {
    on<Speed>("net-speed", (speed) => paintSpeed(el, speed));
    el.querySelector("#net-go")!.addEventListener("click", () => {
      paintSpeed(el, { phase: "ping", pingMs: null, down: null, up: null, error: null });
      void call("net_speed");
    });
    el.querySelector("#dns-again")!.addEventListener("click", () => void measureDns(el));
    const change = async (command: string, args?: Record<string, unknown>) => {
      try {
        host.say(await call<string>(command, args, ""));
        paintDns(el, "Esperando a Windows…");
        setTimeout(() => void measureDns(el), 3500);
      } catch (error) {
        host.say(String(error));
      }
    };
    el.querySelector("#dns-best")!.addEventListener("click", () => {
      const best = dnsList.find((d) => d.provider !== null && d.ms !== null);
      if (best) void change("dns_apply", { provider: best.provider });
    });
    el.querySelector("#dns-auto")!.addEventListener("click", () => void change("dns_reset"));
  },
  show(el) {
    if (!dnsList.length) void measureDns(el);
    else paintDns(el);
  },
};

let clips: ClipListing = { now: 0, clips: [] };

function ago(ms: number) {
  const min = Math.floor(ms / 60000);
  if (min < 1) return "ahora";
  if (min < 60) return `hace ${min} min`;
  return `hace ${Math.floor(min / 60)} h`;
}

function paintClips(el: HTMLElement, filter = "") {
  const needle = filter.toLowerCase();
  const rows = clips.clips
    .map((c, i) => ({ c, i }))
    .filter(({ c }) => c.text.toLowerCase().includes(needle))
    .map(
      ({ c, i }) =>
        `<button class="clip" data-clip="${i}">${
          c.image ? `<span class="ti shot" style="background-image:url(${c.image.thumb})"></span>` : `<span class="ti">${ic("text")}</span>`
        }<span>${esc(c.text.slice(0, 80))}</span><small>${ago(clips.now - c.at)}</small></button>`,
    );
  el.querySelector("#clip-list")!.innerHTML = rows.join("") || `<small class="empty">${clips.clips.length ? "Nada coincide." : "Lo que copies aparecerá aquí."}</small>`;
}

const portapapeles: Page = {
  id: "portapapeles",
  label: "Portapapeles",
  w: 680,
  h: 380,
  html: () =>
    `<label class="search">${ic("search")}<input id="clip-search" placeholder="Buscar en lo que copiaste…" /></label>
     <div class="cliplist" id="clip-list"></div>
     <div class="card preview"><span class="lbl">Vista previa</span><p id="clip-preview">Toca un elemento para dejarlo listo para pegar.</p><img id="clip-image" alt="" hidden /><button class="ghost" id="clip-text" hidden>Copiar el texto de la imagen</button></div>
     <div class="hint"><span>Texto e imágenes, solo en memoria: se borra al cerrar Cocuyo.</span><button class="ghost" id="clip-clear">Borrar todo</button></div>`,
  mount(el, host) {
    const search = el.querySelector<HTMLInputElement>("#clip-search")!;
    search.addEventListener("pointerdown", () => host.focus(search));
    search.addEventListener("input", () => paintClips(el, search.value));
    el.querySelector("#clip-list")!.addEventListener("click", async (e) => {
      const row = (e.target as HTMLElement).closest<HTMLElement>("[data-clip]");
      if (!row) return;
      const index = Number(row.dataset.clip);
      await call("clip_use", { index });
      for (const other of el.querySelectorAll(".clip")) other.classList.toggle("on", other === row);
      const picked = clips.clips[index];
      const picture = el.querySelector<HTMLImageElement>("#clip-image")!;
      el.querySelector("#clip-preview")!.textContent = picked.text.slice(0, 600);
      picture.hidden = !picked.image;
      if (picked.image) picture.src = picked.image.thumb;
      const reader = el.querySelector<HTMLElement>("#clip-text")!;
      reader.hidden = !picked.image;
      reader.dataset.index = String(index);
      host.say("Listo para pegar con Ctrl + V.");
    });
    el.querySelector<HTMLElement>("#clip-text")!.addEventListener("click", async (e) => {
      const index = Number((e.currentTarget as HTMLElement).dataset.index);
      try {
        host.say(await call<string>("ocr_clip", { index }));
      } catch (error) {
        host.say(String(error));
      }
    });
    el.querySelector("#clip-clear")!.addEventListener("click", async () => {
      await call("clip_clear");
      clips = { now: 0, clips: [] };
      el.querySelector<HTMLImageElement>("#clip-image")!.hidden = true;
      el.querySelector<HTMLElement>("#clip-text")!.hidden = true;
      paintClips(el);
    });
  },
  async show(el) {
    clips = await call<ClipListing>("clip_list", undefined, { now: 0, clips: [] });
    paintClips(el, el.querySelector<HTMLInputElement>("#clip-search")!.value);
  },
};

let apps: AppEntry[] = [];
const icons = new Map<string, string | null>();
let iconWatcher: IntersectionObserver | null = null;

const HUES = [150, 210, 20, 265, 40, 190, 330];
const tint = (name: string) => `hsl(${HUES[name.charCodeAt(0) % HUES.length]} 45% 38%)`;

function paintApps(el: HTMLElement, filter = "") {
  const needle = filter.trim().toLowerCase();
  const shown = apps.filter((a) => a.name.toLowerCase().includes(needle));
  const list = el.querySelector<HTMLElement>("#app-list")!;
  list.innerHTML =
    shown
      .map((a) => {
        const icon = icons.get(a.id);
        const picture = icon ? `<img src="${icon}" alt="" />` : esc(a.name[0]?.toUpperCase() ?? "?");
        return `<div class="app" data-app="${esc(a.id)}"><button class="open"><span class="ti" style="--c:${tint(a.name)}">${picture}</span><span>${esc(a.name)}</span><small>${a.custom ? "Añadida por ti" : ""}</small></button>${a.custom ? `<button class="x" data-remove aria-label="Quitar">${ic("close")}</button>` : ""}</div>`;
      })
      .join("") || `<small class="empty">Ninguna app se llama así. Si no aparece, añádela.</small>`;

  iconWatcher?.disconnect();
  iconWatcher = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        const row = entry.target as HTMLElement;
        const id = row.dataset.app!;
        if (!entry.isIntersecting || icons.has(id)) continue;
        icons.set(id, null);
        void call<string | null>("app_icon", { id }, null).then((src) => {
          icons.set(id, src);
          if (src && row.isConnected) row.querySelector(".ti")!.innerHTML = `<img src="${src}" alt="" />`;
        });
      }
    },
    { root: list },
  );
  for (const row of list.querySelectorAll(".app")) iconWatcher.observe(row);
}

const appsPage: Page = {
  id: "apps",
  label: "Apps",
  w: 660,
  h: 470,
  html: () =>
    `<label class="search">${ic("search")}<input id="app-search" placeholder="Buscar una app…" /><kbd>Enter abre la primera</kbd></label>
     <div class="applist" id="app-list"></div>
     <div class="hint"><span>¿Falta un juego o un programa? Añádelo y también podrás abrirlo por voz.</span><button class="ghost" id="app-add">${ic("plus")} Añadir app</button></div>`,
  mount(el, host) {
    const search = el.querySelector<HTMLInputElement>("#app-search")!;
    const open = async (id: string) => {
      try {
        await call("app_launch", { id });
        host.close();
      } catch (error) {
        host.say(String(error));
      }
    };
    search.addEventListener("pointerdown", () => host.focus(search));
    search.addEventListener("input", () => paintApps(el, search.value));
    search.addEventListener("keydown", (e) => {
      const first = el.querySelector<HTMLElement>(".app");
      if (e.key === "Enter" && first) void open(first.dataset.app!);
    });
    el.querySelector("#app-list")!.addEventListener("click", async (e) => {
      const target = e.target as HTMLElement;
      const row = target.closest<HTMLElement>("[data-app]");
      if (!row) return;
      if (target.closest("[data-remove]")) {
        await call("app_remove", { id: row.dataset.app });
        apps = apps.filter((a) => a.id !== row.dataset.app);
        paintApps(el, search.value);
      } else void open(row.dataset.app!);
    });
    el.querySelector("#app-add")!.addEventListener("click", async () => {
      const added = await call<AppEntry | null>("app_add", undefined, null);
      if (!added) return;
      apps = [added, ...apps.filter((a) => a.id !== added.id)];
      paintApps(el, search.value);
      host.say(`«${added.name}» añadida. Ya puedes abrirla por voz.`);
    });
  },
  async show(el) {
    if (!apps.length) el.querySelector("#app-list")!.innerHTML = `<small class="empty">Buscando tus apps…</small>`;
    apps = await call<AppEntry[]>("apps_list", undefined, []);
    paintApps(el, el.querySelector<HTMLInputElement>("#app-search")!.value);
  },
  hide: () => iconWatcher?.disconnect(),
};

const ANIMATED = /\.(mp4|webm|mkv|mov|gif)$/i;
let walls: string[] = [];
let wallIndex = 0;
/** El número junto al deslizador de la barra, y su relleno. */
function extensionUrl(path: string) {
  return /^https?:\/\//.test(path) ? path : convertFileSrc(path);
}

function reloadInto(section: string) {
  try {
    sessionStorage.setItem("cocuyo.reabrir", section);
  } catch {
    /* sin almacenamiento: abre donde caiga */
  }
  setTimeout(() => location.reload(), 350);
}

function paintPreview(el: HTMLElement, style: Settings["barStyle"], opacity: number, center: boolean) {
  const box = el.querySelector<HTMLElement>("#s-bar-preview")!;
  box.style.setProperty("--bp-a", String(style === "normal" ? 0.96 : opacity / 100));
  box.style.setProperty("--bp-blur", style === "blur" ? "6px" : style === "acrylic" ? "10px" : "0px");
  box.style.setProperty("--bp-sat", style === "acrylic" ? "1.8" : "1");
  box.classList.toggle("center", center);
}

function paintAmount(el: HTMLElement, value: number) {
  el.querySelector("#s-bar-amount-text")!.textContent = value === 0 ? "Nada: se ve todo lo de atrás" : `${value} %`;
  el.querySelector<HTMLElement>("#s-bar-amount")!.style.setProperty("--p", `${value}%`);
}

let wallScreen: string | null = null;

async function paintScreens(el: HTMLElement) {
  const screens = await call<{ id: string; number: number; main: boolean; live: string | null }[]>("screens_list", undefined, []);
  if (wallScreen && !screens.some((screen) => screen.id === wallScreen)) wallScreen = null;
  const box = el.querySelector<HTMLElement>("#wall-screens")!;
  box.hidden = screens.length < 2;
  el.querySelector<HTMLElement>("#wall-info")!.hidden = screens.length > 1;
  box.innerHTML = [`<button class="chip${wallScreen === null ? " on" : ""}" data-screen="">Todos</button>`]
    .concat(
      screens.map(
        (screen, i) =>
          `<button class="chip${wallScreen === screen.id ? " on" : ""}" data-screen="${esc(screen.id)}" title="${screen.main ? "Monitor principal" : ""}">Monitor ${screen.number || i + 1}</button>`,
      ),
    )
    .join("");
  const shown = wallScreen ? screens.filter((screen) => screen.id === wallScreen) : screens;
  el.querySelector<HTMLElement>("#wall-stop")!.hidden = !shown.some((screen) => screen.live);
}
const thumbs = new Map<string, string>();

function paintWalls(el: HTMLElement) {
  const stage = el.querySelector<HTMLElement>("#wall")!;
  if (!walls.length) {
    stage.innerHTML = `<small class="empty">No hay imágenes en esta carpeta. Elige otra con el botón de arriba.</small>`;
    return;
  }
  if (stage.children.length !== walls.length || stage.querySelector(".empty")) {
    stage.innerHTML = walls
      .map((path, i) => `<button class="wcard ${ANIMATED.test(path) ? "live" : ""}" data-wall="${i}"></button>`)
      .join("");
  }
  [...stage.children].forEach((node, i) => {
    const card = node as HTMLElement;
    const slot = i - wallIndex;
    card.dataset.slot = String(Math.max(-3, Math.min(3, slot)));
    if (Math.abs(slot) > 3) return;
    const path = walls[i];
    const cached = thumbs.get(path);
    if (cached) card.style.backgroundImage = `url(${cached})`;
    else if (!thumbs.has(path)) {
      thumbs.set(path, "");
      void call<string | null>("wallpaper_thumb", { path }, null).then((src) => {
        if (!src) return;
        thumbs.set(path, src);
        card.style.backgroundImage = `url(${src})`;
      });
    }
  });
}

const fondos: Page = {
  id: "fondos",
  label: "Fondos",
  w: 860,
  h: 250,
  html: () =>
    `<div class="chips"><span class="screens" id="wall-screens" hidden></span><button class="chip" id="wall-dir">${ic("folder")} Cambiar carpeta</button><button class="chip" id="wall-find">${ic("search")} Buscar fondos</button><button class="chip" id="wall-stop" hidden>Quitar fondo animado</button><small id="wall-info"></small></div>
     <div class="wall" id="wall"></div>`,
  mount(el, host) {
    const stage = el.querySelector<HTMLElement>("#wall")!;
    stage.addEventListener("click", async (e) => {
      const card = (e.target as HTMLElement).closest<HTMLElement>("[data-wall]");
      if (!card) return;
      const index = Number(card.dataset.wall);
      if (index !== wallIndex) {
        wallIndex = index;
        return paintWalls(el);
      }
      try {
        const path = walls[index];
        if (ANIMATED.test(path)) {
          await call("live_wallpaper_set", { path, screen: wallScreen });
          host.say("Fondo animado puesto. Se pausa solo cuando una ventana lo tapa.");
        } else {
          await call("live_wallpaper_set", { path: null, screen: wallScreen });
          await call("wallpaper_set", { path, screen: wallScreen });
          host.say("Fondo cambiado.");
        }
        await paintScreens(el);
      } catch (error) {
        host.say(String(error));
      }
    });
    stage.addEventListener(
      "wheel",
      (e) => {
        wallIndex = Math.max(0, Math.min(walls.length - 1, wallIndex + Math.sign(e.deltaY || e.deltaX)));
        paintWalls(el);
      },
      { passive: true },
    );
    el.querySelector("#wall-screens")!.addEventListener("click", (e) => {
      const chip = (e.target as HTMLElement).closest<HTMLElement>("[data-screen]");
      if (!chip) return;
      wallScreen = chip.dataset.screen || null;
      void paintScreens(el);
    });
    el.querySelector("#wall-stop")!.addEventListener("click", async () => {
      await call("live_wallpaper_set", { path: null, screen: wallScreen });
      host.say("Fondo animado quitado.");
      await fondos.show!(el, host);
    });
    el.querySelector("#wall-find")!.addEventListener("click", async () => {
      try {
        await call("wallpaper_browse");
        host.say("Descarga un fondo en la página que se abrió: Cocuyo lo pone solo.");
      } catch (error) {
        host.say(String(error));
      }
    });
    on("wallpaper-caught", () => {
      wallIndex = 0;
      void fondos.show!(el, host);
    });
    el.querySelector("#wall-dir")!.addEventListener("click", async () => {
      if (await call<string | null>("wallpaper_pick_dir", undefined, null)) await fondos.show!(el, host);
    });
  },
  async show(el) {
    const found = await call<{ dir: string; files: string[] } | null>("wallpapers_list", undefined, null);
    walls = found?.files ?? [];
    wallIndex = Math.min(wallIndex, Math.max(0, walls.length - 1));
    await paintScreens(el);
    el.querySelector("#wall-info")!.textContent = found
      ? `${found.dir} · ${walls.length} fondos · toca el del centro para ponerlo`
      : "";
    paintWalls(el);
  },
};

const POWER: [string, string, string, boolean][] = [
  ["lock", "lock", "Bloquear", false],
  ["moon", "sleep", "Suspender", true],
  ["restart", "restart", "Reiniciar", true],
  ["power", "shutdown", "Apagar", true],
];

const energia: Page = {
  id: "energia",
  label: "Energía",
  w: 660,
  h: 172,
  html: () =>
    POWER.map(
      ([icon, action, label]) => `<button class="power" data-power="${action}"><span class="ti">${ic(icon)}</span><span>${label}</span></button>`,
    ).join(""),
  mount(el, host) {
    for (const button of el.querySelectorAll<HTMLElement>("[data-power]")) {
      const [, action, label, confirm] = POWER.find((p) => p[1] === button.dataset.power)!;
      const text = button.querySelector("span:last-child")!;
      let armed = 0;
      let armedAt = 0;
      button.addEventListener("click", (e) => {
        if (!e.isTrusted || e.detail === 0) return;
        if (confirm && !armed) {
          button.classList.add("armed");
          text.textContent = "Toca otra vez";
          armedAt = performance.now();
          armed = window.setTimeout(() => {
            armed = 0;
            button.classList.remove("armed");
            text.textContent = label;
          }, 3000);
          return;
        }
        if (confirm && performance.now() - armedAt < 600) return;
        clearTimeout(armed);
        armed = 0;
        button.classList.remove("armed");
        text.textContent = label;
        host.close();
        void call("power", { action });
      });
    }
  },
};

type ChordKey = "dictationKeys" | "commandKeys" | "lassoKeys";
type HotkeyKey = "captureRegion" | "captureWindow" | "captureScreen" | "captureRecord" | "captureReplay";
type KeysKey = ChordKey | HotkeyKey;

const CHORDS: ChordKey[] = ["dictationKeys", "commandKeys", "lassoKeys"];
const HOTKEYS: HotkeyKey[] = ["captureRegion", "captureWindow", "captureScreen", "captureRecord", "captureReplay"];

interface RecPlan {
  perfil: string;
  tamano: [number, number];
  alto: number;
  fps: number;
  kbps: number;
  rapidez: number;
  codec: string;
  hardware: boolean;
  topes: string[];
  aprendido: number | null;
  mb_por_minuto: number;
}

interface RecMachine {
  graficas: { nombre: string; marca: string; integrada: boolean; memoria_mb: number }[];
  procesador: string;
  hilos: number;
  monitores: { id: string; nombre: string; ancho: number; alto: number; hz: number; principal: boolean; grafica: string }[];
  codecs: string[];
  automatico: string;
  plan: RecPlan;
  codificador: string;
}

export interface RecState {
  grabando: boolean;
  desde: number;
  repeticion: boolean;
  archivo: string | null;
}

const PROFILE_NAMES: Record<string, string> = { ahorro: "Ahorro", equilibrado: "Equilibrada", alta: "Alta", maxima: "Máxima" };
const CODEC_NAMES: Record<string, string> = { h264: "H.264", hevc: "HEVC", av1: "AV1" };

function markSegmented(box: HTMLElement, value: string) {
  const buttons = [...box.querySelectorAll<HTMLElement>("button")];
  box.style.setProperty("--n", String(buttons.length));
  buttons.forEach((button, i) => {
    const active = button.dataset.value === value;
    button.classList.toggle("on", active);
    if (active) box.style.setProperty("--i", String(i));
  });
  box.classList.toggle("none", !buttons.some((button) => button.dataset.value === value));
}

const segmented = (id: string, options: [string | number, string][]) =>
  `<div class="segmented" id="${id}"><span class="thumb"></span>${options.map(([value, label]) => `<button data-value="${value}">${label}</button>`).join("")}</div>`;
const isChord = (key: KeysKey): key is ChordKey => (CHORDS as string[]).includes(key);
const ESCAPE = 0x1b;

const modelsOf = (field: string) => field.split(/[,;\n]/).map((name) => name.trim()).filter(Boolean);

const keyRow = (key: KeysKey, title: string, sub: string) =>
  `<div class="srow slim" data-keys="${key}"><div><b>${title}</b><small>${sub}</small><small class="taken">Otra app ya usa este atajo</small></div><span class="chord"></span><button class="ghost">Cambiar</button></div>`;

const AI_PRESETS: [string, Settings["aiFormat"], string, string, "local" | "gratis" | "pago"][] = [
  ["Gemini", "openai", "https://generativelanguage.googleapis.com/v1beta/openai", "gemini-3.5-flash-lite, gemini-3.1-flash-lite", "gratis"],
  ["Groq", "openai", "https://api.groq.com/openai/v1", "llama-3.3-70b-versatile", "gratis"],
  ["Ollama", "openai", "http://127.0.0.1:11434/v1", "llama3.2", "local"],
  ["llama.cpp", "openai", "http://127.0.0.1:8080/v1", "local", "local"],
  ["LM Studio", "openai", "http://127.0.0.1:1234/v1", "local", "local"],
  ["OpenAI", "openai", "https://api.openai.com/v1", "gpt-4o-mini", "pago"],
  ["DeepSeek", "openai", "https://api.deepseek.com/v1", "deepseek-chat", "pago"],
  ["Anthropic", "anthropic", "https://api.anthropic.com/v1", "claude-haiku-5-5", "pago"],
];

const AI_KIND_NOTE = {
  local: "Corre en tu equipo, gratis y sin clave. Tiene que estar abierto y con un modelo descargado.",
  gratis: "Tiene plan gratis. Crea tu clave con «Conseguir clave», pégala en el campo Clave y pulsa Probar.",
  pago: "De pago: necesita la clave de tu cuenta con ese proveedor.",
};

function permissions(tipo: string, permisos: Permiso[] | null) {
  if (tipo === "tema") return "";
  if (!permisos) return `<span class="perms"><i class="warn" title="Una extensión vieja o hecha a mano: puede usar todo lo que usa Cocuyo.">No dice qué usa</i></span>`;
  if (!permisos.length) return `<span class="perms"><i>No pide nada especial</i></span>`;
  return `<span class="perms">${permisos
    .map((permiso) => `<i class="${permiso === "comandos" || permiso === "red" ? "warn" : ""}" title="${esc(PERMISOS[permiso][1])}">${esc(PERMISOS[permiso][0])}</i>`)
    .join("")}</span>`;
}

function outgoing(s: Settings) {
  if (!s.aiOn) return "Nada: sin IA, tu voz y tus capturas se quedan aquí. Todo se entiende en tu PC.";
  let host = "";
  try {
    host = new URL(s.aiUrl).hostname;
  } catch {
    host = s.aiUrl;
  }
  if (/^(127\.0\.0\.1|localhost|\[::1\])$/.test(host)) return "Nada: tu modelo corre en este PC, así que tu voz y tus capturas no salen de aquí.";
  if (host.endsWith("googleapis.com"))
    return "Lo que digas y las capturas que le pidas mirar van a Google. En el plan gratis de Gemini, Google puede usarlos para mejorar sus productos.";
  return `Lo que digas y las capturas que le pidas mirar van a ${host}. Qué hace con ellos depende de sus condiciones.`;
}

const SECTIONS: [string, string][] = [
  ["tu", "Tú"],
  ["isla", "Isla"],
  ["barra", "Barra"],
  ["voz", "Voz"],
  ["juegos", "Juegos"],
  ["capturas", "Capturas"],
  ["musica", "Música"],
  ["ia", "IA"],
  ["agente", "Agente"],
  ["remoto", "Remoto"],
  ["extensiones", "Extensiones"],
  ["atajos", "Atajos"],
];

interface RemoteState {
  activo: boolean;
  punto: string;
  celulares: { id: string; nombre: string; desde: number; conectado: boolean }[];
  emparejando: string | null;
  quedan: number;
}

const REMOTE_PAGE = "tkyoxx.github.io/Cocuyo";

const LIVE_VOICES: [string, string][] = [
  ["Charon", "grave"],
  ["Fenrir", "intensa"],
  ["Orus", "firme"],
  ["Puck", "alegre"],
  ["Kore", "clara"],
  ["Aoede", "suave"],
  ["Leda", "joven"],
  ["Zephyr", "brillante"],
];

class SettingsPage implements Page {
  id = "ajustes";
  label = "Ajustes";
  w = 900;
  h = 440;

  goTo(section: string) {
    this.section = section;
    if (this.el) this.open(section);
  }

  private open(section: string) {
    this.section = section;
    if (section === "extensiones") void this.paintExtensions();
    if (section === "agente") void this.paintMcp();
    if (section === "remoto") void this.paintRemote();
    if (section === "voz") void this.paintMics();
    for (const node of this.el.querySelectorAll<HTMLElement>("[data-sec]")) node.classList.toggle("on", node.dataset.sec === section);
    if (section === "capturas") void this.paintRec();
    const measure = () => {
      for (const node of this.el.querySelectorAll<HTMLElement>(".ssec.on")) node.classList.toggle("long", node.scrollHeight > node.clientHeight + 4);
      this.navEdges();
    };
    setTimeout(measure, 60);
    setTimeout(measure, 600);
  }

  private navEdges() {
    const nav = this.el.querySelector<HTMLElement>("#s-nav");
    if (nav) {
      nav.classList.toggle("arriba", nav.scrollTop > 4);
      nav.classList.toggle("abajo", nav.scrollHeight - nav.scrollTop - nav.clientHeight > 4);
    }
  }

  private el!: HTMLElement;
  private host!: Host;
  private settings: Settings | null = null;
  private spotify: SpotifyStatus | null = null;
  private section = SECTIONS[0][0];
  private recording: { key: KeysKey; held: number; best: number } | null = null;

  html() {
    const section = (id: string, cards: string, wide = false) =>
      `<section class="ssec${wide ? " one" : ""}" data-sec="${id}">${cards}</section>`;
    return `<nav class="snav" id="s-nav">${SECTIONS.map(([id, label]) => `<button data-sec="${id}">${label}</button>`).join("")}</nav>
      <div class="ssecs">${
        section(
          "tu",
          `<div class="card">
             <div class="srow stack">
               <div><b>Cómo te llamas</b><small>Para saludarte y para que la IA te hable por tu nombre.</small></div>
               <input id="s-name" placeholder="Tu nombre" spellcheck="false" maxlength="40" />
             </div>
             <div class="srow stack">
               <div><b>Tu ciudad</b><small>Para el clima de la pestaña Hoy.</small></div>
               <input id="s-city" placeholder="Tu ciudad" spellcheck="false" maxlength="60" />
             </div>
             <div class="srow stack">
               <div><b>Cómo quieres que te hable</b><small>La personalidad de las respuestas de la IA.</small></div>
               <input id="s-ai-persona" placeholder="Breve, con humor, como un pana…" spellcheck="false" maxlength="200" />
             </div>
           </div>
           <div class="card mem">
             <div class="srow stack">
               <div><b>Lo que Cocuyo recuerda de ti</b><small id="s-mem-count"></small></div>
               <div class="mem-list" id="s-mem-list"></div>
               <input id="s-mem-add" placeholder="Añade un dato y pulsa Enter: «mi talla es la M»" spellcheck="false" maxlength="240" />
             </div>
           </div>`,
        ) +
        section(
          "isla",
          `<div class="card">
             <div class="srow stack">
               <div><b>Dónde vive la isla</b><small id="s-dock-help"></small></div>
               <div class="segmented" id="s-dock"><span class="thumb"></span><button data-value="auto">Automática</button><button data-value="top">Arriba</button><button data-value="right">Derecha</button></div>
             </div>
           </div>
           <div class="card">
             <label class="srow"><div><b>Esconderla a pantalla completa</b><small>Desaparece mientras juegas o ves un video.</small></div><input type="checkbox" class="switch" id="s-hide"></label>
             <label class="srow"><div><b>Abrir con Windows</b><small>Cocuyo arranca solo al iniciar sesión.</small></div><input type="checkbox" class="switch" id="s-auto"></label>
           </div>
           <div class="card">
             <div class="srow"><div><b id="s-version">Cocuyo</b><small id="s-update">Mira en GitHub si hay una versión nueva.</small></div><button class="ghost" id="s-update-go">Buscar ahora</button></div>
             <label class="srow"><div><b>Actualizarse sola</b><small>Cuando sale una versión nueva la baja, se cambia y vuelve a abrirse. Nunca en medio de un juego.</small></div><input type="checkbox" class="switch" id="s-updates"></label>
           </div>`,
        ) +
        section(
          "barra",
          `<div class="card">
             <div class="bar-preview" id="s-bar-preview" aria-hidden="true"><div class="bp-bar"><span class="bp-start"></span><span class="bp-icons"><i></i><i></i><i></i><i></i><i></i></span><span class="bp-clock"></span></div></div>
             <div class="srow stack">
               <div><b>Cómo se ve la barra de tareas</b><small id="s-bar-help">Se aplica al instante, en todas tus barras.</small></div>
               <div class="segmented" id="s-bar-style"><span class="thumb"></span><button data-value="normal">Normal</button><button data-value="clear">Transparente</button><button data-value="blur">Desenfoque</button><button data-value="acrylic">Acrílico</button></div>
             </div>
             <div class="srow stack" id="s-bar-amount-row">
               <div><b>Cuánto se oscurece</b><small id="s-bar-amount-text"></small></div>
               <input type="range" class="range" id="s-bar-amount" min="0" max="100" step="5" />
             </div>
           </div>
           <div class="card">
             <label class="srow"><div><b>Centrar los íconos</b><small>Se deslizan al centro y se acomodan solos cuando abres o cierras algo.</small></div><input type="checkbox" class="switch" id="s-bar-center"></label>
             <label class="srow"><div><b>Normal si hay una ventana maximizada</b><small>Con el escritorio a la vista, como la elegiste; con algo ocupando la pantalla, la de siempre.</small></div><input type="checkbox" class="switch" id="s-bar-solid"></label>
             <label class="srow"><div><b>Esconderla sola</b><small>Aparece al llevar el mouse abajo. Es el ajuste de Windows de siempre.</small></div><input type="checkbox" class="switch" id="s-bar-hide"></label>
             <div class="srow slim" id="s-bar-rival-row" hidden><small class="warn" id="s-bar-rival"></small><button class="ghost" id="s-bar-rival-close">Cerrarlo</button></div>
           </div>`,
        ) +
        section(
          "voz",
          `<div class="card">
             <div class="srow stack">
               <div><b>Idioma</b><small>El de la interfaz, el que guía a la voz para que no transcriba en otro, y en el que responde.</small></div>
               <div class="segmented" id="s-lang"><span class="thumb"></span><button data-value="es">Español</button><button data-value="en">English</button><button data-value="pt">Português</button></div>
             </div>
             <div class="srow stack">
               <div><b>Escribir el dictado en</b><small>Hablas en tu idioma y queda escrito en otro. Necesita un modelo de IA.</small></div>
               <div class="segmented" id="s-into"><span class="thumb"></span><button data-value="">Igual</button><button data-value="es">Español</button><button data-value="en">English</button><button data-value="pt">Português</button></div>
             </div>
             <div class="srow stack">
               <div><b>Voz al responder</b><small id="s-voice-help">La de Windows, una local que suena a persona (se descarga una vez y habla sin internet) o una de Gemini.</small></div>
               <div class="srow chips flat" id="s-voice-kind"></div>
             </div>
           </div>
           <div class="card">
             <label class="srow"><div><b>Conversación en vivo</b><small>Con Gemini, Cocuyo te va escuchando mientras hablas y contesta con voz al instante, como en una llamada. Usa las mismas herramientas: música, apps, mensajes, el mouse. Gratis con tu clave.</small></div><input type="checkbox" class="switch" id="s-live"></label>
             <div class="srow stack"><div><b>Su voz</b><small>Las voces en vivo de Gemini.</small></div><div class="srow chips flat" id="s-live-voice"></div></div>
             <div class="srow"><div><b>Voz local</b><small id="s-voice"></small></div><button class="ghost" id="s-voice-get">Descargar</button></div>
             <div class="bar" id="s-voice-bar"><i></i></div>
             <div class="srow stack"><div><b>Micrófono</b><small id="s-mic-help">El que usa Cocuyo para oírte. Si no te escucha, prueba con otro.</small></div><div class="srow chips flat" id="s-mic"></div></div>
           </div>`,
        ) +
        section(
          "juegos",
          `<div class="scol">
           <div class="card">
             <label class="srow"><div><b>Avisar con sonido</b><small>Un tono al empezar a escucharte y otro al terminar, cuando hay algo a pantalla completa.</small></div><input type="checkbox" class="switch" id="s-cue"></label>
             <label class="srow"><div><b>Responder en voz alta</b><small>Dice lo que hizo con la voz de Windows, porque ahí no se ve la isla.</small></div><input type="checkbox" class="switch" id="s-speak"></label>
           </div>
           <div class="card">
             <label class="srow"><div><b>Hablarle desde el mando</b><small id="s-pad-status">Mantén la combinación para dar una orden sin soltar el mando. Xbox, DualShock 4 y DualSense.</small></div><input type="checkbox" class="switch" id="s-pad"></label>
             <div class="srow stack"><div class="segmented" id="s-pad-combo"><span class="thumb"></span><button data-value="0">L3 + R3</button><button data-value="1">Select + Start</button><button data-value="2">Botón PS</button><button data-value="3">Panel táctil</button></div><button class="ghost" id="s-pad-rec">Otros botones</button></div>
             <label class="srow"><div><b>Dibujar el aviso dentro del juego</b><small>Carga una pequeña librería en el juego para pintar ahí la píldora y los subtítulos, igual que fuera (Direct3D 11, 64 bits).</small><small class="warn">No lo actives en juegos con anti-trampas: pueden cerrarse o sancionar tu cuenta.</small></div><input type="checkbox" class="switch" id="s-layer"></label>
           </div>
           </div>
           <div class="scol">
           <div class="card">
             <div class="srow"><div><b>Discord por voz</b><small id="s-dc-state">Silénciate, ensordécete y entra o sal de canales de voz hablándole a Cocuyo, también con el juego abierto.</small></div><button class="ghost" id="s-dc-go" hidden>Desconectar</button></div>
             <div id="s-dc-def" hidden>
               <div class="srow stack"><div><b>Servidor predeterminado</b><small id="s-dc-def-now">Ninguno.</small></div><input id="s-dc-def-find" placeholder="Busca tu servidor" spellcheck="false" autocomplete="off" /></div>
               <div class="srow chips" id="s-dc-guilds"></div>
             </div>
             <div class="dc-steps" id="s-dc-steps">
               <div class="srow slim"><div><b>1. Crea tu app de Discord</b><small>Es gratis y solo la usas tú. Entra al portal con la misma cuenta que tienes abierta en Discord, pulsa «New Application», ponle de nombre Cocuyo, marca la casilla y dale «Create».</small></div><button class="ghost" id="s-dc-portal">Abrir el portal</button></div>
               <div class="srow slim"><div><b>2. Agrega la dirección de vuelta</b><small>En el menú de la izquierda entra a «OAuth2». En «Redirects» pulsa «Add Redirect», pega la dirección y dale «Save Changes» abajo. Tiene que quedar una sola dirección: si hay otras, bórralas.</small></div><button class="ghost" id="s-dc-copy">Copiar la dirección</button></div>
               <div class="srow stack"><div><b>3. Copia el Client ID</b><small>En la misma página de «OAuth2», arriba en «Client Information». Pulsa «Copy» y pégalo aquí.</small></div><input id="s-dc-id" placeholder="Client ID: solo números" spellcheck="false" autocomplete="off" /></div>
               <div class="srow stack"><div><b>4. Copia el Client Secret</b><small>Justo debajo del Client ID: pulsa «Reset Secret», confirma con «Yes, do it!» y copia el que sale (Discord lo muestra una sola vez).</small></div><input id="s-dc-secret" type="password" placeholder="Client Secret" spellcheck="false" autocomplete="off" /></div>
               <div class="srow slim"><div><b>5. Conecta</b><small>Con Discord abierto, pulsa Conectar. Si la app es de la misma cuenta, se conecta solo; si no, Discord te pide permiso: dale «Autorizar». La llave queda cifrada en tu PC.</small></div><button class="ghost pick" id="s-dc-connect">Conectar</button></div>
             </div>
           </div>
           </div>`,
        ) +
        section(
          "capturas",
          `<div class="scol">
           <div class="card rec-card">
             <div class="srow"><div><b>Grabar la pantalla</b><small id="s-rec-help">Con el audio del PC y tu micrófono. Lo codifica la gráfica, así casi no gasta.</small></div><button class="ghost pick" id="s-rec">Grabar</button></div>
             <div class="srow stack"><div><b>Calidad</b><small id="s-rec-plan"></small></div>${segmented("s-rec-profile", [["auto", "Automática"], ["ahorro", "Ahorro"], ["equilibrado", "Equilibrada"], ["alta", "Alta"], ["maxima", "Máxima"]])}</div>
             <div id="s-rec-manual"></div>
             <label class="srow"><div><b>Mostrar el cursor</b><small>Se dibuja en la gráfica, encima de la imagen.</small></div><input type="checkbox" class="switch" id="s-rec-cursor"></label>
             <label class="srow"><div><b>Que Cocuyo salga en los videos</b><small>La isla, lo que entiende y lo que responde quedan grabados. Apagado, se esconde solo mientras grabas un video.</small></div><input type="checkbox" class="switch" id="s-rec-show"></label>
           </div>
           <div class="card">
             <label class="srow"><div><b>Repetición instantánea</b><small id="s-rec-replay-help"></small></div><input type="checkbox" class="switch" id="s-rec-replay"></label>
             <div class="srow stack" id="s-rec-replay-row">${segmented("s-rec-replay-secs", [[15, "15 s"], [30, "30 s"], [60, "1 min"], [120, "2 min"]])}<button class="ghost" id="s-rec-save">Guardar lo último</button></div>
           </div>
           </div>
           <div class="scol">
           <div class="card">
             <label class="srow"><div><b>Audio del PC</b><small>Todo lo que suena: el juego, la música, la otra persona en una llamada.</small></div><input type="checkbox" class="switch" id="s-rec-system"></label>
             <div class="srow stack" id="s-rec-system-row"><div><b>Volumen del PC</b><small id="s-rec-system-text"></small></div><input type="range" class="range" id="s-rec-system-vol" min="0" max="200" step="5" /></div>
             <label class="srow"><div><b>Micrófono</b><small>Tu voz encima de todo lo demás.</small></div><input type="checkbox" class="switch" id="s-rec-mic"></label>
             <div class="srow chips flat" id="s-rec-mics"></div>
             <div class="srow stack" id="s-rec-mic-row"><div><b>Volumen del micrófono</b><small id="s-rec-mic-text"></small></div><input type="range" class="range" id="s-rec-mic-vol" min="0" max="200" step="5" /></div>
             <div class="srow stack" id="s-rec-noise-row"><div><b>Quitar el ruido</b><small>Suave y Fuerte limpian el ventilador, el zumbido y el soplido. RNNoise, el más completo, es una red neuronal que además quita el teclado y el ruido de la calle. Todo corre dentro de Cocuyo, sin internet.</small></div>${segmented("s-rec-noise", [["", "Apagado"], ["suave", "Suave"], ["fuerte", "Fuerte"], ["rnnoise", "RNNoise"]])}</div>
             <div class="srow stack"><div><b>Calidad del sonido</b><small>AAC estéreo a 48 kHz.</small></div>${segmented("s-rec-audio", [[128, "128 kb/s"], [160, "160 kb/s"], [192, "192 kb/s"]])}</div>
           </div>
           <div class="card">
             <div class="srow"><div><b>Carpeta de grabaciones</b><small class="path" id="s-rec-folder"></small></div><button class="ghost" id="s-rec-reset" hidden>Restablecer</button><button class="ghost" id="s-rec-open">Abrir</button><button class="ghost" id="s-rec-pick">Cambiar</button></div>
             <div class="srow" id="s-folder-row"><div><b>Carpeta de capturas</b><small class="path" id="s-folder"></small></div><button class="ghost" id="s-reset" hidden>Restablecer</button><button class="ghost" id="s-open">Abrir</button><button class="ghost" id="s-pick">Cambiar</button></div>
           </div>
           <div class="card">
             <label class="srow"><div><b>Preguntar dónde guardar</b><small>Apagado: cada captura va directo a la carpeta.</small></div><input type="checkbox" class="switch" id="s-ask"></label>
             <label class="srow"><div><b>Copiar al portapapeles</b><small>La captura queda lista para pegar.</small></div><input type="checkbox" class="switch" id="s-copy"></label>
           </div>
           </div>`,
        ) +
        section(
          "musica",
          `<div class="card">
             <div class="srow"><div><b>Spotify</b><small id="s-sp"></small></div><button class="ghost" id="s-sp-go" hidden>Desconectar</button></div>
             <div class="dc-steps" id="s-sp-steps">
               <div class="srow slim" data-sp-setup><div><b>1. Crea tu app de Spotify</b><small>Es gratis y solo la usas tú. En el portal entra con tu cuenta de Spotify, acepta los términos si te los pide y pulsa «Create app». De nombre ponle Cocuyo y de descripción lo que quieras.</small></div><button class="ghost" id="s-sp-portal">Abrir el portal</button></div>
               <div class="srow slim" data-sp-setup><div><b>2. Agrega la dirección de vuelta</b><small>En la misma página, en «Redirect URIs», pega la dirección y pulsa «Add». Más abajo, en «Which API/SDKs are you planning to use?», marca «Web API», acepta los términos y dale «Save».</small></div><button class="ghost" id="s-sp-copy">Copiar la dirección</button></div>
               <div class="srow stack" data-sp-setup><div><b>3. Copia el Client ID</b><small>Entra a tu app, pulsa «Settings» y copia el «Client ID» de arriba. El Client Secret no hace falta.</small></div><input id="s-sp-id" placeholder="Client ID de tu app de Spotify" spellcheck="false" autocomplete="off" /></div>
               <div class="srow slim"><div><b id="s-sp-last">4. Conecta</b><small>Pulsa Conectar: se abre el navegador, entras con tu cuenta y le das «Aceptar». La llave queda cifrada en tu PC.</small></div><button class="ghost pick" id="s-sp-connect">Conectar</button></div>
             </div>
           </div>
           <div class="card">
             <div class="srow"><div><b>Qué le puedes pedir</b><small>Dilo como te salga; estos son ejemplos.</small></div></div>
             <div class="srow slim"><div><b>«pon Bad Bunny», «pon rock en español», «pon mi lista de entrenar»</b><small>Suena en Spotify: la canción, el artista, un género o tu lista.</small></div></div>
             <div class="srow slim"><div><b>«agrega esta canción a mi lista Gym», «sácala de mi lista Gym», «crea una lista Para el carro»</b><small>Tus listas. Si lo conectaste antes y no te deja, dale Desconectar y Conectar otra vez: hace falta un permiso nuevo.</small></div></div>
             <div class="srow slim"><div><b>«dale me gusta», «¿está en mis Me gusta?», «pon mis Me gusta», «pon en cola Despacito»</b><small>Me gusta y la cola. Spotify no deja poner tus Me gusta desde afuera, así que Cocuyo arma una lista privada «Mis Me gusta · Cocuyo» y la mantiene al día.</small></div></div>
             <div class="srow slim"><div><b>«adelanta 30 segundos», «atrasa 10 segundos», «ve al minuto 2»</b><small>Moverte dentro de la canción. Sin conectar también sirve si la app lo deja.</small></div></div>
             <div class="srow slim"><div><b>«pausa», «siguiente», «súbele», «¿qué suena?»</b><small>Esto funciona aunque no conectes nada: va por los controles de música de Windows.</small></div></div>
           </div>`,
          true,
        ) +
        section(
          "ia",
          `<div class="card">
             <label class="srow"><div><b>Entender con un modelo de IA</b><small>Opcional. Viene con Gemini, que tiene plan gratis; solo falta tu clave.</small></div><input type="checkbox" class="switch" id="s-ai"></label>
             <div class="srow slim chips" id="s-ai-presets">${AI_PRESETS.map(([name], i) => `<button class="chip" data-preset="${i}">${name}</button>`).join("")}</div>
             <div class="srow slim"><input id="s-ai-url" placeholder="Dirección del servicio" spellcheck="false" /></div>
             <div class="srow slim pair"><input id="s-ai-model" placeholder="Modelo" spellcheck="false" /><button class="ghost pick" id="s-ai-pick">Elegir modelos ${ic("down")}</button></div>
             <div class="srow slim pair"><input id="s-ai-key" type="password" placeholder="Clave" spellcheck="false" autocomplete="off" /><button class="ghost" id="s-ai-getkey" hidden>Conseguir clave</button></div>
             <div class="srow"><div><b>Qué sale de tu PC</b><small id="s-ai-out"></small></div></div>
           </div>
           <div class="card">
             <div class="ai-list" id="s-ai-list" hidden></div>
             <div class="srow stack">
               <div><b>Cuándo preguntarle</b><small id="s-ai-when-help"></small></div>
               <div class="segmented" id="s-ai-when"><span class="thumb"></span><button data-value="no">Si no entiendo</button><button data-value="si">Siempre</button></div>
             </div>
             <label class="srow"><div><b>Escuchar con el modelo</b><small>Le manda tu voz en vez del texto: entiende mejor y responde antes. El audio sale de tu equipo.</small></div><input type="checkbox" class="switch" id="s-ai-hear"></label>
             <label class="srow"><div><b>Pulir los dictados</b><small>Quita titubeos y pone la puntuación. Cada dictado pasa por el modelo.</small></div><input type="checkbox" class="switch" id="s-ai-polish"></label>
             <div class="srow slim"><div><b>Probar la conexión</b><small id="s-ai-result">Envía una frase torcida y muestra qué entendió.</small></div><button class="ghost" id="s-ai-test">Probar</button></div>
             <div class="srow slim"><div><b>Tus scripts</b><small id="s-scripts"></small></div><button class="ghost" id="s-gallery">Galería</button><button class="ghost" id="s-scripts-open">Abrir carpeta</button></div>
           </div>`,
        ) +
        section(
          "agente",
          `<div class="card">
             <label class="srow"><div><b>Modo agente</b><small>Para lo que tiene varios pasos, Cocuyo piensa y usa herramientas: sus órdenes, tus scripts, la pantalla, las extensiones y los servidores MCP. Lo simple sigue siendo al instante.</small></div><input type="checkbox" class="switch" id="s-agent"></label>
             <label class="srow"><div><b>Usar el mouse y el teclado</b><small>Abre apps, mira sus botones y hace clic y escribe como tú, en cualquier resolución: «mándale a Angela por WhatsApp que llego tarde». Si mueves el mouse, suelta todo.</small></div><input type="checkbox" class="switch" id="s-agent-hands"></label>
             <label class="srow"><div><b>Pedir permiso antes de enviar</b><small>Antes de mandar, publicar, comprar o borrar algo, te muestra a dónde va y espera tu «Hazlo».</small></div><input type="checkbox" class="switch" id="s-agent-confirm"></label>
             <div class="srow slim"><div><b>Cómo se le habla</b><small>Normal: Cocuyo decide solo si es una orden rápida o una tarea. Para obligarlo a pensar, empieza con «Encárgate de…».</small></div></div>
             <div class="srow slim"><div><b>Necesita</b><small id="s-agent-need">Un modelo de IA conectado en la sección IA, con formato OpenAI (Gemini viene listo).</small></div></div>
           </div>
           <div class="card">
             <div class="srow slim"><div><b>Servidores MCP</b><small id="s-mcp-sum">Los mismos que usa Claude Desktop: archivos, GitHub, el navegador, tu casa…</small></div><button class="ghost pick" id="s-mcp-open">Editar mcp.json</button><button class="ghost" id="s-mcp-reload">Reconectar</button></div>
             <div class="mcp-list" id="s-mcp-list"></div>
           </div>`,
        ) +
        section(
          "remoto",
          `<div class="card">
             <label class="srow"><div><b>Cocuyo en el celular</b><small>Háblale o escríbele desde tu celular, estés donde estés. Va cifrado de punta a punta: el punto de cita solo pasa paquetes que no puede abrir.</small></div><input type="checkbox" class="switch" id="s-remote"></label>
             <div class="srow slim"><div><b>Tu punto de cita</b><small id="s-remote-relay-sum"></small></div><button class="ghost" id="s-remote-redo" hidden>Montarlo otra vez</button></div>
             <div class="remote-steps" id="s-remote-steps">
             <div class="srow slim"><div><b>1. Una cuenta de Cloudflare</b><small>Gratis y sin tarjeta. Ahí vive tu punto de cita y nadie más lo usa. Si ya la tienes, sáltate esto.</small></div><button class="ghost" data-open="https://dash.cloudflare.com/sign-up">Crear cuenta</button></div>
             <div class="srow slim"><div><b>2. Una llave para Cocuyo</b><small>Dale a «Create Token», elige la plantilla «Edit Cloudflare Workers» con «Use template», baja hasta «Continue to summary» y luego «Create Token». Copia la llave que sale.</small></div><button class="ghost" data-open="https://dash.cloudflare.com/profile/api-tokens">Abrir mis llaves</button></div>
             <div class="srow slim"><div><b>3. Pégala aquí</b><small>Cocuyo la usa una sola vez para montar tu punto de cita y la olvida. Después puedes borrarla en Cloudflare.</small></div></div>
             <div class="srow slim pair"><input id="s-remote-key" type="password" placeholder="Llave de Cloudflare" spellcheck="false" autocomplete="off" /><button class="ghost pick" id="s-remote-make">Crear mi punto de cita</button></div>
             <div class="srow stack">
               <div><b>¿Ya tienes uno?</b><small>Si montaste el tuyo a mano, pon aquí su dirección.</small></div>
               <input id="s-remote-relay" placeholder="wss://…" spellcheck="false" />
             </div>
             </div>
           </div>
           <div class="card">
             <div class="srow" id="s-remote-pair-row"><div><b>Emparejar un celular</b><small id="s-remote-pair-help">Te doy un código para escribirlo en el celular. Dura dos minutos.</small></div><button class="ghost" id="s-remote-pair">Emparejar</button></div>
             <div class="remote-code" id="s-remote-code" hidden><div class="remote-qr" id="s-remote-qr"></div><div><b id="s-remote-code-text"></b><small>Escanéalo con la cámara del celular, o abre <span id="s-remote-page"></span> y escribe el código.</small><small id="s-remote-left"></small><button class="ghost" id="s-remote-cancel">Cancelar</button></div></div>
             <div class="mcp-list" id="s-remote-list"></div>
           </div>`,
        ) +
        section(
          "extensiones",
          `<div class="card">
             <label class="srow"><div><b>Extensiones y temas</b><small>Cambian cómo se ve la isla o le añaden cosas nuevas, como Spicetify con Spotify. Apagado, Cocuyo arranca tal cual viene.</small></div><input type="checkbox" class="switch" id="s-ext-on"></label>
             <div class="srow slim"><div><b>Hazlas tú</b><small>Un tema es un CSS; una extensión, un poco de JavaScript con la API de Cocuyo.</small></div><button class="ghost" id="s-ext-docs">Cómo se hacen</button></div>
             <div class="srow slim"><div><b>Tu carpeta</b><small id="s-ext-count"></small></div><button class="ghost pick" id="s-ext-gallery">Galería</button><button class="ghost" id="s-ext-open">Abrir carpeta</button></div>
           </div>
           <div class="card ext-card">
             <div class="ext-list" id="s-ext-list"></div>
             <div class="ai-list wide" id="s-ext-shop" hidden></div>
           </div>`,
        ) +
        section(
          "atajos",
          `<div class="card">${
            keyRow("dictationKeys", "Dictar", "Mantener y hablar") +
            keyRow("commandKeys", "Dar una orden", "Mantener y hablar") +
            keyRow("lassoKeys", "Rodear y preguntar", "Mantener y mover el mouse. Con IA, habla mientras rodeas.")
          }</div>
           <div class="card">${
             keyRow("captureRegion", "Capturar un rectángulo", "") +
             keyRow("captureWindow", "Capturar la ventana activa", "") +
             keyRow("captureScreen", "Capturar toda la pantalla", "") +
             keyRow("captureRecord", "Grabar la pantalla", "Una vez empieza, otra vez para y guarda.") +
             keyRow("captureReplay", "Guardar lo último", "Con la repetición instantánea encendida.")
           }</div>`,
        )
      }<p class="snote" id="s-note"></p></div>`;
  }

  mount(el: HTMLElement, host: Host) {
    this.el = el;
    this.host = host;
    const q = <T extends HTMLElement>(sel: string) => el.querySelector<T>(sel)!;

    for (const button of el.querySelectorAll<HTMLElement>("#s-nav button")) {
      button.addEventListener("click", () => this.open(button.dataset.sec!));
    }
    q("#s-nav").addEventListener("scroll", () => this.navEdges(), { passive: true });
    new ResizeObserver(() => this.navEdges()).observe(q("#s-nav"));
    this.open(this.section);

    for (const button of el.querySelectorAll<HTMLElement>("#s-dock button")) {
      button.addEventListener("click", () => this.save({ dock: button.dataset.value as Settings["dock"] }));
    }
    for (const button of el.querySelectorAll<HTMLElement>("#s-lang button")) {
      button.addEventListener("click", () => this.save({ language: button.dataset.value as Settings["language"] }));
    }
    const toggle = (sel: string, key: "hideOnFullscreen" | "askWhere" | "copyToClipboard" | "autostart" | "cueFullscreen" | "speakFullscreen" | "gameOverlay" | "aiOn" | "aiDictation" | "aiHear" | "padOn" | "checkUpdates" | "barCenter" | "barSolidMax" | "extensionsOn" | "aiAgent" | "agentHands" | "agentConfirm" | "aiLive" | "remoteOn") => {
      const input = q<HTMLInputElement>(sel);
      input.addEventListener("change", () => {
        const change: Partial<Settings> = {};
        change[key] = input.checked;
        void this.save(change);
      });
    };
    toggle("#s-hide", "hideOnFullscreen");
    toggle("#s-cue", "cueFullscreen");
    toggle("#s-speak", "speakFullscreen");
    toggle("#s-layer", "gameOverlay");
    toggle("#s-auto", "autostart");
    toggle("#s-ask", "askWhere");
    toggle("#s-copy", "copyToClipboard");
    q("#s-reset").addEventListener("click", () => this.save({ captureDir: null }));
    q("#s-open").addEventListener("click", () => void call("open_capture_dir"));
    q("#s-pick").addEventListener("click", async () => {
      const picked = await pickCaptureDir();
      if (picked) await this.save({ captureDir: picked });
    });

    q("#s-voice-get").addEventListener("click", () => {
      void call("voice_install");
      this.paintVoice({ installed: false, installing: true, megabytes: 0 }, 0);
    });
    on<VoiceProgress>("voice-install", (p) => {
      if (p.error) this.note(p.error);
      if (p.done || p.error) void this.loadVoice();
      else this.paintVoice({ installed: false, installing: true, megabytes: 0 }, p.fraction);
    });

    const spotifyId = q<HTMLInputElement>("#s-sp-id");
    spotifyId.addEventListener("pointerdown", () => host.focus(spotifyId));
    q("#s-sp-portal").addEventListener("click", () => void call("script_action", { action: { abrir: "https://developer.spotify.com/dashboard" } }).catch(() => {}));
    q("#s-sp-copy").addEventListener("click", async () => {
      try {
        await navigator.clipboard.writeText("http://127.0.0.1:5173/");
        this.note("Copiada: http://127.0.0.1:5173/. Pégala en «Redirect URIs».");
      } catch {
        this.note("Escribe en «Redirect URIs»: http://127.0.0.1:5173/");
      }
    });
    q("#s-sp-go").addEventListener("click", async () => {
      await call("spotify_disconnect");
      void this.loadSpotify();
    });
    q("#s-sp-connect").addEventListener("click", async () => {
      if (!this.spotify?.hasClientId) {
        const pasted = spotifyId.value.trim();
        if (!pasted) return this.note("Pega primero el Client ID de tu app de Spotify.");
        await this.save({ spotifyClientId: pasted });
      }
      this.note("");
      void call("spotify_connect");
      this.paintSpotify({ source: this.spotify?.source ?? null, hasClientId: true, connecting: true, ownSaved: false, ownError: null });
    });
    on<string | null>("spotify", (error) => {
      if (error) this.note(error);
      else this.host.say("Spotify quedó conectado con Cocuyo.");
      void this.loadSpotify();
    });

    q("#s-voice-kind").addEventListener("click", (e) => {
      const target = (e.target as HTMLElement).closest<HTMLElement>("[data-voice],[data-get],#s-voice-try");
      if (!target) return;
      if (target.id === "s-voice-try") {
        this.note("Preparando la voz…");
        return void call("voice_try", { text: tr("Hola, soy Cocuyo. Así sueno cuando te respondo.") })
          .then(() => this.note(""))
          .catch((error) => this.note(String(error)));
      }
      if (target.dataset.get) {
        this.wantedVoice = target.dataset.get;
        this.note("Descargando la voz. Puedes seguir usando Cocuyo; te aviso al terminar.");
        return void call("tts_install", { id: target.dataset.get })
          .then(() => this.paintVoices())
          .catch((error) => this.note(String(error)));
      }
      void this.save({ voice: target.dataset.voice ?? "" });
    });
    on<{ id: string; error: string | null }>("tts-install", (done) => {
      if (done.error) this.note(done.error);
      else if (this.wantedVoice === done.id) {
        this.note("");
        void this.save({ voice: `local:${done.id}` });
      }
      void this.paintVoices();
    });
    toggle("#s-pad", "padOn");
    toggle("#s-bar-center", "barCenter");
    toggle("#s-bar-solid", "barSolidMax");
    for (const button of el.querySelectorAll<HTMLElement>("#s-bar-style button")) {
      button.addEventListener("click", () => this.save({ barStyle: button.dataset.value as Settings["barStyle"] }));
    }
    const amount = q<HTMLInputElement>("#s-bar-amount");
    let sending = 0;
    amount.addEventListener("input", () => {
      paintAmount(this.el, Number(amount.value));
      if (this.settings) paintPreview(this.el, this.settings.barStyle, Number(amount.value), this.settings.barCenter);
      window.clearTimeout(sending);
      sending = window.setTimeout(() => void this.save({ barOpacity: Number(amount.value) }), 90);
    });
    q("#s-bar-rival-close").addEventListener("click", async () => {
      const closed = await call<string | null>("taskbar_close_rival", undefined, null);
      if (closed) this.note(`${closed} cerrado. Ahora la barra la maneja Cocuyo.`);
      await this.paint();
    });
    q<HTMLInputElement>("#s-bar-hide").addEventListener("change", (e) => {
      void call("taskbar_autohide", { on: (e.target as HTMLInputElement).checked });
    });
    toggle("#s-updates", "checkUpdates");
    for (const button of el.querySelectorAll<HTMLElement>("#s-pad-combo button")) {
      button.addEventListener("click", () => this.save({ padCombo: Number(button.dataset.value), padButtons: 0 }));
    }
    q("#s-pad-rec").addEventListener("click", async () => {
      this.note("Mantén en el mando los botones que quieras usar y suéltalos.");
      try {
        const buttons = await call<number>("pad_record");
        await this.save({ padButtons: buttons });
      } catch (error) {
        this.note(String(error));
      }
    });
    for (const button of el.querySelectorAll<HTMLElement>("#s-into button")) {
      button.addEventListener("click", () => this.save({ dictationInto: button.dataset.value ?? "" }));
    }
    q("#s-update-go").addEventListener("click", async () => {
      const line = q("#s-update");
      const button = q<HTMLButtonElement>("#s-update-go");
      button.disabled = true;
      line.textContent = "Buscando…";
      const found = await call<{ version: string; notes: string } | null>("update_check", undefined, null);
      if (!found) line.textContent = "Estás al día.";
      else {
        line.textContent = `Bajando la ${found.version}… Cocuyo se cierra y vuelve solo.`;
        try {
          await call("update_install");
        } catch (error) {
          line.textContent = String(error);
        }
      }
      button.disabled = false;
    });
    q("#s-gallery").addEventListener("click", () => void this.openGallery());
    toggle("#s-ext-on", "extensionsOn");
    toggle("#s-agent", "aiAgent");
    toggle("#s-agent-hands", "agentHands");
    toggle("#s-live", "aiLive");
    const rec = q<HTMLButtonElement>("#s-rec");
    const paintButton = (state: RecState) => {
      rec.textContent = state.grabando ? "Parar" : "Grabar";
      rec.classList.toggle("on", state.grabando);
    };
    const idle: RecState = { grabando: false, desde: 0, repeticion: false, archivo: null };
    void call<RecState>("grabar_estado", undefined, idle).then(paintButton);
    on<RecState>("grabadora", paintButton);
    rec.addEventListener("click", async () => {
      rec.disabled = true;
      try {
        const state = await call<RecState>("grabar_estado", undefined, idle);
        if (state.grabando) {
          const file = await call<string>("grabar_parar");
          this.note(`Guardada: ${file.split("\\").pop()}`);
        } else {
          rec.textContent = "Arrancando…";
          await call<string>("grabar_empezar");
          this.note("Grabando. Pulsa Parar o toca el punto rojo de la isla para guardar.");
        }
      } catch (error) {
        this.note(String(error));
      }
      paintButton(await call<RecState>("grabar_estado", undefined, idle));
      rec.disabled = false;
    });
    const recSection = q(".rec-card").closest<HTMLElement>(".ssec")!;
    recSection.addEventListener("click", (e) => {
      const button = (e.target as HTMLElement).closest<HTMLElement>("button[data-value], button[data-monitor], button[data-recmic]");
      if (!button || !this.settings) return;
      const box = button.parentElement!;
      const value = button.dataset.value ?? "";
      const plan = this.recMachine?.plan;
      const native = this.recNative() ?? 0;
      const manual = (change: Partial<Settings>) =>
        this.save({
          recProfile: "propio",
          recHeight: plan && plan.alto < native ? plan.alto : 0,
          recFps: this.settings!.recProfile === "propio" ? this.settings!.recFps : plan?.fps ?? 60,
          recKbps: plan?.kbps ?? this.settings!.recKbps,
          recSpeed: plan?.rapidez ?? this.settings!.recSpeed,
          ...change,
        });
      if (button.dataset.monitor !== undefined) void this.save({ recMonitor: button.dataset.monitor });
      else if (button.dataset.recmic !== undefined) void this.save({ recMicId: button.dataset.recmic });
      else if (box.id === "s-rec-profile") void this.save({ recProfile: value });
      else if (box.id === "s-rec-height") void manual({ recHeight: Number(value) });
      else if (box.id === "s-rec-fps") void manual({ recFps: Number(value) });
      else if (box.id === "s-rec-speed") void manual({ recSpeed: Number(value) });
      else if (box.id === "s-rec-codec") void this.save({ recCodec: value });
      else if (box.id === "s-rec-audio") void this.save({ recAudioKbps: Number(value) });
      else if (box.id === "s-rec-noise") void this.save({ recNoise: value });
      else if (box.id === "s-rec-replay-secs") void this.save({ recReplaySecs: Number(value) });
    });
    recSection.addEventListener("input", (e) => {
      const input = e.target as HTMLInputElement;
      if (input.id === "s-rec-kbps") this.paintKbps(Number(input.value) * 1000);
      if (input.id === "s-rec-system-vol" || input.id === "s-rec-mic-vol") q(`#${input.id.replace("-vol", "-text")}`).textContent = `${input.value} %`;
    });
    recSection.addEventListener("change", (e) => {
      const input = e.target as HTMLInputElement;
      if (!this.settings) return;
      const plan = this.recMachine?.plan;
      const native = this.recNative() ?? 0;
      if (input.id === "s-rec-kbps")
        void this.save({
          recProfile: "propio",
          recKbps: Number(input.value) * 1000,
          recHeight: plan && plan.alto < native ? plan.alto : 0,
          recFps: this.settings.recProfile === "propio" ? this.settings.recFps : plan?.fps ?? 60,
          recSpeed: plan?.rapidez ?? 100,
        });
      else if (input.id === "s-rec-system-vol") void this.save({ recSystemVol: Number(input.value) });
      else if (input.id === "s-rec-mic-vol") void this.save({ recMicVol: Number(input.value) });
      else if (input.id === "s-rec-system") void this.save({ recSystem: input.checked });
      else if (input.id === "s-rec-mic") void this.save({ recMic: input.checked });
      else if (input.id === "s-rec-cursor") void this.save({ recCursor: input.checked });
      else if (input.id === "s-rec-show") void this.save({ recShowCocuyo: input.checked });
      else if (input.id === "s-rec-replay") void this.save({ recReplay: input.checked });
    });
    const dcId = q<HTMLInputElement>("#s-dc-id");
    const dcSecret = q<HTMLInputElement>("#s-dc-secret");
    for (const input of [dcId, dcSecret]) input.addEventListener("pointerdown", () => host.focus(input));
    const paintDiscord = async () => {
      const state = await call<{ configurado: boolean; conectado: boolean }>("discord_estado", undefined, { configurado: false, conectado: false });
      q("#s-dc-go").hidden = !state.conectado;
      q("#s-dc-steps").hidden = state.conectado;
      q("#s-dc-def").hidden = !state.conectado;
      if (state.conectado) void this.loadGuilds();
      q("#s-dc-state").textContent = state.conectado
        ? "Conectado. Dile «silénciame en Discord», «desmutéame», «ensordéceme», «métete al canal General», «méteme a General en Los Pibes» o «sal del canal»."
        : "Silénciate, ensordécete y entra o sal de canales de voz hablándole a Cocuyo, también con el juego abierto. Se arma una sola vez en 5 pasos:";
    };
    void paintDiscord();
    const guildFind = q<HTMLInputElement>("#s-dc-def-find");
    guildFind.addEventListener("pointerdown", () => host.focus(guildFind));
    guildFind.addEventListener("input", () => this.paintGuilds());
    q("#s-dc-guilds").addEventListener("click", async (e) => {
      const target = (e.target as HTMLElement).closest<HTMLElement>("[data-guild]");
      if (!target) return;
      await this.save({ discordServer: target.dataset.guild ?? "" });
      guildFind.value = "";
      this.paintGuilds();
    });
    q("#s-dc-portal").addEventListener("click", () => void call("script_action", { action: { abrir: "https://discord.com/developers/applications" } }).catch(() => {}));
    q("#s-dc-copy").addEventListener("click", async () => {
      try {
        await navigator.clipboard.writeText("http://127.0.0.1");
        this.note("Copiada: http://127.0.0.1. Pégala en «Add Redirect».");
      } catch {
        this.note("Escribe en «Add Redirect»: http://127.0.0.1");
      }
    });
    q("#s-dc-go").addEventListener("click", async () => {
      await call("discord_quitar");
      this.note("Discord desconectado de Cocuyo.");
      void paintDiscord();
    });
    q("#s-dc-connect").addEventListener("click", async () => {
      const button = q<HTMLButtonElement>("#s-dc-connect");
      if (!dcId.value.trim()) return this.note("Falta el paso 3: pega el Client ID.");
      if (!dcSecret.value.trim()) return this.note("Falta el paso 4: pega el Client Secret.");
      button.disabled = true;
      button.textContent = "Esperando a Discord…";
      this.note("Mira Discord: te va a pedir permiso para Cocuyo. Dale «Autorizar».");
      try {
        this.note(await call<string>("discord_conectar", { id: dcId.value, secreto: dcSecret.value }));
        dcSecret.value = "";
        window.dispatchEvent(new CustomEvent("cocuyo-discord"));
      } catch (error) {
        this.note(String(error));
      }
      button.disabled = false;
      button.textContent = "Conectar";
      void paintDiscord();
    });
    q("#s-rec-save").addEventListener("click", async () => {
      try {
        const file = await call<string>("grabar_ultimo");
        this.note(`Guardé lo último: ${file.split("\\").pop()}`);
      } catch (error) {
        this.note(String(error));
      }
    });
    q("#s-rec-reset").addEventListener("click", () => this.save({ recDir: null }));
    q("#s-rec-open").addEventListener("click", () => void call("open_record_dir").catch((error) => this.note(String(error))));
    q("#s-rec-pick").addEventListener("click", async () => {
      const picked = await pickCaptureDir();
      if (picked) await this.save({ recDir: picked });
    });
    q("#s-mic").addEventListener("click", async (e) => {
      const target = (e.target as HTMLElement).closest<HTMLElement>("[data-mic]");
      if (!target) return;
      await this.save({ mic: target.dataset.mic ?? "" });
      void this.paintMics();
    });
    q("#s-live-voice").addEventListener("click", (e) => {
      const target = (e.target as HTMLElement).closest<HTMLElement>("[data-live]");
      if (target) void this.save({ liveVoice: target.dataset.live ?? "Charon" });
    });
    toggle("#s-agent-confirm", "agentConfirm");
    toggle("#s-remote", "remoteOn");
    const relay = q<HTMLInputElement>("#s-remote-relay");
    relay.addEventListener("pointerdown", () => host.focus(relay));
    relay.addEventListener("change", async () => {
      await this.save({ remoteRelay: relay.value.trim() });
      void this.paintRemote();
    });
    q("#s-remote").addEventListener("change", () => setTimeout(() => void this.paintRemote(), 120));
    const key = q<HTMLInputElement>("#s-remote-key");
    key.addEventListener("pointerdown", () => host.focus(key));
    q("#s-remote-steps").addEventListener("click", (e) => {
      const url = (e.target as HTMLElement).closest<HTMLElement>("[data-open]")?.dataset.open;
      if (url) void call("script_action", { action: { abrir: url } }).catch(() => {});
    });
    q("#s-remote-redo").addEventListener("click", () => {
      this.remoteRedo = true;
      void this.paintRemote();
    });
    const make = q<HTMLButtonElement>("#s-remote-make");
    const create = async () => {
      if (make.disabled) return;
      if (!key.value.trim()) {
        this.note("Pega primero la llave de Cloudflare.");
        key.focus();
        return;
      }
      make.disabled = true;
      make.textContent = "Montando…";
      this.note("Montando tu punto de cita en Cloudflare. Tarda menos de un minuto.");
      try {
        const made = await call<{ punto: string; responde: boolean }>("remoto_crear_punto", { llave: key.value });
        key.value = "";
        this.remoteRedo = false;
        await this.save({ remoteRelay: made.punto, remoteOn: true });
        q<HTMLInputElement>("#s-remote").checked = true;
        this.note(made.responde ? "Listo: tu punto de cita ya está en Cloudflare. Ahora empareja tu celular." : "Tu punto de cita quedó creado, pero todavía no responde. Cloudflare a veces tarda unos minutos con una dirección nueva: empareja en un rato.");
      } catch (error) {
        this.note(String(error));
      }
      make.disabled = false;
      make.textContent = "Crear mi punto de cita";
      void this.paintRemote();
    };
    make.addEventListener("click", () => void create());
    key.addEventListener("keydown", (e) => e.key === "Enter" && void create());
    q("#s-remote-pair").addEventListener("click", async () => {
      try {
        this.note("");
        await call<string>("remoto_emparejar");
      } catch (error) {
        this.note(String(error));
      }
      void this.paintRemote();
    });
    q("#s-remote-cancel").addEventListener("click", async () => {
      await call("remoto_cancelar");
      void this.paintRemote();
    });
    q("#s-remote-list").addEventListener("click", async (e) => {
      const target = (e.target as HTMLElement).closest<HTMLElement>("[data-phone]");
      if (!target) return;
      if (target.dataset.sure !== "1") {
        target.dataset.sure = "1";
        target.textContent = "¿Seguro?";
        return;
      }
      await call("remoto_quitar", { id: target.dataset.phone });
      void this.paintRemote();
    });
    on<{ tipo: string; nombre?: string; texto?: string }>("remoto", (said) => {
      if (said.tipo === "emparejado") this.note(`Listo: «${said.nombre}» quedó emparejado.`);
      if (said.tipo === "fallo" && said.texto) this.note(said.texto);
      if (this.section === "remoto") void this.paintRemote();
    });
    q("#s-mcp-open").addEventListener("click", () => void call("mcp_open").catch((error) => this.note(String(error))));
    q("#s-mcp-reload").addEventListener("click", () => void this.paintMcp(true));
    q("#s-ext-on").addEventListener("change", () => reloadInto("extensiones"));
    q("#s-ext-open").addEventListener("click", () => void call("extensions_open"));
    q("#s-ext-docs").addEventListener("click", () => void call("script_action", { action: { abrir: "https://github.com/Tkyoxx/Cocuyo/tree/main/sdk" } }).catch(() => {}));
    q("#s-ext-gallery").addEventListener("click", () => void this.openExtensionShop());
    q("#s-ext-list").addEventListener("change", async (e) => {
      const input = (e.target as HTMLElement).closest<HTMLInputElement>("[data-ext]");
      if (!input) return;
      await call("extension_set", { id: input.dataset.ext, on: input.checked });
      reloadInto("extensiones");
    });
    q("#s-ext-shop").addEventListener("click", async (e) => {
      const target = e.target as HTMLElement;
      const shop = q("#s-ext-shop");
      if (target.closest("[data-close]")) {
        shop.hidden = true;
        return;
      }
      const install = target.closest<HTMLButtonElement>("[data-get]");
      if (!install) return;
      install.disabled = true;
      install.textContent = "Bajando…";
      try {
        const name = await call<string>("extension_install", { id: install.dataset.get });
        this.note(`Listo: ${name}. Recargando la isla…`);
        reloadInto("extensiones");
      } catch (error) {
        install.disabled = false;
        install.textContent = "Instalar";
        this.note(String(error));
      }
    });
    const name = q<HTMLInputElement>("#s-name");
    const city = q<HTMLInputElement>("#s-city");
    const fact = q<HTMLInputElement>("#s-mem-add");
    for (const input of [name, city, fact]) input.addEventListener("pointerdown", () => host.focus(input));
    name.addEventListener("change", () => this.save({ name: name.value.trim() }));
    city.addEventListener("change", async () => {
      if (!city.value.trim()) return this.save({ city: "" });
      try {
        await call("weather_city", { city: city.value });
        this.settings = await getSettings();
        this.note("");
      } catch (error) {
        this.note(String(error));
      }
      await this.paint();
    });
    fact.addEventListener("keydown", async (e) => {
      if (e.key !== "Enter" || !fact.value.trim()) return;
      await call("memory_add", { fact: fact.value });
      fact.value = "";
      void this.paintMemory();
    });
    q("#s-mem-list").addEventListener("click", async (e) => {
      const target = (e.target as HTMLElement).closest<HTMLElement>("[data-forget]");
      if (!target) return;
      await call("memory_remove", { index: Number(target.dataset.forget) });
      void this.paintMemory();
    });
    toggle("#s-ai", "aiOn");
    q<HTMLInputElement>("#s-ai").addEventListener("change", (e) => {
      if ((e.target as HTMLInputElement).checked) void this.save({ aiOn: true, aiAlways: true, aiDictation: true, aiHear: true, aiAgent: true, aiLive: true });
    });
    toggle("#s-ai-polish", "aiDictation");
    toggle("#s-ai-hear", "aiHear");
    q("#s-ai-pick").addEventListener("click", () => void this.openModels());
    const aiPersona = q<HTMLInputElement>("#s-ai-persona");
    aiPersona.addEventListener("pointerdown", () => host.focus(aiPersona));
    aiPersona.addEventListener("change", () => this.save({ aiPersona: aiPersona.value.trim() }));
    const aiUrl = q<HTMLInputElement>("#s-ai-url");
    const aiModel = q<HTMLInputElement>("#s-ai-model");
    const aiKey = q<HTMLInputElement>("#s-ai-key");
    for (const input of [aiUrl, aiModel, aiKey]) input.addEventListener("pointerdown", () => host.focus(input));
    aiModel.addEventListener("pointerdown", () => void this.openModels());
    q("#s-ai-getkey").addEventListener("click", () => void call("ai_key_page").catch((error) => this.note(String(error))));
    q("#s-ai-list").addEventListener("click", (e) => {
      const target = e.target as HTMLElement;
      if (target.closest("[data-close]")) return this.closeModels();
      if (target.closest("[data-publish]")) return void this.openPublish();
      if (target.closest("[data-gallery]")) return void this.openGallery();
      const mine = target.closest<HTMLButtonElement>("[data-send]");
      if (mine) {
        mine.disabled = true;
        return void call<boolean>("gallery_publish", { name: mine.dataset.send })
          .then((whole) =>
            this.note(
              whole
                ? "Se abrió GitHub con todo lleno: entra con tu cuenta y pulsa «Submit new issue»."
                : "Se abrió GitHub. El script es largo: quedó copiado, pégalo en la casilla «El script».",
            ),
          )
          .catch((error) => this.note(String(error)))
          .finally(() => (mine.disabled = false));
      }
      const shared = target.closest<HTMLButtonElement>("[data-install]");
      if (shared) {
        shared.disabled = true;
        shared.textContent = "Bajando…";
        return void call<string>("gallery_install", { file: shared.dataset.install })
          .then((name) => {
            shared.textContent = "Instalado";
            this.note(`Listo: di «ejecuta ${name}».`);
            return this.paintAi();
          })
          .catch((error) => {
            shared.textContent = "Instalar";
            shared.disabled = false;
            this.note(String(error));
          });
      }
      const model = target.closest<HTMLElement>("[data-model]")?.dataset.model;
      if (!model || !this.settings) return;
      const chosen = modelsOf(this.settings.aiModel);
      const next = chosen.includes(model) ? chosen.filter((name) => name !== model) : [...chosen, model];
      void this.save({ aiModel: next.join(", ") }).then(() => this.paintModels());
    });
    aiUrl.addEventListener("change", () => this.save({ aiUrl: aiUrl.value.trim(), aiFormat: /anthropic/i.test(aiUrl.value) ? "anthropic" : "openai" }));
    aiModel.addEventListener("change", () => this.save({ aiModel: aiModel.value.trim() }));
    aiKey.addEventListener("change", async () => {
      try {
        await call("ai_key_set", { key: aiKey.value });
        aiKey.value = "";
        this.note("");
      } catch (error) {
        this.note(String(error));
      }
      await this.paintAi();
    });
    for (const button of el.querySelectorAll<HTMLElement>("#s-ai-presets button")) {
      button.addEventListener("click", () => {
        const [, aiFormat, url, model, kind] = AI_PRESETS[Number(button.dataset.preset)];
        this.closeModels();
        void this.save({ aiFormat, aiUrl: url, aiModel: model }).then(() => {
          this.note(AI_KIND_NOTE[kind]);
          return this.paintAi();
        });
      });
    }
    for (const button of el.querySelectorAll<HTMLElement>("#s-ai-when button")) {
      button.addEventListener("click", () => this.save({ aiAlways: button.dataset.value === "si" }));
    }
    q("#s-ai-test").addEventListener("click", async () => {
      const result = q("#s-ai-result");
      result.textContent = "Preguntando al modelo…";
      try {
        result.textContent = await call<string>("ai_test");
      } catch (error) {
        result.textContent = String(error);
      }
      this.tested = true;
    });
    q("#s-scripts-open").addEventListener("click", () => void call("scripts_open"));

    for (const row of el.querySelectorAll<HTMLElement>("[data-keys]")) {
      const key = row.dataset.keys as KeysKey;
      const button = row.querySelector("button")!;
      button.addEventListener("click", () => {
        if (this.recording?.key === key) return this.stopRecording();
        this.stopRecording();
        this.recording = { key, held: 0, best: 0 };
        setChordsPaused(true);
        setKeyEcho(true);
        row.classList.add("recording");
        button.textContent = "Cancelar";
        this.note("");
        this.paintKeys(key, 0, 0, 0);
      });
    }
  }

  async show() {
    this.settings = await getSettings();
    await this.paint();
    await this.paintAvailability();
    await this.loadVoice();
    await this.loadSpotify();
    await this.paintAi();
  }

  hide() {
    window.clearInterval(this.remoteTick);
    this.stopRecording();
    this.closeModels();
    this.tested = false;
  }

  keys(mask: number) {
    const rec = this.recording;
    if (!rec) return;
    rec.held = mask;
    if (!isChord(rec.key)) {
      this.paintKeys(rec.key, mask, 0, mask);
      return;
    }
    if (countKeys(mask) >= countKeys(rec.best)) rec.best = mask;
    this.paintKeys(rec.key, mask || rec.best, 0, mask);
    if (mask === 0 && rec.best) {
      const change: Partial<Settings> = {};
      change[rec.key] = rec.best;
      this.stopRecording();
      void this.save(change);
    }
  }

  key(vk: number) {
    const rec = this.recording;
    if (!rec) return;
    if (vk === ESCAPE) return this.stopRecording();
    if (isChord(rec.key)) return;
    const change: Partial<Settings> = {};
    change[rec.key] = { mods: rec.held, vk };
    this.stopRecording();
    void this.save(change);
  }

  private async loadSpotify() {
    this.paintSpotify(
      await call<SpotifyStatus>("spotify_status", undefined, { source: null, hasClientId: false, connecting: false, ownSaved: false, ownError: null }),
    );
  }

  private paintSpotify(status: SpotifyStatus) {
    this.spotify = status;
    const state = this.el.querySelector("#s-sp")!;
    const connected = status.source === "cocuyo";
    const button = this.el.querySelector<HTMLButtonElement>("#s-sp-connect")!;
    this.el.querySelector<HTMLElement>("#s-sp-go")!.hidden = !connected;
    this.el.querySelector<HTMLElement>("#s-sp-steps")!.hidden = connected;
    for (const row of this.el.querySelectorAll<HTMLElement>("[data-sp-setup]")) row.hidden = status.hasClientId;
    this.el.querySelector("#s-sp-last")!.textContent = status.hasClientId ? "Conecta" : "4. Conecta";
    button.disabled = status.connecting;
    if (status.connecting) state.textContent = "Acepta en el navegador que se acaba de abrir…";
    else if (connected) state.textContent = "Conectado con su propia sesión. No necesita Mis Notas abierta.";
    else if (status.ownError && status.source === "mis-notas") state.textContent = `Tu llave propia falló (${status.ownError.replace(/\.$/, "")}), así que estoy usando la de Mis Notas. Dale Conectar para arreglarla.`;
    else if (status.ownError) state.textContent = `Tu llave propia falló (${status.ownError.replace(/\.$/, "")}). Dale Conectar para arreglarla.`;
    else if (status.source === "mis-notas") state.textContent = "Usando la sesión de Mis Notas mientras esté abierta. Conecta para no depender de ella.";
    else if (status.hasClientId) state.textContent = "Sin conectar. Usará el Client ID que ya tienes en Mis Notas.";
    else state.textContent = "Sin conectar. Se arma una sola vez en 4 pasos:";
  }

  private async loadVoice() {
    this.paintVoice(await call<VoiceStatus>("voice_status", undefined, { installed: false, installing: false, megabytes: 424 }), 0);
  }

  private paintVoice(status: VoiceStatus, fraction: number) {
    const state = this.el.querySelector("#s-voice")!;
    const button = this.el.querySelector<HTMLElement>("#s-voice-get")!;
    const bar = this.el.querySelector<HTMLElement>("#s-voice-bar")!;
    button.hidden = status.installed || status.installing;
    bar.classList.toggle("on", status.installing);
    bar.style.setProperty("--p", String(fraction));
    if (status.installed) state.textContent = "Instalada. Transcribe en tu PC: nada sale de aquí.";
    else if (status.installing) state.textContent = `Descargando… ${Math.round(fraction * 100)} %`;
    else state.textContent = `Dictado y órdenes sin internet ni cuentas. Descarga única de ${status.megabytes} MB.`;
  }

  private async paintAvailability() {
    const granted = await captureKeysAvailable();
    HOTKEYS.forEach((key, i) => {
      this.el.querySelector(`[data-keys="${key}"]`)!.classList.toggle("unavailable", !granted[i]);
    });
  }

  private async save(change: Partial<Settings>) {
    if (!this.settings) return;
    const next = { ...this.settings, ...change };
    try {
      await setSettings(next);
      this.settings = next;
      this.note("");
      this.host.settingsChanged(next);
    } catch (error) {
      this.note(String(error));
    }
    await this.paint();
    setTimeout(() => void this.paintAvailability(), 150);
  }

  private async openGallery() {
    const list = this.el.querySelector<HTMLElement>("#s-ai-list")!;
    list.hidden = false;
    list.classList.add("wide");
    list.innerHTML = `<small class="empty">Abriendo la galería…</small>`;
    const head = `<div class="ai-head"><small>Scripts que comparte la gente. Son programas: instala solo los que entiendas.</small><button class="ghost pick" data-publish>Subir el mío</button><button class="ghost" data-close>Cerrar</button></div>`;
    try {
      const shared = await call<{ name: string; about: string; file: string; author: string; have: boolean }[]>("gallery_list");
      list.innerHTML = `${head}<div class="gallery">${shared
        .map((script) => {
          const who = script.author
            ? `<span class="who" data-letter="${esc(script.author[0].toUpperCase())}"><img src="https://avatars.githubusercontent.com/${esc(script.author)}?size=64" alt="" loading="lazy"></span>`
            : `<span class="who" data-letter="?"></span>`;
          const by = script.author ? `<small class="by">@${esc(script.author)}</small>` : "";
          const button = script.have
            ? `<button class="ghost" disabled>Instalado</button>`
            : `<button class="ghost" data-install="${esc(script.file)}">Instalar</button>`;
          return `<div class="shared">${who}<div><b>${esc(script.name)}</b><small title="${esc(script.about)}">${esc(script.about)}</small>${by}</div>${button}</div>`;
        })
        .join("")}</div>`;
      for (const photo of list.querySelectorAll("img")) photo.addEventListener("error", () => photo.remove());
    } catch (error) {
      list.innerHTML = head;
      list.querySelector("small")!.textContent = String(error);
    }
  }

  private async paintMcp(again = false) {
    const sum = this.el.querySelector("#s-mcp-sum")!;
    const list = this.el.querySelector("#s-mcp-list")!;
    sum.textContent = "Conectando…";
    const servers = await call<{ name: string; ok: boolean; tools: number; error: string | null }[]>(again ? "mcp_reload" : "mcp_status", undefined, []);
    const ok = servers.filter((server) => server.ok);
    sum.textContent = servers.length
      ? `${ok.length} de ${servers.length} conectados, ${ok.reduce((all, server) => all + server.tools, 0)} herramientas.`
      : "Ninguno todavía. En mcp.json hay ejemplos listos: quítales el «disabled».";
    list.innerHTML = servers
      .map(
        (server) =>
          `<div class="mcp ${server.ok ? "ok" : "bad"}"><i></i><div><b>${esc(server.name)}</b><small>${server.ok ? `${server.tools} herramientas` : esc(server.error ?? "Sin conectar")}</small></div></div>`,
      )
      .join("");
  }

  private recMachine: RecMachine | null = null;
  private recAsked = 0;

  private recNative() {
    const s = this.settings;
    const screen = this.recMachine?.monitores.find((m) => m.id === s?.recMonitor) ?? this.recMachine?.monitores.find((m) => m.principal);
    return screen?.alto;
  }

  private paintKbps(kbps: number) {
    const audio = this.settings && (this.settings.recSystem || this.settings.recMic) ? this.settings.recAudioKbps : 0;
    const perMinute = (((kbps + audio) * 60) / 8 / 1000).toFixed(0);
    const text = this.el.querySelector("#s-rec-kbps-text");
    if (text) text.textContent = `${(kbps / 1000).toFixed(1).replace(".0", "").replace(".", ",")} Mb/s · unos ${perMinute} MB por minuto`;
  }

  private async paintRec() {
    const s = this.settings;
    if (!s) return;
    const q = <T extends HTMLElement>(sel: string) => this.el.querySelector<T>(sel)!;
    const asked = ++this.recAsked;
    const machine = await call<RecMachine | null>("grabar_equipo", undefined, null);
    if (asked !== this.recAsked || !machine) return;
    this.recMachine = machine;
    const plan = machine.plan;
    const native = this.recNative() ?? plan.tamano[1];
    const screen = machine.monitores.find((m) => m.id === s.recMonitor) ?? machine.monitores.find((m) => m.principal);
    const card = machine.graficas.find((g) => g.nombre === screen?.grafica) ?? machine.graficas[0];
    q("#s-rec-help").textContent = card
      ? `${card.nombre}${card.memoria_mb >= 1024 ? ` (${Math.round(card.memoria_mb / 1024)} GB)` : ""} · codifica ${machine.codificador} · ${machine.procesador || "procesador"} (${machine.hilos} hilos)`
      : "Con el audio del PC y tu micrófono.";
    markSegmented(q("#s-rec-profile"), s.recProfile === "propio" ? "" : s.recProfile || "auto");
    const chosen = PROFILE_NAMES[plan.perfil] ?? "A tu gusto";
    const lead = s.recProfile === "auto" || !s.recProfile ? `Automática eligió ${chosen}: ` : s.recProfile === "propio" ? "A tu gusto: " : "";
    const lines = [`${lead}${plan.alto}p a ${plan.fps} fps, ${(plan.kbps / 1000).toFixed(1).replace(".0", "").replace(".", ",")} Mb/s en ${CODEC_NAMES[plan.codec] ?? plan.codec}, unos ${Math.round(plan.mb_por_minuto)} MB por minuto.`];
    if (plan.aprendido) lines.push(`Aprendió que aquí se sostiene a ${plan.aprendido} fps; sube sola cuando le sobre.`);
    else if (plan.fps >= 30) lines.push("Si pierde cuadros la próxima baja sola, y si le sobra sube hasta el refresco de tu pantalla.");
    lines.push(...plan.topes.map((tope) => `Tope: ${tope}.`));
    q("#s-rec-plan").textContent = lines.join(" ");
    const heights: [number, string][] = [[0, `Nativa (${native}p)`]];
    for (const h of [1440, 1080, 720, 480]) if (h < native) heights.push([h, `${h}p`]);
    const refresh = screen?.hz ?? 60;
    const rates: [number, string][] = [[0, "Auto"], [30, "30"], [60, "60"]];
    for (const f of [120, 144]) if (refresh >= f) rates.push([f, String(f)]);
    const shownHeight = plan.alto >= native ? 0 : plan.alto;
    const monitors =
      machine.monitores.length > 1
        ? `<div class="srow stack"><div><b>Qué pantalla</b><small>Graba solo esa.</small></div><div class="srow chips flat">${machine.monitores
            .map((m) => `<button class="chip${(s.recMonitor ? m.id === s.recMonitor : m.principal) ? " on" : ""}" data-monitor="${m.principal ? "" : esc(m.id)}">${esc(m.nombre)} · ${m.ancho}×${m.alto} · ${m.hz} Hz</button>`)
            .join("")}</div></div>`
        : "";
    q("#s-rec-manual").innerHTML = `
      <div class="srow stack"><div><b>Resolución</b></div>${segmented("s-rec-height", heights)}</div>
      <div class="srow stack"><div><b>Cuadros por segundo</b><small>Auto aprende cuántos aguanta tu máquina sin perder ninguno.</small></div>${segmented("s-rec-fps", rates)}</div>
      <div class="srow stack"><div><b>Tasa de video</b><small id="s-rec-kbps-text"></small></div><input type="range" class="range" id="s-rec-kbps" min="2" max="60" step="0.5" /></div>
      <div class="srow stack"><div><b>Velocidad o calidad</b><small>Velocidad le quita menos al juego; Calidad se ve mejor con la misma tasa.</small></div>${segmented("s-rec-speed", [[100, "Velocidad"], [50, "Equilibrio"], [0, "Calidad"]])}</div>
      <div class="srow stack"><div><b>Códec</b><small>${machine.codecs.length > 1 ? "HEVC y AV1 pesan menos con la misma calidad, pero algunos editores viejos no los abren." : "Tu gráfica codifica H.264."}</small></div>${segmented("s-rec-codec", machine.codecs.map((c) => [c, CODEC_NAMES[c] ?? c]))}</div>
      ${monitors}`;
    markSegmented(q("#s-rec-height"), String(shownHeight));
    markSegmented(q("#s-rec-fps"), s.recProfile === "propio" ? String(s.recFps) : "0");
    markSegmented(q("#s-rec-speed"), String(plan.rapidez >= 75 ? 100 : plan.rapidez >= 25 ? 50 : 0));
    markSegmented(q("#s-rec-codec"), plan.codec);
    q<HTMLInputElement>("#s-rec-kbps").value = String(plan.kbps / 1000);
    this.paintKbps(plan.kbps);
    q<HTMLInputElement>("#s-rec-cursor").checked = s.recCursor;
    q<HTMLInputElement>("#s-rec-show").checked = s.recShowCocuyo;
    q<HTMLInputElement>("#s-rec-system").checked = s.recSystem;
    q<HTMLInputElement>("#s-rec-mic").checked = s.recMic;
    q<HTMLInputElement>("#s-rec-system-vol").value = String(s.recSystemVol);
    q<HTMLInputElement>("#s-rec-mic-vol").value = String(s.recMicVol);
    q("#s-rec-system-text").textContent = `${s.recSystemVol} %`;
    q("#s-rec-mic-text").textContent = `${s.recMicVol} %`;
    q("#s-rec-system-row").classList.toggle("off", !s.recSystem);
    q("#s-rec-mic-row").classList.toggle("off", !s.recMic);
    q("#s-rec-noise-row").classList.toggle("off", !s.recMic);
    markSegmented(q("#s-rec-noise"), s.recNoise);
    markSegmented(q("#s-rec-audio"), String(s.recAudioKbps));
    q<HTMLInputElement>("#s-rec-replay").checked = s.recReplay;
    q("#s-rec-replay-row").classList.toggle("off", !s.recReplay);
    markSegmented(q("#s-rec-replay-secs"), String(s.recReplaySecs));
    const ram = Math.round(((plan.kbps / 8) * s.recReplaySecs) / 1000);
    q("#s-rec-replay-help").textContent = s.recReplay
      ? `Graba sin parar en la memoria (unos ${ram} MB) y guarda los últimos ${s.recReplaySecs} s cuando pulses el atajo o digas «guarda lo último».`
      : "Graba sin parar en la memoria y guarda lo último cuando se lo pidas. Apagada no gasta nada.";
    const mics = await call<{ id: string; nombre: string; windows: boolean }[]>("mic_list", undefined, []);
    const chip = (id: string, label: string, on: boolean) => `<button class="chip${on ? " on" : ""}" data-recmic="${esc(id)}">${esc(label)}</button>`;
    q("#s-rec-mics").innerHTML = chip("", tr("El mismo de Voz"), !s.recMicId || !mics.some((m) => m.id === s.recMicId)) + mics.map((m) => chip(m.id, m.nombre, m.id === s.recMicId)).join("");
    q("#s-rec-mics").classList.toggle("off", !s.recMic);
    const folder = await call<string | null>("current_record_dir", undefined, null);
    q("#s-rec-folder").textContent = folder ? folder + "\\" + tr("un juego o Escritorio") : "";
    q("#s-rec-reset").hidden = !s.recDir;
  }

  private guilds: { id: string; nombre: string }[] = [];

  private async loadGuilds() {
    this.guilds = await call<{ id: string; nombre: string }[]>("discord_servidores", undefined, []);
    this.paintGuilds();
  }

  private paintGuilds() {
    const chosen = this.settings?.discordServer ?? "";
    const picked = this.guilds.find((guild) => guild.id === chosen);
    const plain = (text: string) => text.toLowerCase().normalize("NFD").replace(/[\u0300-\u036f]/g, "");
    const wanted = plain(this.el.querySelector<HTMLInputElement>("#s-dc-def-find")?.value.trim() ?? "");
    const shown = this.guilds.filter((guild) => guild.id !== chosen && (!wanted || plain(guild.nombre).includes(wanted))).slice(0, wanted ? 8 : 5);
    const chip = (id: string, label: string, on: boolean) => `<button class="chip${on ? " on" : ""}" data-guild="${esc(id)}" title="${esc(label)}">${esc(label)}</button>`;
    this.el.querySelector("#s-dc-guilds")!.innerHTML =
      chip("", tr("Ninguno"), !picked) + (picked ? chip(picked.id, picked.nombre, true) : "") + shown.map((guild) => chip(guild.id, guild.nombre, false)).join("");
    this.el.querySelector("#s-dc-def-now")!.textContent = !this.guilds.length
      ? "Abre Discord para ver tus servidores."
      : picked
        ? `Si no dices el servidor y no estás en una llamada, busco los canales en «${picked.nombre}».`
        : "Ninguno: busco en el de la llamada en la que estés y, si no, en el último donde estuviste.";
  }

  private async paintMics() {
    const mics = await call<{ id: string; nombre: string; windows: boolean }[]>("mic_list", undefined, []);
    const chosen = this.settings?.mic ?? "";
    const there = mics.some((mic) => mic.id === chosen);
    const windows = mics.find((mic) => mic.windows);
    const chip = (id: string, label: string, on: boolean) => `<button class="chip${on ? " on" : ""}" data-mic="${esc(id)}">${esc(label)}</button>`;
    this.el.querySelector("#s-mic")!.innerHTML =
      chip("", windows ? `${tr("El de Windows")} · ${windows.nombre}` : tr("El de Windows"), !chosen || !there) + mics.map((mic) => chip(mic.id, mic.nombre, mic.id === chosen)).join("");
    this.el.querySelector("#s-mic-help")!.textContent = !mics.length
      ? "No encuentro ningún micrófono conectado."
      : chosen && !there
        ? "El que elegiste no está conectado; mientras tanto uso el de Windows."
        : "El que usa Cocuyo para oírte. Si no te escucha, prueba con otro.";
  }

  private remoteTick = 0;
  private remoteRedo = false;

  private async paintRemote() {
    const q = <T extends HTMLElement>(sel: string) => this.el.querySelector<T>(sel)!;
    const state = await call<RemoteState>("remoto_estado", undefined, { activo: false, punto: "", celulares: [], emparejando: null, quedan: 0 });
    const relay = q<HTMLInputElement>("#s-remote-relay");
    if (document.activeElement !== relay) relay.value = state.punto;
    q("#s-remote-page").textContent = REMOTE_PAGE;
    const host = state.punto.replace(/^wss?:\/\//, "");
    q("#s-remote-relay-sum").textContent = state.punto ? host : "Todavía no tienes. Cocuyo lo monta por ti en tu cuenta gratis de Cloudflare: son tres pasos.";
    q("#s-remote-redo").hidden = !state.punto || this.remoteRedo;
    q("#s-remote-steps").hidden = !!state.punto && !this.remoteRedo;
    q<HTMLButtonElement>("#s-remote-pair").disabled = !state.activo || !state.punto;
    q("#s-remote-pair-help").textContent = !state.activo
      ? "Enciende primero «Cocuyo en el celular»."
      : !state.punto
        ? "Primero monta tu punto de cita."
        : state.celulares.length >= 5
          ? "Ya hay cinco celulares. Quita uno para emparejar otro."
          : "Te doy un código para escribirlo en el celular. Dura dos minutos.";
    const box = q("#s-remote-code");
    const left = q("#s-remote-left");
    window.clearInterval(this.remoteTick);
    box.hidden = !state.emparejando;
    q("#s-remote-pair-row").hidden = !!state.emparejando;
    if (state.emparejando) {
      q("#s-remote-code-text").textContent = state.emparejando;
      q("#s-remote-qr").innerHTML = qrSvg(`https://${REMOTE_PAGE}/#punto=${state.punto}&codigo=${state.emparejando.replace(/-/g, "")}`);
      const until = Date.now() + state.quedan * 1000;
      const tick = () => {
        const rest = Math.max(0, Math.round((until - Date.now()) / 1000));
        left.textContent = `Vence en ${Math.floor(rest / 60)}:${String(rest % 60).padStart(2, "0")}`;
        if (rest === 0) {
          window.clearInterval(this.remoteTick);
          this.note("El código venció. Pide otro cuando tengas el celular a mano.");
          void this.paintRemote();
        }
      };
      tick();
      this.remoteTick = window.setInterval(tick, 1000);
    }
    q("#s-remote-list").innerHTML = state.celulares.length
      ? state.celulares
          .map(
            (phone) =>
              `<div class="mcp ${phone.conectado ? "ok" : "idle"}"><i></i><div><b>${esc(phone.nombre)}</b><small>${phone.conectado ? "Conectado ahora" : `Emparejado el ${new Date(phone.desde * 1000).toLocaleDateString(locale(), { day: "numeric", month: "short" })}`}</small></div><button class="ghost" data-phone="${esc(phone.id)}">Quitar</button></div>`,
          )
          .join("")
      : `<small class="remote-none">Ningún celular todavía.</small>`;
  }

  private async paintExtensions() {
    const list = await call<ExtensionInfo[]>("extensions_list", undefined, []);
    const on = this.settings?.extensionsOn ?? true;
    this.el.querySelector("#s-ext-count")!.textContent = list.length
      ? `${list.length} instaladas, ${list.filter((item) => item.activa).length} activas.`
      : "Nada todavía. Mira la galería o crea la tuya.";
    this.el.querySelector("#s-ext-list")!.innerHTML =
      list
        .map(
          (item) => `<label class="ext ${on ? "" : "off"}">
            ${item.vista ? `<img src="${esc(extensionUrl(item.vista))}" alt="">` : `<span class="ext-glyph">${item.tipo === "tema" ? "🎨" : "✨"}</span>`}
            <div><b>${esc(item.nombre)}</b><small class="desc">${esc(item.descripcion || (item.tipo === "tema" ? "Tema" : "Extensión"))}</small><small class="by">${item.tipo === "tema" ? "Tema" : "Extensión"}${item.autor ? ` · @${esc(item.autor)}` : ""}</small>${permissions(item.tipo, item.permisos)}</div>
            <input type="checkbox" class="switch" data-ext="${esc(item.id)}" ${item.activa ? "checked" : ""}>
          </label>`,
        )
        .join("") || `<small class="empty">Aquí salen los temas y las extensiones que instales.</small>`;
  }

  private async openExtensionShop() {
    const shop = this.el.querySelector<HTMLElement>("#s-ext-shop")!;
    shop.hidden = false;
    const head = `<div class="ai-head"><small>Temas y extensiones de la comunidad. Cada una dice qué usa; pasa el mouse por encima para ver qué significa. Instala solo lo que entiendas.</small><button class="ghost" data-close>Cerrar</button></div>`;
    shop.innerHTML = `${head}<small class="empty">Abriendo la galería…</small>`;
    try {
      const shared = await call<{ id: string; nombre: string; descripcion: string; autor: string; tipo: string; vista: string; permisos: Permiso[] | null; have: boolean; newer: boolean }[]>("extensions_gallery");
      shop.innerHTML = `${head}<div class="gallery shop">${shared
        .map((item) => {
          const button = item.newer
            ? `<button class="ghost" data-get="${esc(item.id)}">Actualizar</button>`
            : item.have
              ? `<button class="ghost" disabled>Instalada</button>`
              : `<button class="ghost" data-get="${esc(item.id)}">Instalar</button>`;
          const art = item.vista ? `<img class="shop-art" src="${esc(item.vista)}" alt="" loading="lazy">` : `<span class="shop-art">${item.tipo === "tema" ? "🎨" : "✨"}</span>`;
          return `<div class="shared shop-item">${art}<div><b>${esc(item.nombre)}</b><small class="desc">${esc(item.descripcion)}</small><small class="by">${item.tipo === "tema" ? "Tema" : "Extensión"}${item.autor ? ` · @${esc(item.autor)}` : ""}</small>${permissions(item.tipo, item.permisos)}</div>${button}</div>`;
        })
        .join("")}</div>`;
    } catch (error) {
      shop.innerHTML = head;
      shop.querySelector("small")!.textContent = String(error);
    }
  }

  private async openPublish() {
    const list = this.el.querySelector<HTMLElement>("#s-ai-list")!;
    const scripts = await call<{ name: string; about: string }[]>("scripts_list", undefined, []);
    const head = `<div class="ai-head"><small>Se abre GitHub con el script ya escrito. Hay que entrar con tu cuenta: así sale con tu nombre y tu foto, y nadie sube nada a escondidas.</small><button class="ghost" data-gallery>Volver</button><button class="ghost" data-close>Cerrar</button></div>`;
    list.innerHTML = `${head}<div class="gallery">${
      scripts
        .map(
          (script) =>
            `<div class="shared"><div><b>${esc(script.name)}</b><small>${esc(script.about || "Sin descripción: ponle una en la primera línea, como comentario.")}</small></div><button class="ghost" data-send="${esc(script.name)}">Subir</button></div>`,
        )
        .join("") || `<small class="empty">Todavía no tienes scripts. Ábrele la carpeta y crea el primero.</small>`
    }</div>`;
  }

  private async paintMemory() {
    const facts = await call<string[]>("memory_list", undefined, []);
    this.el.querySelector("#s-mem-count")!.textContent = facts.length
      ? `${facts.length} datos. También por voz: «recuerda que…», «olvida lo de…».`
      : "Nada todavía. Díselo por voz («recuerda que…») o escríbelo aquí.";
    this.el.querySelector("#s-mem-list")!.innerHTML = facts
      .map((fact, index) => `<div class="mem-row"><span>${esc(fact)}</span><button class="x" data-forget="${index}" aria-label="Olvidar">${ic("close")}</button></div>`)
      .reverse()
      .join("");
  }

  private wantedVoice = "";

  private async paintVoices() {
    const chosen = this.settings?.voice ?? "";
    const local = await call<{ id: string; label: string; megabytes: number; installed: boolean; installing: boolean; slow: boolean }[]>("tts_voices", undefined, []);
    const chip = (voice: string, label: string) => `<button class="chip${voice === chosen ? " on" : ""}" data-voice="${voice}">${esc(label)}</button>`;
    const locals = local
      .map((voice) =>
        voice.installed
          ? chip(`local:${voice.id}`, voice.label)
          : `<button class="chip get" data-get="${voice.id}" ${voice.installing ? "disabled" : ""}>${ic("down")} ${esc(voice.label)} · ${voice.installing ? "bajando…" : `${voice.megabytes} MB`}</button>`,
      )
      .join("");
    const gemini = ["Kore", "Puck", "Charon"].map((voice) => chip(voice, `${voice} · Gemini`)).join("");
    const language = this.settings?.language ?? "es";
    const system = (await call<{ name: string; language: string }[]>("system_voices", undefined, []))
      .filter((voice) => voice.language.toLowerCase().startsWith(language))
      .map((voice) => chip(`windows:${voice.name}`, `${voice.name} · Windows`))
      .join("");
    const meme = language === "es" ? chip("loquendero", "El Loquendero") : "";
    this.el.querySelector("#s-voice-kind")!.innerHTML = `${chip("", "Windows")}${meme}${system}${locals}${gemini}<button class="ghost" id="s-voice-try">Oír</button>`;
    const now = local.find((voice) => `local:${voice.id}` === chosen);
    const help =
      chosen === "loquendero"
        ? "La voz de los videos de 2015. Si tienes Loquendo instalado habla con la suya; si no, con Pablo, la de Windows, más grave y seca."
        : chosen.startsWith("windows:")
          ? "Una voz que ya trae Windows: habla al instante y sin descargar nada."
          : now?.slow
            ? "Kokoro es la más natural, pero pesada: en un equipo modesto una frase nueva tarda varios segundos. Las ya dichas salen al instante."
            : chosen && !chosen.startsWith("local:")
              ? "Voz de Gemini: usa tu clave con Gemini Live, sin límite diario, y suena mientras se genera. Si Gemini no responde, habla la de Windows."
              : "La de Windows, una local que suena a persona (se descarga una vez y habla sin internet) o una de Gemini.";
    this.el.querySelector("#s-voice-help")!.textContent = help;
  }

  private tested = false;
  private modelList: string[] | null = null;

  private async paintAi() {
    const saved = await call<boolean>("ai_key_saved", undefined, false);
    this.el.querySelector<HTMLInputElement>("#s-ai-key")!.placeholder = saved ? "Clave guardada" : "Clave (opcional)";
    const local = /127\.0\.0\.1|localhost/.test(this.settings?.aiUrl ?? "");
    this.el.querySelector<HTMLElement>("#s-ai-getkey")!.hidden = saved || local;
    if (!this.tested) {
      const [used, budget] = await call<[number, number]>("ai_budget", undefined, [0, 0]);
      this.el.querySelector("#s-ai-result")!.textContent = used
        ? budget
          ? `Hoy: ${used} de ${budget} consultas.`
          : `Hoy: ${used} consultas.`
        : "Envía una frase torcida y muestra qué entendió.";
    }
    const scripts = await call<{ name: string; about: string }[]>("scripts_list", undefined, []);
    this.el.querySelector("#s-scripts")!.textContent = scripts.length
      ? `Di «ejecuta…»: ${scripts.map((script) => script.name).join(", ")}`
      : "Archivos .ps1, .py o .bat: dilos por su nombre.";
  }

  private async openModels() {
    const list = this.el.querySelector<HTMLElement>("#s-ai-list")!;
    if (!list.hidden) return;
    list.hidden = false;
    list.innerHTML = `<small class="empty">Buscando modelos…</small>`;
    try {
      this.modelList = await call<string[]>("ai_models");
      this.paintModels();
    } catch (error) {
      list.innerHTML = `<div class="ai-head"><small></small><button class="ghost" data-close>Cerrar</button></div>`;
      list.querySelector("small")!.textContent = String(error);
    }
  }

  private paintModels() {
    const list = this.el.querySelector<HTMLElement>("#s-ai-list")!;
    if (list.hidden || !this.modelList) return;
    const chosen = modelsOf(this.settings?.aiModel ?? "");
    const scroll = list.querySelector(".ai-models")?.scrollTop ?? 0;
    list.innerHTML = `<div class="ai-head"><small>Elige uno o varios. Si eliges varios se turnan, y cada uno gasta de su propio cupo gratis.</small><button class="ghost" data-close>Listo</button></div><div class="ai-models"></div>`;
    const box = list.querySelector<HTMLElement>(".ai-models")!;
    for (const model of this.modelList) {
      const button = document.createElement("button");
      button.className = `chip${chosen.includes(model) ? " on" : ""}`;
      button.dataset.model = model;
      button.textContent = model;
      box.append(button);
    }
    box.scrollTop = scroll;
  }

  private closeModels() {
    this.el.querySelector<HTMLElement>("#s-ai-list")!.hidden = true;
    this.el.querySelector<HTMLElement>("#s-ai-list")!.classList.remove("wide");
    this.modelList = null;
  }

  private note(text: string) {
    this.el.querySelector("#s-note")!.textContent = text;
  }

  private async paint() {
    const s = this.settings;
    if (!s) return;
    const q = <T extends HTMLElement>(sel: string) => this.el.querySelector<T>(sel)!;

    const dock = q("#s-dock");
    [...dock.querySelectorAll<HTMLElement>("button")].forEach((button, i) => {
      const active = button.dataset.value === s.dock;
      button.classList.toggle("on", active);
      if (active) dock.style.setProperty("--i", String(i));
    });
    const lang = q("#s-lang");
    [...lang.querySelectorAll<HTMLElement>("button")].forEach((button, i) => {
      const active = button.dataset.value === s.language;
      button.classList.toggle("on", active);
      if (active) lang.style.setProperty("--i", String(i));
    });
    q("#s-dock-help").textContent = {
      auto: "Arriba; se pasa al borde derecho si taparía una ventana.",
      top: "Siempre arriba, al centro.",
      right: "Siempre en el borde derecho.",
    }[s.dock];

    q<HTMLInputElement>("#s-hide").checked = s.hideOnFullscreen;
    q<HTMLInputElement>("#s-cue").checked = s.cueFullscreen;
    q<HTMLInputElement>("#s-speak").checked = s.speakFullscreen;
    q<HTMLInputElement>("#s-layer").checked = s.gameOverlay;
    q<HTMLInputElement>("#s-auto").checked = s.autostart;
    q<HTMLInputElement>("#s-ask").checked = s.askWhere;
    q<HTMLInputElement>("#s-copy").checked = s.copyToClipboard;
    void this.paintVoices();
    q<HTMLInputElement>("#s-pad").checked = s.padOn;
    q<HTMLInputElement>("#s-ext-on").checked = s.extensionsOn;
    q<HTMLInputElement>("#s-agent").checked = s.aiAgent;
    q<HTMLInputElement>("#s-agent-hands").checked = s.agentHands;
    q<HTMLInputElement>("#s-live").checked = s.aiLive;
    q("#s-live-voice").innerHTML = LIVE_VOICES.map(([voice, kind]) => `<button class="chip${voice === (s.liveVoice || "Charon") ? " on" : ""}" data-live="${voice}">${voice} · ${tr(kind)}</button>`).join("");
    q<HTMLInputElement>("#s-agent-confirm").checked = s.agentConfirm;
    q<HTMLInputElement>("#s-remote").checked = s.remoteOn;
    q("#s-agent-need").textContent = !s.aiOn
      ? "Primero enciende un modelo de IA en la sección IA (Gemini viene listo)."
      : s.aiFormat === "anthropic"
        ? "Tu servicio usa el formato de Anthropic: el modo agente necesita uno con formato OpenAI (Gemini, Groq, Ollama…)."
        : "Listo: usa el modelo de la sección IA.";
    const barStyle = q("#s-bar-style");
    [...barStyle.querySelectorAll<HTMLElement>("button")].forEach((button, i) => {
      const active = button.dataset.value === s.barStyle;
      button.classList.toggle("on", active);
      if (active) barStyle.style.setProperty("--i", String(i));
    });
    q("#s-bar-amount-row").classList.toggle("off", s.barStyle === "normal");
    q<HTMLInputElement>("#s-bar-amount").value = String(s.barOpacity);
    paintAmount(this.el, s.barOpacity);
    paintPreview(this.el, s.barStyle, s.barOpacity, s.barCenter);
    q<HTMLInputElement>("#s-bar-center").checked = s.barCenter;
    q<HTMLInputElement>("#s-bar-solid").checked = s.barSolidMax;
    void call<{ autohide: boolean; windows11: boolean; rival: string | null; deep: boolean | null; hidden: number | null; fault: number | null } | null>("taskbar_state", undefined, null).then((bar) => {
      if (!bar) return;
      q<HTMLInputElement>("#s-bar-hide").checked = bar.autohide;
      const stuck = bar.deep === false || (bar.deep === true && s.barStyle !== "normal" && bar.hidden === 0);
      q("#s-bar-help").textContent = stuck
        ? `Windows no dejó quitarle a la barra su fondo: se verá con el efecto de Windows encima.${bar.fault ? ` (0x${bar.fault.toString(16).padStart(8, "0")})` : ""}`
        : "Se aplica al instante, en todas tus barras.";
      q("#s-bar-rival-row").hidden = !bar.rival;
      if (bar.rival) q("#s-bar-rival").textContent = `Tienes ${bar.rival} abierto. Si los dos tocan la barra se pelean.`;
    });
    q<HTMLInputElement>("#s-updates").checked = s.checkUpdates;
    if (this.section === "capturas") void this.paintRec();
    void call<string>("version", undefined, "").then((version) => (q("#s-version").textContent = `Cocuyo ${version}`));
    const into = q("#s-into");
    [...into.querySelectorAll<HTMLElement>("button")].forEach((button, i) => {
      const active = (button.dataset.value ?? "") === s.dictationInto;
      button.classList.toggle("on", active);
      if (active) into.style.setProperty("--i", String(i));
    });
    const combo = q("#s-pad-combo");
    [...combo.querySelectorAll<HTMLElement>("button")].forEach((button, i) => {
      const active = !s.padButtons && Number(button.dataset.value) === s.padCombo;
      button.classList.toggle("on", active);
      if (active) combo.style.setProperty("--i", String(i));
    });
    combo.classList.toggle("off", !s.padOn);
    if (s.padButtons) void call<string>("pad_label", { buttons: s.padButtons }, "").then((label) => (q("#s-pad-status").textContent = `Para hablar: mantén ${label}.`));
    else if (s.padOn) void call<string>("pad_status", undefined, "").then((seen) => seen && (q("#s-pad-status").textContent = seen));
    for (const [sel, value] of [["#s-name", s.name], ["#s-city", s.city]] as const) {
      const input = q<HTMLInputElement>(sel);
      if (document.activeElement !== input) input.value = value;
    }
    void this.paintMemory();
    q<HTMLInputElement>("#s-ai").checked = s.aiOn;
    q<HTMLInputElement>("#s-ai-polish").checked = s.aiDictation;
    q<HTMLInputElement>("#s-ai-hear").checked = s.aiHear;
    if (document.activeElement !== q("#s-ai-persona")) q<HTMLInputElement>("#s-ai-persona").value = s.aiPersona;
    for (const [sel, value] of [["#s-ai-url", s.aiUrl], ["#s-ai-model", s.aiModel]] as const) {
      const input = q<HTMLInputElement>(sel);
      if (document.activeElement !== input) input.value = value;
    }
    for (const button of this.el.querySelectorAll<HTMLElement>("#s-ai-presets button")) {
      button.classList.toggle("on", AI_PRESETS[Number(button.dataset.preset)][2] === s.aiUrl);
    }
    const when = q("#s-ai-when");
    [...when.querySelectorAll<HTMLElement>("button")].forEach((button, i) => {
      const active = (button.dataset.value === "si") === s.aiAlways;
      button.classList.toggle("on", active);
      if (active) when.style.setProperty("--i", String(i));
    });
    q("#s-ai-out").textContent = outgoing(s);
    q("#s-ai-when-help").textContent = s.aiAlways
      ? "En cada orden: afina más los nombres, pero tarda más."
      : "Solo cuando Cocuyo no reconoce la frase.";
    q("#s-folder-row").classList.toggle("off", s.askWhere);
    q("#s-reset").hidden = s.captureDir === null;
    q("#s-folder").textContent = (await currentCaptureDir()) ?? "";

    for (const key of [...CHORDS, ...HOTKEYS]) this.paintSaved(key);
  }

  private paintSaved(key: KeysKey) {
    const s = this.settings;
    if (!s) return;
    if (isChord(key)) this.paintKeys(key, s[key], 0, 0);
    else this.paintKeys(key, s[key].mods, s[key].vk, 0);
  }

  private paintKeys(key: KeysKey, mods: number, vk: number, down: number) {
    const row = this.el.querySelector(`[data-keys="${key}"]`)!;
    const caps = keysOf(mods).map(([bit, label]) => `<kbd class="${down & bit ? "down" : ""}">${label}</kbd>`);
    if (vk) caps.push(`<kbd>${keyLabel(vk)}</kbd>`);
    const waiting = isChord(key) ? "Mantén las teclas nuevas…" : "Pulsa el atajo nuevo…";
    row.querySelector(".chord")!.innerHTML = caps.join("") || `<small>${waiting}</small>`;
  }

  private stopRecording() {
    if (!this.recording) return;
    const key = this.recording.key;
    this.recording = null;
    setChordsPaused(false);
    setKeyEcho(false);
    const row = this.el.querySelector(`[data-keys="${key}"]`)!;
    row.classList.remove("recording");
    row.querySelector("button")!.textContent = "Cambiar";
    this.paintSaved(key);
  }
}

export const settingsPage = new SettingsPage();

interface DiscordPerson {
  id: string;
  nombre: string;
  silenciado: boolean;
  ensordecido: boolean;
  volumen: number;
  tu: boolean;
  callado: boolean;
}

interface DiscordPanel {
  mute: boolean;
  deaf: boolean;
  entrada: number;
  salida: number;
  ruido: boolean;
  eco: boolean;
  pulsar: boolean;
  canal: string | null;
  servidor: string;
  gente: DiscordPerson[];
}

let discordTimer = 0;
let discordBusy = false;
let discordDragging = false;

const DISCORD_TILES: [string, string, string][] = [
  ["d-mute", "mic", "Micrófono"],
  ["d-deaf", "headset", "Ensordecer"],
  ["d-leave", "leave", "Salir del canal"],
  ["d-ruido", "wave", "Supresión de ruido"],
  ["d-eco", "echo", "Cancelar eco"],
  ["d-pulsar", "hand", "Pulsar para hablar"],
];

function paintDiscord(el: HTMLElement, panel: DiscordPanel | null, problem = "") {
  const q = (id: string) => el.querySelector<HTMLElement>(`#${id}`)!;
  el.classList.toggle("off", !panel);
  const state = (id: string, on: boolean, yes: string, no: string) => {
    q(id).classList.toggle("on", on);
    q(id).querySelector("small")!.textContent = tr(on ? yes : no);
  };
  if (!panel) {
    for (const [id] of DISCORD_TILES) q(id).querySelector("small")!.textContent = "";
    q("d-chan").textContent = tr(problem || "Discord no está abierto.");
    q("d-guild").textContent = tr("Ábrelo y aquí verás tu canal.");
    q("d-people").innerHTML = "";
    return;
  }
  state("d-mute", panel.mute, "Silenciado", "Activo");
  state("d-deaf", panel.deaf, "Activado", "Apagado");
  state("d-ruido", panel.ruido, "Activada", "Apagada");
  state("d-eco", panel.eco, "Activada", "Apagada");
  state("d-pulsar", panel.pulsar, "Activado", "Detecta tu voz");
  q("d-leave").classList.toggle("dim", !panel.canal);
  q("d-leave").querySelector("small")!.textContent = panel.canal ? panel.canal : tr("No estás en un canal");
  if (!discordDragging) {
    setSlider(q("d-in"), panel.entrada / 100, panel.entrada);
    setSlider(q("d-out"), Math.min(1, panel.salida / 200), panel.salida);
  }
  q("d-chan").textContent = panel.canal ?? tr("No estás en un canal de voz");
  q("d-guild").textContent = panel.canal ? panel.servidor : tr("Dile «métete al canal General».");
  q("d-people").innerHTML = panel.gente
    .map((p) => {
      const status = p.ensordecido ? "Ensordecido" : p.silenciado ? "Silenciado" : p.callado ? "Silenciado para ti" : "";
      const name = p.tu ? `${esc(p.nombre)} <i>${tr("(tú)")}</i>` : esc(p.nombre);
      const level = p.tu ? "" : `<em>${Math.round(p.volumen)} %</em>`;
      return `<button class="dp${p.callado ? " quiet" : ""}${p.tu ? " me" : ""}" data-person="${esc(p.id)}" data-level="${p.volumen}"><b>${name}</b><small>${tr(status)}</small>${level}</button>`;
    })
    .join("");
}

function setSlider(slider: HTMLElement, share: number, value: number) {
  slider.style.setProperty("--v", String(Math.min(1, Math.max(0, share))));
  slider.querySelector("em")!.textContent = String(Math.round(value));
}

async function refreshDiscord(el: HTMLElement) {
  if (discordBusy) return;
  discordBusy = true;
  try {
    paintDiscord(el, await call<DiscordPanel | null>("discord_panel"));
  } catch (error) {
    paintDiscord(el, null, String(error));
  }
  discordBusy = false;
}

async function discordDo(el: HTMLElement, host: Host, accion: string, valor?: number, quien?: string) {
  try {
    paintDiscord(el, await call<DiscordPanel>("discord_hacer", { accion, valor: valor ?? null, quien: quien ?? null }));
  } catch (error) {
    host.say(String(error));
  }
}

export const discordPage: Page = {
  id: "discord",
  label: "Discord",
  w: 700,
  h: 260,
  html: () =>
    DISCORD_TILES.map(([id, icon, title]) => `<button class="tile" id="${id}"><span class="ti">${ic(icon)}</span><span><b>${title}</b><small></small></span></button>`).join("") +
    `<div class="vslider" id="d-in" title="Tu micrófono en Discord"><span class="fill"></span><em></em>${ic("mic")}</div>
     <div class="vslider" id="d-out" title="Volumen de Discord"><span class="fill"></span><em></em>${ic("volume")}</div>
     <div class="card dcard">
       <span class="lbl">Discord</span>
       <b id="d-chan"></b>
       <small id="d-guild"></small>
       <div class="dpeople" id="d-people"></div>
     </div>`,
  mount(el, host) {
    const tiles: [string, string][] = [["d-mute", "mute"], ["d-deaf", "deaf"], ["d-leave", "salir"], ["d-ruido", "ruido"], ["d-eco", "eco"], ["d-pulsar", "pulsar"]];
    for (const [id, accion] of tiles) el.querySelector(`#${id}`)!.addEventListener("click", () => void discordDo(el, host, accion));
    for (const [id, accion, top] of [["d-in", "entrada", 100], ["d-out", "salida", 200]] as [string, string, number][]) {
      const slider = el.querySelector<HTMLElement>(`#${id}`)!;
      let last = 0;
      const level = (e: PointerEvent) => {
        const r = slider.getBoundingClientRect();
        return Math.min(1, Math.max(0, 1 - (e.clientY - r.top) / r.height));
      };
      const drag = (e: PointerEvent, final: boolean) => {
        const share = level(e);
        setSlider(slider, share, share * top);
        const now = performance.now();
        if (final || now - last > 120) {
          last = now;
          void call("discord_hacer", { accion, valor: Math.round(share * top), quien: null }).catch(() => {});
        }
      };
      slider.addEventListener("pointerdown", (e) => {
        slider.setPointerCapture(e.pointerId);
        discordDragging = true;
        drag(e, false);
      });
      slider.addEventListener("pointermove", (e) => {
        if (slider.hasPointerCapture(e.pointerId)) drag(e, false);
      });
      slider.addEventListener("pointerup", (e) => {
        drag(e, true);
        discordDragging = false;
      });
    }
    const people = el.querySelector<HTMLElement>("#d-people")!;
    people.addEventListener("click", (e) => {
      const row = (e.target as HTMLElement).closest<HTMLElement>("[data-person]");
      if (!row || row.classList.contains("me")) return;
      void discordDo(el, host, "persona", undefined, row.dataset.person);
    });
    people.addEventListener(
      "wheel",
      (e) => {
        const row = (e.target as HTMLElement).closest<HTMLElement>("[data-person]");
        if (!row || row.classList.contains("me")) return;
        e.preventDefault();
        const next = Math.min(200, Math.max(0, Number(row.dataset.level ?? 100) + (e.deltaY < 0 ? 10 : -10)));
        row.dataset.level = String(next);
        const em = row.querySelector("em");
        if (em) em.textContent = `${next} %`;
        void call("discord_hacer", { accion: "persona", valor: next, quien: row.dataset.person }).catch(() => {});
      },
      { passive: false },
    );
  },
  show(el) {
    void refreshDiscord(el);
    discordTimer = window.setInterval(() => void refreshDiscord(el), 1500);
  },
  hide: () => clearInterval(discordTimer),
};

export const PAGES: Page[] = [controles, hoy, equipo, red, portapapeles, appsPage, fondos, energia, settingsPage];
