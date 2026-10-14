// los tipos de la api de extensiones, para que el editor te autocomplete

export interface Boton {
  texto: string;
  abrir?: string;
  copiar?: string;
  ejecutar?: string;
  dato?: string;
  accion?: () => void;
}

export interface Fila {
  texto: string;
  valor?: string;
  barra?: number;
}

export interface Tarjeta {
  titulo: string;
  texto?: string;
  icono?: string;
  imagen?: string;
  color?: string;
  filas?: Fila[];
  botones?: Boton[];
  segundos?: number;
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

export interface Eventos {
  orden: { chord: "dictation" | "command" | "lasso"; phase: "start" | "end"; cancelled: boolean };
  nivel: number;
  oido: { chord: string; text: string; reply: string; error: string | null };
  nota: string;
  tarjeta: Tarjeta;
  progreso: { script: string; valor: number | null; texto: string };
  menu: boolean;
  lugar: "top" | "right" | "hidden";
  listo: undefined;
}

export interface Cocuyo {
  version: string;
  extension: { id: string; nombre: string };
  isla: HTMLElement;
  decir(texto: string, ms?: number): void;
  hablar(texto: string): Promise<void>;
  tarjeta(tarjeta: Tarjeta): void;
  progreso(id: string, valor: number | null, texto?: string): void;
  pestana(pestana: Pestana): void;
  orden(frases: string | string[], fn: (texto: string) => void): () => void;
  al<E extends keyof Eventos>(evento: E, fn: (dato: Eventos[E]) => void): () => void;
  al(evento: string, fn: (dato: unknown) => void): () => void;
  flotante(el: HTMLElement): () => void;
  herramienta(herramienta: {
    nombre: string;
    descripcion: string;
    parametros?: Record<string, unknown>;
    hacer(args: Record<string, unknown>): unknown | Promise<unknown>;
  }): () => void;
  ejecutar(script: string, dato?: string): Promise<string>;
  invocar<T = unknown>(comando: string, args?: Record<string, unknown>): Promise<T>;
  ajustes(): Promise<Record<string, unknown>>;
  css(texto: string): HTMLStyleElement;
  leer<T>(clave: string, porDefecto: T): T;
  guardar(clave: string, valor: unknown): void;
  abrirMenu(pestana?: string): void;
  cerrarMenu(): void;
}
