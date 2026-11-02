// la burbuja de whatsapp: una luciérnaga que llega, te deja leer y responder ahí mismo, y se va en chispas
// solo se anima transform y opacity, que los mueve la gráfica sin repintar; quieta no gasta ni un cuadro

import { call, on, type Settings, type Shape } from "./bridge";
import { tr } from "./i18n";

export interface Linea {
  yo: boolean;
  texto: string;
  hora: number;
  estado?: "enviando" | "listo" | "mal";
  error?: string;
}

export interface Llegada {
  chat: string;
  nombre: string;
  texto: string;
  hora: number;
  hilo: Linea[];
  sonar: boolean;
  avatar: string | null;
  prueba?: boolean;
}

interface Chat {
  chat: string;
  nombre: string;
  avatar: string | null;
  hilo: Linea[];
  nuevos: number;
  prueba: boolean;
}

interface Curve {
  easing: string;
  duration: number;
}

const ORB = 60;
const EDGE = 26;
const REST = 26;
const GLYPH = `<svg viewBox="0 0 32 32" aria-hidden="true"><path class="ring" d="M16 4.2C9.5 4.2 4.2 9.4 4.2 15.9c0 2.2.6 4.2 1.6 6L4.1 27.9l6.2-1.6c1.7.9 3.6 1.4 5.7 1.4 6.5 0 11.8-5.3 11.8-11.8S22.5 4.2 16 4.2z"/><path class="phone" d="M11.7 10.2c-.3-.6-.6-.6-.9-.6h-.7c-.3 0-.7.1-1 .5-.4.4-1.3 1.3-1.3 3.1s1.3 3.6 1.5 3.8c.2.3 2.6 4.1 6.4 5.6 3.2 1.2 3.8 1 4.5.9.7-.1 2.2-.9 2.5-1.8.3-.9.3-1.6.2-1.8-.1-.2-.3-.3-.7-.5l-2.6-1.2c-.3-.1-.6-.2-.8.2-.3.4-.9 1.2-1.2 1.4-.2.3-.4.3-.8.1-.4-.2-1.6-.6-3-1.9-1.1-1-1.9-2.2-2.1-2.6-.2-.4 0-.6.2-.8l.6-.7c.2-.2.3-.4.4-.6.1-.3.1-.5 0-.7l-1.2-2.4z"/></svg>`;
const SEND = `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4.5 12 20 4.5 13.5 20l-2.2-6.3z"/></svg>`;
const QUICK = ["👍", "Ya voy", "Ahora no puedo", "Te llamo luego"];

function springy(stiffness: number, damping: number, fallback: Curve): Curve {
  if (!CSS.supports("transition-timing-function", "linear(0, 1)")) return fallback;
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

const POP = springy(420, 24, { easing: "cubic-bezier(.2,1.25,.35,1)", duration: 480 });
const OPEN = springy(330, 27, { easing: "cubic-bezier(.2,1.12,.3,1)", duration: 560 });
const LAND = springy(520, 17, { easing: "cubic-bezier(.2,1.5,.35,1)", duration: 620 });
const SNAP = springy(260, 24, { easing: "cubic-bezier(.2,1.1,.3,1)", duration: 600 });
const EASE_OUT = "cubic-bezier(.2,.8,.2,1)";
const EASE_IN = "cubic-bezier(.55,0,.8,.3)";

function esc(text: string) {
  return text.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);
}

function hue(name: string) {
  let h = 0;
  for (const c of name) h = (h * 31 + c.charCodeAt(0)) % 360;
  return h;
}

function initials(name: string) {
  const parts = name.replace(/[^\p{L}\p{N} ]/gu, "").trim().split(/\s+/).filter(Boolean);
  return ((parts[0]?.[0] ?? "?") + (parts[1]?.[0] ?? "")).toUpperCase();
}

function face(chat: { nombre: string; avatar: string | null }, size: number) {
  if (chat.avatar) return `<img class="wa-av" src="${esc(chat.avatar)}" width="${size}" height="${size}" alt="">`;
  return `<span class="wa-av" style="--h:${hue(chat.nombre)};width:${size}px;height:${size}px">${esc(initials(chat.nombre))}</span>`;
}

function clock(ms: number) {
  return new Date(ms).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
}

class Sonidos {
  private ctx: AudioContext | null = null;
  private cache = new Map<string, Promise<AudioBuffer | null>>();
  private sleep = 0;

  warm(name: string) {
    clearTimeout(this.sleep);
    this.ctx ??= new AudioContext();
    if (this.ctx.state === "suspended") void this.ctx.resume().catch(() => {});
    if (name) void this.load(name);
  }

  private load(name: string) {
    if (!this.cache.has(name)) {
      const ctx = this.ctx!;
      this.cache.set(
        name,
        fetch(`/sonidos/wa-${name}.ogg`)
          .then((r) => r.arrayBuffer())
          .then((bytes) => ctx.decodeAudioData(bytes))
          .catch(() => null),
      );
    }
    return this.cache.get(name)!;
  }

  async play(name: string, volume: number) {
    if (!name || volume <= 0) return;
    this.warm("");
    const buffer = await this.load(name);
    if (!buffer || !this.ctx) return;
    const source = this.ctx.createBufferSource();
    const gain = this.ctx.createGain();
    gain.gain.value = Math.min(1, volume);
    source.buffer = buffer;
    source.connect(gain).connect(this.ctx.destination);
    source.start();
    clearTimeout(this.sleep);
    this.sleep = window.setTimeout(() => void this.ctx?.suspend().catch(() => {}), buffer.duration * 1000 + 1500);
  }
}

export class Burbuja {
  onShape = () => {};
  onFocus: (on: boolean) => void = () => {};

  private root = document.createElement("div");
  private flyer!: HTMLElement;
  private orb!: HTMLButtonElement;
  private halo!: HTMLElement;
  private rim!: HTMLElement;
  private sweep!: HTMLElement;
  private count!: HTMLElement;
  private mini!: HTMLElement;
  private peek!: HTMLElement;
  private peekText!: HTMLElement;
  private panel!: HTMLElement;
  private thread!: HTMLElement;
  private input!: HTMLInputElement;
  private tabs!: HTMLElement;
  private sparks!: HTMLElement;

  private chats = new Map<string, Chat>();
  private current: string | null = null;
  private state: "fuera" | "llegando" | "quieta" | "abierta" | "yendose" = "fuera";
  private peeking = false;
  private peekToken = 0;
  private dragging = false;
  private stages = new Set<Shape>();
  private aura = 0;
  private peekShown = false;
  private panelShown = false;
  private peekTimer = 0;
  private stayTimer = 0;
  private sonidos = new Sonidos();
  private gustos = { texto: true, sonido: "kalimba", volumen: 0.45, ui: true, queda: 0, suave: false };
  private x = 0;
  private y = 0;

  constructor() {
    this.root.id = "wa";
    this.root.hidden = true;
    this.root.innerHTML = `
      <div class="wa-peek"><div class="wa-peek-in"><b class="wa-who"></b><p class="wa-said"></p></div></div>
      <section class="wa-panel" aria-label="WhatsApp">
        <header class="wa-head"><span class="wa-hface"></span><div class="wa-htext"><b class="wa-hname"></b><small class="wa-hsub">WhatsApp</small></div><div class="wa-tabs"></div><button class="wa-close" aria-label="${tr("Cerrar")}">×</button></header>
        <div class="wa-thread"></div>
        <div class="wa-quick">${QUICK.map((q) => `<button type="button" class="wa-q">${esc(tr(q))}</button>`).join("")}</div>
        <form class="wa-write"><input autocomplete="off" spellcheck="true" maxlength="2000" placeholder="${esc(tr("Escribe tu respuesta"))}"><button class="wa-send" aria-label="${tr("Enviar")}">${SEND}</button></form>
      </section>
      <div class="wa-flyer"><div class="wa-halo"></div><i class="wa-glow"></i><i class="wa-sweep"></i><button class="wa-orb" aria-label="WhatsApp">${GLYPH}<span class="wa-mini"></span><span class="wa-count"></span></button></div>
      <div class="wa-sparks"></div>`;
    document.body.append(this.root);
    const q = <T extends HTMLElement>(sel: string) => this.root.querySelector<T>(sel)!;
    this.flyer = q(".wa-flyer");
    this.orb = q(".wa-orb");
    this.halo = q(".wa-halo");
    this.rim = q(".wa-glow");
    this.sweep = q(".wa-sweep");
    this.count = q(".wa-count");
    this.mini = q(".wa-mini");
    this.peek = q(".wa-peek");
    this.peekText = q(".wa-peek-in");
    this.panel = q(".wa-panel");
    this.thread = q(".wa-thread");
    this.input = q("input");
    this.tabs = q(".wa-tabs");
    this.sparks = q(".wa-sparks");
    this.place(...this.home());
    this.wire();
    on<Llegada>("whatsapp", (llegada) => this.llega(llegada));
    on<{ chat: string; avatar: string }>("whatsapp-avatar", ({ chat, avatar }) => {
      const known = this.chats.get(chat);
      if (!known) return;
      known.avatar = avatar;
      this.paintFaces();
    });
    on<{ chat: string; hilo: Linea[] }>("whatsapp-hilo", ({ chat, hilo }) => {
      const known = this.chats.get(chat);
      if (!known) return;
      known.hilo = hilo;
      if (this.state === "abierta" && this.current === chat) this.paintThread(false);
    });
  }

  setGustos(s: Settings) {
    this.gustos = {
      texto: s.whatsappText,
      sonido: s.whatsappSound,
      volumen: s.whatsappVolume / 100,
      ui: s.whatsappUiSounds,
      queda: s.whatsappStay,
      suave: s.whatsappMotion === "suave" || matchMedia("(prefers-reduced-motion: reduce)").matches,
    };
    this.root.classList.toggle("suave", this.gustos.suave);
  }

  get open() {
    return this.state === "abierta";
  }

  shapes(): Shape[] {
    if (this.state === "fuera") return [];
    if (this.dragging) return [{ x: 0, y: 0, w: innerWidth, h: innerHeight, r: 0 }];
    const out: Shape[] = [...this.stages, this.orbArea(this.aura ? 80 : REST)];
    if (this.peekShown) out.push(this.box(this.peek, 18, 40));
    if (this.panelShown) out.push(this.box(this.panel, 18, 52));
    return out;
  }

  probar() {
    const now = Date.now();
    const hola = tr("¡Hola! Así te llegan los mensajes. Tócame para responder desde aquí 🙂");
    this.llega({ chat: "prueba", nombre: "Cocuyo", texto: hola, hora: now, hilo: [{ yo: false, texto: hola, hora: now }], sonar: true, avatar: null, prueba: true });
  }

  oir(name: string) {
    void this.sonidos.play(name, this.gustos.volumen);
    if (this.state !== "fuera") this.glow();
  }

  private home(): [number, number] {
    try {
      const saved = JSON.parse(localStorage.getItem("cocuyo.burbuja") ?? "null") as { x: number; y: number } | null;
      if (saved) return [saved.x * innerWidth, saved.y * innerHeight];
    } catch {}
    return [innerWidth - EDGE - ORB / 2, innerHeight - 120];
  }

  private place(x: number, y: number) {
    this.x = x;
    this.y = y;
    this.root.style.transform = `translate(${x}px, ${y}px)`;
    this.root.dataset.side = x > innerWidth / 2 ? "right" : "left";
    this.root.dataset.v = y > innerHeight / 2 ? "up" : "down";
  }

  private hold(stage: Shape, work: Promise<unknown>) {
    this.stages.add(stage);
    this.onShape();
    return work.finally(() => {
      this.stages.delete(stage);
      this.onShape();
    });
  }

  private orbArea(pad: number): Shape {
    return { x: this.x - ORB / 2 - pad, y: this.y - ORB / 2 - pad, w: ORB + pad * 2, h: ORB + pad * 2, r: ORB / 2 + pad };
  }

  private box(el: HTMLElement, pad: number, below: number): Shape {
    return { x: this.x + el.offsetLeft - pad, y: this.y + el.offsetTop - pad, w: el.offsetWidth + pad * 2, h: el.offsetHeight + pad + below, r: 0 };
  }

  private glowing(work: Promise<unknown>) {
    if (++this.aura === 1) this.onShape();
    void work.finally(() => {
      if (--this.aura === 0) this.onShape();
    });
  }

  private anim(el: Element, frames: Keyframe[], options: KeyframeAnimationOptions) {
    return el.animate(frames, options).finished.then(
      () => true,
      () => false,
    );
  }

  private side() {
    return this.root.dataset.side === "right" ? 1 : -1;
  }

  private wire() {
    let drag: { px: number; py: number; ox: number; oy: number; moved: boolean; trail: [number, number, number][] } | null = null;
    const speed = (trail: [number, number, number][], at: number) => {
      const [t1, x1, y1] = trail[trail.length - 1];
      const [t0, x0, y0] = trail.find(([t]) => t1 - t <= 80) ?? trail[0];
      const dt = t1 - t0;
      return at - t1 > 90 || dt < 12 ? [0, 0] : [(x1 - x0) / dt, (y1 - y0) / dt];
    };
    this.orb.addEventListener("pointerdown", (e) => {
      if (this.state === "yendose" || this.state === "llegando") return;
      this.orb.setPointerCapture(e.pointerId);
      drag = { px: e.clientX, py: e.clientY, ox: this.x, oy: this.y, moved: false, trail: [[e.timeStamp, e.clientX, e.clientY]] };
    });
    this.orb.addEventListener("pointermove", (e) => {
      if (!drag) return;
      drag.trail.push([e.timeStamp, e.clientX, e.clientY]);
      while (drag.trail.length > 2 && e.timeStamp - drag.trail[0][0] > 120) drag.trail.shift();
      if (!drag.moved && (Math.hypot(e.clientX - drag.px, e.clientY - drag.py) < 5 || this.state === "abierta")) return;
      if (!drag.moved) {
        drag.moved = true;
        this.dragging = true;
        this.hidePeek(true);
        this.root.classList.add("dragging");
        this.root.getAnimations().forEach((a) => a.cancel());
        this.onShape();
      }
      this.place(drag.ox + e.clientX - drag.px, drag.oy + e.clientY - drag.py);
    });
    const release = (e: PointerEvent) => {
      if (!drag) return;
      const was = drag;
      drag = null;
      if (!was.moved) {
        if (e.type === "pointerup") void (this.state === "abierta" ? this.close() : this.openPanel());
        return;
      }
      this.root.classList.remove("dragging");
      const [vx, vy] = speed(was.trail, e.timeStamp);
      const aimX = this.x + vx * 180;
      const aimY = this.y + vy * 180;
      if (Math.hypot(vx, vy) > 0.5 && (aimX < -40 || aimX > innerWidth + 40 || aimY < -40 || aimY > innerHeight + 40)) {
        void this.flick(vx, vy);
        return;
      }
      const left = Math.abs(vx) > 0.6 ? vx < 0 : aimX < innerWidth / 2;
      void this.snapTo(left ? EDGE + ORB / 2 : innerWidth - EDGE - ORB / 2, Math.min(innerHeight - 70, Math.max(70, aimY)));
    };
    this.orb.addEventListener("pointerup", release);
    this.orb.addEventListener("pointercancel", release);
    this.orb.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      void this.flick(this.side() * 1.4, 0.2);
    });
    this.orb.addEventListener("pointerenter", () => {
      if (this.state === "quieta" && !this.peeking && !drag && this.gustos.texto && this.current) this.showPeek();
    });
    this.peek.addEventListener("click", () => void this.openPanel());
    this.root.querySelector(".wa-close")!.addEventListener("click", () => void this.close());
    this.root.querySelector(".wa-quick")!.addEventListener("click", (e) => {
      const chip = (e.target as HTMLElement).closest<HTMLButtonElement>(".wa-q");
      if (chip) void this.send(chip.textContent ?? "", chip);
    });
    this.root.querySelector("form")!.addEventListener("submit", (e) => {
      e.preventDefault();
      void this.send(this.input.value);
    });
    this.input.addEventListener("pointerdown", () => this.onFocus(true));
    this.input.addEventListener("keydown", (e) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        void this.close();
      }
    });
    this.tabs.addEventListener("click", (e) => {
      const tab = (e.target as HTMLElement).closest<HTMLElement>("[data-chat]");
      if (!tab || tab.dataset.chat === this.current) return;
      this.current = tab.dataset.chat!;
      void this.switchTo();
    });
    this.root.addEventListener("click", (e) => e.stopPropagation());
  }

  private async snapTo(x: number, y: number) {
    const from = { x: this.x, y: this.y };
    this.place(x, y);
    try {
      localStorage.setItem("cocuyo.burbuja", JSON.stringify({ x: x / innerWidth, y: y / innerHeight }));
    } catch {}
    const area: Shape = { x: Math.min(from.x, x) - 60, y: Math.min(from.y, y) - 60, w: Math.abs(from.x - x) + 120, h: Math.abs(from.y - y) + 120, r: 0 };
    this.stages.add(area);
    this.dragging = false;
    this.onShape();
    await this.anim(this.root, [{ transform: `translate(${from.x}px, ${from.y}px)` }, { transform: `translate(${x}px, ${y}px)` }], SNAP);
    this.stages.delete(area);
    this.onShape();
  }

  private total() {
    let n = 0;
    for (const chat of this.chats.values()) n += chat.nuevos;
    return n;
  }

  private paintCount(bump: boolean) {
    const n = this.total();
    this.count.textContent = n > 9 ? "9+" : String(n);
    this.count.hidden = n === 0;
    if (bump && n) void this.anim(this.count, [{ transform: "scale(.4)" }, { transform: "none" }], POP);
  }

  private paintFaces() {
    const chat = this.current ? this.chats.get(this.current) : null;
    this.mini.innerHTML = chat ? face(chat, 24) : "";
    if (chat) {
      this.peek.querySelector(".wa-who")!.innerHTML = `${face(chat, 22)}<span>${esc(chat.nombre)}</span>`;
      this.root.querySelector(".wa-hface")!.innerHTML = face(chat, 38);
      this.root.querySelector(".wa-hname")!.textContent = chat.nombre;
    }
    const others = [...this.chats.values()].filter((c) => c.chat !== this.current);
    this.tabs.innerHTML = others.map((c) => `<button type="button" data-chat="${esc(c.chat)}" title="${esc(c.nombre)}">${face(c, 26)}${c.nuevos ? "<i></i>" : ""}</button>`).join("");
  }

  private llega(llegada: Llegada) {
    if (this.state === "yendose") this.reset();
    const known = this.chats.get(llegada.chat);
    const chat: Chat = known ?? { chat: llegada.chat, nombre: llegada.nombre, avatar: llegada.avatar, hilo: [], nuevos: 0, prueba: !!llegada.prueba };
    chat.nombre = llegada.nombre;
    chat.avatar = llegada.avatar ?? chat.avatar;
    chat.hilo = llegada.hilo.length ? llegada.hilo : [...chat.hilo, { yo: false, texto: llegada.texto, hora: llegada.hora }];
    const reading = this.state === "abierta" && this.current === llegada.chat;
    chat.nuevos = reading ? 0 : chat.nuevos + 1;
    this.chats.delete(llegada.chat);
    this.chats.set(llegada.chat, chat);
    if (this.state !== "abierta") this.current = llegada.chat;
    this.paintFaces();
    this.paintCount(this.state === "quieta");
    clearTimeout(this.stayTimer);
    const ring = llegada.sonar && this.gustos.sonido;
    if (this.state === "fuera") {
      void this.enter(ring ? this.gustos.sonido : "", llegada.texto);
      return;
    }
    if (ring) void this.sonidos.play(this.gustos.sonido, this.gustos.volumen);
    if (reading) {
      this.paintThread(true);
      this.glow();
      return;
    }
    if (this.state === "quieta") {
      this.wake();
      if (this.gustos.texto) this.showPeek(llegada.texto);
    }
    this.scheduleStay();
  }

  private glow() {
    if (this.gustos.suave) {
      void this.anim(this.halo, [{ opacity: 0.4 }, { opacity: 0.8, offset: 0.2 }, { opacity: 0.4 }], { duration: 1400, easing: "ease-in-out" });
      return;
    }
    const halo = this.anim(
      this.halo,
      [
        { transform: "scale(1)", opacity: 0.4, easing: "cubic-bezier(.2,.7,.3,1)" },
        { transform: "scale(1.5)", opacity: 0.95, offset: 0.14, easing: "cubic-bezier(.45,0,.55,1)" },
        { transform: "scale(1.14)", opacity: 0.55, offset: 0.36, easing: "cubic-bezier(.3,.6,.4,1)" },
        { transform: "scale(1.32)", opacity: 0.78, offset: 0.48, easing: "cubic-bezier(.4,0,.2,1)" },
        { transform: "scale(1)", opacity: 0.4 },
      ],
      { duration: 1900 },
    );
    const rim = this.anim(
      this.rim,
      [
        { transform: "scale(.88)", opacity: 0, easing: "cubic-bezier(.2,.7,.3,1)" },
        { transform: "scale(1.06)", opacity: 0.95, offset: 0.14, easing: "cubic-bezier(.45,0,.55,1)" },
        { transform: "scale(1)", opacity: 0.45, offset: 0.36, easing: "cubic-bezier(.3,.6,.4,1)" },
        { transform: "scale(1.04)", opacity: 0.7, offset: 0.48, easing: "cubic-bezier(.4,0,.2,1)" },
        { transform: "scale(1.12)", opacity: 0 },
      ],
      { duration: 1900 },
    );
    this.glowing(Promise.all([halo, rim]));
    void this.anim(
      this.sweep,
      [
        { transform: "rotate(-60deg)", opacity: 0 },
        { opacity: 1, offset: 0.22 },
        { transform: "rotate(300deg)", opacity: 0 },
      ],
      { duration: 1400, easing: "cubic-bezier(.45,.05,.3,1)" },
    );
  }

  private wake() {
    this.glow();
    if (this.gustos.suave) return;
    void this.anim(this.orb, [{ transform: "none" }, { transform: "rotate(-14deg)" }, { transform: "rotate(11deg)" }, { transform: "rotate(-6deg)" }, { transform: "none" }], { duration: 620, easing: "ease-out" });
  }

  private async enter(sound: string, text: string) {
    this.root.hidden = false;
    this.state = "llegando";
    this.place(this.x, this.y);
    const fromX = this.side() * 120;
    const fromY = this.root.dataset.v === "up" ? 160 : -160;
    const stage: Shape = { x: Math.min(this.x, this.x + fromX) - 90, y: Math.min(this.y, this.y + fromY) - 90, w: Math.abs(fromX) + 180, h: Math.abs(fromY) + 180, r: 0 };
    if (sound) {
      this.sonidos.warm(sound);
      setTimeout(() => void this.sonidos.play(sound, this.gustos.volumen), this.gustos.suave ? 0 : 420);
    }
    await this.hold(
      stage,
      (async () => {
        if (this.gustos.suave) {
          await this.anim(this.flyer, [{ transform: "scale(.7)", opacity: 0 }, { transform: "none", opacity: 1 }], { duration: 260, easing: EASE_OUT });
          return;
        }
        this.trail(fromX, fromY);
        await this.anim(
          this.flyer,
          [
            { transform: `translate(${fromX}px, ${fromY}px) scale(.25)`, opacity: 0 },
            { transform: `translate(${fromX * 0.32}px, ${fromY * 0.12 - 28}px) scale(.72)`, opacity: 1, offset: 0.55 },
            { transform: "none", opacity: 1 },
          ],
          { duration: 640, easing: "cubic-bezier(.3,.7,.35,1)" },
        );
        this.glowing(Promise.all([this.anim(this.orb, [{ transform: "scale(1.2, .82)" }, { transform: "none" }], LAND), this.burst(7, 46, "bloom")]));
      })(),
    );
    this.state = "quieta";
    this.glow();
    if (this.gustos.texto) this.showPeek(text);
    this.scheduleStay();
  }

  private scheduleStay() {
    clearTimeout(this.stayTimer);
    if (this.gustos.queda > 0 && this.state === "quieta") this.stayTimer = window.setTimeout(() => void this.leave(), this.gustos.queda * 1000);
  }

  private showPeek(text?: string) {
    const chat = this.current ? this.chats.get(this.current) : null;
    if (!chat || this.state === "abierta") return;
    const said = text ?? [...chat.hilo].reverse().find((l) => !l.yo)?.texto ?? "";
    this.peek.querySelector(".wa-said")!.textContent = said;
    const token = ++this.peekToken;
    this.peek.getAnimations({ subtree: true }).forEach((a) => a.cancel());
    this.peeking = true;
    this.peek.classList.add("on");
    if (!this.peekShown) {
      this.peekShown = true;
      this.onShape();
    }
    if (this.gustos.suave) void this.anim(this.peek, [{ opacity: 0 }, { opacity: 1 }], { duration: 200, easing: "ease-out" });
    else
      void Promise.all([
          this.anim(this.peek, [{ transform: `translateX(${this.side() * 12}px) scale(.6)` }, { transform: "none" }], POP),
          this.anim(this.peek, [{ opacity: 0 }, { opacity: 1 }], { duration: 160, easing: "ease-out" }),
          this.anim(this.peekText, [{ opacity: 0, transform: "translateY(4px)" }, { opacity: 1, transform: "none" }], { duration: 300, delay: 90, easing: EASE_OUT, fill: "backwards" }),
        ]);
    clearTimeout(this.peekTimer);
    this.peekTimer = window.setTimeout(() => token === this.peekToken && this.hidePeek(), Math.min(14000, 5200 + said.length * 45));
  }

  private hidePeek(fast = false) {
    clearTimeout(this.peekTimer);
    if (!this.peeking) return;
    this.peeking = false;
    const token = ++this.peekToken;
    void this.anim(this.peek, [{ opacity: 1, transform: "none" }, { opacity: 0, transform: `translateX(${this.side() * 8}px) scale(.94)` }], { duration: fast ? 130 : 220, easing: EASE_IN, fill: "forwards" }).then(() => {
      if (token !== this.peekToken) return;
      this.peek.classList.remove("on");
      this.peek.getAnimations({ subtree: true }).forEach((a) => a.cancel());
      this.peekShown = false;
      this.onShape();
    });
  }

  private async openPanel() {
    if (this.state !== "quieta" || !this.current) return;
    this.state = "abierta";
    clearTimeout(this.stayTimer);
    this.hidePeek(true);
    const chat = this.chats.get(this.current)!;
    chat.nuevos = 0;
    this.paintCount(false);
    this.paintFaces();
    if (!chat.prueba) chat.hilo = await call<Linea[]>("whatsapp_hilo", { chat: chat.chat }, chat.hilo).catch(() => chat.hilo);
    this.paintThread(false);
    this.panel.getAnimations({ subtree: true }).forEach((a) => a.cancel());
    this.panel.classList.add("on");
    this.panelShown = true;
    this.onShape();
    if (this.gustos.ui) this.oirUi("abrir");
    const opened = this.gustos.suave
      ? this.anim(this.panel, [{ opacity: 0, transform: "scale(.97)" }, { opacity: 1, transform: "none" }], { duration: 200, easing: EASE_OUT })
      : Promise.all([
          this.anim(this.panel, [{ transform: "scale(.14)" }, { transform: "none" }], OPEN),
          this.anim(this.panel, [{ opacity: 0 }, { opacity: 1 }], { duration: 170, easing: "ease-out" }),
          this.anim(this.orb, [{ transform: "scale(.86)" }, { transform: "none" }], LAND),
          ...[...this.panel.querySelectorAll<HTMLElement>(".wa-head, .wa-thread, .wa-quick, .wa-write")].map((el, i) =>
            this.anim(el, [{ opacity: 0, transform: "translateY(8px)" }, { opacity: 1, transform: "none" }], { duration: 320, delay: 110 + i * 45, easing: EASE_OUT, fill: "backwards" }),
          ),
        ]);
    this.onFocus(true);
    this.input.focus({ preventScroll: true });
    this.thread.scrollTop = this.thread.scrollHeight;
    await opened;
  }

  private async switchTo() {
    const chat = this.chats.get(this.current!)!;
    chat.nuevos = 0;
    this.paintCount(false);
    this.paintFaces();
    if (!chat.prueba) chat.hilo = await call<Linea[]>("whatsapp_hilo", { chat: chat.chat }, chat.hilo).catch(() => chat.hilo);
    const out = this.thread.animate([{ opacity: 1, transform: "none" }, { opacity: 0, transform: "translateX(-12px)" }], { duration: 140, easing: "ease-in", fill: "forwards" });
    await out.finished.catch(() => {});
    this.paintThread(false);
    void this.anim(this.thread, [{ opacity: 0, transform: "translateX(12px)" }, { opacity: 1, transform: "none" }], { duration: 220, easing: EASE_OUT });
    out.cancel();
  }

  private paintThread(animateLast: boolean) {
    const chat = this.current ? this.chats.get(this.current) : null;
    if (!chat) return;
    this.thread.innerHTML = chat.hilo
      .map((l) => `<div class="wa-line${l.yo ? ` yo ${l.estado ?? "listo"}` : ""}"${l.error ? ` title="${esc(l.error)}"` : ""}><p>${esc(l.texto)}</p><time>${clock(l.hora)}</time></div>`)
      .join("");
    this.thread.scrollTop = this.thread.scrollHeight;
    const last = this.thread.lastElementChild as HTMLElement | null;
    if (animateLast && last) void this.anim(last, [{ opacity: 0, transform: "translateY(14px) scale(.9)" }, { opacity: 1, transform: "none" }], POP);
  }

  private async send(raw: string, chip?: HTMLElement) {
    const text = raw.trim();
    const chat = this.current ? this.chats.get(this.current) : null;
    if (!text || !chat || this.state !== "abierta") return;
    this.input.value = "";
    const from = (chip ?? this.input).getBoundingClientRect();
    const mine: Linea = { yo: true, texto: text, hora: Date.now(), estado: "enviando" };
    chat.hilo.push(mine);
    this.paintThread(false);
    const line = this.thread.lastElementChild as HTMLElement;
    const to = line.getBoundingClientRect();
    if (this.gustos.ui) this.oirUi("enviado");
    void this.anim(
      line,
      [
        { transform: `translate(${from.left - to.left}px, ${from.top - to.top}px) scale(.82)`, opacity: 0.4 },
        { transform: "translate(0, -6px) scale(1.04)", opacity: 1, offset: 0.7 },
        { transform: "none", opacity: 1 },
      ],
      { duration: 460, easing: EASE_OUT },
    );
    const sent = chat.prueba ? Promise.resolve() : call("whatsapp_responder", { chat: chat.chat, texto: text });
    try {
      await sent;
      mine.estado = "listo";
      line.classList.replace("enviando", "listo");
      this.pulse();
      if (chat.prueba) {
        const said = chat.hilo.filter((l) => l.yo).length;
        setTimeout(() => {
          if (this.current !== chat.chat || this.state !== "abierta") return;
          const reply = said > 1 ? "Arrástrame al otro lado, o lánzame fuera de la pantalla y me despido 👋" : "¡Así de fácil! Nada salió de tu PC: esto es una prueba 😄";
          chat.hilo.push({ yo: false, texto: tr(reply), hora: Date.now() });
          this.paintThread(true);
          this.glow();
          void this.sonidos.play(this.gustos.sonido, this.gustos.volumen * 0.6);
        }, 1300);
      }
    } catch (error) {
      mine.estado = "mal";
      mine.error = String(error);
      line.classList.replace("enviando", "mal");
      line.title = mine.error;
      void this.anim(line, [{ transform: "none" }, { transform: "translateX(-7px)" }, { transform: "translateX(6px)" }, { transform: "translateX(-3px)" }, { transform: "none" }], { duration: 380 });
    }
  }

  private pulse() {
    const ring = document.createElement("i");
    ring.className = "wa-ring";
    this.flyer.append(ring);
    void this.anim(ring, [{ transform: "scale(1)", opacity: 0.55 }, { transform: "scale(2.1)", opacity: 0 }], { duration: 700, easing: "cubic-bezier(.2,.7,.3,1)" }).then(() => ring.remove());
  }

  private oirUi(name: string) {
    void this.sonidos.play(name, this.gustos.volumen * 0.55);
  }

  async close() {
    if (this.state !== "abierta") return;
    this.onFocus(false);
    this.input.blur();
    if (this.gustos.ui) this.oirUi("cerrar");
    const frames: Keyframe[] = this.gustos.suave
      ? [{ opacity: 1 }, { opacity: 0 }]
      : [
          { transform: "none", opacity: 1 },
          { transform: "scale(.14)", opacity: 0 },
        ];
    await this.anim(this.panel, frames, { duration: this.gustos.suave ? 160 : 300, easing: EASE_IN, fill: "forwards" });
    this.panel.classList.remove("on");
    this.panel.getAnimations({ subtree: true }).forEach((a) => a.cancel());
    this.panelShown = false;
    if (!this.gustos.suave) void this.anim(this.orb, [{ transform: "scale(1.12)" }, { transform: "none" }], LAND);
    this.state = "quieta";
    this.onShape();
    if (!this.total()) setTimeout(() => this.state === "quieta" && !this.total() && void this.leave(), 1600);
    else this.scheduleStay();
  }

  private async leave() {
    if (this.state !== "quieta") return;
    this.state = "yendose";
    this.hidePeek(true);
    await this.hold(
      { x: this.x - 120, y: this.y - 220, w: 240, h: 300, r: 0 },
      (async () => {
        void this.burst(8, 70, "rise");
        await this.anim(
          this.flyer,
          [
            { transform: "none", opacity: 1 },
            { transform: "translateY(-4px) scale(1.08)", opacity: 1, offset: 0.25 },
            { transform: "translateY(-44px) scale(.1)", opacity: 0 },
          ],
          { duration: this.gustos.suave ? 260 : 560, easing: "cubic-bezier(.5,0,.7,.4)", fill: "forwards" },
        );
      })(),
    );
    if (this.state === "yendose") this.reset();
  }

  private async flick(vx: number, vy: number) {
    if (this.state === "abierta") {
      this.onFocus(false);
      this.panel.classList.remove("on");
      this.panelShown = false;
    }
    this.state = "yendose";
    this.dragging = false;
    this.hidePeek(true);
    if (this.gustos.ui) this.oirUi("descartar");
    const len = Math.max(0.001, Math.hypot(vx, vy));
    const far = Math.max(innerWidth, innerHeight) * 0.6;
    await this.hold(
      { x: 0, y: 0, w: innerWidth, h: innerHeight, r: 0 },
      this.anim(this.flyer, [{ transform: "none", opacity: 1 }, { transform: `translate(${(vx / len) * far}px, ${(vy / len) * far}px) rotate(${vx > 0 ? 220 : -220}deg)`, opacity: 0 }], {
        duration: 520,
        easing: "cubic-bezier(.15,.6,.35,1)",
        fill: "forwards",
      }),
    );
    if (this.state !== "yendose") return;
    this.chats.clear();
    this.reset();
  }

  private reset() {
    this.state = "fuera";
    this.root.hidden = true;
    this.peeking = false;
    this.dragging = false;
    this.peekShown = false;
    this.panelShown = false;
    this.peek.classList.remove("on");
    this.panel.classList.remove("on");
    this.root.classList.remove("dragging");
    this.root.getAnimations({ subtree: true }).forEach((a) => a.cancel());
    this.sparks.replaceChildren();
    this.place(...this.home());
    for (const chat of this.chats.values()) chat.nuevos = 0;
    this.paintCount(false);
    this.onShape();
  }

  private trail(fromX: number, fromY: number) {
    for (let i = 0; i < 5; i++) {
      const dot = document.createElement("i");
      dot.className = "wa-spark";
      this.sparks.append(dot);
      const t = i / 5;
      void this.anim(
        dot,
        [
          { transform: `translate(${fromX * (1 - t)}px, ${fromY * (1 - t) - Math.sin(t * Math.PI) * 26}px) scale(${0.4 + t * 0.6})`, opacity: 0 },
          { opacity: 0.9, offset: 0.3 },
          { transform: `translate(${fromX * (1 - t) * 0.9}px, ${fromY * (1 - t) * 0.9 - 10}px) scale(.1)`, opacity: 0 },
        ],
        { duration: 640, delay: i * 70, easing: "ease-out", fill: "backwards" },
      ).then(() => dot.remove());
    }
  }

  private burst(n: number, radius: number, kind: "bloom" | "rise") {
    if (this.gustos.suave) return Promise.resolve();
    const all: Promise<void>[] = [];
    for (let i = 0; i < n; i++) {
      const dot = document.createElement("i");
      dot.className = "wa-spark";
      this.sparks.append(dot);
      const a = (i / n) * Math.PI * 2 + Math.random() * 0.5;
      const r = radius * (0.7 + Math.random() * 0.5);
      const end = kind === "bloom" ? `translate(${Math.cos(a) * r}px, ${Math.sin(a) * r}px)` : `translate(${Math.cos(a) * r * 0.6}px, ${-90 - Math.random() * 90}px)`;
      all.push(
        this.anim(dot, [{ transform: "scale(.6)", opacity: 0.95 }, { transform: `${end} scale(${0.2 + Math.random() * 0.4})`, opacity: 0 }], {
          duration: kind === "bloom" ? 620 : 900 + Math.random() * 300,
          delay: kind === "rise" ? Math.random() * 160 : 0,
          easing: "cubic-bezier(.2,.7,.3,1)",
          fill: "backwards",
        }).then(() => dot.remove()),
      );
    }
    return Promise.all(all).then(() => undefined);
  }
}
