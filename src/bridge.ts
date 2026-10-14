// el puente con el nucleo en rust

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type Chord = "dictation" | "lasso" | "command";

export interface Heard {
  chord: Chord;
  text: string;
  reply: string;
  error: string | null;
}

export interface VoiceStatus {
  installed: boolean;
  installing: boolean;
  megabytes: number;
}

export interface VoiceProgress {
  fraction: number;
  done: boolean;
  error: string | null;
}

export interface AppEntry {
  name: string;
  id: string;
  custom: boolean;
}

export interface Stats {
  cpu: number;
  threads: number;
  memoryUsed: number;
  memoryTotal: number;
  diskUsed: number;
  diskTotal: number;
  uptimeSecs: number;
  top: { name: string; bytes: number; path: string }[];
}

export interface SpotifyStatus {
  source: "cocuyo" | "mis-notas" | null;
  hasClientId: boolean;
  connecting: boolean;
}

export interface NowPlaying {
  title: string;
  artist: string;
  source: string;
  playing: boolean;
  position: number;
  duration: number;
  key: string;
  art: string | null;
}

export interface Todo {
  text: string;
  done: boolean;
}

export interface ClipListing {
  now: number;
  clips: { text: string; at: number; image: { w: number; h: number; thumb: string } | null }[];
}

export interface Speed {
  phase: "ping" | "down" | "up" | "done";
  pingMs: number | null;
  down: number | null;
  up: number | null;
  error: string | null;
}

export interface Dns {
  name: string;
  address: string;
  ms: number | null;
  current: boolean;
  provider: number | null;
}

export function call<T>(command: string, args?: Record<string, unknown>, fallback?: T): Promise<T> {
  if (inTauri) return invoke<T>(command, args);
  const shown = vitrina()?.call(command, args);
  return Promise.resolve((shown === undefined ? fallback : shown) as T);
}

export function on<T>(event: string, handler: (payload: T) => void) {
  subscribe(event, handler);
}

// la vitrina: la misma interfaz en un navegador con datos inventados, para grabar lo del github
interface Vitrina {
  call(command: string, args?: Record<string, unknown>): unknown;
  on(event: string, handler: (payload: never) => void): void;
}

function vitrina(): Vitrina | undefined {
  return (window as unknown as { __vitrina?: Vitrina }).__vitrina;
}

function subscribe<T>(event: string, handler: (payload: T) => void) {
  if (inTauri) void listen<T>(event, (e) => handler(e.payload));
  else vitrina()?.on(event, handler as (payload: never) => void);
}

export interface ChordEvent {
  chord: Chord;
  phase: "start" | "end";
  cancelled: boolean;
  cursor: [number, number] | null;
  asking?: boolean;
}

export interface Settings {
  captureDir: string | null;
  askWhere: boolean;
  copyToClipboard: boolean;
  dock: "auto" | "top" | "right";
  hideOnFullscreen: boolean;
  cueFullscreen: boolean;
  speakFullscreen: boolean;
  gameOverlay: boolean;
  language: "es" | "en" | "pt";
  dictationKeys: number;
  lassoKeys: number;
  commandKeys: number;
  wallpaperDir: string | null;
  liveWallpaper: string | null;
  liveScreens: Record<string, string>;
  barStyle: "normal" | "clear" | "blur" | "acrylic";
  barOpacity: number;
  barCenter: boolean;
  barSolidMax: boolean;
  autostart: boolean;
  portable: boolean;
  spotifyClientId: string | null;
  city: string;
  name: string;
  padOn: boolean;
  padCombo: number;
  padButtons: number;
  dictationInto: string;
  extensionsOn: boolean;
  aiAgent: boolean;
  agentHands: boolean;
  agentConfirm: boolean;
  extensions: string[];
  checkUpdates: boolean;
  aiDaily: number;
  voice: string;
  aiOn: boolean;
  aiFormat: "openai" | "anthropic";
  aiUrl: string;
  aiModel: string;
  aiAlways: boolean;
  aiDictation: boolean;
  aiHear: boolean;
  aiPersona: string;
  captureRegion: Hotkey;
  captureWindow: Hotkey;
  captureScreen: Hotkey;
}

export interface Hotkey {
  mods: number;
  vk: number;
}

export interface Shape {
  x: number;
  y: number;
  w: number;
  h: number;
  r: number;
}

export type Place = "top" | "right" | "hidden";

export interface Captured {
  path: string | null;
  w: number;
  h: number;
  copied: boolean;
  error: string | null;
}

const inTauri = "__TAURI_INTERNALS__" in window;

export function onPlace(handler: (place: Place) => void) {
  subscribe("place", handler);
  if (inTauri) void invoke<Place>("get_place").then(handler);
}

export function onSelect(handler: (phase: "start" | "cancel") => void) {
  subscribe("select", handler);
}

export function onCaptured(handler: (result: Captured) => void) {
  subscribe("captured", handler);
}

export function captureCommit(box: { x: number; y: number; w: number; h: number }): Promise<Captured> {
  if (inTauri) return invoke<Captured>("capture_commit", box);
  return Promise.resolve({ path: null, w: box.w, h: box.h, copied: false, error: null });
}

export function captureDiscard() {
  if (inTauri) void invoke("capture_discard");
}

export function onSettingsRequested(handler: () => void) {
  subscribe("settings", handler);
}

const DEFAULTS: Settings = {
  captureDir: null,
  askWhere: false,
  copyToClipboard: true,
  dock: "auto",
  hideOnFullscreen: true,
  cueFullscreen: true,
  speakFullscreen: true,
  gameOverlay: false,
  language: "es",
  dictationKeys: 9,
  lassoKeys: 3,
  commandKeys: 12,
  wallpaperDir: null,
  liveWallpaper: null,
  liveScreens: {},
  barStyle: "normal",
  barOpacity: 0,
  barCenter: false,
  barSolidMax: true,
  autostart: false,
  portable: false,
  spotifyClientId: null,
  city: "",
  name: "",
  padOn: false,
  padCombo: 0,
  padButtons: 0,
  dictationInto: "",
  extensionsOn: true,
  aiAgent: true,
  agentHands: true,
  agentConfirm: true,
  extensions: [],
  checkUpdates: true,
  aiDaily: 480,
  voice: "",
  aiOn: false,
  aiFormat: "openai",
  aiUrl: "https://generativelanguage.googleapis.com/v1beta/openai",
  aiModel: "gemini-3.5-flash-lite, gemini-3.1-flash-lite",
  aiAlways: false,
  aiDictation: false,
  aiHear: false,
  aiPersona: "",
  captureRegion: { mods: 0, vk: 0x2c },
  captureWindow: { mods: 4, vk: 0x2c },
  captureScreen: { mods: 1, vk: 0x2c },
};

export function onKey(handler: (vk: number) => void) {
  subscribe("key", handler);
}

export function getSettings(): Promise<Settings> {
  if (inTauri) return invoke<Settings>("get_settings");
  return Promise.resolve({ ...DEFAULTS, ...((vitrina()?.call("get_settings") as Partial<Settings> | undefined) ?? {}) });
}

export function setSettings(settings: Settings): Promise<void> {
  if (!inTauri) vitrina()?.call("set_settings", { settings });
  return inTauri ? invoke<void>("set_settings", { settings }) : Promise.resolve();
}

export function currentCaptureDir(): Promise<string | null> {
  return inTauri ? invoke<string | null>("current_capture_dir") : Promise.resolve("Imágenes\\Cocuyo");
}

export function pickCaptureDir(): Promise<string | null> {
  return inTauri ? invoke<string | null>("pick_capture_dir") : Promise.resolve(null);
}

export function captureKeysAvailable(): Promise<boolean[]> {
  return inTauri ? invoke<boolean[]>("capture_keys_available") : Promise.resolve([true, true, true]);
}

export function setChordsPaused(on: boolean) {
  if (inTauri) void invoke("set_chords_paused", { on });
}

export function onChord(handler: (e: ChordEvent) => void) {
  subscribe("chord", handler);
  if (!inTauri) browserChords.push(handler);
}

export function onKeys(handler: (mask: number) => void) {
  if (inTauri) void listen<number>("keys", (e) => handler(e.payload));
  else browserKeys.push(handler);
}

export function onGuideRequested(handler: () => void) {
  subscribe("guide", handler);
}

export function onDemo(handler: (step: string) => void) {
  subscribe("demo", handler);
}

export function setRegion(shapes: Shape[] | null) {
  if (inTauri) void invoke("set_region", { shapes });
}

export function setKeyEcho(on: boolean) {
  if (inTauri) void invoke("set_key_echo", { on });
}

const browserChords: ((e: ChordEvent) => void)[] = [];
const browserKeys: ((mask: number) => void)[] = [];

if (!inTauri) {
  const ARM_DELAY = 140;
  const mods = { ctrl: false, shift: false, alt: false, win: false };
  let dirty = false;
  let active: Chord | null = null;
  let timer = 0;
  let pointer: [number, number] | null = null;

  window.addEventListener("pointermove", (e) => (pointer = [e.clientX, e.clientY]));

  const held = (): Chord | null => {
    if (dirty) return null;
    if (mods.ctrl && mods.win && !mods.shift) return "dictation";
    if (mods.ctrl && mods.shift && !mods.win) return "lasso";
    return null;
  };

  const emit = (chord: Chord, phase: "start" | "end", cancelled = false) => {
    const cursor = phase === "start" && chord === "lasso" ? pointer : null;
    for (const h of browserChords) h({ chord, phase, cancelled, cursor });
  };

  const update = () => {
    const mask = +mods.ctrl | (+mods.shift << 1) | (+mods.alt << 2) | (+mods.win << 3);
    for (const h of browserKeys) h(mask);
    const now = held();
    if (active && now !== active) {
      emit(active, "end", dirty);
      active = null;
    }
    clearTimeout(timer);
    if (!active && now) {
      timer = window.setTimeout(() => {
        if (held() === now) emit((active = now), "start");
      }, ARM_DELAY);
    }
    if (!mods.ctrl && !mods.shift && !mods.alt && !mods.win) dirty = false;
  };

  const key = (down: boolean) => (e: KeyboardEvent) => {
    if (e.key === "Control") mods.ctrl = down;
    else if (e.key === "Shift") mods.shift = down;
    else if (e.key === "Alt") mods.alt = down;
    else if (e.key === "Meta" || e.key === "OS") mods.win = down;
    else if (down) dirty = true;
    update();
  };

  window.addEventListener("keydown", key(true));
  window.addEventListener("keyup", key(false));
}
