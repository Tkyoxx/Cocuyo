// pulso: el corazon del pc en una esquina del escritorio. pregunta cada 2 segundos y se calla si el pc esta tranquilo

export default function (cocuyo) {
  const HISTORY = 40;
  const cpu = [];
  const ram = [];
  const el = document.createElement("div");
  el.innerHTML = `
    <div class="pulso-head"><b>Pulso</b><span class="pulso-dot"></span></div>
    <div class="pulso-row"><span>CPU</span><canvas data-g="cpu"></canvas><b data-v="cpu">–</b></div>
    <div class="pulso-row"><span>RAM</span><canvas data-g="ram"></canvas><b data-v="ram">–</b></div>`;
  el.className = "pulso";
  cocuyo.css(`
    .pulso { position: fixed; right: 18px; bottom: 18px; z-index: 30; width: 220px; padding: 12px 14px; border-radius: 18px;
      background: rgb(10 10 13 / 0.86); border: 1px solid rgb(255 255 255 / 0.09); box-shadow: 0 14px 34px rgb(0 0 0 / 0.45);
      backdrop-filter: blur(14px); color: var(--ink); font: 500 12px var(--font); cursor: pointer; user-select: none;
      animation: pulso-in 0.6s var(--pop) both; }
    @keyframes pulso-in { from { opacity: 0; transform: translateY(14px) scale(0.94); } }
    .pulso-head { display: flex; align-items: center; justify-content: space-between; margin-bottom: 8px; }
    .pulso-head b { font-size: 13px; letter-spacing: 0.02em; }
    .pulso-dot { width: 7px; height: 7px; border-radius: 50%; background: #5fd38d; box-shadow: 0 0 8px #5fd38d; animation: pulso-latido 1.4s ease-in-out infinite; }
    .pulso.alto .pulso-dot { background: #ff6b5e; box-shadow: 0 0 10px #ff6b5e; animation-duration: 0.55s; }
    @keyframes pulso-latido { 50% { transform: scale(0.6); opacity: 0.5; } }
    .pulso-row { display: grid; grid-template-columns: 34px 1fr 40px; align-items: center; gap: 8px; height: 26px; }
    .pulso-row span { color: var(--dim); }
    .pulso-row canvas { width: 100%; height: 24px; }
    .pulso-row b { text-align: right; font-variant-numeric: tabular-nums; }
  `);

  function spark(canvas, values, color) {
    const ratio = devicePixelRatio || 1;
    const w = canvas.clientWidth;
    const h = canvas.clientHeight;
    if (canvas.width !== Math.round(w * ratio)) {
      canvas.width = Math.round(w * ratio);
      canvas.height = Math.round(h * ratio);
    }
    const g = canvas.getContext("2d");
    g.setTransform(ratio, 0, 0, ratio, 0, 0);
    g.clearRect(0, 0, w, h);
    if (values.length < 2) return;
    const step = w / (HISTORY - 1);
    const x0 = w - step * (values.length - 1);
    const fill = g.createLinearGradient(0, 0, 0, h);
    fill.addColorStop(0, color + "66");
    fill.addColorStop(1, color + "00");
    g.beginPath();
    values.forEach((v, i) => g.lineTo(x0 + i * step, h - 2 - v * (h - 4)));
    g.lineTo(w, h);
    g.lineTo(x0, h);
    g.fillStyle = fill;
    g.fill();
    g.beginPath();
    values.forEach((v, i) => (i ? g.lineTo : g.moveTo).call(g, x0 + i * step, h - 2 - v * (h - 4)));
    g.strokeStyle = color;
    g.lineWidth = 1.6;
    g.stroke();
  }

  async function beat() {
    const stats = await cocuyo.invocar("system_stats").catch(() => null);
    if (stats) {
      const c = Math.min(1, stats.cpu / 100);
      const r = stats.memoryUsed / stats.memoryTotal;
      cpu.push(c);
      ram.push(r);
      if (cpu.length > HISTORY) cpu.shift();
      if (ram.length > HISTORY) ram.shift();
      el.querySelector('[data-v="cpu"]').textContent = `${Math.round(c * 100)}%`;
      el.querySelector('[data-v="ram"]').textContent = `${Math.round(r * 100)}%`;
      spark(el.querySelector('[data-g="cpu"]'), cpu, "#f2b84b");
      spark(el.querySelector('[data-g="ram"]'), ram, "#5fd38d");
      el.classList.toggle("alto", c > 0.85 || r > 0.9);
    }
    setTimeout(beat, document.hidden ? 6000 : 2000);
  }

  el.addEventListener("click", () => cocuyo.abrirMenu("equipo"));
  cocuyo.flotante(el);
  beat();
}
