// la pestaña red: prueba de velocidad y dns

import { call, on, type Dns, type Speed } from "../bridge";
import { Page, esc, ic } from "./comun";

export let dnsList: Dns[] = [];
export let dnsBusy = false;

export const mbps = (value: number | null) => (value === null ? "—" : value >= 100 ? value.toFixed(0) : value.toFixed(1));

export function paintSpeed(el: HTMLElement, speed: Speed) {
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

export function paintDns(el: HTMLElement, note = "") {
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

export async function measureDns(el: HTMLElement) {
  if (dnsBusy) return;
  dnsBusy = true;
  paintDns(el);
  dnsList = await call<Dns[]>("dns_measure", undefined, []);
  dnsBusy = false;
  paintDns(el);
}

export const red: Page = {
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
