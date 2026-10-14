// el menu de la isla y sus pestañas

import { call, type Settings } from "./bridge";
import type { Size } from "./island";
import { PAGES, settingsPage, type Host, type Page } from "./pages";
import { Spring } from "./spring";

export class Menu {
  open = false;
  typing = false;
  onSize: (size: Size) => void = () => {};
  onSettings: (settings: Settings) => void = () => {};
  onSay: (text: string) => void = () => {};
  onClose = () => {};

  private root = document.getElementById("menu")!;
  private tabsEl = document.getElementById("tabs")!;
  private ind = document.getElementById("tab-ind")!;
  private pagesEl = document.getElementById("pages")!;
  private buttons = new Map<string, HTMLButtonElement>();
  private built = new Map<string, HTMLElement>();
  private current = PAGES[0];
  private indX = new Spring(0, (x) => (this.ind.style.transform = `translateX(${x}px)`), {
    stiffness: 380,
    damping: 30,
  });
  private indW = new Spring(0, (x) => (this.ind.style.width = `${x}px`), {
    stiffness: 380,
    damping: 30,
  });

  private host: Host = {
    say: (text) => this.onSay(text),
    close: () => this.onClose(),
    focus: (input) => {
      if (this.typing) return;
      this.typing = true;
      void call("set_focusable", { on: true }).then(() => input.focus());
    },
    settingsChanged: (settings) => this.onSettings(settings),
  };

  constructor() {
    for (const page of PAGES) this.addButton(page);
    new ResizeObserver(() => {
      if (this.open) this.layout(true);
    }).observe(this.tabsEl);
    window.addEventListener("blur", () => {
      if (this.typing) this.onClose();
    });
    window.addEventListener("keydown", (e) => {
      if (e.key === "Escape" && this.open) this.onClose();
    });
  }

  addPage(page: Page) {
    if (this.buttons.has(page.id)) return;
    const settings = PAGES.indexOf(settingsPage);
    PAGES.splice(settings < 0 ? PAGES.length : settings, 0, page);
    this.addButton(page, this.buttons.get(settingsPage.id));
    if (this.open) this.layout(true);
  }

  private addButton(page: Page, before?: HTMLElement) {
    const button = document.createElement("button");
    button.className = "tab";
    button.textContent = page.label;
    button.addEventListener("click", () => this.select(page));
    if (before) this.tabsEl.insertBefore(button, before);
    else this.tabsEl.append(button);
    this.buttons.set(page.id, button);
  }

  show(id?: string) {
    const page = PAGES.find((p) => p.id === id) ?? this.current;
    const wasOpen = this.open;
    this.open = true;
    this.select(page, !wasOpen);
  }

  hide() {
    if (!this.open) return;
    this.open = false;
    this.current.hide?.();
    if (this.typing) {
      this.typing = false;
      (document.activeElement as HTMLElement | null)?.blur();
      void call("set_focusable", { on: false });
    }
    for (const el of this.built.values()) el.classList.remove("on");
  }

  keys(mask: number) {
    settingsPage.keys(mask);
  }

  key(vk: number) {
    settingsPage.key(vk);
  }

  private layout(instant: boolean) {
    const page = this.current;
    const wide = Math.max(page.w, this.tabsEl.scrollWidth + 64);
    const size = { w: Math.min(wide, innerWidth - 80), h: page.h, r: 30 };
    this.root.style.width = `${size.w}px`;
    this.root.style.height = `${size.h}px`;
    this.root.style.marginLeft = `${-size.w / 2}px`;
    const button = this.buttons.get(page.id)!;
    if (instant) {
      this.indX.jump(button.offsetLeft);
      this.indW.jump(button.offsetWidth);
    } else {
      this.indX.to(button.offsetLeft);
      this.indW.to(button.offsetWidth);
    }
    if (instant && this.open) this.onSize(size);
    return size;
  }

  private select(page: Page, instant = false) {
    if (page !== this.current) this.current.hide?.();
    this.current = page;
    for (const [id, button] of this.buttons) button.classList.toggle("on", id === page.id);
    const size = this.layout(instant);

    let el = this.built.get(page.id);
    if (!el) {
      el = document.createElement("div");
      el.className = "page";
      el.dataset.page = page.id;
      el.innerHTML = page.html();
      this.pagesEl.append(el);
      this.built.set(page.id, el);
      page.mount?.(el, this.host);
      void el.offsetWidth;
    }
    for (const [id, other] of this.built) other.classList.toggle("on", id === page.id);
    page.show?.(el, this.host);
    this.onSize(size);
  }
}
