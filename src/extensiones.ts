// el cargador de extensiones y la api que reciben. lo que esta aqui es lo que la comunidad puede tocar

import { convertFileSrc } from "@tauri-apps/api/core";
import { call, getSettings, on, type Settings } from "./bridge";
import type { Page } from "./pages";
import type { Card } from "./tarjeta";

export interface ExtensionInfo {
  id: string;
  nombre: string;
  descripcion: string;
  autor: string;
  version: string;
  tipo: "tema" | "extension";
  estilo: string | null;
  codigo: string | null;
  vista: string | null;
  permisos: Permiso[] | null;
  activa: boolean;
}

export type Permiso = "red" | "portapapeles" | "pantalla" | "voz" | "equipo" | "datos" | "scripts" | "agente" | "escritorio" | "comandos";

export const PERMISOS: Record<Permiso, [string, string]> = {
  red: ["Internet", "Se conecta a internet. Cocuyo no puede impedirlo: esto es lo que el autor declara."],
  portapapeles: ["Lo que copias", "Lee y usa lo que tienes en el portapapeles."],
  pantalla: ["Tu pantalla", "Usa tus pantallas y lo que rodeas con el lazo."],
  voz: ["Lo que dices", "Recibe el texto de lo que le dices a Cocuyo y lo que te responde."],
  equipo: ["Datos del PC", "Lee el procesador, la memoria, la red, tus apps y lo que suena."],
  datos: ["Tus datos", "Lee tus ajustes, tu agenda, tus pendientes y lo que Cocuyo recuerda de ti."],
  scripts: ["Tus scripts", "Ejecuta los scripts de tu carpeta."],
  agente: ["Herramientas del agente", "Le da herramientas nuevas a la IA, que puede usarlas sola."],
  escritorio: ["El escritorio", "Pone cosas en el escritorio, fuera de la isla."],
  comandos: ["Todo Cocuyo", "Puede llamar a cualquier comando del núcleo: lo mismo que la app."],
};

const NEEDS: [RegExp, Permiso][] = [
  [/^(clip_list|clip_use|clip_clear|ocr_clip)$/, "portapapeles"],
  [/^(screens_list|lasso_ask|capture_commit|capture_discard)$/, "pantalla"],
  [/^(system_stats|net_speed|dns_measure|program_info|app_icon|apps_list|media_now|volume_get|mic_muted|pad_status|version|taskbar_state|wallpapers_list|wallpaper_thumb|voice_status)$/, "equipo"],
  [/^(get_settings|agenda_list|memory_list|todos_get|weather|weather_city|spotify_status|scripts_list)$/, "datos"],
  [/^(script_run|script_action)$/, "scripts"],
];

const EVENT_NEEDS: Record<string, Permiso> = { oido: "voz", agenda: "datos", spotify: "equipo" };

export interface Pestana {
  id: string;
  titulo: string;
  ancho?: number;
  alto?: number;
  html: string | (() => string);
  montar?(el: HTMLElement): void;
  mostrar?(el: HTMLElement): void;
  ocultar?(): void;
}

export interface Deps {
  island: HTMLElement;
  say(text: string, ms?: number): void;
  card(card: Card): void;
  progress(id: string, value: number | null, text: string): void;
  addPage(page: Page): void;
  openMenu(id?: string): void;
  closeMenu(): void;
  float(el: HTMLElement): () => void;
}

const orders = new Map<string, { frases: string[]; fn: (texto: string) => void }>();
let sending = 0;

function sendOrders() {
  clearTimeout(sending);
  sending = window.setTimeout(() => {
    void call("extension_commands", { commands: [...orders].map(([id, order]) => ({ id, frases: order.frases })) });
  }, 50);
}

const tools = new Map<string, { descripcion: string; parametros: unknown; hacer: (args: Record<string, unknown>) => unknown }>();
let sendingTools = 0;

function sendTools() {
  clearTimeout(sendingTools);
  sendingTools = window.setTimeout(() => {
    void call("extension_tools", { tools: [...tools].map(([nombre, tool]) => ({ nombre, descripcion: tool.descripcion, parametros: tool.parametros })) });
  }, 50);
}

on<{ call: number; nombre: string; args: Record<string, unknown> }>("agent-tool", async (asked) => {
  const tool = tools.get(asked.nombre);
  try {
    if (!tool) throw new Error("esa herramienta ya no existe");
    const result = await tool.hacer(asked.args ?? {});
    await call("extension_tool_done", { call: asked.call, ok: true, result: typeof result === "string" ? result : JSON.stringify(result ?? "Hecho.") });
  } catch (error) {
    await call("extension_tool_done", { call: asked.call, ok: false, result: String(error instanceof Error ? error.message : error) });
  }
});

on<{ id: string; texto: string }>("extension-command", (heard) => {
  try {
    orders.get(heard.id)?.fn(heard.texto);
  } catch (error) {
    console.error(`[extensiones] orden ${heard.id}`, error);
  }
});

type Handler = (payload: unknown) => void;
const handlers = new Map<string, Set<Handler>>();

export function emit(event: string, payload?: unknown) {
  for (const handler of handlers.get(event) ?? []) {
    try {
      handler(payload);
    } catch (error) {
      console.error(`[extensiones] «${event}»`, error);
    }
  }
}

const FORWARD: [string, string][] = [
  ["heard", "oido"],
  ["chord", "orden"],
  ["level", "nivel"],
  ["note", "nota"],
  ["place", "lugar"],
  ["script-card", "tarjeta"],
  ["script-progress", "progreso"],
  ["agenda", "agenda"],
  ["spotify", "spotify"],
];
for (const [tauri, spanish] of FORWARD) on<unknown>(tauri, (payload) => emit(spanish, payload));

export const url = (path: string) => (/^https?:\/\//.test(path) ? path : convertFileSrc(path));

function api(deps: Deps, extension: { id: string; nombre: string }, permisos: Permiso[] | null) {
  const key = (name: string) => `cocuyo.ext.${extension.id}.${name}`;
  const need = (permiso: Permiso, what: string) => {
    if (permisos && !permisos.includes(permiso) && !permisos.includes("comandos")) {
      throw new Error(`«${extension.nombre}» usa ${what} sin declarar el permiso «${permiso}» en su manifest.json`);
    }
  };
  return Object.freeze({
    version: "3",
    extension,
    permisos: permisos ? [...permisos] : null,
    isla: deps.island,
    decir: (texto: string, ms?: number) => deps.say(String(texto), ms),
    hablar: (texto: string) => call("say_aloud", { text: String(texto) }),
    tarjeta: (tarjeta: Card) => deps.card({ ...tarjeta, script: tarjeta.script ?? extension.nombre }),
    progreso: (id: string, valor: number | null, texto = "") => deps.progress(`${extension.nombre}: ${id}`, valor, texto),
    pestana: (pestana: Pestana) =>
      deps.addPage({
        id: `ext-${extension.id}-${pestana.id}`,
        label: pestana.titulo,
        w: Math.max(420, Math.min(1100, pestana.ancho ?? 640)),
        h: Math.max(160, Math.min(520, pestana.alto ?? 300)),
        html: () => (typeof pestana.html === "function" ? pestana.html() : pestana.html),
        mount: (el) => pestana.montar?.(el),
        show: (el) => pestana.mostrar?.(el),
        hide: () => pestana.ocultar?.(),
      }),
    abrirMenu: (pestana?: string) => deps.openMenu(pestana),
    cerrarMenu: () => deps.closeMenu(),
    al: (evento: string, fn: Handler) => {
      if (EVENT_NEEDS[evento]) need(EVENT_NEEDS[evento], `el evento «${evento}»`);
      if (!handlers.has(evento)) handlers.set(evento, new Set());
      handlers.get(evento)!.add(fn);
      return () => handlers.get(evento)?.delete(fn);
    },
    orden: (frases: string | string[], fn: (texto: string) => void) => {
      const id = `${extension.id}:${orders.size}`;
      orders.set(id, { frases: (Array.isArray(frases) ? frases : [frases]).map(String), fn });
      sendOrders();
      return () => {
        orders.delete(id);
        sendOrders();
      };
    },
    flotante: (el: HTMLElement) => {
      need("escritorio", "cocuyo.flotante");
      return deps.float(el);
    },
    herramienta: (herramienta: { nombre: string; descripcion: string; parametros?: unknown; hacer: (args: Record<string, unknown>) => unknown }) => {
      need("agente", "cocuyo.herramienta");
      const nombre = `${extension.id}_${herramienta.nombre}`.replace(/[^a-zA-Z0-9_-]/g, "_").slice(0, 58);
      tools.set(nombre, {
        descripcion: herramienta.descripcion,
        parametros: herramienta.parametros ?? { type: "object", properties: {} },
        hacer: herramienta.hacer,
      });
      sendTools();
      return () => {
        tools.delete(nombre);
        sendTools();
      };
    },
    ejecutar: (script: string, dato = "") => {
      need("scripts", "cocuyo.ejecutar");
      return call<string>("script_run", { name: script, data: dato });
    },
    invocar: <T>(comando: string, args?: Record<string, unknown>) => {
      const permiso = NEEDS.find(([pattern]) => pattern.test(comando))?.[1] ?? "comandos";
      try {
        need(permiso, `el comando «${comando}»`);
      } catch (error) {
        return Promise.reject(error);
      }
      return call<T>(comando, args);
    },
    ajustes: (): Promise<Settings> => {
      try {
        need("datos", "cocuyo.ajustes");
      } catch (error) {
        return Promise.reject(error);
      }
      return getSettings();
    },
    css: (texto: string) => {
      const style = document.createElement("style");
      style.dataset.extension = extension.id;
      style.textContent = texto;
      document.head.append(style);
      return style;
    },
    leer: <T>(clave: string, porDefecto: T): T => {
      try {
        const raw = localStorage.getItem(key(clave));
        return raw === null ? porDefecto : (JSON.parse(raw) as T);
      } catch {
        return porDefecto;
      }
    },
    guardar: (clave: string, valor: unknown) => {
      try {
        localStorage.setItem(key(clave), JSON.stringify(valor));
      } catch {
        /* sin almacenamiento, se pierde al cerrar */
      }
    },
  });
}

export type CocuyoApi = ReturnType<typeof api>;

export async function loadExtensions(deps: Deps) {
  const settings = await getSettings();
  (window as unknown as { cocuyo: CocuyoApi }).cocuyo = api(deps, { id: "consola", nombre: "Consola" }, null);
  if (!settings.extensionsOn) return;
  const list = await call<ExtensionInfo[]>("extensions_list", undefined, []);
  for (const extension of list.filter((item) => item.activa)) {
    if (extension.estilo) {
      const link = document.createElement("link");
      link.rel = "stylesheet";
      link.href = url(extension.estilo);
      link.dataset.extension = extension.id;
      document.head.append(link);
    }
    if (!extension.codigo) continue;
    try {
      const module = (await import(/* @vite-ignore */ url(extension.codigo))) as { default?: (cocuyo: CocuyoApi) => unknown };
      await module.default?.(api(deps, { id: extension.id, nombre: extension.nombre }, extension.permisos));
    } catch (error) {
      console.error(`[extensiones] ${extension.id}`, error);
      const denied = error instanceof Error && error.message.includes("sin declarar el permiso");
      deps.say(denied ? `${(error as Error).message}.` : `La extensión «${extension.nombre}» falló al arrancar. Desactívala en Ajustes, sección Extensiones.`, 6000);
    }
  }
  emit("listo");
}
