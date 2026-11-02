// controles y hoy: volumen, la musica, el reloj, el clima y las tareas

import { locale, tr, weekLetters } from "../i18n";
import { call, getSettings, on, type NowPlaying, type Todo } from "../bridge";
import { Page, esc, ic } from "./comun";

export const SHORTCUTS: [string, string, string][] = [
  ["wifi", "Wi‑Fi", "network-wifi"],
  ["bluetooth", "Bluetooth", "bluetooth"],
  ["moon", "No molestar", "quiethours"],
  ["plane", "Modo avión", "network-airplanemode"],
];

export const controles: Page = {
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

export let mediaTimer = 0;
export let mediaKey: string | null = null;
export let mediaLength = 0;
export let seeking = false;

export const clock = (seconds: number) => `${Math.floor(seconds / 60)}:${String(Math.floor(seconds % 60)).padStart(2, "0")}`;

export async function paintMedia(el: HTMLElement) {
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

export function setVolumeUi(el: HTMLElement, level: number) {
  const slider = el.querySelector<HTMLElement>("#c-vol")!;
  slider.style.setProperty("--v", String(level));
  slider.querySelector("em")!.textContent = String(Math.round(level * 100));
}

export async function paintVolume(el: HTMLElement) {
  const volume = await call<{ level: number; muted: boolean } | null>("volume_get", undefined, { level: 0.5, muted: false });
  if (!volume) return;
  setVolumeUi(el, volume.level);
  const mute = el.querySelector<HTMLElement>("#c-mute")!;
  mute.classList.toggle("on", volume.muted);
  mute.querySelector("small")!.textContent = volume.muted ? "Activado" : "Apagado";
}

export async function paintMic(el: HTMLElement) {
  const muted = await call<boolean | null>("mic_muted", undefined, null);
  const mic = el.querySelector<HTMLElement>("#c-mic")!;
  mic.classList.toggle("on", muted === true);
  mic.querySelector("small")!.textContent = muted === null ? "Sin micrófono" : muted ? "Silenciado" : "Abierto";
}

export let clockTimer = 0;
export let who = "";
export let todos: Todo[] = [];
export let planned: { id: number; what: string; act: boolean; when: string }[] = [];

export function paintClock(el: HTMLElement) {
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

export interface Weather {
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

export const skyIcon = (code: number, day = true) =>
  code <= 1 ? (day ? "sun" : "moon") : code <= 3 ? "cloud" : code <= 48 ? "fog" : code <= 67 || (code >= 80 && code <= 82) ? "rain" : code <= 86 ? "snow" : "storm";

export function paintWeather(el: HTMLElement, weather: Weather | null | undefined, problem = "") {
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

export function paintTodos(el: HTMLElement) {
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

export const hoy: Page = {
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
