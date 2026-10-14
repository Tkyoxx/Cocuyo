// cocuyos: luciernagas alrededor de la isla mientras hablas. solo dibuja cuando estas hablando, el resto del tiempo duerme

export default function (cocuyo) {
  const PAD = 18;
  const canvas = document.createElement("canvas");
  canvas.style.cssText = `position:absolute;inset:-${PAD}px;width:calc(100% + ${PAD * 2}px);height:calc(100% + ${PAD * 2}px);pointer-events:none;opacity:0;transition:opacity .35s`;
  cocuyo.isla.append(canvas);
  const g = canvas.getContext("2d");

  const flies = Array.from({ length: 16 }, (_, i) => ({ t: i / 16, speed: 0.04 + Math.random() * 0.05, wobble: Math.random() * 6.28, size: 1.2 + Math.random() * 1.6 }));
  let level = 0;
  let target = 0;
  let on = false;
  let frame = 0;
  let last = 0;

  function point(t, w, h) {
    const perimeter = 2 * (w + h);
    let d = (t % 1) * perimeter;
    if (d < w) return [d, 0];
    d -= w;
    if (d < h) return [w, d];
    d -= h;
    if (d < w) return [w - d, h];
    return [0, h - (d - w)];
  }

  function draw(now) {
    const dt = Math.min(0.05, (now - last) / 1000 || 0.016);
    last = now;
    const ratio = devicePixelRatio || 1;
    const w = canvas.clientWidth;
    const h = canvas.clientHeight;
    if (canvas.width !== Math.round(w * ratio)) {
      canvas.width = Math.round(w * ratio);
      canvas.height = Math.round(h * ratio);
    }
    level += (target - level) * 0.25;
    g.setTransform(ratio, 0, 0, ratio, 0, 0);
    g.clearRect(0, 0, w, h);
    for (const fly of flies) {
      fly.t += fly.speed * dt * (1 + level * 3);
      fly.wobble += dt * 3;
      const [x, y] = point(fly.t, w - PAD * 1.2, h - PAD * 1.2);
      const drift = Math.sin(fly.wobble) * (4 + level * 8);
      const px = x + PAD * 0.6 + drift * 0.6;
      const py = y + PAD * 0.6 + Math.cos(fly.wobble * 1.3) * (3 + level * 6);
      const glow = (0.35 + level * 0.65) * (0.6 + 0.4 * Math.sin(fly.wobble * 2.2));
      const radius = fly.size * (2.2 + level * 3);
      const light = g.createRadialGradient(px, py, 0, px, py, radius);
      light.addColorStop(0, `rgba(255, 236, 160, ${glow})`);
      light.addColorStop(0.35, `rgba(242, 184, 75, ${glow * 0.6})`);
      light.addColorStop(1, "rgba(242, 184, 75, 0)");
      g.fillStyle = light;
      g.beginPath();
      g.arc(px, py, radius, 0, Math.PI * 2);
      g.fill();
    }
    frame = on || level > 0.02 ? requestAnimationFrame(draw) : 0;
  }

  function wake() {
    if (!frame) {
      last = performance.now();
      frame = requestAnimationFrame(draw);
    }
  }

  cocuyo.al("orden", (event) => {
    on = event.phase === "start";
    canvas.style.opacity = on ? "1" : "0";
    if (on) wake();
    else target = 0;
  });
  cocuyo.al("nivel", (peak) => {
    target = Math.min(1, Math.sqrt(Number(peak) || 0) * 1.8);
    if (on) wake();
  });
}
