// la forma de la isla, probada sin abrir nada: npm test

import assert from "node:assert/strict";
import { test } from "node:test";
import { face, leaveDelay, LEAVE, LEAVE_AFTER_SHRINK, NOTCH, PEEK, region, shrinks, size, SLIVER, VOICE, type Look } from "../src/forma.ts";

const look = (change: Partial<Look> = {}): Look => ({ base: "idle", place: "top", voice: false, hover: false, panel: { w: 640, h: 420, r: 24 }, ...change });

test("en reposo es el notch y al pasar el mouse se asoma", () => {
  assert.equal(face(look()), "idle");
  assert.deepEqual(size(look(), 1920), NOTCH);
  assert.equal(face(look({ hover: true })), "peek");
  assert.deepEqual(size(look({ hover: true }), 1920), PEEK);
});

test("la voz gana sobre el menú pero no sobre la guía ni la instalación", () => {
  assert.equal(face(look({ voice: true, base: "menu" })), "voice");
  assert.deepEqual(size(look({ voice: true }), 1920), VOICE);
  assert.equal(face(look({ voice: true, base: "guide" })), "guide");
  assert.equal(face(look({ voice: true, base: "install" })), "install");
});

test("al anotar, la isla toma el tamaño del editor y la voz no se lo quita", () => {
  assert.equal(face(look({ base: "anota", voice: true })), "anota");
  assert.deepEqual(size(look({ base: "anota", panel: { w: 900, h: 600, r: 30 } }), 1920), { w: 900, h: 600, r: 30 });
});

test("el menú nunca es más ancho que la pantalla", () => {
  assert.equal(size(look({ base: "menu" }), 1920).w, 640);
  assert.equal(size(look({ base: "menu" }), 500).w, 500 - 60);
});

test("acoplada a la derecha es una rayita y se acuesta al hablar", () => {
  assert.deepEqual(size(look({ place: "right" }), 1920), SLIVER);
  assert.deepEqual(size(look({ place: "right", voice: true }), 1920), { w: VOICE.h, h: VOICE.w, r: VOICE.r });
  assert.deepEqual(size(look({ place: "right", hover: true }), 1920), { w: NOTCH.h, h: NOTCH.w, r: NOTCH.r });
});

test("lo que Windows deja tocar sigue a la isla", () => {
  assert.equal(region("hidden", 168, 28, 1920, 1080), null);
  assert.deepEqual(region("top", 168, 28, 1920, 1080), { x: 1920 / 2 - 228 / 2, y: 0, w: 228, h: 46, r: 0 });
  const right = region("right", 8, 132, 1920, 1080)!;
  assert.equal(right.x + right.w, 1920);
  assert.equal(right.y + right.h / 2, 540);
});

test("si el menú se encoge, el mouse tiene más tiempo para salir", () => {
  assert.equal(shrinks("menu", { w: 640, h: 420, r: 24 }, { w: 640, h: 300, r: 24 }), true);
  assert.equal(shrinks("menu", { w: 640, h: 300, r: 24 }, { w: 640, h: 420, r: 24 }), false);
  assert.equal(shrinks("idle", { w: 640, h: 420, r: 24 }, { w: 1, h: 1, r: 1 }), false);
  assert.equal(leaveDelay(100, 500), LEAVE_AFTER_SHRINK);
  assert.equal(leaveDelay(600, 500), LEAVE);
});
