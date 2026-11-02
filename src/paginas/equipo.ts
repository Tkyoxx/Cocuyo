// la pestaña equipo: procesador, memoria, disco y quien se come la ram

import { call, type Stats } from "../bridge";
import { tint } from "./apps";
import { Page, esc, gb } from "./comun";

export let statsTimer = 0;

export const gauge = (id: string, color: string, title: string) =>
  `<div class="card gauge" id="${id}" style="--p:0;--c:${color}">
     <svg viewBox="0 0 68 68"><circle class="track" cx="34" cy="34" r="26"/><circle class="arc" cx="34" cy="34" r="26"/></svg>
     <span class="pct"></span><b>${title}</b><small></small>
   </div>`;

export const mini = (id: string, label: string) => `<div class="card mini"><span class="lbl">${label}</span><b id="${id}"></b></div>`;

export function paintGauge(el: HTMLElement, id: string, fraction: number, sub: string) {
  const card = el.querySelector<HTMLElement>(`#${id}`)!;
  card.style.setProperty("--p", String(fraction));
  card.querySelector(".pct")!.textContent = `${Math.round(fraction * 100)}%`;
  card.querySelector("small")!.textContent = sub;
}

export async function paintStats(el: HTMLElement) {
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

export function paintTop(el: HTMLElement, top: Stats["top"]) {
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

export interface Program {
  name: string | null;
  icon: string | null;
}
export const programs = new Map<string, Program | null>();

export const equipo: Page = {
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
