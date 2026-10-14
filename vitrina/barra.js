// la barra de tareas de windows 10, para que la vitrina se vea como un escritorio de verdad
(() => {
  const HEIGHT = 40;
  const css = `
    body { position: absolute !important; inset: 0 0 ${HEIGHT}px 0 !important; height: auto !important; transform: translateZ(0); }
    #w10 { position: fixed; left: 0; right: 0; bottom: 0; height: ${HEIGHT}px; z-index: 2147483000; display: flex; align-items: stretch;
      font: 400 12px/1 "Segoe UI", sans-serif; color: #fff; -webkit-font-smoothing: antialiased; user-select: none; }
    #w10 .fondo { position: absolute; inset: 0; background: rgb(16 16 16 / var(--a, 0.88)); backdrop-filter: blur(var(--blur, 22px)) saturate(var(--sat, 1.25));
      transition: background-color .32s cubic-bezier(.25,1,.5,1), backdrop-filter .32s cubic-bezier(.25,1,.5,1); }
    #w10 > :not(.fondo) { position: relative; }
    #w10 .ico { font-family: "Segoe MDL2 Assets"; font-size: 15px; }
    #w10 .boton { width: 48px; display: grid; place-items: center; }
    #w10 .inicio svg { width: 16px; height: 16px; }
    #w10 .buscar { width: 344px; margin: 0; display: flex; align-items: center; gap: 12px; padding: 0 14px; background: rgb(255 255 255 / .1);
      color: rgb(255 255 255 / .78); font-size: 13.5px; border-right: 1px solid rgb(255 255 255 / .04); }
    #w10 .buscar .ico { font-size: 14px; color: #fff; }
    #w10 .tareas { display: flex; transform: translateX(var(--x, 0px)); transition: transform .32s cubic-bezier(.16,1,.3,1); }
    #w10 .app { width: 48px; display: grid; place-items: center; position: relative; }
    #w10 .app img { width: 24px; height: 24px; }
    #w10 .app.abierta::after { content: ""; position: absolute; left: 4px; right: 4px; bottom: 0; height: 2px; background: #9fbfdc; }
    #w10 .app.activa { background: rgb(255 255 255 / .12); }
    #w10 .app.activa::after { background: #c3dcf2; }
    #w10 .resto { flex: 1; }
    #w10 .bandeja { display: flex; align-items: stretch; }
    #w10 .bandeja .mini { width: 24px; display: grid; place-items: center; }
    #w10 .bandeja .mini .ico { font-size: 14px; }
    #w10 .idioma { width: 34px; display: grid; place-items: center; font-size: 12px; }
    #w10 .hora { padding: 0 9px; display: flex; flex-direction: column; justify-content: center; align-items: center; gap: 3px; font-size: 12px; }
    #w10 .avisos { width: 44px; display: grid; place-items: center; }
    #w10 .escritorio { width: 5px; border-left: 1px solid rgb(255 255 255 / .28); }
  `;
  const style = document.createElement("style");
  style.textContent = css;
  document.head.append(style);

  const pinned = ["carpeta", "navegador", "musica", "charla", "editor", "notas"];
  const bar = document.createElement("div");
  bar.id = "w10";
  bar.innerHTML = `
    <div class="fondo"></div>
    <div class="boton inicio"><svg viewBox="0 0 16 16"><path fill="#fff" d="M0 0h7.5v7.5H0zM8.5 0H16v7.5H8.5zM0 8.5h7.5V16H0zM8.5 8.5H16V16H8.5z"/></svg></div>
    <div class="buscar"><span class="ico">&#xE721;</span><span>Escribe aquí para buscar</span></div>
    <div class="boton"><span class="ico" style="font-size:16px">&#xE7C4;</span></div>
    <div class="tareas">${pinned.map((id, i) => `<div class="app ${i < 4 ? "abierta" : ""} ${i === 2 ? "activa" : ""}" data-app="${id}"><img alt=""></div>`).join("")}</div>
    <div class="resto"></div>
    <div class="bandeja">
      <div class="mini"><span class="ico" style="font-size:12px">&#xE70E;</span></div>
      <div class="mini"><span class="ico">&#xE839;</span></div>
      <div class="mini"><span class="ico">&#xE767;</span></div>
      <div class="idioma">ESP</div>
      <div class="hora"><span id="w10-hora"></span><span id="w10-fecha"></span></div>
      <div class="avisos"><span class="ico" style="font-size:16px">&#xE91C;</span></div>
      <div class="escritorio"></div>
    </div>`;
  document.documentElement.append(bar);

  const clock = () => {
    const now = new Date();
    const h = now.getHours();
    bar.querySelector("#w10-hora").textContent = `${h % 12 || 12}:${String(now.getMinutes()).padStart(2, "0")} ${h < 12 ? "a. m." : "p. m."}`;
    bar.querySelector("#w10-fecha").textContent = `${now.getDate()}/${String(now.getMonth() + 1).padStart(2, "0")}/${now.getFullYear()}`;
  };
  clock();
  setInterval(clock, 15000);

  const icons = () => {
    const data = window.__vitrina?.data;
    if (!data) return setTimeout(icons, 50);
    for (const app of bar.querySelectorAll(".app")) app.querySelector("img").src = data.app_icon({ id: app.dataset.app }) ?? "";
  };
  icons();

  window.__vitrinaBarra = ({ barStyle, barOpacity, barCenter }) => {
    const back = bar.querySelector(".fondo");
    const alpha = barStyle === "normal" ? 0.88 : barOpacity / 100;
    back.style.setProperty("--a", String(alpha));
    back.style.setProperty("--blur", barStyle === "clear" ? "0px" : barStyle === "normal" ? "22px" : "26px");
    back.style.setProperty("--sat", barStyle === "acrylic" ? "1.9" : "1.25");
    const tasks = bar.querySelector(".tareas");
    const shift = (innerWidth - tasks.offsetWidth) / 2 - tasks.offsetLeft;
    tasks.style.setProperty("--x", `${barCenter ? Math.max(0, shift) : 0}px`);
  };
})();
