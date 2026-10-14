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
  activa: boolean;
}

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

function api(deps: Deps, extension: { id: string; nombre: string }) {
  const key = (name: string) => `cocuyo.ext.${extension.id}.${name}`;
  return Object.freeze({
    version: "2",
    extension,
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
    flotante: (el: HTMLElement) => deps.float(el),
    ejecutar: (script: string, dato = "") => call<string>("script_run", { name: script, data: dato }),
    invocar: <T>(comando: string, args?: Record<string, unknown>) => call<T>(comando, args),
    ajustes: (): Promise<Settings> => getSettings(),
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
  (window as unknown as { cocuyo: CocuyoApi }).cocuyo = api(deps, { id: "consola", nombre: "Consola" });
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
      await module.default?.(api(deps, { id: extension.id, nombre: extension.nombre }));
    } catch (error) {
      console.error(`[extensiones] ${extension.id}`, error);
      deps.say(`La extensión «${extension.nombre}» falló al arrancar. Desactívala en Ajustes, sección Extensiones.`, 6000);
    }
  }
  emit("listo");
}
