// el aviso de version nueva: llega ya bajada, cuenta lo que trae y te deja elegir; al volver te recibe con lo nuevo
// todo se mueve con transform y opacity, y nada se queda animando

import { call, on } from "./bridge";
import { tr } from "./i18n";
import type { Cards } from "./tarjeta";

export interface Update {
  version: string;
  actual: string;
  tamano: number;
  novedades: [string, string][];
}

const ARROW = `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 4.5 5.8 10.7l1.6 1.6 3.5-3.5V19h2.2V8.8l3.5 3.5 1.6-1.6z"/></svg>`;
const CHECK = `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="m9.6 16.6-4.3-4.3 1.6-1.6 2.7 2.7 7.5-7.5 1.6 1.6z"/></svg>`;
const OUT = "cubic-bezier(.2,.8,.2,1)";

const esc = (text: string) => text.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);

function springy(stiffness: number, damping: number) {
  if (!CSS.supports("transition-timing-function", "linear(0, 1)")) return { easing: "cubic-bezier(.2,1.3,.35,1)", duration: 520 };
  const points: number[] = [];
  let x = 0;
  let v = 0;
  for (let i = 0; i < 360; i++) {
    v += (stiffness * (1 - x) - damping * v) / 120;
    x += v / 120;
    points.push(x);
    if (Math.abs(1 - x) < 0.002 && Math.abs(v) < 0.02) break;
  }
  const step = Math.max(1, Math.ceil(points.length / 40));
  const picked = points.filter((_, i) => i % step === 0);
  picked.push(1);
  return { easing: `linear(0, ${picked.map((p) => p.toFixed(3)).join(", ")})`, duration: Math.round((points.length / 120) * 1000) };
}

const POP = springy(420, 22);
const SOFT = springy(300, 26);

function megas(bytes: number) {
  return bytes > 0 ? `${(bytes / 1048576).toLocaleString(undefined, { maximumFractionDigits: 1 })} MB` : "";
}

function list(items: [string, string][]) {
  return items.length
    ? `<ol class="up-list">${items.map(([title, detail]) => `<li><b>${esc(tr(title))}</b>${detail ? `<span>${esc(tr(detail))}</span>` : ""}</li>`).join("")}</ol>`
    : "";
}

export class Aviso {
  private card: HTMLElement | null = null;
  private timer = 0;
  private calm = matchMedia("(prefers-reduced-motion: reduce)").matches;

  constructor(private cards: Cards) {
    on<number>("update-progress", (p) => this.progress(p));
  }

  get busy() {
    return this.card?.classList.contains("installing") ?? false;
  }

  offer(found: Update) {
    this.close();
    const card = this.build(
      "up",
      `<span class="up-badge">${ARROW}<i class="up-ring"></i></span>
       <div class="up-title"><b>${esc(tr("Cocuyo"))} <span class="up-ver"><s>${esc(found.actual)}</s><em>${esc(found.version)}</em></span></b><small>${esc([tr("Ya la bajé y la verifiqué"), megas(found.tamano)].filter(Boolean).join(" · "))}</small></div>`,
      `${list(found.novedades)}
       <div class="up-foot"><button class="up-go">${esc(tr("Actualizar ahora"))}</button><button class="up-later">${esc(tr("Al reiniciar"))}</button></div>
       <div class="up-bar"><i></i></div>
       <small class="up-hint">${esc(tr("Tarda unos segundos y Cocuyo vuelve solo. Si no eliges, se pone cuando dejes el PC quieto un rato."))}</small>`,
    );
    card.querySelector(".up-go")!.addEventListener("click", () => this.install(true));
    card.querySelector(".up-later")!.addEventListener("click", () => this.later());
    card.querySelector(".sc-x")!.addEventListener("click", () => this.later());
    this.show(card, 300);
    if (!this.calm) {
      const [old, fresh] = [card.querySelector(".up-ver s")!, card.querySelector(".up-ver em")!];
      void old.animate([{ transform: "none", opacity: 0.55 }, { transform: "translateY(-90%)", opacity: 0 }], { duration: 380, delay: 650, easing: "cubic-bezier(.5,0,.75,0)", fill: "both" });
      void fresh.animate([{ transform: "translateY(90%)", opacity: 0 }, { transform: "none", opacity: 1 }], { ...POP, delay: 820, fill: "both" });
    }
  }

  installing() {
    if (this.card?.classList.contains("up") && !this.busy) this.install(false);
    if (!this.card) return false;
    const bar = this.card.querySelector<HTMLElement>(".up-bar i")!;
    const now = getComputedStyle(bar).transform;
    bar.getAnimations().forEach((a) => a.cancel());
    void bar.animate([{ transform: now === "none" ? "scaleX(0)" : now }, { transform: "scaleX(1)" }], { duration: 1600, easing: OUT, fill: "forwards" });
    this.card.querySelector(".up-hint")!.textContent = tr("Vuelvo en un segundo…");
    return true;
  }

  welcome(version: string, items: [string, string][]) {
    this.close(true);
    const card = this.build(
      "up hola",
      `<span class="up-badge ok">${CHECK}<i class="up-ring"></i></span>
       <div class="up-title"><b>${esc(tr("Ya estás en la"))} ${esc(version)}</b><small>${esc(tr(items.length ? "Esto es lo nuevo" : "Todo quedó en su sitio"))}</small></div>`,
      `${list(items)}
       <div class="up-foot"><button class="up-go">${esc(tr("Listo"))}</button><button class="up-later">${esc(tr("Ver todo"))}</button></div>`,
    );
    card.querySelector(".up-go")!.addEventListener("click", () => this.close());
    card.querySelector(".sc-x")!.addEventListener("click", () => this.close());
    card.querySelector(".up-later")!.addEventListener("click", () => {
      void call("script_action", { action: { abrir: `https://github.com/Tkyoxx/Cocuyo/releases/tag/v${version}` } });
      this.close();
    });
    this.show(card, 45);
    if (!this.calm) this.sparks(card);
  }

  private build(kind: string, head: string, body: string) {
    const card = document.createElement("article");
    card.className = `sc ${kind}`;
    card.innerHTML = `<div class="up-head">${head}<button class="sc-x" aria-label="${esc(tr("Cerrar"))}">×</button></div>${body}`;
    card.addEventListener("pointerenter", () => clearTimeout(this.timer));
    return card;
  }

  private show(card: HTMLElement, seconds: number) {
    this.card = card;
    this.cards.mount(card);
    const rearm = (after: number) => {
      clearTimeout(this.timer);
      if (!this.busy && !card.classList.contains("deferred")) this.timer = window.setTimeout(() => this.close(), after * 1000);
    };
    card.addEventListener("pointerleave", () => rearm(Math.min(seconds, 15)));
    rearm(seconds);
    if (this.calm) return;
    const badge = card.querySelector<HTMLElement>(".up-badge")!;
    const ring = card.querySelector<HTMLElement>(".up-ring")!;
    void badge.animate([{ transform: "scale(.3) rotate(-30deg)", opacity: 0 }, { transform: "none", opacity: 1 }], { ...POP, delay: 120, fill: "backwards" });
    void ring.animate(
      [
        { transform: "scale(.9)", opacity: 0.7 },
        { transform: "scale(1.9)", opacity: 0, offset: 0.48 },
        { transform: "scale(.9)", opacity: 0.6, offset: 0.5 },
        { transform: "scale(1.9)", opacity: 0 },
      ],
      { duration: 2200, delay: 260, easing: "cubic-bezier(.2,.6,.3,1)", fill: "both" },
    );
    [...card.querySelectorAll<HTMLElement>(".up-list li, .up-foot, .up-hint")].forEach(
      (el, i) => void el.animate([{ opacity: 0, transform: "translateY(8px)" }, { opacity: 1, transform: "none" }], { ...SOFT, delay: 200 + i * 70, fill: "backwards" }),
    );
    const sweep = document.createElement("i");
    sweep.className = "up-sweep";
    card.append(sweep);
    void sweep.animate([{ transform: "translateX(-120%) skewX(-18deg)", opacity: 0 }, { opacity: 1, offset: 0.3 }, { transform: "translateX(320%) skewX(-18deg)", opacity: 0 }], { duration: 1300, delay: 450, easing: "cubic-bezier(.4,0,.2,1)", fill: "both" }).finished.then(
      () => sweep.remove(),
      () => sweep.remove(),
    );
  }

  private sparks(card: HTMLElement) {
    const badge = card.querySelector<HTMLElement>(".up-badge")!;
    for (let i = 0; i < 9; i++) {
      const dot = document.createElement("i");
      dot.className = "up-spark";
      badge.append(dot);
      const a = (i / 9) * Math.PI * 2 + Math.random() * 0.4;
      const r = 30 + Math.random() * 18;
      void dot
        .animate([{ transform: "scale(.6)", opacity: 0.95 }, { transform: `translate(${Math.cos(a) * r}px, ${Math.sin(a) * r}px) scale(${0.2 + Math.random() * 0.4})`, opacity: 0 }], {
          duration: 700 + Math.random() * 250,
          delay: 300,
          easing: "cubic-bezier(.2,.7,.3,1)",
          fill: "both",
        })
        .finished.then(
          () => dot.remove(),
          () => dot.remove(),
        );
    }
  }

  private install(asked: boolean) {
    const card = this.card;
    if (!card) return;
    clearTimeout(this.timer);
    card.classList.add("installing");
    card.querySelectorAll<HTMLButtonElement>("button").forEach((b) => (b.disabled = true));
    card.querySelector(".up-hint")!.textContent = tr("Preparando la nueva versión…");
    this.cards.reshape();
    if (asked) void call("update_choice", { now: true });
  }

  private progress(p: number) {
    if (!this.card || !this.busy) return;
    const bar = this.card.querySelector<HTMLElement>(".up-bar i")!;
    bar.style.transform = `scaleX(${Math.max(0, Math.min(1, p)) * 0.92})`;
    this.card.querySelector(".up-hint")!.textContent = `${tr("Bajando")} ${Math.round(p * 100)} %`;
  }

  private later() {
    const card = this.card;
    if (!card || this.busy) return;
    void call("update_choice", { now: false });
    card.classList.add("deferred");
    card.querySelectorAll<HTMLButtonElement>(".up-foot button").forEach((b) => (b.disabled = true));
    card.querySelector(".up-hint")!.textContent = tr("Listo: se pone sola la próxima vez que abras Cocuyo.");
    clearTimeout(this.timer);
    this.timer = window.setTimeout(() => this.close(), 2600);
  }

  close(force = false) {
    clearTimeout(this.timer);
    if (!this.card || (this.busy && !force)) return;
    this.cards.unmount(this.card);
    this.card = null;
  }
}
