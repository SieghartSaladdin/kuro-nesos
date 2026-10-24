"""Generates the Kuro Nesos logo: a black cat whose head is the notch, drawn in a
pencil-sketch style (jittered overlapping strokes, cross-hatched graphite fill).

    python assets/make_logo.py        (needs Pillow)

Outputs (next to this script):
    logo.png              1024x1024 sketch on a paper card (README / store art)
    logo-transparent.png  1024x1024 graphite strokes only, transparent background
    kuro-nesos.ico        app/tray icon; small sizes use a bolder variant so they stay legible
"""
import io
import math
import os
import random
import struct

from PIL import Image, ImageChops, ImageDraw, ImageFilter

HERE = os.path.dirname(os.path.abspath(__file__))
PAPER = (243, 239, 230)
GRAPHITE = (28, 28, 30)

# ---------------------------------------------------------------- geometry (0..1 units)

LINE_Y = 0.31  # the "screen edge" the notch hangs from

# head outline incl. ears; True marks a sharp corner (ear tips / bases)
HEAD = [
    ((0.250, 0.345), True),
    ((0.285, 0.115), True),
    ((0.425, 0.275), True),
    ((0.575, 0.275), True),
    ((0.715, 0.115), True),
    ((0.750, 0.345), True),
    ((0.765, 0.500), False),
    ((0.742, 0.625), False),
    ((0.680, 0.705), False),
    ((0.580, 0.748), False),
    ((0.420, 0.748), False),
    ((0.320, 0.705), False),
    ((0.258, 0.625), False),
    ((0.235, 0.500), False),
]
EYES = [(0.415, 0.505), (0.585, 0.505)]
EYE_RX, EYE_RY = 0.050, 0.064


def catmull(points, closed=True, steps=14):
    """Smooth closed path through `points` ((x, y), sharp) keeping sharp corners."""
    out = []
    n = len(points)
    for i in range(n if closed else n - 1):
        p0, p1, p2, p3 = (points[(i + k) % n][0] for k in (-1, 0, 1, 2))
        sharp1, sharp2 = points[i][1], points[(i + 1) % n][1]
        for s in range(steps):
            t = s / steps
            if sharp1 and sharp2:
                out.append((p1[0] + (p2[0] - p1[0]) * t, p1[1] + (p2[1] - p1[1]) * t))
                continue
            a0 = p1 if sharp1 else p0
            a3 = p2 if sharp2 else p3
            t2, t3 = t * t, t * t * t
            out.append(tuple(
                0.5 * ((2 * p1[k]) + (-a0[k] + p2[k]) * t + (2 * a0[k] - 5 * p1[k] + 4 * p2[k] - a3[k]) * t2 + (-a0[k] + 3 * p1[k] - 3 * p2[k] + a3[k]) * t3)
                for k in (0, 1)))
    return out


def inside(poly, x, y):
    c = False
    j = len(poly) - 1
    for i in range(len(poly)):
        xi, yi = poly[i]
        xj, yj = poly[j]
        if (yi > y) != (yj > y) and x < (xj - xi) * (y - yi) / (yj - yi + 1e-12) + xi:
            c = not c
        j = i
    return c


def in_eye(x, y, grow=1.0):
    return any(((x - ex) / (EYE_RX * grow)) ** 2 + ((y - ey) / (EYE_RY * grow)) ** 2 <= 1.0 for ex, ey in EYES)


# ---------------------------------------------------------------- pencil strokes

class Sketch:
    def __init__(self, size, seed):
        self.size = size
        self.rng = random.Random(seed)
        self.ink = Image.new("L", (size, size), 0)

    def px(self, p):
        return (p[0] * self.size, p[1] * self.size)

    def stroke(self, pts, width, dark, jitter=0.0035, overshoot=0.0, draw=None):
        """One pencil pass along pts (0..1 units) with smooth wobble. Separate passes add up
        (darker where strokes cross); strokes sharing `draw` form a single pass."""
        rng = self.rng
        if len(pts) < 2:
            return
        if overshoot:
            (x0, y0), (x1, y1) = pts[0], pts[1]
            d = math.hypot(x1 - x0, y1 - y0) or 1
            pts = [(x0 - (x1 - x0) / d * overshoot, y0 - (y1 - y0) / d * overshoot)] + list(pts)
            (x0, y0), (x1, y1) = pts[-1], pts[-2]
            d = math.hypot(x1 - x0, y1 - y0) or 1
            pts = list(pts) + [(x0 + (x0 - x1) / d * overshoot, y0 + (y0 - y1) / d * overshoot)]
        phases = [rng.uniform(0, 6.3) for _ in range(4)]
        freqs = [rng.uniform(3, 7), rng.uniform(9, 17), rng.uniform(2, 4), rng.uniform(11, 19)]
        layer = None
        d = draw
        if d is None:
            layer = Image.new("L", self.ink.size, 0)
            d = ImageDraw.Draw(layer)
        out = []
        total = len(pts)
        for i, (x, y) in enumerate(pts):
            t = i / max(total - 1, 1)
            ox = jitter * (math.sin(freqs[0] * t + phases[0]) * 0.7 + math.sin(freqs[1] * t + phases[1]) * 0.3)
            oy = jitter * (math.sin(freqs[2] * t + phases[2]) * 0.7 + math.sin(freqs[3] * t + phases[3]) * 0.3)
            out.append(self.px((x + ox, y + oy)))
        w = max(1, int(width * self.size))
        d.line(out, fill=int(dark), width=w, joint="curve")
        if layer is not None:
            self.ink = ImageChops.add(self.ink, layer)

    def hatch(self, angle_deg, spacing, dark, width, keep, length_jitter=0.02, density=None):
        """Parallel strokes at angle inside keep(x, y), broken into dashes; density(y)
        is the chance that a dash is drawn (heavier shading where it is high)."""
        rng = self.rng
        layer = Image.new("L", self.ink.size, 0)
        draw = ImageDraw.Draw(layer)
        a = math.radians(angle_deg)
        dx, dy = math.cos(a), math.sin(a)
        nx, ny = -dy, dx
        c = 0.5
        k = -0.9
        while k <= 0.9:
            seg = []
            t = -0.9
            dash_end = t + rng.uniform(0.03, 0.10)
            while t <= 0.9:
                x, y = c + nx * k + dx * t, c + ny * k + dy * t
                if t > dash_end:
                    # end of a dash: flush it (maybe), skip a small gap
                    if len(seg) > 1 and (density is None or rng.random() < density(seg[0][1])):
                        self.stroke(seg, width, dark * rng.uniform(0.7, 1.0), jitter=0.0015, draw=draw)
                    seg = []
                    t += rng.uniform(0.004, 0.02)
                    dash_end = t + rng.uniform(0.03, 0.10)
                    continue
                if 0 <= x <= 1 and 0 <= y <= 1 and keep(x, y):
                    seg.append((x, y))
                elif seg:
                    if len(seg) > 1 and (density is None or rng.random() < density(seg[0][1])):
                        self.stroke(seg, width, dark * rng.uniform(0.7, 1.0), jitter=0.0015, draw=draw)
                    seg = []
                t += 0.004 + rng.uniform(0, length_jitter) * 0.05
            if len(seg) > 1 and (density is None or rng.random() < density(seg[0][1])):
                self.stroke(seg, width, dark * rng.uniform(0.7, 1.0), jitter=0.0015, draw=draw)
            k += spacing * rng.uniform(0.8, 1.2)
        self.ink = ImageChops.add(self.ink, layer)


def draw(size, bold=False, seed=7):
    s = Sketch(size, seed)
    head = catmull(HEAD)

    if bold:
        # small icon sizes: solid graphite head, clear eyes, one strong edge line
        mask = Image.new("L", (size, size), 0)
        d = ImageDraw.Draw(mask)
        d.polygon([s.px(p) for p in head], fill=235)
        for ex, ey in EYES:
            d.ellipse([s.px((ex - EYE_RX * 1.15, ey - EYE_RY * 1.1)), s.px((ex + EYE_RX * 1.15, ey + EYE_RY * 1.1))], fill=0)
        d.line([s.px((0.05, LINE_Y)), s.px((0.24, LINE_Y))], fill=235, width=max(1, int(0.035 * size)))
        d.line([s.px((0.76, LINE_Y)), s.px((0.95, LINE_Y))], fill=235, width=max(1, int(0.035 * size)))
        for ex, ey in EYES:
            d.ellipse([s.px((ex - 0.012, ey - EYE_RY * 0.8)), s.px((ex + 0.012, ey + EYE_RY * 0.8))], fill=235)
        s.ink = mask
        return s.ink

    rng = s.rng
    inside_head = lambda x, y: inside(head, x, y) and not in_eye(x, y, 1.18)
    n = len(head)

    def sub_loop(poly, start, frac):
        k0 = int(start * len(poly))
        k1 = k0 + max(2, int(frac * len(poly)))
        return [poly[i % len(poly)] for i in range(k0, k1)]

    def inset(poly, f, cx=0.5, cy=0.47):
        return [(cx + (x - cx) * f, cy + (y - cy) * f) for x, y in poly]

    # faint construction guides, like a sketch that started from shapes
    guide = [(0.5 + 0.27 * math.cos(t), 0.5 + 0.25 * math.sin(t)) for t in [k * 2 * math.pi / 60 for k in range(61)]]
    s.stroke(guide, 0.0016, 45, jitter=0.004)
    s.stroke([(0.5, 0.20), (0.5, 0.80)], 0.0014, 35, jitter=0.002)
    s.stroke([(0.30, 0.505), (0.70, 0.505)], 0.0014, 35, jitter=0.002)

    # fur: loose strands that follow the head shape, denser towards the rim
    layer = Image.new("L", s.ink.size, 0)
    fur = ImageDraw.Draw(layer)
    for k in range(70):
        f = 1.0 - (k / 70) ** 1.4 * 0.62
        cx, cy = 0.5 + rng.uniform(-0.04, 0.04), 0.42 + rng.uniform(-0.06, 0.04)
        strand = sub_loop(inset(head, f, cx, cy), rng.random(), rng.uniform(0.06, 0.22))
        strand = [p for p in strand if not in_eye(p[0], p[1], 1.25)]
        if len(strand) > 2:
            s.stroke(strand, rng.uniform(0.0014, 0.0026), rng.uniform(70, 130), jitter=0.006, draw=fur)
    s.ink = ImageChops.add(s.ink, layer)

    # graphite: broken diagonal hatching, heavier in the lower half (shadow)
    s.hatch(40, 0.0105, 125, 0.0021, inside_head, density=lambda y: 0.6 + 0.4 * min(1.0, max(0.0, y - 0.4) / 0.3))
    s.hatch(-48, 0.0135, 100, 0.0019, inside_head, density=lambda y: 0.2 + 0.8 * min(1.0, max(0.0, y - 0.5) / 0.22))
    s.hatch(80, 0.019, 75, 0.0017, lambda x, y: inside_head(x, y) and y > 0.6)

    # outline: overlapping partial strokes that do not quite agree, slight overshoot
    for i in range(8):
        part = sub_loop(head, rng.random(), rng.uniform(0.3, 0.7))
        s.stroke(part, rng.uniform(0.0016, 0.0034), rng.uniform(90, 170), jitter=0.007, overshoot=0.014)
    s.stroke(head + head[:2], 0.0024, 120, jitter=0.0045)
    # inner ear strokes
    for (bx, by), (tx, ty) in (((0.30, 0.30), (0.292, 0.17)), ((0.70, 0.30), (0.708, 0.17))):
        for i in range(3):
            s.stroke([(bx + rng.uniform(-0.02, 0.02), by), (tx + rng.uniform(-0.01, 0.01), ty + rng.uniform(0, 0.03))], 0.0018, 90, jitter=0.003)

    # the screen edge the notch cuts into, with small concave shoulders
    for x0, x1, sh in ((0.04, 0.215, 1), (0.785, 0.96, -1)):
        for i in range(4):
            a0 = x0 + rng.uniform(-0.02, 0.03)
            a1 = x1 + rng.uniform(-0.03, 0.01)
            s.stroke([(a0, LINE_Y + rng.uniform(-0.003, 0.003)), (a1, LINE_Y + rng.uniform(-0.003, 0.003))], rng.uniform(0.0018, 0.0034), rng.uniform(90, 180), jitter=0.003, overshoot=0.012)
        cx = 0.25 if sh > 0 else 0.75
        arc = [(cx - sh * 0.035 * math.cos(t), LINE_Y + 0.035 * (1 - math.sin(t))) for t in [k * math.pi / 16 for k in range(9)]]
        for i in range(2):
            s.stroke(arc, 0.0024, 140, jitter=0.002)

    # eyes: paper-white almonds with a thin rim, slit pupils and a highlight
    for ex, ey in EYES:
        rim = [(ex + EYE_RX * math.cos(t), ey + EYE_RY * math.sin(t)) for t in [k * 2 * math.pi / 40 for k in range(41)]]
        for i in range(4):
            start = rng.uniform(0, 0.3)
            part = rim[int(start * 40):] + rim[: int(start * 40) + rng.randint(4, 10)]
            s.stroke(part, 0.0030 if i == 0 else 0.0018, 200 if i == 0 else 110, jitter=0.0028)
        pupil = [(ex, ey - EYE_RY * 0.78), (ex + 0.011, ey), (ex, ey + EYE_RY * 0.78), (ex - 0.011, ey), (ex, ey - EYE_RY * 0.78)]
        for i in range(2):
            s.stroke(pupil, 0.004, 170, jitter=0.0012)
        s.hatch(70, 0.004, 160, 0.0022, lambda x, y, ex=ex, ey=ey: abs(x - ex) < 0.0105 * (1 - ((y - ey) / (EYE_RY * 0.78)) ** 2) ** 0.5 if abs(y - ey) < EYE_RY * 0.78 else False)

    # whiskers outside the head
    for side in (-1, 1):
        for k, (dy, bend) in enumerate(((-0.02, -0.02), (0.012, 0.0), (0.042, 0.025))):
            x0 = 0.5 + side * 0.255
            y0 = 0.58 + dy
            pts = [(x0 + side * t * 0.17, y0 + bend * t + 0.03 * t * t * (1 if k == 2 else 0)) for t in [j / 10 for j in range(11)]]
            for i in range(2):
                s.stroke(pts, 0.0024 if i == 0 else 0.0014, 150 if i == 0 else 80, jitter=0.003, overshoot=0.01)

    # paper-white eyes cut through any stray graphite, highlights on top
    clear = Image.new("L", (size, size), 0)
    d = ImageDraw.Draw(clear)
    for ex, ey in EYES:
        d.ellipse([s.px((ex - EYE_RX * 0.92, ey - EYE_RY * 0.92)), s.px((ex + EYE_RX * 0.92, ey + EYE_RY * 0.92))], fill=255)
    pupils = Image.new("L", (size, size), 0)
    pd = ImageDraw.Draw(pupils)
    for ex, ey in EYES:
        pd.polygon([s.px((ex, ey - EYE_RY * 0.78)), s.px((ex + 0.011, ey)), s.px((ex, ey + EYE_RY * 0.78)), s.px((ex - 0.011, ey))], fill=255)
    clear = ImageChops.subtract(clear, pupils)
    s.ink = ImageChops.subtract(s.ink, clear)
    hl = Image.new("L", (size, size), 0)
    hd = ImageDraw.Draw(hl)
    for ex, ey in EYES:
        hd.ellipse([s.px((ex - 0.022, ey - 0.03)), s.px((ex - 0.008, ey - 0.016))], fill=255)
    s.ink = ImageChops.subtract(s.ink, hl)
    return s.ink


def paper_card(size, ink, radius=0.22, seed=3):
    rng = random.Random(seed)
    card = Image.new("RGB", (size, size), PAPER)
    # faint paper grain
    grain = Image.effect_noise((size, size), 14).filter(ImageFilter.GaussianBlur(0.6))
    card = Image.blend(card, Image.merge("RGB", (grain, grain, grain)), 0.05)
    graphite = Image.new("RGB", (size, size), GRAPHITE)
    card = Image.composite(graphite, card, ink)
    mask = Image.new("L", (size, size), 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, size - 1, size - 1], radius=int(size * radius), fill=255)
    out = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    out.paste(card, (0, 0), mask)
    _ = rng
    return out


def render(px, bold=False, card=True):
    ss = 4 if px >= 128 else 8
    ink = draw(px * ss, bold=bold)
    if card:
        img = paper_card(px * ss, ink)
    else:
        img = Image.new("RGBA", (px * ss, px * ss), GRAPHITE + (0,))
        img.putalpha(ink)
    return img.resize((px, px), Image.LANCZOS)


def write_ico(path, images):
    """ICO with PNG-compressed entries (Vista+), one image per size."""
    blobs = []
    for im in images:
        b = io.BytesIO()
        im.save(b, "PNG")
        blobs.append((im.size[0], b.getvalue()))
    header = struct.pack("<HHH", 0, 1, len(blobs))
    offset = 6 + 16 * len(blobs)
    entries, data = b"", b""
    for size, blob in blobs:
        dim = 0 if size >= 256 else size
        entries += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(blob), offset + len(data))
        data += blob
    with open(path, "wb") as f:
        f.write(header + entries + data)


if __name__ == "__main__":
    render(1024).save(os.path.join(HERE, "logo.png"))
    render(1024, card=False).save(os.path.join(HERE, "logo-transparent.png"))
    sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256]
    write_ico(os.path.join(HERE, "kuro-nesos.ico"), [render(s, bold=s <= 48) for s in sizes])
    print("wrote logo.png, logo-transparent.png, kuro-nesos.ico")
