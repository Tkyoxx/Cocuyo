// la guia de la primera vez

import { call, getSettings, setKeyEcho, setSettings, type Chord, type ChordEvent, type VoiceStatus } from "./bridge";
import type { Size } from "./island";
import { keysOf } from "./keys";

type Kind = "chord" | "brain" | "end";

interface Step {
  kind: Kind;
  title: string;
  body: string;
  waiting: string;
  holding: string;
  chord?: Chord;
}

const STEPS: Step[] = [
  {
    kind: "chord",
    title: "Dicta en cualquier parte",
    body: "Mantén estas teclas, habla y suelta. Lo que digas se escribe donde tengas el cursor.",
    waiting: "Pruébalo: mantenlas un momento.",
    holding: "Eso es. Suelta cuando quieras.",
    chord: "dictation",
  },
  {
    kind: "chord",
    title: "Rodea lo que ves",
    body: "Mantén estas teclas y mueve el mouse alrededor de cualquier cosa en pantalla. Se guarda como captura y queda copiada.",
    waiting: "Pruébalo: mantenlas y dibuja un círculo.",
    holding: "Rodea algo y suelta.",
    chord: "lasso",
  },
  {
    kind: "chord",
    title: "Dale una orden",
    body: "Mantén estas teclas y di lo que quieres, como «abre la calculadora» o «sube el volumen». Ahora es de práctica: no se hace nada.",
    waiting: "Pruébalo: mantenlas y di una orden.",
    holding: "Di la orden y suelta.",
    chord: "command",
  },
  {
    kind: "brain",
    title: "¿Cómo piensa Cocuyo?",
    body: "Sin IA entiende una lista fija de órdenes. Con IA entiende cualquier frase, responde preguntas y mira lo que rodeas.",
    waiting: "",
    holding: "",
  },
  {
    kind: "end",
    title: "Lo último: la voz local",
    body: "",
    waiting: "Todo lo demás está en la isla: haz clic en ella, o gira la rueda hacia abajo encima, para abrir el menú.",
    holding: "",
  },
];

type Brain = "local" | "gemini" | "later";

const BRAIN_NOTE: Record<Brain, string> = {
  local: "Tu voz y tus capturas no salen de tu PC: todo se entiende aquí mismo.",
  gemini:
    "Lo que digas y las capturas que le pidas mirar van a Google. En el plan gratis de Gemini, Google puede usarlos para mejorar sus productos. No le cuentes lo que no le contarías a un desconocido.",
  later: "Queda sin IA, así que tu voz y tus capturas no salen de tu PC. Lo eliges cuando quieras en Ajustes → IA.",
};

const GEMINI = {
  aiFormat: "openai",
  aiUrl: "https://generativelanguage.googleapis.com/v1beta/openai",
  aiModel: "gemini-3.5-flash-lite, gemini-3.1-flash-lite",
} as const;

const SEEN_KEY = "cocuyo.guide";
const WIDTH = 380;

export class Guide {
  open = false;
  onOpen: (size: Size) => void = () => {};
  onResize: (size: Size) => void = () => {};
  onClose = () => {};
  onVoice = () => {};

  private el = document.getElementById("guide")!;
  private stepEl = document.getElementById("guide-step")!;
  private title = document.getElementById("guide-title")!;
  private keysEl = document.getElementById("guide-keys")!;
  private body = document.getElementById("guide-body")!;
  private brainEl = document.getElementById("guide-brain")!;
  private keyRow = document.getElementById("guide-keyrow")!;
  private keyInput = document.getElementById("guide-key") as HTMLInputElement;
  private getKey = document.getElementById("guide-getkey") as HTMLButtonElement;
  private live = document.getElementById("guide-live")!;
  private skip = document.getElementById("guide-skip") as HTMLButtonElement;
  private voiceGo = document.getElementById("guide-voice") as HTMLButtonElement;
  private done = document.getElementById("guide-done") as HTMLButtonElement;
  private lampsEl = this.el.querySelector<HTMLElement>(".lamps")!;
  private lamps: HTMLElement[] = [];
  private keyEls = new Map<number, HTMLElement>();
  private chords: Record<Chord, number> = { dictation: 9, lasso: 3, command: 12 };
  private index = 0;
  private brain: Brain | null = null;
  private typing = false;
  private voice: VoiceStatus | null = null;

  constructor() {
    this.lamps = STEPS.map(() => document.createElement("i"));
    this.lampsEl.replaceChildren(...this.lamps);
    for (const el of [this.title, this.keysEl, this.body, this.brainEl, this.keyRow, this.live]) el.classList.add("swap");
    this.skip.addEventListener("click", () => this.close());
    this.done.addEventListener("click", () => (this.index === STEPS.length - 1 ? this.close() : this.next()));
    this.voiceGo.addEventListener("click", () => {
      this.close();
      this.onVoice();
    });
    for (const button of this.brainEl.querySelectorAll<HTMLButtonElement>("button")) {
      button.addEventListener("click", () => void this.choose(button.dataset.brain as Brain));
    }
    this.keyInput.addEventListener("pointerdown", () => {
      if (this.typing) return;
      this.typing = true;
      void call("set_focusable", { on: true }).then(() => this.keyInput.focus());
    });
    this.keyInput.addEventListener("change", () => void this.saveKey());
    this.keyInput.addEventListener("keydown", (e) => {
      if (e.key === "Enter") void this.saveKey();
    });
    this.getKey.addEventListener("click", () => void call("ai_key_page").catch((error) => this.say(String(error), false)));
  }

  get seen() {
    return localStorage.getItem(SEEN_KEY) === "done";
  }

  start() {
    this.index = 0;
    this.brain = null;
    this.render();
    this.open = true;
    setKeyEcho(true);
    this.onOpen(this.size());
    void call<VoiceStatus | null>("voice_status", undefined, null).then((status) => (this.voice = status));
  }

  close() {
    if (!this.open) return;
    this.open = false;
    this.release();
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

  private release() {
    if (!this.typing) return;
    this.typing = false;
    this.keyInput.blur();
    void call("set_focusable", { on: false });
  }

  private next() {
    if (!this.open) return;
    this.release();
    this.index += 1;
    this.el.classList.add("swapping");
    setTimeout(() => {
      this.render();
      this.el.classList.remove("swapping");
      this.onResize(this.size());
    }, 170);
  }

  private async choose(brain: Brain) {
    this.brain = brain;
    this.paintBrain();
    try {
      const settings = await getSettings();
      if (brain === "local") await setSettings({ ...settings, aiOn: false });
      else if (brain === "gemini") {
        const gemini = settings.aiUrl.includes("googleapis.com") ? {} : GEMINI;
        await setSettings({ ...settings, ...gemini, aiOn: true });
        const saved = await call<boolean>("ai_key_saved", undefined, false);
        this.keyInput.placeholder = saved ? "Clave guardada" : "Pega aquí tu clave";
        this.getKey.hidden = saved;
      }
    } catch (error) {
      this.say(String(error), false);
    }
    this.onResize(this.size());
  }

  private async saveKey() {
    const key = this.keyInput.value.trim();
    if (!key) return;
    try {
      await call("ai_key_set", { key });
      this.keyInput.value = "";
      this.keyInput.placeholder = "Clave guardada";
      this.getKey.hidden = true;
      this.release();
    } catch (error) {
      this.say(String(error), false);
    }
  }

  private paintBrain() {
    for (const button of this.brainEl.querySelectorAll<HTMLElement>("button")) button.classList.toggle("on", button.dataset.brain === this.brain);
    this.keyRow.hidden = this.brain !== "gemini";
    this.say(this.brain ? BRAIN_NOTE[this.brain] : "Elige una. Lo puedes cambiar después en Ajustes → IA.", false);
  }

  private render() {
    const step = STEPS[this.index];
    const last = this.index === STEPS.length - 1;

    this.stepEl.textContent = `Guía rápida · ${this.index + 1} de ${STEPS.length}`;
    this.title.textContent = step.title;
    this.body.textContent = step.kind === "end" ? this.voiceLine() : step.body;
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

    this.brainEl.hidden = step.kind !== "brain";
    this.keyRow.hidden = true;
    if (step.kind === "brain") this.paintBrain();
    else this.say(keys.length || !step.chord ? step.waiting : "Ese atajo está apagado en tus ajustes.", false);

    this.skip.hidden = last;
    this.voiceGo.hidden = !last || this.voice?.installed === true;
    this.done.hidden = step.kind === "chord" && keys.length > 0;
    this.done.textContent = last ? "Entendido" : "Siguiente";
  }

  private voiceLine() {
    if (this.voice?.installed) return "La voz local ya está en tu PC: dictar y dar órdenes funciona sin internet.";
    const size = this.voice?.megabytes ?? 424;
    return `Para dictar y dar órdenes hace falta la voz local. Se baja una sola vez en Ajustes → Voz (${size} MB) y luego funciona sin internet.`;
  }

  private say(text: string, good: boolean) {
    this.live.textContent = text;
    this.live.classList.toggle("good", good);
  }
}
