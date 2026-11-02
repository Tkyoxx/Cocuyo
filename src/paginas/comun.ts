// lo que comparten todas las pestañas: los iconos, escapar texto y las interfaces

import { type Settings } from "../bridge";

export interface Host {
  say(text: string): void;
  close(): void;
  focus(input: HTMLInputElement): void;
  settingsChanged(settings: Settings): void;
}

export interface Page {
  id: string;
  label: string;
  w: number;
  h: number;
  html(): string;
  mount?(el: HTMLElement, host: Host): void;
  show?(el: HTMLElement, host: Host): void;
  hide?(): void;
}

export const ICONS: Record<string, string> = {
  wifi: "M4 10a12 12 0 0116 0M7 13.5a7.5 7.5 0 0110 0M10 17a3 3 0 014 0",
  bluetooth: "M7 7l10 10-5 4V3l5 4L7 17",
  moon: "M20 14.5A8 8 0 019.5 4 8 8 0 1020 14.5z",
  plane: "M3 13l18-8-6 16-3-7-9-1z",
  cast: "M4 6h16v12h-6M4 14a4 4 0 014 4M4 10a8 8 0 018 8",
  volume: "M4 10v4h3l5 4V6L7 10H4zM16 9a4 4 0 010 6M18.5 6.5a8 8 0 010 11",
  mute: "M4 10v4h3l5 4V6L7 10H4zM16 9.5l5 5M21 9.5l-5 5",
  search: "M11 4a7 7 0 100 14 7 7 0 000-14zM20 20l-4-4",
  mic: "M12 3a3 3 0 00-3 3v5a3 3 0 006 0V6a3 3 0 00-3-3zM6 11a6 6 0 0012 0M12 17v4M9 21h6",
  headset: "M4 15v-3a8 8 0 0116 0v3M4 15a2 2 0 002 2h1v-5H6a2 2 0 00-2 2zM20 15a2 2 0 01-2 2h-1v-5h1a2 2 0 012 2z",
  leave: "M3 14c5-5 13-5 18 0l-2 3-4-1v-3a10 10 0 00-6 0v3l-4 1-2-3z",
  wave: "M3 12h2l2-5 3 10 3-12 3 9 2-2h3",
  echo: "M4 15a8 8 0 0116 0M7 15a5 5 0 0110 0M10 15a2 2 0 014 0",
  hand: "M8 13V6a1.5 1.5 0 013 0v5M11 11V4.5a1.5 1.5 0 013 0V11M14 11V6a1.5 1.5 0 013 0v8a6 6 0 01-6 6h-1a6 6 0 01-5-2.7L3 14a1.5 1.5 0 012.5-1.6L8 15",
  sun: "M12 7a5 5 0 100 10 5 5 0 000-10zM12 2v2M12 20v2M2 12h2M20 12h2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4",
  cloud: "M7 18a4 4 0 01-.5-7.97A5.5 5.5 0 0117.2 11.5 3.3 3.3 0 0117 18H7z",
  rain: "M7 15a4 4 0 01-.5-7.97A5.5 5.5 0 0117.2 8.5 3.3 3.3 0 0117 15H7zM8 18l-1 3M12 18l-1 3M16 18l-1 3",
  storm: "M7 15a4 4 0 01-.5-7.97A5.5 5.5 0 0117.2 8.5 3.3 3.3 0 0117 15H7zM13 16l-2.5 3.5h3L11 23",
  snow: "M7 15a4 4 0 01-.5-7.97A5.5 5.5 0 0117.2 8.5 3.3 3.3 0 0117 15H7zM8 19v.01M12 19v.01M16 19v.01M10 22v.01M14 22v.01",
  fog: "M4 8h16M4 12h16M6 16h12M8 20h8",
  lock: "M6 11h12v9H6zM8.5 11V8a3.5 3.5 0 017 0v3",
  restart: "M20 12a8 8 0 11-2.6-5.9M20 4v5h-5",
  power: "M12 3v9M6.3 6.3a8 8 0 1011.4 0",
  text: "M5 6h14M5 12h14M5 18h9",
  folder: "M3 7h6l2 2h10v10H3z",
  plus: "M12 5v14M5 12h14",
  close: "M6 6l12 12M18 6L6 18",
  image: "M4 5h16v14H4zM4 16l5-5 4 4 3-3 4 4",
  down: "M12 4v12M6 11l6 6 6-6M5 20h14",
  up: "M12 20V8M6 13l6-6 6 6M5 4h14",
};
export const SOLID: Record<string, string> = {
  play: "M8 5v14l11-7z",
  pause: "M7 5h4v14H7zM13 5h4v14h-4z",
  prev: "M6 5h2v14H6zM20 5v14l-10-7z",
  next: "M16 5h2v14h-2zM4 5v14l10-7z",
};

export const ic = (name: string) =>
  name in SOLID
    ? `<svg class="ic solid" viewBox="0 0 24 24"><path d="${SOLID[name]}"/></svg>`
    : `<svg class="ic" viewBox="0 0 24 24"><path d="${ICONS[name]}"/></svg>`;

export const esc = (text: string) =>
  text.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);

export const gb = (bytes: number) => (bytes / 2 ** 30).toFixed(1).replace(".", ",");
