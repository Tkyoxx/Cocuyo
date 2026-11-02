// apagar, reiniciar, bloquear y compañia

import { call } from "../bridge";
import { Page, ic } from "./comun";

export const POWER: [string, string, string, boolean][] = [
  ["lock", "lock", "Bloquear", false],
  ["moon", "sleep", "Suspender", true],
  ["restart", "restart", "Reiniciar", true],
  ["power", "shutdown", "Apagar", true],
];

export const energia: Page = {
  id: "energia",
  label: "Energía",
  w: 660,
  h: 172,
  html: () =>
    POWER.map(
      ([icon, action, label]) => `<button class="power" data-power="${action}"><span class="ti">${ic(icon)}</span><span>${label}</span></button>`,
    ).join(""),
  mount(el, host) {
    for (const button of el.querySelectorAll<HTMLElement>("[data-power]")) {
      const [, action, label, confirm] = POWER.find((p) => p[1] === button.dataset.power)!;
      const text = button.querySelector("span:last-child")!;
      let armed = 0;
      let armedAt = 0;
      button.addEventListener("click", (e) => {
        if (!e.isTrusted || e.detail === 0) return;
        if (confirm && !armed) {
          button.classList.add("armed");
          text.textContent = "Toca otra vez";
          armedAt = performance.now();
          armed = window.setTimeout(() => {
            armed = 0;
            button.classList.remove("armed");
            text.textContent = label;
          }, 3000);
          return;
        }
        if (confirm && performance.now() - armedAt < 600) return;
        clearTimeout(armed);
        armed = 0;
        button.classList.remove("armed");
        text.textContent = label;
        host.close();
        void call("power", { action });
      });
    }
  },
};
