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

export class Spring {
  x: number;
  target: number;
  private v = 0;
  private k: number;
  private c: number;
  private onRest?: () => void;

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
    run(this.tick);
  }

  jump(x: number) {
    this.x = this.target = x;
    this.v = 0;
    this.apply(x);
  }

  private tick = (dt: number) => {
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
