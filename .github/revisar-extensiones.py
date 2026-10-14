# mira que cada extension de la galeria diga lo que usa, y que el indice diga lo mismo

import json
import pathlib
import re
import sys

GALERIA = pathlib.Path(__file__).resolve().parent.parent / "galeria" / "extensiones"
PERMISOS = {"red", "portapapeles", "pantalla", "voz", "equipo", "datos", "scripts", "agente", "escritorio", "comandos"}

COMANDOS = [
    (r"^(clip_list|clip_use|clip_clear|ocr_clip)$", "portapapeles"),
    (r"^(screens_list|lasso_ask|capture_commit|capture_discard)$", "pantalla"),
    (r"^(system_stats|net_speed|dns_measure|program_info|app_icon|apps_list|media_now|volume_get|mic_muted|pad_status|version|taskbar_state|wallpapers_list|wallpaper_thumb|voice_status)$", "equipo"),
    (r"^(get_settings|agenda_list|memory_list|todos_get|weather|weather_city|spotify_status|scripts_list)$", "datos"),
    (r"^(script_run|script_action)$", "scripts"),
]

USOS = [
    (r"\bfetch\s*\(|XMLHttpRequest|WebSocket|EventSource|sendBeacon", "red"),
    (r"navigator\.clipboard", "portapapeles"),
    (r"cocuyo\.herramienta\s*\(", "agente"),
    (r"cocuyo\.flotante\s*\(", "escritorio"),
    (r"cocuyo\.ejecutar\s*\(", "scripts"),
    (r"cocuyo\.ajustes\s*\(", "datos"),
    (r"cocuyo\.al\s*\(\s*[\"']oido[\"']", "voz"),
    (r"cocuyo\.al\s*\(\s*[\"']agenda[\"']", "datos"),
    (r"cocuyo\.al\s*\(\s*[\"']spotify[\"']", "equipo"),
    (r"__TAURI|window\.__", "comandos"),
]


def pide(codigo):
    usa = {permiso for patron, permiso in USOS if re.search(patron, codigo)}
    for comando in re.findall(r"cocuyo\.invocar\s*\(\s*[\"'`]([\w-]+)", codigo):
        usa.add(next((permiso for patron, permiso in COMANDOS if re.match(patron, comando)), "comandos"))
    if re.search(r"cocuyo\.invocar\s*\(\s*[^\"'`\s]", codigo):
        usa.add("comandos")
    return usa


def main():
    fallas = []
    indice = {item["id"]: item for item in json.loads((GALERIA / "indice.json").read_text(encoding="utf-8"))}
    for carpeta in sorted(p for p in GALERIA.iterdir() if p.is_dir()):
        nombre = carpeta.name
        manifest = json.loads((carpeta / "manifest.json").read_text(encoding="utf-8"))
        entrada = indice.get(nombre)
        if entrada is None:
            fallas.append(f"{nombre}: no está en indice.json")
            continue
        if manifest.get("version") != entrada.get("version"):
            fallas.append(f"{nombre}: la versión del manifest ({manifest.get('version')}) y la del índice ({entrada.get('version')}) no coinciden")
        if manifest.get("tipo") == "tema":
            if (carpeta / "index.js").exists() or manifest.get("codigo"):
                fallas.append(f"{nombre}: un tema no lleva código")
            continue
        declarados = manifest.get("permisos")
        if declarados is None:
            fallas.append(f"{nombre}: el manifest no dice sus permisos (pon \"permisos\": [] si no pide nada)")
            continue
        raros = set(declarados) - PERMISOS
        if raros:
            fallas.append(f"{nombre}: permisos que no existen: {', '.join(sorted(raros))}")
        if sorted(declarados) != sorted(entrada.get("permisos") or []):
            fallas.append(f"{nombre}: los permisos del índice no son los del manifest")
        codigo = "".join(p.read_text(encoding="utf-8") for p in carpeta.glob("*.js"))
        faltan = pide(codigo) - set(declarados) - ({"red", "portapapeles", "pantalla", "voz", "equipo", "datos", "scripts", "agente", "escritorio"} if "comandos" in declarados else set())
        if faltan:
            fallas.append(f"{nombre}: el código usa {', '.join(sorted(faltan))} y no lo declara")
        sobran = set(declarados) - pide(codigo) - {"red"}
        if sobran:
            print(f"aviso, {nombre}: declara {', '.join(sorted(sobran))} y no veo dónde lo usa")
    for falla in fallas:
        print(f"::error::{falla}")
    if fallas:
        sys.exit(1)
    print(f"{len(indice)} extensiones y temas en orden")


main()
