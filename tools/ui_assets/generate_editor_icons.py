#!/usr/bin/env python3
"""Generate lightweight editor icon PNGs with a neutral orange-ready palette."""

import argparse
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


def draw_icon(name, painter, output_size=CANVAS):
    image = canvas()
    draw = ImageDraw.Draw(image)
    painter(draw)
    output = image.resize((output_size, output_size), Image.Resampling.LANCZOS)
    output_path = ICON_DIR / name
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output.save(output_path)


def preview_canvas():
    return Image.new("RGBA", (640, 360), (12, 15, 19, 255))


def gradient(image, top, bottom):
    pixels = image.load()
    height = image.height - 1
    for y in range(image.height):
        amount = y / height if height else 0.0
        color = tuple(round(top[index] * (1.0 - amount) + bottom[index] * amount) for index in range(4))
        for x in range(image.width):
            pixels[x, y] = color


def hub_game_preview():
    image = preview_canvas()
    gradient(image, (8, 12, 16, 255), (33, 23, 16, 255))
    draw = ImageDraw.Draw(image)
    draw.ellipse((426, 36, 594, 204), fill=(92, 61, 28, 255))
    draw.ellipse((448, 54, 574, 180), fill=(235, 143, 42, 255))
    for index, (left, top, right) in enumerate(
        [(0, 178, 86), (68, 142, 141), (123, 196, 205), (187, 122, 282), (264, 167, 342), (324, 104, 425), (408, 150, 486), (468, 117, 560), (542, 186, 640)]
    ):
        shade = 20 + (index % 3) * 8
        draw.rectangle((left, top, right, 286), fill=(shade, shade + 3, shade + 5, 255))
        for window_y in range(top + 24, 270, 28):
            draw.rectangle((left + 16, window_y, min(right - 12, left + 34), window_y + 5), fill=(207, 107, 27, 210))
    draw.polygon([(0, 360), (260, 220), (395, 220), (640, 360)], fill=(14, 16, 18, 255))
    draw.line([(118, 360), (298, 222)], fill=(232, 132, 28, 230), width=4)
    draw.line([(517, 360), (356, 222)], fill=(232, 132, 28, 230), width=4)
    draw.line([(318, 360), (328, 224)], fill=(154, 88, 25, 220), width=3)
    draw.rounded_rectangle((24, 24, 204, 72), radius=8, fill=(7, 10, 13, 195), outline=(232, 133, 28, 190), width=2)
    for left, width in [(42, 48), (100, 28), (138, 42)]:
        draw.rounded_rectangle((left, 43, left + width, 51), radius=4, fill=(238, 235, 227, 230))
    image.save(ICON_DIR / "hub_game_preview.png")


def hub_electronics_preview():
    image = preview_canvas()
    gradient(image, (10, 14, 15, 255), (24, 28, 23, 255))
    draw = ImageDraw.Draw(image)
    board = (96, 42, 550, 318)
    draw.rounded_rectangle(board, radius=28, fill=(20, 31, 27, 255), outline=(226, 128, 27, 255), width=4)
    draw.rounded_rectangle((216, 104, 423, 252), radius=14, fill=(30, 37, 31, 255), outline=(221, 140, 39, 255), width=3)
    for x in range(238, 406, 22):
        draw.rectangle((x, 90, x + 10, 107), fill=(229, 150, 54, 255))
        draw.rectangle((x, 252, x + 10, 270), fill=(229, 150, 54, 255))
    for y in range(122, 238, 22):
        draw.rectangle((200, y, 217, y + 10), fill=(229, 150, 54, 255))
        draw.rectangle((423, y, 440, y + 10), fill=(229, 150, 54, 255))
    traces = [
        [(112, 92), (168, 92), (168, 146), (216, 146)],
        [(532, 92), (476, 92), (476, 172), (423, 172)],
        [(112, 268), (170, 268), (170, 216), (216, 216)],
        [(532, 268), (476, 268), (476, 230), (423, 230)],
        [(140, 180), (184, 180), (184, 180), (216, 180)],
    ]
    for trace in traces:
        draw.line(trace, fill=(231, 137, 31, 255), width=7, joint="curve")
        for x, y in (trace[0], trace[-1]):
            draw.ellipse((x - 8, y - 8, x + 8, y + 8), fill=(245, 174, 74, 255))
    for x, y in [(140, 116), (498, 122), (148, 242), (502, 242), (182, 180)]:
        draw.ellipse((x - 15, y - 15, x + 15, y + 15), outline=(194, 198, 187, 255), width=3)
        draw.ellipse((x - 5, y - 5, x + 5, y + 5), fill=(231, 137, 31, 255))
    draw.rounded_rectangle((24, 24, 238, 72), radius=8, fill=(7, 10, 13, 195), outline=(232, 133, 28, 190), width=2)
    for left, width in [(42, 56), (108, 34), (152, 58), (220, 10)]:
        draw.rounded_rectangle((left, 43, left + width, 51), radius=4, fill=(238, 235, 227, 230))
    image.save(ICON_DIR / "hub_electronics_preview.png")


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


def folder(draw):
    outline = [(8, 20), (8, 50), (56, 50), (56, 20), (31, 20), (26, 14), (8, 14), (8, 20)]
    line(draw, outline, ORANGE, 3)
    line(draw, [(10, 24), (54, 24)], WHITE, 2)
    line(draw, [(14, 44), (50, 44)], MUTED, 2)


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


def send(draw):
    draw.polygon(
        [(10 * SCALE, 30 * SCALE), (54 * SCALE, 10 * SCALE), (42 * SCALE, 54 * SCALE), (31 * SCALE, 37 * SCALE)],
        fill=WHITE,
    )
    line(draw, [(13, 30), (50, 14)], ORANGE, 3)
    line(draw, [(31, 37), (42, 54)], ORANGE, 3)


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


def undo(draw):
    draw.arc((12 * SCALE, 12 * SCALE, 53 * SCALE, 53 * SCALE), 208, 52, fill=WHITE, width=4 * SCALE)
    draw.polygon([(11 * SCALE, 24 * SCALE), (27 * SCALE, 17 * SCALE), (25 * SCALE, 33 * SCALE)], fill=WHITE)
    line(draw, [(16, 24), (26, 21)], ORANGE, 2)


def redo(draw):
    draw.arc((11 * SCALE, 12 * SCALE, 52 * SCALE, 53 * SCALE), 128, 332, fill=WHITE, width=4 * SCALE)
    draw.polygon([(53 * SCALE, 24 * SCALE), (37 * SCALE, 17 * SCALE), (39 * SCALE, 33 * SCALE)], fill=WHITE)
    line(draw, [(48, 24), (38, 21)], ORANGE, 2)


def top_file(draw):
    rounded(draw, (14, 9, 50, 55), 4, outline=WHITE, width=3)
    line(draw, [(24, 9), (24, 22), (38, 22)], ORANGE, 3)
    line(draw, [(24, 36), (42, 36)], MUTED, 2)
    line(draw, [(24, 44), (42, 44)], MUTED, 2)


def top_edit(draw):
    line(draw, [(14, 48), (42, 20)], WHITE, 6)
    line(draw, [(38, 16), (48, 26)], ORANGE, 5)
    line(draw, [(12, 52), (24, 50)], MUTED, 3)


def top_view(draw):
    draw.ellipse((9 * SCALE, 20 * SCALE, 55 * SCALE, 44 * SCALE), outline=WHITE, width=3 * SCALE)
    draw.ellipse((26 * SCALE, 27 * SCALE, 38 * SCALE, 39 * SCALE), fill=ORANGE)


def top_project(draw):
    outline = [(9, 19), (9, 52), (55, 52), (55, 19), (31, 19), (25, 13), (9, 13), (9, 19)]
    line(draw, outline, WHITE, 3)
    line(draw, [(14, 28), (50, 28)], ORANGE, 3)


def top_help(draw):
    draw.ellipse((10 * SCALE, 10 * SCALE, 54 * SCALE, 54 * SCALE), outline=WHITE, width=3 * SCALE)
    line(draw, [(26, 25), (29, 20), (37, 20), (42, 25), (40, 31), (32, 35)], ORANGE, 3)
    draw.ellipse((30 * SCALE, 42 * SCALE, 34 * SCALE, 46 * SCALE), fill=WHITE)


def top_save(draw):
    rounded(draw, (12, 10, 52, 54), 4, outline=WHITE, width=3)
    rounded(draw, (22, 12, 42, 27), 2, outline=ORANGE, width=2)
    rounded(draw, (22, 36, 42, 50), 3, outline=MUTED, width=2)


def top_minimize(draw):
    line(draw, [(14, 42), (50, 42)], WHITE, 4)


def top_maximize(draw):
    rounded(draw, (14, 14, 50, 50), 2, outline=WHITE, width=3)
    line(draw, [(22, 21), (42, 21)], ORANGE, 2)


def top_close(draw):
    line(draw, [(16, 16), (48, 48)], WHITE, 4)
    line(draw, [(48, 16), (16, 48)], WHITE, 4)


def build_hub_variants(base_name):
    source = Image.open(ICON_DIR / base_name).convert("RGBA")
    stem = base_name.removesuffix("_HUB.png")
    for size in (12, 16, 20, 24, 28, 32):
        source.resize((size, size), Image.Resampling.LANCZOS).save(ICON_DIR / f"{stem}_{size}x{size}_HUB.png")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--history-only",
        action="store_true",
        help="Regenerate only the undo/redo pair.",
    )
    args = parser.parse_args()
    ICON_DIR.mkdir(parents=True, exist_ok=True)
    if args.history_only:
        draw_icon("undo.png", undo)
        draw_icon("redo.png", redo)
        return
    draw_icon("hidden.png", lambda draw: eye(draw, True))
    draw_icon("visible.png", eye)
    draw_icon("preview.png", preview)
    draw_icon("select.png", select)
    draw_icon("3-dots vertical.png", dots)
    draw_icon("folder.png", folder, output_size=256)
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
    draw_icon("send.png", send)
    draw_icon("assets.png", assets)
    draw_icon("node_editor.png", node_editor)
    draw_icon("project_settings.png", project_settings)
    draw_icon("ai_chat.png", chat)
    draw_icon("complement.png", complement)
    draw_icon("undo.png", undo)
    draw_icon("redo.png", redo)
    for name, painter in (
        ("top/file.png", top_file),
        ("top/edit.png", top_edit),
        ("top/view.png", top_view),
        ("top/project.png", top_project),
        ("top/help.png", top_help),
        ("top/save.png", top_save),
        ("top/minimize.png", top_minimize),
        ("top/maximize.png", top_maximize),
        ("top/close.png", top_close),
    ):
        draw_icon(name, painter)
    hub_game_preview()
    hub_electronics_preview()
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
