#!/usr/bin/env python3
"""Aperçu GIF de l'animation « baleine deepseek » — prototype du rendu ratatui.

Tout est dessiné en cellules de terminal : la baleine est un **silhouette
rasterisée en demi-blocs** (▀ ▄ █), ce qui permet de la faire pivoter (montée /
apex / plongée) tout en restant dans une grille monospace. Le GIF montre donc
exactement ce que ratatui affichera.

Usage : python3 tools/whale_preview.py [sortie.gif]
"""

from __future__ import annotations

import math
import os
import sys

from PIL import Image, ImageDraw, ImageFont

# Police SANS ligatures (FiraCode transforme `..-'` et `'''` en glyphes).
FONT_PATHS = [
    "/usr/share/fonts/liberation/LiberationMono-Regular.ttf",
    "/usr/share/fonts/noto/NotoSansMono-Light.ttf",
]
FONT_SIZE = 16
CELL_W, CELL_H = 10, 19

COLS, ROWS = 78, 26
BG = (11, 13, 18)
DIM = (74, 85, 104)
TEXT = (196, 205, 219)
SEA = (46, 134, 194)
DEEP = (4, 13, 24)
FOAM = (176, 240, 255)
BODY = (231, 240, 250)
BELLY = (168, 199, 226)
FIN = (140, 176, 208)
EYE = (14, 20, 30)
TITLE_FG = (122, 202, 242)

FRAMES = 54
FRAME_MS = 60
DURATION = FRAMES * FRAME_MS / 1000.0

OT, OL, OW, OH = 5, 7, 64, 16
SEA_Y = OT + 10
FLOOR = OT + OH - 1

# Baleine : longueur 26 cellules, hauteur ~7 (aspect 2:1 d'un caractère).
WHALE_COLS = 22
WHALE_ROWS = 6
SS = 2          # sous-lignes par cellule (demi-blocs)


def lerp(a, b, t):
    t = max(0.0, min(1.0, t))
    return tuple(int(round(x + (y - x) * t)) for x, y in zip(a, b))


class Grid:
    def __init__(self):
        self.cells = [[(" ", None) for _ in range(COLS)] for _ in range(ROWS)]

    def put(self, x, y, ch, color):
        x, y = int(round(x)), int(round(y))
        if 0 <= x < COLS and 0 <= y < ROWS:
            self.cells[y][x] = (ch, color)

    def text(self, x, y, s, color):
        for i, ch in enumerate(s):
            self.put(int(x) + i, int(y), ch, color)


# ── Silhouette de baleine ────────────────────────────────────────────────────
# Coords locales : x de 0 (pointe de la caudale) à 1 (bout du museau),
# y de 0 (dos) à 1 (ventre). Deux profils interpolés = un cétacé lisible.
TOP = [(0.00, 0.42), (0.10, 0.39), (0.20, 0.32), (0.35, 0.25), (0.52, 0.21),
       (0.68, 0.22), (0.80, 0.26), (0.90, 0.34), (0.96, 0.39), (1.00, 0.47)]
BOT = [(0.00, 0.46), (0.12, 0.50), (0.28, 0.56), (0.46, 0.60), (0.62, 0.61),
       (0.76, 0.59), (0.88, 0.53), (0.96, 0.48), (1.00, 0.47)]


def _interp(pts, x):
    if x <= pts[0][0]:
        return pts[0][1]
    for (x0, y0), (x1, y1) in zip(pts, pts[1:]):
        if x <= x1:
            t = (x - x0) / (x1 - x0) if x1 > x0 else 0.0
            return y0 + (y1 - y0) * t
    return pts[-1][1]


def whale_shape(x: float, y: float) -> int:
    """0 = vide, 1 = dos, 2 = ventre, 3 = caudale, 4 = pectorale, 5 = dorsale."""
    # corps
    if 0.06 <= x <= 1.0:
        if _interp(TOP, x) <= y <= _interp(BOT, x):
            return 2 if y > _interp(BOT, x) - 0.13 else 1
    # caudale : éventail vertical fourchu à gauche
    if x <= 0.17:
        spread = 0.26 * (1.0 - x / 0.17) + 0.03
        d = abs(y - 0.44)
        if d <= spread and not (x < 0.09 and d < 0.07):
            return 3
    # dorsale : petit aileron sur le dos
    if 0.42 <= x <= 0.54:
        t = (x - 0.42) / 0.12
        top = _interp(TOP, x)
        if top - (0.14 - 0.10 * abs(t - 0.5) * 2) <= y <= top:
            return 5
    # pectorale : triangle sous le ventre
    if 0.58 <= x <= 0.74:
        t = (0.74 - x) / 0.16
        bot = _interp(BOT, x)
        if bot <= y <= bot + 0.18 * t:
            return 4
    return 0


def rasterize_whale(pitch: float) -> list[list[tuple[str, int]]]:
    """Rasterise la baleine pivotée de `pitch` (radians) en demi-blocs."""
    sub_h = WHALE_ROWS * SS
    glyphs = [[(" ", 0) for _ in range(WHALE_COLS)] for _ in range(WHALE_ROWS)]
    cos_p, sin_p = math.cos(-pitch), math.sin(-pitch)
    for row in range(WHALE_ROWS):
        for col in range(WHALE_COLS):
            k = [0, 0]
            for half in range(SS):
                sy = row * SS + half
                lx = (col + 0.5) / WHALE_COLS
                ly = (sy + 0.5) / sub_h
                # rotation inverse autour du centre de la silhouette
                dx, dy = lx - 0.5, ly - 0.5
                rx = dx * cos_p - dy * sin_p + 0.5
                ry = dx * sin_p + dy * cos_p + 0.5
                k[half] = whale_shape(rx, ry)
            if not k[0] and not k[1]:
                continue
            if k[0] and k[1]:
                ch = "█"
            elif k[0]:
                ch = "▀"
            else:
                ch = "▄"
            glyphs[row][col] = (ch, k[0] or k[1])
    # œil
    ec = int(WHALE_COLS * 0.88)
    er = int(WHALE_ROWS * 0.55)
    if glyphs[er][ec][0] != " ":
        glyphs[er][ec] = ("▄", 9)
    return glyphs


def draw_context(g: Grid) -> None:
    g.text(2, 1, "ZER0│GH0ST   ·   openrouter   ·   deepseek/deepseek-chat", DIM)
    g.text(2, 24, "❯ ", TEXT)


def draw_box(g: Grid) -> None:
    g.text(OL, OT, "╭" + "─" * (OW - 2) + "╮", DIM)
    for y in range(OT + 1, OT + OH - 1):
        g.put(OL, y, "│", DIM)
        g.put(OL + OW - 1, y, "│", DIM)
    g.text(OL, OT + OH - 1, "╰" + "─" * (OW - 2) + "╯", DIM)
    g.text(OL + (OW - 11) // 2, OT, " deepseek ", TITLE_FG)


def draw_sea(g: Grid, t: float) -> None:
    for y in range(SEA_Y, FLOOR):
        depth = (y - SEA_Y) / max(1, FLOOR - SEA_Y)
        for x in range(OL + 1, OL + OW - 1):
            h = math.sin(x * 0.30 + t * 3.4) + 0.55 * math.sin(x * 0.09 - t * 1.9)
            if y == SEA_Y:
                if h > 0.85:
                    g.put(x, y, "≈", FOAM)
                elif h > 0.10:
                    g.put(x, y, "~", SEA)
                else:
                    g.put(x, y, "-", lerp(SEA, DEEP, 0.30))
                continue
            k = depth * 2.6 + math.sin(x * 0.8 + y * 1.7 + t * 2) * 0.30
            if k < 0.85:
                continue
            ch = "░" if k < 1.9 else "▒"
            g.put(x, y, ch, lerp(SEA, DEEP, min(1.0, 0.22 + depth * 1.15)))


def trajectory(phase: float) -> tuple[float, float, float]:
    """(x, y, pitch) en cellules. x,y = coin haut-gauche du sprite."""
    x0 = OL + 6
    x1 = OL + OW - WHALE_COLS - 5
    span = x1 - x0
    up_y = SEA_Y - WHALE_ROWS + 2       # émergé (le corps sort de l'eau)
    apex_y = OT + 2                      # au plus haut
    if phase < 0.14:                     # poussée sous l'eau
        u = phase / 0.14
        return x0, up_y + int(5 * (1 - u)), -0.15
    if phase < 0.46:                     # montée
        u = (phase - 0.14) / 0.32
        y = up_y + (apex_y - up_y) * math.sin(u * math.pi / 2)
        return x0 + span * 0.55 * u, y, -0.55 + 0.45 * u
    if phase < 0.62:                     # apex
        u = (phase - 0.46) / 0.16
        return x0 + span * (0.55 + 0.15 * u), apex_y, 0.0
    if phase < 0.88:                     # plongée
        u = (phase - 0.62) / 0.26
        y = apex_y + (SEA_Y - apex_y + 2) * (u ** 1.4)
        return x0 + span * (0.70 + 0.30 * u), y, 0.15 + 0.55 * u
    u = (phase - 0.88) / 0.12            # sous la surface, il s'enfonce
    return x0 + span, SEA_Y + 1 + int(4 * u), 0.75


def draw_whale(g: Grid, phase: float) -> tuple[int, int]:
    sink = 0.0
    x, y, pitch = trajectory(phase)
    glyphs = rasterize_whale(pitch)
    if phase > 0.88:
        sink = min(1.0, (phase - 0.88) / 0.12)
    for r, row in enumerate(glyphs):
        for c, (ch, kind) in enumerate(row):
            if ch == " ":
                continue
            base = {1: BODY, 2: BELLY, 3: FIN, 4: FIN, 5: FIN, 9: EYE}.get(kind, BODY)
            yy = y + r
            if yy > FLOOR:
                continue
            # immergé : on teinte vers l'eau
            under = max(0.0, min(1.0, (yy - SEA_Y + 1) / 4.0 + sink))
            g.put(x + c, yy, ch, lerp(base, lerp(SEA, DEEP, 0.55), under * 0.85))
    return x, y


def draw_splash(g: Grid, t: float) -> None:
    for tb, xb, power in ((0.12, OL + 11, 1.0), (0.88, OL + OW - 13, 0.85)):
        dt = t - tb
        if not (0.0 <= dt <= 0.38):
            continue
        u = dt / 0.38
        for i in range(17):
            vx = (i - 8.0) * 1.0 * power
            vy = -8.5 + abs(i - 8.0) * 0.62
            px = int(xb + vx * u * 2.4)
            py = SEA_Y + int(vy * u + 12 * u * u)
            if py > FLOOR or py < OT + 1:
                continue
            ch = "·" if i % 3 == 0 else ("'" if i % 3 == 1 else "°")
            g.put(px, py, ch, lerp(FOAM, SEA, u * 0.85))
        # éclats au ras de l'eau (départ de la gerbe)
        for i in range(-9, 10):
            if u > 0.55:
                continue
            g.put(int(xb + i * 2.0 * power), SEA_Y, "*" if abs(i) < 3 else "·",
                  lerp(FOAM, SEA, 0.35))


def draw_bubbles(g: Grid, t: float, head: tuple[int, int]) -> None:
    if t < 0.88:
        return
    for i in range(9):
        px = head[0] - 10 + i * 4 + int(math.sin(i * 2.3 + t * 5) * 2)
        py = SEA_Y + 1 + int(i * 0.5) + int((t - 0.88) * 10)
        if py >= FLOOR:
            continue
        g.put(px, py, "o" if i % 2 else "·", lerp(FOAM, SEA, 0.45))


def render_frame(idx: int, fnt) -> Image.Image:
    t = idx / FRAMES
    g = Grid()
    draw_context(g)
    draw_box(g)
    draw_sea(g, t * DURATION)
    phase = min(1.0, t / 0.94)
    head = draw_whale(g, phase)
    draw_splash(g, t)
    draw_bubbles(g, t, head)

    label = ("émergence", "saut", "apex", "plongeon", "profondeur")[
        0 if phase < 0.14 else 1 if phase < 0.46 else 2 if phase < 0.62 else 3 if phase < 0.88 else 4
    ]
    g.text(OL + 2, FLOOR, f"◈ {label}", DIM)

    img = Image.new("RGB", (COLS * CELL_W, ROWS * CELL_H), BG)
    d = ImageDraw.Draw(img)
    for y in range(ROWS):
        for x in range(COLS):
            ch, color = g.cells[y][x]
            if ch != " ":
                d.text((x * CELL_W, y * CELL_H), ch, font=fnt, fill=color or TEXT)
    return img


def main() -> None:
    out = sys.argv[1] if len(sys.argv) > 1 else "whale-deepseek.gif"
    fnt = None
    for p in FONT_PATHS:
        if os.path.exists(p):
            fnt = ImageFont.truetype(p, FONT_SIZE)
            break
    if fnt is None:
        raise SystemExit("aucune police monospace trouvée")

    frames = [render_frame(i, fnt) for i in range(FRAMES)]
    pf = [f.quantize(colors=96, method=Image.MEDIANCUT, dither=Image.NONE) for f in frames]
    pf[0].save(out, save_all=True, append_images=pf[1:], duration=FRAME_MS,
               loop=0, optimize=True, disposal=1)
    print(f"{out} — {FRAMES} frames, {FRAME_MS} ms ({DURATION:.2f} s)")

    picks = [0, 8, 16, 28, 38, 46, 52]
    sheet = Image.new("RGB", (COLS * CELL_W, ROWS * CELL_H * len(picks)), BG)
    for i, p in enumerate(picks):
        sheet.paste(frames[p], (0, i * ROWS * CELL_H))
    sheet.save("whale-sheet.png")
    print("whale-sheet.png —", len(picks), "instants clés")


if __name__ == "__main__":
    main()
