// la isla: su forma y como se mueve

import type { Place, Shape } from "./bridge";
import { face, leaveDelay, NOTCH, region, shrinks, size, type Base, type Look, type Size } from "./forma";
import { Spring } from "./spring";

export type { Base, Size } from "./forma";
export type State = "oye" | "piensa" | "espera" | "mal" | "bien" | "nueva";

export class Island {
  base: Base = "idle";
  place: Place = "top";
  onShape = () => {};
  onOpen = () => {};
  onLeave = () => {};
  onPeek = () => {};

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
  private stateTimer = 0;

  constructor() {
    this.el.addEventListener("pointerenter", () => {
      clearTimeout(this.leaveTimer);
      const was = this.hover;
      this.hover = true;
      this.apply();
      if (!was && this.base === "idle") this.onPeek();
    });
    this.el.addEventListener("pointerleave", () => {
      this.leaveTimer = window.setTimeout(
        () => {
          this.hover = false;
          this.apply();
          this.onLeave();
        },
        leaveDelay(performance.now(), this.shrunkUntil),
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
    if (shrinks(this.base, this.panel, next)) this.shrunkUntil = performance.now() + 900;
  }

  greet() {
    if (this.base !== "idle" || this.hover) return;
    this.el.classList.add("hola");
    this.hover = true;
    this.apply();
    window.setTimeout(() => {
      this.el.classList.remove("hola");
      if (!this.el.matches(":hover")) {
        this.hover = false;
        this.apply();
      }
    }, 1600);
  }

  dropping(on: boolean) {
    this.el.classList.toggle("suelta", on);
    if (on === this.hover || this.base !== "idle") return;
    this.hover = on;
    this.apply();
  }

  gulp() {
    this.el.classList.remove("traga");
    void this.el.offsetWidth;
    this.el.classList.add("traga");
    window.setTimeout(() => this.el.classList.remove("traga"), 600);
  }

  vitals(text: string, hot: boolean) {
    const el = document.getElementById("vitals")!;
    el.textContent = text;
    el.classList.toggle("on", !!text);
    el.classList.toggle("hot", hot);
  }

  setState(state: State | null, ms = 0) {
    clearTimeout(this.stateTimer);
    if (state) this.el.dataset.st = state;
    else delete this.el.dataset.st;
    if (state && ms) this.stateTimer = window.setTimeout(() => this.setState(null), ms);
  }

  get state(): State | null {
    return (this.el.dataset.st as State | undefined) ?? null;
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
        document.documentElement.classList.toggle("escondida", place === "hidden");
        if (place !== "hidden") {
          this.el.dataset.dock = place;
          const next = this.size();
          this.w.jump(next.w);
          this.h.jump(next.h);
          void this.el.offsetWidth;
          this.el.classList.remove("away");
        }
        this.apply();
      },
      instant ? 0 : 200,
    );
  }

  shape(): Shape | null {
    return region(this.place, Math.max(this.w.x, this.w.target), Math.max(this.h.x, this.h.target), innerWidth, innerHeight);
  }

  private look(): Look {
    return { base: this.base, place: this.place, voice: this.voice, hover: this.hover, panel: this.panel };
  }

  private size(): Size {
    return size(this.look(), innerWidth);
  }

  private apply() {
    const next = this.size();
    this.el.dataset.face = face(this.look());
    this.el.style.setProperty("--r", `${next.r}px`);
    const shrinking = next.w < this.w.target || next.h < this.h.target;
    if (shrinking) {
      this.w.glide(next.w);
      this.h.glide(next.h);
    } else {
      this.w.to(next.w);
      this.h.to(next.h);
    }
    this.onShape();
  }
}
