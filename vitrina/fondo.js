// el escritorio de mentira que va detrás de la isla en la vitrina
(() => {
  const canvas = document.createElement("canvas");
  const w = (canvas.width = Math.max(1280, innerWidth * devicePixelRatio));
  const h = (canvas.height = Math.max(720, innerHeight * devicePixelRatio));
  const g = canvas.getContext("2d");
  const sky = g.createLinearGradient(0, 0, 0, h);
  sky.addColorStop(0, "#141a3d");
  sky.addColorStop(0.45, "#3b2a6b");
  sky.addColorStop(0.72, "#a2527f");
  sky.addColorStop(1, "#f3a07a");
  g.fillStyle = sky;
  g.fillRect(0, 0, w, h);
  const glow = g.createRadialGradient(w * 0.7, h * 0.66, 10, w * 0.7, h * 0.66, h * 0.55);
  glow.addColorStop(0, "rgba(255,226,180,0.95)");
  glow.addColorStop(0.25, "rgba(255,170,130,0.45)");
  glow.addColorStop(1, "rgba(255,150,120,0)");
  g.fillStyle = glow;
  g.fillRect(0, 0, w, h);
  for (let i = 0; i < 140; i++) {
    g.fillStyle = `rgba(255,255,255,${0.15 + Math.random() * 0.5})`;
    g.fillRect(Math.random() * w, Math.random() * h * 0.45, 1.4, 1.4);
  }
  const ridges = [
    [0.62, "rgba(72,40,104,0.85)", 0.0021, 38],
    [0.72, "rgba(46,26,78,0.95)", 0.0034, 30],
    [0.84, "rgba(24,14,44,1)", 0.0052, 22],
  ];
  for (const [lift, color, freq, amp] of ridges) {
    g.fillStyle = color;
    g.beginPath();
    g.moveTo(0, h);
    for (let x = 0; x <= w; x += 8) {
      const y = h * lift + Math.sin(x * freq + lift * 11) * amp * devicePixelRatio + Math.sin(x * freq * 3.1) * amp * 0.35 * devicePixelRatio;
      g.lineTo(x, y);
    }
    g.lineTo(w, h);
    g.fill();
  }
  document.documentElement.style.background = `#000 url(${canvas.toDataURL("image/jpeg", 0.92)}) center / cover no-repeat`;
  const cursor = document.createElement("div");
  cursor.id = "vitrina-cursor";
  cursor.innerHTML = '<svg width="22" height="30" viewBox="0 0 22 30"><path d="M2 2 L2 24 L7.5 18.5 L11.5 27.5 L15 26 L11 17 L19 17 Z" fill="#fff" stroke="#000" stroke-width="1.5" stroke-linejoin="round"/></svg>';
  cursor.style.cssText = "position:fixed;left:0;top:0;z-index:2147483647;pointer-events:none;transform:translate(-100px,-100px);filter:drop-shadow(0 1px 2px rgba(0,0,0,.45))";
  addEventListener("DOMContentLoaded", () => document.documentElement.append(cursor));
  document.documentElement.append(cursor);
  addEventListener("pointermove", (e) => (cursor.style.transform = `translate(${e.clientX - 2}px, ${e.clientY - 2}px)`), true);
})();
