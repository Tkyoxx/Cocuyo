// anotar una captura: flechas, recuadros, lapiz y difuminar, sin salir de la isla

import { call } from "./bridge";
import type { Size } from "./island";

type Tool = "flecha" | "caja" | "lapiz" | "borroso";
interface Stroke {
  tool: Tool;
  color: string;
  points: number[];
}

const TOOLS: [Tool, string][] = [
  ["flecha", "Flecha"],
  ["caja", "Recuadro"],
  ["lapiz", "Lápiz"],
  ["borroso", "Difuminar"],
];
const COLORS = ["#ff453a", "#ffd60a", "#5cb8ff"];
const BAR = 46;

export class Anotador {
  open = false;
  onOpen: (size: Size) => void = () => {};
  onClose = () => {};
  onSay: (text: string) => void = () => {};

  private el = document.createElement("section");
  private canvas = document.createElement("canvas");
  private ctx = this.canvas.getContext("2d")!;
  private image = new Image();
  private path = "";
  private tool: Tool = "flecha";
  private color = COLORS[0];
  private strokes: Stroke[] = [];
  private drawing: Stroke | null = null;

  constructor() {
    this.el.id = "anota";
    this.el.innerHTML = `
      <div class="an-bar">
        ${TOOLS.map(([id, label]) => `<button class="chip${id === this.tool ? " on" : ""}" data-tool="${id}">${label}</button>`).join("")}
        <span class="an-colors">${COLORS.map((c, i) => `<button class="swatch${i ? "" : " on"}" data-color="${c}" style="--c:${c}" aria-label="Color"></button>`).join("")}</span>
        <span class="an-gap"></span>
        <button class="ghost" data-act="deshacer">Deshacer</button>
        <button class="ghost" data-act="copiar">Copiar</button>
        <button class="ghost pick" data-act="guardar">Guardar</button>
        <button class="ghost an-x" data-act="cerrar" aria-label="Cerrar">×</button>
      </div>`;
    this.el.append(this.canvas);
    document.querySelector("#island .shell")!.append(this.el);
    this.el.addEventListener("click", (e) => {
      e.stopPropagation();
      const target = (e.target as HTMLElement).closest<HTMLElement>("button");
      if (!target) return;
      if (target.dataset.tool) {
        this.tool = target.dataset.tool as Tool;
        this.el.querySelectorAll("[data-tool]").forEach((b) => b.classList.toggle("on", b === target));
      } else if (target.dataset.color) {
        this.color = target.dataset.color;
        this.el.querySelectorAll("[data-color]").forEach((b) => b.classList.toggle("on", b === target));
      } else if (target.dataset.act === "deshacer") {
        this.strokes.pop();
        this.paint();
      } else if (target.dataset.act === "copiar") void this.save(true, false);
      else if (target.dataset.act === "guardar") void this.save(false, true);
      else if (target.dataset.act === "cerrar") this.close();
    });
    this.canvas.addEventListener("pointerdown", (e) => {
      this.canvas.setPointerCapture(e.pointerId);
      const [x, y] = this.at(e);
      this.drawing = { tool: this.tool, color: this.color, points: [x, y, x, y] };
      this.strokes.push(this.drawing);
      this.paint();
    });
    this.canvas.addEventListener("pointermove", (e) => {
      if (!this.drawing) return;
      const [x, y] = this.at(e);
      if (this.drawing.tool === "lapiz") this.drawing.points.push(x, y);
      else this.drawing.points.splice(2, 2, x, y);
      this.paint();
    });
    const finish = () => {
      if (!this.drawing) return;
      const p = this.drawing.points;
      if (this.drawing.tool !== "lapiz" && Math.hypot(p[2] - p[0], p[3] - p[1]) < 4) this.strokes.pop();
      this.drawing = null;
      this.paint();
    };
    this.canvas.addEventListener("pointerup", finish);
    this.canvas.addEventListener("pointercancel", finish);
  }

  async start(path: string) {
    const data = await call<string>("anotar_abrir", { path });
    this.path = path;
    this.strokes = [];
    await new Promise<void>((done, fail) => {
      this.image.onload = () => done();
      this.image.onerror = () => fail(new Error("No pude abrir la captura."));
      this.image.src = data;
    });
    const { naturalWidth: w, naturalHeight: h } = this.image;
    this.canvas.width = w;
    this.canvas.height = h;
    const maxW = Math.min(innerWidth - 112, 1000);
    const maxH = Math.min(innerHeight * 0.7, 560);
    const scale = Math.min(1, maxW / w, maxH / h);
    const shownW = Math.round(w * scale);
    const shownH = Math.round(h * scale);
    this.canvas.style.width = `${shownW}px`;
    this.canvas.style.height = `${shownH}px`;
    this.paint();
    this.open = true;
    this.onOpen({ w: Math.max(shownW + 32, 560), h: shownH + BAR + 40, r: 30 });
  }

  close() {
    if (!this.open) return;
    this.open = false;
    this.onClose();
  }

  private at(e: PointerEvent): [number, number] {
    const box = this.canvas.getBoundingClientRect();
    return [((e.clientX - box.left) / box.width) * this.canvas.width, ((e.clientY - box.top) / box.height) * this.canvas.height];
  }

  private paint() {
    const ctx = this.ctx;
    const scale = this.canvas.width / (this.canvas.getBoundingClientRect().width || this.canvas.width);
    ctx.drawImage(this.image, 0, 0);
    for (const s of this.strokes) {
      const [x1, y1, x2, y2] = s.points;
      ctx.strokeStyle = ctx.fillStyle = s.color;
      ctx.lineWidth = 4 * scale;
      ctx.lineCap = ctx.lineJoin = "round";
      if (s.tool === "caja") {
        ctx.strokeRect(Math.min(x1, x2), Math.min(y1, y2), Math.abs(x2 - x1), Math.abs(y2 - y1));
      } else if (s.tool === "flecha") {
        const angle = Math.atan2(y2 - y1, x2 - x1);
        const head = 16 * scale;
        ctx.beginPath();
        ctx.moveTo(x1, y1);
        ctx.lineTo(x2 - Math.cos(angle) * head * 0.6, y2 - Math.sin(angle) * head * 0.6);
        ctx.stroke();
        ctx.beginPath();
        ctx.moveTo(x2, y2);
        ctx.lineTo(x2 - head * Math.cos(angle - 0.45), y2 - head * Math.sin(angle - 0.45));
        ctx.lineTo(x2 - head * Math.cos(angle + 0.45), y2 - head * Math.sin(angle + 0.45));
        ctx.closePath();
        ctx.fill();
      } else if (s.tool === "lapiz") {
        ctx.beginPath();
        ctx.moveTo(s.points[0], s.points[1]);
        for (let i = 2; i < s.points.length; i += 2) ctx.lineTo(s.points[i], s.points[i + 1]);
        ctx.stroke();
      } else this.blur(x1, y1, x2, y2, scale);
    }
  }

  private blur(x1: number, y1: number, x2: number, y2: number, scale: number) {
    const [x, y, w, h] = [Math.min(x1, x2), Math.min(y1, y2), Math.abs(x2 - x1), Math.abs(y2 - y1)].map(Math.round);
    if (w < 2 || h < 2) return;
    const block = Math.max(6, Math.round(10 * scale));
    const small = document.createElement("canvas");
    small.width = Math.max(1, Math.ceil(w / block));
    small.height = Math.max(1, Math.ceil(h / block));
    const sctx = small.getContext("2d")!;
    sctx.drawImage(this.canvas, x, y, w, h, 0, 0, small.width, small.height);
    this.ctx.imageSmoothingEnabled = false;
    this.ctx.drawImage(small, 0, 0, small.width, small.height, x, y, w, h);
    this.ctx.imageSmoothingEnabled = true;
  }

  private async save(copy: boolean, keep: boolean) {
    const data = this.canvas.toDataURL("image/png").split(",")[1] ?? "";
    try {
      const said = await call<string>("anotar_guardar", { path: this.path, png: data, copiar: copy, guardar: keep });
      this.onSay(said);
      if (keep) this.close();
    } catch (error) {
      this.onSay(String(error));
    }
  }
}
