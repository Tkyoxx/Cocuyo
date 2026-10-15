// la conexion cifrada con el pc: emparejar, saludar y mandar todo sellado. el punto de cita no ve nada

import { abrir, acordar, b64, bytes, deB64, deCodigo, deMarco, hmac, iguales, limpiarCodigo, llavesDeSesion, marco, nuevoPar, sellar, utf8 } from "./cifrado.js";

const PUNTO_DE_FABRICA = "";
const JSON_ = 1;
const AUDIO = 2;
const VOZ = 3;
const MAXIMO = 1_900_000;
const PING = '{"t":"ping"}';

const parametros = new URLSearchParams(location.search);
const ancla = new URLSearchParams(location.hash.slice(1));
if (ancla.has("codigo") || ancla.has("punto")) history.replaceState(null, "", location.pathname + location.search + (ancla.has("punto") ? `#punto=${encodeURIComponent(ancla.get("punto"))}` : ""));

function base() {
  return new Promise((listo, mal) => {
    const pedido = indexedDB.open("cocuyo", 1);
    pedido.onupgradeneeded = () => pedido.result.createObjectStore("llaves");
    pedido.onsuccess = () => listo(pedido.result);
    pedido.onerror = () => mal(pedido.error);
  });
}

async function guardado(cual, valor) {
  const db = await base();
  return new Promise((listo, mal) => {
    const tx = db.transaction("llaves", valor === undefined ? "readonly" : "readwrite");
    const almacen = tx.objectStore("llaves");
    const pedido = valor === undefined ? almacen.get(cual) : valor === null ? almacen.delete(cual) : almacen.put(valor, cual);
    tx.oncomplete = () => {
      db.close();
      listo(pedido.result);
    };
    tx.onerror = () => {
      db.close();
      mal(tx.error);
    };
  });
}

function puntoLimpio(punto) {
  const p = String(punto ?? "").trim().replace(/\/+$/, "");
  return /^wss?:\/\/[^\s/]+/.test(p) ? p : "";
}

async function nombreDelCelular() {
  const ua = navigator.userAgent;
  if (/iPhone/.test(ua)) return "iPhone";
  if (/iPad/.test(ua) || (navigator.platform === "MacIntel" && navigator.maxTouchPoints > 1)) return "iPad";
  try {
    const { model } = (await navigator.userAgentData?.getHighEntropyValues(["model"])) ?? {};
    if (model) return model.replace(/[\u0000-\u001f\u007f]/g, "").slice(0, 40);
  } catch {}
  return /Android/.test(ua) ? "Android" : "Celular";
}

function conectar(url) {
  return new Promise((listo, mal) => {
    const ws = new WebSocket(url);
    ws.binaryType = "arraybuffer";
    const fallo = () => mal(new Error("No llegué al punto de cita. Revisa tu conexión."));
    ws.addEventListener("open", () => {
      ws.removeEventListener("error", fallo);
      listo(ws);
    }, { once: true });
    ws.addEventListener("error", fallo, { once: true });
  });
}

function esperar(ws, quiero, segundos) {
  return new Promise((listo, mal) => {
    const reloj = setTimeout(() => terminar(new Error("Tu PC no contestó a tiempo. Pide un código nuevo.")), segundos * 1000);
    const terminar = (error, dato) => {
      clearTimeout(reloj);
      ws.removeEventListener("message", oir);
      ws.removeEventListener("close", cerrado);
      error ? mal(error) : listo(dato);
    };
    const oir = (e) => {
      if (typeof e.data !== "string") return;
      let dicho;
      try {
        dicho = JSON.parse(e.data);
      } catch {
        return;
      }
      if (dicho.t === "sin-pc") terminar(new Error("Ese código no está esperando en ningún PC. Revisa las letras o pide uno nuevo."));
      else if (dicho.t === quiero) terminar(null, dicho);
    };
    const cerrado = () => terminar(new Error("Se cortó la conexión mientras emparejábamos."));
    ws.addEventListener("message", oir);
    ws.addEventListener("close", cerrado);
  });
}

async function emparejar(codigo, punto) {
  const limpio = limpiarCodigo(codigo);
  if (!limpio) throw new Error("El código tiene nueve letras y números, como K7Q-2XM-94P.");
  if (!punto) throw new Error("Falta la dirección de tu punto de cita. Escanea con la cámara el QR que te muestra Cocuyo en tu PC.");
  const { sala, llave } = await deCodigo(limpio);
  const par = await nuevoPar();
  const nombre = await nombreDelCelular();
  const ws = await conectar(`${punto}/sala/${sala}?rol=cel`);
  try {
    const respuesta = esperar(ws, "listo", 30);
    const mac = await hmac(llave, bytes(utf8("hola"), par.publica, utf8(nombre)));
    ws.send(JSON.stringify({ t: "hola", cel: b64(par.publica), nombre, mac: b64(mac) }));
    const listo = await respuesta;
    const pc = deB64(listo.pc ?? "");
    const salaFija = String(listo.sala ?? "");
    const pcNombre = String(listo.nombre ?? "");
    const esperado = await hmac(llave, bytes(utf8("listo"), pc, par.publica, utf8(salaFija), utf8(pcNombre)));
    if (!iguales(esperado, deB64(listo.mac ?? "")) || pc.length !== 65 || !/^[0-9a-f]{32}$/.test(salaFija)) throw new Error("La respuesta no vino de tu PC. Empieza otra vez.");
    const datos = { celPrivada: par.privada, celPublica: par.publica, pcPublica: pc, sala: salaFija, nombre: pcNombre.slice(0, 60) || "tu PC", punto, desde: Date.now() };
    await guardado("pc", datos);
    void navigator.storage?.persist?.();
    return datos;
  } finally {
    ws.close(1000);
  }
}

class Enlace {
  constructor(datos, recibir, avisar) {
    this.datos = datos;
    this.recibir = recibir;
    this.avisar = avisar;
    this.ws = null;
    this.sesion = null;
    this.espera = 1000;
    this.reintento = null;
    this.latido = null;
    this.cola = Promise.resolve();
    this.entrada = Promise.resolve();
    this.cerrado = false;
    addEventListener("online", () => this.yaMismo());
    document.addEventListener("visibilitychange", () => {
      if (document.visibilityState === "visible") this.yaMismo();
      this.latir();
    });
    this.abrir();
  }

  yaMismo() {
    if (this.cerrado || (this.ws && this.ws.readyState <= WebSocket.OPEN)) return;
    clearTimeout(this.reintento);
    this.espera = 1000;
    this.abrir();
  }

  async abrir() {
    if (this.cerrado) return;
    this.avisar("buscando");
    let ws;
    try {
      ws = await conectar(`${this.datos.punto}/sala/${this.datos.sala}?rol=cel`);
    } catch {
      return this.luego();
    }
    if (this.cerrado) return ws.close(1000);
    this.ws = ws;
    ws.addEventListener("message", (e) => {
      this.entrada = this.entrada.then(() => this.mensaje(ws, e.data)).catch(() => {});
    });
    ws.addEventListener("close", () => {
      if (this.ws !== ws) return;
      this.ws = null;
      this.sesion = null;
      this.latir();
      this.luego();
    });
    this.espera = 1000;
    this.latir();
    await this.saludar(ws);
  }

  luego() {
    if (this.cerrado) return;
    this.avisar("buscando");
    clearTimeout(this.reintento);
    this.reintento = setTimeout(() => this.abrir(), this.espera);
    this.espera = Math.min(this.espera * 2, 30000);
  }

  latir() {
    clearInterval(this.latido);
    if (this.ws?.readyState === WebSocket.OPEN && document.visibilityState === "visible") {
      this.latido = setInterval(() => this.ws?.readyState === WebSocket.OPEN && this.ws.send(PING), 30000);
    }
  }

  async saludar(ws) {
    this.sesion = null;
    const efimera = await nuevoPar();
    this.saludo = efimera;
    if (ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify({ t: "inicio", e: b64(efimera.publica) }));
  }

  async mensaje(ws, data) {
    if (ws !== this.ws) return;
    if (typeof data === "string") {
      let dicho;
      try {
        dicho = JSON.parse(data);
      } catch {
        return;
      }
      if (dicho.t === "sin-pc") {
        if (!this.sesion) this.avisar("dormido");
      } else if (dicho.t === "pc-se-fue") {
        this.sesion = null;
        this.avisar("dormido");
      } else if (dicho.t === "pc-llego") {
        await this.saludar(ws);
      } else if (dicho.t === "acepto") {
        await this.aceptado(dicho);
      }
      return;
    }
    const pieza = deMarco(new Uint8Array(data));
    const sesion = this.sesion;
    if (!pieza || !sesion || pieza.contador <= sesion.ultimo) return;
    const plano = await abrir(sesion.entra, pieza.contador, pieza.sellado);
    if (!plano || this.sesion !== sesion || pieza.contador <= sesion.ultimo) return;
    sesion.ultimo = pieza.contador;
    const [tipo] = plano;
    const cuerpo = plano.subarray(1);
    if (tipo === JSON_) {
      try {
        this.recibir(JSON.parse(new TextDecoder().decode(cuerpo)));
      } catch {}
    } else if (tipo === VOZ) {
      this.recibir({ tipo: "voz", wav: cuerpo.slice() });
    }
  }

  async aceptado(dicho) {
    const efimera = this.saludo;
    if (!efimera) return;
    const efPc = deB64(dicho.e ?? "");
    const { celPrivada, celPublica, pcPublica } = this.datos;
    let llaves;
    try {
      const [dh1, dh2, dh3] = await Promise.all([acordar(efimera.privada, efPc), acordar(efimera.privada, pcPublica), acordar(celPrivada, efPc)]);
      llaves = await llavesDeSesion({ celPublica, pcPublica, efCel: efimera.publica, efPc, dh1, dh2, dh3 });
    } catch {
      return;
    }
    const prueba = await abrir(llaves.pcACel, 0, deB64(dicho.prueba ?? ""));
    if (!prueba || new TextDecoder().decode(prueba) !== "acepto") return;
    this.saludo = null;
    this.sesion = { sale: llaves.celAPc, entra: llaves.pcACel, siguiente: 1, ultimo: 0 };
    this.avisar("conectado");
  }

  enviar(mensaje) {
    const turno = this.cola.then(() => this.sellarYMandar(mensaje));
    this.cola = turno.catch(() => {});
    return turno;
  }

  async sellarYMandar(mensaje) {
    const sesion = this.sesion;
    const ws = this.ws;
    if (!sesion || ws?.readyState !== WebSocket.OPEN) throw new Error(`${this.datos.nombre} no está conectado ahora. Revisa que esté prendido con Cocuyo abierto.`);
    const cuerpo = mensaje.tipo === "voz" ? bytes(Uint8Array.of(AUDIO), mensaje.pcm) : bytes(Uint8Array.of(JSON_), utf8(JSON.stringify(mensaje)));
    if (cuerpo.length > MAXIMO) throw new Error("Eso es muy largo para mandarlo de una. Habla menos de un minuto.");
    const contador = sesion.siguiente++;
    ws.send(marco(contador, await sellar(sesion.sale, contador, cuerpo)));
  }

  cerrar() {
    this.cerrado = true;
    clearTimeout(this.reintento);
    clearInterval(this.latido);
    this.ws?.close(1000);
  }
}

export async function abrirEnlace(recibir, avisar) {
  let datos = null;
  try {
    datos = await guardado("pc");
  } catch {}
  const punto = puntoLimpio(ancla.get("punto")) || puntoLimpio(parametros.get("punto")) || puntoLimpio(datos?.punto) || puntoLimpio(PUNTO_DE_FABRICA);
  const olvidar = async () => {
    enlace?.cerrar();
    await guardado("pc", null);
  };
  let enlace = null;
  if (datos?.celPrivada && datos.pcPublica && datos.sala && datos.punto) {
    enlace = new Enlace(datos, recibir, (cual) => avisar(cual, datos.nombre));
    return { emparejado: true, nombre: datos.nombre, enlace, olvidar };
  }
  return {
    emparejado: false,
    sinPunto: !punto,
    codigo: limpiarCodigo(ancla.get("codigo") ?? "") ?? "",
    olvidar,
    async emparejar(codigo) {
      const nuevos = await emparejar(codigo, punto);
      enlace = new Enlace(nuevos, recibir, (cual) => avisar(cual, nuevos.nombre));
      return { nombre: nuevos.nombre, enlace };
    },
  };
}
