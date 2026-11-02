// resortes hechos a mano, sin librerias

type Tick = (dt: number) => boolean;

const running = new Set<Tick>();
let frame = 0;
let last = 0;

function loop(now: number) {
  const dt = Math.min((now - last) / 1000, 1 / 30);
  last = now;
  for (const tick of running) {
    if (!tick(dt)) running.delete(tick);
  }
  frame = running.size ? requestAnimationFrame(loop) : 0;
}

export function run(tick: Tick) {
  running.add(tick);
  if (!frame) {
    last = performance.now();
    frame = requestAnimationFrame(loop);
  }
}

export interface SpringOptions {
  stiffness?: number;
  damping?: number;
  onRest?: () => void;
}

const STEP = 1 / 120;

function bezier(x1: number, y1: number, x2: number, y2: number) {
  const at = (t: number, a: number, b: number) => 3 * (1 - t) ** 2 * t * a + 3 * (1 - t) * t * t * b + t ** 3;
  return (x: number) => {
    let [lo, hi, t] = [0, 1, x];
    for (let i = 0; i < 14; i++) {
      if (at(t, x1, x2) < x) lo = t;
      else hi = t;
      t = (lo + hi) / 2;
    }
    return at(t, y1, y2);
  };
}

export const closing = bezier(0.45, 0, 0.2, 1);

export class Spring {
  x: number;
  target: number;
  private v = 0;
  private k: number;
  private c: number;
  private onRest?: () => void;
  private curve: { from: number; start: number; ms: number } | null = null;

  constructor(
    x: number,
    private apply: (x: number) => void,
    { stiffness = 320, damping = 26, onRest }: SpringOptions = {},
  ) {
    this.x = this.target = x;
    this.k = stiffness;
    this.c = damping;
    this.onRest = onRest;
  }

  to(target: number) {
    this.target = target;
    this.curve = null;
    run(this.tick);
  }

  glide(target: number, ms = 340) {
    if (target === this.target && !this.curve) return;
    this.target = target;
    this.v = 0;
    this.curve = { from: this.x, start: performance.now(), ms };
    run(this.tick);
  }

  jump(x: number) {
    this.curve = null;
    this.x = this.target = x;
    this.v = 0;
    this.apply(x);
  }

  private tick = (dt: number) => {
    if (this.curve) {
      const p = Math.min(1, (performance.now() - this.curve.start) / this.curve.ms);
      this.x = this.curve.from + (this.target - this.curve.from) * closing(p);
      this.apply(this.x);
      if (p < 1) return true;
      this.curve = null;
      this.x = this.target;
      this.onRest?.();
      return false;
    }
    for (let t = dt; t > 0; t -= STEP) {
      const h = Math.min(t, STEP);
      this.v += (-this.k * (this.x - this.target) - this.c * this.v) * h;
      this.x += this.v * h;
    }
    if (Math.abs(this.v) < 0.05 && Math.abs(this.x - this.target) < 0.05) {
      this.x = this.target;
      this.v = 0;
      this.apply(this.x);
      this.onRest?.();
      return false;
    }
    this.apply(this.x);
    return true;
  };
}
