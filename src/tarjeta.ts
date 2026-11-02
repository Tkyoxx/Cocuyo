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
  md?: boolean;
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

const safeLink = (url: string) => (/^https?:\/\/[^\s"<>]+$/i.test(url) ? url : "");

function inline(text: string) {
  return esc(text)
    .replace(/`([^`]+)`/g, "<code>$1</code>")
    .replace(/\*\*([^*]+)\*\*/g, "<b>$1</b>")
    .replace(/(^|[\s(])\*([^*\s][^*]*)\*/g, "$1<i>$2</i>")
    .replace(/\[([^\]]+)\]\((https?:[^)\s]+)\)/g, (_, label: string, url: string) => {
      const link = safeLink(url.replace(/&amp;/g, "&"));
      return link ? `<a href="${esc(link)}" data-abrir="${esc(link)}">${label}</a>` : label;
    });
}

export function markdown(text: string) {
  const out: string[] = [];
  let list: "ul" | "ol" | null = null;
  let code: string[] | null = null;
  const close = () => {
    if (list) out.push(`</${list}>`);
    list = null;
  };
  for (const line of text.replace(/\r/g, "").split("\n")) {
    if (line.trim().startsWith("```")) {
      if (code) {
        out.push(`<pre><code>${esc(code.join("\n"))}</code></pre>`);
        code = null;
      } else {
        close();
        code = [];
      }
      continue;
    }
    if (code) {
      code.push(line);
      continue;
    }
    const bullet = line.match(/^\s*[-*•]\s+(.*)$/);
    const numbered = line.match(/^\s*\d+[.)]\s+(.*)$/);
    const heading = line.match(/^\s*#{1,4}\s+(.*)$/);
    if (bullet || numbered) {
      const kind = bullet ? "ul" : "ol";
      if (list !== kind) {
        close();
        out.push(`<${kind}>`);
        list = kind;
      }
      out.push(`<li>${inline((bullet ?? numbered)![1])}</li>`);
    } else if (heading) {
      close();
      out.push(`<b class="h">${inline(heading[1])}</b>`);
    } else if (line.trim()) {
      close();
      out.push(`<p>${inline(line.trim())}</p>`);
    } else close();
  }
  if (code) out.push(`<pre><code>${esc((code as string[]).join("\n"))}</code></pre>`);
  close();
  return out.join("");
}

export class Cards {
  onShape = () => {};
  onWaiting: (waiting: boolean) => void = () => {};
  private waiting = false;
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
      ${card.texto ? (card.md ? `<div class="sc-text sc-md">${markdown(card.texto)}</div>` : `<p class="sc-text">${esc(tr(card.texto))}</p>`) : ""}
      ${rows ? `<div class="sc-rows">${rows}</div>` : ""}
      ${buttons.length ? `<div class="sc-buttons">${buttons.map((b, i) => `<button data-b="${i}">${esc(tr(b.texto))}</button>`).join("")}</div>` : ""}
      <span class="sc-time"></span>`;
    el.querySelector(".sc-x")!.addEventListener("click", () => this.drop(el));
    el.querySelectorAll<HTMLElement>("[data-abrir]").forEach((link) =>
      link.addEventListener("click", (e) => {
        e.preventDefault();
        void call("script_action", { action: { abrir: link.dataset.abrir } });
      }),
    );
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
      if (Number(el.dataset.id) >= 900000) {
        const left = Number(el.dataset.until ?? 0) - Date.now();
        this.later(el, Math.max(2, left / 1000));
      } else this.later(el, 3);
    });
    el.querySelector("img")?.addEventListener("load", () => this.onShape());
    while (this.root.children.length >= MAX) this.drop(this.root.lastElementChild as HTMLElement, true);
    this.root.prepend(el);
    this.waitingNow();
    const seconds = Math.max(2, Math.min(120, card.segundos ?? 10));
    el.style.setProperty("--life", `${seconds}s`);
    el.dataset.until = String(Date.now() + seconds * 1000);
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

  dropId(id: number) {
    const el = ([...this.root.children] as HTMLElement[]).find((card) => Number(card.dataset.id) === id);
    if (el) this.drop(el);
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

  private waitingNow() {
    const waiting = ([...this.root.children] as HTMLElement[]).some((el) => Number(el.dataset.id) >= 900000 && !el.classList.contains("gone"));
    if (waiting !== this.waiting) {
      this.waiting = waiting;
      this.onWaiting(waiting);
    }
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
      this.waitingNow();
      this.onShape();
    };
    if (now) remove();
    else window.setTimeout(remove, 260);
  }
}
