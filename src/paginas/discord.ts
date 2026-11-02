// la pestaña de discord: quien esta en el canal y los botones

import { tr } from "../i18n";
import { call } from "../bridge";
import { Host, Page, esc, ic } from "./comun";


export interface DiscordPerson {
  id: string;
  nombre: string;
  silenciado: boolean;
  ensordecido: boolean;
  volumen: number;
  tu: boolean;
  callado: boolean;
}

export interface DiscordPanel {
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

export let discordTimer = 0;
export let discordBusy = false;
export let discordDragging = false;

export const DISCORD_TILES: [string, string, string][] = [
  ["d-mute", "mic", "Micrófono"],
  ["d-deaf", "headset", "Ensordecer"],
  ["d-leave", "leave", "Salir del canal"],
  ["d-ruido", "wave", "Supresión de ruido"],
  ["d-eco", "echo", "Cancelar eco"],
  ["d-pulsar", "hand", "Pulsar para hablar"],
];

export function paintDiscord(el: HTMLElement, panel: DiscordPanel | null, problem = "") {
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

export function setSlider(slider: HTMLElement, share: number, value: number) {
  slider.style.setProperty("--v", String(Math.min(1, Math.max(0, share))));
  slider.querySelector("em")!.textContent = String(Math.round(value));
}

export async function refreshDiscord(el: HTMLElement) {
  if (discordBusy) return;
  discordBusy = true;
  try {
    paintDiscord(el, await call<DiscordPanel | null>("discord_panel"));
  } catch (error) {
    paintDiscord(el, null, String(error));
  }
  discordBusy = false;
}

export async function discordDo(el: HTMLElement, host: Host, accion: string, valor?: number, quien?: string) {
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
