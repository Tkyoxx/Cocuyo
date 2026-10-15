// codigos qr hechos en casa: modo byte, correccion M, y un svg para pintarlos

const ECC_PER_BLOCK = [-1, 10, 16, 26, 18, 24, 16, 18, 22, 22, 26, 30, 22, 22, 24, 24, 28, 28, 26, 26, 26, 26, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28];
const BLOCKS = [-1, 1, 1, 1, 2, 2, 4, 4, 4, 5, 5, 5, 8, 9, 9, 10, 10, 11, 13, 14, 16, 17, 17, 18, 20, 21, 23, 25, 26, 28, 29, 31, 33, 35, 37, 38, 40, 43, 45, 47, 49];
const M_BITS = 0;

const bit = (x: number, i: number) => ((x >>> i) & 1) !== 0;

function rawModules(version: number) {
  let total = (16 * version + 128) * version + 64;
  if (version >= 2) {
    const align = Math.floor(version / 7) + 2;
    total -= (25 * align - 10) * align - 55;
    if (version >= 7) total -= 36;
  }
  return total;
}

const dataCodewords = (version: number) => Math.floor(rawModules(version) / 8) - ECC_PER_BLOCK[version] * BLOCKS[version];

function multiply(x: number, y: number) {
  let z = 0;
  for (let i = 7; i >= 0; i--) {
    z = (z << 1) ^ ((z >>> 7) * 0x11d);
    z ^= ((y >>> i) & 1) * x;
  }
  return z;
}

function divisor(degree: number) {
  const out = new Array<number>(degree).fill(0);
  out[degree - 1] = 1;
  let root = 1;
  for (let i = 0; i < degree; i++) {
    for (let j = 0; j < degree; j++) {
      out[j] = multiply(out[j], root);
      if (j + 1 < degree) out[j] ^= out[j + 1];
    }
    root = multiply(root, 0x02);
  }
  return out;
}

function remainder(data: number[], by: number[]) {
  const out = new Array<number>(by.length).fill(0);
  for (const b of data) {
    const factor = b ^ (out.shift() as number);
    out.push(0);
    by.forEach((coef, i) => (out[i] ^= multiply(coef, factor)));
  }
  return out;
}

function withCorrection(data: number[], version: number) {
  const blocks = BLOCKS[version];
  const eccLen = ECC_PER_BLOCK[version];
  const raw = Math.floor(rawModules(version) / 8);
  const short = blocks - (raw % blocks);
  const shortLen = Math.floor(raw / blocks);
  const by = divisor(eccLen);
  const parts: number[][] = [];
  for (let i = 0, k = 0; i < blocks; i++) {
    const piece = data.slice(k, k + shortLen - eccLen + (i < short ? 0 : 1));
    k += piece.length;
    const ecc = remainder(piece, by);
    if (i < short) piece.push(0);
    parts.push(piece.concat(ecc));
  }
  const out: number[] = [];
  for (let i = 0; i < parts[0].length; i++) {
    for (let j = 0; j < parts.length; j++) {
      if (i !== shortLen - eccLen || j >= short) out.push(parts[j][i]);
    }
  }
  return out;
}

function encode(bytes: Uint8Array) {
  for (let version = 1; version <= 40; version++) {
    const countBits = version < 10 ? 8 : 16;
    const capacity = dataCodewords(version) * 8;
    if (4 + countBits + bytes.length * 8 > capacity) continue;
    const bits: number[] = [];
    const put = (value: number, length: number) => {
      for (let i = length - 1; i >= 0; i--) bits.push((value >>> i) & 1);
    };
    put(4, 4);
    put(bytes.length, countBits);
    for (const b of bytes) put(b, 8);
    put(0, Math.min(4, capacity - bits.length));
    put(0, (8 - (bits.length % 8)) % 8);
    for (let pad = 0xec; bits.length < capacity; pad ^= 0xec ^ 0x11) put(pad, 8);
    const words: number[] = [];
    for (let i = 0; i < bits.length; i += 8) words.push(bits.slice(i, i + 8).reduce((acc, b) => (acc << 1) | b, 0));
    return { version, words };
  }
  throw new Error("Demasiado largo para un QR");
}

export function qr(text: string): boolean[][] {
  const { version, words } = encode(new TextEncoder().encode(text));
  const size = version * 4 + 17;
  const dark = Array.from({ length: size }, () => new Array<boolean>(size).fill(false));
  const fixed = Array.from({ length: size }, () => new Array<boolean>(size).fill(false));
  const set = (x: number, y: number, on: boolean) => {
    dark[y][x] = on;
    fixed[y][x] = true;
  };
  for (let i = 0; i < size; i++) {
    set(6, i, i % 2 === 0);
    set(i, 6, i % 2 === 0);
  }
  for (const [cx, cy] of [[3, 3], [size - 4, 3], [3, size - 4]]) {
    for (let dy = -4; dy <= 4; dy++) {
      for (let dx = -4; dx <= 4; dx++) {
        const x = cx + dx;
        const y = cy + dy;
        const ring = Math.max(Math.abs(dx), Math.abs(dy));
        if (x >= 0 && x < size && y >= 0 && y < size) set(x, y, ring !== 2 && ring !== 4);
      }
    }
  }
  if (version > 1) {
    const count = Math.floor(version / 7) + 2;
    const step = Math.floor((version * 8 + count * 3 + 5) / (count * 4 - 4)) * 2;
    const spots = [6];
    for (let pos = size - 7; spots.length < count; pos -= step) spots.splice(1, 0, pos);
    spots.forEach((ax, i) =>
      spots.forEach((ay, j) => {
        if ((i === 0 && j === 0) || (i === 0 && j === count - 1) || (i === count - 1 && j === 0)) return;
        for (let dy = -2; dy <= 2; dy++) for (let dx = -2; dx <= 2; dx++) set(ax + dx, ay + dy, Math.max(Math.abs(dx), Math.abs(dy)) !== 1);
      }),
    );
  }
  const format = (mask: number) => {
    const data = (M_BITS << 3) | mask;
    let rem = data;
    for (let i = 0; i < 10; i++) rem = (rem << 1) ^ ((rem >>> 9) * 0x537);
    const bits = ((data << 10) | rem) ^ 0x5412;
    for (let i = 0; i <= 5; i++) set(8, i, bit(bits, i));
    set(8, 7, bit(bits, 6));
    set(8, 8, bit(bits, 7));
    set(7, 8, bit(bits, 8));
    for (let i = 9; i < 15; i++) set(14 - i, 8, bit(bits, i));
    for (let i = 0; i < 8; i++) set(size - 1 - i, 8, bit(bits, i));
    for (let i = 8; i < 15; i++) set(8, size - 15 + i, bit(bits, i));
    set(8, size - 8, true);
  };
  format(0);
  if (version >= 7) {
    let rem = version;
    for (let i = 0; i < 12; i++) rem = (rem << 1) ^ ((rem >>> 11) * 0x1f25);
    const bits = (version << 12) | rem;
    for (let i = 0; i < 18; i++) {
      const a = size - 11 + (i % 3);
      const b = Math.floor(i / 3);
      set(a, b, bit(bits, i));
      set(b, a, bit(bits, i));
    }
  }
  const all = withCorrection(words, version);
  let n = 0;
  for (let right = size - 1; right >= 1; right -= 2) {
    if (right === 6) right = 5;
    for (let vert = 0; vert < size; vert++) {
      for (let j = 0; j < 2; j++) {
        const x = right - j;
        const y = ((right + 1) & 2) === 0 ? size - 1 - vert : vert;
        if (!fixed[y][x] && n < all.length * 8) {
          dark[y][x] = bit(all[n >>> 3], 7 - (n & 7));
          n++;
        }
      }
    }
  }
  const masked = (mask: number, x: number, y: number) =>
    [
      (x + y) % 2 === 0,
      y % 2 === 0,
      x % 3 === 0,
      (x + y) % 3 === 0,
      (Math.floor(x / 3) + Math.floor(y / 2)) % 2 === 0,
      ((x * y) % 2) + ((x * y) % 3) === 0,
      (((x * y) % 2) + ((x * y) % 3)) % 2 === 0,
      (((x + y) % 2) + ((x * y) % 3)) % 2 === 0,
    ][mask];
  const flip = (mask: number) => {
    for (let y = 0; y < size; y++) for (let x = 0; x < size; x++) if (!fixed[y][x] && masked(mask, x, y)) dark[y][x] = !dark[y][x];
  };
  const penalty = () => {
    let score = 0;
    for (let a = 0; a < size; a++) {
      let rowRun = 1;
      let colRun = 1;
      for (let b = 1; b < size; b++) {
        rowRun = dark[a][b] === dark[a][b - 1] ? rowRun + 1 : 1;
        colRun = dark[b][a] === dark[b - 1][a] ? colRun + 1 : 1;
        if (rowRun === 5) score += 3;
        else if (rowRun > 5) score++;
        if (colRun === 5) score += 3;
        else if (colRun > 5) score++;
      }
    }
    let lit = 0;
    for (let y = 0; y < size; y++) {
      for (let x = 0; x < size; x++) {
        if (dark[y][x]) lit++;
        if (x < size - 1 && y < size - 1 && dark[y][x] === dark[y][x + 1] && dark[y][x] === dark[y + 1][x] && dark[y][x] === dark[y + 1][x + 1]) score += 3;
      }
    }
    return score + Math.floor(Math.abs(lit * 20 - size * size * 10) / (size * size)) * 10;
  };
  let best = 0;
  let lowest = Infinity;
  for (let mask = 0; mask < 8; mask++) {
    flip(mask);
    format(mask);
    const score = penalty();
    if (score < lowest) {
      lowest = score;
      best = mask;
    }
    flip(mask);
  }
  flip(best);
  format(best);
  return dark;
}

export function qrSvg(text: string) {
  const dark = qr(text);
  const size = dark.length + 8;
  let path = "";
  dark.forEach((row, y) => row.forEach((on, x) => on && (path += `M${x + 4} ${y + 4}h1v1h-1z`)));
  return `<svg viewBox="0 0 ${size} ${size}" shape-rendering="crispEdges" role="img" aria-label="QR"><rect width="${size}" height="${size}" fill="#fff"/><path d="${path}" fill="#000"/></svg>`;
}
