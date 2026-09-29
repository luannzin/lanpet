#!/usr/bin/env python3
"""LanPet asset generator: procedural, shaded, auto-outlined pixel art.

Writes assets/<species>.png (13 animation rows x 4 frames of 32x32),
assets/hats.png, assets/items.png, assets/habitat.png and assets/meta.json.

    python3 tools/gen_assets.py [preview.png]

Needs Pillow. Re-run after tweaking; the game embeds whatever is in assets/.
"""
import json
import math
import os
import random
import sys

from PIL import Image

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "assets")
F = 32  # pet frame size
N4 = ((0, 1), (0, -1), (-1, 0), (1, 0))


def rgb(h, a=255):
    h = h.lstrip("#")
    return (int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16), a)


def mix(c1, c2, t):
    return tuple(round(c1[i] * (1 - t) + c2[i] * t) for i in range(3)) + (255,)


INK = rgb("#1b0f2e")
EYE = rgb("#24163a")
WHITE = rgb("#ffffff")
MOUTH = rgb("#7a1f3d")
TONGUE = rgb("#ff6f8e")
SWEAT = (rgb("#e6f8ff"), rgb("#8fd8ff"), rgb("#4fa8e8"))


def tones(hi, base, sh):
    return (rgb(hi), rgb(base), rgb(sh))


def pick(t, u, v):
    """Light from the top-left: highlight crescent up-left, shadow crescent down-right."""
    if len(t) == 4:  # single color
        return t
    hi, base, sh = t
    light = -0.5 * u - 0.85 * v
    r2 = u * u + v * v
    if light < -0.38 and r2 > 0.3:
        return sh
    if light > 0.5 and r2 > 0.3:
        return hi
    return base


class Canvas:
    def __init__(self, w=F, h=F):
        self.w, self.h = w, h
        self.px = [[None] * w for _ in range(h)]
        self.part = [[-1] * w for _ in range(h)]
        self.edge = []

    def _part(self, edge):
        self.edge.append(edge)
        return len(self.edge) - 1

    def put(self, x, y, c, p):
        x, y = math.floor(x), math.floor(y)
        if 0 <= x < self.w and 0 <= y < self.h:
            self.px[y][x] = c
            self.part[y][x] = p

    def ellipse(self, cx, cy, rx, ry, t, edge=True, clip=None):
        p = self._part(edge)
        for y in range(int(cy - ry) - 1, int(cy + ry) + 2):
            for x in range(int(cx - rx) - 1, int(cx + rx) + 2):
                u, v = (x + 0.5 - cx) / rx, (y + 0.5 - cy) / ry
                if u * u + v * v <= 1.0 and (clip is None or clip(x, y)):
                    self.put(x, y, pick(t, u, v), p)
        return p

    def ring(self, cx, cy, rx, ry, w, t, edge=True, clip=None):
        p = self._part(edge)
        for y in range(int(cy - ry) - 1, int(cy + ry) + 2):
            for x in range(int(cx - rx) - 1, int(cx + rx) + 2):
                u, v = (x + 0.5 - cx) / rx, (y + 0.5 - cy) / ry
                ui, vi = (x + 0.5 - cx) / (rx - w), (y + 0.5 - cy) / (ry - w)
                if u * u + v * v <= 1 and ui * ui + vi * vi > 1 and (clip is None or clip(x, y)):
                    self.put(x, y, pick(t, u, v), p)
        return p

    def poly(self, pts, t, edge=True):
        p = self._part(edge)
        xs, ys = [q[0] for q in pts], [q[1] for q in pts]
        x0, x1, y0, y1 = min(xs), max(xs), min(ys), max(ys)
        cx, cy, rx, ry = (x0 + x1) / 2, (y0 + y1) / 2, max((x1 - x0) / 2, 1), max((y1 - y0) / 2, 1)
        for y in range(int(y0), int(y1) + 1):
            for x in range(int(x0), int(x1) + 1):
                if inside(pts, x + 0.5, y + 0.5):
                    self.put(x, y, pick(t, (x + 0.5 - cx) / rx, (y + 0.5 - cy) / ry), p)
        return p

    def rect(self, x0, y0, x1, y1, t, edge=True):
        return self.poly([(x0, y0), (x1, y0), (x1, y1), (x0, y1)], t, edge)

    def dots(self, pts, c):
        p = self._part(False)
        for x, y in pts:
            self.put(x, y, c, p)

    def art(self, x0, y0, rows, pal):
        p = self._part(False)
        for dy, row in enumerate(rows):
            for dx, ch in enumerate(row):
                if ch != ".":
                    self.put(x0 + dx, y0 + dy, pal[ch], p)

    def image(self):
        out = [row[:] for row in self.px]
        # inner lines where a later, edged part overlaps an earlier one
        for y in range(self.h):
            for x in range(self.w):
                p = self.part[y][x]
                if p < 0:
                    continue
                for dx, dy in N4:
                    nx, ny = x + dx, y + dy
                    if 0 <= nx < self.w and 0 <= ny < self.h:
                        q = self.part[ny][nx]
                        if q > p and self.edge[q] and self.edge[p]:
                            out[y][x] = mix(self.px[y][x], INK, 0.5)
                            break
        # colored outer outline
        for y in range(self.h):
            for x in range(self.w):
                if self.px[y][x] is not None:
                    continue
                for dx, dy in N4:
                    nx, ny = x + dx, y + dy
                    if 0 <= nx < self.w and 0 <= ny < self.h and self.px[ny][nx] is not None:
                        out[y][x] = mix(self.px[ny][nx], INK, 0.78)
                        break
        img = Image.new("RGBA", (self.w, self.h), (0, 0, 0, 0))
        img.putdata([c if c else (0, 0, 0, 0) for row in out for c in row])
        return img


def inside(pts, x, y):
    c = False
    j = len(pts) - 1
    for i in range(len(pts)):
        xi, yi = pts[i]
        xj, yj = pts[j]
        if (yi > y) != (yj > y) and x < (xj - xi) * (y - yi) / (yj - yi) + xi:
            c = not c
        j = i
    return c


# ----------------------------------------------------------------------------- pets

SPECIES = {
    "dino": dict(
        body=tones("#a8ef7c", "#6fcf5a", "#46a24b"),
        belly=tones("#fffbd0", "#fff0a6", "#f0cb6a"),
        acc=tones("#ffc584", "#ff8f3a", "#d9612b"),
    ),
    "lizard": dict(
        body=tones("#ffbe86", "#ff8a4c", "#dd5a33"),
        belly=tones("#fff6d0", "#ffe39a", "#f2bf62"),
        acc=tones("#fff29a", "#ffd93d", "#e6a524"),
    ),
    "monkey": dict(
        body=tones("#dc9a60", "#b0713f", "#855029"),
        belly=tones("#fff0d4", "#f6d09c", "#dfae76"),
        acc=tones("#fff0d4", "#f6d09c", "#dfae76"),
    ),
    "frog": dict(
        body=tones("#9ad8ff", "#56a8ff", "#3576de"),
        belly=tones("#ffffff", "#d7eeff", "#a8cdf0"),
        acc=tones("#3f7de6", "#2d5fc4", "#244c9e"),
    ),
    "wolf": dict(
        body=tones("#d5deee", "#9ca8c0", "#6f7b97"),
        belly=tones("#ffffff", "#f1f4fa", "#cdd5e4"),
        acc=tones("#ffc2d3", "#ff9fbd", "#e0708f"),
    ),
}

STATES = [  # name, frames, fps
    ("idle", 4, 3), ("walk", 4, 8), ("run", 4, 12), ("sleep", 2, 1.5), ("eat", 2, 5),
    ("happy", 2, 6), ("train", 2, 3), ("study", 2, 1), ("attack", 2, 6), ("hurt", 1, 1),
    ("sad", 2, 1.5), ("blink", 1, 1), ("egg", 4, 4),
]


def pose(state, f):
    p = dict(hy=0, by=0, bw=0.0, bh=0.0, arms="down", fl=0, fr=0, eyes="open", mouth="smile",
             prop=None, fx=None, cheek=1)
    if state == "idle":
        p["hy"] = [0, 0, 1, 1][f]
        p["bw"] = [0, 0, 0.4, 0.4][f]
        p["arms"] = ["down", "down", "down2", "down2"][f]
    elif state == "blink":
        p["eyes"] = "closed"
    elif state == "walk":
        p["hy"] = p["by"] = [0, -1, 0, -1][f]
        p["fl"], p["fr"] = [(0, 0), (2, 0), (0, 0), (0, 2)][f]
        p["arms"] = ["down", "swingL", "down", "swingR"][f]
    elif state == "run":
        p["hy"] = p["by"] = [0, -2, -1, -2][f]
        p["fl"], p["fr"] = [(0, 0), (3, 0), (0, 0), (0, 3)][f]
        p["arms"] = ["swingL", "swingR", "swingL", "swingR"][f]
        p["eyes"] = "determined"
        p["mouth"] = "open"
    elif state == "sleep":
        p["hy"] = [4, 3][f]
        p["bw"], p["bh"] = [(1.5, -1.5), (1.2, -1.0)][f]
        p["arms"] = "tuck"
        p["eyes"] = "sleep"
        p["mouth"] = ["o", "dot"][f]
    elif state == "eat":
        p["hy"] = [0, 1][f]
        p["mouth"] = ["open", "chew"][f]
        p["eyes"] = ["open", "happy"][f]
        p["cheek"] = [1, 2][f]
        p["arms"] = "hold"
    elif state == "happy":
        p["hy"] = p["by"] = [0, -1][f]
        p["arms"] = ["up", "up2"][f]
        p["eyes"] = "happy"
        p["mouth"] = "open"
    elif state == "train":
        p["hy"] = [1, 0][f]
        p["arms"] = ["curl", "hold_lo"][f]
        p["prop"] = ["bell_hi", "bell_lo"][f]
        p["eyes"] = "determined"
        p["mouth"] = ["grit", "smile"][f]
        p["fx"] = "sweat"
    elif state == "study":
        p["hy"] = [1, 1][f]
        p["arms"] = "book"
        p["prop"] = ["book", "book2"][f]
        p["eyes"] = ["down", "closed"][f]
        p["mouth"] = ["dot", "smile"][f]
    elif state == "attack":
        p["hy"] = p["by"] = [1, -2][f]
        p["bw"], p["bh"] = [(0.8, -0.8), (-0.6, 0.8)][f]
        p["arms"] = ["tuck", "up2"][f]
        p["eyes"] = "angry"
        p["mouth"] = ["grit", "roar"][f]
    elif state == "hurt":
        p["hy"] = 1
        p["arms"] = "up"
        p["eyes"] = "hurt"
        p["mouth"] = "wobble"
        p["cheek"] = 0
    elif state == "sad":
        p["hy"] = 1
        p["eyes"] = "sad"
        p["mouth"] = "frown"
        p["arms"] = "down2"
        p["fx"] = [None, "tear"][f]
        p["cheek"] = 0
    return p


def tail(c, sp, P, by):
    body, acc = P["body"], P["acc"]
    if sp == "dino":
        for i, (x, y, r) in enumerate([(22, 26.5, 3.2), (24.5, 25.5, 2.7), (26.5, 24, 2.2), (28, 22.3, 1.6)]):
            c.ellipse(x, y + by, r, r * 0.9, body)
        c.poly([(24, 23 + by), (25.5, 20.5 + by), (26.5, 23 + by)], acc)
    elif sp == "lizard":
        for x, y, r in [(22, 27, 3), (25, 27.3, 2.5), (27.3, 26.2, 2), (28.6, 24.2, 1.6), (28.2, 22.2, 1.3)]:
            c.ellipse(x, y + by, r, r * 0.85, body)
        c.ellipse(26.6, 22.6 + by, 1.2, 1.2, body)
    elif sp == "wolf":
        c.ellipse(25.5, 22.5 + by, 3.4, 5.2, body)
        c.ellipse(26.3, 18.6 + by, 2.2, 2.4, P["belly"])
    elif sp == "monkey":
        for x, y, r in [(23, 26.5, 1.6), (25.2, 25.4, 1.6), (26.8, 23.4, 1.6), (27.3, 21, 1.6), (26.2, 19, 1.6), (24.3, 18.6, 1.4)]:
            c.ellipse(x, y + by, r, r, body)
    elif sp == "frog":
        pass


def behind_head(c, sp, P, hy):
    body, acc = P["body"], P["acc"]
    if sp == "dino":
        c.poly([(12.5, 6 + hy), (16, -0.5 + hy), (19.5, 6 + hy)], acc)
        c.poly([(8, 8 + hy), (9.5, 2 + hy), (13, 6 + hy)], acc)
        c.poly([(24, 8 + hy), (22.5, 2 + hy), (19, 6 + hy)], acc)
    elif sp == "wolf":
        for s in (-1, 1):
            c.poly([(16 + s * 4, 7 + hy), (16 + s * 10.5, -0.5 + hy), (16 + s * 11.5, 10 + hy)], body)
            c.poly([(16 + s * 6.5, 7 + hy), (16 + s * 10, 2 + hy), (16 + s * 10.4, 8 + hy)], acc, edge=False)
    elif sp == "monkey":
        for s in (-1, 1):
            c.ellipse(16 + s * 12, 13.5 + hy, 3.6, 3.8, body)
            c.ellipse(16 + s * 12.3, 13.7 + hy, 2, 2.3, P["belly"], edge=False)
    elif sp == "frog":
        for s in (-1, 1):
            c.ellipse(16 + s * 6.5, 7 + hy, 5, 4.5, body)
    elif sp == "lizard":  # frilled lizard: spiky fan behind the head
        cx, cy = 16, 13 + hy
        pts = [(cx + (14.5 if i % 2 == 0 else 11.5) * math.cos(math.pi + i * math.pi / 12),
                cy + (14.5 if i % 2 == 0 else 11.5) * 0.8 * math.sin(math.pi + i * math.pi / 12)) for i in range(13)]
        c.poly(pts + [(cx + 10, cy + 4), (cx - 10, cy + 4)], acc)
        c.poly([(cx - 9, cy - 3), (cx - 12, cy - 7.5), (cx - 7.5, cy - 5)], P["body"], edge=False)
        c.poly([(cx + 9, cy - 3), (cx + 12, cy - 7.5), (cx + 7.5, cy - 5)], P["body"], edge=False)


def eye(c, x0, y0, style, flip):
    """4 wide eye; x0 = left column. flip mirrors asymmetric styles for the right eye."""
    def m(dx):
        return x0 + (3 - dx if flip else dx)
    pts = []
    if style in ("open", "down", "determined", "angry", "sad"):
        top = 1 if style in ("down", "angry", "determined") else 0
        shape = [".ee.", "eeee", "eeee", "eeee", ".ee."]
        for dy, row in enumerate(shape):
            if dy < top:
                continue
            for dx, ch in enumerate(row):
                if ch == "e":
                    pts.append((m(dx), y0 + dy))
        c.dots(pts, EYE)
        hy = y0 + (2 if style == "down" else 1)
        c.dots([(m(1), hy), (m(1), hy + 1) if style not in ("down",) else (m(2), hy)], WHITE)
        c.dots([(m(2), y0 + 3)] if style == "open" else [], WHITE)
        if style == "angry":
            c.dots([(m(0), y0 - 1), (m(1), y0 - 1), (m(2), y0), (m(3), y0)], EYE)
        if style == "determined":
            c.dots([(m(0), y0 - 1), (m(1), y0 - 1), (m(2), y0 - 1), (m(3), y0)], EYE)
        if style == "sad":
            c.dots([(m(0), y0), (m(1), y0 - 1), (m(2), y0 - 1), (m(3), y0 - 2)], EYE)
    elif style == "closed":
        c.dots([(m(0), y0 + 2), (m(1), y0 + 3), (m(2), y0 + 3), (m(3), y0 + 2)], EYE)
    elif style == "sleep":
        c.dots([(m(0), y0 + 3), (m(1), y0 + 3), (m(2), y0 + 3), (m(3), y0 + 3)], EYE)
    elif style == "happy":
        c.dots([(m(0), y0 + 3), (m(1), y0 + 2), (m(2), y0 + 2), (m(3), y0 + 3)], EYE)
    elif style == "hurt":
        c.dots([(m(0), y0), (m(1), y0 + 1), (m(2), y0 + 2), (m(1), y0 + 3), (m(0), y0 + 4)], EYE)


def mouth(c, style, y, sp):
    if sp == "frog" and style == "smile":
        c.dots([(9, y - 1), (10, y)] + [(x, y + 1) for x in range(11, 21)] + [(21, y), (22, y - 1)], MOUTH)
        return
    if style == "smile":
        c.dots([(14, y), (15, y + 1), (16, y + 1), (17, y)], MOUTH)
    elif style == "open":
        c.dots([(14, y), (15, y), (16, y), (17, y), (14, y + 1), (17, y + 1), (15, y + 2), (16, y + 2)], MOUTH)
        c.dots([(15, y + 1), (16, y + 1)], TONGUE)
    elif style == "roar":
        c.dots([(x, y) for x in range(13, 19)] + [(13, y + 1), (18, y + 1), (13, y + 2), (18, y + 2)]
               + [(x, y + 3) for x in range(14, 18)], MOUTH)
        c.dots([(x, y + 1) for x in range(14, 18)] + [(x, y + 2) for x in range(14, 18)], TONGUE)
        c.dots([(14, y + 1), (17, y + 1)], WHITE)
    elif style == "grit":
        c.dots([(x, y + 1) for x in range(13, 19)], MOUTH)
        c.dots([(x, y) for x in range(14, 18)], WHITE)
    elif style == "chew":
        c.dots([(15, y + 1), (16, y + 1)], MOUTH)
    elif style == "o":
        c.dots([(15, y), (16, y), (15, y + 1), (16, y + 1)], MOUTH)
    elif style == "dot":
        c.dots([(15, y + 1), (16, y + 1)], MOUTH)
    elif style == "frown":
        c.dots([(14, y + 1), (15, y), (16, y), (17, y + 1)], MOUTH)
    elif style == "wobble":
        c.dots([(13, y + 1), (14, y), (15, y + 1), (16, y), (17, y + 1), (18, y)], MOUTH)


def arms(c, style, body, by, front):
    """front=False draws arms that sit beside the body, True draws ones held in front / raised."""
    A = {
        "down": [(9.2, 23.5), (22.8, 23.5)], "down2": [(9.4, 24), (22.6, 24)],
        "swingL": [(9, 22), (23, 24.5)], "swingR": [(9, 24.5), (23, 22)],
        "tuck": [(10, 24.5), (22, 24.5)],
        "hold": [(12.3, 23), (19.7, 23)], "hold_lo": [(9.5, 25.5), (22.5, 25.5)],
        "curl": [(9.5, 20.5), (22.5, 20.5)], "book": [(7.8, 23.5), (24.2, 23.5)],
        "up": [(5, 15.5), (27, 15.5)], "up2": [(4.5, 12.5), (27.5, 12.5)],
    }[style]
    is_front = style in ("hold", "hold_lo", "curl", "book", "up", "up2")
    if is_front != front:
        return
    for x, y in A:
        c.ellipse(x, y + by, 2.1, 2.7, body)


def prop(c, kind, hy):
    steel = tones("#e4eaf2", "#b4bdcb", "#7d8799")
    plate = tones("#6a7488", "#454d60", "#2c3242")
    if kind in ("bell_hi", "bell_lo"):
        y = 19.5 if kind == "bell_hi" else 25.5
        c.rect(3, y - 0.8, 29, y + 0.8, steel)
        for x in (3, 26):
            c.rect(x, y - 3.5, x + 3, y + 3.5, plate)
    elif kind in ("book", "book2"):
        cover = tones("#ff8a8a", "#e84a5f", "#b02e45")
        page = (WHITE, rgb("#fffaf0"), rgb("#ecdcc0"))
        y = 19.5 + hy
        c.rect(8, y, 24, y + 7.5, cover)
        c.rect(9, y - 1, 15.6, y + 6.5, page, edge=False)
        c.rect(16.4, y - 1, 23, y + 6.5, page, edge=False)
        ink = rgb("#c9b89a")
        c.dots([(x, y + 2) for x in (10, 11, 12, 13, 14, 18, 19, 20, 21)], ink)
        c.dots([(16, y + r) for r in range(0, 7)], cover[2])
        if kind == "book2":
            c.poly([(16, y - 1), (21, y - 3), (21, y + 4), (16, y + 6)], page)


def fx(c, kind, hy):
    if kind == "sweat":
        c.ellipse(26, 7.5 + hy, 1.6, 2.2, SWEAT)
        c.put(26, 5 + hy, SWEAT[1], c._part(True))
    elif kind == "tear":
        c.ellipse(10.5, 19.5 + hy, 1.2, 1.8, SWEAT)


def pet_frame(sp, state, f):
    P = SPECIES[sp]
    c = Canvas()
    if state == "egg":
        return egg_frame(sp, f), (16, 9)
    p = pose(state, f)
    hy, by = p["hy"], p["by"]
    body, belly = P["body"], P["belly"]
    tail(c, sp, P, by)
    fw = 3.6 if sp == "frog" else 3.0
    c.ellipse(16 - 4.5 - (0.6 if sp == "frog" else 0), 28.6 - p["fl"], fw, 1.9, body)
    c.ellipse(16 + 4.5 + (0.6 if sp == "frog" else 0), 28.6 - p["fr"], fw, 1.9, body)
    c.ellipse(16, 23.5 + by + p["bh"] * 0.5, 6.6 + p["bw"], 5.4 + p["bh"], body)
    c.ellipse(16, 24.6 + by, 4.2 + p["bw"] * 0.6, 3.4, belly, edge=False)
    arms(c, p["arms"], body, by, front=False)
    behind_head(c, sp, P, hy)
    hrx, hry = (12.5, 8.3) if sp == "frog" else (11.6, 9.0)
    c.ellipse(16, 13 + hy + (0.7 if sp == "frog" else 0), hrx, hry, body)
    # face
    if sp == "monkey":
        c.ellipse(12.3, 13.3 + hy, 4.3, 4.6, belly, edge=False)
        c.ellipse(19.7, 13.3 + hy, 4.3, 4.6, belly, edge=False)
        c.ellipse(16, 17.4 + hy, 6.2, 3.6, belly, edge=False)
        c.dots([(15, 3 + hy), (16, 2 + hy), (17, 3 + hy), (16, 4 + hy)], P["body"][2])
    if sp == "wolf":
        c.ellipse(16, 17.3 + hy, 5.2, 3.6, belly, edge=False)
        for s in (-1, 1):
            c.poly([(16 + s * 9, 15 + hy), (16 + s * 13.4, 19 + hy), (16 + s * 8, 20 + hy)], body)
    if sp == "lizard":
        c.dots([(7, 9 + hy), (8, 9 + hy), (24, 9 + hy), (23, 10 + hy), (22, 20 + by), (10, 21 + by)], P["acc"][1])
    if sp == "frog":
        c.dots([(8, 14 + hy), (9, 15 + hy), (23, 14 + hy), (22, 15 + hy), (15, 9 + hy), (17, 8 + hy)], P["acc"][1])
    ey = 11 + hy
    if sp == "frog":
        ey = 4 + hy
        for s in (-1, 1):
            c.ellipse(16 + s * 6.5, 6.5 + hy, 3.4, 3.1, (WHITE, WHITE, rgb("#dde8f5")), edge=False)
        ex = (8, 20)
    else:
        ex = (9, 19)
    eye(c, ex[0], ey, p["eyes"], False)
    eye(c, ex[1], ey, p["eyes"], True)
    cheek = mix(P["body"][1], rgb("#ff5f9a"), 0.55)
    if p["cheek"]:
        cy = 16 + hy
        blush = [(6, cy), (7, cy), (8, cy)] if p["cheek"] == 1 else [(x, y) for x in (5, 6, 7, 8) for y in (cy, cy + 1)]
        c.dots(blush + [(31 - x, y) for x, y in blush], cheek)
    my = 16 + hy
    if sp == "wolf":
        c.dots([(15, 15 + hy), (16, 15 + hy), (15, 16 + hy), (16, 16 + hy)], EYE)
        c.dots([(15, 15 + hy)], rgb("#6a5a88"))
        my = 17 + hy
    if sp == "dino":
        c.dots([(13, 15 + hy), (18, 15 + hy)], P["body"][2])
    if sp == "monkey":
        c.dots([(15, 16 + hy), (16, 16 + hy)], P["belly"][2])
        my = 17 + hy
    mouth(c, p["mouth"], my, sp)
    arms(c, p["arms"], body, by, front=True) if not p["prop"] else None
    if p["prop"]:
        prop(c, p["prop"], hy)
        arms(c, p["arms"], body, by, front=True)
    if p["fx"]:
        fx(c, p["fx"], hy)
    head_top = (16, 2 + hy) if sp == "frog" else (16, int(round(13 + hy - hry)) + 1)
    return c.image(), head_top


def egg_frame(sp, f):
    P = SPECIES[sp]
    c = Canvas()
    shell = tones("#ffffff", "#fff4dc", "#e8cfa4")
    tilt = [0, -1, 1, 0][f]
    c.ellipse(16 + tilt * 0.6, 20, 8.2, 10, shell)
    spot = P["body"]
    for x, y, r in [(12, 15, 1.9), (19.5, 12.5, 1.5), (18, 21, 2.2), (11.5, 24.5, 1.4), (21.5, 26, 1.4)]:
        c.ellipse(x + tilt * 0.6, y, r, r, spot, edge=False)
    if f >= 2:
        crack = rgb("#6b4a2a")
        c.dots([(9, 18), (10, 19), (11, 18), (12, 19), (13, 18), (14, 19), (15, 18)], crack)
    if f >= 3:
        c.dots([(16, 19), (17, 18), (18, 19), (19, 18), (20, 19), (21, 18), (22, 19)], crack)
    return c.image()


# ----------------------------------------------------------------------------- hats
# canvas 32x28, anchor (16,14) = top of the pet's head
HATS = ["PartyHat", "Flower", "Headphones", "WizardHat", "CowboyHat", "Crown", "Halo"]


def hat(name):
    c = Canvas(32, 28)
    if name == "PartyHat":
        pink, yel = tones("#ffb3d4", "#ff6fae", "#d9468a"), tones("#fff3a0", "#ffd84a", "#e0a82a")
        c.poly([(16, 2), (8.5, 16.5), (23.5, 16.5)], pink)
        for y in (6.5, 11.5):  # stripes follow the cone's slope
            k = 7.5 / 14.5
            c.poly([(16 - (y - 2) * k, y), (16 + (y - 2) * k, y), (16 + (y + 2 - 2) * k, y + 2), (16 - (y + 2 - 2) * k, y + 2)], yel, edge=False)
        c.ellipse(16, 2.5, 2.4, 2.4, tones("#ffffff", "#9ef0ff", "#4fc3e8"))
    elif name == "Flower":
        petal = tones("#ffd1e6", "#ff8ec2", "#e0609e")
        for a in range(5):
            ang = a * 2 * math.pi / 5 - math.pi / 2
            c.ellipse(22 + math.cos(ang) * 2.6, 13 + math.sin(ang) * 2.6, 2.1, 2.1, petal)
        c.ellipse(22, 13, 1.6, 1.6, tones("#fff6a0", "#ffd84a", "#e0a82a"), edge=False)
        c.ellipse(17.5, 16, 2.2, 1.2, tones("#a8ef7c", "#5cbf4a", "#3a8f3a"))
    elif name == "Headphones":
        band = tones("#8a8fa8", "#4a4f66", "#2c3044")
        c.ring(16, 23, 13.6, 12, 2.2, band, clip=lambda x, y: y < 21)
        cup = tones("#ff9a9a", "#ff5a6e", "#c73a52")
        c.ellipse(3.5, 22, 2.8, 4.2, cup)
        c.ellipse(28.5, 22, 2.8, 4.2, cup)
    elif name == "WizardHat":
        purple = tones("#c9a0ff", "#8e5bff", "#6238c9")
        c.poly([(19, 0), (8.5, 14), (23.5, 14)], purple)
        c.ellipse(16, 15, 11.5, 2.6, purple)
        star = rgb("#ffe066")
        c.dots([(15, 8), (14, 9), (15, 9), (16, 9), (15, 10), (18, 4)], star)
    elif name == "CowboyHat":
        brown = tones("#e0a36a", "#b0703c", "#7d4a24")
        c.poly([(10, 5), (22, 5), (23, 14), (9, 14)], brown)
        c.rect(9.3, 11, 22.7, 13, tones("#5a3a22", "#3d2616", "#2a190e"))
        c.ellipse(16, 14.5, 14.5, 2.8, brown)
    elif name == "Crown":
        gold = tones("#fff3a0", "#ffcc33", "#d9961f")
        c.poly([(9, 16), (9, 6), (12.5, 10.5), (16, 4), (19.5, 10.5), (23, 6), (23, 16)], gold)
        c.dots([(15, 12), (16, 12), (15, 13), (16, 13)], rgb("#ff3d6e"))
        c.dots([(11, 13), (20, 13)], rgb("#4fc3ff"))
        c.dots([(9, 5), (16, 3), (23, 5)], rgb("#fffbe0"))
    elif name == "Halo":
        c.ring(16, 7, 8.5, 2.8, 1.4, tones("#fffbd0", "#ffe45c", "#f0b429"))
    return c.image()


# ----------------------------------------------------------------------------- items (16x16)
ITEMS = ["Apple", "Burger", "Cake", "Sushi", "GoldenFish", "Coffee", "EnergyDrink", "Potion", "MegaPotion",
         "WoodSword", "IronSword", "DragonBlade", "MagicWand", "ArcaneOrb", "LeatherVest", "KnightArmor",
         "LuckyClover", "SpeedBoots"]


def blade(c, t, hilt, guard, length=10):
    c.poly([(3, 12), (4.2, 13.2), (4 + length * 0.95, 3.2), (3 + length, 2), (2.8 + length * 0.95, 1.8)], t)
    c.poly([(2, 10), (6, 14), (7, 13), (3, 9)], guard)
    c.poly([(0.5, 14.5), (1.5, 15.5), (4, 13), (3, 12)], hilt)


def item(name):
    c = Canvas(16, 16)
    T = tones
    if name == "Apple":
        c.ellipse(8, 9.5, 5.6, 5.2, T("#ff9d9d", "#f0434f", "#b52a3c"))
        c.rect(7.5, 2, 8.5, 5, T("#a0703c", "#7d4a24", "#5a3418"))
        c.ellipse(10.5, 3.5, 2.4, 1.3, T("#b4f07c", "#5cbf4a", "#3a8f3a"))
    elif name == "Burger":
        c.ellipse(8, 7, 6.6, 4.2, T("#ffd08a", "#f0a040", "#c77a2a"), clip=lambda x, y: y < 8)
        c.rect(1.5, 8, 14.5, 9, T("#b4f07c", "#5cbf4a", "#3a8f3a"))
        c.rect(1.5, 9, 14.5, 11, T("#a0603c", "#6e3a22", "#4a2414"))
        c.rect(2, 11, 14, 13, T("#ffd08a", "#f0a040", "#c77a2a"))
        c.dots([(5, 5), (8, 4), (11, 5)], rgb("#fff6d0"))
    elif name == "Cake":
        c.rect(2, 7, 14, 14, T("#fff6f0", "#ffe0e8", "#f0b8c8"))
        c.rect(2, 9.5, 14, 10.5, T("#ff9ec4", "#ff6fae", "#d9468a"))
        c.rect(2, 6, 14, 8, T("#ffffff", "#fffaf5", "#e8d8d0"))
        c.ellipse(8, 4.5, 1.8, 1.8, T("#ff9d9d", "#f0434f", "#b52a3c"))
    elif name == "Sushi":
        c.ellipse(8, 10, 6.5, 3.5, T("#ffffff", "#f6f6f0", "#d8d8cc"))
        c.ellipse(8, 7.5, 6.8, 2.8, T("#ffc0a0", "#ff8a5c", "#e0603c"))
        c.dots([(5, 7), (8, 6), (11, 7)], rgb("#fff0e0"))
        c.rect(6.5, 6, 9.5, 13, T("#3a5a4a", "#22382e", "#16261e"))
    elif name == "GoldenFish":
        gold = T("#fffbd0", "#ffcc33", "#d9961f")
        c.ellipse(7, 8.5, 5.5, 3.8, gold)
        c.poly([(11, 8.5), (15, 5), (15, 12)], gold)
        c.dots([(4, 7)], EYE)
        c.dots([(2, 3), (13, 2), (14, 14)], rgb("#fffbe0"))
    elif name == "Coffee":
        c.ring(12, 9.5, 2.8, 2.8, 1.2, T("#ffffff", "#f0f0f6", "#c8c8d8"))
        c.rect(2.5, 6, 11.5, 14, T("#ffffff", "#f0f0f6", "#c8c8d8"))
        c.rect(3.5, 6, 10.5, 7.5, T("#a0703c", "#6e3a22", "#4a2414"))
        c.dots([(5, 3), (6, 2), (8, 4), (9, 3)], rgb("#dfe6f0"))
    elif name == "EnergyDrink":
        c.rect(4.5, 2, 11.5, 15, T("#b4ff8a", "#4cd964", "#2a9e4a"))
        c.rect(4.5, 2, 11.5, 3, T("#e4eaf2", "#b4bdcb", "#7d8799"))
        c.dots([(9, 5), (8, 6), (7, 7), (8, 7), (9, 7), (8, 8), (7, 9), (6, 10)], rgb("#fff36a"))
    elif name in ("Potion", "MegaPotion"):
        liq = T("#ffb0b0", "#ff4a6a", "#c02a4a") if name == "Potion" else T("#e0b8ff", "#a05aff", "#6a2ecc")
        glass = T("#ffffff", "#e8f4ff", "#b8d0e8")
        c.rect(6.5, 2.5, 9.5, 6, glass)
        c.ellipse(8, 10.5, 5.2, 4.8, glass)
        c.ellipse(8, 11.2, 4.2, 3.8, liq, edge=False, clip=lambda x, y: y >= 9)
        c.rect(6, 1, 10, 3, T("#e0a36a", "#b0703c", "#7d4a24"))
        c.dots([(5, 9), (5, 10)], WHITE)
        if name == "MegaPotion":
            c.dots([(12, 3), (13, 2), (14, 3), (13, 4)], rgb("#fff36a"))
    elif name == "WoodSword":
        blade(c, T("#f0c890", "#c8904c", "#946030"), T("#8a5a30", "#5a3418", "#3a2010"), T("#a0703c", "#7d4a24", "#5a3418"))
    elif name == "IronSword":
        blade(c, T("#ffffff", "#c8d2e0", "#8a96aa"), T("#8a5a30", "#5a3418", "#3a2010"), T("#fff3a0", "#ffcc33", "#d9961f"))
    elif name == "DragonBlade":
        blade(c, T("#ffb0a0", "#e8403c", "#9e1e2a"), T("#4a3a5a", "#2a2038", "#1a1224"), T("#fff3a0", "#ffcc33", "#d9961f"), 11)
        c.dots([(8, 7), (10, 5)], rgb("#fff36a"))
    elif name == "MagicWand":
        c.poly([(1.5, 14.5), (2.5, 15.5), (10.5, 7.5), (9.5, 6.5)], T("#e0a36a", "#b0703c", "#7d4a24"))
        c.poly([(12, 0.5), (13.2, 3.6), (16, 4.2), (13.8, 6), (14.4, 9), (12, 7.4), (9.6, 9), (10.2, 6), (8, 4.2), (10.8, 3.6)],
               T("#fffbd0", "#ffd84a", "#e0a82a"))
    elif name == "ArcaneOrb":
        c.rect(4, 12, 12, 15, T("#e0a36a", "#b0703c", "#7d4a24"))
        c.ellipse(8, 7.5, 5.5, 5.5, T("#f0d8ff", "#a05aff", "#5a2eaa"))
        c.dots([(6, 5), (7, 5), (6, 6)], WHITE)
        c.dots([(9, 9), (10, 8)], rgb("#e0c0ff"))
    elif name == "LeatherVest":
        lea = T("#e0a36a", "#a8683a", "#7d4a24")
        c.poly([(2, 3), (6, 1.5), (8, 4), (10, 1.5), (14, 3), (13, 15), (3, 15)], lea)
        c.dots([(8, y) for y in range(5, 15)], rgb("#5a3418"))
        c.dots([(7, 7), (9, 7), (7, 10), (9, 10)], rgb("#ffcc33"))
    elif name == "KnightArmor":
        st = T("#ffffff", "#c8d2e0", "#7d8799")
        c.poly([(1.5, 3.5), (5, 1.5), (11, 1.5), (14.5, 3.5), (13, 15), (3, 15)], st)
        c.rect(7, 3, 9, 14, T("#fff3a0", "#ffcc33", "#d9961f"), edge=False)
        c.rect(4, 7, 12, 8, T("#fff3a0", "#ffcc33", "#d9961f"), edge=False)
    elif name == "LuckyClover":
        leaf = T("#b4f07c", "#4cc454", "#2a8a3a")
        for x, y in [(5.5, 5.5), (10.5, 5.5), (5.5, 10.5), (10.5, 10.5)]:
            c.ellipse(x, y, 3, 3, leaf)
        c.poly([(8, 8), (9, 8), (13, 15), (12, 15.5)], T("#8ad85c", "#3a9a3a", "#2a6a2a"))
    elif name == "SpeedBoots":
        red = T("#ff9d9d", "#f0434f", "#b52a3c")
        c.poly([(3, 2), (8, 2), (8, 10), (14, 11), (14.5, 14.5), (2.5, 14.5)], red)
        c.rect(2.5, 13, 14.5, 14.5, T("#ffffff", "#f0f0f6", "#c8c8d8"))
        c.poly([(8, 4), (14, 1.5), (13, 4.5), (15, 5), (9, 8)], T("#ffffff", "#e8f4ff", "#b8d0e8"))
    return c.image()


# ----------------------------------------------------------------------------- rooms
# Dark, neutral, minimal interiors. 200x84 each; the floor starts at FLOOR.
RW, RH, FLOOR = 200, 84, 64
BAYER = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]]
T = tones
WOOD = T("#5a4a44", "#4a3c37", "#382d2a")
DARKWOOD = T("#4a3a34", "#3b2e2a", "#2c2220")
METAL = T("#7a7f8e", "#5e6372", "#474b58")
WARM = "#ffcf8a"


def dq(t, x, y, steps):
    """Ordered-dither quantize t in 0..1 to `steps` levels."""
    return max(0, min(steps, math.floor(t * steps + (BAYER[y % 4][x % 4] + 0.5) / 16))) / steps


def base_room(wall=("#2d2b37", "#211f28"), floor=("#322e3b", "#25212c"), planks=True):
    img = Image.new("RGBA", (RW, RH))
    px = img.load()
    w0, w1, f0, f1 = (rgb(c) for c in (*wall, *floor))
    for y in range(RH):
        for x in range(RW):
            if y < FLOOR - 2:
                px[x, y] = mix(w0, w1, dq(y / (FLOOR - 2), x, y, 4))
            elif y < FLOOR:
                px[x, y] = rgb("#46414f") if y == FLOOR - 2 else rgb("#3a3544")
            else:
                col = mix(f0, f1, dq((y - FLOOR) / (RH - FLOOR), x, y, 3))
                band = (y - FLOOR) // 7
                if planks and ((x + band * 23) % 38 == 0 or (y - FLOOR) % 7 == 6):
                    col = mix(col, INK, 0.25)
                px[x, y] = col
    return img


def glow(img, cx, cy, r, color, amt, sy=1.0):
    px = img.load()
    col = rgb(color)
    for y in range(max(0, int(cy - r * sy)), min(RH, int(cy + r * sy) + 1)):
        for x in range(max(0, int(cx - r)), min(RW, int(cx + r) + 1)):
            d = math.hypot(x + 0.5 - cx, (y + 0.5 - cy) / sy) / r
            if d < 1:
                lvl = dq((1 - d) ** 1.6, x, y, 4) * amt
                if lvl > 0:
                    px[x, y] = mix(px[x, y], col, lvl)


def window(c, x, y, w, h):
    frame = T("#4c4859", "#3d3a4b", "#2f2c3b")
    c.rect(x, y, x + w, y + h, frame)
    c.rect(x + 2, y + 2, x + w - 2, y + h - 2, (rgb("#2b3663"), rgb("#1e2649"), rgb("#161c37")), edge=False)
    c.rect(x + w / 2 - 0.5, y + 2, x + w / 2 + 0.5, y + h - 2, frame, edge=False)
    c.rect(x + 2, y + h / 2 - 0.5, x + w - 2, y + h / 2 + 0.5, frame, edge=False)
    c.ellipse(x + w * 0.72, y + h * 0.28, 3, 3, rgb("#efe6c4"), edge=False)
    c.ellipse(x + w * 0.72 + 1.3, y + h * 0.28 - 0.8, 2.4, 2.4, rgb("#1e2649"), edge=False)
    c.dots([(x + 5, y + 5), (x + w * 0.3, y + h - 7), (x + w - 5, y + h - 5), (x + 8, y + h * 0.7)], rgb("#9aa6d0"))
    c.rect(x - 2, y + h, x + w + 2, y + h + 2, frame)


def room_home():
    img, c = base_room(), Canvas(RW, RH)
    c.ellipse(104, 75, 52, 5, T("#403a4c", "#3a3446", "#332e3e"), edge=False)
    window(c, 18, 12, 40, 30)
    sofa, cushion = T("#5a6075", "#474c5f", "#363a4a"), T("#6a7088", "#565c72", "#444a5c")
    c.rect(124, 42, 188, 58, sofa)
    c.rect(118, 54, 194, 66, sofa)
    c.rect(116, 47, 125, 66, sofa)
    c.rect(187, 47, 196, 66, sofa)
    c.rect(128, 49, 156, 56, cushion)
    c.rect(158, 49, 184, 56, cushion)
    c.rect(99, 25, 100, 65, METAL)
    c.rect(95, 64, 104, 66, METAL)
    c.poly([(92, 26), (107, 26), (104, 16), (95, 16)], T("#ffe2b0", "#f0c888", "#c9a068"))
    c.rect(68, 56, 78, 66, T("#6a5550", "#58463f", "#453631"))
    leaf = T("#6f9a70", "#557d58", "#405f44")
    for x, y, r in ((70, 50, 3.5), (76, 49, 3.5), (73, 45, 3.8), (67, 46, 2.8), (79, 44, 2.6)):
        c.ellipse(x, y, r, r * 1.2, leaf)
    img.alpha_composite(c.image())
    glow(img, 99.5, 24, 36, WARM, 0.32)
    glow(img, 38, 27, 26, "#7a9ae6", 0.14)
    return img, dict(spot=[100, 74])


def room_bedroom():
    img, c = base_room(), Canvas(RW, RH)
    window(c, 14, 10, 34, 28)
    c.rect(130, 14, 158, 32, T("#4c4859", "#3d3a4b", "#2f2c3b"))
    c.rect(132, 16, 156, 30, rgb("#2a2c3a"), edge=False)
    c.poly([(133, 30), (140, 21), (146, 27), (150, 23), (156, 30)], rgb("#4a5470"), edge=False)
    c.rect(182, 32, 192, 68, WOOD)
    c.rect(104, 47, 110, 68, WOOD)
    c.rect(108, 50, 184, 62, T("#6377a0", "#4d5f86", "#3c4a6a"))
    c.rect(162, 50, 184, 54, T("#e0dbe8", "#c8c2d4", "#a49eb4"), edge=False)
    c.ellipse(174, 48.5, 7, 3, T("#eeeaf4", "#d4cfe0", "#aca6bc"))
    c.rect(108, 62, 184, 66, WOOD)
    c.rect(82, 52, 98, 66, WOOD)
    c.rect(84, 58, 96, 59, rgb("#2c2220"), edge=False)
    c.rect(88, 46, 92, 52, METAL)
    c.poly([(84, 46), (96, 46), (94, 39), (86, 39)], T("#ffe2b0", "#f0c888", "#c9a068"))
    c.ellipse(60, 75, 30, 4.5, T("#3e3a50", "#383448", "#312d40"), edge=False)
    img.alpha_composite(c.image())
    glow(img, 90, 42, 32, WARM, 0.34)
    glow(img, 31, 24, 24, "#7a9ae6", 0.14)
    return img, dict(spot=[146, 51])


def room_kitchen():
    img, c = base_room(), Canvas(RW, RH)
    cab = T("#4c4652", "#3e3944", "#302c36")
    c.rect(6, 10, 76, 28, cab)
    for x in (29, 52):
        c.rect(x, 10, x + 1, 28, rgb("#2a2630"), edge=False)
    c.dots([(26, 24), (32, 24), (49, 24), (55, 24)], rgb("#8a8494"))
    c.rect(4, 45, 78, 66, cab)
    c.rect(2, 42, 80, 46, T("#7a7682", "#65616e", "#524e5a"))
    c.dots([(22, 52), (22, 53), (60, 52), (60, 53)], rgb("#8a8494"))
    c.ellipse(24, 38.5, 5, 4, T("#9aa0b0", "#7c8292", "#5e6474"))
    c.rect(22, 33, 26, 35, METAL)
    c.rect(158, 14, 192, 66, T("#8e93a2", "#767b8b", "#5e6373"))
    c.rect(158, 34, 192, 35, rgb("#4a4e5c"), edge=False)
    c.rect(161, 22, 162, 30, rgb("#c8ccd6"), edge=False)
    c.rect(161, 38, 162, 50, rgb("#c8ccd6"), edge=False)
    c.dots([(112, y) for y in range(0, 14)], rgb("#4a4658"))
    c.poly([(103, 20), (121, 20), (116, 13), (108, 13)], T("#ffe2b0", "#f0c888", "#c9a068"))
    img.alpha_composite(c.image())
    glow(img, 112, 22, 40, WARM, 0.34)
    return img, dict(spot=[112, 74])


def bookshelf(c, rnd, x0, x1, y0=8, y1=66):
    c.rect(x0, y0, x1, y1, DARKWOOD)
    books = ["#6b4f52", "#4f5f6e", "#5b6b52", "#6e6452", "#54526e", "#7a6a5c", "#4f6a66"]
    for sy in range(y0 + 3, y1 - 6, 13):
        c.rect(x0 + 2, sy, x1 - 2, sy + 10, rgb("#221a18"), edge=False)
        x = x0 + 3
        while x < x1 - 5:
            w, h = rnd.randint(2, 3), rnd.randint(6, 10)
            b = rgb(rnd.choice(books))
            c.rect(x, sy + 10 - h, x + w, sy + 10, (mix(b, WHITE, 0.15), b, mix(b, INK, 0.3)), edge=False)
            x += w + (2 if rnd.random() < 0.15 else 0)


def room_library():
    rnd = random.Random(3)
    img, c = base_room(), Canvas(RW, RH)
    bookshelf(c, rnd, 4, 62)
    bookshelf(c, rnd, 138, 196)
    c.rect(72, 49, 128, 53, WOOD)
    c.rect(75, 53, 79, 66, WOOD)
    c.rect(121, 53, 125, 66, WOOD)
    c.rect(111, 42, 112, 49, METAL)
    c.poly([(104, 42), (118, 42), (116, 38), (106, 38)], T("#7fb088", "#5a8a64", "#40664a"))
    c.rect(80, 47, 96, 49, T("#e8e2d4", "#d4ccb8", "#b0a890"), edge=False)
    c.ellipse(100, 75, 34, 4.5, T("#3a3446", "#343040", "#2e2a38"), edge=False)
    img.alpha_composite(c.image())
    glow(img, 111, 44, 30, "#e6f0a0", 0.28)
    return img, dict(spot=[100, 74])


def room_gym():
    img, c = base_room(floor=("#2c2b32", "#222127"), planks=False), Canvas(RW, RH)
    c.rect(0, 28, 200, 31, T("#5a4048", "#4a353c", "#3a2a30"), edge=False)
    plate, steel = T("#5a5f70", "#3f4454", "#2c303c"), METAL
    c.rect(10, 50, 56, 52, steel)
    c.rect(12, 52, 14, 66, steel)
    c.rect(52, 52, 54, 66, steel)
    c.rect(10, 58, 56, 60, steel)
    for row in (44, 54):
        for i in range(4):
            x = 15 + i * 10
            c.rect(x, row + 1, x + 2, row + 6, plate)
            c.rect(x + 6, row + 1, x + 8, row + 6, plate)
            c.rect(x + 2, row + 3, x + 6, row + 4, steel)
    c.rect(64, 68, 132, 79, T("#4a4f60", "#3d4252", "#323644"))
    c.rect(140, 62, 192, 68, T("#3a3a44", "#2c2c34", "#222228"))
    c.rect(143, 60, 188, 62, rgb("#1c1c22"), edge=False)
    c.rect(186, 36, 190, 62, steel)
    c.rect(172, 40, 190, 42, steel)
    c.poly([(179, 30), (197, 30), (195, 38), (181, 38)], steel)
    c.rect(184, 32, 192, 35, rgb("#3fd07a"), edge=False)
    for x in (40, 150):
        c.rect(x, 6, x + 26, 8, T("#f0f2ff", "#d8dcf0", "#b0b4c8"))
    img.alpha_composite(c.image())
    glow(img, 53, 8, 40, "#dfe6ff", 0.18, sy=0.8)
    glow(img, 163, 8, 40, "#dfe6ff", 0.18, sy=0.8)
    glow(img, 188, 33, 12, "#3fd07a", 0.25)
    return img, dict(spot=[98, 74], spot2=[163, 61])


def room_portal():
    img, c = base_room(wall=("#27243a", "#1c1a28")), Canvas(RW, RH)
    c.rect(14, 14, 56, 40, T("#9a8a66", "#857658", "#6a5d45"))
    c.dots([(20, 34), (23, 32), (26, 31), (29, 29), (33, 28), (36, 25), (40, 23), (44, 21)], rgb("#a0453c"))
    c.dots([(47, 18), (49, 20), (49, 18), (47, 20), (48, 19)], rgb("#c03a3a"))
    stone = T("#5c586c", "#4a4658", "#393647")
    c.ring(112, 50, 30, 40, 6, stone, clip=lambda x, y: y < 65)
    p = c._part(False)
    swirl = [rgb(h) for h in ("#1d1540", "#3a2a8a", "#6048d0", "#44c6d6", "#b8f4ff")]
    for y in range(10, 65):
        for x in range(86, 139):
            u, v = (x + 0.5 - 112) / 24, (y + 0.5 - 50) / 34
            r = math.hypot(u, v)
            if r <= 1:
                band = (math.atan2(v, u) / (2 * math.pi) * 3 + r * 2.2) % 1
                i = int(dq(band * 0.8 + (1 - r) * 0.5, x, y, 4) * 4)
                c.put(x, y, swirl[min(4, i)], p)
    c.rect(80, 64, 144, 68, stone)
    img.alpha_composite(c.image())
    glow(img, 112, 46, 64, "#7a5aff", 0.3)
    glow(img, 112, 52, 30, "#5ae0f0", 0.18)
    return img, dict(spot=[58, 74])


def room_arena():
    img, c = base_room(wall=("#262030", "#19161f"), floor=("#2e2835", "#211d27"), planks=False), Canvas(RW, RH)
    for x, col in ((22, ("#8a5064", "#6e3e50", "#54303d")), (160, ("#50648a", "#3e4f6e", "#303d54"))):
        c.poly([(x, 4), (x + 18, 4), (x + 18, 38), (x + 9, 33), (x, 38)], T(*col))
        c.ellipse(x + 9, 18, 4, 4, T("#f0d890", "#d8b860", "#b09040"), edge=False)
    c.ring(100, 74, 72, 8, 1.2, T("#5a4a6e", "#4c3e5e", "#40344f"), edge=False)
    c.rect(99, 66, 101, 82, rgb("#4c3e5e"), edge=False)
    img.alpha_composite(c.image())
    px = img.load()
    for y in range(RH):
        half = 10 + y * 0.75
        for x in range(RW):
            d = abs(x + 0.5 - 100) / half
            if d < 1:
                px[x, y] = mix(px[x, y], rgb("#e8e0ff"), dq((1 - d) ** 0.7 * (0.3 + 0.5 * y / RH), x, y, 4) * 0.22)
    return img, dict(spot=[60, 74], spot2=[140, 74])


def room_shop():
    rnd = random.Random(11)
    img, c = base_room(), Canvas(RW, RH)
    for sy in (24, 42):
        c.rect(22, sy, 178, sy + 2, WOOD)
        x = 28
        while x < 170:
            col = rgb(rnd.choice(["#b05a6a", "#5a82b0", "#6ab086", "#c0a05a", "#8e6ab0", "#b0845a"]))
            if rnd.random() < 0.5:
                c.ellipse(x + 2.5, sy - 3, 3, 3, (mix(col, WHITE, 0.3), col, mix(col, INK, 0.3)))
                c.rect(x + 1.5, sy - 8, x + 3.5, sy - 6, T("#e0dce8", "#c8c2d4", "#a49eb4"))
            else:
                c.rect(x, sy - 7, x + 5, sy, (mix(col, WHITE, 0.3), col, mix(col, INK, 0.3)))
            x += rnd.randint(9, 13)
    c.dots([(100, y) for y in range(0, 5)], rgb("#4a4658"))
    c.rect(86, 5, 114, 13, WOOD)
    c.ellipse(100, 9, 2.5, 2.5, T("#fff3a0", "#ffcc33", "#d9961f"), edge=False)
    c.rect(40, 50, 160, 66, T("#5a4a40", "#4a3c34", "#3a2e28"))
    c.rect(36, 47, 164, 51, T("#7a6a5c", "#665748", "#524538"))
    c.rect(128, 38, 146, 47, T("#4a4e5c", "#3a3e4a", "#2c2f38"))
    c.rect(131, 40, 138, 43, rgb("#3fd07a"), edge=False)
    img.alpha_composite(c.image())
    glow(img, 100, 10, 60, WARM, 0.2, sy=0.9)
    return img, dict(spot=[100, 74])


# Where pets stand when several share a room (LAN). Slot 0 (and gym slot 1) sit on the furniture
# a job uses, so a sleeping pet gets the bed and a runner gets the treadmill.
SLOTS = {
    "home": [[100, 74], [62, 75], [138, 75], [28, 77], [172, 77]],
    "bedroom": [[146, 51], [58, 74], [98, 76], [24, 77], [124, 78]],
    "kitchen": [[112, 74], [74, 75], [146, 76], [38, 77], [182, 77]],
    "library": [[100, 74], [68, 76], [132, 76], [32, 77], [168, 77]],
    "gym": [[98, 74], [163, 61], [66, 76], [130, 77], [30, 77]],
    "portal": [[58, 74], [28, 76], [160, 76], [86, 78], [186, 77]],
    "arena": [[60, 74], [140, 74], [100, 77], [24, 77], [176, 77]],
    "shop": [[100, 74], [62, 75], [138, 75], [26, 77], [174, 77]],
}

ROOMS = [("home", room_home), ("bedroom", room_bedroom), ("kitchen", room_kitchen), ("library", room_library),
         ("gym", room_gym), ("portal", room_portal), ("arena", room_arena), ("shop", room_shop)]


# ----------------------------------------------------------------------------- main
def sheet(frames, cols, fw, fh):
    rows = (len(frames) + cols - 1) // cols
    img = Image.new("RGBA", (cols * fw, rows * fh), (0, 0, 0, 0))
    for i, fr in enumerate(frames):
        img.paste(fr, ((i % cols) * fw, (i // cols) * fh))
    return img


def main():
    os.makedirs(ROOT, exist_ok=True)
    meta = {"frame": F, "states": [], "head": {}, "hats": HATS, "hat_size": [32, 28], "hat_anchor": [16, 14],
            "items": ITEMS, "item_size": 16}
    for row, (name, n, fps) in enumerate(STATES):
        meta["states"].append({"name": name, "row": row, "frames": n, "fps": fps})
    sheets = {}
    for sp in SPECIES:
        img = Image.new("RGBA", (4 * F, len(STATES) * F), (0, 0, 0, 0))
        heads = {}
        for row, (name, n, _) in enumerate(STATES):
            heads[name] = []
            for f in range(n):
                fr, head = pet_frame(sp, name, f)
                img.paste(fr, (f * F, row * F))
                heads[name].append(head)
        meta["head"][sp] = heads
        img.save(os.path.join(ROOT, f"{sp}.png"))
        sheets[sp] = img
    hats = sheet([hat(n) for n in HATS], len(HATS), 32, 28)
    hats.save(os.path.join(ROOT, "hats.png"))
    items = sheet([item(n) for n in ITEMS], 6, 16, 16)
    items.save(os.path.join(ROOT, "items.png"))
    rooms = Image.new("RGBA", (RW, RH * len(ROOMS)))
    meta["room_size"] = [RW, RH]
    meta["rooms"] = []
    for i, (name, fn) in enumerate(ROOMS):
        img, info = fn()
        rooms.paste(img, (0, i * RH))
        meta["rooms"].append({"name": name, **info, "slots": SLOTS[name]})
    rooms.save(os.path.join(ROOT, "rooms.png"))
    with open(os.path.join(ROOT, "meta.json"), "w") as fh:
        json.dump(meta, fh, separators=(",", ":"))

    if len(sys.argv) > 1:  # contact sheet: every room with a pet on its spot, 2x
        prev = Image.new("RGBA", (RW * 2 * 2 + 8, RH * 2 * 4 + 24), rgb("#111016"))
        idle = sheets["dino"].crop((0, 0, F, F))
        sleep = sheets["wolf"].crop((0, 3 * F, F, 4 * F))
        for i, (name, _) in enumerate(ROOMS):
            room = rooms.crop((0, i * RH, RW, (i + 1) * RH))
            info = meta["rooms"][i]
            x, y = info["spot"]
            room.alpha_composite(sleep if name == "bedroom" else idle, (x - 16, y - 31))
            if "spot2" in info:
                x, y = info["spot2"]
                room.alpha_composite(sheets["frog"].crop((0, 2 * F, F, 3 * F)), (x - 16, y - 31))
            prev.alpha_composite(room.resize((RW * 2, RH * 2), Image.NEAREST), ((i % 2) * (RW * 2 + 8), (i // 2) * (RH * 2 + 8)))
        prev.save(sys.argv[1])
    print("assets ->", os.path.normpath(ROOT))


if __name__ == "__main__":
    main()
