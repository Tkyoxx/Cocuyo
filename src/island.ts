// la isla: su forma y como se mueve

import type { Place, Shape } from "./bridge";
import { Spring } from "./spring";

export interface Size {
  w: number;
  h: number;
  r: number;
}

export type Base = "idle" | "menu" | "guide" | "install";

const NOTCH: Size = { w: 168, h: 28, r: 14 };
const PEEK: Size = { w: 190, h: 32, r: 16 };
const VOICE: Size = { w: 216, h: 36, r: 18 };
const SLIVER: Size = { w: 8, h: 132, r: 6 };
const EAR = 12;
const MARGIN = 18;
const LEAVE = 320;
const LEAVE_AFTER_SHRINK = 4500;

export class Island {
  base: Base = "idle";
  place: Place = "top";
  onShape = () => {};
  onOpen = () => {};
  onLeave = () => {};

  private el = document.getElementById("island")!;
  private w = new Spring(NOTCH.w, (x) => (this.el.style.width = `${x}px`), {
    stiffness: 300,
    damping: 25,
    onRest: () => this.onShape(),
  });
  private h = new Spring(NOTCH.h, (x) => (this.el.style.height = `${x}px`), {
    stiffness: 300,
    damping: 27,
    onRest: () => this.onShape(),
  });
  private panel = NOTCH;
  private voice = false;
  private hover = false;
  private leaveTimer = 0;
  private moveTimer = 0;
  private shrunkUntil = 0;

  constructor() {
    this.el.addEventListener("pointerenter", () => {
      clearTimeout(this.leaveTimer);
      this.hover = true;
      this.apply();
    });
    this.el.addEventListener("pointerleave", () => {
      const shrunk = performance.now() < this.shrunkUntil;
      this.leaveTimer = window.setTimeout(
        () => {
          this.hover = false;
          this.apply();
          this.onLeave();
        },
        shrunk ? LEAVE_AFTER_SHRINK : LEAVE,
      );
    });
    this.el.addEventListener("click", () => {
      if (this.base === "idle") this.onOpen();
    });
    this.el.addEventListener(
      "wheel",
      (e) => {
        if (e.deltaY > 0 && this.base === "idle") this.onOpen();
      },
      { passive: true },
    );
  }

  setBase(base: Base, panel?: Size) {
    if (panel) {
      this.noteShrink(panel);
      this.panel = panel;
    }
    this.base = base;
    this.apply();
  }

  setPanel(panel: Size) {
    this.noteShrink(panel);
    this.panel = panel;
    this.apply();
  }

  private noteShrink(next: Size) {
    if (this.base !== "idle" && (next.w < this.panel.w || next.h < this.panel.h)) {
      this.shrunkUntil = performance.now() + 900;
    }
  }

  setVoice(on: boolean) {
    this.voice = on;
    this.apply();
  }

  setPlace(place: Place, instant = false) {
    if (place === this.place) return;
    clearTimeout(this.moveTimer);
    this.el.classList.add("away");
    this.moveTimer = window.setTimeout(
      () => {
        this.place = place;
        if (place !== "hidden") {
          this.el.dataset.dock = place;
          const size = this.size();
          this.w.jump(size.w);
          this.h.jump(size.h);
          void this.el.offsetWidth;
          this.el.classList.remove("away");
        }
        this.apply();
      },
      instant ? 0 : 200,
    );
  }

  shape(): Shape | null {
    if (this.place === "hidden") return null;
    const w = Math.max(this.w.x, this.w.target);
    const h = Math.max(this.h.x, this.h.target);
    if (this.place === "right") {
      const tall = h + (EAR + MARGIN) * 2;
      return { x: innerWidth - w - MARGIN, y: innerHeight / 2 - tall / 2, w: w + MARGIN, h: tall, r: 0 };
    }
    const wide = w + (EAR + MARGIN) * 2;
    return { x: innerWidth / 2 - wide / 2, y: 0, w: wide, h: h + MARGIN, r: 0 };
  }

  private face() {
    if (this.voice && this.base !== "guide" && this.base !== "install") return "voice";
    if (this.base !== "idle") return this.base;
    return this.hover ? "peek" : "idle";
  }

  private size(): Size {
    const face = this.face();
    if (face === "menu" || face === "guide" || face === "install") {
      return { ...this.panel, w: Math.min(this.panel.w, innerWidth - (EAR + MARGIN) * 2) };
    }
    if (this.place !== "right") return face === "voice" ? VOICE : face === "peek" ? PEEK : NOTCH;
    if (face === "idle") return SLIVER;
    const size = face === "voice" ? VOICE : NOTCH;
    return { w: size.h, h: size.w, r: size.r };
  }

  private apply() {
    const size = this.size();
    this.el.dataset.face = this.face();
    this.el.style.setProperty("--r", `${size.r}px`);
    this.w.to(size.w);
    this.h.to(size.h);
    this.onShape();
  }
}
