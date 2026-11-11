"""Generate the RafUI white-line icon family.

One vector description emits editable SVG and compact runtime PNG assets.
Registered names come from the shared Rust catalog. ApiGraphicBasic consumes
the PNG representation on GPU and CPU without exposing paths to surfaces.
Edit the vector description before regeneration; runtime never invokes this tool.
"""

from __future__ import annotations

import math
import argparse
import re
from dataclasses import dataclass
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


@dataclass(frozen=True)
class IconManifestEntry:
    variant: str
    key: str
    master_svg: str
    source_raster_size: tuple[int, int]
    source_canvas_size: tuple[int, int]
    optical_size_px: int
    aliases: tuple[str, ...]


ICON_ENTRY_RE = re.compile(
    r'^[ \t]*(?P<variant>[A-Z][A-Za-z0-9_]*)\s*\{\s*'
    r'key:\s*"(?P<key>[a-z0-9-]+)",\s*'
    r'master_svg:\s*"(?P<master_svg>[^"]+)",\s*'
    r'source_raster_size:\s*\[(?P<raster_width>\d+),\s*(?P<raster_height>\d+)\],\s*'
    r'source_canvas_size:\s*\[(?P<canvas_width>\d+),\s*(?P<canvas_height>\d+)\],\s*'
    r'optical_size_px:\s*(?P<optical_size_px>\d+),\s*'
    r'aliases:\s*\[(?P<aliases>[^\]]*)\],?\s*\},',
    re.MULTILINE | re.DOTALL,
)


def load_icon_manifest() -> tuple[IconManifestEntry, ...]:
    catalog_path = ROOT / "crates" / "raf_ui" / "src" / "icon_catalog.rs"
    catalog_source = catalog_path.read_text(encoding="utf-8")
    entries = []
    for match in ICON_ENTRY_RE.finditer(catalog_source):
        aliases = tuple(re.findall(r'"([^"]+)"', match.group("aliases")))
        entry = IconManifestEntry(
            variant=match.group("variant"),
            key=match.group("key"),
            master_svg=match.group("master_svg"),
            source_raster_size=(int(match.group("raster_width")), int(match.group("raster_height"))),
            source_canvas_size=(int(match.group("canvas_width")), int(match.group("canvas_height"))),
            optical_size_px=int(match.group("optical_size_px")),
            aliases=aliases,
        )
        master_path = ROOT / entry.master_svg
        if not master_path.is_file():
            raise ValueError(f"Missing SVG master for {entry.key}: {entry.master_svg}")
        if master_path.parent != RUNTIME_SVG_DIR:
            raise ValueError(f"SVG master is outside the runtime icon directory: {entry.master_svg}")
        if master_path.stem != entry.key:
            raise ValueError(f"SVG master filename must match its stable key: {entry.master_svg}")
        if any(size <= 0 for size in (*entry.source_raster_size, *entry.source_canvas_size, entry.optical_size_px)):
            raise ValueError(f"Icon sizes must be positive for {entry.key}")
        if entry.source_canvas_size != (SIZE, SIZE):
            raise ValueError(f"The current vector artwork uses a {SIZE}x{SIZE} design grid: {entry.key}")
        entries.append(entry)

    if not entries:
        raise ValueError(f"No icon records found in {catalog_path}")
    keys = [entry.key for entry in entries]
    variants = [entry.variant for entry in entries]
    master_paths = [entry.master_svg for entry in entries]
    aliases = [alias for entry in entries for alias in entry.aliases]
    if len(set(keys)) != len(keys):
        raise ValueError("Icon manifest contains duplicate stable keys")
    if len(set(variants)) != len(variants):
        raise ValueError("Icon manifest contains duplicate variants")
    if len(set(master_paths)) != len(master_paths):
        raise ValueError("Icon manifest contains duplicate SVG masters")
    if len(set(aliases)) != len(aliases) or set(keys).intersection(aliases):
        raise ValueError("Icon manifest contains duplicate or conflicting aliases")
    return tuple(entries)


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

    def svg_text(
        self,
        source_raster_size: tuple[int, int] = (SIZE, SIZE),
        source_canvas_size: tuple[int, int] = (SIZE, SIZE),
    ) -> str:
        body = "".join(self.svg)
        return (
            f'<svg xmlns="http://www.w3.org/2000/svg" width="{source_raster_size[0]}" '
            f'height="{source_raster_size[1]}" viewBox="0 0 {source_canvas_size[0]} {source_canvas_size[1]}" '
            'fill="none">' + body + "</svg>\n"
        )


def draw_icon(name: str) -> Icon:
    icon = Icon()
    L = icon.line
    R = icon.rect
    C = icon.circle
    P = icon.fill_polygon
    D = icon.dot

    if name == "camera":
        L([(10,20),(38,20),(38,52),(10,52),(10,20)])
        L([(38,30),(54,22),(54,50),(38,42)])
        L([(15,20),(17,10),(29,10),(33,20)])
        L([(15,36),(31,36)])
    elif name == "scene-preview":
        R(8,12,56,46,4)
        P([(27,21),(42,29),(27,38)])
        L([(23,54),(41,54)])
        L([(32,46),(32,54)])
    elif name == "select":
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
        P([(23, 14), (49, 32), (23, 50)])
    elif name == "pause":
        P([(18, 15), (26, 15), (26, 49), (18, 49)])
        P([(38, 15), (46, 15), (46, 49), (38, 49)])
    elif name == "step-forward":
        P([(15, 16), (39, 32), (15, 48)])
        L([(47, 16), (47, 48)], 5)
    elif name == "stop":
        P([(19, 18), (45, 18), (46, 19), (46, 45), (45, 46), (19, 46), (18, 45), (18, 19)])
    elif name == "sun":
        C(32, 32, 10)
        for angle in range(0, 360, 45):
            a = math.radians(angle)
            L([(32 + math.cos(a) * 17, 32 + math.sin(a) * 17),
               (32 + math.cos(a) * 23, 32 + math.sin(a) * 23)], 3.5)
    elif name == "sky":
        C(39, 23, 10)
        L([(10, 39), (16, 33), (24, 33), (29, 26), (38, 26), (45, 34), (51, 34), (55, 40)])
        L([(10, 47), (54, 47)], 3)
    elif name == "shadows":
        L([(14, 20), (32, 10), (49, 20), (49, 39), (32, 50), (14, 39), (14, 20), (32, 30), (49, 20)])
        L([(32, 30), (32, 50)])
        L([(39, 48), (55, 48), (58, 43), (49, 39)], 3)
    elif name == "clock":
        C(32, 32, 21)
        L([(32, 18), (32, 32), (43, 39)])
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
    elif name == "refresh":
        icon.arc(32, 32, 20, 55, 330)
        L([(45, 11), (51, 22), (40, 27)])
    elif name == "trash":
        L([(14, 20), (50, 20)])
        L([(24, 20), (24, 13), (40, 13), (40, 20)])
        L([(19, 20), (21, 51), (43, 51), (45, 20)])
        L([(28, 27), (28, 44)], 3)
        L([(36, 27), (36, 44)], 3)
    elif name == "wire":
        D(11, 32, 3.2)
        D(53, 32, 3.2)
        L([(11, 32), (22, 32), (32, 20), (42, 20), (53, 32)])
    elif name == "cad-route":
        # Copper routing has two endpoint pads and a chamfered path.
        D(13, 49, 3.5)
        L([(13, 49), (24, 35), (37, 35), (49, 16)], 3.6)
        D(49, 16, 3.5)
    elif name == "cad-outline":
        R(13, 12, 51, 51, 7)
    elif name == "cad-place":
        # Two-pad placement reads independently of the schematic Add glyph.
        R(22, 21, 42, 43, 3)
        R(12, 25, 22, 39, 1)
        R(42, 25, 52, 39, 1)
    elif name == "cad-via":
        C(32, 32, 16)
        C(32, 32, 6)
        L([(5, 32), (16, 32)])
        L([(48, 32), (59, 32)])
    elif name == "route":
        D(11, 46, 3.2)
        D(53, 20, 3.2)
        L([(11, 46), (24, 46), (24, 20), (53, 20)])
    elif name == "board-outline":
        L([(14, 24), (14, 14), (24, 14)])
        L([(40, 14), (50, 14), (50, 24)])
        L([(50, 40), (50, 50), (40, 50)])
        L([(24, 50), (14, 50), (14, 40)])
        C(32, 32, 5)
    elif name == "zoom-in":
        C(27, 27, 15)
        L([(38, 38), (53, 53)])
        L([(20, 27), (34, 27)])
        L([(27, 20), (27, 34)])
    elif name == "zoom-out":
        C(27, 27, 15)
        L([(38, 38), (53, 53)])
        L([(20, 27), (34, 27)])
    elif name == "script":
        L([(16, 10), (40, 10), (50, 20), (50, 54), (16, 54), (16, 10)])
        L([(40, 10), (40, 20), (50, 20)])
        L([(31, 30), (25, 36), (31, 42)])
        L([(33, 43), (39, 29)])
        L([(35, 30), (41, 36), (35, 42)])
    elif name == "file":
        L([(16, 10), (40, 10), (50, 20), (50, 54), (16, 54), (16, 10)])
        L([(40, 10), (40, 20), (50, 20)])
        L([(23, 30), (43, 30)], 3)
        L([(23, 38), (43, 38)], 3)
        L([(23, 46), (35, 46)], 3)
    elif name == "external-link":
        L([(30, 34), (12, 34), (12, 52), (46, 52), (46, 38)])
        L([(34, 30), (52, 12)])
        L([(40, 12), (52, 12), (52, 24)])
    elif name == "pencil":
        L([(16, 48), (19, 38), (42, 15), (50, 23), (27, 46), (16, 48)])
        L([(19, 38), (27, 46)])
        L([(42, 15), (50, 23)])
        L([(16, 48), (20, 52)])
    elif name == "copy":
        R(10, 24, 40, 54, 4)
        L([(18, 24), (18, 10), (48, 10), (48, 40), (40, 40)])
    elif name == "magnet":
        icon.arc(32, 26, 16, 180, 360)
        L([(16, 26), (16, 40)])
        L([(48, 26), (48, 40)])
        R(11, 40, 21, 50, 2)
        R(43, 40, 53, 50, 2)
    elif name == "frame":
        L([(12, 22), (12, 12), (22, 12)])
        L([(42, 12), (52, 12), (52, 22)])
        L([(52, 42), (52, 52), (42, 52)])
        L([(22, 52), (12, 52), (12, 42)])
        R(26, 26, 38, 38, 2)
    elif name == "capsule":
        R(14, 20, 50, 44, 12)
        L([(32, 20), (32, 44)])
    else:
        raise ValueError(f"Unknown icon: {name}")
    return icon


ICON_MANIFEST = load_icon_manifest()
ICON_BY_KEY = {entry.key: entry for entry in ICON_MANIFEST}
ICON_NAMES = [entry.key for entry in ICON_MANIFEST]

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


def write_icon(entry: IconManifestEntry, png_dir: Path) -> None:
    icon = draw_icon(entry.key)
    png_dir.mkdir(parents=True, exist_ok=True)
    raster_image(icon, entry.source_raster_size).save(png_dir / f"{entry.key}.png")
    master_path = ROOT / entry.master_svg
    master_path.write_text(
        icon.svg_text(entry.source_raster_size, entry.source_canvas_size),
        encoding="utf-8",
    )


def raster_image(icon: Icon, size: tuple[int, int] = (SIZE, SIZE)) -> Image.Image:
    """Downsample the alpha mask while keeping transparent RGB pixels white.

    Resizing straight-alpha RGBA can leak black RGB values into antialiased
    edges. The renderers blend the alpha channel, so a white RGB payload is
    the stable result for every partially covered stroke.
    """
    alpha = icon.image.getchannel("A").resize(size, Image.Resampling.LANCZOS)
    white = Image.new("L", size, 255)
    return Image.merge("RGBA", (white, white, white, alpha))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--only", nargs="+", choices=ICON_NAMES,
                        help="Generate only named runtime assets; leave unrelated masters untouched.")
    parser.add_argument("--preview", type=Path, help="Optional development contact sheet, never loaded by the engine.")
    args = parser.parse_args()
    if args.only:
        selected_icons = [ICON_BY_KEY[name] for name in args.only]
        for name in args.only:
            write_icon(ICON_BY_KEY[name], RUNTIME_PNG_DIR)
        if args.preview:
            preview = Image.new("RGB", (640, len(args.only) * 72), (16, 20, 28))
            draw = ImageDraw.Draw(preview)
            draw.rectangle((320, 0, 640, preview.height), fill=(240, 242, 245))
            for index, entry in enumerate(selected_icons):
                for column, tint in [(0, (243,245,247)), (320, (35,40,48))]:
                    # Tint without changing the original antialiased alpha mask.
                    alpha = raster_image(draw_icon(entry.key), entry.source_raster_size).getchannel("A")
                    source = Image.new("RGBA", entry.source_raster_size, (*tint,255))
                    source.putalpha(alpha)
                    for x, size in [(12,entry.optical_size_px), (80,24), (126,16)]:
                        icon = source.resize((size,size), Image.Resampling.LANCZOS)
                        preview.paste(icon, (column + x, index*72 + (72-size)//2), icon)
                    draw.text((column+170, index*72+30), entry.key, fill=tint)
            args.preview.parent.mkdir(parents=True, exist_ok=True)
            preview.save(args.preview)
        print(f"Generated {len(args.only)} selected runtime SVG/PNG icons.")
        return
    for entry in ICON_MANIFEST:
        write_icon(entry, RUNTIME_PNG_DIR)
    for top_name, source_name in TOP_ICONS.items():
        source = draw_icon(source_name)
        TOP_PNG_DIR.mkdir(parents=True, exist_ok=True)
        TOP_SVG_DIR.mkdir(parents=True, exist_ok=True)
        raster_image(source).save(TOP_PNG_DIR / f"{top_name}.png")
        (TOP_SVG_DIR / f"{top_name}.svg").write_text(source.svg_text(), encoding="utf-8")
    print(f"Generated {len(ICON_MANIFEST)} RafUI icons and {len(TOP_ICONS)} top-bar icons.")


if __name__ == "__main__":
    main()
