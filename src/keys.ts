export const CTRL = 1;
export const SHIFT = 2;
export const ALT = 4;
export const WIN = 8;

const NAMES: [number, string][] = [
  [CTRL, "Ctrl"],
  [SHIFT, "Shift"],
  [ALT, "Alt"],
  [WIN, "Win"],
];

export const keysOf = (mask: number) => NAMES.filter(([bit]) => mask & bit);

export const countKeys = (mask: number) => keysOf(mask).length;

const NAMED: Record<number, string> = {
  0x08: "Retroceso",
  0x09: "Tab",
  0x0d: "Enter",
  0x13: "Pausa",
  0x20: "Espacio",
  0x21: "Re Pág",
  0x22: "Av Pág",
  0x23: "Fin",
  0x24: "Inicio",
  0x25: "←",
  0x26: "↑",
  0x27: "→",
  0x28: "↓",
  0x2c: "Impr Pant",
  0x2d: "Insert",
  0x2e: "Supr",
  0x91: "Bloq Despl",
};

export function keyLabel(vk: number): string {
  if (NAMED[vk]) return NAMED[vk];
  if (vk >= 0x70 && vk <= 0x87) return `F${vk - 0x6f}`;
  if ((vk >= 0x30 && vk <= 0x39) || (vk >= 0x41 && vk <= 0x5a)) return String.fromCharCode(vk);
  if (vk >= 0x60 && vk <= 0x69) return `Num ${vk - 0x60}`;
  return `Tecla ${vk}`;
}
