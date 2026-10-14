"""Convierte la plantilla de instalador de Tauri en la de Cocuyo.

La plantilla original va dentro del CLI de Tauri. Este script la saca de ahí y le
aplica unos pocos cambios (cada página avisa a `dark.nsh` para que la pinte, se
quita el paso de elegir carpeta, y al terminar pasa sola a la página final), de
modo que al actualizar Tauri basta con volver a ejecutarlo:

    python make-template.py
"""

import glob
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))

def stock_template():
    found = glob.glob(os.path.join(ROOT, "node_modules", "@tauri-apps", "cli-win32-*", "*.node"))
    if not found:
        sys.exit("No encuentro el CLI de Tauri en node_modules: ejecuta `npm install` primero.")
    data = open(found[0], "rb").read()
    start = re.search(rb"Unicode true\r?\nManifestDPIAware true", data)
    if not start:
        sys.exit("Esta versión del CLI no trae la plantilla donde se esperaba.")
    end = start.start()
    while end < len(data) and (data[end] in (9, 10, 13) or 32 <= data[end] < 127 or data[end] >= 0xC2):
        end += 1
    text = data[start.start() : end].decode("utf-8", errors="replace")
    return text[: text.rfind("FunctionEnd") + len("FunctionEnd")] + "\n"

EDITS = [
    (
        "!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive\n!insertmacro MUI_PAGE_WELCOME",
        "!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive\n"
        "!define MUI_PAGE_CUSTOMFUNCTION_SHOW CocuyoWelcomeShow\n"
        "!define MUI_PAGE_CUSTOMFUNCTION_LEAVE CocuyoStopPulse\n"
        "!insertmacro MUI_PAGE_WELCOME",
    ),
    (
        "    ${NSD_SetFocus} $R2\n    nsDialogs::Show",
        "    ${NSD_SetFocus} $R2\n    StrCpy $0 $R4\n    Call CocuyoDarken\n    Call CocuyoTidy\n    nsDialogs::Show",
    ),
    (
        "!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive\n!insertmacro MUI_PAGE_DIRECTORY",
        "!define MUI_PAGE_CUSTOMFUNCTION_PRE Skip\n!insertmacro MUI_PAGE_DIRECTORY",
    ),
    (
        "; 7. Installation page\n!insertmacro MUI_PAGE_INSTFILES",
        "; 7. Installation page\n!define MUI_PAGE_CUSTOMFUNCTION_SHOW CocuyoInstShow\n!insertmacro MUI_PAGE_INSTFILES",
    ),
    ("!define MUI_FINISHPAGE_NOAUTOCLOSE\n", ""),
    (
        "!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive\n!insertmacro MUI_PAGE_FINISH",
        "!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive\n"
        "!define MUI_PAGE_CUSTOMFUNCTION_SHOW CocuyoFinishShow\n"
        "!define MUI_PAGE_CUSTOMFUNCTION_LEAVE CocuyoStopPulse\n"
        "!insertmacro MUI_PAGE_FINISH",
    ),
    (
        "{{#each language_files}}\n  !include \"{{this}}\"\n{{/each}}",
        "{{#each language_files}}\n  !include \"{{this}}\"\n{{/each}}\n!include \"${COCUYO_DIR}\\lang.nsh\"",
    ),
    (
        "  SendMessage $DeleteAppDataCheckbox ${WM_SETFONT} $1 1\nFunctionEnd",
        "  SendMessage $DeleteAppDataCheckbox ${WM_SETFONT} $1 1\n"
        '  FindWindow $0 "#32770" "" $HWNDPARENT\n  Call un.CocuyoDarken\n  Call un.CocuyoTidy\nFunctionEnd',
    ),
    (
        "; 2. Uninstalling Page\n!insertmacro MUI_UNPAGE_INSTFILES",
        "; 2. Uninstalling Page\n!define MUI_PAGE_CUSTOMFUNCTION_SHOW un.CocuyoInstShow\n!insertmacro MUI_UNPAGE_INSTFILES",
    ),
]

def main():
    text = stock_template().replace("\r\n", "\n")
    for old, new in EDITS:
        if text.count(old) != 1:
            sys.exit(f"La plantilla de Tauri cambió; este trozo ya no está una sola vez:\n{old}")
        text = text.replace(old, new)
    banner = "; Plantilla de Tauri con los cambios de Cocuyo. No se edita a mano: ver make-template.py.\n"
    with open(os.path.join(HERE, "installer.nsi"), "w", encoding="utf-8", newline="\n") as out:
        out.write(banner + text)
    print("installer.nsi:", len(text.splitlines()), "líneas,", len(EDITS), "cambios")

if __name__ == "__main__":
    main()
