// graba la vitrina cuadro a cuadro y la deja como webp animado en media/
// uso: npm run dev (en otra terminal), y luego: node vitrina/grabar.mjs [escena...]
// necesita playwright-core, Chrome y ffmpeg en el PATH

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright-core";

const ROOT = process.env.COCUYO_RAIZ ?? path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const OUT = path.join(ROOT, "media");
const CHROME = process.env.COCUYO_CHROME ?? "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe";
const SCALE = 1.5;
const VIEW = { width: 1280, height: 720 };

const wait = (ms) => new Promise((done) => setTimeout(done, ms));

async function open(view = VIEW) {
  const browser = await chromium.launch({ executablePath: CHROME });
  const page = await browser.newPage({ viewport: view, deviceScaleFactor: (VIEW.width * SCALE) / view.width });
  page.on("pageerror", (e) => console.log("error en la página:", e.message));
  // las fotos de github solo para la cuenta del proyecto: los demás autores son inventados y no deben salir con la cara de alguien real
  await page.route("https://avatars.githubusercontent.com/**", (route) => (route.request().url().includes("/Tkyoxx?") ? route.continue() : route.abort()));
  await page.clock.setFixedTime(new Date("2026-10-14T10:24:00"));
  await page.goto("http://localhost:1420/vitrina.html");
  await wait(1400);
  await page.mouse.move(view.width - 100, view.height - 80);
  const emit = (event, payload) => page.evaluate(([e, p]) => window.__vitrina.emit(e, p), [event, payload]);
  return { browser, page, emit };
}

async function record(page, name, crop, run) {
  const frames = [];
  const cdp = await page.context().newCDPSession(page);
  cdp.on("Page.screencastFrame", async ({ data, metadata, sessionId }) => {
    frames.push({ data, at: metadata.timestamp });
    await cdp.send("Page.screencastFrameAck", { sessionId }).catch(() => {});
  });
  const view = page.viewportSize();
  await cdp.send("Page.startScreencast", { format: "jpeg", quality: 92, everyNthFrame: 1, maxWidth: VIEW.width * SCALE, maxHeight: Math.round((VIEW.width * SCALE * view.height) / view.width) });
  const started = Date.now() / 1000;
  await run();
  const ended = Date.now() / 1000;
  await cdp.send("Page.stopScreencast");
  await wait(200);

  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "cocuyo-vitrina-"));
  const list = [];
  frames.forEach((frame, i) => {
    const file = path.join(dir, `f${String(i).padStart(5, "0")}.jpg`);
    fs.writeFileSync(file, Buffer.from(frame.data, "base64"));
    const next = frames[i + 1]?.at ?? Math.max(ended, frame.at + 0.05);
    list.push(`file '${file.replaceAll("\\", "/")}'`, `duration ${Math.max(0.001, next - frame.at).toFixed(4)}`);
  });
  list.push(list.at(-2));
  fs.writeFileSync(path.join(dir, "lista.txt"), list.join("\n"));
  const probe = execFileSync("ffprobe", ["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width", "-of", "csv=p=0", path.join(dir, "f00000.jpg")]);
  const factor = Number(String(probe).trim()) / view.width;
  const [x, y, w, h] = crop.map((v) => Math.round((v * factor) / 2) * 2);
  const width = Math.min(w, 1440);
  const target = path.join(OUT, `${name}.webp`);
  execFileSync("ffmpeg", [
    "-y", "-loglevel", "error", "-f", "concat", "-safe", "0", "-i", path.join(dir, "lista.txt"),
    "-vf", `fps=30,crop=${w}:${h}:${x}:${y},scale=${width}:-2:flags=lanczos`,
    "-c:v", "libwebp_anim", "-lossless", "0", "-quality", "80", "-compression_level", "6", "-loop", "0", "-an", target,
  ]);
  if (process.env.COCUYO_MUESTRAS) {
    for (const part of [0.15, 0.3, 0.45, 0.6, 0.75, 0.9]) {
      const i = Math.floor(frames.length * part);
      fs.copyFileSync(path.join(dir, `f${String(i).padStart(5, "0")}.jpg`), path.join(process.env.COCUYO_MUESTRAS, `${name}-${Math.round(part * 100)}.jpg`));
    }
  }
  fs.rmSync(dir, { recursive: true, force: true });
  console.log(`${name}.webp: ${frames.length} cuadros, ${(ended - started).toFixed(1)} s, ${(fs.statSync(target).size / 1048576).toFixed(2)} MB`);
}

async function still(page, name, crop) {
  const [x, y, width, height] = crop;
  const png = path.join(os.tmpdir(), `cocuyo-${name}.png`);
  await page.screenshot({ path: png, clip: { x, y, width, height } });
  execFileSync("ffmpeg", ["-y", "-loglevel", "error", "-i", png, "-c:v", "libwebp", "-quality", "90", path.join(OUT, `${name}.webp`)]);
  fs.rmSync(png);
  console.log(`${name}.webp: imagen fija`);
}

async function speak(emit, seconds) {
  const start = Date.now();
  while (Date.now() - start < seconds * 1000) {
    const t = (Date.now() - start) / 1000;
    const syllable = Math.abs(Math.sin(t * 9.5)) * (0.55 + 0.45 * Math.sin(t * 2.3));
    await emit("level", Math.max(0.02, syllable * 0.32 + Math.random() * 0.04));
    await wait(33);
  }
  await emit("level", 0);
}

const TOP = [0, 0, 1280, 720];
const MENU = [180, 0, 920, 460];

const scenes = {
  async portada({ page, emit }) {
    await record(page, "portada", [0, 0, 1024, 576], async () => {
      await wait(900);
      await emit("chord", { chord: "command", phase: "start", cancelled: false, cursor: null });
      await speak(emit, 2.4);
      await emit("chord", { chord: "command", phase: "end", cancelled: false, cursor: null });
      await wait(950);
      await emit("heard", { chord: "command", text: "pon algo de salsa para cocinar", reply: "Listo: «Neón en la playa», de Marea Alta.", error: null });
      await wait(3600);
      await page.mouse.move(512, 16, { steps: 36 });
      await wait(500);
      await page.mouse.down();
      await page.mouse.up();
      await wait(600);
      const tab = await page.locator("#tabs button.tab", { hasText: "Controles" }).boundingBox();
      await page.mouse.move(tab.x + tab.width / 2, tab.y + tab.height / 2, { steps: 16 });
      await page.mouse.down();
      await page.mouse.up();
      await wait(3200);
      await page.mouse.move(980, 540, { steps: 30 });
      await wait(1600);
    });
  },

  async recorrido({ page }) {
    await record(page, "recorrido", TOP, async () => {
      await wait(500);
      await page.mouse.move(640, 18, { steps: 40 });
      await wait(450);
      await page.mouse.down();
      await page.mouse.up();
      await wait(1500);
      for (const tab of ["Hoy", "Controles", "Equipo", "Apps", "Fondos", "Portapapeles"]) {
        const button = page.locator("#tabs button.tab", { hasText: tab });
        const box = await button.boundingBox();
        await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2, { steps: 18 });
        await page.mouse.down();
        await page.mouse.up();
        await wait(tab === "Fondos" ? 2600 : 2000);
      }
      await page.mouse.move(1150, 650, { steps: 30 });
      await wait(1800);
    });
  },

  async barra({ page, emit }) {
    await page.evaluate(() => {
      const settings = window.__vitrina.data.get_settings;
      Object.assign(settings, { barStyle: "normal", barOpacity: 30, barCenter: false });
      window.__vitrinaBarra(settings);
    });
    await wait(500);
    await emit("demo", "ajustes");
    await wait(900);
    await page.click('button[data-sec="barra"]');
    await wait(700);
    await page.mouse.move(640, 300, { steps: 10 });
    await record(page, "barra", TOP, async () => {
      await wait(700);
      const choose = async (label) => {
        const box = await page.locator("#s-bar-style button", { hasText: label }).boundingBox();
        await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2, { steps: 16 });
        await page.mouse.down();
        await page.mouse.up();
        await wait(1300);
      };
      await choose("Transparente");
      const slider = await page.locator("#s-bar-amount").boundingBox();
      await page.mouse.move(slider.x + slider.width * 0.3, slider.y + slider.height / 2, { steps: 14 });
      await page.mouse.down();
      await page.mouse.move(slider.x + slider.width * 0.75, slider.y + slider.height / 2, { steps: 26 });
      await page.mouse.move(slider.x + 4, slider.y + slider.height / 2, { steps: 30 });
      await page.mouse.up();
      await wait(900);
      await choose("Acrílico");
      const center = await page.locator("#s-bar-center").boundingBox();
      await page.mouse.move(center.x + center.width / 2, center.y + center.height / 2, { steps: 16 });
      await page.mouse.down();
      await page.mouse.up();
      await wait(2000);
      await choose("Transparente");
      await wait(1200);
    });
  },

  async galeria({ page, emit }) {
    await page.evaluate(() => {
      window.__vitrina.data.gallery_install = () => new Promise((done) => setTimeout(() => done("como esta mi internet"), 900));
    });
    await emit("demo", "ajustes");
    await wait(900);
    await page.click('button[data-sec="ia"]');
    await wait(700);
    await page.mouse.move(640, 300, { steps: 10 });
    await record(page, "galeria", MENU, async () => {
      await wait(600);
      await page.locator("#s-gallery").scrollIntoViewIfNeeded();
      await wait(300);
      const button = await page.locator("#s-gallery").boundingBox();
      await page.mouse.move(button.x + button.width / 2, button.y + button.height / 2, { steps: 24 });
      await page.mouse.down();
      await page.mouse.up();
      await wait(2200);
      await page.locator('#s-ai-list [data-install="como esta mi internet.ps1"]').waitFor({ timeout: 5000 });
      const install = await page.locator('#s-ai-list [data-install="como esta mi internet.ps1"]').boundingBox();
      await page.mouse.move(install.x + install.width / 2, install.y + install.height / 2, { steps: 26 });
      await wait(400);
      await page.mouse.down();
      await page.mouse.up();
      await wait(2600);
      const publish = await page.locator("#s-ai-list [data-publish]").boundingBox();
      await page.mouse.move(publish.x + publish.width / 2, publish.y + publish.height / 2, { steps: 24 });
      await page.mouse.down();
      await page.mouse.up();
      await wait(2400);
    });
  },

  async fondos({ page, emit }) {
    await emit("demo", "fondos");
    await wait(1600);
    await page.mouse.move(640, 200, { steps: 10 });
    await record(page, "fondos", MENU, async () => {
      await wait(600);
      for (const step of [1, 2, 3]) {
        const card = await page.locator(`#wall [data-wall="${step}"]`).boundingBox();
        await page.mouse.move(card.x + card.width / 2, card.y + card.height / 2, { steps: 18 });
        await page.mouse.down();
        await page.mouse.up();
        await wait(1100);
      }
      const chip = await page.locator('#wall-screens [data-screen]', { hasText: "Monitor 2" }).boundingBox();
      await page.mouse.move(chip.x + chip.width / 2, chip.y + chip.height / 2, { steps: 20 });
      await page.mouse.down();
      await page.mouse.up();
      await wait(1600);
    });
  },

  async sdk({ page, emit }) {
    await record(page, "sdk", TOP, async () => {
      await wait(700);
      await emit("chord", { chord: "command", phase: "start", cancelled: false, cursor: null });
      await speak(emit, 2.2);
      await emit("chord", { chord: "command", phase: "end", cancelled: false, cursor: null });
      await wait(700);
      await emit("heard", { chord: "command", text: "a cómo está el dólar hoy", reply: "El dólar está a 3.185,26 COP.", error: null });
      await emit("script-card", {
        id: 1, script: "dolar hoy", titulo: "1 USD = 3.185,26 COP", texto: "Cambio de referencia de hoy. El de la casa de cambio siempre es peor, ya sabes.",
        icono: "💵", color: "#5fd38d", segundos: 9,
        filas: [{ texto: "Dólar", valor: "3.185,26" }, { texto: "Euro", valor: "3.565,24" }, { texto: "100 dólares", valor: "318.526" }],
        botones: [{ texto: "Copiar", copiar: "3185" }, { texto: "Ver gráfica", abrir: "https://x" }],
      });
      await wait(4200);
      for (let i = 0; i <= 10; i++) {
        await emit("script-progress", { script: "organiza descargas", valor: i / 10, texto: i < 10 ? `Moviendo ${i * 4} de 40 archivos` : "Listo" });
        await wait(220);
      }
      await wait(1500);
      await page.mouse.move(640, 16, { steps: 30 });
      await wait(400);
      await page.mouse.down();
      await page.mouse.up();
      await wait(500);
      const tab = await page.locator("#tabs button.tab", { hasText: "Foco" }).boundingBox();
      await page.mouse.move(tab.x + tab.width / 2, tab.y + tab.height / 2, { steps: 16 });
      await page.mouse.down();
      await page.mouse.up();
      await wait(1200);
      const go = await page.locator(".foco-go").boundingBox();
      await page.mouse.move(go.x + go.width / 2, go.y + go.height / 2, { steps: 16 });
      await page.mouse.down();
      await page.mouse.up();
      await wait(3200);
      await page.mouse.move(1100, 600, { steps: 24 });
      await wait(1400);
      await emit("chord", { chord: "command", phase: "start", cancelled: false, cursor: null });
      await speak(emit, 3);
      await emit("chord", { chord: "command", phase: "end", cancelled: false, cursor: null });
      await wait(600);
      await emit("heard", { chord: "command", text: "cuánto falta del foco", reply: "", error: null });
      await emit("note", "Quedan 24:56 de trabajo.");
      await wait(2600);
    });
  },

  async fijas({ page, emit }) {
    for (const [tab, wait_ms] of [["hoy", 1600], ["controles", 1600], ["equipo", 1800], ["apps", 1600], ["portapapeles", 1400]]) {
      await emit("demo", tab);
      await wait(wait_ms);
      await still(page, `pantalla-${tab}`, [180, 0, 920, 470]);
    }
  },
};

const wanted = process.argv.slice(2);
const SMALL = new Set(["portada"]);
for (const name of wanted.length ? wanted : Object.keys(scenes)) {
  const session = await open(SMALL.has(name) ? { width: 1024, height: 576 } : VIEW);
  try {
    await scenes[name](session);
  } catch (error) {
    console.log(`${name}: ${error.message}`);
  }
  await session.browser.close();
}
