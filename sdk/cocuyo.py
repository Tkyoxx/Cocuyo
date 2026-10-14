# sdk de cocuyo para python: import cocuyo y listo, cocuyo ya deja la carpeta en el path

import json
import os
import sys

dato = os.environ.get("COCUYO_DATO", "")
frase = os.environ.get("COCUYO_FRASE", "")
respuesta = os.environ.get("COCUYO_RESPUESTA", "")
idioma = os.environ.get("COCUYO_IDIOMA", "es")
nombre = os.environ.get("COCUYO_NOMBRE", "")
ciudad = os.environ.get("COCUYO_CIUDAD", "")
carpeta = os.environ.get("COCUYO_CARPETA", ".")
conversacion = os.environ.get("COCUYO_CONVERSACION", "")
version = os.environ.get("COCUYO_VERSION", "")


def enviar(datos):
    print("@cocuyo " + json.dumps(datos, ensure_ascii=False), flush=True)


def decir(texto):
    enviar({"decir": texto})


def hablar(texto):
    enviar({"decir": texto, "hablar": True})


def preguntar(pregunta):
    enviar({"preguntar": pregunta})


def progreso(valor, texto=""):
    enviar({"progreso": valor, "texto": texto})


def copiar(texto):
    enviar({"copiar": texto})


def abrir(destino):
    enviar({"abrir": destino})


def escribir(texto):
    enviar({"escribir": texto})


def recordar(dato_nuevo):
    enviar({"recordar": dato_nuevo})


def programar(cuando, orden):
    enviar({"programar": {"cuando": cuando, "orden": orden}})


def boton(texto, abrir=None, ejecutar=None, dato=None, copiar=None):
    hecho = {"texto": texto}
    for clave, valor in (("abrir", abrir), ("ejecutar", ejecutar), ("dato", dato), ("copiar", copiar)):
        if valor:
            hecho[clave] = valor
    return hecho


def fila(texto, valor="", barra=None):
    hecho = {"texto": texto, "valor": valor}
    if barra is not None:
        hecho["barra"] = barra
    return hecho


def tarjeta(titulo, texto="", imagen="", icono="", color="", botones=(), filas=(), segundos=0):
    hecho = {"titulo": titulo}
    for clave, valor in (("texto", texto), ("imagen", imagen), ("icono", icono), ("color", color), ("segundos", segundos)):
        if valor:
            hecho[clave] = valor
    if botones:
        hecho["botones"] = list(botones)
    if filas:
        hecho["filas"] = list(filas)
    enviar({"tarjeta": hecho})


def _archivo():
    return os.path.join(carpeta, "datos.json")


def leer(clave, por_defecto=None):
    try:
        with open(_archivo(), encoding="utf-8") as leido:
            return json.load(leido).get(clave, por_defecto)
    except (OSError, ValueError):
        return por_defecto


def guardar(clave, valor):
    try:
        with open(_archivo(), encoding="utf-8") as leido:
            todo = json.load(leido)
    except (OSError, ValueError):
        todo = {}
    todo[clave] = valor
    with open(_archivo(), "w", encoding="utf-8") as escrito:
        json.dump(todo, escrito, ensure_ascii=False, indent=2)


if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8")
