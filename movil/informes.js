// lo que cocuyo investigo, guardado en el celular para leerlo con calma

import { t } from "./idioma.js";

const TOPE = 40;

function base() {
  return new Promise((listo, mal) => {
    const pedido = indexedDB.open("cocuyo-informes", 1);
    pedido.onupgradeneeded = () => pedido.result.createObjectStore("informes", { keyPath: "en" });
    pedido.onsuccess = () => listo(pedido.result);
    pedido.onerror = () => mal(pedido.error);
  });
}

async function tocar(modo, hacer) {
  const db = await base();
  return new Promise((listo, mal) => {
    const tx = db.transaction("informes", modo);
    const resultado = hacer(tx.objectStore("informes"));
    tx.oncomplete = () => {
      db.close();
      listo(resultado?.result);
    };
    tx.onerror = () => {
      db.close();
      mal(tx.error);
    };
  });
}

export async function guardarInforme(informe) {
  const limpio = { ...informe, en: Number(informe.en) || Date.now() };
  await tocar("readwrite", (almacen) => almacen.put(limpio));
  const todos = await informes();
  for (const viejo of todos.slice(TOPE)) await tocar("readwrite", (almacen) => almacen.delete(viejo.en));
  return limpio;
}

export async function informes() {
  const todos = (await tocar("readonly", (almacen) => almacen.getAll())) ?? [];
  return todos.sort((a, b) => b.en - a.en);
}

export async function borrarInforme(en) {
  await tocar("readwrite", (almacen) => almacen.delete(en));
}

const texto = (valor) => (typeof valor === "string" ? valor.trim() : "");
const lista = (valor) => (Array.isArray(valor) ? valor : []);

function conFuentes(cadena, fuentes) {
  const frag = document.createDocumentFragment();
  for (const parte of cadena.split(/(\[\d+\])/)) {
    const n = parte.match(/^\[(\d+)\]$/)?.[1];
    const fuente = n ? fuentes[Number(n) - 1] : null;
    if (fuente && /^https?:\/\//.test(fuente.url ?? "")) {
      const a = document.createElement("a");
      a.className = "cita";
      a.href = fuente.url;
      a.target = "_blank";
      a.rel = "noopener noreferrer";
      a.textContent = n;
      a.title = fuente.titulo ?? "";
      frag.append(a);
    } else if (parte) {
      frag.append(parte);
    }
  }
  return frag;
}

function parrafos(cadena, fuentes) {
  const frag = document.createDocumentFragment();
  for (const bloque of cadena.split(/\n\s*\n/)) {
    const lineas = bloque.split("\n").map((l) => l.trim()).filter(Boolean);
    if (!lineas.length) continue;
    if (lineas.every((l) => /^[-•*]\s/.test(l))) {
      const ul = document.createElement("ul");
      for (const l of lineas) {
        const li = document.createElement("li");
        li.append(conFuentes(l.replace(/^[-•*]\s+/, ""), fuentes));
        ul.append(li);
      }
      frag.append(ul);
    } else {
      const p = document.createElement("p");
      p.append(conFuentes(lineas.join(" "), fuentes));
      frag.append(p);
    }
  }
  return frag;
}

function titulo(nivel, cadena) {
  const h = document.createElement(nivel);
  h.textContent = cadena;
  return h;
}

export function fecha(en) {
  return new Date(en).toLocaleDateString(undefined, { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
}

export function pintarInforme(caja, informe) {
  caja.replaceChildren();
  const fuentes = lista(informe.fuentes);
  caja.append(titulo("h1", texto(informe.titulo) || texto(informe.tema)));
  const sub = document.createElement("small");
  sub.className = "lector-sub";
  sub.textContent = `${fecha(informe.en)} · ${fuentes.length} ${t("fuentes")}`;
  caja.append(sub);
  if (texto(informe.resumen)) {
    const r = document.createElement("div");
    r.className = "resumen";
    r.append(parrafos(texto(informe.resumen), fuentes));
    caja.append(r);
  }
  const puntos = lista(informe.puntos).map(texto).filter(Boolean);
  if (puntos.length) {
    caja.append(titulo("h2", t("Lo clave")));
    const ul = document.createElement("ul");
    for (const punto of puntos) {
      const li = document.createElement("li");
      li.append(conFuentes(punto, fuentes));
      ul.append(li);
    }
    caja.append(ul);
  }
  for (const seccion of lista(informe.secciones)) {
    if (texto(seccion?.titulo)) caja.append(titulo("h2", texto(seccion.titulo)));
    if (texto(seccion?.texto)) caja.append(parrafos(texto(seccion.texto), fuentes));
  }
  const choques = lista(informe.contradicciones).filter((c) => texto(c?.detalle));
  if (choques.length) {
    caja.append(titulo("h2", t("Donde no se ponen de acuerdo")));
    for (const c of choques) {
      const div = document.createElement("div");
      div.className = "choque";
      if (texto(c.tema)) div.append(titulo("b", texto(c.tema)));
      const p = document.createElement("p");
      p.append(conFuentes(texto(c.detalle), fuentes));
      div.append(p);
      if (texto(c.veredicto)) {
        const v = document.createElement("p");
        v.className = "veredicto";
        v.append(conFuentes(texto(c.veredicto), fuentes));
        div.append(v);
      }
      caja.append(div);
    }
  }
  const finales = lista(informe.conclusiones).map(texto).filter(Boolean);
  if (finales.length) {
    caja.append(titulo("h2", t("Conclusiones")));
    const ul = document.createElement("ul");
    for (const c of finales) {
      const li = document.createElement("li");
      li.append(conFuentes(c, fuentes));
      ul.append(li);
    }
    caja.append(ul);
  }
  if (texto(informe.limites)) {
    caja.append(titulo("h2", t("Lo que no se pudo confirmar")));
    caja.append(parrafos(texto(informe.limites), fuentes));
  }
  if (fuentes.length) {
    caja.append(titulo("h2", t("Fuentes")));
    const ol = document.createElement("ol");
    ol.className = "fuentes";
    for (const fuente of fuentes) {
      const li = document.createElement("li");
      if (/^https?:\/\//.test(fuente?.url ?? "")) {
        const a = document.createElement("a");
        a.href = fuente.url;
        a.target = "_blank";
        a.rel = "noopener noreferrer";
        a.textContent = texto(fuente.titulo) || fuente.url;
        li.append(a);
      } else {
        li.textContent = texto(fuente?.titulo);
      }
      if (texto(fuente?.sitio)) {
        const s = document.createElement("small");
        s.textContent = fuente.sitio;
        li.append(s);
      }
      ol.append(li);
    }
    caja.append(ol);
  }
  const pie = document.createElement("small");
  pie.className = "lector-sub";
  if (informe.llamadas) pie.textContent = t(`Gastó ${informe.llamadas} llamadas a la IA.`);
  caja.append(pie);
}
