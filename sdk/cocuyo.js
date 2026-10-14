// sdk de cocuyo para node: const cocuyo = require("cocuyo")

const fs = require("fs");
const path = require("path");

const env = process.env;
const carpeta = env.COCUYO_CARPETA || ".";

function enviar(datos) {
  process.stdout.write("@cocuyo " + JSON.stringify(datos) + "\n");
}

const archivo = () => path.join(carpeta, "datos.json");
const todo = () => {
  try {
    return JSON.parse(fs.readFileSync(archivo(), "utf8"));
  } catch {
    return {};
  }
};

module.exports = {
  dato: env.COCUYO_DATO || "",
  frase: env.COCUYO_FRASE || "",
  respuesta: env.COCUYO_RESPUESTA || "",
  idioma: env.COCUYO_IDIOMA || "es",
  nombre: env.COCUYO_NOMBRE || "",
  ciudad: env.COCUYO_CIUDAD || "",
  carpeta,
  conversacion: env.COCUYO_CONVERSACION || "",
  version: env.COCUYO_VERSION || "",
  enviar,
  decir: (texto) => enviar({ decir: texto }),
  hablar: (texto) => enviar({ decir: texto, hablar: true }),
  preguntar: (pregunta) => enviar({ preguntar: pregunta }),
  progreso: (valor, texto = "") => enviar({ progreso: valor, texto }),
  copiar: (texto) => enviar({ copiar: texto }),
  abrir: (destino) => enviar({ abrir: destino }),
  escribir: (texto) => enviar({ escribir: texto }),
  recordar: (dato) => enviar({ recordar: dato }),
  programar: (cuando, orden) => enviar({ programar: { cuando, orden } }),
  tarjeta: (tarjeta) => enviar({ tarjeta }),
  boton: (texto, accion = {}) => ({ texto, ...accion }),
  fila: (texto, valor = "", barra) => (barra === undefined ? { texto, valor } : { texto, valor, barra }),
  leer: (clave, porDefecto = null) => (clave in todo() ? todo()[clave] : porDefecto),
  guardar: (clave, valor) => fs.writeFileSync(archivo(), JSON.stringify({ ...todo(), [clave]: valor }, null, 2)),
};
