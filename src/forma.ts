// la forma de la isla sin pantalla: que cara pone y cuanto mide, para poder probarla

import type { Place, Shape } from "./bridge";

export interface Size {
  w: number;
  h: number;
  r: number;
}

export type Base = "idle" | "menu" | "guide" | "install" | "anota";
export type Face = Base | "voice" | "peek";

export interface Look {
  base: Base;
  place: Place;
  voice: boolean;
  hover: boolean;
  panel: Size;
}

export const NOTCH: Size = { w: 168, h: 28, r: 14 };
export const PEEK: Size = { w: 190, h: 32, r: 16 };
export const VOICE: Size = { w: 216, h: 36, r: 18 };
export const SLIVER: Size = { w: 8, h: 132, r: 6 };
export const EAR = 12;
export const MARGIN = 18;
export const LEAVE = 320;
export const LEAVE_AFTER_SHRINK = 4500;

export function face(look: Look): Face {
  if (look.voice && look.base !== "guide" && look.base !== "install" && look.base !== "anota") return "voice";
  if (look.base !== "idle") return look.base;
  return look.hover ? "peek" : "idle";
}

export function size(look: Look, viewportW: number): Size {
  const now = face(look);
  if (now === "menu" || now === "guide" || now === "install" || now === "anota") {
    return { ...look.panel, w: Math.min(look.panel.w, viewportW - (EAR + MARGIN) * 2) };
  }
  if (look.place !== "right") return now === "voice" ? VOICE : now === "peek" ? PEEK : NOTCH;
  if (now === "idle") return SLIVER;
  const turned = now === "voice" ? VOICE : NOTCH;
  return { w: turned.h, h: turned.w, r: turned.r };
}

export function region(place: Place, w: number, h: number, viewportW: number, viewportH: number): Shape | null {
  if (place === "hidden") return null;
  if (place === "right") {
    const tall = h + (EAR + MARGIN) * 2;
    return { x: viewportW - w - MARGIN, y: viewportH / 2 - tall / 2, w: w + MARGIN, h: tall, r: 0 };
  }
  const wide = w + (EAR + MARGIN) * 2;
  return { x: viewportW / 2 - wide / 2, y: 0, w: wide, h: h + MARGIN, r: 0 };
}

export function shrinks(base: Base, from: Size, to: Size): boolean {
  return base !== "idle" && (to.w < from.w || to.h < from.h);
}

export function leaveDelay(now: number, shrunkUntil: number): number {
  return now < shrunkUntil ? LEAVE_AFTER_SHRINK : LEAVE;
}
