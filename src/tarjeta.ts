// las tarjetas que mandan los scripts y las extensiones: caen de la isla y se van solas

import { call, type Shape } from "./bridge";
import { tr } from "./i18n";

export interface CardButton {
  texto: string;
  abrir?: string;
  ejecutar?: string;
  dato?: string;
  copiar?: string;
  accion?: () => void;
}

export interface CardRow {
  texto: string;
  valor?: string;
  barra?: number | null;
}

export interface Card {
  id?: number;
  script?: string;
  titulo: string;
  texto?: string;
  imagen?: string;
  icono?: string;
  color?: string;
  botones?: CardButton[];
  filas?: CardRow[];
  segundos?: number;
}

export interface Progress {
  script: string;
  valor: number | null;
  texto: string;
}

const MAX = 3;
const MARGIN = 14;
let next = 1_000_000;

const esc = (text: string) => text.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);
const safeImage = (src: string) => /^(https:\/\/|data:image\/|http:\/\/asset\.localhost\/|asset:)/.test(src);

export class Cards {
  onShape = () => {};
  onSay = (_text: string) => {};

  private root = document.createElement("div");
  private timers = new Map<HTMLElement, number>();
  private bars = new Map<string, HTMLElement>();

  constructor() {
    this.root.id = "cards";
    document.body.append(this.root);
  }

  show(card: Card) {
    const el = document.createElement("article");
    el.className = "sc";
    el.dataset.id = String(card.id ?? next++);
    if (card.color && /^#[0-9a-f]{3,8}$/i.test(card.color)) el.style.setProperty("--tint", card.color);
    const art = card.imagen && safeImage(card.imagen) ? `<img class="sc-art" src="${esc(card.imagen)}" alt="">` : card.icono ? `<span class="sc-art sc-emoji">${esc(card.icono)}</span>` : "";
    const rows = (card.filas ?? [])
      .map((row) => {
        const bar = typeof row.barra === "number" ? `<i style="--p:${Math.max(0, Math.min(1, row.barra))}"></i>` : "";
        return `<div class="sc-row"><span>${esc(tr(row.texto))}</span>${bar}<b>${esc(row.valor ?? "")}</b></div>`;
      })
      .join("");
    const buttons = (card.botones ?? []).slice(0, 4);
    el.innerHTML = `
      <div class="sc-head">${art}<div class="sc-title"><b>${esc(tr(card.titulo))}</b>${card.script ? `<small>${esc(card.script)}</small>` : ""}</div><button class="sc-x" aria-label="${tr("Cerrar")}">×</button></div>
      ${card.texto ? `<p class="sc-text">${esc(tr(card.texto))}</p>` : ""}
      ${rows ? `<div class="sc-rows">${rows}</div>` : ""}
      ${buttons.length ? `<div class="sc-buttons">${buttons.map((b, i) => `<button data-b="${i}">${esc(tr(b.texto))}</button>`).join("")}</div>` : ""}
      <span class="sc-time"></span>`;
    el.querySelector(".sc-x")!.addEventListener("click", () => this.drop(el));
    el.querySelectorAll<HTMLButtonElement>("[data-b]").forEach((button) => {
      button.addEventListener("click", async () => {
        const chosen = buttons[Number(button.dataset.b)];
        if (chosen.accion) chosen.accion();
        else {
          button.disabled = true;
          const { texto: _, accion: __, ...action } = chosen;
          const said = await call<string>("script_action", { action }, "").catch((error) => String(error));
          if (said) this.onSay(said);
        }
        this.drop(el);
      });
    });
    el.addEventListener("pointerenter", () => {
      clearTimeout(this.timers.get(el));
      el.classList.add("held");
    });
    el.addEventListener("pointerleave", () => {
      el.classList.remove("held");
      this.later(el, 3);
    });
    el.querySelector("img")?.addEventListener("load", () => this.onShape());
    while (this.root.children.length >= MAX) this.drop(this.root.lastElementChild as HTMLElement, true);
    this.root.prepend(el);
    const seconds = Math.max(2, Math.min(120, card.segundos ?? 10));
    el.style.setProperty("--life", `${seconds}s`);
    requestAnimationFrame(() => el.classList.add("on"));
    this.later(el, seconds);
    this.onShape();
  }

  progress(update: Progress) {
    let el = this.bars.get(update.script);
    if (!el) {
      el = document.createElement("article");
      el.className = "sc sc-progress";
      el.innerHTML = `<div class="sc-head"><span class="sc-art sc-spin"></span><div class="sc-title"><b></b><small></small></div></div><div class="sc-bar"><i></i></div>`;
      el.querySelector("b")!.textContent = update.script;
      this.bars.set(update.script, el);
      this.root.prepend(el);
      requestAnimationFrame(() => el!.classList.add("on"));
    }
    el.querySelector("small")!.textContent = tr(update.texto || "Trabajando…");
    el.classList.toggle("unknown", update.valor === null);
    if (update.valor !== null) el.style.setProperty("--p", String(update.valor));
    if (update.valor !== null && update.valor >= 1) {
      this.bars.delete(update.script);
      el.classList.add("finished");
      this.later(el, 1.1);
    }
    this.onShape();
  }

  set behind(on: boolean) {
    this.root.classList.toggle("behind", on);
    this.onShape();
  }

  hideAll() {
    for (const el of [...this.root.children] as HTMLElement[]) this.drop(el, true);
    this.bars.clear();
  }

  shapes(): Shape[] {
    if (this.root.classList.contains("behind")) return [];
    return ([...this.root.children] as HTMLElement[]).map((el) => {
      const r = el.getBoundingClientRect();
      return { x: r.left - MARGIN, y: r.top - MARGIN, w: r.width + MARGIN * 2, h: r.height + MARGIN * 2, r: 22 + MARGIN };
    });
  }

  private later(el: HTMLElement, seconds: number) {
    clearTimeout(this.timers.get(el));
    this.timers.set(el, window.setTimeout(() => this.drop(el), seconds * 1000));
  }

  private drop(el: HTMLElement | null, now = false) {
    if (!el || !el.isConnected) return;
    clearTimeout(this.timers.get(el));
    this.timers.delete(el);
    el.classList.remove("on");
    el.classList.add("gone");
    const remove = () => {
      el.remove();
      this.onShape();
    };
    if (now) remove();
    else window.setTimeout(remove, 260);
  }
}
