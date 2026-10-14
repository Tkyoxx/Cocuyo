import { call, on } from "./bridge";
import type { Size } from "./island";

const WIDTH = 420;

export class Installer {
  open = false;
  onOpen: (size: Size) => void = () => {};
  onClose = () => {};

  private el = document.getElementById("install")!;
  private title = document.getElementById("install-title")!;
  private sub = document.getElementById("install-sub")!;
  private steps = [...this.el.querySelectorAll<HTMLElement>(".inst-steps li")];
  private go = document.getElementById("install-go") as HTMLButtonElement;
  private skip = document.getElementById("install-skip") as HTMLButtonElement;

  constructor() {
    this.go.addEventListener("click", () => void this.run());
    this.skip.addEventListener("click", () => {
      void call("install_skip");
      this.close();
    });
    on<number>("install-step", (index) => this.mark(index));
  }

  start() {
    this.open = true;
    this.onOpen({ w: WIDTH, h: this.el.offsetHeight, r: 30 });
  }

  private close() {
    this.open = false;
    this.onClose();
  }

  private mark(index: number) {
    this.steps[index]?.classList.add("done");
    this.steps[index + 1]?.classList.add("doing");
    this.el.style.setProperty("--p", String((index + 1) / this.steps.length));
  }

  private async run() {
    this.go.disabled = this.skip.disabled = true;
    this.el.classList.add("running");
    this.steps[0].classList.add("doing");
    this.title.textContent = "Instalando…";
    try {
      await call("install_run");
    } catch (error) {
      this.el.classList.remove("running");
      this.go.disabled = this.skip.disabled = false;
      this.title.textContent = "No se pudo instalar";
      this.sub.textContent = String(error);
      for (const step of this.steps) step.classList.remove("doing");
      return;
    }
    this.el.classList.add("finished");
    this.title.textContent = "Listo";
    this.sub.textContent = "Cocuyo se vuelve a abrir desde su sitio definitivo.";
    setTimeout(() => void call("install_relaunch"), 1500);
  }
}
