// el candado del lado del celular: lo mismo que candado.rs, con el webcrypto del navegador

const sutil = globalThis.crypto.subtle;
const texto = new TextEncoder();
export const AAD = texto.encode("cocuyo-remoto-1");
const LETRAS = "23456789ABCDEFGHJKMNPQRSTUVWXYZ";

export const bytes = (...partes) => {
  const total = partes.reduce((n, p) => n + p.length, 0);
  const out = new Uint8Array(total);
  let i = 0;
  for (const p of partes) {
    out.set(p, i);
    i += p.length;
  }
  return out;
};

export const utf8 = (t) => texto.encode(t);

export const b64 = (data) => {
  let s = "";
  for (const b of data) s += String.fromCharCode(b);
  return btoa(s).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
};

export const deB64 = (t) => {
  const s = atob(String(t).replace(/-/g, "+").replace(/_/g, "/"));
  return Uint8Array.from(s, (c) => c.charCodeAt(0));
};

export const hex = (data) => [...data].map((b) => b.toString(16).padStart(2, "0")).join("");

export function limpiarCodigo(codigo) {
  const limpio = String(codigo).toUpperCase().replace(/[\s-]/g, "");
  if (limpio.length !== 9 || [...limpio].some((c) => !LETRAS.includes(c))) return null;
  return limpio;
}

export async function sha256(data) {
  return new Uint8Array(await sutil.digest("SHA-256", data));
}

export async function hmac(llave, data) {
  const k = await sutil.importKey("raw", llave, { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
  return new Uint8Array(await sutil.sign("HMAC", k, data));
}

export async function hkdf(ikm, salt, info, largo) {
  const k = await sutil.importKey("raw", ikm, "HKDF", false, ["deriveBits"]);
  return new Uint8Array(await sutil.deriveBits({ name: "HKDF", hash: "SHA-256", salt, info }, k, largo * 8));
}

export async function pbkdf2(clave, salt, vueltas, largo) {
  const k = await sutil.importKey("raw", clave, "PBKDF2", false, ["deriveBits"]);
  return new Uint8Array(await sutil.deriveBits({ name: "PBKDF2", hash: "SHA-256", salt, iterations: vueltas }, k, largo * 8));
}

export async function nuevoPar() {
  const par = await sutil.generateKey({ name: "ECDH", namedCurve: "P-256" }, false, ["deriveBits"]);
  const publica = new Uint8Array(await sutil.exportKey("raw", par.publicKey));
  return { privada: par.privateKey, publica };
}

export async function acordar(privada, suPublica) {
  if (suPublica.length !== 65 || suPublica[0] !== 4) throw new Error("llave del otro lado mal formada");
  const otra = await sutil.importKey("raw", suPublica, { name: "ECDH", namedCurve: "P-256" }, false, []);
  return new Uint8Array(await sutil.deriveBits({ name: "ECDH", public: otra }, privada, 256));
}

const nonce = (contador) => {
  const n = new Uint8Array(12);
  new DataView(n.buffer).setBigUint64(4, BigInt(contador));
  return n;
};

export async function sellar(llave, contador, plano) {
  const k = await sutil.importKey("raw", llave, "AES-GCM", false, ["encrypt"]);
  return new Uint8Array(await sutil.encrypt({ name: "AES-GCM", iv: nonce(contador), additionalData: AAD, tagLength: 128 }, k, plano));
}

export async function abrir(llave, contador, sellado) {
  const k = await sutil.importKey("raw", llave, "AES-GCM", false, ["decrypt"]);
  try {
    return new Uint8Array(await sutil.decrypt({ name: "AES-GCM", iv: nonce(contador), additionalData: AAD, tagLength: 128 }, k, sellado));
  } catch {
    return null;
  }
}

export function iguales(a, b) {
  if (a.length !== b.length) return false;
  let d = 0;
  for (let i = 0; i < a.length; i++) d |= a[i] ^ b[i];
  return d === 0;
}

// el codigo de nueve letras se vuelve la sala temporal y la llave del saludo. caro a proposito para que adivinarlo no salga a cuenta
export async function deCodigo(codigo) {
  const base = await pbkdf2(utf8(codigo), utf8("cocuyo-emparejar-1"), 150000, 64);
  return { sala: hex(base.slice(0, 16)), llave: base.slice(32, 64) };
}

// las llaves de una sesion: tres acuerdos, como noise kk
export async function llavesDeSesion({ celPublica, pcPublica, efCel, efPc, dh1, dh2, dh3 }) {
  const sal = await sha256(bytes(celPublica, pcPublica, efCel, efPc));
  const todo = await hkdf(bytes(dh1, dh2, dh3), sal, utf8("cocuyo-sesion-1"), 64);
  return { celAPc: todo.slice(0, 32), pcACel: todo.slice(32, 64) };
}

export function marco(contador, sellado) {
  const out = new Uint8Array(8 + sellado.length);
  new DataView(out.buffer).setBigUint64(0, BigInt(contador));
  out.set(sellado, 8);
  return out;
}

export function deMarco(data) {
  if (data.length < 8 + 16) return null;
  const contador = Number(new DataView(data.buffer, data.byteOffset, 8).getBigUint64(0));
  return { contador, sellado: data.slice(8) };
}
