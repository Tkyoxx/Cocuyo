// la burbuja de whatsapp: una luciérnaga que llega, te deja leer y responder ahí mismo, y se va en chispas
// solo se anima transform y opacity, que los mueve la gráfica sin repintar; quieta no gasta ni un cuadro

import { convertFileSrc } from "@tauri-apps/api/core";
import { call, on, type Settings, type Shape } from "./bridge";
import { tr } from "./i18n";

export interface Media {
  tipo: "sticker" | "foto" | "nota" | "video" | "documento" | "ubicacion" | "contacto";
  ruta?: string;
  nombre?: string;
  enlace?: string;
}

export interface Linea {
  yo: boolean;
  texto: string;
  hora: number;
  media?: Media;
  estado?: "enviando" | "listo" | "entregado" | "leido" | "mal";
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
  borrador: string;
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
const CROSS = `<svg viewBox="0 0 10 10" aria-hidden="true"><path d="M2.6 2.6l4.8 4.8M7.4 2.6 2.6 7.4"/></svg>`;
const BOLT = `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M13.2 2.8 5.6 13.1h5.2l-1 8.1 7.6-10.3h-5.2z"/></svg>`;
const PLAY = `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M8 5.5v13l10.5-6.5z"/></svg>`;
const PAUSE = `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 5h3.6v14H7zm6.4 0H17v14h-3.6z"/></svg>`;
const MEDIA_ICON: Record<Media["tipo"], string> = { sticker: "✨", foto: "📷", nota: "🎤", video: "🎥", documento: "📄", ubicacion: "📍", contacto: "👤" };
const MEDIA_NAME: Record<Media["tipo"], string> = { sticker: "Sticker", foto: "Foto", nota: "Nota de voz", video: "Video", documento: "Documento", ubicacion: "Ubicación", contacto: "Contacto" };

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

function src(media?: Media) {
  if (!media?.ruta) return "";
  return /^(data:|https?:)/.test(media.ruta) ? media.ruta : convertFileSrc(media.ruta);
}

function mediaHtml(media: Media) {
  const file = src(media);
  const label = esc(media.nombre || tr(MEDIA_NAME[media.tipo]));
  if ((media.tipo === "sticker" || media.tipo === "foto") && file)
    return `<img class="wa-pic ${media.tipo}" src="${esc(file)}" alt="${label}" decoding="async"${media.tipo === "foto" ? ` data-open="${esc(media.ruta!)}"` : ""}>`;
  if (media.tipo === "nota" && file)
    return `<div class="wa-note" data-src="${esc(file)}"><button type="button" class="wa-play" aria-label="${esc(tr("Escuchar"))}">${PLAY}</button><span class="wa-track"><i></i></span><time>0:00</time></div>`;
  const target = media.enlace ?? media.ruta;
  const tag = target ? `button type="button" data-open="${esc(target)}"` : "span";
  return `<${tag} class="wa-file"><b>${MEDIA_ICON[media.tipo]}</b><span>${label}</span></${target ? "button" : "span"}>`;
}

function seconds(total: number) {
  if (!Number.isFinite(total)) return "0:00";
  const s = Math.max(0, Math.round(total));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

class Sonidos {
  private ctx: AudioContext | null = null;
  private cache = new Map<string, Promise<AudioBuffer | null>>();
  private sleep = 0;

  warm(name: string) {
    clearTimeout(this.sleep);
    if (this.ctx?.state === "closed") this.ctx = null;
    this.ctx ??= new AudioContext({ latencyHint: "interactive" });
    if (this.ctx.state !== "running") void this.ctx.resume().catch(() => {});
    if (name) void this.load(name);
  }

  private load(name: string) {
    if (!this.cache.has(name)) {
      const ctx = this.ctx!;
      const loading = fetch(`/sonidos/wa-${name}.ogg`)
        .then((r) => (r.ok ? r.arrayBuffer() : Promise.reject(new Error(String(r.status)))))
        .then((bytes) => ctx.decodeAudioData(bytes))
        .catch(() => {
          this.cache.delete(name);
          return null;
        });
      this.cache.set(name, loading);
    }
    return this.cache.get(name)!;
  }

  private async awake() {
    const ctx = this.ctx!;
    if (ctx.state === "running") return true;
    await Promise.race([ctx.resume().catch(() => {}), new Promise((done) => setTimeout(done, 400))]);
    return (ctx.state as AudioContextState) === "running";
  }

  async play(name: string, volume: number) {
    if (!name || volume <= 0) return;
    this.warm("");
    const buffer = await this.load(name);
    if (!buffer || !(await this.awake())) {
      void this.ctx?.close().catch(() => {});
      this.ctx = null;
      this.cache.clear();
      const plain = new Audio(`/sonidos/wa-${name}.ogg`);
      plain.volume = Math.min(1, volume);
      await plain.play().catch(() => {});
      return;
    }
    const ctx = this.ctx!;
    const source = ctx.createBufferSource();
    const gain = ctx.createGain();
    gain.gain.value = Math.min(1, volume);
    source.buffer = buffer;
    source.connect(gain).connect(ctx.destination);
    source.start();
    clearTimeout(this.sleep);
    this.sleep = window.setTimeout(() => void ctx.suspend().catch(() => {}), buffer.duration * 1000 + 1500);
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
  private input!: HTMLTextAreaElement;
  private attachBar!: HTMLElement;
  private adjuntos: string[] = [];
  private rail!: HTMLElement;
  private thumb!: HTMLElement;
  private thumbSize = 0;
  private railTimer = 0;
  private quick!: HTMLElement;
  private bolt!: HTMLButtonElement;
  private playing: { audio: HTMLAudioElement; note: HTMLElement } | null = null;
  private blurCheck = 0;
  private openedAt = 0;
  private reclaimed = false;
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
  private gustos = { texto: true, sonido: "kalimba", volumen: 0.45, ui: true, queda: 0, suave: false, rapidas: "boton", respuestas: ["👍", "Ya voy", "Ahora no puedo", "Te llamo luego"] };
  private x = 0;
  private y = 0;

  constructor() {
    this.root.id = "wa";
    this.root.hidden = true;
    this.root.innerHTML = `
      <div class="wa-peek"><div class="wa-peek-in"><b class="wa-who"></b><div class="wa-peek-body"><img class="wa-peek-pic" alt="" hidden><p class="wa-said"></p></div></div></div>
      <section class="wa-panel" aria-label="WhatsApp">
        <header class="wa-head"><span class="wa-hface"></span><div class="wa-htext"><b class="wa-hname"></b><small class="wa-hsub">WhatsApp</small></div><div class="wa-tabs"></div><button class="wa-close" aria-label="${tr("Cerrar")}">×</button></header>
        <div class="wa-scroll"><div class="wa-thread"></div><div class="wa-rail off"><span class="wa-thumb"><b></b></span></div></div>
        <div class="wa-quick" hidden></div>
        <div class="wa-attach" hidden></div>
        <form class="wa-write"><button type="button" class="wa-bolt" aria-label="${esc(tr("Respuestas rápidas"))}" title="${esc(tr("Respuestas rápidas"))}">${BOLT}</button><textarea rows="1" autocomplete="off" spellcheck="true" maxlength="2000" placeholder="${esc(tr("Escribe tu respuesta"))}"></textarea><button class="wa-send" aria-label="${tr("Enviar")}">${SEND}</button></form>
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
    this.input = q("textarea");
    this.rail = q(".wa-rail");
    this.thumb = q(".wa-thumb");
    this.quick = q(".wa-quick");
    this.attachBar = q(".wa-attach");
    this.attachBar.addEventListener("click", (e) => {
      const out = (e.target as HTMLElement).closest<HTMLElement>("[data-quitar]");
      if (!out) return;
      this.adjuntos.splice(Number(out.dataset.quitar), 1);
      this.paintAttach();
    });
    this.bolt = q(".wa-bolt");
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
    on("whatsapp-leidos", () => {
      for (const chat of this.chats.values()) chat.nuevos = 0;
      this.paintCount(false);
      this.paintFaces();
    });
    on<{ chat: string; hilo: Linea[] }>("whatsapp-hilo", ({ chat, hilo }) => {
      const known = this.chats.get(chat);
      if (!known) return;
      const same = known.hilo.length === hilo.length && hilo.every((l, i) => l.texto === known.hilo[i].texto && l.yo === known.hilo[i].yo);
      known.hilo = hilo;
      if (this.state !== "abierta" || this.current !== chat) return;
      if (same) this.paintTicks();
      else this.paintThread(false);
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
      rapidas: s.whatsappQuick || "boton",
      respuestas: (s.whatsappReplies ?? []).map((r) => r.trim()).filter(Boolean).slice(0, 8),
    };
    this.root.classList.toggle("suave", this.gustos.suave);
    this.quick.innerHTML = this.gustos.respuestas.map((r) => `<button type="button" class="wa-q">${esc(r)}</button>`).join("");
    this.bolt.hidden = this.gustos.rapidas !== "boton" || !this.gustos.respuestas.length;
    this.showQuick(this.gustos.rapidas === "siempre" && this.gustos.respuestas.length > 0, false);
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

  probarHilo(hilo: Linea[]) {
    const last = hilo[hilo.length - 1];
    this.llega({ chat: "prueba", nombre: "Cocuyo", texto: last.texto || tr(MEDIA_NAME[last.media?.tipo ?? "sticker"]), hora: last.hora, hilo, sonar: true, avatar: null, prueba: true });
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
    this.quick.addEventListener("click", (e) => {
      const chip = (e.target as HTMLElement).closest<HTMLButtonElement>(".wa-q");
      if (!chip) return;
      const said = chip.textContent ?? "";
      const now = this.input.value.trim();
      this.input.value = now && /^\p{Extended_Pictographic}/u.test(said) ? `${now} ${said}` : said;
      this.fit();
      this.input.focus({ preventScroll: true });
      this.input.setSelectionRange(this.input.value.length, this.input.value.length);
      if (this.gustos.rapidas === "boton") this.showQuick(false, true);
    });
    this.bolt.addEventListener("click", () => {
      this.showQuick(!!this.quick.hidden, true);
      this.input.focus({ preventScroll: true });
    });
    this.root.querySelector("form")!.addEventListener("submit", (e) => {
      e.preventDefault();
      void this.send(this.input.value);
    });
    this.input.addEventListener("pointerdown", () => this.onFocus(true));
    this.input.addEventListener("input", () => this.fit());
    this.input.addEventListener("keydown", (e) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        void this.close();
      } else if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
        e.preventDefault();
        void this.send(this.input.value);
      } else if (e.ctrlKey && e.key.toLowerCase() === "w" && this.current) {
        e.preventDefault();
        void this.closeChat(this.current);
      }
    });
    this.root.querySelector(".wa-hface")!.addEventListener("click", (e) => {
      if (this.current && (e.target as HTMLElement).closest(".wa-x")) void this.closeChat(this.current);
    });
    this.thread.addEventListener("click", (e) => {
      const target = e.target as HTMLElement;
      const play = target.closest<HTMLElement>(".wa-play");
      if (play) {
        this.toggleNote(play.parentElement!);
        return;
      }
      const opener = target.closest<HTMLElement>("[data-open]");
      if (opener && !this.chats.get(this.current ?? "")?.prueba) void call("whatsapp_abrir", { que: opener.dataset.open });
    });
    this.thread.addEventListener(
      "load",
      () => {
        this.thread.scrollTop = this.thread.scrollHeight;
        this.measureRail();
      },
      true,
    );
    this.thread.addEventListener("scroll", () => this.placeThumb(true), { passive: true });
    this.thumb.addEventListener("pointerdown", (e) => {
      e.preventDefault();
      e.stopPropagation();
      this.thumb.setPointerCapture(e.pointerId);
      this.rail.classList.add("drag");
      const start = { y: e.clientY, top: this.thread.scrollTop };
      const move = (m: PointerEvent) => {
        const span = this.rail.clientHeight - this.thumbSize;
        const room = this.thread.scrollHeight - this.thread.clientHeight;
        if (span > 0) this.thread.scrollTop = start.top + ((m.clientY - start.y) * room) / span;
      };
      const up = () => {
        this.rail.classList.remove("drag");
        this.thumb.removeEventListener("pointermove", move);
        this.thumb.removeEventListener("pointerup", up);
        this.thumb.removeEventListener("pointercancel", up);
        this.placeThumb(true);
      };
      this.thumb.addEventListener("pointermove", move);
      this.thumb.addEventListener("pointerup", up);
      this.thumb.addEventListener("pointercancel", up);
    });
    this.rail.addEventListener("pointerdown", (e) => {
      if (e.target !== this.rail) return;
      const thumbTop = this.thumb.getBoundingClientRect().top;
      this.thread.scrollBy({ top: (e.clientY < thumbTop ? -1 : 1) * this.thread.clientHeight * 0.85, behavior: "smooth" });
    });
    document.addEventListener(
      "pointerdown",
      (e) => {
        if (this.state === "abierta" && !this.root.contains(e.target as Node)) void this.close();
      },
      true,
    );
    window.addEventListener("blur", () => {
      if (this.state !== "abierta") return;
      clearTimeout(this.blurCheck);
      if (performance.now() - this.openedAt < 800 && !this.reclaimed) {
        this.reclaimed = true;
        this.onFocus(true);
        this.input.focus({ preventScroll: true });
        return;
      }
      this.blurCheck = window.setTimeout(() => {
        if (this.state === "abierta" && !document.hasFocus()) void this.close();
      }, 150);
    });
    this.tabs.addEventListener("click", (e) => {
      const tab = (e.target as HTMLElement).closest<HTMLElement>("[data-chat]");
      if (!tab) return;
      if ((e.target as HTMLElement).closest(".wa-x")) {
        void this.closeChat(tab.dataset.chat!);
        return;
      }
      if (tab.dataset.chat === this.current) return;
      this.keepDraft();
      this.current = tab.dataset.chat!;
      void this.switchTo();
    });
    this.tabs.addEventListener("auxclick", (e) => {
      const tab = (e.target as HTMLElement).closest<HTMLElement>("[data-chat]");
      if (tab && e.button === 1) void this.closeChat(tab.dataset.chat!);
    });
    this.root.addEventListener("click", (e) => e.stopPropagation());
  }

  private showQuick(on: boolean, animate: boolean) {
    if (this.quick.hidden === !on) return;
    this.quick.hidden = !on;
    this.bolt.classList.toggle("on", on);
    if (on && animate && !this.gustos.suave)
      [...this.quick.children].forEach(
        (chip, i) => void this.anim(chip, [{ opacity: 0, transform: "translateY(6px) scale(.9)" }, { opacity: 1, transform: "none" }], { duration: 260, delay: i * 35, easing: EASE_OUT, fill: "backwards" }),
      );
  }

  private fit() {
    const before = this.input.style.height;
    this.input.style.height = "auto";
    this.input.style.height = `${Math.min(96, this.input.scrollHeight)}px`;
    if (this.input.style.height !== before) this.measureRail();
  }

  private measureRail() {
    const { scrollHeight: total, clientHeight: view } = this.thread;
    const overflow = total - view > 2;
    this.rail.classList.toggle("off", !overflow);
    if (!overflow) return;
    const track = this.rail.clientHeight;
    const size = Math.max(28, Math.round((track * view) / total));
    if (size !== this.thumbSize) {
      this.thumbSize = size;
      this.thumb.style.height = `${size}px`;
    }
    this.placeThumb(false);
  }

  private placeThumb(wake: boolean) {
    const { scrollTop, scrollHeight, clientHeight } = this.thread;
    const room = scrollHeight - clientHeight;
    const span = this.rail.clientHeight - this.thumbSize;
    const y = room > 0 && span > 0 ? (Math.min(room, Math.max(0, scrollTop)) / room) * span : 0;
    this.thumb.style.transform = `translateY(${y.toFixed(1)}px)`;
    if (!wake) return;
    this.rail.classList.add("on");
    clearTimeout(this.railTimer);
    this.railTimer = window.setTimeout(() => this.rail.classList.remove("on"), 1100);
  }

  private paintTicks() {
    const chat = this.current ? this.chats.get(this.current) : null;
    if (!chat) return;
    const lines = this.thread.children;
    chat.hilo.forEach((l, i) => {
      const el = lines[i];
      if (!l.yo || !el) return;
      el.classList.remove("enviando", "listo", "entregado", "leido");
      el.classList.add(l.estado ?? "listo");
    });
  }

  private tick(line: HTMLElement, mine: Linea, estado: "listo" | "entregado" | "leido") {
    mine.estado = estado;
    line.classList.remove("enviando", "listo", "entregado", "leido");
    line.classList.add(estado);
  }

  private toggleNote(note: HTMLElement) {
    if (this.playing?.note === note) {
      const { audio } = this.playing;
      if (audio.paused) void audio.play().catch(() => {});
      else audio.pause();
      return;
    }
    this.stopNote();
    const audio = new Audio(note.dataset.src);
    const bar = note.querySelector<HTMLElement>(".wa-track i")!;
    const time = note.querySelector("time")!;
    const button = note.querySelector(".wa-play")!;
    audio.volume = Math.min(1, Math.max(0.2, this.gustos.volumen * 2));
    audio.addEventListener("loadedmetadata", () => (time.textContent = seconds(audio.duration)));
    audio.addEventListener("timeupdate", () => {
      bar.style.transform = `scaleX(${audio.duration ? audio.currentTime / audio.duration : 0})`;
      time.textContent = seconds(audio.currentTime);
    });
    audio.addEventListener("play", () => {
      button.innerHTML = PAUSE;
      note.classList.add("on");
    });
    audio.addEventListener("pause", () => {
      button.innerHTML = PLAY;
      note.classList.remove("on");
    });
    audio.addEventListener("ended", () => {
      bar.style.transform = "scaleX(0)";
      time.textContent = seconds(audio.duration);
    });
    this.playing = { audio, note };
    void audio.play().catch(() => {});
  }

  private stopNote() {
    if (!this.playing) return;
    const { audio } = this.playing;
    this.playing = null;
    audio.pause();
    audio.removeAttribute("src");
    audio.load();
  }

  takes(x: number, y: number) {
    if (this.state === "fuera" || this.state === "yendose" || !this.current) return false;
    const inside = (el: HTMLElement) => {
      const r = el.getBoundingClientRect();
      return r.width > 0 && x >= r.left - 14 && x <= r.right + 14 && y >= r.top - 14 && y <= r.bottom + 14;
    };
    return inside(this.orb) || (this.state === "abierta" && inside(this.panel));
  }

  hovering(on: boolean) {
    this.root.classList.toggle("soltando", on);
  }

  async attach(paths: string[]) {
    this.hovering(false);
    if (this.state === "quieta") await this.openPanel();
    if (this.state !== "abierta") return;
    for (const path of paths.slice(0, 10)) if (!this.adjuntos.includes(path)) this.adjuntos.push(path);
    this.paintAttach();
    this.input.focus({ preventScroll: true });
  }

  private paintAttach() {
    const name = (path: string) => path.split(/[\\/]/).pop() ?? path;
    const icon = (path: string) => (/\.(jpe?g|png|webp)$/i.test(path) ? "📷" : /\.(mp4|mov|mkv|3gp)$/i.test(path) ? "🎥" : /\.(mp3|ogg|opus|m4a|aac|wav)$/i.test(path) ? "🎤" : "📄");
    this.attachBar.innerHTML = this.adjuntos.map((p, i) => `<span class="wa-chip">${icon(p)} <b>${esc(name(p))}</b><button type="button" data-quitar="${i}" aria-label="${esc(tr("Quitar"))}">×</button></span>`).join("");
    this.attachBar.hidden = !this.adjuntos.length;
  }

  private async sendFiles(text: string, chat: Chat) {
    const files = this.adjuntos.splice(0);
    this.paintAttach();
    this.input.value = "";
    chat.borrador = "";
    this.fit();
    if (this.gustos.ui) this.oirUi("enviado");
    for (const [i, path] of files.entries()) {
      const nombre = path.split(/[\\/]/).pop() ?? path;
      const tipo = /\.(jpe?g|png|webp)$/i.test(path) ? "foto" : /\.(mp4|mov|mkv|3gp)$/i.test(path) ? "video" : /\.(mp3|ogg|opus|m4a|aac|wav)$/i.test(path) ? "nota" : "documento";
      const mine: Linea = { yo: true, texto: i === 0 ? text : "", hora: Date.now(), estado: "enviando", media: { tipo, nombre } };
      chat.hilo.push(mine);
      this.paintThread(true);
      const line = this.thread.lastElementChild as HTMLElement;
      try {
        if (!chat.prueba) await call("whatsapp_enviar_archivo", { chat: chat.chat, path, texto: mine.texto });
        this.tick(line, mine, "listo");
      } catch (error) {
        mine.estado = "mal";
        mine.error = String(error);
        line.classList.replace("enviando", "mal");
        line.title = mine.error;
      }
    }
    this.pulse();
  }

  private keepDraft() {
    const chat = this.current ? this.chats.get(this.current) : null;
    if (chat) chat.borrador = this.input.value;
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
      const head = this.root.querySelector<HTMLElement>(".wa-hface")!;
      head.innerHTML = `${face(chat, 38)}<span class="wa-x" title="${esc(tr("Cerrar este chat"))}">${CROSS}</span>`;
            this.root.querySelector(".wa-hname")!.textContent = chat.nombre;
    }
    const others = [...this.chats.values()].filter((c) => c.chat !== this.current);
    this.tabs.innerHTML = others
      .map((c) => `<button type="button" data-chat="${esc(c.chat)}" title="${esc(c.nombre)}">${face(c, 26)}${c.nuevos ? `<i class="wa-dot"></i>` : ""}<span class="wa-x" title="${esc(tr("Cerrar este chat"))}">${CROSS}</span></button>`)
      .join("");
  }

  private poof(el: HTMLElement) {
    for (let i = 0; i < 6; i++) {
      const dot = document.createElement("i");
      dot.className = "wa-spark roja";
      el.append(dot);
      const a = (i / 6) * Math.PI * 2 + Math.random() * 0.5;
      const r = 18 + Math.random() * 10;
      void this.anim(dot, [{ transform: "scale(.7)", opacity: 0.95 }, { transform: `translate(${Math.cos(a) * r}px, ${Math.sin(a) * r}px) scale(.2)`, opacity: 0 }], {
        duration: 420 + Math.random() * 160,
        easing: "cubic-bezier(.2,.7,.3,1)",
      }).then(() => dot.remove());
    }
  }

  private async closeChat(id: string) {
    const chat = this.chats.get(id);
    if (!chat || this.state !== "abierta") return;
    if (this.gustos.ui) this.oirUi("descartar");
    const current = id === this.current;
    const el = current ? this.root.querySelector<HTMLElement>(".wa-hface") : this.tabs.querySelector<HTMLElement>(`[data-chat="${CSS.escape(id)}"]`);
    if (el && !this.gustos.suave) {
      this.poof(el);
      const avatar = el.querySelector<HTMLElement>(".wa-av") ?? el;
      await this.anim(avatar, [{ transform: "none", opacity: 1 }, { transform: "scale(.2) rotate(-35deg)", opacity: 0 }], { duration: 230, easing: EASE_IN, fill: "forwards" });
    }
    this.chats.delete(id);
    this.paintCount(false);
    if (!current) {
      this.paintFaces();
      return;
    }
    this.stopNote();
    const next = [...this.chats.keys()].pop() ?? null;
    if (!next) {
      this.input.value = "";
      await this.close();
      this.current = null;
      void this.leave();
      return;
    }
    this.current = next;
    await this.switchTo();
    const head = this.root.querySelector<HTMLElement>(".wa-hface .wa-av");
    if (head && !this.gustos.suave) void this.anim(head, [{ transform: "scale(.4)", opacity: 0 }, { transform: "none", opacity: 1 }], POP);
  }

  private llega(llegada: Llegada) {
    if (this.state === "yendose") this.reset();
    const known = this.chats.get(llegada.chat);
    const chat: Chat = known ?? { chat: llegada.chat, nombre: llegada.nombre, avatar: llegada.avatar, hilo: [], nuevos: 0, prueba: !!llegada.prueba, borrador: "" };
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
    const last = [...chat.hilo].reverse().find((l) => !l.yo);
    const said = text ?? (last?.texto || (last?.media ? tr(MEDIA_NAME[last.media.tipo]) : ""));
    const pic = last?.media && (last.media.tipo === "sticker" || last.media.tipo === "foto") ? src(last.media) : "";
    const thumb = this.peek.querySelector<HTMLImageElement>(".wa-peek-pic")!;
    thumb.hidden = !pic;
    thumb.className = `wa-peek-pic ${last?.media?.tipo ?? ""}`;
    if (pic) {
      thumb.onload = () => this.peekShown && this.onShape();
      thumb.src = pic;
    } else thumb.removeAttribute("src");
    this.peek.querySelector(".wa-said")!.textContent = pic && !last?.texto ? "" : said;
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
          ...[...this.panel.querySelectorAll<HTMLElement>(".wa-head, .wa-scroll, .wa-quick, .wa-write")].map((el, i) =>
            this.anim(el, [{ opacity: 0, transform: "translateY(8px)" }, { opacity: 1, transform: "none" }], { duration: 320, delay: 110 + i * 45, easing: EASE_OUT, fill: "backwards" }),
          ),
        ]);
    this.openedAt = performance.now();
    this.reclaimed = false;
    this.input.value = chat.borrador;
    this.fit();
    this.onFocus(true);
    this.input.focus({ preventScroll: true });
    this.thread.scrollTop = this.thread.scrollHeight;
    await opened;
  }

  private async switchTo() {
    const chat = this.chats.get(this.current!)!;
    this.stopNote();
    this.input.value = chat.borrador;
    this.fit();
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
    this.stopNote();
    this.thread.innerHTML = chat.hilo
      .map((l) => {
        const kind = l.media ? ` m-${l.media.tipo}` : "";
        const media = l.media ? mediaHtml(l.media) : "";
        const text = l.texto ? `<p>${esc(l.texto)}</p>` : "";
        return `<div class="wa-line${l.yo ? ` yo ${l.estado ?? "listo"}` : ""}${kind}"${l.error ? ` title="${esc(l.error)}"` : ""}>${media}${text}<time>${clock(l.hora)}</time></div>`;
      })
      .join("");
    this.thread.scrollTop = this.thread.scrollHeight;
    this.measureRail();
    const last = this.thread.lastElementChild as HTMLElement | null;
    if (animateLast && last) void this.anim(last, [{ opacity: 0, transform: "translateY(14px) scale(.9)" }, { opacity: 1, transform: "none" }], POP);
  }

  private async send(raw: string, chip?: HTMLElement) {
    const text = raw.trim();
    const chat = this.current ? this.chats.get(this.current) : null;
    if (chat && this.state === "abierta" && this.adjuntos.length) return this.sendFiles(text, chat);
    if (!text || !chat || this.state !== "abierta") return;
    this.input.value = "";
    chat.borrador = "";
    this.fit();
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
      if (mine.estado === "enviando") this.tick(line, mine, "listo");
      this.pulse();
      if (chat.prueba) {
        const said = chat.hilo.filter((l) => l.yo).length;
        setTimeout(() => this.tick(line, mine, "entregado"), 650);
        setTimeout(() => {
          this.tick(line, mine, "leido");
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
    clearTimeout(this.blurCheck);
    this.keepDraft();
    this.stopNote();
    if (this.gustos.rapidas === "boton") this.showQuick(false, false);
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
      this.stopNote();
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
