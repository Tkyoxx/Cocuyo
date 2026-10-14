"""Dibuja el arte del instalador: la noche negra de la isla y sus cocuyos.

Genera, junto a este archivo:
  sidebar.bmp      164x314  la imagen fija de bienvenida y final
  glow-00..15.bmp  164x314  los cuadros de la animación (un parpadeo completo)
  header.bmp       150x57   la cabecera de las páginas interiores

    python make-art.py
"""

import math
import os

from PIL import Image, ImageChops, ImageDraw, ImageFont

HERE = os.path.dirname(os.path.abspath(__file__))
FRAMES = 16
AMBER = (242, 184, 75)
INK = (244, 245, 248)

FLIES = [
    (46, 62, 30, 0.00, 5),
    (118, 108, 20, 0.35, 4),
    (70, 150, 13, 0.62, 3),
    (132, 44, 10, 0.80, 3),
    (26, 124, 9, 0.18, 2),
    (104, 190, 16, 0.50, 4),
    (150, 160, 7, 0.92, 2),
]

def font(size, bold=True):
    for name in (("seguisb.ttf", "segoeuib.ttf") if bold else ("segoeui.ttf",)):
        try:
            return ImageFont.truetype(os.path.join(os.environ.get("WINDIR", r"C:\Windows"), "Fonts", name), size)
        except OSError:
            continue
    return ImageFont.load_default()

def halo(radius, strength):
    """Un halo ámbar con el centro casi blanco, listo para sumarse al fondo."""
    size = radius * 2
    falloff = Image.radial_gradient("L").resize((size, size), Image.BILINEAR)
    edge = max(1, falloff.getpixel((radius, 1)))
    near = lambda v: max(0.0, 1 - v / edge)
    glow = falloff.point(lambda v: int(255 * near(v) ** 2.2 * strength))
    core = falloff.point(lambda v: int(255 * max(0.0, 1 - v / (edge * 0.16)) ** 2 * strength))
    amber = Image.merge("RGB", [glow.point(lambda v, c=c: v * c // 255) for c in AMBER])
    white = Image.merge("RGB", [core, core, core.point(lambda v: v * 200 // 255)])
    return ImageChops.add(amber, white)

def night(width, height):
    """Negro arriba, con un rescoldo muy tenue subiendo desde abajo."""
    image = Image.new("RGB", (width, height))
    draw = ImageDraw.Draw(image)
    for y in range(height):
        warmth = max(0.0, (y - height * 0.45) / (height * 0.55)) ** 2
        draw.line([(0, y), (width, y)], fill=(int(22 * warmth), int(13 * warmth), int(4 * warmth)))
    return image

def sidebar(t):
    """Un cuadro de la barra lateral; `t` va de 0 a 1 y vuelve al principio."""
    image = night(164, 314)
    for x, y, radius, phase, wander in FLIES:
        angle = 2 * math.pi * (t + phase)
        pulse = 0.22 + 0.78 * max(0.0, math.sin(angle)) ** 1.6
        cx = x + wander * math.cos(angle)
        cy = y + wander * 0.6 * math.sin(2 * angle)
        layer = Image.new("RGB", image.size)
        layer.paste(halo(radius, pulse), (round(cx - radius), round(cy - radius)))
        image = ImageChops.add(image, layer)

    draw = ImageDraw.Draw(image)
    draw.text((18, 262), "Cocuyo", font=font(27), fill=INK)
    return image

def header():
    image = Image.new("RGB", (150, 57))
    layer = Image.new("RGB", image.size)
    layer.paste(halo(15, 0.9), (4, 13))
    image = ImageChops.add(image, layer)
    ImageDraw.Draw(image).text((40, 15), "Cocuyo", font=font(19), fill=INK)
    return image

def main():
    for index in range(FRAMES):
        sidebar(index / FRAMES).save(os.path.join(HERE, f"glow-{index:02d}.bmp"))
    sidebar(0.25).save(os.path.join(HERE, "sidebar.bmp"))
    header().save(os.path.join(HERE, "header.bmp"))
    print("listo:", FRAMES + 2, "imágenes en", HERE)

if __name__ == "__main__":
    main()
