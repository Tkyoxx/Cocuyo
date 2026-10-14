// reloj mundial: la hora en otras ciudades, para no despertar a nadie a las 3 de la mañana

const ZONES = {
  "Bogotá": "America/Bogota",
  "Medellín": "America/Bogota",
  "Ciudad de México": "America/Mexico_City",
  "Buenos Aires": "America/Argentina/Buenos_Aires",
  "Santiago": "America/Santiago",
  "Lima": "America/Lima",
  "Caracas": "America/Caracas",
  "Nueva York": "America/New_York",
  "Los Ángeles": "America/Los_Angeles",
  "Miami": "America/New_York",
  "Madrid": "Europe/Madrid",
  "Londres": "Europe/London",
  "París": "Europe/Paris",
  "Berlín": "Europe/Berlin",
  "Tokio": "Asia/Tokyo",
  "Seúl": "Asia/Seoul",
  "Sídney": "Australia/Sydney",
  "Dubái": "Asia/Dubai",
  "São Paulo": "America/Sao_Paulo",
};

const plain = (text) => text.toLowerCase().normalize("NFD").replace(/\p{Mn}/gu, "");

export default function (cocuyo) {
  const cities = cocuyo.leer("ciudades", ["Madrid", "Nueva York", "Tokio", "Buenos Aires"]);
  let page = null;
  let timer = 0;

  const parts = (zone) => {
    const now = new Date();
    const time = new Intl.DateTimeFormat("es", { hour: "2-digit", minute: "2-digit", hour12: false, timeZone: zone }).format(now);
    const hour = Number(time.slice(0, 2));
    const day = new Intl.DateTimeFormat("es", { weekday: "long", timeZone: zone }).format(now);
    const here = new Date(now.toLocaleString("en-US"));
    const there = new Date(now.toLocaleString("en-US", { timeZone: zone }));
    const diff = Math.round((there - here) / 3600000);
    return { time, hour, day, diff };
  };

  const paint = () => {
    if (!page) return;
    page.innerHTML = cities
      .map((city) => {
        const { time, hour, day, diff } = parts(ZONES[city] ?? "UTC");
        const night = hour < 6 || hour >= 19;
        const offset = diff === 0 ? "Tu hora" : `${diff > 0 ? "+" : ""}${diff} h`;
        return `<div class="rm-city ${night ? "rm-night" : ""}"><span class="rm-sky">${night ? "🌙" : hour < 9 ? "🌅" : "☀️"}</span><b>${time}</b><span class="rm-name">${city}</span><small>${day} · ${offset}</small></div>`;
      })
      .join("");
  };

  cocuyo.css(`
    .rm { display: grid; grid-template-columns: repeat(4, 1fr); gap: 10px; height: 100%; padding: 2px 18px 16px; }
    .rm-city { display: flex; flex-direction: column; justify-content: flex-end; gap: 4px; padding: 14px; border-radius: 18px;
      background: linear-gradient(160deg, #5aa9ff33, #f2b84b22); border: 1px solid rgb(255 255 255 / 0.06); }
    .rm-city.rm-night { background: linear-gradient(160deg, #1b2050, #0b0f24); }
    .rm-sky { font-size: 24px; margin-bottom: auto; }
    .rm-city b { font-size: 34px; font-weight: 700; font-variant-numeric: tabular-nums; letter-spacing: -0.02em; }
    .rm-name { font-size: 14px; font-weight: 600; }
    .rm-city small { color: var(--dim); font-size: 11.5px; text-transform: capitalize; }
  `);

  cocuyo.pestana({
    id: "reloj",
    titulo: "Mundo",
    ancho: 760,
    alto: 230,
    html: `<div class="rm"></div>`,
    montar(el) {
      page = el.querySelector(".rm");
      paint();
    },
    mostrar() {
      paint();
      clearInterval(timer);
      timer = setInterval(paint, 15000);
    },
    ocultar() {
      clearInterval(timer);
    },
  });

  cocuyo.herramienta({
    nombre: "hora_en",
    descripcion: `Dice la hora actual en una ciudad. Ciudades que conoce: ${Object.keys(ZONES).join(", ")}.`,
    parametros: { type: "object", properties: { ciudad: { type: "string" } }, required: ["ciudad"] },
    hacer({ ciudad }) {
      const found = Object.keys(ZONES).find((name) => plain(name) === plain(String(ciudad ?? ""))) ?? Object.keys(ZONES).find((name) => plain(String(ciudad ?? "")).includes(plain(name)));
      if (!found) return "No conozco esa ciudad.";
      const { time, day, diff } = parts(ZONES[found]);
      return `En ${found} son las ${time} del ${day} (${diff === 0 ? "la misma hora que aquí" : `${diff > 0 ? "+" : ""}${diff} horas respecto a aquí`}).`;
    },
  });

  cocuyo.orden(["que hora es en", "que horas son en"], (texto) => {
    const city = Object.keys(ZONES).find((name) => plain(texto).includes(plain(name)));
    if (!city) return cocuyo.decir("Esa ciudad no la tengo en el reloj mundial.");
    const { time } = parts(ZONES[city]);
    cocuyo.decir(`En ${city} son las ${time}.`);
  });
}
