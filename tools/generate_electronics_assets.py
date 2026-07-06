from __future__ import annotations

import math
import struct
import zlib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1] / "editor" / "assets" / "electronics"
WHITE = (235, 238, 244, 255)
DIM = (132, 141, 154, 255)
ORANGE = (238, 132, 28, 255)
GREEN = (92, 214, 142, 255)
BLUE = (94, 176, 245, 255)
RED = (255, 76, 92, 255)
YELLOW = (255, 204, 86, 255)
TRANSPARENT = (0, 0, 0, 0)


class Canvas:
    def __init__(self, width: int, height: int):
        self.width = width
        self.height = height
        self.pixels = [TRANSPARENT] * (width * height)

    def set(self, x: int, y: int, color):
        if 0 <= x < self.width and 0 <= y < self.height:
            self.pixels[y * self.width + x] = color

    def line(self, x0, y0, x1, y1, color=WHITE, width=3):
        x0, y0, x1, y1 = map(lambda v: int(round(v)), (x0, y0, x1, y1))
        dx = abs(x1 - x0)
        dy = -abs(y1 - y0)
        sx = 1 if x0 < x1 else -1
        sy = 1 if y0 < y1 else -1
        err = dx + dy
        x, y = x0, y0
        while True:
            self.circle(x, y, max(1, width // 2), color)
            if x == x1 and y == y1:
                break
            e2 = 2 * err
            if e2 >= dy:
                err += dy
                x += sx
            if e2 <= dx:
                err += dx
                y += sy

    def rect(self, x, y, w, h, color, outline=None, radius=0):
        x, y, w, h = map(int, (x, y, w, h))
        for yy in range(y, y + h):
            for xx in range(x, x + w):
                if radius:
                    cx = min(max(xx, x + radius), x + w - radius - 1)
                    cy = min(max(yy, y + radius), y + h - radius - 1)
                    if (xx - cx) ** 2 + (yy - cy) ** 2 > radius ** 2:
                        continue
                self.set(xx, yy, color)
        if outline:
            self.line(x + radius, y, x + w - radius, y, outline, 2)
            self.line(x + radius, y + h, x + w - radius, y + h, outline, 2)
            self.line(x, y + radius, x, y + h - radius, outline, 2)
            self.line(x + w, y + radius, x + w, y + h - radius, outline, 2)

    def circle(self, cx, cy, r, color, fill=True):
        cx, cy, r = int(cx), int(cy), int(r)
        for y in range(cy - r, cy + r + 1):
            for x in range(cx - r, cx + r + 1):
                d = (x - cx) ** 2 + (y - cy) ** 2
                if fill and d <= r * r:
                    self.set(x, y, color)
                elif not fill and r * r - r <= d <= r * r + r:
                    self.set(x, y, color)

    def triangle(self, points, color):
        xs = [p[0] for p in points]
        ys = [p[1] for p in points]
        min_x, max_x = int(min(xs)), int(max(xs))
        min_y, max_y = int(min(ys)), int(max(ys))

        def area(a, b, c):
            return (a[0] * (b[1] - c[1]) + b[0] * (c[1] - a[1]) + c[0] * (a[1] - b[1]))

        total = area(points[0], points[1], points[2])
        if total == 0:
            return
        for y in range(min_y, max_y + 1):
            for x in range(min_x, max_x + 1):
                p = (x, y)
                a = area(p, points[1], points[2])
                b = area(points[0], p, points[2])
                c = area(points[0], points[1], p)
                if (a >= 0 and b >= 0 and c >= 0) or (a <= 0 and b <= 0 and c <= 0):
                    self.set(x, y, color)

    def save(self, path: Path):
        path.parent.mkdir(parents=True, exist_ok=True)
        raw = bytearray()
        for y in range(self.height):
            raw.append(0)
            for x in range(self.width):
                raw.extend(self.pixels[y * self.width + x])
        png = bytearray(b"\x89PNG\r\n\x1a\n")

        def chunk(kind: bytes, data: bytes):
            png.extend(struct.pack(">I", len(data)))
            png.extend(kind)
            png.extend(data)
            png.extend(struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF))

        chunk(b"IHDR", struct.pack(">IIBBBBB", self.width, self.height, 8, 6, 0, 0, 0))
        chunk(b"IDAT", zlib.compress(bytes(raw), 9))
        chunk(b"IEND", b"")
        path.write_bytes(bytes(png))


def symbol(kind: str, color=WHITE) -> Canvas:
    c = Canvas(256, 128)
    cx, cy = 128, 64
    c.line(18, cy, 56, cy, color, 4)
    c.line(200, cy, 238, cy, color, 4)
    if kind == "resistor":
        pts = [(56, cy), (72, 44), (88, 84), (104, 44), (120, 84), (136, 44), (152, 84), (168, 44), (184, cy), (200, cy)]
        for a, b in zip(pts, pts[1:]):
            c.line(*a, *b, color, 5)
    elif kind == "capacitor":
        c.line(94, 34, 94, 98, color, 6)
        c.line(162, 34, 162, 98, color, 6)
        c.line(56, cy, 94, cy, color, 4)
        c.line(162, cy, 200, cy, color, 4)
    elif kind == "led":
        c.triangle([(88, 34), (88, 98), (154, 64)], color)
        c.line(166, 34, 166, 98, color, 5)
        c.line(154, cy, 200, cy, color, 4)
        c.line(180, 42, 212, 18, ORANGE, 4)
        c.line(199, 41, 212, 18, ORANGE, 4)
        c.line(180, 86, 212, 62, ORANGE, 4)
        c.line(199, 85, 212, 62, ORANGE, 4)
    elif kind == "battery":
        c.line(88, 28, 88, 100, color, 6)
        c.line(142, 42, 142, 88, color, 5)
        c.line(176, 28, 176, 100, color, 3)
        c.line(56, cy, 88, cy, color, 4)
        c.line(176, cy, 200, cy, color, 4)
        c.line(108, 34, 124, 34, RED, 4)
        c.line(116, 26, 116, 42, RED, 4)
        c.line(184, 98, 202, 98, color, 4)
    elif kind == "ground":
        c.line(cx, 18, cx, 54, color, 5)
        c.line(84, 54, 172, 54, color, 5)
        c.line(98, 76, 158, 76, color, 5)
        c.line(112, 98, 144, 98, color, 5)
    elif kind == "magnet":
        c.rect(72, 32, 112, 64, (0, 0, 0, 0), color, 10)
        c.line(128, 32, 128, 96, color, 4)
        c.line(56, cy, 72, cy, color, 4)
        c.line(184, cy, 200, cy, color, 4)
        c.line(96, 42, 96, 86, BLUE, 7)
        c.line(160, 42, 160, 86, RED, 7)
    else:
        c.rect(76, 34, 104, 60, (0, 0, 0, 0), color, 6)
        c.line(56, cy, 76, cy, color, 4)
        c.line(180, cy, 200, cy, color, 4)
    return c


def scale_nearest(src: Canvas, width: int, height: int) -> Canvas:
    dst = Canvas(width, height)
    for y in range(height):
        sy = int(y * src.height / height)
        for x in range(width):
            sx = int(x * src.width / width)
            dst.pixels[y * width + x] = src.pixels[sy * src.width + sx]
    return dst


def icon(kind: str) -> Canvas:
    c = Canvas(96, 96)
    if kind == "select":
        c.triangle([(30, 18), (32, 70), (46, 56)], WHITE)
        c.line(44, 54, 58, 78, ORANGE, 5)
    elif kind == "wire":
        c.line(16, 54, 38, 54, GREEN, 5)
        c.line(38, 54, 38, 32, GREEN, 5)
        c.line(38, 32, 72, 32, GREEN, 5)
        c.circle(16, 54, 7, BLUE, False)
        c.circle(72, 32, 7, ORANGE, False)
    elif kind == "rotate":
        for i in range(34):
            a = math.radians(210 + i * 7)
            c.circle(48 + math.cos(a) * 26, 48 + math.sin(a) * 26, 2, WHITE)
        c.triangle([(68, 25), (80, 30), (68, 38)], ORANGE)
    elif kind == "play":
        c.triangle([(34, 24), (34, 72), (72, 48)], ORANGE)
    elif kind == "export":
        c.rect(24, 44, 48, 28, (0, 0, 0, 0), WHITE, 4)
        c.line(48, 18, 48, 54, ORANGE, 5)
        c.triangle([(36, 42), (60, 42), (48, 58)], ORANGE)
    elif kind == "grid":
        for x in (26, 48, 70):
            c.line(x, 20, x, 76, DIM, 3)
        for y in (26, 48, 70):
            c.line(20, y, 76, y, DIM, 3)
        c.circle(48, 48, 5, ORANGE)
    elif kind == "fit":
        c.rect(22, 22, 52, 52, (0, 0, 0, 0), WHITE, 4)
        c.line(22, 38, 22, 22, ORANGE, 4)
        c.line(38, 22, 22, 22, ORANGE, 4)
        c.line(74, 58, 74, 74, ORANGE, 4)
        c.line(58, 74, 74, 74, ORANGE, 4)
    elif kind == "outline":
        c.line(20, 24, 74, 24, GREEN, 5)
        c.line(74, 24, 74, 70, GREEN, 5)
        c.line(74, 70, 20, 70, GREEN, 5)
        c.line(20, 70, 20, 24, GREEN, 5)
        c.circle(20, 24, 5, ORANGE)
        c.circle(74, 70, 5, ORANGE)
    elif kind == "airwire":
        for i in range(0, 52, 12):
            c.line(18 + i, 66 - i * 0.5, 26 + i, 62 - i * 0.5, YELLOW, 4)
        c.circle(18, 66, 7, BLUE, False)
        c.circle(74, 38, 7, ORANGE, False)
    elif kind == "layers":
        for offset, color in [(0, BLUE), (10, GREEN), (20, ORANGE)]:
            c.line(24, 34 + offset, 48, 22 + offset, color, 4)
            c.line(48, 22 + offset, 72, 34 + offset, color, 4)
            c.line(72, 34 + offset, 48, 46 + offset, color, 4)
            c.line(48, 46 + offset, 24, 34 + offset, color, 4)
    else:
        c.circle(48, 48, 24, ORANGE, False)
    return c


def footprint(kind: str) -> Canvas:
    c = Canvas(192, 128)
    if kind == "0805":
        c.rect(54, 46, 84, 36, (44, 48, 56, 255), WHITE, 8)
        c.rect(24, 52, 34, 24, ORANGE, YELLOW, 5)
        c.rect(134, 52, 34, 24, ORANGE, YELLOW, 5)
        c.line(72, 64, 120, 64, GREEN, 4)
    elif kind == "magnet-10x5":
        c.rect(38, 36, 116, 56, (34, 40, 50, 255), WHITE, 10)
        c.rect(50, 46, 32, 36, BLUE, None, 6)
        c.rect(110, 46, 32, 36, RED, None, 6)
        c.rect(18, 56, 28, 18, ORANGE, YELLOW, 5)
        c.rect(146, 56, 28, 18, ORANGE, YELLOW, 5)
    elif kind == "battery-18650":
        c.rect(28, 40, 136, 48, (36, 42, 50, 255), WHITE, 20)
        c.rect(36, 50, 24, 28, RED, YELLOW, 8)
        c.rect(132, 50, 24, 28, DIM, WHITE, 8)
        c.line(48, 58, 48, 70, WHITE, 3)
        c.line(42, 64, 54, 64, WHITE, 3)
        c.line(138, 64, 150, 64, WHITE, 3)
    elif kind == "test-point":
        c.circle(96, 64, 34, GREEN, True)
        c.circle(96, 64, 22, (18, 22, 28, 255), True)
        c.circle(96, 64, 12, ORANGE, False)
    else:
        c.rect(48, 38, 96, 52, (42, 46, 54, 255), WHITE, 8)
        c.rect(24, 54, 28, 20, ORANGE, YELLOW, 4)
        c.rect(140, 54, 28, 20, ORANGE, YELLOW, 4)
    return c


def main():
    for name, color in {
        "resistor": GREEN,
        "capacitor": YELLOW,
        "led": ORANGE,
        "magnet": BLUE,
        "battery": ORANGE,
        "ground": GREEN,
        "generic": WHITE,
    }.items():
        component_symbol = symbol(name, color)
        component_symbol.save(ROOT / "symbols" / f"{name}.png")
        scale_nearest(component_symbol, 128, 128).save(ROOT / "library" / f"{name}.png")

    for name in ["select", "wire", "rotate", "play", "export", "grid", "fit", "outline", "airwire", "layers"]:
        icon(name).save(ROOT / "toolbar" / f"{name}.png")

    for name in ["0805", "magnet-10x5", "battery-18650", "test-point", "generic"]:
        footprint(name).save(ROOT / "footprints" / f"{name}.png")

    print(f"Generated electronics assets in {ROOT}")


if __name__ == "__main__":
    main()
