// los fondos animados

import { call, on } from "../bridge";
import { Page, esc, ic } from "./comun";

export const ANIMATED = /\.(mp4|webm|mkv|mov|gif)$/i;
export let walls: string[] = [];
export let wallIndex = 0;

export let wallScreen: string | null = null;

export async function paintScreens(el: HTMLElement) {
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
export const thumbs = new Map<string, string>();

export function paintWalls(el: HTMLElement) {
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

export const fondos: Page = {
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
