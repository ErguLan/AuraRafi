"""Generate the RafUI white-line icon family.

The SVG files are the editable masters. PNG files are the compact runtime
representation consumed by ApiGraphicBasic, so the same artwork is shared by
the WGPU and CPU paths without making surfaces depend on file paths.
"""

from __future__ import annotations

import math
from pathlib import Path

from PIL import Image, ImageDraw


ROOT = Path(__file__).resolve().parents[2]
RUNTIME_DIR = ROOT / "crates" / "raf_render" / "assets" / "ui_icons"
RUNTIME_PNG_DIR = RUNTIME_DIR / "png"
RUNTIME_SVG_DIR = RUNTIME_DIR / "svg"
TOP_PNG_DIR = ROOT / "editor" / "assets" / "ui_icons" / "top"
TOP_SVG_DIR = TOP_PNG_DIR / "svg"

SIZE = 64
SCALE = 8
STROKE = 3.8
COLOR = "#F3F5F7"
RGBA = (243, 245, 247, 255)


class Icon:
    def __init__(self) -> None:
        self.svg: list[str] = []
        self.image = Image.new("RGBA", (SIZE * SCALE, SIZE * SCALE), (0, 0, 0, 0))
        self.draw = ImageDraw.Draw(self.image)

    def _points(self, points: list[tuple[float, float]]) -> list[tuple[int, int]]:
        return [(round(x * SCALE), round(y * SCALE)) for x, y in points]

    def line(self, points: list[tuple[float, float]], width: float = STROKE) -> None:
        self.svg.append(
            f'<polyline points="{" ".join(f"{x:g},{y:g}" for x, y in points)}" '
            f'fill="none" stroke="{COLOR}" stroke-width="{width:g}" '
            'stroke-linecap="round" stroke-linejoin="round"/>'
        )
        self.draw.line(self._points(points), fill=RGBA, width=round(width * SCALE), joint="curve")

    def rect(self, left: float, top: float, right: float, bottom: float, radius: float = 0) -> None:
        if radius:
            self.svg.append(
                f'<rect x="{left:g}" y="{top:g}" width="{right-left:g}" height="{bottom-top:g}" '
                f'rx="{radius:g}" fill="none" stroke="{COLOR}" stroke-width="{STROKE:g}"/>'
            )
            self.draw.rounded_rectangle(
                (round(left * SCALE), round(top * SCALE), round(right * SCALE), round(bottom * SCALE)),
                radius=round(radius * SCALE),
                outline=RGBA,
                width=round(STROKE * SCALE),
            )
        else:
            self.svg.append(
                f'<rect x="{left:g}" y="{top:g}" width="{right-left:g}" height="{bottom-top:g}" '
                f'fill="none" stroke="{COLOR}" stroke-width="{STROKE:g}" stroke-linejoin="round"/>'
            )
            self.draw.rectangle(
                (round(left * SCALE), round(top * SCALE), round(right * SCALE), round(bottom * SCALE)),
                outline=RGBA,
                width=round(STROKE * SCALE),
            )

    def circle(self, cx: float, cy: float, radius: float, width: float = STROKE) -> None:
        self.svg.append(
            f'<circle cx="{cx:g}" cy="{cy:g}" r="{radius:g}" fill="none" '
            f'stroke="{COLOR}" stroke-width="{width:g}"/>'
        )
        self.draw.ellipse(
            (round((cx - radius) * SCALE), round((cy - radius) * SCALE),
             round((cx + radius) * SCALE), round((cy + radius) * SCALE)),
            outline=RGBA,
            width=round(width * SCALE),
        )

    def fill_polygon(self, points: list[tuple[float, float]]) -> None:
        encoded = " ".join(f"{x:g},{y:g}" for x, y in points)
        self.svg.append(f'<polygon points="{encoded}" fill="{COLOR}"/>')
        self.draw.polygon(self._points(points), fill=RGBA)

    def dot(self, cx: float, cy: float, radius: float = 2.3) -> None:
        self.svg.append(f'<circle cx="{cx:g}" cy="{cy:g}" r="{radius:g}" fill="{COLOR}"/>')
        self.draw.ellipse(
            (round((cx - radius) * SCALE), round((cy - radius) * SCALE),
             round((cx + radius) * SCALE), round((cy + radius) * SCALE)),
            fill=RGBA,
        )

    def arc(self, cx: float, cy: float, radius: float, start: float, end: float) -> None:
        points = []
        steps = max(8, round(abs(end - start) / 8))
        for index in range(steps + 1):
            angle = math.radians(start + (end - start) * index / steps)
            points.append((cx + math.cos(angle) * radius, cy + math.sin(angle) * radius))
        self.line(points)

    def svg_text(self) -> str:
        body = "".join(self.svg)
        return (
            '<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" '
            'viewBox="0 0 64 64" fill="none">' + body + "</svg>\n"
        )


def draw_icon(name: str) -> Icon:
    icon = Icon()
    L = icon.line
    R = icon.rect
    C = icon.circle
    P = icon.fill_polygon
    D = icon.dot

    if name == "select":
        P([(19, 10), (48, 36), (36, 37), (43, 52), (37, 55), (30, 39), (22, 48)])
        L([(19, 10), (22, 48)])
    elif name == "move":
        L([(32, 10), (32, 54)])
        L([(10, 32), (54, 32)])
        L([(32, 10), (25, 17)])
        L([(32, 10), (39, 17)])
        L([(32, 54), (25, 47)])
        L([(32, 54), (39, 47)])
        L([(10, 32), (17, 25)])
        L([(10, 32), (17, 39)])
        L([(54, 32), (47, 25)])
        L([(54, 32), (47, 39)])
    elif name == "rotate":
        icon.arc(32, 32, 21, 35, 325)
        L([(48, 12), (50, 25), (37, 23)])
    elif name == "scale":
        L([(13, 25), (13, 13), (25, 13)])
        L([(13, 13), (27, 27)])
        L([(39, 13), (51, 13), (51, 25)])
        L([(51, 13), (37, 27)])
        L([(13, 39), (13, 51), (25, 51)])
        L([(13, 51), (27, 37)])
        L([(39, 51), (51, 51), (51, 39)])
        L([(51, 51), (37, 37)])
    elif name == "focus":
        C(32, 32, 17)
        C(32, 32, 4)
        L([(32, 8), (32, 17)])
        L([(32, 47), (32, 56)])
        L([(8, 32), (17, 32)])
        L([(47, 32), (56, 32)])
    elif name in ("undo", "redo"):
        if name == "undo":
            L([(25, 17), (13, 28), (25, 39)])
            icon.arc(32, 32, 19, 195, 355)
        else:
            L([(39, 17), (51, 28), (39, 39)])
            icon.arc(32, 32, 19, 185, 345)
    elif name == "grid":
        R(12, 12, 52, 52, 3)
        L([(25, 12), (25, 52)], 3)
        L([(39, 12), (39, 52)], 3)
        L([(12, 25), (52, 25)], 3)
        L([(12, 39), (52, 39)], 3)
    elif name == "view-2d":
        R(11, 15, 53, 49, 3)
        L([(32, 15), (32, 49)], 3)
        L([(11, 32), (53, 32)], 3)
    elif name in ("view-3d", "cube", "entity"):
        L([(32, 10), (52, 21), (52, 44), (32, 55), (12, 44), (12, 21), (32, 10)])
        L([(12, 21), (32, 33), (52, 21)])
        L([(32, 33), (32, 55)])
        if name == "entity":
            D(32, 33, 2.6)
    elif name == "shaded":
        P([(32, 10), (52, 21), (32, 33), (12, 21)])
        L([(12, 21), (12, 44), (32, 55), (32, 33)])
        L([(32, 33), (52, 21), (52, 44), (32, 55)])
    elif name == "wireframe":
        L([(32, 10), (52, 21), (52, 44), (32, 55), (12, 44), (12, 21), (32, 10)])
        L([(12, 21), (32, 33), (52, 21)])
        L([(32, 33), (32, 55)])
    elif name == "sphere":
        C(32, 32, 21)
        icon.arc(32, 32, 12, -90, 90)
        icon.arc(32, 32, 12, 90, 270)
        L([(11, 32), (53, 32)], 3)
    elif name == "plane":
        L([(12, 21), (48, 13), (53, 42), (17, 51), (12, 21)])
        L([(17, 51), (28, 32), (48, 13)])
        L([(28, 32), (53, 42)])
    elif name == "cylinder":
        icon.arc(32, 17, 20, 180, 540)
        L([(12, 17), (12, 47)])
        L([(52, 17), (52, 47)])
        icon.arc(32, 47, 20, 0, 180)
        icon.arc(32, 47, 20, 180, 360)
    elif name == "folder":
        L([(9, 19), (25, 19), (30, 24), (55, 24), (52, 48), (12, 48), (9, 19)])
        L([(10, 25), (54, 25)])
    elif name == "scene":
        R(14, 10, 50, 54, 4)
        L([(23, 21), (41, 21)], 3)
        L([(23, 31), (41, 31)], 3)
        L([(23, 41), (35, 41)], 3)
    elif name in ("eye", "eye-off"):
        L([(9, 32), (18, 22), (32, 18), (46, 22), (55, 32), (46, 42), (32, 46), (18, 42), (9, 32)])
        C(32, 32, 7, 3)
        if name == "eye-off":
            L([(12, 12), (52, 52)])
    elif name in ("lock", "unlock"):
        R(16, 29, 48, 53, 4)
        if name == "lock":
            icon.arc(32, 29, 12, 180, 360)
            L([(20, 29), (20, 22)])
            L([(44, 29), (44, 22)])
        else:
            icon.arc(35, 29, 12, 190, 350)
            L([(23, 29), (23, 22)])
            L([(47, 29), (47, 25)])
        C(32, 41, 2.2, 2)
        L([(32, 43), (32, 47)], 2)
    elif name.startswith("chevron-"):
        direction = name.split("-", 1)[1]
        if direction == "left":
            L([(40, 12), (24, 32), (40, 52)])
        elif direction == "right":
            L([(24, 12), (40, 32), (24, 52)])
        else:
            L([(12, 24), (32, 40), (52, 24)])
    elif name == "more":
        D(18, 32)
        D(32, 32)
        D(46, 32)
    elif name == "search":
        C(28, 28, 14)
        L([(39, 39), (52, 52)])
    elif name == "filter":
        L([(10, 14), (54, 14), (38, 33), (38, 50), (26, 50), (26, 33), (10, 14)])
    elif name == "add":
        C(32, 32, 20)
        L([(32, 20), (32, 44)])
        L([(20, 32), (44, 32)])
    elif name == "minimize":
        L([(12, 32), (52, 32)], 3.2)
    elif name == "maximize":
        L([(26, 12), (52, 12), (52, 38)], 3.2)
        L([(12, 22), (12, 52), (42, 52), (42, 22), (12, 22)], 3.2)
        L([(26, 12), (26, 22)], 3.2)
    elif name == "close":
        L([(17, 17), (47, 47)])
        L([(47, 17), (17, 47)])
    elif name == "play":
        P([(24, 15), (49, 32), (24, 49)])
    elif name == "stop":
        R(17, 17, 47, 47, 3)
    elif name == "console":
        R(10, 13, 54, 51, 4)
        L([(19, 25), (29, 32), (19, 39)])
        L([(35, 40), (46, 40)])
    elif name == "assets":
        R(12, 18, 52, 49, 3)
        L([(12, 28), (52, 28)], 3)
        L([(22, 18), (22, 49)], 3)
        L([(32, 18), (32, 49)], 3)
        L([(42, 18), (42, 49)], 3)
    elif name == "project":
        R(11, 15, 53, 51, 4)
        L([(20, 25), (44, 25)], 3)
        L([(20, 35), (44, 35)], 3)
        L([(20, 45), (36, 45)], 3)
    elif name == "node":
        L([(19, 20), (32, 32), (45, 20), (32, 32), (45, 44), (32, 32), (19, 44)])
        C(19, 20, 6)
        C(45, 20, 6)
        C(19, 44, 6)
        C(45, 44, 6)
    elif name == "agent":
        C(32, 32, 20)
        L([(22, 39), (27, 28), (32, 37), (37, 22), (42, 39)])
        L([(25, 43), (39, 43)], 3)
    elif name == "schematic":
        R(15, 15, 49, 49, 4)
        L([(9, 24), (15, 24)], 3)
        L([(9, 40), (15, 40)], 3)
        L([(49, 24), (55, 24)], 3)
        L([(49, 40), (55, 40)], 3)
        L([(24, 9), (24, 15)], 3)
        L([(40, 9), (40, 15)], 3)
        L([(24, 49), (24, 55)], 3)
        L([(40, 49), (40, 55)], 3)
    elif name == "pcb":
        R(12, 12, 52, 52, 4)
        L([(18, 42), (28, 42), (28, 22), (46, 22)])
        D(18, 42, 3)
        D(46, 22, 3)
        D(46, 42, 3)
    elif name == "settings":
        C(32, 32, 9)
        for angle in range(0, 360, 45):
            radians = math.radians(angle)
            L([(32 + math.cos(radians) * 15, 32 + math.sin(radians) * 15),
               (32 + math.cos(radians) * 23, 32 + math.sin(radians) * 23)])
    elif name == "menu":
        L([(12, 18), (52, 18)])
        L([(12, 32), (52, 32)])
        L([(12, 46), (52, 46)])
    elif name == "warning":
        L([(32, 10), (54, 51), (10, 51), (32, 10)])
        L([(32, 24), (32, 38)])
        D(32, 44, 1.8)
    elif name == "error":
        C(32, 32, 20)
        L([(24, 24), (40, 40)])
        L([(40, 24), (24, 40)])
    elif name == "success":
        C(32, 32, 20)
        L([(20, 32), (29, 41), (46, 22)])
    else:
        raise ValueError(f"Unknown icon: {name}")
    return icon


ICON_NAMES = [
    "select", "move", "rotate", "scale", "focus", "undo", "redo", "grid",
    "view-2d", "view-3d", "shaded", "wireframe", "folder", "scene", "entity",
    "cube", "sphere", "plane", "cylinder", "eye", "eye-off", "lock", "unlock",
    "chevron-left", "chevron-right", "chevron-down", "more", "search", "filter",
    "add", "close", "play", "stop", "console", "assets", "project", "node",
    "agent", "schematic", "pcb", "settings", "menu", "warning", "error", "success",
]

TOP_ICONS = {
    "file": "project",
    "edit": "select",
    "view": "eye",
    "project": "folder",
    "help": "focus",
    "save": "success",
    "minimize": "minimize",
    "maximize": "maximize",
    "close": "close",
}


def write_icon(name: str, png_dir: Path, svg_dir: Path) -> None:
    icon = draw_icon(name)
    png_dir.mkdir(parents=True, exist_ok=True)
    svg_dir.mkdir(parents=True, exist_ok=True)
    raster_image(icon).save(png_dir / f"{name}.png")
    (svg_dir / f"{name}.svg").write_text(icon.svg_text(), encoding="utf-8")


def raster_image(icon: Icon) -> Image.Image:
    """Downsample the alpha mask while keeping transparent RGB pixels white.

    Resizing straight-alpha RGBA can leak black RGB values into antialiased
    edges. The renderers blend the alpha channel, so a white RGB payload is
    the stable result for every partially covered stroke.
    """
    alpha = icon.image.getchannel("A").resize((SIZE, SIZE), Image.Resampling.LANCZOS)
    white = Image.new("L", (SIZE, SIZE), 255)
    return Image.merge("RGBA", (white, white, white, alpha))


def main() -> None:
    for name in ICON_NAMES:
        write_icon(name, RUNTIME_PNG_DIR, RUNTIME_SVG_DIR)
    for top_name, source_name in TOP_ICONS.items():
        source = draw_icon(source_name)
        TOP_PNG_DIR.mkdir(parents=True, exist_ok=True)
        TOP_SVG_DIR.mkdir(parents=True, exist_ok=True)
        raster_image(source).save(TOP_PNG_DIR / f"{top_name}.png")
        (TOP_SVG_DIR / f"{top_name}.svg").write_text(source.svg_text(), encoding="utf-8")
    print(f"Generated {len(ICON_NAMES)} RafUI icons and {len(TOP_ICONS)} top-bar icons.")


if __name__ == "__main__":
    main()
