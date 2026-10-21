from __future__ import annotations

"""Footprint artwork generator for the native Electronics canvas.

Only the `footprints/*.png` set lives here. Schematic symbols are edited in
`tools/generate_electronics_ui_icons.ps1`, and the copper pads, traces and
outline on the canvas are painted live by ApiGraphicBasic, never from PNG.

Artwork contract per footprint image:
- Body silhouette: translucent dark slate so the netlist and the outline
  stay readable underneath.
- Silkscreen: light outline plus the pin-1 marker, authored in a mid tone so
  it reads on both dark and light canvases without a tint.
- No copper: the renderer owns pads and traces as geometry.
"""

import struct
import zlib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1] / "editor" / "assets" / "electronics"

BODY = (30, 34, 42, 66)
BODY_LIGHT = (30, 34, 42, 54)
SILK = (222, 226, 234, 170)
SILK_PIN = (222, 226, 234, 220)
TRANSPARENT = (0, 0, 0, 0)


class Canvas:
    def __init__(self, width: int, height: int):
        self.width = width
        self.height = height
        self.pixels = [TRANSPARENT] * (width * height)

    def set(self, x: int, y: int, color):
        if 0 <= x < self.width and 0 <= y < self.height:
            self.pixels[y * self.width + x] = color

    def blend(self, x: int, y: int, color):
        if 0 <= x < self.width and 0 <= y < self.height:
            index = y * self.width + x
            base = self.pixels[index]
            alpha = color[3] / 255.0
            mixed = tuple(
                int(base[i] * (1 - alpha) + color[i] * alpha) for i in range(3)
            )
            self.pixels[index] = (mixed[0], mixed[1], mixed[2], max(base[3], color[3]))

    def circle(self, cx, cy, r, color, fill=True):
        cx, cy, r = int(cx), int(cy), int(r)
        for y in range(cy - r - 1, cy + r + 2):
            for x in range(cx - r - 1, cx + r + 2):
                d = (x - cx) ** 2 + (y - cy) ** 2
                if fill and d <= r * r:
                    self.blend(x, y, color)

    def polygon(self, points, color):
        xs = [p[0] for p in points]
        ys = [p[1] for p in points]

        def area(a, b, c):
            return a[0] * (b[1] - c[1]) + b[0] * (c[1] - a[1]) + c[0] * (a[1] - b[1])

        for y in range(int(min(ys)) - 1, int(max(ys)) + 2):
            for x in range(int(min(xs)) - 1, int(max(xs)) + 2):
                p = (x, y)
                total = area(points[0], points[1], points[2])
                if len(points) == 4:
                    total += area(points[0], points[2], points[3])
                    a = area(p, points[1], points[2]) + area(points[0], points[2], points[3])
                    b = area(points[0], p, points[2])
                    b += area(points[0], points[2], p)
                    b += area(points[0], points[2], points[3])
                    inside = (total >= 0 and area(p, points[1], points[2]) >= -total) or (
                        total < 0 and area(p, points[1], points[2]) <= -total
                    )
                    if inside and area(points[0], points[2], points[3]) * total >= 0:
                        self.blend(x, y, color)
                    continue
                if total == 0:
                    continue
                a = area(p, points[1], points[2])
                b = area(points[0], p, points[2])
                c = area(points[0], points[1], p)
                if (a >= 0 and b >= 0 and c >= 0) or (a <= 0 and b <= 0 and c <= 0):
                    self.blend(x, y, color)

    def rounded_rect(self, x, y, w, h, radius, body, outline, stroke=2):
        x, y, w, h, radius = int(x), int(y), int(w), int(h), int(radius)
        if body:
            for yy in range(y, y + h):
                for xx in range(x, x + w):
                    cx = min(max(xx, x + radius), x + w - radius - 1)
                    cy = min(max(yy, y + radius), y + h - radius - 1)
                    if (xx - cx) ** 2 + (yy - cy) ** 2 <= radius ** 2:
                        self.blend(xx, yy, body)
        if outline:
            self.outline_rounded(x, y, w, h, radius, outline, stroke)

    def outline_rounded(self, x, y, w, h, radius, color, stroke):
        radius = int(radius)
        # Straight segments
        for offset in range(stroke):
            for xx in range(x + radius, x + w - radius):
                self.blend(xx, y + offset, color)
                self.blend(xx, y + h - 1 - offset, color)
            for yy in range(y + radius, y + h - radius):
                self.blend(x + offset, yy, color)
                self.blend(x + w - 1 - offset, yy, color)
        # Quarter arcs
        for a in range(radius * 2):
            for b in range(radius * 2):
                dx = a - radius + 0.5
                dy = b - radius + 0.5
                d = (dx ** 2 + dy ** 2) ** 0.5
                if radius - stroke / 2 <= d <= radius + stroke / 2:
                    for anchor_x, anchor_y, sign_x, sign_y in (
                        (x + radius, y + radius, -1, -1),
                        (x + w - radius - 1, y + radius, 1, -1),
                        (x + radius, y + h - radius - 1, -1, 1),
                        (x + w - radius - 1, y + h - radius - 1, 1, 1),
                    ):
                        ax = anchor_x + sign_x * int(radius - 0.5 - dx * sign_x)
                        ay = anchor_y + sign_y * int(radius - 0.5 - dy * sign_y)
                        self.blend(ax, ay, color)

    def ring(self, cx, cy, r, color, stroke):
        cx, cy, r = int(cx), int(cy), int(r)
        outer = r + stroke
        for y in range(cy - outer - 1, cy + outer + 2):
            for x in range(cx - outer - 1, cx + outer + 2):
                d = (x - cx) ** 2 + (y - cy) ** 2
                if (outer * outer >= d >= r * r):
                    self.blend(x, y, color)

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


def footprint(kind: str) -> Canvas:
    """Draw one footprint body+silkscreen artwork.

    Pads are excluded on purpose: ApiGraphicBasic paints the copper pad
    geometry on top at the exact footprint offsets, so painted pads would
    duplicate the live copper during placement preview and rotation.
    """
    c = Canvas(192, 128)
    if kind == "0805":
        c.rounded_rect(16, 26, 160, 76, 26, BODY, SILK, 2)
        # Pin-1 silk dot beside the first pad position (body-relative).
        c.circle(16, 64, 4, SILK_PIN)
    elif kind == "MAG-10x5" or kind == "magnet-10x5":
        c.rounded_rect(10, 22, 172, 84, 30, BODY, SILK, 2)
        c.circle(10, 64, 4, SILK_PIN)
    elif kind == "BAT-18650" or kind == "battery-18650":
        c.rounded_rect(4, 24, 184, 80, 34, BODY_LIGHT, SILK, 2)
        # Battery polarity silk marks, not copper: renderer draws pads.
        for base_x, sign in ((14, 1), (176, -1)):
            s = sign
            for step in range(3):
                c.circle(base_x + s * step * 3, 64, 1, SILK)
        c.circle(4, 64, 4, SILK_PIN)
    elif kind == "TP-GND" or kind == "test-point":
        # Test point: silk ring only; the copper disc is drawn by the renderer.
        c.ring(96, 64, 40, SILK, 3)
        c.circle(96, 64, 4, SILK_PIN)
    else:
        # Generic chip body: translucent, silk outline, notch on the first
        # pad side. Pads land on the DIP columns via the live geometry.
        c.rounded_rect(28, 26, 136, 76, 18, BODY, SILK, 2)
        c.circle(34, 64, 5, SILK_PIN)
    return c


def main():
    for name in ["0805", "MAG-10x5", "BAT-18650", "TP-GND", "generic"]:
        footprint(name).save(ROOT / "footprints" / f"{name}.png")

    print(f"Generated electronics footprint artwork in {ROOT / 'footprints'}")


if __name__ == "__main__":
    main()
