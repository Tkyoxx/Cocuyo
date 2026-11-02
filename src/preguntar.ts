// preguntarle a cocuyo por un archivo o un texto: de que trata, un resumen o lo que quieras, y seguir la charla ahi mismo

import { call, on } from "./bridge";
import { tr } from "./i18n";
import type { Size } from "./island";
import { markdown } from "./tarjeta";

export interface Archivo {
  name: string;
  path: string;
  kind: "imagen" | "texto" | "pdf" | "documento" | "otro";
  bytes: number;
}

export const SE_PREGUNTA = ["imagen", "texto", "pdf", "documento"];

const ICON: Record<Archivo["kind"], string> = { imagen: "🖼️", texto: "📄", pdf: "📕", documento: "📘", otro: "📦" };
const QUICK: [string, string][] = [
  ["¿De qué trata?", "¿De qué trata?"],
  ["Resúmelo", "Resúmelo en pocas líneas."],
  ["Lo importante", "¿Qué es lo más importante? Datos, fechas, nombres, cifras y pendientes."],
  ["Tradúcelo", "Tradúcelo al español; si ya está en español, al inglés."],
];
const SEND = `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 19V5M5 12l7-7 7 7"/></svg>`;
const WIDE = 600;

function esc(text: string) {
  return text.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);
}

function weight(bytes: number) {
  if (!bytes) return "";
  if (bytes < 1024 * 1024) return `${Math.max(1, Math.round(bytes / 1024))} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1).replace(".", ",")} MB`;
}

export class Preguntas {
  open = false;
  onOpen: (size: Size) => void = () => {};
  onClose = () => {};
  onSay: (text: string) => void = () => {};

  private file: Archivo | null = null;
  private busy = false;
  private focused = false;
  private el = document.createElement("section");
  private log: HTMLElement;
  private input: HTMLInputElement;
  private waitingEl: HTMLElement | null = null;

  constructor() {
    this.el.id = "pregunta";
    this.el.innerHTML = `
      <div class="pq-head"><span class="pq-icon"></span><div class="pq-title"><b></b><small></small></div><button class="ghost pq-x" aria-label="${tr("Cerrar")}">×</button></div>
      <div class="pq-log"></div>
      <div class="pq-quick">${QUICK.map(([label], i) => `<button class="chip" data-q="${i}">${esc(tr(label))}</button>`).join("")}</div>
      <form class="pq-ask"><input autocomplete="off" spellcheck="false" maxlength="1000" /><button class="pq-go" aria-label="${tr("Preguntar")}">${SEND}</button></form>`;
    document.querySelector("#island .shell")!.append(this.el);
    this.log = this.el.querySelector(".pq-log")!;
    this.input = this.el.querySelector("input")!;
    this.el.addEventListener("click", (e) => e.stopPropagation());
    this.el.querySelector(".pq-x")!.addEventListener("click", () => this.close());
    this.el.querySelector(".pq-quick")!.addEventListener("click", (e) => {
      const chip = (e.target as HTMLElement).closest<HTMLElement>("[data-q]");
      if (chip) void this.ask(tr(QUICK[Number(chip.dataset.q)][1]), tr(QUICK[Number(chip.dataset.q)][0]));
    });
    this.el.querySelector("form")!.addEventListener("submit", (e) => {
      e.preventDefault();
      void this.ask(this.input.value);
    });
    this.input.addEventListener("pointerdown", () => this.focus());
    this.input.addEventListener("keydown", (e) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        this.close();
      }
    });
    this.log.addEventListener("click", (e) => {
      const copy = (e.target as HTMLElement).closest<HTMLElement>("[data-copiar]");
      if (!copy) return;
      const text = copy.closest(".pq-a")?.getAttribute("data-texto") ?? "";
      void call<string>("script_action", { action: { copiar: text } }, "").then(() => {
        copy.textContent = tr("Copiado");
        setTimeout(() => (copy.textContent = tr("Copiar")), 1400);
      });
    });
    window.addEventListener("blur", () => {
      if (!this.focused) return;
      this.focused = false;
      void call("set_focusable", { on: false });
    });
    on<[number, number]>("bandeja-leyendo", ([page, of]) => {
      if (this.waitingEl) this.waitingEl.querySelector("small")!.textContent = `${tr("Leyendo la página")} ${page} ${tr("de")} ${of}…`;
    });
    on<{ pregunta: string; respuesta: string | null; error: string | null }>("bandeja-respuesta", (said) => {
      if (!this.open) return;
      this.you(said.pregunta);
      if (said.respuesta) this.answer(said.respuesta);
      else this.failed(said.error ?? tr("No pude responder."));
      this.layout();
    });
  }

  async start(file: Archivo, first?: string, label?: string) {
    await call("bandeja_abrir", { path: file.path });
    this.show(file);
    if (first) void this.ask(first, label);
    else this.focus();
  }

  show(file: Archivo, asked?: string, answered?: string) {
    this.file = file;
    this.busy = false;
    this.waitingEl = null;
    this.el.querySelector(".pq-icon")!.textContent = ICON[file.kind] ?? "📄";
    this.el.querySelector(".pq-title b")!.textContent = file.name;
    this.el.querySelector(".pq-title small")!.textContent = [tr(file.kind === "documento" ? "Documento" : file.kind === "pdf" ? "PDF" : file.kind === "imagen" ? "Imagen" : "Texto"), weight(file.bytes)].filter(Boolean).join(" · ");
    this.input.placeholder = tr("Pregúntale lo que quieras, o usa tus teclas de órdenes");
    this.log.innerHTML = "";
    if (asked && answered) {
      this.you(asked);
      this.answer(answered);
    } else {
      this.log.innerHTML = `<p class="pq-hint">${esc(tr("Toca una de abajo o escribe tu pregunta. Me acuerdo de lo que vamos hablando."))}</p>`;
    }
    this.open = true;
    this.layout();
  }

  close() {
    if (!this.open) return;
    this.open = false;
    this.file = null;
    void call("bandeja_cerrar");
    if (this.focused) {
      this.focused = false;
      this.input.blur();
      void call("set_focusable", { on: false });
    }
    this.onClose();
  }

  private focus() {
    this.focused = true;
    void call("set_focusable", { on: true }).then(() => this.input.focus({ preventScroll: true }));
  }

  private you(text: string) {
    this.log.querySelector(".pq-hint")?.remove();
    const el = document.createElement("div");
    el.className = "pq-q";
    el.textContent = text;
    this.log.append(el);
  }

  private answer(text: string) {
    const el = document.createElement("div");
    el.className = "pq-a";
    el.setAttribute("data-texto", text);
    el.innerHTML = `<div class="sc-md">${markdown(text)}</div><button class="pq-copy" data-copiar>${esc(tr("Copiar"))}</button>`;
    this.log.append(el);
  }

  private failed(text: string) {
    const el = document.createElement("div");
    el.className = "pq-a pq-mal";
    el.textContent = text;
    this.log.append(el);
  }

  private async ask(text: string, shown = text) {
    const question = text.trim();
    if (!question || this.busy || !this.file) return;
    this.busy = true;
    this.input.value = "";
    this.you(shown.trim());
    const waiting = document.createElement("div");
    waiting.className = "pq-a pq-wait";
    waiting.innerHTML = `<span class="dots"><i></i><i></i><i></i></span><small>${esc(tr(this.file.kind === "pdf" ? "Leyendo el PDF…" : "Pensando…"))}</small>`;
    this.log.append(waiting);
    this.waitingEl = waiting;
    this.layout();
    const file = this.file;
    try {
      const said = await call<string>("bandeja_preguntar", { path: file.path, pregunta: question });
      if (this.file !== file) return;
      waiting.remove();
      this.answer(said);
    } catch (error) {
      if (this.file !== file) return;
      waiting.remove();
      this.failed(String(error));
    } finally {
      if (this.waitingEl === waiting) this.waitingEl = null;
      this.busy = false;
    }
    this.layout();
    if (this.focused) this.input.focus({ preventScroll: true });
  }

  private layout() {
    if (!this.open) return;
    const chrome = 46 + 34 + 46 + 14 * 2 + 10 * 3;
    const room = Math.min(this.log.scrollHeight, Math.max(160, innerHeight - 240 - chrome));
    this.onOpen({ w: WIDE, h: Math.round(chrome + Math.max(56, room)), r: 30 });
    requestAnimationFrame(() => (this.log.scrollTop = this.log.scrollHeight));
  }
}
