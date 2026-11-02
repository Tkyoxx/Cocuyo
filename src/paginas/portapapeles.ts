// lo que copiaste, tambien las imagenes

import { call, type ClipListing } from "../bridge";
import { Page, esc, ic } from "./comun";

export let clips: ClipListing = { now: 0, clips: [] };

export function ago(ms: number) {
  const min = Math.floor(ms / 60000);
  if (min < 1) return "ahora";
  if (min < 60) return `hace ${min} min`;
  return `hace ${Math.floor(min / 60)} h`;
}

export function paintClips(el: HTMLElement, filter = "") {
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

export const portapapeles: Page = {
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
