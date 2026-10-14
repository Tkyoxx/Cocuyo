# Galería de scripts

Cada archivo de esta carpeta es una orden nueva para Cocuyo. Se instalan desde **Ajustes → IA → Galería**, y ahí sale quién subió cada uno, con su foto de GitHub.

## Subir el tuyo

Lo fácil: en Cocuyo, **Ajustes → IA → Galería → Subir el mío**. Eliges uno de tus scripts y se abre GitHub con todo ya escrito; solo queda darle a *Submit new issue*.

También a mano: [abre un issue con el formulario](https://github.com/Tkyoxx/Cocuyo/issues/new?template=script.yml).

De las dos formas hace falta una cuenta de GitHub, y es a propósito: el script sale con tu nombre y tu foto, y así nadie sube cosas raras a escondidas. Cuando lo apruebo, un bot lo mete aquí a tu nombre y te cierra el issue.

## Cómo se escribe uno

- Vale `.ps1`, `.py`, `.bat`, `.cmd` o `.sh`.
- El nombre del archivo es lo que se dice: `apaga la pantalla.ps1` se ejecuta con «ejecuta apaga la pantalla».
- La primera línea es un comentario que cuenta para qué sirve. Es lo que se ve en la galería.
- Lo último que el script escribe es lo que Cocuyo responde.
- Lo que digas después del nombre llega como primer argumento: «ejecuta elige pizza o hamburguesa».

## Lo que no entra

- Nada que borre archivos, apague el equipo o toque el registro sin que quede clarísimo en el nombre.
- Nada que mande datos de nadie a otro sitio.
- Nada que descargue y ejecute otra cosa.

Los scripts son programas. Los reviso antes de que entren, pero igual lee lo que instalas.

## Para quien mantiene esto

Un issue de script se aprueba comentando `/aprobar` (solo cuenta si lo escribe el dueño del repo o un colaborador). El resto lo hace `.github/workflows/galeria.yml`: valida el nombre, guarda el archivo, lo apunta en `indice.json` con el usuario que abrió el issue y cierra el issue. Lee el script **justo antes** de aprobar: se publica lo que el issue diga en ese momento.
