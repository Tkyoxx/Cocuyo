import type { Box } from "./lasso";

const MIN_SIDE = 6;

export class Selector {
  active = false;
  onDone: (box: Box | null) => void = () => {};

  private el = document.getElementById("select")!;
  private box = document.getElementById("select-box")!;
  private size = document.getElementById("select-size")!;
  private origin: [number, number] | null = null;

  constructor() {
    this.el.addEventListener("pointerdown", (e) => {
      if (e.button !== 0) return this.finish(null);
      this.origin = [e.clientX, e.clientY];
      this.el.setPointerCapture(e.pointerId);
      this.el.classList.add("dragging");
    });
    this.el.addEventListener("pointermove", (e) => {
      if (this.origin) this.draw(this.rect(e));
    });
    this.el.addEventListener("pointerup", (e) => {
      if (!this.origin) return;
      const box = this.rect(e);
      this.finish(box.w >= MIN_SIDE && box.h >= MIN_SIDE ? box : null);
    });
    this.el.addEventListener("contextmenu", (e) => e.preventDefault());
  }

  start() {
    this.active = true;
    this.origin = null;
    this.box.style.display = "none";
    this.el.classList.remove("dragging");
    this.el.classList.add("on");
  }

  cancel() {
    if (this.active) this.finish(null);
  }

  private rect(e: PointerEvent): Box {
    const [ox, oy] = this.origin!;
    return {
      x: Math.min(ox, e.clientX),
      y: Math.min(oy, e.clientY),
      w: Math.abs(e.clientX - ox),
      h: Math.abs(e.clientY - oy),
    };
  }

  private draw(box: Box) {
    const s = this.box.style;
    s.display = "block";
    s.left = `${box.x}px`;
    s.top = `${box.y}px`;
    s.width = `${box.w}px`;
    s.height = `${box.h}px`;
    const dpr = devicePixelRatio;
    this.size.textContent = `${Math.round(box.w * dpr)} × ${Math.round(box.h * dpr)}`;
  }

  private finish(box: Box | null) {
    this.active = false;
    this.origin = null;
    this.el.classList.remove("on", "dragging");
    this.onDone(box);
  }
}
