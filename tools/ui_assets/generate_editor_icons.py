#!/usr/bin/env python3
"""Generate lightweight editor icon PNGs with a neutral orange-ready palette."""

from pathlib import Path

from PIL import Image, ImageDraw


ROOT = Path(__file__).resolve().parents[2]
ICON_DIR = ROOT / "editor" / "assets" / "ui_icons"
SCALE = 4
CANVAS = 64
WHITE = (245, 245, 245, 255)
ORANGE = (224, 116, 24, 255)
MUTED = (184, 184, 184, 255)


def canvas():
    return Image.new("RGBA", (CANVAS * SCALE, CANVAS * SCALE), (0, 0, 0, 0))


def draw_icon(name, painter):
    image = canvas()
    draw = ImageDraw.Draw(image)
    painter(draw)
    output = image.resize((CANVAS, CANVAS), Image.Resampling.LANCZOS)
    output.save(ICON_DIR / name)


def line(draw, points, fill=WHITE, width=3):
    draw.line([(x * SCALE, y * SCALE) for x, y in points], fill=fill, width=width * SCALE, joint="curve")


def rounded(draw, rect, radius=6, outline=WHITE, width=3, fill=None):
    draw.rounded_rectangle(tuple(value * SCALE for value in rect), radius=radius * SCALE, outline=outline, width=width * SCALE, fill=fill)


def eye(draw, hidden=False):
    draw.ellipse((8 * SCALE, 18 * SCALE, 56 * SCALE, 46 * SCALE), outline=WHITE, width=3 * SCALE)
    draw.ellipse((25 * SCALE, 25 * SCALE, 39 * SCALE, 39 * SCALE), fill=WHITE)
    if hidden:
        line(draw, [(10, 52), (54, 12)], ORANGE, 4)


def preview(draw):
    rounded(draw, (10, 12, 50, 45), 5)
    line(draw, [(16, 38), (27, 27), (35, 34), (43, 22)], MUTED, 2)
    draw.ellipse((17 * SCALE, 18 * SCALE, 25 * SCALE, 26 * SCALE), fill=ORANGE)
    rounded(draw, (16, 20, 56, 53), 5, outline=ORANGE, width=2)


def select(draw):
    polygon = [(14, 8), (45, 35), (32, 37), (39, 52), (32, 56), (24, 40), (14, 49)]
    draw.polygon([(x * SCALE, y * SCALE) for x, y in polygon], fill=WHITE)
    line(draw, [(14, 8), (45, 35), (32, 37), (39, 52), (32, 56), (24, 40), (14, 49), (14, 8)], ORANGE, 1)


def dots(draw):
    for y in (18, 32, 46):
        draw.ellipse((28 * SCALE, (y - 3) * SCALE, 34 * SCALE, (y + 3) * SCALE), fill=WHITE)


def game(draw):
    rounded(draw, (7, 25, 57, 48), 11, outline=WHITE, width=3)
    line(draw, [(22, 30), (22, 42)], WHITE, 3)
    line(draw, [(17, 36), (27, 36)], WHITE, 3)
    draw.ellipse((42 * SCALE, 32 * SCALE, 47 * SCALE, 37 * SCALE), fill=ORANGE)
    draw.ellipse((48 * SCALE, 38 * SCALE, 53 * SCALE, 43 * SCALE), fill=ORANGE)
    line(draw, [(20, 25), (14, 17), (9, 16)], MUTED, 2)
    line(draw, [(44, 25), (50, 17), (55, 16)], MUTED, 2)


def electronics(draw):
    rounded(draw, (10, 10, 54, 54), 6, outline=WHITE, width=3)
    for x in (17, 47):
        for y in (18, 29, 40):
            draw.ellipse(((x - 2) * SCALE, (y - 2) * SCALE, (x + 2) * SCALE, (y + 2) * SCALE), fill=ORANGE)
    rounded(draw, (23, 20, 41, 44), 3, outline=MUTED, width=2)
    line(draw, [(17, 18), (23, 24), (23, 32), (17, 40)], WHITE, 2)
    line(draw, [(47, 18), (41, 24), (41, 32), (47, 40)], WHITE, 2)


def project_type(draw):
    rounded(draw, (9, 12, 55, 52), 7, outline=WHITE, width=3)
    line(draw, [(18, 23), (46, 23)], MUTED, 2)
    line(draw, [(18, 32), (39, 32)], WHITE, 3)
    line(draw, [(18, 41), (32, 41)], ORANGE, 3)


def settings(draw):
    draw.ellipse((16 * SCALE, 16 * SCALE, 48 * SCALE, 48 * SCALE), outline=WHITE, width=3 * SCALE)
    draw.ellipse((26 * SCALE, 26 * SCALE, 38 * SCALE, 38 * SCALE), outline=ORANGE, width=3 * SCALE)
    for x1, y1, x2, y2 in [(32, 7, 32, 17), (32, 47, 32, 57), (7, 32, 17, 32), (47, 32, 57, 32)]:
        line(draw, [(x1, y1), (x2, y2)], WHITE, 3)


def search(draw):
    draw.ellipse((12 * SCALE, 12 * SCALE, 39 * SCALE, 39 * SCALE), outline=WHITE, width=3 * SCALE)
    line(draw, [(34, 34), (54, 54)], ORANGE, 4)


def open_icon(draw):
    rounded(draw, (9, 18, 55, 49), 5, outline=WHITE, width=3)
    line(draw, [(25, 34), (44, 34)], ORANGE, 3)
    line(draw, [(37, 27), (44, 34), (37, 41)], ORANGE, 3)


def duplicate(draw):
    rounded(draw, (13, 11, 43, 42), 4, outline=MUTED, width=3)
    rounded(draw, (22, 21, 52, 52), 4, outline=WHITE, width=3)


def delete(draw):
    rounded(draw, (19, 18, 45, 51), 3, outline=WHITE, width=3)
    line(draw, [(16, 16), (48, 16)], ORANGE, 3)
    line(draw, [(27, 26), (27, 43)], MUTED, 2)
    line(draw, [(37, 26), (37, 43)], MUTED, 2)


def pin(draw):
    draw.ellipse((17 * SCALE, 10 * SCALE, 47 * SCALE, 40 * SCALE), outline=WHITE, width=3 * SCALE)
    line(draw, [(32, 36), (32, 55)], ORANGE, 3)
    line(draw, [(23, 48), (32, 55), (41, 48)], ORANGE, 2)


def empty(draw):
    rounded(draw, (12, 19, 52, 49), 5, outline=MUTED, width=3)
    line(draw, [(22, 29), (42, 29)], WHITE, 3)
    line(draw, [(22, 38), (36, 38)], ORANGE, 3)


def console(draw):
    rounded(draw, (8, 12, 56, 52), 6, outline=WHITE, width=3)
    line(draw, [(18, 25), (27, 32), (18, 39)], ORANGE, 3)
    line(draw, [(33, 40), (46, 40)], MUTED, 3)


def assets(draw):
    rounded(draw, (11, 12, 53, 50), 5, outline=WHITE, width=3)
    line(draw, [(12, 23), (52, 23)], MUTED, 2)
    for x, y in [(20, 32), (32, 32), (44, 32), (20, 42), (32, 42), (44, 42)]:
        rounded(draw, (x - 3, y - 3, x + 3, y + 3), 1, outline=ORANGE, width=2)


def node_editor(draw):
    for x, y, color in [(15, 19, ORANGE), (47, 20, WHITE), (22, 46, WHITE), (49, 45, ORANGE)]:
        rounded(draw, (x - 6, y - 5, x + 6, y + 5), 2, outline=color, width=3)
    line(draw, [(21, 20), (41, 20)], MUTED, 2)
    line(draw, [(19, 25), (24, 40)], MUTED, 2)
    line(draw, [(42, 25), (47, 39)], MUTED, 2)
    line(draw, [(28, 45), (43, 45)], MUTED, 2)


def project_settings(draw):
    for y, knob_x in [(20, 24), (32, 43), (44, 31)]:
        line(draw, [(12, y), (52, y)], WHITE, 3)
        draw.ellipse(((knob_x - 4) * SCALE, (y - 4) * SCALE, (knob_x + 4) * SCALE, (y + 4) * SCALE), fill=ORANGE)


def chat(draw):
    rounded(draw, (10, 13, 53, 45), 7, outline=WHITE, width=3)
    draw.polygon([(22 * SCALE, 45 * SCALE), (18 * SCALE, 54 * SCALE), (31 * SCALE, 45 * SCALE)], fill=WHITE)
    for x in (22, 32, 42):
        draw.ellipse(((x - 2) * SCALE, 27 * SCALE, (x + 2) * SCALE, 31 * SCALE), fill=ORANGE)


def complement(draw):
    rounded(draw, (13, 13, 51, 51), 7, outline=WHITE, width=3)
    line(draw, [(21, 32), (43, 32)], ORANGE, 3)
    line(draw, [(32, 21), (32, 43)], ORANGE, 3)
    draw.ellipse((26 * SCALE, 26 * SCALE, 38 * SCALE, 38 * SCALE), outline=MUTED, width=2 * SCALE)


def build_hub_variants(base_name):
    source = Image.open(ICON_DIR / base_name).convert("RGBA")
    stem = base_name.removesuffix("_HUB.png")
    for size in (12, 16, 20, 24, 28, 32):
        source.resize((size, size), Image.Resampling.LANCZOS).save(ICON_DIR / f"{stem}_{size}x{size}_HUB.png")


def main():
    ICON_DIR.mkdir(parents=True, exist_ok=True)
    draw_icon("hidden.png", lambda draw: eye(draw, True))
    draw_icon("visible.png", eye)
    draw_icon("preview.png", preview)
    draw_icon("select.png", select)
    draw_icon("3-dots vertical.png", dots)
    draw_icon("project_game.png", game)
    draw_icon("project_electronics.png", electronics)
    draw_icon("project_type_HUB.png", project_type)
    draw_icon("settings_HUB.png", settings)
    draw_icon("search_filter_HUB.png", search)
    draw_icon("open_HUB.png", open_icon)
    draw_icon("duplicate_HUB.png", duplicate)
    draw_icon("delete_HUB.png", delete)
    draw_icon("favorite_pin_HUB.png", pin)
    draw_icon("empty_state.png", empty)
    draw_icon("console.png", console)
    draw_icon("assets.png", assets)
    draw_icon("node_editor.png", node_editor)
    draw_icon("project_settings.png", project_settings)
    draw_icon("ai_chat.png", chat)
    draw_icon("complement.png", complement)
    for name in (
        "project_type_HUB.png",
        "settings_HUB.png",
        "search_filter_HUB.png",
        "open_HUB.png",
        "duplicate_HUB.png",
        "delete_HUB.png",
        "favorite_pin_HUB.png",
    ):
        build_hub_variants(name)


if __name__ == "__main__":
    main()
