#!/usr/bin/env python3
"""Génère le logo ZER0│GH0ST.

Le logo est **dérivé du banner de la TUI** (`src/tui/banner.rs`) : mêmes
glyphes 5×5, donc le logo et ce que l'agent affiche au démarrage ne peuvent
pas diverger. Tout est reproductible :

    python3 scripts/make-logo.py

Produit :
  - logo.svg      wordmark vectoriel (ZER0│GH0ST + « ghost in the shell »)
  - logo.png      le même wordmark, fond transparent (1280×… )
  - avatar.png    icône carrée 512×512 (avatar GitHub / favicon)
  - favicon.svg   version 32×32 simplifiée

Pourquoi un logo *généré* plutôt qu'un dessin : il reste net à toute taille,
pèse quelques kilo-octets, et suit automatiquement le banner si celui-ci change.
"""

from __future__ import annotations

import os
import xml.etree.ElementTree as ET

from PIL import Image, ImageDraw, ImageFilter, ImageFont

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# ── Glyphes 5×5 : identiques à src/tui/banner.rs ────────────────────────────
GLYPHS = {
    "A": [" ███ ", "█   █", "█████", "█   █", "█   █"],
    "G": [" ████", "█    ", "█ ███", "█   █", " ███ "],
    "E": ["█████", "█    ", "████ ", "█    ", "█████"],
    "N": ["█   █", "██  █", "█ █ █", "█  ██", "█   █"],
    "H": ["█   █", "█   █", "█████", "█   █", "█   █"],
    "S": [" ████", "█    ", " ███ ", "    █", "████ "],
    "T": ["█████", "  █  ", "  █  ", "  █  ", "  █  "],
    "Z": ["█████", "   █ ", "  █  ", " █   ", "█████"],
    "R": ["████ ", "█   █", "████ ", "█  █ ", "█   █"],
    "O": [" ███ ", "█   █", "█   █", "█   █", " ███ "],
    "0": [" ███ ", "█   █", "█   █", "█   █", " ███ "],
    "|": ["│", "│", "│", "│", "│"],
    " ": ["  ", "  ", "  ", "  ", "  "],
}
TEXT = "ZER0│GH0ST"
TAGLINE = "ghost in the shell"

CELL = 24          # taille d'un pixel du glyphe (en unités SVG)
GAP = 1            # espace entre caractères (en cellules)
ROWS = 5

# Palette : cyan → magenta (identique au site)
CYAN = (34, 211, 238)
MAGENTA = (217, 70, 239)
DARK = (10, 10, 18)


def text_width(text: str) -> int:
    return sum(len(GLYPHS.get(c, GLYPHS[" "])[0]) + GAP for c in text) - GAP


def gradient(t: float) -> tuple[int, int, int]:
    """Interpolation cyan → magenta."""
    return tuple(round(a + (b - a) * t) for a, b in zip(CYAN, MAGENTA))


def cells(text: str):
    """Yield (col, row, filled) pour chaque pixel plein du wordmark."""
    x = 0
    for ch in text:
        glyph = GLYPHS.get(ch, GLYPHS[" "])
        for r, line in enumerate(glyph):
            for c, px in enumerate(line):
                if px != " ":
                    yield x + c, r, True
        x += len(glyph[0]) + GAP
    return


# ── SVG ──────────────────────────────────────────────────────────────────────
def write_svg_wordmark(path: str) -> None:
    w = text_width(TEXT) * CELL
    h = ROWS * CELL
    pad = CELL
    total_w, total_h = w + 2 * pad, h + 2 * pad

    svg = ET.Element("svg", {
        "xmlns": "http://www.w3.org/2000/svg",
        "viewBox": f"0 0 {total_w} {total_h}",
        "width": str(total_w),
        "height": str(total_h),
        "role": "img",
        "aria-label": "ZER0 GH0ST — ghost in the shell",
    })
    defs = ET.SubElement(svg, "defs")
    grad = ET.SubElement(defs, "linearGradient", {
        "id": "g", "x1": "0", "y1": "0", "x2": "1", "y2": "0"})
    ET.SubElement(grad, "stop", {"offset": "0", "stop-color": "#22d3ee"})
    ET.SubElement(grad, "stop", {"offset": "1", "stop-color": "#d946ef"})

    cols = text_width(TEXT)
    for c, r, _ in cells(TEXT):
        ET.SubElement(svg, "rect", {
            "x": str(pad + c * CELL),
            "y": str(pad + r * CELL),
            "width": str(CELL),
            "height": str(CELL),
            "fill": "url(#g)",
        })

    # signature : le bloc est un « secteur » de la TUI
    ET.SubElement(svg, "rect", {
        "x": str(pad + (cols + 0.5) * CELL - CELL / 2),
        "y": str(pad),
        "width": str(CELL // 2),
        "height": str(h),
        "fill": "#22d3ee",
        "opacity": "0.55",
    })
    ET.ElementTree(svg).write(path, encoding="utf-8", xml_declaration=True)
    return total_w, total_h


def write_svg_avatar(path: str) -> None:
    """Icône carrée : le « 0 » de ZER0 dans un cadre néon."""
    s = 512
    svg = ET.Element("svg", {
        "xmlns": "http://www.w3.org/2000/svg",
        "viewBox": f"0 0 {s} {s}", "width": str(s), "height": str(s),
        "role": "img", "aria-label": "ZER0 GH0ST",
    })
    defs = ET.SubElement(svg, "defs")
    g = ET.SubElement(defs, "linearGradient", {"id": "g", "x1": "0", "y1": "0", "x2": "1", "y2": "1"})
    ET.SubElement(g, "stop", {"offset": "0", "stop-color": "#22d3ee"})
    ET.SubElement(g, "stop", {"offset": "1", "stop-color": "#d946ef"})
    ET.SubElement(svg, "rect", {"x": "0", "y": "0", "width": str(s), "height": str(s),
                                "rx": "96", "fill": "#0a0a12"})
    ET.SubElement(svg, "rect", {"x": "14", "y": "14", "width": str(s - 28), "height": str(s - 28),
                                "rx": "84", "fill": "none", "stroke": "url(#g)", "stroke-width": "10"})

    # glyphe « 0 » centré, en 5×5 cellules
    cell = 56
    glyph = GLYPHS["0"]
    gw = len(glyph[0]) * cell
    gh = ROWS * cell
    ox = (s - gw) // 2
    oy = (s - gh) // 2 - 12
    for r, line in enumerate(glyph):
        for c, px in enumerate(line):
            if px != " ":
                ET.SubElement(svg, "rect", {
                    "x": str(ox + c * cell), "y": str(oy + r * cell),
                    "width": str(cell), "height": str(cell), "fill": "url(#g)",
                })
    # le « fantôme » : deux yeux clairs dans le creux du 0
    eye = max(3, cell // 4)
    ey = oy + 1 * cell + cell // 3
    for ex in (ox + 1 * cell + cell * 2 // 5, ox + 3 * cell - cell * 2 // 5 - eye):
        ET.SubElement(svg, "rect", {"x": str(ex), "y": str(ey),
                                    "width": str(eye), "height": str(eye), "fill": "#e6faff"})
    ET.ElementTree(svg).write(path, encoding="utf-8", xml_declaration=True)


# ── PNG ──────────────────────────────────────────────────────────────────────
def render_png(path: str, scale: int = 4) -> None:
    """Wordmark PNG (fond transparent), avec la tagline."""
    cell = 8 * scale
    cols = text_width(TEXT)
    w = cols * cell + 2 * cell
    h = ROWS * cell + 2 * cell
    tagline_h = 0

    # on dessine d'abord le wordmark pour mesurer
    img = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    total = cols * cell
    for c, r, _ in cells(TEXT):
        t = (c * cell) / max(total - cell, 1)
        col = gradient(min(max(t, 0.0), 1.0))
        d.rectangle([cell + c * cell, cell + r * cell,
                     cell + (c + 1) * cell - 1, cell + (r + 1) * cell - 1], fill=col + (255,))
    img.save(path)
    return img


def render_avatar(path: str, size: int = 512) -> None:
    """Avatar carré : « 0 » néon sur fond sombre, halo propre."""
    img = Image.new("RGBA", (size, size), DARK + (255,))
    d = ImageDraw.Draw(img)

    cell = int(size * 56 / 512)
    glyph = GLYPHS["0"]
    gw, gh = len(glyph[0]) * cell, ROWS * cell
    ox, oy = (size - gw) // 2, (size - gh) // 2 - int(size * 0.02)

    # 1) halo
    glow = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    gd = ImageDraw.Draw(glow)
    for r, line in enumerate(glyph):
        for c, px in enumerate(line):
            if px != " ":
                gd.rectangle([ox + c * cell, oy + r * cell,
                              ox + (c + 1) * cell - 1, oy + (r + 1) * cell - 1], fill=CYAN + (170,))
    glow = glow.filter(ImageFilter.GaussianBlur(radius=size * 0.035))
    img = Image.alpha_composite(img, glow)
    d = ImageDraw.Draw(img)

    # 2) le « trou » du 0 est purgé du halo (sinon il bave et parait sale)
    d.rectangle([ox + 1 * cell, oy + 1 * cell, ox + 4 * cell - 1, oy + 4 * cell - 1],
                fill=DARK + (255,))

    # 3) le glyphe net, dégradé cyan → magenta
    for r, line in enumerate(glyph):
        for c, px in enumerate(line):
            if px != " ":
                t = (c * cell) / max(gw - cell, 1)
                d.rectangle([ox + c * cell, oy + r * cell,
                             ox + (c + 1) * cell - 1, oy + (r + 1) * cell - 1],
                            fill=gradient(t) + (255,))
    # 4) les deux yeux du fantôme (clairs : le creux du 0 est sombre)
    eye = max(2, int(cell * 0.30))
    ey = oy + 1 * cell + int(cell * 0.42)
    for ex in (ox + 1 * cell + int(cell * 0.40), ox + 3 * cell - int(cell * 0.40) - eye):
        d.rectangle([ex, ey, ex + eye, ey + eye], fill=(230, 250, 255, 255))
    # 5) cadre
    d.rounded_rectangle([14, 14, size - 15, size - 15], radius=int(size * 0.16),
                        outline=MAGENTA + (200,), width=max(2, size // 96))
    img.save(path)


def main() -> None:
    out = os.path.join(ROOT, "site")
    os.makedirs(out, exist_ok=True)

    w, h = write_svg_wordmark(os.path.join(ROOT, "logo.svg"))
    print(f"  logo.svg    {w}×{h}")
    write_svg_avatar(os.path.join(ROOT, "avatar.svg"))
    print("  avatar.svg  512×512")
    render_png(os.path.join(ROOT, "logo.png"))
    print("  logo.png    wordmark (fond transparent)")
    render_avatar(os.path.join(ROOT, "avatar.png"))
    print("  avatar.png  512×512")
    # copies pour le site
    import shutil
    for name in ("logo.svg", "logo.png", "avatar.svg", "avatar.png"):
        shutil.copy(os.path.join(ROOT, name), os.path.join(out, name))
    print("  → copies dans site/")


if __name__ == "__main__":
    main()
