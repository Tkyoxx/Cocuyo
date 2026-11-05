// el micrófono limpio en Ajustes: modos, ajustes finos que se entienden, grabarte y oír el antes y el después, y los juegos

import { call, type Settings } from "./bridge";
import { locale, tr } from "./i18n";
import { esc } from "./paginas/comun";

type Save = (change: Partial<Settings>) => Promise<unknown>;

interface Estado {
  prendido: boolean;
  estado: string;
  cable: string;
  elige: string;
  entrada: string;
  escuchan: string[];
  limpiando: boolean;
  cpu: number;
  voz: boolean;
  entra: number;
  sale: number;
  latencia: number;
  error: string;
  bajo: boolean;
  ahora: string[];
}

interface Resumen {
  segundos: number;
  ruido_antes: number;
  ruido_despues: number;
  voz_antes: number;
  voz_despues: number;
  cpu: number;
  picos_crudo: number[];
  picos_limpio: number[];
}

interface Juego {
  exe: string;
  nombre: string;
  grupo: string;
}

interface Aparato {
  id: string;
  nombre: string;
  cable: boolean;
  windows: boolean;
}

const PRESETS: Record<string, { amount: number; between: boolean; gate: number }> = {
  natural: { amount: 40, between: false, gate: 0 },
  llamada: { amount: 100, between: false, gate: 0 },
  tope: { amount: 100, between: true, gate: 2 },
};

const MODE_HELP: Record<string, string> = {
  natural: "Quita el ruido y deja un poco del ambiente: suenas como en un cuarto tranquilo.",
  llamada: "La recomendada: quita todo el ruido de fondo y deja tu voz tal cual.",
  tope: "Para lugares muy ruidosos: quita todo, limpia entre palabras y calla el micro mientras no hablas.",
  propio: "Tú decides con los ajustes de al lado.",
};

const WHEN_HELP: Record<string, string> = {
  siempre: "Limpia siempre que una app te esté escuchando.",
  jugando: "Se prende sola con lo que reconozca como juego: los de Steam y Epic, los que traen piezas de juego como Unity o Unreal, tengan tienda o no, los conocidos y lo que corra a pantalla completa, sin contar videos ni navegador. Se apaga al cerrarlo.",
  elegidos: "Se prende sola mientras esté abierto lo que marques aquí abajo, sea app o juego, y se apaga al cerrarlo. Fuera de eso tu voz sale igual, sin limpiar.",
};

const PLAY = `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M8 5.5v13l10.5-6.5z"/></svg>`;
const PAUSE = `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 5h3.6v14H7zm6.4 0H17v14h-3.6z"/></svg>`;

const seg = (id: string, options: [string | number, string][]) =>
  `<div class="segmented" id="${id}"><span class="thumb"></span>${options.map(([value, label]) => `<button data-value="${value}">${label}</button>`).join("")}</div>`;

function mark(box: HTMLElement, value: string) {
  const buttons = [...box.querySelectorAll<HTMLElement>("button")];
  box.style.setProperty("--n", String(buttons.length));
  buttons.forEach((button, i) => {
    const active = button.dataset.value === value;
    button.classList.toggle("on", active);
    if (active) box.style.setProperty("--i", String(i));
  });
  box.classList.toggle("none", !buttons.some((button) => button.dataset.value === value));
}

const decimal = (value: number) => (Math.round(value * 10) / 10).toLocaleString(locale(), { maximumFractionDigits: 1 });

export const limpioHtml = () => `<div class="scol">
  <div class="card">
    <label class="srow"><div><b>Quitar el ruido de tu micrófono</b><small>Para Discord, Zoom, WhatsApp y tus juegos: el teclado, el ventilador, el perro, la calle. Todo dentro de tu PC, sin internet.</small></div><input type="checkbox" class="switch" id="s-lm-on"></label>
    <div class="lm-status" id="s-lm-status" hidden></div>
    <div class="srow stack"><div><b>Modo</b><small id="s-lm-mode-help"></small></div>${seg("s-lm-mode", [
      ["natural", "Natural"],
      ["llamada", "Llamada"],
      ["tope", "A tope"],
      ["propio", "A tu gusto"],
    ])}</div>
    <div class="lm-meters" id="s-lm-meters" hidden>
      <div class="lm-meter"><b>Tu micrófono</b><i><s id="s-lm-in"></s></i></div>
      <div class="lm-meter clean"><b>Lo que oyen</b><i><s id="s-lm-out"></s></i></div>
    </div>
  </div>
  <div class="card">
    <div class="srow"><div><b>Escucha la diferencia</b><small id="s-lm-test-help">Grábate 5 segundos hablando normal, con el ruido de siempre, y compara.</small></div><button class="ghost pick" id="s-lm-rec">Grabarme</button></div>
    <div class="lm-player" id="s-lm-player" hidden>
      <button class="lm-play" id="s-lm-play" aria-label="Escuchar">${PLAY}</button>
      <div class="lm-wave"><canvas id="s-lm-wave"></canvas><i class="lm-head" id="s-lm-head"></i></div>
      ${seg("s-lm-ab", [
        ["crudo", "Sin limpiar"],
        ["limpio", "Limpio"],
      ])}
    </div>
    <small class="lm-facts" id="s-lm-facts" hidden></small>
  </div>
  <div class="card">
    <div class="srow stack"><div><b>Cuándo limpiar</b><small id="s-lm-when-help"></small></div>${seg("s-lm-when", [
      ["siempre", "Siempre"],
      ["jugando", "Jugando"],
      ["elegidos", "Los que elijas"],
    ])}<small class="lm-now" id="s-lm-now" hidden></small></div>
    <div class="lm-pick" id="s-lm-games" hidden>
      <input class="s-find lm-filter" id="s-lm-filter" placeholder="Buscar en la lista" spellcheck="false" autocomplete="off" />
      <div class="srow chips flat lm-games" id="s-lm-list"></div>
    </div>
  </div>
</div>
<div class="scol">
  <div class="card">
    <div class="srow stack"><div><b>Cuánto ruido quitar</b><small id="s-lm-amount-text"></small></div><input type="range" class="range" id="s-lm-amount" min="0" max="100" step="5" /></div>
    <label class="srow"><div><b>Limpiar entre palabras</b><small>Borra lo que queda pegado a tu voz. Más limpio, un poco menos natural.</small></div><input type="checkbox" class="switch" id="s-lm-between"></label>
    <div class="srow stack"><div><b>Silencio cuando callas</b><small>Corta todo mientras no hablas: ni el teclado se cuela.</small></div>${seg("s-lm-gate", [
      [0, "Apagado"],
      [1, "Suave"],
      [2, "Medio"],
      [3, "Fuerte"],
    ])}</div>
  </div>
  <div class="card">
    <div class="srow stack"><div><b>Volumen de tu voz</b><small id="s-lm-gain-text"></small></div><input type="range" class="range" id="s-lm-gain" min="-12" max="18" step="1" /></div>
    <label class="srow"><div><b>Voz pareja</b><small>Te sube cuando hablas bajito y te baja cuando gritas.</small></div><input type="checkbox" class="switch" id="s-lm-level"></label>
    <div class="srow stack"><div><b>Precisión</b><small id="s-lm-light-help"></small></div>${seg("s-lm-light", [
      ["exacta", "Exacta"],
      ["ligera", "Ligera"],
    ])}</div>
  </div>
  <div class="card">
    <div class="srow stack"><div><b>Tu micrófono</b><small>El de verdad: el que Cocuyo limpia.</small></div><div class="srow chips flat" id="s-lm-ins"></div></div>
    <div class="srow stack"><div><b>Sale por</b><small id="s-lm-out-help"></small></div><div class="srow chips flat" id="s-lm-outs"></div></div>
  </div>
</div>`;

export class Limpio {
  private el!: HTMLElement;
  private save!: Save;
  private s: Settings | null = null;
  private visible = false;
  private tick = 0;
  private ctx: AudioContext | null = null;
  private crudo: AudioBuffer | null = null;
  private limpio: AudioBuffer | null = null;
  private resumen: Resumen | null = null;
  private side: "crudo" | "limpio" = "limpio";
  private play: { sources: AudioBufferSourceNode[]; gains: GainNode[]; started: number; from: number } | null = null;
  private head = 0;
  private redo = 0;
  private tuned = "";
  private busy = false;
  private cableBusy = false;
  private cableNote = "";
  private gamesTick = 0;
  private gamesShown = "";
  private games: Juego[] = [];
  private filter = "";
  private loadAgain = 0;
  private myGames: string[] | null = null;
  private savingGames = 0;

  mount(el: HTMLElement, save: Save) {
    this.el = el;
    this.save = save;
    const q = <T extends HTMLElement>(sel: string) => el.querySelector<T>(sel)!;
    q<HTMLInputElement>("#s-lm-on").addEventListener("change", (e) => void this.save({ cleanMic: (e.target as HTMLInputElement).checked }));
    q<HTMLInputElement>("#s-lm-between").addEventListener("change", (e) => void this.fine({ cleanMicBetween: (e.target as HTMLInputElement).checked }));
    q<HTMLInputElement>("#s-lm-level").addEventListener("change", (e) => void this.save({ cleanMicLevel: (e.target as HTMLInputElement).checked }));
    const amount = q<HTMLInputElement>("#s-lm-amount");
    amount.addEventListener("input", () => this.paintAmount(Number(amount.value)));
    amount.addEventListener("change", () => void this.fine({ cleanMicAmount: Number(amount.value) }));
    const gain = q<HTMLInputElement>("#s-lm-gain");
    gain.addEventListener("input", () => this.paintGain(Number(gain.value)));
    gain.addEventListener("change", () => void this.save({ cleanMicGain: Number(gain.value) }));
    const section = el.closest<HTMLElement>(".ssec") ?? el;
    section.addEventListener("click", (e) => {
      const target = e.target as HTMLElement;
      const button = target.closest<HTMLElement>("button");
      if (!button || !this.s) return;
      const box = button.parentElement!;
      const value = button.dataset.value ?? "";
      if (button.dataset.lmIn !== undefined) void this.save({ cleanMicIn: button.dataset.lmIn });
      else if (button.dataset.lmOut !== undefined) void this.save({ cleanMicOut: button.dataset.lmOut });
      else if (button.dataset.lmFind !== undefined) void this.findProgram();
      else if (button.dataset.lmGame !== undefined) {
        const exe = button.dataset.lmGame;
        const games = new Set(this.s.cleanMicGames.map((g) => g.toLowerCase()));
        const on = !games.has(exe);
        if (on) games.add(exe);
        else games.delete(exe);
        button.classList.toggle("on", on);
        this.setGames([...games]);
      } else if (button.dataset.lmInstall !== undefined) void this.installCable();
      else if (button.dataset.lmGet !== undefined) void call("script_action", { action: { abrir: "https://vb-audio.com/Cable/" } }).catch(() => {});
      else if (button.dataset.lmRetry !== undefined) void this.save({});
      else if (box.id === "s-lm-mode") void this.save({ cleanMicMode: value });
      else if (box.id === "s-lm-gate") void this.fine({ cleanMicGate: Number(value) });
      else if (box.id === "s-lm-light") void this.save({ cleanMicLight: value === "ligera" });
      else if (box.id === "s-lm-when") void this.save({ cleanMicWhen: value }).then(() => this.el.querySelector("#s-lm-games")?.scrollIntoView({ block: "nearest" }));
      else if (box.id === "s-lm-ab") this.choose(value === "crudo" ? "crudo" : "limpio");
    });
    q<HTMLInputElement>("#s-lm-filter").addEventListener("input", (e) => {
      this.filter = (e.target as HTMLInputElement).value;
      this.renderGames();
    });
    q("#s-lm-rec").addEventListener("click", () => void this.record());
    q("#s-lm-play").addEventListener("click", () => (this.play ? this.stop() : this.start(0)));
    q("#s-lm-wave").addEventListener("click", (e) => {
      if (!this.crudo) return;
      const box = (e.currentTarget as HTMLElement).getBoundingClientRect();
      this.start(Math.max(0, Math.min(1, (e.clientX - box.left) / box.width)) * this.crudo.duration);
    });
  }

  private effective(s: Settings) {
    return s.cleanMicMode === "propio" ? { amount: s.cleanMicAmount, between: s.cleanMicBetween, gate: s.cleanMicGate } : (PRESETS[s.cleanMicMode] ?? PRESETS.llamada);
  }

  private fine(change: Partial<Settings>) {
    if (!this.s) return;
    const now = this.effective(this.s);
    return this.save({ cleanMicMode: "propio", cleanMicAmount: now.amount, cleanMicBetween: now.between, cleanMicGate: now.gate, ...change });
  }

  private setGames(games: string[]) {
    this.myGames = games;
    this.s = { ...this.s!, cleanMicGames: games };
    this.savingGames += 1;
    void Promise.resolve(this.save({ cleanMicGames: games })).finally(() => {
      this.savingGames -= 1;
      if (!this.savingGames) this.myGames = null;
    });
  }

  paint(s: Settings) {
    this.s = this.myGames ? { ...s, cleanMicGames: this.myGames } : s;
    s = this.s;
    const q = <T extends HTMLElement>(sel: string) => this.el.querySelector<T>(sel)!;
    q<HTMLInputElement>("#s-lm-on").checked = s.cleanMic;
    mark(q("#s-lm-mode"), s.cleanMicMode);
    q("#s-lm-mode-help").textContent = tr(MODE_HELP[s.cleanMicMode] ?? MODE_HELP.llamada);
    const now = this.effective(s);
    const amount = q<HTMLInputElement>("#s-lm-amount");
    amount.value = String(now.amount);
    this.paintAmount(now.amount);
    q<HTMLInputElement>("#s-lm-between").checked = now.between;
    mark(q("#s-lm-gate"), String(now.gate));
    const gain = q<HTMLInputElement>("#s-lm-gain");
    gain.value = String(s.cleanMicGain);
    this.paintGain(s.cleanMicGain);
    q<HTMLInputElement>("#s-lm-level").checked = s.cleanMicLevel;
    mark(q("#s-lm-light"), s.cleanMicLight ? "ligera" : "exacta");
    q("#s-lm-light-help").textContent = tr(s.cleanMicLight ? "Gasta menos procesador, para PC justos. Casi igual de limpio." : "Suena igual al modelo original. Usa un poco más de procesador.");
    mark(q("#s-lm-when"), s.cleanMicWhen);
    q("#s-lm-when-help").textContent = tr(WHEN_HELP[s.cleanMicWhen] ?? WHEN_HELP.siempre);
    q("#s-lm-games").hidden = s.cleanMicWhen !== "elegidos";
    if (s.cleanMicWhen !== "elegidos") this.gamesShown = "";
    else this.renderGames();
    const tuned = JSON.stringify([now, s.cleanMicGain, s.cleanMicLevel, s.cleanMicLight]);
    if (this.tuned && tuned !== this.tuned && this.resumen) {
      window.clearTimeout(this.redo);
      this.redo = window.setTimeout(() => void this.reprocess(), 350);
    }
    this.tuned = tuned;
    if (this.visible) {
      void this.paintDevices();
      if (s.cleanMicWhen === "elegidos" && !this.games.length) void this.fetchGames();
      void this.poll();
    }
  }

  show() {
    this.visible = true;
    window.clearInterval(this.tick);
    this.tick = window.setInterval(() => void this.poll(), 250);
    window.clearInterval(this.gamesTick);
    this.gamesTick = window.setInterval(() => {
      if (this.s?.cleanMicWhen === "elegidos") void this.fetchGames();
    }, 3000);
    void this.poll();
    void this.paintDevices();
    if (this.s?.cleanMicWhen === "elegidos") void this.fetchGames();
  }

  hide() {
    this.visible = false;
    window.clearInterval(this.tick);
    this.tick = 0;
    window.clearInterval(this.gamesTick);
    this.gamesTick = 0;
    window.clearTimeout(this.loadAgain);
    this.stop();
  }

  private paintAmount(value: number) {
    const text =
      value >= 100
        ? tr("Todo el ruido")
        : value >= 70
          ? tr("Casi todo: queda un susurro del ambiente")
          : value >= 35
            ? tr("Deja un poco de ambiente")
            : value > 0
              ? tr("Apenas lo baja")
              : tr("Nada: tu micrófono tal cual");
    this.el.querySelector("#s-lm-amount-text")!.textContent = text;
    this.el.querySelector<HTMLElement>("#s-lm-amount")!.style.setProperty("--p", `${value}%`);
  }

  private paintGain(value: number) {
    this.el.querySelector("#s-lm-gain-text")!.textContent = value === 0 ? tr("Igual que tu micrófono") : value > 0 ? `+${value} dB` : `${value} dB`;
    this.el.querySelector<HTMLElement>("#s-lm-gain")!.style.setProperty("--p", `${((value + 12) / 30) * 100}%`);
  }

  private async paintDevices() {
    const found = await call<{ entradas: Aparato[]; salidas: Aparato[] }>("limpio_aparatos", undefined, { entradas: [], salidas: [] });
    if (!this.s) return;
    const chip = (kind: "lmIn" | "lmOut", id: string, label: string, on: boolean) => `<button class="chip${on ? " on" : ""}" data-${kind === "lmIn" ? "lm-in" : "lm-out"}="${esc(id)}">${esc(label)}</button>`;
    const inChosen = found.entradas.some((a) => a.id === this.s!.cleanMicIn) ? this.s.cleanMicIn : "";
    this.el.querySelector("#s-lm-ins")!.innerHTML =
      chip("lmIn", "", tr("El mismo que oye Cocuyo"), !inChosen) + found.entradas.map((a) => chip("lmIn", a.id, a.nombre, a.id === inChosen)).join("");
    const outChosen = found.salidas.some((a) => a.id === this.s!.cleanMicOut) ? this.s.cleanMicOut : "";
    this.el.querySelector("#s-lm-outs")!.innerHTML = found.salidas.length
      ? chip("lmOut", "", tr("Automático"), !outChosen) + found.salidas.map((a) => chip("lmOut", a.id, a.nombre, a.id === outChosen)).join("")
      : "";
    this.el.querySelector("#s-lm-out-help")!.textContent = found.salidas.length
      ? tr("El cable virtual. En las otras apps eliges su otra punta, «CABLE Output», como micrófono.")
      : tr("No encuentro un cable virtual. Instala VB-Cable, es gratis.");
  }

  private async fetchGames() {
    const found = await call<{ lista: Juego[]; cargando: boolean }>("limpio_juegos", undefined, { lista: [], cargando: false });
    this.games = found.lista;
    this.renderGames();
    window.clearTimeout(this.loadAgain);
    if (found.cargando && this.visible) this.loadAgain = window.setTimeout(() => void this.fetchGames(), 800);
  }

  private renderGames() {
    if (!this.s) return;
    const chosen = new Set(this.s.cleanMicGames.map((g) => g.toLowerCase()));
    const fold = (text: string) => text.toLowerCase().normalize("NFD").replace(/[\u0300-\u036f]/g, "");
    const wanted = fold(this.filter.trim());
    const shown = wanted ? this.games.filter((g) => fold(g.nombre).includes(wanted) || g.exe.includes(wanted)) : this.games;
    const chip = (g: Juego) => `<button class="chip${chosen.has(g.exe) ? " on" : ""}" data-lm-game="${esc(g.exe)}" title="${esc(g.exe)}.exe">${esc(g.nombre)}</button>`;
    const group = (id: string, title: string) => {
      const inside = shown.filter((g) => g.grupo === id);
      return inside.length ? `<small class="lm-group">${esc(tr(title))}</small>${inside.map(chip).join("")}` : "";
    };
    const empty = wanted && !shown.length ? `<small class="lm-group">${esc(tr("Nada con ese nombre."))}</small>` : "";
    const html = group("abierta", "Abiertas ahora") + group("voz", "Llamadas y micrófono") + group("juego", "Tus juegos") + group("programa", "Más programas") + group("otra", "Otras") + empty + `<small class="lm-group"></small><button class="chip" data-lm-find>${esc(tr("Buscar un programa…"))}</button>`;
    if (html === this.gamesShown) return;
    this.gamesShown = html;
    this.el.querySelector("#s-lm-list")!.innerHTML = html;
  }

  private async findProgram() {
    const exe = await call<string | null>("limpio_buscar_programa", undefined, null);
    if (!exe || !this.s) return;
    this.setGames([...new Set([...this.s.cleanMicGames, exe])]);
    void this.fetchGames();
  }

  private async poll() {
    if (!this.visible || !this.s) return;
    const st = await call<Estado>("limpio_estado", undefined, { prendido: false, estado: "apagado", cable: "", elige: "", entrada: "", escuchan: [], limpiando: false, cpu: 0, voz: false, entra: -120, sale: -120, latencia: 0, error: "", bajo: false, ahora: [] });
    const status = this.el.querySelector<HTMLElement>("#s-lm-status")!;
    const meters = this.el.querySelector<HTMLElement>("#s-lm-meters")!;
    const running = st.estado === "limpiando" || st.estado === "pasando" || st.estado === "no_da";
    meters.hidden = !running;
    if (running) {
      const fill = (db: number) => Math.max(0, Math.min(1, (db + 60) / 60)).toFixed(3);
      this.el.querySelector<HTMLElement>("#s-lm-in")!.style.transform = `scaleX(${fill(st.entra)})`;
      this.el.querySelector<HTMLElement>("#s-lm-out")!.style.transform = `scaleX(${fill(st.sale)})`;
    }
    if (!this.s.cleanMic) {
      status.hidden = true;
      return;
    }
    const now = this.el.querySelector<HTMLElement>("#s-lm-now")!;
    const watching = this.s.cleanMicWhen !== "siempre";
    now.hidden = !watching;
    if (watching) now.textContent = st.ahora.length ? tr("Ahora: {}. Limpia.").replace("{}", st.ahora.join(tr(" y "))) : tr(this.s.cleanMicWhen === "jugando" ? "Ahora no veo ningún juego abierto." : "Ahora no está abierto nada de lo que marcaste.");
    const who = st.escuchan.length ? st.escuchan.join(tr(" y ")) : tr("Una app");
    let html = "";
    let tone = "";
    if (st.estado === "sin_cable") {
      tone = "warn";
      const note = this.cableNote ? `<small>${esc(this.cableNote)}</small>` : "";
      html = `<span>${esc(tr("Falta el cable virtual. Con VB-Cable (gratis) Discord o Zoom reciben tu voz limpia."))}${note}<small>${esc(tr("VB-Cable es de VB-Audio (vb-cable.com). Es donationware: gratis, y si te sirve puedes apoyarlos."))}</small></span><button class="ghost" data-lm-install${this.cableBusy ? " disabled" : ""}>${esc(tr(this.cableBusy ? "Bajando…" : "Instalar VB-Cable"))}</button><button class="ghost" data-lm-get>${esc(tr("Su página"))}</button><button class="ghost" data-lm-retry>${esc(tr("Ya lo instalé"))}</button>`;
    } else if (st.estado === "sin_micro") {
      tone = "warn";
      html = `<span>${esc(tr("No encuentro tu micrófono."))}</span>`;
    } else if (st.estado === "esperando") {
      html = `<span>${esc(tr("Listo. En Discord, Zoom o WhatsApp elige «{}» como micrófono. Mientras nadie lo use, no gasta nada.").replace("{}", st.elige || "CABLE Output"))}</span>`;
    } else if (st.estado === "limpiando") {
      tone = st.voz ? "on talk" : "on";
      html = `<span>${esc(tr("{} te escucha limpio").replace("{}", who))} · ${esc(tr("{} % de un núcleo").replace("{}", decimal(st.cpu)))} · ${st.latencia} ms${st.bajo ? `<small>${esc(tr("Bajé a Ligera: tu procesador no daba para Exacta."))}</small>` : ""}</span>`;
    } else if (st.estado === "no_da") {
      tone = "warn";
      html = `<span>${esc(tr("Tu procesador no da para limpiar en vivo: tu voz sale sin limpiar. Vuelvo a intentar solo en un rato."))}</span>`;
    } else if (st.estado === "pasando") {
      const none = this.s.cleanMicWhen === "elegidos" && !this.s.cleanMicGames.length;
      tone = none ? "warn" : "";
      html = `<span>${esc(tr(none ? "{} te escucha sin limpiar: marca abajo en qué apps o juegos quieres que limpie." : "{} te escucha sin limpiar: no está abierto ninguno de tus juegos.").replace("{}", who))}</span>`;
    } else if (st.estado === "error") {
      tone = "warn";
      html = `<span>${esc(st.error || tr("Algo falló con el audio."))}</span><button class="ghost" data-lm-retry>${esc(tr("Reintentar"))}</button>`;
    } else {
      html = `<span>${esc(tr("Preparando…"))}</span>`;
    }
    status.hidden = false;
    status.className = `lm-status ${tone}`;
    if (status.dataset.html !== html) {
      status.innerHTML = `<i></i>${html}`;
      status.dataset.html = html;
    }
  }

  private async installCable() {
    if (this.cableBusy) return;
    this.cableBusy = true;
    this.cableNote = tr("Bajando su paquete oficial de 1,3 MB…");
    void this.poll();
    try {
      await call("limpio_instalar_cable");
      this.cableNote = tr("Se abrió su instalador: dale «Install Driver» y reinicia el PC cuando termine. Al volver queda listo.");
    } catch (problem) {
      this.cableNote = tr(String(problem));
    } finally {
      this.cableBusy = false;
      void this.poll();
    }
  }

  private async record() {
    if (this.busy) return;
    this.busy = true;
    this.stop();
    const button = this.el.querySelector<HTMLButtonElement>("#s-lm-rec")!;
    const help = this.el.querySelector<HTMLElement>("#s-lm-test-help")!;
    button.disabled = true;
    let left = 5;
    button.textContent = `${tr("Habla…")} ${left}`;
    const countdown = window.setInterval(() => {
      left = Math.max(0, left - 1);
      button.textContent = left ? `${tr("Habla…")} ${left}` : tr("Limpiando…");
    }, 1000);
    try {
      this.resumen = await call<Resumen>("limpio_grabar", { segundos: 5 });
      await this.load();
      help.textContent = tr("Dale play y cambia entre «Sin limpiar» y «Limpio» mientras suena. Si mueves los ajustes, se vuelve a limpiar solo.");
    } catch (problem) {
      help.textContent = String(problem);
    } finally {
      window.clearInterval(countdown);
      button.disabled = false;
      button.textContent = tr("Grabarme otra vez");
      this.busy = false;
    }
  }

  private async reprocess() {
    if (this.busy || !this.resumen) return;
    this.busy = true;
    try {
      const at = this.play ? this.position() : null;
      this.resumen = await call<Resumen>("limpio_reprocesar");
      await this.load();
      if (at !== null) this.start(at);
    } catch {
    } finally {
      this.busy = false;
    }
  }

  private async load() {
    this.ctx ??= new AudioContext({ sampleRate: 48000 });
    const fetch = async (cual: string) => {
      const bytes = await call<ArrayBuffer>("limpio_audio", { cual });
      const samples = new Float32Array(bytes);
      const buffer = this.ctx!.createBuffer(1, Math.max(1, samples.length), 48000);
      buffer.copyToChannel(samples, 0);
      return buffer;
    };
    [this.crudo, this.limpio] = await Promise.all([fetch("crudo"), fetch("limpio")]);
    this.el.querySelector<HTMLElement>("#s-lm-player")!.hidden = false;
    mark(this.el.querySelector("#s-lm-ab")!, this.side);
    this.draw();
    this.paintFacts();
  }

  private paintFacts() {
    const r = this.resumen;
    const facts = this.el.querySelector<HTMLElement>("#s-lm-facts")!;
    if (!r) {
      facts.hidden = true;
      return;
    }
    const before = Math.round(r.ruido_antes);
    const after = r.ruido_despues <= -110 ? null : Math.round(r.ruido_despues);
    const noise = after === null ? tr("Ruido de fondo: de {} dB a silencio total.").replace("{}", String(before)) : tr("Ruido de fondo: de {} dB a {} dB ({} dB menos).").replace("{}", String(before)).replace("{}", String(after)).replace("{}", String(Math.max(0, before - after)));
    facts.textContent = `${noise} ${tr("Limpiarlo gastó el {} % de un núcleo.").replace("{}", decimal(r.cpu))}`;
    facts.hidden = false;
  }

  private draw() {
    const r = this.resumen;
    const canvas = this.el.querySelector<HTMLCanvasElement>("#s-lm-wave")!;
    if (!r) return;
    const ratio = window.devicePixelRatio || 1;
    const width = canvas.clientWidth || 300;
    const height = canvas.clientHeight || 44;
    canvas.width = Math.round(width * ratio);
    canvas.height = Math.round(height * ratio);
    const g = canvas.getContext("2d")!;
    g.scale(ratio, ratio);
    g.clearRect(0, 0, width, height);
    const accent = getComputedStyle(this.el).getPropertyValue("--accent").trim() || "#f2b84b";
    const top = Math.max(0.05, ...r.picos_crudo, ...r.picos_limpio);
    const bars = r.picos_crudo.length;
    const step = width / bars;
    const paint = (peaks: number[], color: string) => {
      g.fillStyle = color;
      peaks.forEach((p, i) => {
        const h = Math.max(1.5, (Math.sqrt(p / top) * height) / 1.05);
        g.beginPath();
        g.roundRect(i * step + step * 0.18, (height - h) / 2, Math.max(1, step * 0.64), h, 1.5);
        g.fill();
      });
    };
    paint(r.picos_crudo, "rgba(255,255,255,0.16)");
    paint(r.picos_limpio, accent);
  }

  private position() {
    if (!this.play || !this.ctx || !this.crudo) return 0;
    return Math.min(this.crudo.duration, this.play.from + this.ctx.currentTime - this.play.started);
  }

  private start(at: number) {
    if (!this.ctx || !this.crudo || !this.limpio) return;
    this.stop();
    void this.ctx.resume();
    const now = this.ctx.currentTime + 0.02;
    const sources: AudioBufferSourceNode[] = [];
    const gains: GainNode[] = [];
    for (const [buffer, side] of [
      [this.crudo, "crudo"],
      [this.limpio, "limpio"],
    ] as const) {
      const source = this.ctx.createBufferSource();
      source.buffer = buffer;
      const gain = this.ctx.createGain();
      gain.gain.value = side === this.side ? 1 : 0;
      source.connect(gain).connect(this.ctx.destination);
      source.start(now, Math.min(at, buffer.duration - 0.01));
      sources.push(source);
      gains.push(gain);
    }
    sources[0].onended = () => {
      if (this.play?.sources[0] === sources[0]) this.stop();
    };
    this.play = { sources, gains, started: now, from: at };
    this.el.querySelector("#s-lm-play")!.innerHTML = PAUSE;
    const move = () => {
      if (!this.play || !this.crudo) return;
      const done = this.position() / this.crudo.duration;
      this.el.querySelector<HTMLElement>("#s-lm-head")!.style.transform = `translateX(${(done * this.el.querySelector<HTMLElement>(".lm-wave")!.clientWidth).toFixed(1)}px)`;
      this.head = requestAnimationFrame(move);
    };
    this.el.querySelector<HTMLElement>("#s-lm-head")!.classList.add("on");
    move();
  }

  private stop() {
    cancelAnimationFrame(this.head);
    if (this.play) {
      for (const source of this.play.sources) {
        source.onended = null;
        try {
          source.stop();
        } catch {}
      }
      this.play = null;
    }
    this.el?.querySelector<HTMLElement>("#s-lm-head")?.classList.remove("on");
    const button = this.el?.querySelector("#s-lm-play");
    if (button) button.innerHTML = PLAY;
  }

  private choose(side: "crudo" | "limpio") {
    this.side = side;
    mark(this.el.querySelector("#s-lm-ab")!, side);
    if (!this.play || !this.ctx) return;
    const t = this.ctx.currentTime;
    this.play.gains[0].gain.setTargetAtTime(side === "crudo" ? 1 : 0, t, 0.012);
    this.play.gains[1].gain.setTargetAtTime(side === "limpio" ? 1 : 0, t, 0.012);
  }
}
