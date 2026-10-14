"""Mete en la galería el script que viene en un issue aprobado.

Lo llama .github/workflows/galeria.yml. Todo lo que llega del issue es texto de un
desconocido: se valida, se escribe a un archivo y ya. Aquí no se ejecuta nada.
"""
import json
import os
import re
import sys

TIPOS = ("ps1", "py", "bat", "cmd", "sh")
GALERIA = "galeria"


def fuera(motivo):
    print(motivo)
    with open(os.environ.get("GITHUB_OUTPUT", os.devnull), "a", encoding="utf-8") as salida:
        salida.write("error<<FIN\n" + motivo + "\nFIN\n")
    sys.exit(1)


def campo(cuerpo, titulo):
    hallado = re.search(r"^### " + re.escape(titulo) + r"\s*\n(.*?)(?=^### |\Z)", cuerpo, re.S | re.M)
    return hallado.group(1).strip() if hallado else ""


def nombre_valido(archivo):
    return (
        0 < len(archivo.encode("utf-8")) <= 80
        and archivo.rsplit(".", 1)[-1].lower() in TIPOS
        and "." in archivo
        and not archivo.startswith(".")
        and ".." not in archivo
        and all(letra.isalnum() or letra in " -_." for letra in archivo)
    )


cuerpo = os.environ["CUERPO"].replace("\r\n", "\n")
autor = os.environ["AUTOR"]
archivo = campo(cuerpo, "Nombre del archivo")
que_hace = " ".join(campo(cuerpo, "Qué hace").split())[:160]
# El script puede traer sus propios "###": va desde su título hasta la última casilla del formulario.
desde = cuerpo.find("### El script\n")
hasta = cuerpo.rfind("\n### Antes de mandarlo")
bloque = cuerpo[desde + len("### El script\n") : hasta if hasta > desde else len(cuerpo)].strip() if desde >= 0 else ""
codigo = re.search(r"^```[a-z]*\n(.*)\n```\s*$", bloque, re.S)
codigo = (codigo.group(1) if codigo else bloque).strip("\n") + "\n"

if not re.fullmatch(r"[A-Za-z0-9-]{1,39}", autor):
    fuera("El nombre de quien lo manda es raro. No se publica.")
if not nombre_valido(archivo):
    fuera("El nombre del archivo solo puede llevar letras, números, espacios y guiones, y terminar en " + ", ".join("." + t for t in TIPOS) + ".")
if not que_hace:
    fuera("Falta decir qué hace.")
if not codigo.strip() or len(codigo.encode("utf-8")) > 20000:
    fuera("El script está vacío o pesa más de 20 KB.")

indice_en = os.path.join(GALERIA, "indice.json")
with open(indice_en, encoding="utf-8") as leido:
    indice = json.load(leido)

anterior = next((entrada for entrada in indice if entrada["file"].lower() == archivo.lower()), None)
if anterior and anterior.get("author", "").lower() != autor.lower():
    fuera("Ya hay un script con ese nombre y es de otra persona. Ponle otro nombre.")

with open(os.path.join(GALERIA, archivo), "w", encoding="utf-8", newline="\n") as escrito:
    escrito.write(codigo)

entrada = {"name": archivo.rsplit(".", 1)[0].replace("_", " ").replace("-", " ").strip(), "about": que_hace, "file": archivo, "author": autor}
if anterior:
    indice[indice.index(anterior)] = entrada
else:
    indice.append(entrada)
with open(indice_en, "w", encoding="utf-8", newline="\n") as escrito:
    escrito.write(json.dumps(indice, ensure_ascii=False, indent=2) + "\n")

with open(os.environ.get("GITHUB_OUTPUT", os.devnull), "a", encoding="utf-8") as salida:
    salida.write("archivo=" + archivo + "\n")
print("publicado:", archivo, "de", autor)
