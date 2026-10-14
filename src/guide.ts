// la guia de la primera vez

import { setKeyEcho, type Chord, type ChordEvent } from "./bridge";
import type { Size } from "./island";
import { keysOf } from "./keys";

interface Step {
  title: string;
  body: string;
  waiting: string;
  holding: string;
  chord?: Chord;
}

const STEPS: Step[] = [
  {
    title: "Dicta en cualquier parte",
    body: "Mantén estas teclas, habla y suelta. Lo que digas se escribe donde tengas el cursor.",
    waiting: "Pruébalo: mantenlas un momento.",
    holding: "Eso es. Suelta cuando quieras.",
    chord: "dictation",
  },
  {
    title: "Rodea lo que ves",
    body: "Mantén estas teclas y mueve el mouse alrededor de cualquier cosa en pantalla. Se guarda como captura y queda copiada.",
    waiting: "Pruébalo: mantenlas y dibuja un círculo.",
    holding: "Rodea algo y suelta.",
    chord: "lasso",
  },
  {
    title: "Todo lo demás está en la isla",
    body: "Haz clic en ella, o gira la rueda hacia abajo encima, para abrir el menú. Empieza por la pestaña Ajustes: ahí descargas la voz local para dictar y dar órdenes, y cambias los atajos a tu gusto.",
    waiting: "",
    holding: "",
  },
];

const SEEN_KEY = "cocuyo.guide";
const WIDTH = 380;

export class Guide {
  open = false;
  onOpen: (size: Size) => void = () => {};
  onResize: (size: Size) => void = () => {};
  onClose = () => {};

  private el = document.getElementById("guide")!;
  private stepEl = document.getElementById("guide-step")!;
  private title = document.getElementById("guide-title")!;
  private keysEl = document.getElementById("guide-keys")!;
  private body = document.getElementById("guide-body")!;
  private live = document.getElementById("guide-live")!;
  private skip = document.getElementById("guide-skip") as HTMLButtonElement;
  private done = document.getElementById("guide-done") as HTMLButtonElement;
  private lamps = [...this.el.querySelectorAll<HTMLElement>(".lamps i")];
  private keyEls = new Map<number, HTMLElement>();
  private chords: Record<Chord, number> = { dictation: 9, lasso: 3, command: 12 };
  private index = 0;

  constructor() {
    for (const el of [this.title, this.keysEl, this.body, this.live]) el.classList.add("swap");
    this.skip.addEventListener("click", () => this.close());
    this.done.addEventListener("click", () => this.close());
  }

  get seen() {
    return localStorage.getItem(SEEN_KEY) === "done";
  }

  start() {
    this.index = 0;
    this.render();
    this.open = true;
    setKeyEcho(true);
    this.onOpen(this.size());
  }

  close() {
    if (!this.open) return;
    this.open = false;
    localStorage.setItem(SEEN_KEY, "done");
    setKeyEcho(false);
    this.onClose();
  }

  setChords(dictation: number, lasso: number, command: number) {
    this.chords = { dictation, lasso, command };
    if (this.open) this.render();
  }

  keys(mask: number) {
    for (const [bit, el] of this.keyEls) el.classList.toggle("down", (mask & bit) !== 0);
  }

  chord(e: ChordEvent, valid: boolean) {
    const step = STEPS[this.index];
    if (!this.open || e.chord !== step.chord) return;
    if (e.phase === "start") {
      this.say(step.holding, true);
    } else if (valid) {
      this.lamps[this.index].classList.add("lit");
      this.say("Hecho.", true);
      setTimeout(() => this.next(), 750);
    } else {
      this.say(step.waiting, false);
    }
  }

  private size(): Size {
    return { w: WIDTH, h: this.el.offsetHeight, r: 30 };
  }

  private next() {
    if (!this.open) return;
    this.index += 1;
    this.el.classList.add("swapping");
    setTimeout(() => {
      this.render();
      this.el.classList.remove("swapping");
      this.onResize(this.size());
    }, 170);
  }

  private render() {
    const step = STEPS[this.index];
    const last = this.index === STEPS.length - 1;

    this.stepEl.textContent = `Guía rápida · ${this.index + 1} de ${STEPS.length}`;
    this.title.textContent = step.title;
    this.body.textContent = step.body;
    this.say(step.waiting, false);
    this.lamps.forEach((lamp, i) => lamp.classList.toggle("lit", i < this.index || last));

    this.keyEls.clear();
    this.keysEl.replaceChildren();
    const keys = step.chord ? keysOf(this.chords[step.chord]) : [];
    keys.forEach(([bit, label], i) => {
      if (i > 0) {
        const plus = document.createElement("span");
        plus.className = "plus";
        plus.textContent = "+";
        this.keysEl.append(plus);
      }
      const key = document.createElement("span");
      key.className = "key";
      key.textContent = label;
      this.keysEl.append(key);
      this.keyEls.set(bit, key);
    });

    this.skip.hidden = last;
    this.done.hidden = !last;
  }

  private say(text: string, good: boolean) {
    this.live.textContent = text;
    this.live.classList.toggle("good", good);
  }
}
