#!/usr/bin/env python3
"""Maquette du mod natif `/0cg` — panneau watchlist crypto.

Rendu en **cellules** (grille monospace), avec les palettes réelles des thèmes
`paper` et `noir` : ce que montre l'image est ce que ratatui affichera.
Les sparklines sont en **braille** (Canvas + Marker::Braille côté Rust).
"""
from PIL import Image, ImageDraw, ImageFont
import math

FONT = "/usr/share/fonts/TTF/FiraCodeNerdFontMono-Regular.ttf"
CW, CH, FS = 10, 19, 16
COLS, ROWS = 104, 30

PAPER = dict(bg=(250,249,245), text=(27,27,31), dim=(116,114,105), accent=(31,78,140),
             border=(74,74,69), ok=(30,107,58), bad=(166,27,27), warn=(138,90,0),
             panel=(240,239,233), grid=(228,226,217), ink=(44,94,159))
NOIR = dict(bg=(4,4,5), text=(235,235,239), dim=(105,105,113), accent=(218,218,224),
            border=(91,91,99), ok=(170,195,175), bad=(220,85,85), warn=(205,185,140),
            panel=(18,18,21), grid=(30,30,34), ink=(150,150,158))

BANNER = ["█████ █████ ████   ███  █  ████ █   █  ███   ████ █████",
          "   █  █     █   █ █   █ █ █     █   █ █   █ █       █",
          "  █   ████  ████  █   █ █ █ ███ █████ █   █  ███    █",
          " █    █     █  █  █   █ █ █   █ █   █ █   █     █   █",
          "█████ █████ █   █  ███  █  ███  █   █  ███  ████    █"]


class G:
    def __init__(self, cols, rows, pal):
        self.c = [[(" ", None) for _ in range(cols)] for _ in range(rows)]
        self.cols, self.rows, self.p = cols, rows, pal

    def put(self, x, y, ch, col, force=True):
        if not (0 <= x < self.cols and 0 <= y < self.rows):
            return
        if not force and self.c[y][x][0] != " ":
            return
        self.c[y][x] = (ch, col)

    def s(self, x, y, t, col, force=True):
        for i, ch in enumerate(t):
            self.put(x + i, y, ch, col, force)


def shade(rgb, f):
    return tuple(max(0, min(255, int(v * f))) for v in rgb)


def bevel_banner(g, x0, y0, rainbow=False):
    """Banner en blocs relief 3D (arête claire / face / ombre)."""
    wmax = max(len(r) for r in BANNER)
    mask = [list(r.ljust(wmax)) for r in BANNER]
    h, w = len(mask), len(mask[0])
    inks = [(44,94,159), (166,27,27), (30,107,58), (138,90,0)]
    for y in range(h):
        for x in range(w):
            if mask[y][x] == " ":
                continue
            up = mask[y-1][x] != " " if y > 0 else False
            left = mask[y][x-1] != " " if x > 0 else False
            down = mask[y+1][x] != " " if y+1 < h else False
            right = mask[y][x+1] != " " if x+1 < w else False
            base = inks[(x // 7) % 4] if not rainbow else (
                int(127 + 127*math.sin(x*0.08)), int(127 + 127*math.sin(x*0.08+2)), int(127+127*math.sin(x*0.08+4)))
            f = 1.45 if (not up or not left) else (0.62 if (not down or not right) else 1.0)
            g.put(x0 + x, y0 + y, "█", shade(base, f))


def braille_line(g, x0, y0, w, h, series, col):
    """Courbe en braille : 2×4 points par cellule."""
    BITS = {(0,0):0x01,(0,1):0x02,(0,2):0x04,(1,0):0x08,(1,1):0x10,(1,2):0x20,(0,3):0x40,(1,3):0x80}
    W, H = w * 2, h * 4
    vals = series
    lo, hi = min(vals), max(vals)
    span = (hi - lo) or 1.0
    pts = []
    for i, v in enumerate(vals):
        px = int(i * (W - 1) / max(1, len(vals) - 1))
        py = H - 1 - int((v - lo) / span * (H - 1))
        pts.append((px, py))
    cells = {}
    def plot(px, py):
        if 0 <= px < W and 0 <= py < H:
            cx, cy = px // 2, py // 4
            cells[(cx, cy)] = cells.get((cx, cy), 0) | BITS[(px % 2, py % 4)]
    for (a, b) in zip(pts, pts[1:]):
        (x1, y1), (x2, y2) = a, b
        dx, dy = abs(x2-x1), -abs(y2-y1)
        sx, sy = (1 if x1 < x2 else -1), (1 if y1 < y2 else -1)
        err = dx + dy
        while True:
            plot(x1, y1)
            if x1 == x2 and y1 == y2:
                break
            e2 = 2 * err
            if e2 >= dy:
                err += dy; x1 += sx
            if e2 <= dx:
                err += dx; y1 += sy
    for (cx, cy), bits in cells.items():
        g.put(x0 + cx, y0 + cy, chr(0x2800 + bits), col)


def spark(n, seed=0.0):
    """Marche aléatoire déterministe et lissée (allure d'un cours sur 7 jours)."""
    import random
    rnd = random.Random(int(seed * 1000) + 7)
    v, out = 0.0, []
    for i in range(n):
        v += rnd.uniform(-0.18, 0.20) + 0.03 * math.sin(i * 0.15 + seed)
        out.append(v)
    # lissage léger : un cours ne zigzague pas cellule par cellule
    sm = []
    for i in range(len(out)):
        w = out[max(0, i - 2):i + 3]
        sm.append(sum(w) / len(w))
    return sm


COINS = [
    ("BTC", "64 947 $", "+2,4 %", 0.0),
    ("ETH",  "1 822 $", "−1,1 %", 1.3),
    ("TAO",    "412 $", "+5,9 %", 2.1),
    ("SOL",    "148 $", "+0,7 %", 3.4),
    ("XRP",   "0,52 $", "−3,2 %", 4.2),
]


def panel_rows(g, x0, x1, y0, y1, coins, pal, stale=None, compact=False):
    """Watchlist. 3 rangées par actif (sparkline 12×2) ou 1 rangée (compact)."""
    for x in range(x0, x1 + 1):
        g.put(x, y0, "─", pal["border"]); g.put(x, y1, "─", pal["border"])
    for y in range(y0, y1 + 1):
        g.put(x0, y, "│", pal["border"]); g.put(x1, y, "│", pal["border"])
    for (x, y) in [(x0,y0),(x1,y0),(x0,y1),(x1,y1)]:
        g.put(x, y, "┼", pal["border"])
    g.s(x0 + 3, y0, f" 0cg · watchlist ({len(coins)}) ", pal["accent"])
    row = y0 + 1
    step = 1 if compact else 3
    for (sym, price, chg, seed) in coins:
        up = not chg.startswith("−")
        g.s(x0 + 2, row, sym, pal["text"])
        g.s(x0 + 8, row, price.rjust(10), pal["text"])
        g.s(x0 + 20, row, chg.rjust(7), pal["ok"] if up else pal["bad"])
        braille_line(g, x0 + 30, row, 12, 1 if compact else 2, spark(60, seed), pal["accent"])
        row += step
    if stale:
        g.s(x0 + 2, y1 - 2, stale, pal["warn"])
    else:
        g.s(x0 + 2, y1 - 2, " maj il y a 12 s · 1 requête / 15 s", pal["dim"])
    g.s(x0 + 2, y1 - 1, " /0cg add <id|sym> · rm · list · r", pal["dim"])


def screen(pal, theme_name, coins, stale=None):
    g = G(COLS, ROWS, pal)
    for y in range(ROWS):
        for x in range(COLS):
            g.put(x, y, " ", pal["bg"])
    # statut
    g.s(0, 0, "ctx 0.0%/1.0M · 96 tok  ·······  O No project   think:off", pal["dim"])
    g.s(66, 0, "OpenRouter · deepseek-v4.1-flash", pal["dim"])
    bevel_banner(g, 0, 1)
    g.s(3, 6, "ghost in the shell", pal["dim"])
    g.s(0, 7, "Local /home/omar0   |   Projet /home/omar0/ZER0v1", pal["dim"])
    # chat (gauche) : cadre de plan + 2 messages
    fx1, fy0, fy1 = 56, 9, 26
    for x in range(0, fx1):
        g.put(x, fy0, "─", pal["border"]); g.put(x, fy1, "─", pal["border"])
    for y in range(fy0, fy1 + 1):
        g.put(0, y, "│", pal["border"]); g.put(fx1, y, "│", pal["border"])
    for (x, y) in [(0,fy0),(fx1,fy0),(0,fy1),(fx1,fy1)]:
        g.put(x, y, "┼", pal["border"])
    g.s(3, fy0, " № 0042 · PLAN DE COUPE ", pal["dim"])
    for y in range(fy0 + 1, fy1, 2):
        for x in range(2, fx1, 4):
            g.put(x, y, "·", pal["grid"], force=False)
    g.s(2, fy0 + 1, "> vous  montre-moi où en est mon panier", pal["accent"])
    g.s(2, fy0 + 3, "> zer0  Trois actifs en hausse, deux en baisse.", pal["text"])
    g.s(2, fy0 + 4, "   Le panneau 0cg se rafraîchit toutes les 15 s.", pal["text"])
    g.s(2, fy0 + 6, "> vous  ajoute TAO et retire XRP", pal["accent"])
    g.s(2, fy0 + 8, "> zer0  Fait : /0cg add tao · /0cg rm xrp", pal["text"])
    g.s(0, 27, "❯ ", pal["accent"])
    # panneau 0cg
    panel_rows(g, 58, 103, 9, 26, coins, pal, stale, compact=len(coins) > 4)
    # pied
    g.s(0, 29, " ◆ PLAN ", pal["accent"] if theme_name == "paper" else pal["text"])
    g.s(9, 29, "ÉCH. 1:1 │ ", pal["dim"])
    g.s(20, 29, " F6  Chats   F7  Nudge   Ctrl+C  Exit   F3  Read&Write   F4  Code-exec", pal["dim"])
    return g


def render(g, path):
    fnt = ImageFont.truetype(FONT, FS)
    img = Image.new("RGB", (g.cols * CW, g.rows * CH), g.p["bg"])
    d = ImageDraw.Draw(img)
    for y in range(g.rows):
        for x in range(g.cols):
            ch, col = g.c[y][x]
            if ch != " ":
                d.text((x * CW, y * CH), ch, font=fnt, fill=col or g.p["text"])
    img.save(path)
    print(path)


if __name__ == "__main__":
    render(screen(PAPER, "paper", COINS[:4]), "/home/omar0/bg-variants/0cg-paper.png")
    render(screen(NOIR, "noir", COINS), "/home/omar0/bg-variants/0cg-noir.png")
    render(
        screen(PAPER, "paper", COINS[:4], stale=" ! prix indisponibles depuis 40 s (API)"),
        "/home/omar0/bg-variants/0cg-paper-erreur.png",
    )
