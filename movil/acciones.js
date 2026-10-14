// lo que cocuyo puede hacer en el celular. una pagina no puede tocar el celular sola: todo sale como un boton que tocas tu

import { t } from "./idioma.js";

const ios = /iPhone|iPad|iPod/.test(navigator.userAgent) || (navigator.platform === "MacIntel" && navigator.maxTouchPoints > 1);

const dos = (n) => String(n).padStart(2, "0");

function cuando(hora, fecha) {
  const [h, m] = String(hora ?? "").split(":").map(Number);
  const dia = fecha ? new Date(`${fecha}T00:00:00`) : new Date();
  dia.setHours(h || 0, m || 0, 0, 0);
  if (!fecha && dia.getTime() < Date.now()) dia.setDate(dia.getDate() + 1);
  return dia;
}

const utc = (d) => `${d.getUTCFullYear()}${dos(d.getUTCMonth() + 1)}${dos(d.getUTCDate())}T${dos(d.getUTCHours())}${dos(d.getUTCMinutes())}00Z`;

function calendario(titulo, inicio, minutos = 15) {
  const fin = new Date(inicio.getTime() + minutos * 60000);
  const limpio = (t) => String(t).replace(/[\\;,]/g, (c) => `\\${c}`).replace(/\r?\n/g, "\\n");
  const ics = [
    "BEGIN:VCALENDAR",
    "VERSION:2.0",
    "PRODID:-//Cocuyo//ES",
    "BEGIN:VEVENT",
    `UID:${crypto.randomUUID()}@cocuyo`,
    `DTSTAMP:${utc(new Date())}`,
    `DTSTART:${utc(inicio)}`,
    `DTEND:${utc(fin)}`,
    `SUMMARY:${limpio(titulo)}`,
    "BEGIN:VALARM",
    "ACTION:DISPLAY",
    `DESCRIPTION:${limpio(titulo)}`,
    "TRIGGER:PT0M",
    "END:VALARM",
    "END:VEVENT",
    "END:VCALENDAR",
  ].join("\r\n");
  return URL.createObjectURL(new Blob([ics], { type: "text/calendar" }));
}

function googleCalendar(titulo, inicio, minutos = 15) {
  const fin = new Date(inicio.getTime() + minutos * 60000);
  const q = new URLSearchParams({ action: "TEMPLATE", text: titulo, dates: `${utc(inicio)}/${utc(fin)}`, details: "Desde Cocuyo" });
  return `https://calendar.google.com/calendar/render?${q}`;
}

const abrir = (url) => {
  const a = document.createElement("a");
  a.href = url;
  a.rel = "noopener";
  if (/^https?:/.test(url)) a.target = "_blank";
  a.click();
};

const segura = (url) => {
  try {
    const u = new URL(url);
    return ["https:", "http:", "tel:", "mailto:", "sms:", "geo:"].includes(u.protocol) ? u.href : null;
  } catch {
    return null;
  }
};

// cada accion se vuelve una tarjeta con su boton. devuelve null si no se entiende
export function tarjeta(accion) {
  const titulo = String(accion.titulo ?? accion.texto ?? "").slice(0, 120);
  switch (accion.tipo) {
    case "buscar": {
      const q = String(accion.texto ?? "").slice(0, 300);
      return { icono: "🔎", titulo: q, sub: t("Búsqueda en el celular"), botones: [[t("Buscar"), () => abrir(`https://www.google.com/search?q=${encodeURIComponent(q)}`)]] };
    }
    case "abrir": {
      const url = segura(accion.url);
      if (!url) return null;
      return { icono: "🔗", titulo: titulo || new URL(url).hostname, sub: new URL(url).hostname, botones: [[t("Abrir"), () => abrir(url)]] };
    }
    case "mapa": {
      const q = String(accion.texto ?? "").slice(0, 200);
      const url = ios ? `https://maps.apple.com/?daddr=${encodeURIComponent(q)}` : `https://www.google.com/maps/dir/?api=1&destination=${encodeURIComponent(q)}`;
      return { icono: "🧭", titulo: q, sub: t("Cómo llegar"), botones: [[t("Ir"), () => abrir(url)]] };
    }
    case "llamar": {
      const numero = String(accion.numero ?? "").replace(/[^\d+]/g, "");
      if (!numero) return null;
      return { icono: "📞", titulo: titulo || numero, sub: numero, botones: [[t("Llamar"), () => abrir(`tel:${numero}`)]] };
    }
    case "whatsapp": {
      const numero = String(accion.numero ?? "").replace(/[^\d]/g, "");
      const texto = String(accion.texto ?? "");
      const url = `https://wa.me/${numero}?text=${encodeURIComponent(texto)}`;
      return { icono: "💬", titulo: texto || "WhatsApp", sub: t(numero ? `Para +${numero}` : "Elige el chat"), botones: [[t("Abrir WhatsApp"), () => abrir(url)]] };
    }
    case "alarma":
    case "recordatorio": {
      const inicio = Number.isFinite(accion.en) ? new Date(accion.en) : cuando(accion.hora, accion.fecha);
      const nombre = titulo || t(accion.tipo === "alarma" ? "Alarma de Cocuyo" : "Recordatorio");
      const hora = inicio.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
      const dia = inicio.toDateString() === new Date().toDateString() ? t("hoy") : inicio.toLocaleDateString([], { weekday: "long", day: "numeric" });
      const botones = ios
        ? [[t("Poner en el calendario"), () => abrir(calendario(nombre, inicio))]]
        : [[t("Poner en el calendario"), () => abrir(googleCalendar(nombre, inicio))], [t("Archivo .ics"), () => abrir(calendario(nombre, inicio))]];
      if (ios && accion.tipo === "alarma") {
        botones.unshift([t("Alarma (atajo)"), () => abrir(`shortcuts://run-shortcut?name=${encodeURIComponent("Cocuyo alarma")}&input=text&text=${encodeURIComponent(`${dos(inicio.getHours())}:${dos(inicio.getMinutes())}`)}`)]);
      }
      return { icono: accion.tipo === "alarma" ? "⏰" : "🔔", titulo: nombre, sub: `${dia}, ${hora}`, botones };
    }
    case "copiar": {
      const texto = String(accion.texto ?? "");
      return { icono: "📋", titulo: texto.slice(0, 80), sub: t("Para pegar en el celular"), botones: [[t("Copiar"), () => navigator.clipboard?.writeText(texto)]] };
    }
    case "compartir": {
      const texto = String(accion.texto ?? "");
      return { icono: "📤", titulo: texto.slice(0, 80), sub: t("Compartir"), botones: [[t("Compartir"), () => navigator.share?.({ text: texto }).catch(() => {})]] };
    }
    default:
      return null;
  }
}
