#!/usr/bin/env python3
"""LanPet asset generator: procedural, shaded, auto-outlined pixel art.

Writes assets/<species>.png (4 life stages x 15 animation rows x 4 frames of 32x32),
assets/hats.png, assets/items.png, assets/needs.png, assets/places.png (the ground of the town and of every
building's inside), assets/props.png (buildings, trees, furniture), assets/icon.png, assets/tray*.png
and assets/meta.json.

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
        # (scale, ox, oy, drop): shapes shrink towards (ox, oy), then move down by `drop`.
        # It's how the younger life stages get a smaller body under a still-big head.
        self.xf = None

    def _part(self, edge):
        self.edge.append(edge)
        return len(self.edge) - 1

    def pt(self, x, y):
        if not self.xf or (self.xf[0] == 1 and self.xf[3] == 0):
            return x, y
        s, ox, oy, drop = self.xf
        return ox + (x - ox) * s, oy + (y - oy) * s + drop

    def _r(self, r):
        return r * self.xf[0] if self.xf else r

    def put(self, x, y, c, p):
        x, y = math.floor(x), math.floor(y)
        if 0 <= x < self.w and 0 <= y < self.h:
            self.px[y][x] = c
            self.part[y][x] = p

    def ellipse(self, cx, cy, rx, ry, t, edge=True, clip=None):
        (cx, cy), rx, ry = self.pt(cx, cy), self._r(rx), self._r(ry)
        p = self._part(edge)
        for y in range(int(cy - ry) - 1, int(cy + ry) + 2):
            for x in range(int(cx - rx) - 1, int(cx + rx) + 2):
                u, v = (x + 0.5 - cx) / rx, (y + 0.5 - cy) / ry
                if u * u + v * v <= 1.0 and (clip is None or clip(x, y)):
                    self.put(x, y, pick(t, u, v), p)
        return p

    def ring(self, cx, cy, rx, ry, w, t, edge=True, clip=None):
        (cx, cy), rx, ry, w = self.pt(cx, cy), self._r(rx), self._r(ry), self._r(w)
        p = self._part(edge)
        for y in range(int(cy - ry) - 1, int(cy + ry) + 2):
            for x in range(int(cx - rx) - 1, int(cx + rx) + 2):
                u, v = (x + 0.5 - cx) / rx, (y + 0.5 - cy) / ry
                ui, vi = (x + 0.5 - cx) / (rx - w), (y + 0.5 - cy) / (ry - w)
                if u * u + v * v <= 1 and ui * ui + vi * vi > 1 and (clip is None or clip(x, y)):
                    self.put(x, y, pick(t, u, v), p)
        return p

    def poly(self, pts, t, edge=True):
        pts = [self.pt(*q) for q in pts]
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

    def dots(self, pts, c, over=False):
        """Single pixels. `over` keeps only the ones that land on something already drawn."""
        p = self._part(False)
        for x, y in pts:
            x, y = math.floor(x), math.floor(y)
            if self.xf:
                x, y = (math.floor(v) for v in self.pt(x + 0.5, y + 0.5))
            if not over or (0 <= x < self.w and 0 <= y < self.h and self.px[y][x] is not None):
                self.put(x, y, c, p)

    def block(self, x0, y0, w, h):
        """Where a w x h pixel block (an eye) lands under the transform, kept whole."""
        cx, cy = self.pt(x0 + w / 2, y0 + h / 2)
        return math.floor(cx - w / 2 + 0.5), math.floor(cy - h / 2 + 0.5)

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
    ("sad", 2, 1.5), ("blink", 1, 1), ("egg", 4, 4), ("sick", 2, 1.5), ("ghost", 2, 2),
]

# Life stages, in `Stage` order from game.rs: (head scale, head drop in px, body scale).
# Babies are mostly head; elders keep the adult shape and go grey (see palette()).
STAGES = ["baby", "teen", "adult", "elder"]
SHAPE = {"baby": (0.84, 6, 0.68), "teen": (0.93, 3, 0.86), "adult": (1, 0, 1), "elder": (1, 0, 1)}
GHOST = tones("#ffffff", "#e3ecff", "#b7c6f2")
BROW = rgb("#f6f3fb")


def palette(sp, stage, state):
    P = SPECIES[sp]
    if state == "ghost":
        return dict(body=GHOST, belly=GHOST, acc=GHOST)

    def fade(t, to, k):
        return tuple(mix(c, rgb(to), k) for c in t)

    if stage == "elder":
        P = {k: fade(t, "#b8b3c2", 0.38) for k, t in P.items()}
    if state == "sick":
        P = dict(P, body=fade(P["body"], "#b9c9a0", 0.45))
    return P


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
    elif state == "sick":
        p["hy"] = [1, 2][f]
        p["bw"] = [0.3, 0.6][f]
        p["eyes"] = ["sad", "closed"][f]
        p["mouth"] = "sick"
        p["arms"] = "down2"
        p["fx"] = "sweat"
        p["cheek"] = 0
    elif state == "ghost":
        p["hy"] = p["by"] = [2, 1][f]
        p["eyes"] = "closed"
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
    elif style == "sick":  # a thermometer sticking out
        c.dots([(13, y + 1), (14, y), (15, y + 1), (16, y)], MOUTH)
        c.dots([(17, y), (18, y), (19, y + 1), (20, y + 1)], WHITE)
        c.dots([(21, y + 2), (22, y + 2)], rgb("#ff4a5a"))


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
        c.put(*c.pt(26.5, 5.5 + hy), SWEAT[1], c._part(True))
    elif kind == "tear":
        c.ellipse(10.5, 19.5 + hy, 1.2, 1.8, SWEAT)


def pet_frame(sp, state, f, stage="adult"):
    if state == "egg":
        return egg_frame(sp, f), (16, 9)
    ghost = state == "ghost"
    P = palette(sp, stage, state)
    hs, hd, bs = SHAPE["adult" if ghost else stage]
    small = hs != 1  # shrunk heads keep blush and freckles on the head
    c = Canvas()
    p = pose(state, f)
    hy, by = p["hy"], p["by"]
    body, belly = P["body"], P["belly"]
    HEAD, BODY = (hs, 16, 13 + hy, hd), (bs, 16, 30.5, 0)
    c.xf = BODY
    if ghost:  # no feet, no tail, a halo
        c.ring(16, hy - 0.2, 6.5, 1.9, 1.1, tones("#fffbd0", "#ffe45c", "#f0b429"))
    else:
        tail(c, sp, P, by)
        fw = 3.6 if sp == "frog" else 3.0
        c.ellipse(16 - 4.5 - (0.6 if sp == "frog" else 0), 28.6 - p["fl"], fw, 1.9, body)
        c.ellipse(16 + 4.5 + (0.6 if sp == "frog" else 0), 28.6 - p["fr"], fw, 1.9, body)
    c.ellipse(16, 23.5 + by + p["bh"] * 0.5, 6.6 + p["bw"], 5.4 + p["bh"], body)
    c.ellipse(16, 24.6 + by, 4.2 + p["bw"] * 0.6, 3.4, belly, edge=False)
    arms(c, p["arms"], body, by, front=False)
    c.xf = HEAD
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
        c.dots([(7, 9 + hy), (8, 9 + hy), (24, 9 + hy), (23, 10 + hy), (22, 20 + by), (10, 21 + by)], P["acc"][1], over=small)
    if sp == "frog":
        c.dots([(8, 14 + hy), (9, 15 + hy), (23, 14 + hy), (22, 15 + hy), (15, 9 + hy), (17, 8 + hy)], P["acc"][1], over=small)
    ey = 11 + hy
    if sp == "frog":
        ey = 4 + hy
        for s in (-1, 1):
            c.ellipse(16 + s * 6.5, 6.5 + hy, 3.4, 3.1, (WHITE, WHITE, rgb("#dde8f5")), edge=False)
        ex = (8, 20)
    else:
        ex = (9, 19)
    # eyes stay whole pixel blocks whatever the head's size
    (lx, ey), (rx, _) = c.block(ex[0], ey, 4, 5), c.block(ex[1], ey, 4, 5)
    c.xf = None
    eye(c, lx, ey, p["eyes"], False)
    eye(c, rx, ey, p["eyes"], True)
    if stage == "elder" and not ghost:  # bushy white brows
        c.dots([(x, ey - 2) for x in range(lx, lx + 4)] + [(lx - 1, ey - 1)], BROW, over=True)
        c.dots([(x, ey - 2) for x in range(rx, rx + 4)] + [(rx + 4, ey - 1)], BROW, over=True)
    c.xf = HEAD
    cheek = mix(P["body"][1], rgb("#ff5f9a"), 0.55)
    if p["cheek"]:
        cy = 16 + hy
        blush = [(6, cy), (7, cy), (8, cy)] if p["cheek"] == 1 else [(x, y) for x in (5, 6, 7, 8) for y in (cy, cy + 1)]
        c.dots(blush + [(31 - x, y) for x, y in blush], cheek, over=small)
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
    if stage == "elder" and not ghost:  # and a little goatee
        c.dots([(x, my + 3) for x in (14, 15, 16, 17)] + [(15, my + 4), (16, my + 4)], BROW, over=True)
    mouth(c, p["mouth"], my, sp)
    # arms raised beside the head follow the head; the rest (and what they hold) follow the body
    c.xf = BODY
    if p["prop"]:
        prop(c, p["prop"], hy)
    c.xf = HEAD if p["arms"] in ("up", "up2") else BODY
    arms(c, p["arms"], body, by, front=True)
    c.xf = HEAD
    if p["fx"]:
        fx(c, p["fx"], hy)
    top = 2 + hy if sp == "frog" else int(round(13 + hy - hry)) + 1
    head_top = (16, math.floor(c.pt(16, top)[1] + 0.5))
    img = c.image()
    if ghost:
        img.putalpha(img.getchannel("A").point(lambda a: a * 205 // 255))
    return img, head_top


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
         "LuckyClover", "SpeedBoots", "Medicine"]


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
    elif name == "Medicine":
        red = rgb("#f0434f")
        c.rect(3.5, 5, 12.5, 15, T("#ffffff", "#f0f0f6", "#c8c8d8"))
        c.rect(5.5, 1.5, 10.5, 5, T("#ff9d9d", "#f0434f", "#b52a3c"))
        c.rect(7, 7, 9, 13, red, edge=False)
        c.rect(5, 9, 11, 11, red, edge=False)
    return c.image()


# ----------------------------------------------------------------------------- need icons
# 8x8 shapes on a 10x10 canvas (room for the outline), in the vitals row's order.
NEEDS = [
    ("hp", dict(R="#e0483e", r="#a92f33", w="#ffc2b8"), [
        ".RR..RR.",
        "RwRRRRRR",
        "RwRRRRRR",
        "RRRRRRRr",
        ".RRRRRr.",
        "..RRRr..",
        "...Rr...",
    ]),
    ("energy", dict(Y="#f6c343", y="#c08a17", w="#fff1b0"), [
        "....wY..",
        "...wY...",
        "..wYY...",
        ".YYYYYY.",
        "...YYy..",
        "...Yy...",
        "..Yy....",
        "..y.....",
    ]),
    ("food", dict(A="#5dbb4f", a="#2f7a2b", w="#c8f0a8", b="#7d4a24"), [
        "....b...",
        "...b....",
        ".AA.AAA.",
        "AwAAAAAA",
        "AwAAAAAa",
        "AAAAAAAa",
        ".AAAAAa.",
        "..AA.a..",
    ]),
    ("water", dict(B="#5ac8f0", b="#2f8fc4", w="#dff6ff"), [
        "...B....",
        "...BB...",
        "..BBB...",
        "..BwBB..",
        ".BwBBBB.",
        ".BBBBBb.",
        ".BBBBbb.",
        "..BBbb..",
    ]),
    ("mood", dict(P="#ff8fc7", p="#d9609a", d="#3b2112"), [
        "..PPPP..",
        ".PPPPPP.",
        "PPdPPdPP",
        "PPdPPdPP",
        "PPPPPPPP",
        "PdPPPPdp",
        ".PddddP.",
        "..PPpp..",
    ]),
]


def need_icon(rows, pal):
    c = Canvas(10, 10)
    c.art(1, 1, rows, {k: rgb(v) for k, v in pal.items()})
    return c.image()


# ----------------------------------------------------------------------------- world
# Top-down places on a 16 px tile grid, seen a little from above (3/4 view): the town island you
# walk around and the buildings you walk into. Each place's ground (grass, water, floors, walls) is
# baked into places.png. Props (buildings, trees, furniture) are sprites in props.png that the game
# sorts with the pets by their feet, so a pet can walk behind a tree or a bookshelf. meta.json gets
# each place's walk mask, doors, props and job slots.
TILE = 16
BAYER = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]]
T = tones
WOOD = T("#d9a46c", "#bb8650", "#92653a")
DARKWOOD = T("#8a5c3e", "#6b4630", "#4f3324")
METAL = T("#d4d9e2", "#a3aab8", "#747b8c")
GLASS = T("#e6f8ff", "#9fd8f4", "#6aaed8")
TRIM = rgb("#4a3328")
WARM = "#ffcf8a"


def shades(h):
    """Highlight, base and shadow from one color."""
    b = rgb(h)
    return (mix(b, WHITE, 0.3), b, mix(b, INK, 0.3))


def dq(t, x, y, steps):
    """Ordered-dither quantize t in 0..1 to `steps` levels."""
    return max(0, min(steps, math.floor(t * steps + (BAYER[y % 4][x % 4] + 0.5) / 16))) / steps


def hsh(x, y, seed=0):
    """Deterministic 0..1 per (x, y)."""
    n = (x * 374761393 + y * 668265263 + seed * 1442695041) & 0xFFFFFFFF
    n = ((n ^ (n >> 13)) * 1274126177) & 0xFFFFFFFF
    return ((n ^ (n >> 16)) & 0xFFFF) / 65535


def vnoise(x, y, cell, seed=0):
    """Smooth value noise in -1..1, blobs about `cell` px across."""
    gx, gy = x / cell, y / cell
    i, j = math.floor(gx), math.floor(gy)
    fx, fy = gx - i, gy - j
    sx, sy = fx * fx * (3 - 2 * fx), fy * fy * (3 - 2 * fy)
    a, b, c, d = (hsh(i + u, j + v, seed) for u, v in ((0, 0), (1, 0), (0, 1), (1, 1)))
    top, bot = a + (b - a) * sx, c + (d - c) * sx
    return (top + (bot - top) * sy) * 2 - 1


def tone3(t, x, y, n):
    """Highlight, base or shadow of `t`, dithered by n in -1..1."""
    return t[min(2, int(dq(0.5 - n * 0.5, x, y, 2) * 2))]


def glow(img, cx, cy, r, color, amt, sy=1.0):
    px = img.load()
    col = rgb(color)
    for y in range(max(0, int(cy - r * sy)), min(img.height, int(cy + r * sy) + 1)):
        for x in range(max(0, int(cx - r)), min(img.width, int(cx + r) + 1)):
            d = math.hypot(x + 0.5 - cx, (y + 0.5 - cy) / sy) / r
            if d < 1:
                lvl = dq((1 - d) ** 1.6, x, y, 4) * amt
                if lvl > 0:
                    px[x, y] = mix(px[x, y], col, lvl)


def bookshelf(c, rnd, x0, x1, y0=8, y1=66):
    c.rect(x0, y0, x1, y1, DARKWOOD)
    books = ["#e85a6a", "#5a8ae0", "#5cb85a", "#f0b84a", "#8e6ad8", "#e88a4a", "#4ab8b0"]
    for sy in range(y0 + 3, y1 - 6, 13):
        c.rect(x0 + 2, sy, x1 - 2, sy + 10, rgb("#221a18"), edge=False)
        x = x0 + 3
        while x < x1 - 5:
            w, h = rnd.randint(2, 3), rnd.randint(6, 10)
            b = rgb(rnd.choice(books))
            c.rect(x, sy + 10 - h, x + w, sy + 10, (mix(b, WHITE, 0.15), b, mix(b, INK, 0.3)), edge=False)
            x += w + (2 if rnd.random() < 0.15 else 0)


GRASS = T("#a2e274", "#82cd5e", "#64b04d")
SAND = T("#fff0c0", "#f4dc9c", "#e0c07a")
COBBLE = T("#f0e2bc", "#dccaa0", "#bea67c")
PLANK = T("#e0aa70", "#c48c56", "#9c6a3e")


def grass(x, y):
    c = tone3(GRASS, x, y, vnoise(x, y, 28, 1) * 0.8 + vnoise(x, y, 7, 2) * 0.45)
    if hsh(x, y, 3) < 0.015:
        return GRASS[0]
    return GRASS[2] if hsh(x, y, 4) < 0.01 else c


def sand(x, y):
    c = tone3(SAND, x, y, vnoise(x, y, 9, 6) * 0.7)
    return SAND[2] if hsh(x, y, 7) < 0.02 else c


def water(x, y, depth):
    c = mix(rgb("#6fcaf0"), rgb("#2c6fc0"), dq(min(1.0, depth), x, y, 4))
    if y % 6 == 0 and (x + int(4 * math.sin(y * 0.37))) % 23 < 4:  # wave crests
        c = mix(c, WHITE, 0.4)
    return c


def cobble(x, y):
    c = tone3(COBBLE, x, y, vnoise(x, y, 5, 5) * 0.9)
    if y % 5 == 4 or (x + (y // 5 % 2) * 4) % 8 == 7:
        c = mix(c, INK, 0.16)
    return c


def planks(t, across=True):
    """Floorboards; `across` runs them left-right."""
    def f(x, y):
        a, b = (y, x) if across else (x, y)
        row = a // 6
        c = tone3(t, x, y, vnoise(b, a + row * 40, 24, 9) * 0.7)
        if a % 6 == 5 or (b + row * 29) % 46 == 0:
            c = mix(c, INK, 0.2)
        return c
    return f


def checker(a, b):
    a, b = rgb(a), rgb(b)

    def f(x, y):
        c = a if (x // 8 + y // 8) % 2 == 0 else b
        return mix(c, INK, 0.12) if x % 8 == 7 or y % 8 == 7 else c
    return f


def mats(base):
    b = rgb(base)

    def f(x, y):
        c = mix(b, WHITE, 0.08) if hsh(x, y, 13) < 0.06 else b
        return mix(c, INK, 0.25) if x % 16 == 0 or y % 16 == 0 else c
    return f


def flagstones(t):
    def f(x, y):
        c = tone3(t, x, y, vnoise(x, y, 6, 14) * 0.8)
        return mix(c, INK, 0.25) if y % 10 == 9 or (x + (y // 10 % 2) * 7) % 14 == 13 else c
    return f


def dirt(t):
    def f(x, y):
        return tone3(t, x, y, vnoise(x, y, 10, 15) * 0.7 + vnoise(x, y, 3, 16) * 0.3)
    return f


def wallpaper(base, stripe=None):
    b, s = rgb(base), rgb(stripe or base)

    def f(x, y):
        if y >= 2 * TILE - 5:  # skirting board
            return mix(b, INK, 0.5 if y == 2 * TILE - 5 else 0.32)
        c = s if (x // 4) % 3 == 0 else b
        return mix(c, INK, dq(max(0.0, 0.3 - y / 70), x, y, 3))  # a touch darker under the ceiling
    return f


def shadow(img, cx, cy, rx, ry, amt=0.22):
    px = img.load()
    for y in range(int(cy - ry), int(cy + ry) + 1):
        for x in range(int(cx - rx), int(cx + rx) + 1):
            if 0 <= x < img.width and 0 <= y < img.height and ((x + 0.5 - cx) / rx) ** 2 + ((y + 0.5 - cy) / ry) ** 2 <= 1:
                px[x, y] = mix(px[x, y], INK, amt)


def wall_window(c, x, y, w, h):
    frame = T("#fffaf2", "#ece2d2", "#c8b8a2")
    c.rect(x, y, x + w, y + h, frame)
    c.rect(x + 2, y + 2, x + w - 2, y + h - 2, GLASS, edge=False)
    c.rect(x + w / 2 - 0.5, y + 2, x + w / 2 + 0.5, y + h - 2, frame, edge=False)
    c.dots([(x + 4, y + 5), (x + 5, y + 4), (x + w / 2 + 4, y + 5), (x + w / 2 + 5, y + 4)], WHITE)


def rug(c, x0, y0, x1, y1, edge, inner):
    c.rect(x0, y0, x1, y1, shades(edge))
    c.rect(x0 + 3, y0 + 3, x1 - 3, y1 - 3, shades(inner), edge=False)


def picture(c, x, y, w, h, color):
    c.rect(x, y, x + w, y + h, DARKWOOD)
    c.rect(x + 2, y + 2, x + w - 2, y + h - 2, shades(color), edge=False)
    c.poly([(x + 2, y + h - 2), (x + w * 0.4, y + h * 0.45), (x + w - 2, y + h - 2)], shades("#5cb85a"), edge=False)


# ---- props: drawn bottom-centre on the feet (the bottom edge of the tiles they stand on)

def tree(kind):
    c = Canvas(34, 46)
    c.rect(14, 30, 20, 44, T("#b07a4a", "#8a5a34", "#663f22"))
    if kind == 0:
        leaf = T("#a6ec78", "#6cc24e", "#4a9a3e")
        for x, y, r in ((17, 20, 12), (9, 25, 8), (25, 25, 8), (17, 11, 9)):
            c.ellipse(x, y, r, r * 0.9, leaf)
        c.dots([(12, 14), (22, 19), (15, 26)], rgb("#ff6a7a"))
    else:  # pine
        leaf = T("#7ee0a0", "#3faa70", "#2a7c52")
        for y0, hw in ((2, 6), (10, 10), (18, 13)):
            c.poly([(17, y0), (17 - hw, y0 + 15), (17 + hw, y0 + 15)], leaf)
    return c.image()


def bush():
    c = Canvas(22, 16)
    leaf = T("#a6ec78", "#6cc24e", "#4a9a3e")
    for x, y, r in ((7, 9, 5.5), (15, 9, 5.5), (11, 6, 5)):
        c.ellipse(x, y, r, r * 0.85, leaf)
    c.dots([(8, 6), (14, 8)], rgb("#ffd1e6"))
    return c.image()


def rock():
    c = Canvas(20, 14)
    c.ellipse(10, 8, 8, 5, T("#e2e0ea", "#b8b4c6", "#8a86a0"))
    c.ellipse(7, 6, 3, 2, T("#f4f2f8", "#d8d4e4", "#b0aac4"), edge=False)
    return c.image()


def lamp_post():
    c = Canvas(12, 38)
    pole = T("#5a5f70", "#3f4454", "#2c303c")
    c.rect(5, 8, 7, 36, pole)
    c.rect(3, 33, 9, 36, pole)
    c.ellipse(6, 6, 4.5, 4.5, T("#fffbe0", "#ffe08a", "#f0b84a"))
    return c.image()


def fountain():
    c = Canvas(50, 42)
    stone = T("#f2f0f6", "#d2d0de", "#a8a6ba")
    c.ellipse(25, 30, 23, 10, stone)
    c.ellipse(25, 29, 19, 7, T("#b8ecff", "#6cc6f0", "#3a96d0"), edge=False)
    c.rect(22, 10, 28, 29, stone)
    c.ellipse(25, 10, 7, 3, stone)
    c.dots([(18, 6), (16, 9), (32, 6), (34, 9), (25, 3), (21, 16), (29, 16)], rgb("#b8ecff"))
    return c.image()


def apartment():
    c = Canvas(82, 118)
    c.rect(3, 10, 79, 32, T("#c8ccd8", "#a8aebe", "#8a90a2"))  # flat roof, seen from above
    c.rect(56, 13, 70, 24, METAL)  # the air conditioner
    c.rect(3, 30, 79, 116, T("#fff4dc", "#f6e2bc", "#dcc092"))
    c.rect(1, 28, 81, 33, T("#e8ecf4", "#cdd2de", "#a8aebe"))
    sill = T("#fffaf2", "#ece2d2", "#c8b8a2")
    for row in range(3):
        for col in range(3):
            if row == 2 and col == 1:
                continue
            x, y = 9 + col * 24, 40 + row * 25
            c.rect(x, y, x + 16, y + 15, sill)
            c.rect(x + 2, y + 2, x + 14, y + 13, GLASS, edge=False)
            c.rect(x - 1, y + 15, x + 17, y + 17, sill)
    c.poly([(28, 94), (54, 94), (50, 88), (32, 88)], shades("#f06a5a"))  # awning
    c.rect(33, 94, 49, 116, shades("#4f86e0"))
    c.rect(40.5, 96, 41.5, 116, rgb("#2f5aa0"), edge=False)
    return c.image()


def library_b():
    c = Canvas(114, 104)
    stone = T("#f4f0e8", "#dcd6ca", "#b8b0a2")
    c.rect(6, 40, 108, 100, stone)
    c.poly([(2, 42), (57, 6), (112, 42)], shades("#5f86d8"))  # pediment
    c.poly([(18, 37), (57, 14), (96, 37)], stone, edge=False)
    c.ellipse(57, 28, 6, 6, shades("#ffcc33"))
    for x in (12, 30, 76, 94):
        c.rect(x, 46, x + 8, 96, T("#ffffff", "#ece8e0", "#c8c0b2"))
    c.rect(46, 62, 68, 98, DARKWOOD)
    c.rect(50, 66, 64, 76, GLASS, edge=False)
    c.rect(4, 96, 110, 102, stone)  # steps
    return c.image()


def gym_b():
    c = Canvas(114, 96)
    brick = shades("#e0664a")
    c.rect(4, 8, 110, 28, T("#8a8fa8", "#62677e", "#474b5e"))
    c.rect(4, 26, 110, 94, brick)
    c.dots([(x, y) for y in range(32, 92, 6) for x in range(8 + (y // 6 % 2) * 6, 108, 12)], mix(brick[1], INK, 0.25))
    for x in (10, 70):
        c.rect(x, 38, x + 34, 66, METAL)
        c.rect(x + 2, 40, x + 32, 64, GLASS, edge=False)
    c.rect(46, 66, 68, 94, METAL)
    c.rect(48, 68, 56.5, 94, GLASS, edge=False)
    c.rect(57.5, 68, 66, 94, GLASS, edge=False)
    c.rect(36, 1, 78, 13, T("#ffffff", "#f0f2fa", "#d4d8e8"))  # the sign on the roof
    c.rect(46, 6, 68, 8, METAL, edge=False)
    for x in (43, 66):
        c.rect(x, 3, x + 5, 11, shades("#3f4454"), edge=False)
    return c.image()


def shop_b():
    c = Canvas(82, 100)
    c.poly([(2, 34), (12, 6), (70, 6), (80, 34)], shades("#f08840"))
    c.rect(4, 32, 78, 98, shades("#f0b878"))
    for i in range(8):
        x = 4 + i * 9.25
        c.rect(x, 46, x + 9.25, 56, shades("#f05a5a" if i % 2 == 0 else "#fff6f0"))
    c.rect(24, 36, 58, 44, DARKWOOD)
    c.ellipse(41, 40, 3, 3, shades("#ffcc33"))
    c.rect(8, 62, 28, 84, GLASS)
    c.ellipse(14, 80, 3, 2.5, shades("#f0434f"))
    c.ellipse(21, 80, 3, 2.5, shades("#a05aff"))
    c.rect(33, 64, 49, 98, shades("#6cbf4a"))
    c.rect(56, 62, 74, 84, GLASS)
    return c.image()


def arena_b():
    c = Canvas(114, 108)
    stone = T("#f0dcc0", "#d8bc98", "#b49674")
    c.ellipse(57, 50, 54, 44, stone)
    c.ellipse(57, 40, 42, 24, T("#ffe9b0", "#f4d48a", "#dcb468"), edge=False)  # the sand, over the rim
    c.rect(3, 50, 111, 106, stone)
    dark = shades("#6a4a3a")
    for i in range(5):
        x = 8 + i * 21
        if i != 2:
            c.rect(x, 66, x + 12, 88, dark)
            c.ellipse(x + 6, 66, 6, 4, dark)
    c.rect(48, 76, 66, 106, dark)
    c.ellipse(57, 76, 9, 6, dark)
    for x, col in ((14, "#e0483e"), (100, "#5a8ae0")):
        c.rect(x, 2, x + 1, 30, METAL)
        c.poly([(x + 1, 3), (x + 12, 7), (x + 1, 12)], shades(col))
    return c.image()


def shrine():
    c = Canvas(82, 100)
    stone = T("#ece6ff", "#bdb4dc", "#8e84b4")
    c.rect(4, 84, 78, 98, stone)
    for x in (8, 64):
        c.rect(x, 32, x + 10, 86, stone)
    c.poly([(2, 36), (41, 4), (80, 36)], shades("#6a4ad8"))
    c.ellipse(41, 22, 4, 4, shades("#ff6ad8"))
    c.ring(41, 62, 20, 24, 4, stone, clip=lambda x, y: y < 86)
    swirl_disc(c, 41, 62, 16, 20, 86)
    return c.image()


def swirl_disc(c, cx, cy, rx, ry, below):
    """The portal's swirl, an ellipse cut off at y `below`."""
    p = c._part(False)
    swirl = [rgb(h) for h in ("#1d1540", "#3a2a8a", "#6048d0", "#44c6d6", "#b8f4ff")]
    for y in range(int(cy - ry), min(below, int(cy + ry) + 1)):
        for x in range(int(cx - rx), int(cx + rx) + 1):
            u, v = (x + 0.5 - cx) / rx, (y + 0.5 - cy) / ry
            r = math.hypot(u, v)
            if r <= 1:
                band = (math.atan2(v, u) / (2 * math.pi) * 3 + r * 2.2) % 1
                c.put(x, y, swirl[min(4, int(dq(band * 0.8 + (1 - r) * 0.5, x, y, 4) * 4))], p)


def portal_arch():
    c = Canvas(82, 62)
    stone = T("#d8d0f0", "#aaa0cc", "#7e74a4")
    c.rect(4, 50, 78, 60, stone)
    c.ring(41, 34, 30, 30, 6, stone, clip=lambda x, y: y < 52)
    swirl_disc(c, 41, 34, 24, 24, 52)
    c.ellipse(41, 6, 3, 3, shades("#ff6ad8"))
    return c.image()


def candle():
    c = Canvas(10, 20)
    c.rect(3, 8, 7, 18, shades("#fff4dc"))
    c.ellipse(5, 5, 2, 3, T("#fffbd0", "#ffcc33", "#ff8a3a"))
    return c.image()


def crystal():
    c = Canvas(16, 22)
    t = shades("#b07aff")
    c.poly([(8, 1), (12, 8), (10, 20), (6, 20), (4, 8)], t)
    c.poly([(3, 10), (6, 13), (5, 20), (2, 20), (1, 14)], t)
    return c.image()


def bed():
    c = Canvas(34, 58)
    c.rect(2, 2, 32, 14, DARKWOOD)  # headboard, against the wall
    c.rect(2, 10, 32, 56, WOOD)
    c.rect(4, 12, 30, 54, T("#ffffff", "#f2eef8", "#d4cce4"))
    c.rect(7, 13, 27, 22, T("#ffffff", "#f6f4fa", "#dcd6e8"))
    c.rect(4, 28, 30, 54, shades("#6e9cf0"))
    c.rect(4, 28, 30, 31, shades("#a4c4ff"), edge=False)
    return c.image()


def nightstand():
    c = Canvas(18, 30)
    c.rect(2, 16, 16, 28, WOOD)
    c.rect(4, 21, 14, 22, DARKWOOD[2], edge=False)
    c.rect(8, 8, 10, 16, METAL)
    c.poly([(3, 9), (15, 9), (12, 2), (6, 2)], T("#fff2c8", "#ffd98a", "#e6b45e"))
    return c.image()


def sofa():
    c = Canvas(50, 32)
    cloth, cush = shades("#f07e6e"), shades("#ff9e8e")
    c.rect(4, 2, 46, 18, cloth)
    c.rect(2, 14, 48, 30, cloth)
    c.rect(1, 8, 9, 30, cloth)
    c.rect(41, 8, 49, 30, cloth)
    c.rect(10, 12, 25, 22, cush)
    c.rect(25, 12, 40, 22, cush)
    return c.image()


def fridge():
    c = Canvas(18, 42)
    c.rect(1, 2, 17, 40, T("#ffffff", "#e6ecf4", "#b8c2d2"))
    c.rect(2, 15, 16, 16, rgb("#9aa4b4"), edge=False)
    c.rect(13, 6, 14, 12, METAL, edge=False)
    c.rect(13, 19, 14, 30, METAL, edge=False)
    c.dots([(5, 6), (8, 8), (6, 22)], rgb("#ff6a7a"))
    c.dots([(9, 5), (5, 25)], rgb("#5ac8f0"))
    return c.image()


def stove():
    c = Canvas(34, 30)
    c.rect(1, 10, 33, 28, T("#fff8ee", "#ece2d2", "#c8b8a2"))
    c.rect(1, 4, 33, 11, T("#d0d6e0", "#aab2c0", "#848c9c"))
    c.ring(9, 7.5, 4, 2.2, 1, rgb("#4a4e5c"), edge=False)
    c.rect(5, 14, 29, 25, shades("#3f4454"))
    c.rect(8, 17, 26, 22, rgb("#ffb070"), edge=False)
    c.ellipse(23, 5, 5, 3.5, shades("#f0434f"))
    return c.image()


def cooler():
    c = Canvas(16, 34)
    c.rect(2, 16, 14, 32, T("#ffffff", "#e8ecf4", "#b8c2d2"))
    c.rect(3, 4, 13, 16, T("#c8ecff", "#7cc4f0", "#4f9ad0"))
    c.ellipse(8, 4, 5, 2.4, T("#c8ecff", "#7cc4f0", "#4f9ad0"))
    c.dots([(5, 21), (6, 21)], rgb("#5ac8f0"))
    c.dots([(10, 21), (11, 21)], rgb("#e0483e"))
    return c.image()


def plant():
    c = Canvas(18, 30)
    c.rect(4, 18, 14, 28, shades("#e07a50"))
    leaf = T("#a6ec78", "#5cb84a", "#3f8f3c")
    for x, y, rx, ry in ((9, 10, 4, 7), (5, 13, 3.5, 5), (13, 13, 3.5, 5)):
        c.ellipse(x, y, rx, ry, leaf)
    return c.image()


def table():
    c = Canvas(34, 26)
    c.rect(6, 12, 9, 24, DARKWOOD)
    c.rect(25, 12, 28, 24, DARKWOOD)
    c.ellipse(17, 11, 15, 7, WOOD)
    c.rect(20, 4, 25, 10, T("#ffffff", "#f0f0f6", "#c8c8d8"))
    c.ellipse(11, 10, 3, 2, shades("#f0434f"))
    return c.image()


def books_shelf(seed):
    c = Canvas(34, 46)
    bookshelf(c, random.Random(seed), 1, 33, 2, 44)
    return c.image()


def desk():
    c = Canvas(34, 30)
    c.rect(1, 12, 33, 28, WOOD)
    c.rect(1, 7, 33, 13, T("#f6d2a0", "#e0b47e", "#c0925c"))
    c.rect(14, 8, 22, 11, T("#ffffff", "#fffaf0", "#ecdcc0"), edge=False)
    c.rect(4, 4, 11, 9, shades("#5a8ae0"))
    c.rect(5, 2, 10, 5, shades("#e85a6a"))
    c.rect(27, 3, 28, 9, METAL)
    c.poly([(24, 4), (31, 4), (29, 1), (26, 1)], T("#fff2c8", "#ffd98a", "#e6b45e"))
    return c.image()


def floor_lamp():
    c = Canvas(14, 36)
    c.rect(6, 10, 8, 33, METAL)
    c.ellipse(7, 33, 4, 1.6, METAL)
    c.poly([(2, 11), (12, 11), (10, 2), (4, 2)], T("#fff2c8", "#ffd98a", "#e6b45e"))
    return c.image()


def rack():
    c = Canvas(34, 32)
    c.rect(2, 8, 4, 30, METAL)
    c.rect(30, 8, 32, 30, METAL)
    for y in (13, 23):
        c.rect(2, y, 32, y + 2, METAL)
        for i, col in enumerate(("#ff6a5a", "#5a8ae0", "#ffcc33", "#5cb85a")):
            x = 6 + i * 6.5
            c.rect(x, y - 4, x + 2, y + 2, shades(col))
            c.rect(x + 3, y - 4, x + 5, y + 2, shades(col))
    return c.image()


def platform():
    c = Canvas(34, 34)
    c.rect(1, 6, 33, 33, shades("#6a7084"))
    c.rect(5, 10, 29, 29, shades("#c89a62"), edge=False)
    c.rect(2, 1, 4, 10, METAL)
    c.rect(30, 1, 32, 10, METAL)
    return c.image()


def treadmill():
    c = Canvas(30, 36)
    frame = shades("#5a5f70")
    c.rect(3, 8, 27, 35, frame)
    c.rect(7, 11, 23, 33, shades("#2c303c"), edge=False)
    for y in range(13, 33, 4):
        c.rect(7, y, 23, y + 0.8, rgb("#4a4f60"), edge=False)
    c.rect(1, 6, 4, 22, METAL)
    c.rect(26, 6, 29, 22, METAL)
    c.rect(5, 1, 25, 8, frame)
    c.rect(9, 3, 21, 6, rgb("#3fd07a"), edge=False)
    return c.image()


def punching_bag():
    c = Canvas(16, 42)
    c.rect(7.5, 1, 8.5, 10, METAL)
    c.rect(3, 9, 13, 38, shades("#e0483e"))
    c.ellipse(8, 9, 5, 2, shades("#e0483e"))
    c.rect(3, 15, 13, 17, shades("#2c303c"), edge=False)
    return c.image()


def ring():
    c = Canvas(130, 84)
    c.rect(2, 8, 128, 82, shades("#5a8ae0"))
    c.rect(8, 12, 122, 76, T("#ffffff", "#f0f2fa", "#d4d8e8"), edge=False)
    for x0, y0 in ((4, 2), (122, 2), (4, 64), (122, 64)):
        c.rect(x0, y0, x0 + 5, y0 + 16, shades("#e0483e"))
    for y in (5, 9):
        c.rect(9, y, 122, y + 1, rgb("#ffcc33"), edge=False)
    return c.image()


def bleachers():
    c = Canvas(50, 28)
    c.rect(1, 2, 49, 13, WOOD)
    c.rect(1, 12, 49, 26, shades("#c8935e"))
    c.rect(1, 12, 49, 13, DARKWOOD[1], edge=False)
    return c.image()


def shop_shelf(seed):
    c = Canvas(50, 46)
    c.rect(2, 2, 48, 44, WOOD)
    rnd = random.Random(seed)
    for sy in (15, 29, 43):
        c.rect(3, sy - 1, 47, sy + 1, DARKWOOD, edge=False)
        x = 5
        while x < 42:
            col = shades(rnd.choice(["#e85a6a", "#5a8ae0", "#5cb85a", "#f0b84a", "#8e6ad8", "#e88a4a"]))
            if rnd.random() < 0.5:
                c.ellipse(x + 2.5, sy - 4, 3, 3, col)
            else:
                c.rect(x, sy - 9, x + 5, sy - 1, col)
            x += rnd.randint(7, 9)
    return c.image()


def counter():
    c = Canvas(66, 32)
    c.rect(1, 12, 65, 30, WOOD)
    c.rect(1, 7, 65, 13, T("#f6d2a0", "#e0b47e", "#c0925c"))
    c.rect(44, 1, 58, 10, shades("#4a4e5c"))
    c.rect(46, 3, 56, 6, rgb("#3fd07a"), edge=False)
    c.ellipse(14, 8, 4, 2.5, shades("#ffcc33"))
    c.ellipse(22, 9, 3, 2, shades("#ffcc33"))
    return c.image()


def display():
    c = Canvas(34, 24)
    c.rect(3, 12, 6, 22, DARKWOOD)
    c.rect(28, 12, 31, 22, DARKWOOD)
    c.rect(1, 8, 33, 14, WOOD)
    for x, col in ((9, "#ff4a6a"), (17, "#a05aff"), (25, "#5cb85a")):
        c.ellipse(x, 6, 3, 3.4, shades(col))
    return c.image()


BUILDINGS = {"apartment": apartment, "library": library_b, "gym": gym_b, "shop": shop_b, "arena": arena_b, "shrine": shrine}
SPRITES, SPRITE_IDS = [], {}


def sprite_id(name, fn, args=()):
    """Index of sprite `name` in props.png, drawing it with fn(*args) the first time."""
    if name not in SPRITE_IDS:
        SPRITE_IDS[name] = len(SPRITES)
        SPRITES.append(fn(*args))
    return SPRITE_IDS[name]


def put(pl, name, fn, tx, ty, fw=1, fh=1, act=None, stand=None, low=False, block=True, args=()):
    """Stand prop `name` (drawn by `fn`) on tiles tx..tx+fw, ty..ty+fh. `low` ones (beds, mats) go
    under every pet; the rest sort with the pets by their feet. Clicking a prop with an `act`
    walks the pet to `stand` (by default just in front of it) and then does it."""
    i = sprite_id(name, fn, args)
    img = SPRITES[i]
    feet = [(tx + fw / 2) * TILE, (ty + fh) * TILE]
    if block:
        for y in range(ty, ty + fh):
            for x in range(tx, tx + fw):
                pl["walk"][y][x] = False
    hot = None
    if act:
        hot = [feet[0] - img.width / 2, feet[1] - img.height, img.width, img.height]
        stand = stand or [feet[0], feet[1] + TILE / 2]
    pl["props"].append(dict(sprite=i, feet=feet, low=low, hot=hot, act=act, stand=stand))
    return feet


def tile_dist(w, h, solid):
    """Steps (8-way) from every tile to the nearest one in `solid`."""
    d = [[99] * w for _ in range(h)]
    q = [(x, y) for x, y in solid if 0 <= x < w and 0 <= y < h]
    for x, y in q:
        d[y][x] = 0
    i = 0
    while i < len(q):
        x, y = q[i]
        i += 1
        for dy in (-1, 0, 1):
            for dx in (-1, 0, 1):
                nx, ny = x + dx, y + dy
                if 0 <= nx < w and 0 <= ny < h and d[ny][nx] > d[y][x] + 1:
                    d[ny][nx] = d[y][x] + 1
                    q.append((nx, ny))
    return d


def sample(grid, x, y):
    """`grid` (one value per tile centre) read smoothly at pixel (x, y)."""
    gx, gy = x / TILE - 0.5, y / TILE - 0.5
    i, j = math.floor(gx), math.floor(gy)
    fx, fy = gx - i, gy - j
    h, w = len(grid), len(grid[0])

    def g(a, b):
        return grid[min(h - 1, max(0, b))][min(w - 1, max(0, a))]
    top = g(i, j) + (g(i + 1, j) - g(i, j)) * fx
    bot = g(i, j + 1) + (g(i + 1, j + 1) - g(i, j + 1)) * fx
    return top + (bot - top) * fy


def build_town():
    w, h = 60, 40
    cx, cy, rx, ry = 30, 20, 28.5, 18
    plaza = (26, 17, 9, 7)
    # (place, sprite, footprint x, y, w, h in tiles); the door is the tile under the middle of the footprint
    buildings = [("lobby", "apartment", 10, 8, 5, 4),("library", "library", 27, 6, 7, 4), ("portal", "shrine", 44, 7, 5, 4),
                 ("gym", "gym", 44, 17, 7, 4), ("shop", "shop", 38, 27, 5, 4), ("arena", "arena", 13, 25, 7, 5)]
    roads = [[(12, 12), (12, 20), (26, 20)], [(30, 10), (30, 17)], [(46, 11), (46, 14), (35, 14), (35, 17)],
             [(47, 21), (47, 22), (35, 22)], [(40, 31), (40, 33), (30, 33), (30, 24)], [(16, 30), (16, 33), (30, 33)]]
    pier = {(x, y) for x in range(29, 32) for y in range(34, 40)}
    path = {(x, y) for x in range(plaza[0], plaza[0] + plaza[2]) for y in range(plaza[1], plaza[1] + plaza[3])}
    for pts in roads:  # three tiles wide
        for (x0, y0), (x1, y1) in zip(pts, pts[1:]):
            path |= {(x, y) for x in range(min(x0, x1) - 1, max(x0, x1) + 2) for y in range(min(y0, y1) - 1, max(y0, y1) + 2)}
    solid = path | pier | {(x, y) for _, _, bx, by, bw, bh in buildings for x in range(bx, bx + bw) for y in range(by - 2, by + bh)}
    dist = tile_dist(w, h, solid)
    lift = [[0.45 * max(0.0, 1 - d / 4) for d in row] for row in dist]  # keeps everything built on dry land

    def field(x, y):
        """Above 0 is land: an island, a bit ragged, raised wherever there's something built."""
        u, v = abs(x / TILE - cx) / rx, abs(y / TILE - cy) / ry
        return 1 - u ** 3 - v ** 3 + 0.12 * vnoise(x, y, 70, 11) + 0.05 * vnoise(x, y, 23, 12) + sample(lift, x, y)

    img = Image.new("RGBA", (w * TILE, h * TILE))
    px = img.load()
    walk = [[False] * w for _ in range(h)]
    deck = planks(PLANK)
    for y in range(h * TILE):
        for x in range(w * TILE):
            t = (x // TILE, y // TILE)
            f = field(x, y)
            if t in pier:
                c = deck(x, y)
                if (x % TILE == 0 and (t[0] - 1, t[1]) not in pier) or (x % TILE == 15 and (t[0] + 1, t[1]) not in pier):
                    c = mix(c, INK, 0.35)
            elif f < 0:
                c = mix(water(x, y, -f * 3), WHITE, 0.55) if f > -0.025 else water(x, y, -f * 3)
            elif t in path:
                c = cobble(x, y)
            else:
                c = sand(x, y) if f < 0.08 else grass(x, y)
                if any(((x + dx) // TILE, (y + dy) // TILE) in path for dx, dy in N4):  # kerb
                    c = mix(c, INK, 0.2)
            px[x, y] = c
            if x % TILE == 8 and y % TILE == 8:
                walk[t[1]][t[0]] = f >= 0 or t in pier
    pl = dict(key="town", w=w, h=h, ground=img, walk=walk, bg=list(rgb("#2c6fc0")[:3]), spawn=[12 * TILE + 8, 13 * TILE + 8],
              doors=[], props=[], slots={})
    for key, kind, bx, by, bw, bh in buildings:
        door = [bx + bw // 2, by + bh]
        feet = put(pl, kind, BUILDINGS[kind], bx, by, bw, bh, act="go:" + key, stand=[door[0] * TILE + 8, door[1] * TILE + 8])
        shadow(img, feet[0], feet[1], bw * 8 + 4, 3)
        pl["doors"].append(dict(at=door, to=key, arrive=None))
    put(pl, "fountain", fountain, 29, 19, 3, 2)
    for x, y in ((27, 18), (33, 18), (27, 22), (33, 22)):
        put(pl, "lamp", lamp_post, x, y)
    for ty in range(h):
        for tx in range(w):
            if not walk[ty][tx] or dist[ty][tx] < 2 or (tx, ty) in pier:
                continue
            f = field(tx * TILE + 8, ty * TILE + 8)
            r = hsh(tx, ty, 31)
            dense = 0.16 + 0.3 * vnoise(tx * TILE, ty * TILE, 120, 32)
            if f < 0.12:
                if r < 0.05:
                    put(pl, "rock", rock, tx, ty)
            elif r < dense:
                kind = 0 if hsh(tx, ty, 33) < 0.55 else 1
                feet = put(pl, f"tree{kind}", tree, tx, ty, args=(kind,))
                shadow(img, feet[0], feet[1] - 3, 9, 3.5)
            elif r < dense + 0.05:
                put(pl, "bush", bush, tx, ty)
            elif r > 0.9:  # flowers
                for k in range(2):
                    fx, fy = tx * TILE + 3 + int(hsh(tx, ty, 42 + k) * 10), ty * TILE + 3 + int(hsh(tx, ty, 44 + k) * 10)
                    col = rgb(("#ff8ec2", "#ffffff", "#ffd84a", "#c49aff")[int(hsh(tx, ty, 46 + k) * 3.99)])
                    for dx, dy in N4:
                        px[fx + dx, fy + dy] = col
                    px[fx, fy] = rgb("#ff8a3a")
    return pl


def room(key, w, h, floor, wall, decor=None, glows=(), exit="town"):
    """A building's inside: `floor` under a two-tile wall painted by `wall` along the top, trim on the
    other sides, and the way out (to `exit`, if any) in the middle of the bottom edge."""
    W, H = w * TILE, h * TILE
    img = Image.new("RGBA", (W, H))
    px = img.load()
    for y in range(H):
        for x in range(W):
            if y < 2 * TILE:
                c = wall(x, y)
            else:
                c = floor(x, y)
                if y < 2 * TILE + 4:  # the wall's shadow
                    c = mix(c, INK, 0.2 - (y - 2 * TILE) * 0.05)
            px[x, y] = c
    door = w // 2
    c = Canvas(W, H)
    if decor:
        decor(c, W, H)
    if exit:
        rug(c, door * TILE + 1, H - 12, (door + 1) * TILE - 1, H - 1, "#a86a4a", "#d89a6a")
    img.alpha_composite(c.image())
    for g in glows:
        glow(img, *g)
    lip = mix(TRIM, WHITE, 0.25)
    for y in range(H):
        for x in range(W):
            gap = exit and door * TILE <= x < (door + 1) * TILE
            if y < 4 or x < 5 or x >= W - 5 or (y >= H - 5 and not gap):
                inner = (y == 3 and 4 <= x < W - 4) or (x in (4, W - 5) and y >= 3) or (y == H - 5 and 4 <= x < W - 4)
                px[x, y] = lip if inner else TRIM
    walk = [[1 <= x < w - 1 and 2 <= y < h - 1 for x in range(w)] for y in range(h)]
    if exit:
        walk[h - 1][door] = True
    pl = dict(key=key, w=w, h=h, ground=img, walk=walk, bg=[42, 32, 28], spawn=[door * TILE + 8, (h - 2) * TILE + 8],
              doors=[dict(at=[door, h - 1], to=exit, arrive=None)] if exit else [], props=[], slots={})
    return pl


def elevator():
    c = Canvas(34, 42)
    c.rect(1, 6, 33, 40, METAL)
    steel = T("#f2f5fa", "#ccd3de", "#9aa4b4")
    c.rect(4, 10, 16.5, 40, steel)
    c.rect(17.5, 10, 30, 40, steel)
    c.rect(11, 1, 23, 7, shades("#3f4454"))  # the floor indicator
    c.rect(13, 3, 21, 5, rgb("#ffcc33"), edge=False)
    return c.image()


def build_lobby():
    """The apartment block's ground floor: the elevator up to everyone's homes."""
    def decor(c, W, H):
        wall_window(c, 24, 5, 40, 20)
        for i in range(8):  # mailboxes
            x, y = 152 + (i % 4) * 14, 6 + (i // 4) * 11
            c.rect(x, y, x + 12, y + 9, shades("#c8935e"))
            c.rect(x + 3, y + 3, x + 9, y + 4, DARKWOOD[2], edge=False)
        rug(c, 4 * TILE, 5 * TILE, 10 * TILE, 8 * TILE - 4, "#5a8ae0", "#9ec4ff")
    pl = room("lobby", 14, 9, checker("#fbf6ec", "#ece2d0"), wallpaper("#f2e6d6", "#e8d8c2"), decor, glows=[(112, 30, 50, WARM, 0.2)])
    put(pl, "elevator", elevator, 6, 2, 2, 1, act="elevator")
    put(pl, "counter", counter, 1, 4, 4, 1)
    put(pl, "sofa", sofa, 10, 5, 3, 1)
    put(pl, "plant", plant, 5, 2)
    put(pl, "plant", plant, 12, 7)
    pl["slots"] = {"lift": [[7 * TILE, 3 * TILE + 8]]}
    return pl


FLOOR_DOORS = (5, 9, 13, 17)  # tile columns of a floor's four apartment doors


def build_floor():
    """One floor of the apartment block, the same for every floor: a corridor with four front doors
    (whose they are depends on who's online) and the elevator."""
    def decor(c, W, H):
        for x in FLOOR_DOORS:
            x0 = x * TILE + 1
            c.rect(x0, 6, x0 + 14, 32, DARKWOOD)
            c.rect(x0 + 2, 8, x0 + 12, 32, WOOD, edge=False)
            c.ellipse(x0 + 10, 22, 1.2, 1.2, shades("#ffcc33"), edge=False)
        c.rect(3 * TILE, 2 * TILE + 3, W - 5, 5 * TILE - 3, shades("#b83a4a"))
    pl = room("floor", 20, 6, dirt(T("#f0b0a0", "#d8907e", "#b87262")), wallpaper("#ede4f6", "#e0d4ee"), decor, exit=None)
    put(pl, "elevator", elevator, 1, 2, 2, 1, act="elevator")
    for i, x in enumerate(FLOOR_DOORS):
        pl["doors"].append(dict(at=[x, 2], to=f"apt{i}", arrive=None))
    pl["slots"] = {"lift": [[2 * TILE, 3 * TILE + 8]], "apt": [[x * TILE + 8, 3 * TILE + 8] for x in FLOOR_DOORS]}
    return pl


def build_home():
    """An empty home: the game furnishes it from its owner's layout (Layout in game.rs)."""
    def decor(c, W, H):
        wall_window(c, 86, 5, 36, 20)
        picture(c, 150, 8, 18, 14, "#8ab4ff")
    pl = room("home", 14, 10, planks(T("#f2c690", "#deac74", "#c08e58")), wallpaper("#ffe6c8", "#ffd6b0"), decor,
              glows=[(104, 16, 36, WARM, 0.18)], exit="floor")
    pl["slots"] = {"egg": [[120, 104]]}
    return pl


def armchair():
    c = Canvas(20, 26)
    cloth = shades("#5a8ae0")
    c.rect(2, 2, 18, 16, cloth)
    c.rect(1, 10, 19, 24, cloth)
    c.rect(4, 12, 16, 18, shades("#8ab4ff"))
    return c.image()


def beanbag():
    c = Canvas(22, 18)
    c.ellipse(11, 10, 9.5, 6.5, shades("#ffcc33"))
    c.ellipse(11, 7, 5.5, 3, shades("#ffe08a"), edge=False)
    return c.image()


def tv():
    c = Canvas(34, 30)
    c.rect(2, 17, 32, 28, WOOD)
    c.rect(4, 2, 30, 18, shades("#3f4454"))
    c.rect(6, 4, 28, 16, T("#c8f0ff", "#5aa8e0", "#2c6fc0"), edge=False)
    c.dots([(9, 6), (10, 6), (9, 7)], WHITE)
    return c.image()


def aquarium():
    c = Canvas(34, 32)
    c.rect(2, 18, 32, 30, DARKWOOD)
    c.rect(2, 3, 32, 19, T("#c8f0ff", "#7cc8f0", "#4a96d0"))
    c.rect(3, 16, 31, 18, SAND, edge=False)
    c.poly([(7, 17), (8, 9), (10, 17)], shades("#5cb85a"), edge=False)
    c.poly([(25, 17), (27, 7), (28, 17)], shades("#5cb85a"), edge=False)
    for x, y in ((14, 8), (20, 12)):
        c.ellipse(x, y, 2.5, 1.6, shades("#ff8a3a"), edge=False)
    return c.image()


def arcade():
    c = Canvas(18, 38)
    c.rect(2, 2, 16, 36, shades("#8e6ad8"))
    c.rect(4, 7, 14, 17, T("#d0ffd8", "#3fd07a", "#2a9e5a"), edge=False)
    c.rect(3, 19, 15, 24, shades("#3f4454"), edge=False)
    c.dots([(6, 21), (10, 21), (12, 22)], rgb("#ff5a6e"))
    return c.image()


def cactus():
    c = Canvas(14, 28)
    c.rect(3, 20, 11, 26, shades("#e07a50"))
    green = shades("#5cb85a")
    c.ellipse(7, 12, 3, 9, green)
    c.ellipse(3, 11, 1.6, 3.5, green)
    c.ellipse(11, 9, 1.6, 3.5, green)
    return c.image()


def rug_prop():
    c = Canvas(82, 50)
    rug(c, 1, 1, 81, 49, "#5a8ae0", "#9ec4ff")
    return c.image()


# Furniture for homes (Furni in game.rs): sprite, its arguments, footprint in tiles, low (under the
# pets), solid (blocks walking), and where a pet stands to use it, from its feet.
FURNITURE = {
    "Bed": ("bed", bed, (), 2, 3, True, True, [0, -14]),
    "Fridge": ("fridge", fridge, (), 1, 1, False, True, [0, 8]),
    "Stove": ("stove", stove, (), 2, 1, False, True, [0, 8]),
    "Cooler": ("cooler", cooler, (), 1, 1, False, True, [0, 8]),
    "Rug": ("rug", rug_prop, (), 5, 3, True, False, [0, 8]),
    "Nightstand": ("nightstand", nightstand, (), 1, 1, False, True, [0, 8]),
    "Sofa": ("sofa", sofa, (), 3, 1, False, True, [0, 8]),
    "Table": ("table", table, (), 2, 1, False, True, [0, 8]),
    "Plant": ("plant", plant, (), 1, 1, False, True, [0, 8]),
    "Cactus": ("cactus", cactus, (), 1, 1, False, True, [0, 8]),
    "Lamp": ("floor lamp", floor_lamp, (), 1, 1, False, True, [0, 8]),
    "Armchair": ("armchair", armchair, (), 1, 1, False, True, [0, 8]),
    "Beanbag": ("beanbag", beanbag, (), 1, 1, False, True, [0, 8]),
    "Bookshelf": ("shelf0", books_shelf, (3,), 2, 1, False, True, [0, 8]),
    "Desk": ("desk", desk, (), 2, 1, False, True, [0, 8]),
    "Tv": ("tv", tv, (), 2, 1, False, True, [0, 8]),
    "Aquarium": ("aquarium", aquarium, (), 2, 1, False, True, [0, 8]),
    "Arcade": ("arcade", arcade, (), 1, 1, False, True, [0, 8]),
}


def furniture():
    return {k: dict(sprite=sprite_id(name, fn, args), size=[w, h], low=low, solid=solid, stand=stand)
            for k, (name, fn, args, w, h, low, solid, stand) in FURNITURE.items()}


def build_library():
    def decor(c, W, H):
        wall_window(c, 104, 5, 48, 20)
        rug(c, 7 * TILE + 2, 3 * TILE, 9 * TILE - 2, H - 12, "#b83a4a", "#e05a5a")
    pl = room("library", 16, 10, planks(T("#d8a878", "#c0905e", "#9a7046")), wallpaper("#e4ecf8", "#d4e0f2"), decor,
              glows=[(128, 40, 40, "#fff3c0", 0.2)])
    for i, x in enumerate((1, 3, 11, 13)):
        put(pl, f"shelf{i % 2}", books_shelf, x, 2, 2, 1, act="study", args=(3 + i % 2,))
    for x in (5, 10):
        put(pl, "floor lamp", floor_lamp, x, 2)
    study = []
    for x, y in ((3, 5), (11, 5), (3, 7), (11, 7)):
        spot = [(x + 1) * TILE, y * TILE - 4]  # behind the desk, which hides the pet's legs
        put(pl, "desk", desk, x, y, 2, 1, act="study", stand=spot)
        study.append(spot)
    put(pl, "plant", plant, 1, 8)
    put(pl, "plant", plant, 14, 8)
    pl["slots"] = {"study": study}
    return pl


def build_gym():
    def decor(c, W, H):
        c.rect(5, 17, W - 5, 21, shades("#ff6a5a"), edge=False)
        c.rect(150, 5, 240, 26, METAL)
        c.rect(152, 7, 238, 24, GLASS, edge=False)
    pl = room("gym", 16, 10, mats("#6a7084"), wallpaper("#eef2f8"), decor, glows=[(195, 16, 40, "#dfe6ff", 0.2)])
    put(pl, "rack", rack, 2, 2, 2, 1, act="lift")
    put(pl, "bag", punching_bag, 7, 2)
    lift, run = [], []
    for x in (2, 5):
        spot = [(x + 1) * TILE, 6 * TILE - 10]
        put(pl, "platform", platform, x, 4, 2, 2, act="lift", low=True, stand=spot)
        lift.append(spot)
    for x in (10, 13):
        spot = [(x + 1) * TILE, 6 * TILE - 10]
        put(pl, "treadmill", treadmill, x, 4, 2, 2, act="run", low=True, stand=spot)
        run.append(spot)
    put(pl, "plant", plant, 14, 8)
    pl["slots"] = {"lift": lift, "run": run}
    return pl


def build_portal():
    def decor(c, W, H):
        c.ring(W / 2, 6.2 * TILE, 52, 18, 2, shades("#b8a0ff"), edge=False)
    pl = room("portal", 14, 10, flagstones(T("#a89ec8", "#8c82b2", "#6e6496")), wallpaper("#5a4a88", "#4e3f7a"), decor,
              glows=[(112, 40, 64, "#7a5aff", 0.3), (112, 50, 30, "#5ae0f0", 0.15)])
    put(pl, "portal", portal_arch, 5, 2, 5, 2, act="explore", stand=[7 * TILE + 8, 5 * TILE + 8])
    for x in (3, 10):
        put(pl, "candle", candle, x, 2)
    for x in (1, 12):
        put(pl, "crystal", crystal, x, 7)
    return pl


def build_arena():
    def decor(c, W, H):
        for x, col in ((40, "#e0483e"), (200, "#5a8ae0")):
            c.poly([(x, 4), (x + 16, 4), (x + 16, 28), (x + 8, 23), (x, 28)], shades(col))
            c.ellipse(x + 8, 14, 3.5, 3.5, shades("#ffcc33"), edge=False)
    pl = room("arena", 16, 11, dirt(SAND), wallpaper("#f0dcc0", "#e8cca8"), decor)
    put(pl, "ring", ring, 4, 4, 8, 5, act="challenge", low=True, block=False, stand=[8 * TILE + 8, 9 * TILE + 8])
    for x in (1, 12):
        put(pl, "bleachers", bleachers, x, 2, 3, 1)
    pl["slots"] = {"fight": [[6 * TILE, 7 * TILE], [10 * TILE, 7 * TILE]]}
    return pl


def build_shop():
    def decor(c, W, H):
        wall_window(c, 84, 5, 56, 20)
    pl = room("shop", 14, 10, checker("#fff6e6", "#bfe8dc"), wallpaper("#e4f6ec", "#d0eedf"), decor,
              glows=[(112, 10, 60, WARM, 0.18)])
    put(pl, "shelf", shop_shelf, 1, 2, 3, 1, act="browse", args=(11,))
    put(pl, "shelf2", shop_shelf, 10, 2, 3, 1, act="browse", args=(12,))
    put(pl, "counter", counter, 5, 4, 4, 1, act="browse")
    put(pl, "display", display, 10, 6, 2, 1, act="browse")
    put(pl, "plant", plant, 1, 8)
    put(pl, "plant", plant, 12, 8)
    return pl


def build_world():
    """The town first, then the inside of every building, with the doors between them joined up."""
    town = build_town()
    rooms = [build_home(), build_lobby(), build_floor(), build_library(), build_gym(), build_portal(), build_arena(), build_shop()]
    inside = {p["key"]: p for p in rooms}
    for d in town["doors"]:
        d["arrive"] = inside[d["to"]]["spawn"]
    for p in rooms:
        for d in p["doors"]:
            if d["to"] == "town":
                x, y = next(e["at"] for e in town["doors"] if e["to"] == p["key"])
                d["arrive"] = [x * TILE + 8, (y + 1) * TILE + 8]
            elif d["to"].startswith("apt"):
                d["arrive"] = inside["home"]["spawn"]
            else:
                d["arrive"] = [0, 0]  # the game works it out from who lives where
    return [town] + rooms


# ----------------------------------------------------------------------------- main
def pack(images, width, gap=0):
    """Images left to right in rows `width` wide; returns the sheet and each one's [x, y, w, h]."""
    x = y = row = 0
    rects = []
    for im in images:
        if x and x + im.width > width:
            x, y, row = 0, y + row + gap, 0
        rects.append([x, y, im.width, im.height])
        x += im.width + gap
        row = max(row, im.height)
    out = Image.new("RGBA", (width, y + row), (0, 0, 0, 0))
    for im, r in zip(images, rects):
        out.paste(im, (r[0], r[1]))
    return out, rects


def blit(dst, src, x, y):
    """alpha_composite that may hang off the top or left edge."""
    x, y = round(x), round(y)
    l, t = max(0, -x), max(0, -y)
    dst.alpha_composite(src.crop((l, t, src.width, src.height)), (x + l, y + t))


MINI = (120, 80)  # the most room a place's minimap gets (pixels)


def composite(p):
    """A place as the game draws it: the ground, then its props back to front."""
    im = p["ground"].copy()
    for q in sorted(p["props"], key=lambda q: (not q["low"], q["feet"][1])):
        s = SPRITES[q["sprite"]]
        blit(im, s, q["feet"][0] - s.width / 2, q["feet"][1] - s.height)
    return im


def sheet(frames, cols, fw, fh):
    rows = (len(frames) + cols - 1) // cols
    img = Image.new("RGBA", (cols * fw, rows * fh), (0, 0, 0, 0))
    for i, fr in enumerate(frames):
        img.paste(fr, ((i % cols) * fw, (i // cols) * fh))
    return img


def tray_icons(pet):
    """Tray icons at 2x: the dino trimmed to its pixels, plus copies with a badge:
    gold for 'something happened', red for 'your pet needs you'."""
    body = pet.crop(pet.getbbox())
    side = max(body.size) + 2
    sq = Image.new("RGBA", (side, side), (0, 0, 0, 0))
    sq.paste(body, ((side - body.width) // 2, side - body.height - 1))
    plain = sq.resize((side * 2, side * 2), Image.NEAREST)
    plain.save(os.path.join(ROOT, "tray.png"))
    s = side * 2
    for name, color in (("tray_done", "#f6c343"), ("tray_need", "#e0483e")):
        badged = plain.copy()
        for x in range(s - 12, s):
            for y in range(0, 12):
                edge = x in (s - 12, s - 11, s - 2, s - 1) or y in (0, 1, 10, 11)
                badged.putpixel((x, y), INK if edge else rgb(color))
        badged.save(os.path.join(ROOT, f"{name}.png"))


def main():
    os.makedirs(ROOT, exist_ok=True)
    meta = {"frame": F, "states": [], "stages": [s.capitalize() for s in STAGES], "head": {}, "hats": HATS, "hat_size": [32, 28],
            "hat_anchor": [16, 14], "items": ITEMS, "item_size": 16, "need_size": 10}
    for row, (name, n, fps) in enumerate(STATES):
        meta["states"].append({"name": name, "row": row, "frames": n, "fps": fps})
    sheets = {}
    for sp in SPECIES:
        # one block of animation rows per life stage, stacked in STAGES order
        img = Image.new("RGBA", (4 * F, len(STAGES) * len(STATES) * F), (0, 0, 0, 0))
        heads = []
        for si, stage in enumerate(STAGES):
            heads.append([])
            for row, (name, n, _) in enumerate(STATES):
                frames = [pet_frame(sp, name, f, stage) for f in range(n)]
                for f, (fr, _) in enumerate(frames):
                    img.paste(fr, (f * F, (si * len(STATES) + row) * F))
                heads[-1].append([head for _, head in frames])
        meta["head"][sp] = heads
        img.save(os.path.join(ROOT, f"{sp}.png"))
        sheets[sp] = img
    adult = STAGES.index("adult") * len(STATES) * F  # y of the adult block in a sheet
    hats = sheet([hat(n) for n in HATS], len(HATS), 32, 28)
    hats.save(os.path.join(ROOT, "hats.png"))
    items = sheet([item(n) for n in ITEMS], 6, 16, 16)
    items.save(os.path.join(ROOT, "items.png"))
    sheet([need_icon(rows, pal) for _, pal, rows in NEEDS], len(NEEDS), 10, 10).save(os.path.join(ROOT, "needs.png"))
    world = build_world()
    meta["furniture"] = furniture()
    ground, rects = pack([p["ground"] for p in world], 960)
    ground.save(os.path.join(ROOT, "places.png"))
    # minimaps: each place with its props, shrunk to fit MINI
    shots = [composite(p) for p in world]
    minis = []
    for im in shots:
        k = min(MINI[0] / im.width, MINI[1] / im.height)
        minis.append(im.resize((round(im.width * k), round(im.height * k)), Image.BOX))
    mini, mrects = pack(minis, 256, gap=1)
    mini.save(os.path.join(ROOT, "minimaps.png"))
    meta["places"] = [dict(key=p["key"], size=[p["w"], p["h"]], img=r, mini=m, bg=p["bg"], spawn=p["spawn"], doors=p["doors"],
                           props=p["props"], slots=p["slots"],
                           walk=["".join("." if ok else "#" for ok in row) for row in p["walk"]]) for p, r, m in zip(world, rects, mrects)]
    props, meta["sprites"] = pack(SPRITES, 512, gap=1)
    props.save(os.path.join(ROOT, "props.png"))
    # app icon (.deb, macOS .app, Windows installer): happy dino, 16x so platform downscaling stays crisp
    happy = sheets["dino"].crop((0, adult + 5 * F, F, adult + 6 * F))
    icon = happy.resize((512, 512), Image.NEAREST)
    icon.save(os.path.join(ROOT, "icon.png"))
    icon.save(os.path.join(ROOT, "icon.ico"), sizes=[(s, s) for s in (16, 32, 48, 64, 128, 256)])
    tray_icons(happy)
    with open(os.path.join(ROOT, "meta.json"), "w") as fh:
        json.dump(meta, fh, separators=(",", ":"))

    if len(sys.argv) > 1:  # contact sheet: every place with its props and a pet on its spawn point, then the minimaps
        idle = sheets["dino"].crop((0, adult, F, adult + F))
        for p, im in zip(world, shots):
            blit(im, idle, p["spawn"][0] - F / 2, p["spawn"][1] - F)
        pack(shots + [mini.resize((mini.width * 2, mini.height * 2), Image.NEAREST)], 960, gap=8)[0].save(sys.argv[1])
    print("assets ->", os.path.normpath(ROOT))


if __name__ == "__main__":
    main()
