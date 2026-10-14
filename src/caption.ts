import type { Shape } from "./bridge";
import { tr } from "./i18n";

const MARGIN = 26;

export class Caption {
  onShape = () => {};

  private el = document.getElementById("caption")!;
  private said = this.el.querySelector<HTMLElement>(".said")!;
  private who = document.getElementById("caption-who")!;
  private words = document.getElementById("caption-words")!;
  private visible = false;
  private timer = 0;

  say(who: "you" | "app", text: string, ms = 3400) {
    clearTimeout(this.timer);
    this.el.dataset.who = who;
    this.who.textContent = who === "you" ? tr("Tú") : "Cocuyo";
    text = tr(text);

    const spans = text.split(/(?<=\s)/).map((word, i) => {
      const span = document.createElement("span");
      span.className = "word";
      span.textContent = word;
      span.style.animationDelay = `${i * 34}ms`;
      return span;
    });
    this.words.replaceChildren(...spans);

    this.visible = true;
    this.el.classList.add("on");
    this.onShape();
    this.timer = window.setTimeout(() => this.hide(), ms);
  }

  hide() {
    clearTimeout(this.timer);
    this.el.classList.remove("on");
    this.timer = window.setTimeout(() => {
      this.visible = false;
      this.onShape();
    }, 240);
  }

  shape(): Shape | null {
    if (!this.visible) return null;
    const r = this.said.getBoundingClientRect();
    return {
      x: r.left - MARGIN,
      y: r.top - MARGIN,
      w: r.width + MARGIN * 2,
      h: r.height + MARGIN * 2,
      r: 20 + MARGIN,
    };
  }
}

const PILL = { w: 76, h: 34, bottom: 34 };

export class Pill {
  onShape = () => {};

  private el = document.getElementById("pill")!;
  private visible = false;
  private timer = 0;

  show() {
    clearTimeout(this.timer);
    this.el.classList.remove("done", "thinking");
    this.el.classList.add("on");
    this.visible = true;
    this.onShape();
  }

  think() {
    this.el.classList.add("thinking");
    this.timer = window.setTimeout(() => this.leave(), 25000);
  }

  done() {
    clearTimeout(this.timer);
    this.el.classList.remove("thinking");
    this.el.classList.add("done");
    this.timer = window.setTimeout(() => this.leave(), 950);
  }

  cancel() {
    this.leave();
  }

  shape(): Shape | null {
    if (!this.visible) return null;
    return {
      x: innerWidth / 2 - PILL.w / 2 - MARGIN,
      y: innerHeight - PILL.bottom - PILL.h - MARGIN,
      w: PILL.w + MARGIN * 2,
      h: PILL.h + MARGIN * 2,
      r: PILL.h / 2 + MARGIN,
    };
  }

  private leave() {
    clearTimeout(this.timer);
    this.el.classList.remove("on");
    this.timer = window.setTimeout(() => {
      this.el.classList.remove("done", "thinking");
      this.visible = false;
      this.onShape();
    }, 260);
  }
}
