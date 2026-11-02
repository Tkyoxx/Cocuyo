// el lanzador de apps

import { call, type AppEntry } from "../bridge";
import { Page, esc, ic } from "./comun";

export let apps: AppEntry[] = [];
export const icons = new Map<string, string | null>();
export let iconWatcher: IntersectionObserver | null = null;

export const HUES = [150, 210, 20, 265, 40, 190, 330];
export const tint = (name: string) => `hsl(${HUES[name.charCodeAt(0) % HUES.length]} 45% 38%)`;

export function paintApps(el: HTMLElement, filter = "") {
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

export const appsPage: Page = {
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
