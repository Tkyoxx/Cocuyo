// foco: un pomodoro que vive en la isla. 25 de trabajo, 5 de descanso, y por voz

export default function (cocuyo) {
  let WORK = cocuyo.leer("trabajo", 25) * 60;
  const REST = cocuyo.leer("descanso", 5) * 60;
  let left = WORK;
  let resting = false;
  let timer = 0;
  let done = cocuyo.leer("hoy", { dia: "", vueltas: 0 });
  let el = null;

  const today = () => new Date().toDateString();
  if (done.dia !== today()) done = { dia: today(), vueltas: 0 };

  const clock = (s) => `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;

  function paint() {
    if (!el) return;
    const total = resting ? REST : WORK;
    el.querySelector(".foco-ring").style.setProperty("--p", String(1 - left / total));
    el.querySelector(".foco-time").textContent = clock(left);
    el.querySelector(".foco-mode").textContent = resting ? "Descanso" : "Trabajo";
    el.querySelector(".foco-go").textContent = timer ? "Pausa" : left === total ? "Empezar" : "Seguir";
    el.querySelector(".foco-count").textContent = `${done.vueltas} ${done.vueltas === 1 ? "vuelta" : "vueltas"} hoy`;
    el.classList.toggle("resting", resting);
  }

  function start() {
    if (timer) return;
    timer = setInterval(() => {
      left -= 1;
      if (left <= 0) finish();
      paint();
    }, 1000);
    paint();
  }

  function pause() {
    clearInterval(timer);
    timer = 0;
    paint();
  }

  function finish() {
    pause();
    if (!resting) {
      done.vueltas += 1;
      cocuyo.guardar("hoy", done);
    }
    resting = !resting;
    left = resting ? REST : WORK;
    const said = resting ? "Se acabó la vuelta. Cinco minutos de descanso, te los ganaste." : "Se acabó el descanso. A darle.";
    cocuyo.hablar(said);
    cocuyo.tarjeta({
      titulo: resting ? "Descanso" : "De vuelta al trabajo",
      texto: said,
      icono: resting ? "☕" : "🎯",
      color: resting ? "#5ef2c2" : "#f2b84b",
      segundos: 20,
      botones: [{ texto: resting ? "Descansar" : "Empezar", accion: start }],
    });
    paint();
  }

  cocuyo.css(`
    .foco { display: grid; grid-template-columns: 190px 1fr; align-items: center; gap: 26px; height: 100%; padding: 4px 24px; }
    .foco-ring { --p: 0; position: relative; width: 170px; height: 170px; border-radius: 50%;
      background: conic-gradient(var(--accent) calc(var(--p) * 360deg), rgb(255 255 255 / 0.08) 0); display: grid; place-items: center;
      transition: background 0.4s; }
    .foco.resting .foco-ring { background: conic-gradient(#5ef2c2 calc(var(--p) * 360deg), rgb(255 255 255 / 0.08) 0); }
    .foco-ring::before { content: ""; position: absolute; inset: 12px; border-radius: 50%; background: var(--black); }
    .foco-time { position: relative; font-size: 38px; font-weight: 700; font-variant-numeric: tabular-nums; }
    .foco-side { display: flex; flex-direction: column; gap: 10px; }
    .foco-mode { font-size: 22px; font-weight: 650; }
    .foco-count { color: var(--dim); font-size: 13px; }
    .foco-buttons { display: flex; gap: 8px; margin-top: 6px; }
    .foco-buttons button { height: 34px; padding: 0 18px; border: 0; border-radius: 12px; font: 600 13px var(--font); background: var(--tile); color: var(--ink); }
    .foco-buttons .foco-go { background: var(--accent); color: var(--accent-ink); }
    .foco-tip { color: var(--dim); font-size: 12px; }
  `);

  cocuyo.pestana({
    id: "foco",
    titulo: "Foco",
    ancho: 560,
    alto: 250,
    html: `<div class="foco">
      <div class="foco-ring"><span class="foco-time">25:00</span></div>
      <div class="foco-side">
        <span class="foco-mode">Trabajo</span>
        <span class="foco-count"></span>
        <div class="foco-buttons"><button class="foco-go">Empezar</button><button class="foco-reset">Reiniciar</button><button class="foco-skip">Saltar</button></div>
        <span class="foco-tip">También por voz: «empieza el foco», «pausa el foco».</span>
      </div>
    </div>`,
    montar(page) {
      el = page.querySelector(".foco");
      el.querySelector(".foco-go").addEventListener("click", () => (timer ? pause() : start()));
      el.querySelector(".foco-reset").addEventListener("click", () => {
        pause();
        left = resting ? REST : WORK;
        paint();
      });
      el.querySelector(".foco-skip").addEventListener("click", finish);
      paint();
    },
  });

  cocuyo.orden(["empieza el foco", "modo pomodoro", "inicia el pomodoro", "start focus"], () => {
    start();
    cocuyo.decir(`Foco en marcha: ${clock(left)}. Yo te aviso.`);
  });
  cocuyo.orden(["pausa el foco", "para el foco", "deten el pomodoro"], () => {
    pause();
    cocuyo.decir("Foco en pausa.");
  });
  cocuyo.orden(["cuanto falta del foco", "cuanto le queda al foco"], () => cocuyo.decir(`Quedan ${clock(left)} de ${resting ? "descanso" : "trabajo"}.`));

  cocuyo.herramienta({
    nombre: "empezar",
    descripcion: "Empieza un pomodoro en la extensión Foco. Si la persona dice cuántos minutos, pásalos.",
    parametros: { type: "object", properties: { minutos: { type: "number", description: "Minutos de trabajo" } } },
    hacer({ minutos }) {
      if (Number(minutos) > 0) {
        pause();
        resting = false;
        WORK = Math.min(180, Math.round(Number(minutos))) * 60;
        left = WORK;
      }
      start();
      return `Foco en marcha: ${clock(left)} de trabajo.`;
    },
  });
  cocuyo.herramienta({
    nombre: "estado",
    descripcion: "Dice cómo va el pomodoro de la extensión Foco: si corre, cuánto falta y cuántas vueltas van hoy.",
    hacer: () => `${timer ? "Corriendo" : "En pausa"}; quedan ${clock(left)} de ${resting ? "descanso" : "trabajo"}; ${done.vueltas} vueltas hoy.`,
  });
  cocuyo.herramienta({ nombre: "pausar", descripcion: "Pausa el pomodoro de la extensión Foco.", hacer: () => (pause(), "Foco en pausa.") });
}
