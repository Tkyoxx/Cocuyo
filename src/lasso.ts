// el lazo para rodear cosas

import { run } from "./spring";

export interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

const MIN_STEP = 2.5;
const MIN_POINTS = 8;
const MIN_SIDE = 24;
const BLEED = 10;

const GLOW = "92 184 255";
const CORE = "226 242 255";

export class Lasso {
  active = false;

  private canvas = document.getElementById("trail") as HTMLCanvasElement;
  private ctx = this.canvas.getContext("2d")!;
  private pts: number[] = [];
  private box = { x0: 0, y0: 0, x1: 0, y1: 0 };
  private painted: Box | null = null;
  private dirty = false;
  private fade = 1;

  constructor() {
    window.addEventListener("pointermove", (e) => {
      if (this.active) this.add(e.clientX, e.clientY);
    });
  }

  start(cursor: [number, number] | null) {
    const dpr = devicePixelRatio;
    this.canvas.width = Math.round(innerWidth * dpr);
    this.canvas.height = Math.round(innerHeight * dpr);
    this.ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    this.ctx.lineCap = this.ctx.lineJoin = "round";

    this.active = true;
    this.pts = [];
    this.painted = null;
    this.fade = 1;
    if (cursor) this.add(cursor[0], cursor[1]);

    run(() => {
      if (this.dirty) this.paint(false);
      return this.active;
    });
  }

  end(cancelled: boolean): Promise<Box | null> {
    this.active = false;
    const box = cancelled ? null : this.bounds();
    const duration = box ? 0.5 : 0.18;

    return new Promise((resolve) => {
      run((dt) => {
        this.fade -= dt / duration;
        if (this.fade > 0) {
          this.paint(box !== null);
          return true;
        }
        this.canvas.width = this.canvas.height = 0;
        resolve(box);
        return false;
      });
    });
  }

  private add(x: number, y: number) {
    const n = this.pts.length;
    if (n === 0) {
      this.box = { x0: x, y0: y, x1: x, y1: y };
    } else if (Math.hypot(x - this.pts[n - 2], y - this.pts[n - 1]) < MIN_STEP) {
      return;
    }
    this.pts.push(x, y);
    const b = this.box;
    b.x0 = Math.min(b.x0, x);
    b.y0 = Math.min(b.y0, y);
    b.x1 = Math.max(b.x1, x);
    b.y1 = Math.max(b.y1, y);
    this.dirty = true;
  }

  private bounds(): Box | null {
    const { x0, y0, x1, y1 } = this.box;
    if (this.pts.length < MIN_POINTS * 2 || x1 - x0 < MIN_SIDE || y1 - y0 < MIN_SIDE) return null;
    return { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
  }

  private paint(closed: boolean) {
    this.dirty = false;
    const { ctx, pts } = this;
    if (this.painted) {
      const p = this.painted;
      ctx.clearRect(p.x, p.y, p.w, p.h);
    }
    if (pts.length < 4 || this.fade <= 0) return;

    const path = new Path2D();
    path.moveTo(pts[0], pts[1]);
    for (let i = 2; i < pts.length - 2; i += 2) {
      path.quadraticCurveTo(
        pts[i],
        pts[i + 1],
        (pts[i] + pts[i + 2]) / 2,
        (pts[i + 1] + pts[i + 3]) / 2,
      );
    }
    path.lineTo(pts[pts.length - 2], pts[pts.length - 1]);
    if (closed) path.closePath();

    const a = this.fade;
    const bloom = closed ? 1 + (1 - a) * 0.9 : 1;
    ctx.strokeStyle = `rgb(${GLOW} / ${0.14 * a})`;
    ctx.lineWidth = 12 * bloom;
    ctx.stroke(path);
    ctx.strokeStyle = `rgb(${GLOW} / ${0.5 * a})`;
    ctx.lineWidth = 4.5 * bloom;
    ctx.stroke(path);
    ctx.strokeStyle = `rgb(${CORE} / ${0.95 * a})`;
    ctx.lineWidth = 1.8;
    ctx.stroke(path);

    const pad = BLEED * bloom + 2;
    const b = this.box;
    this.painted = { x: b.x0 - pad, y: b.y0 - pad, w: b.x1 - b.x0 + pad * 2, h: b.y1 - b.y0 + pad * 2 };
  }
}
