"""Generate the Hub's raster/vector icon set outside the Rust UI code."""

from __future__ import annotations

from pathlib import Path

from PIL import Image, ImageDraw


SIZE = 96
ROOT = Path(__file__).resolve().parents[1] / "editor" / "assets" / "ui_icons" / "hub_generated"
STROKE = (222, 226, 232, 255)
ACCENT = (232, 133, 28, 255)
MUTED = (151, 159, 170, 255)
TRANSPARENT = (0, 0, 0, 0)


def svg(name: str, body: str) -> None:
    content = (
        '<svg xmlns="http://www.w3.org/2000/svg" width="96" height="96" '
        'viewBox="0 0 96 96" fill="none">'
        f"{body}</svg>\n"
    )
    (ROOT / f"{name}.svg").write_text(content, encoding="utf-8")


def png(name: str, draw_icon) -> None:
    image = Image.new("RGBA", (SIZE, SIZE), TRANSPARENT)
    draw_icon(ImageDraw.Draw(image))
    image.save(ROOT / f"{name}.png")


def line(draw, points, fill=STROKE, width=6, joint="curve"):
    draw.line(points, fill=fill, width=width, joint=joint)


def game(draw):
    draw.rounded_rectangle((13, 29, 83, 68), radius=18, outline=STROKE, width=6)
    line(draw, ((27, 48), (43, 48)), STROKE, 6)
    line(draw, ((35, 40), (35, 56)), STROKE, 6)
    draw.ellipse((58, 38, 68, 48), fill=ACCENT)
    draw.ellipse((70, 48, 80, 58), fill=ACCENT)
    line(draw, ((18, 29), (25, 19), (35, 19)), STROKE, 5)
    line(draw, ((78, 29), (71, 19), (61, 19)), STROKE, 5)


def electronics(draw):
    draw.rounded_rectangle((24, 24, 72, 72), radius=8, outline=STROKE, width=5)
    draw.rounded_rectangle((34, 34, 62, 62), radius=4, outline=ACCENT, width=5)
    for offset in (0, 12, 24):
        line(draw, ((29 + offset, 16), (29 + offset, 24)), ACCENT, 5)
        line(draw, ((29 + offset, 72), (29 + offset, 80)), ACCENT, 5)
        line(draw, ((16, 29 + offset), (24, 29 + offset)), ACCENT, 5)
        line(draw, ((72, 29 + offset), (80, 29 + offset)), ACCENT, 5)


def folder(draw):
    draw.rounded_rectangle((14, 27, 82, 70), radius=7, outline=STROKE, width=6)
    line(draw, ((16, 30), (24, 20), (45, 20), (52, 29)), STROKE, 6)


def agent(draw):
    draw.ellipse((18, 27, 68, 77), outline=MUTED, width=5)
    draw.ellipse((34, 43, 52, 61), fill=ACCENT)
    line(draw, ((70, 31), (70, 67)), STROKE, 5)
    line(draw, ((77, 38), (77, 60)), MUTED, 5)


def settings(draw):
    draw.ellipse((30, 30, 66, 66), outline=STROKE, width=6)
    draw.ellipse((42, 42, 54, 54), fill=ACCENT)
    for x, y in ((48, 16), (48, 80), (16, 48), (80, 48), (25, 25), (71, 71), (71, 25), (25, 71)):
        draw.ellipse((x - 5, y - 5, x + 5, y + 5), fill=STROKE)


def plus(draw):
    line(draw, ((48, 20), (48, 76)), ACCENT, 7)
    line(draw, ((20, 48), (76, 48)), ACCENT, 7)


def chevron(draw):
    line(draw, ((31, 39), (48, 56), (65, 39)), STROKE, 7)


def more(draw):
    for y in (28, 48, 68):
        draw.ellipse((43, y - 5, 53, y + 5), fill=STROKE)


def arrow(draw):
    line(draw, ((18, 48), (72, 48)), STROKE, 6)
    line(draw, ((53, 29), (72, 48), (53, 67)), STROKE, 6)


def pulse(draw):
    line(draw, ((12, 52), (28, 52), (36, 31), (47, 67), (57, 43), (66, 52), (84, 52)), ACCENT, 5)


def minimize(draw):
    line(draw, ((25, 50), (71, 50)), STROKE, 6)


def maximize(draw):
    draw.rounded_rectangle((24, 24, 72, 72), radius=4, outline=STROKE, width=6)


def close(draw):
    line(draw, ((27, 27), (69, 69)), STROKE, 6)
    line(draw, ((69, 27), (27, 69)), STROKE, 6)


ICONS = {
    "game": (game, '<path d="M18 58c3-16 10-25 19-25h22c9 0 16 9 19 25 2 10-11 15-18 5l-7-10H43l-7 10c-7 10-20 5-18-5Z" stroke="#DEE2E8" stroke-width="6" stroke-linejoin="round"/><path d="M35 43v16M27 51h16" stroke="#DEE2E8" stroke-width="6" stroke-linecap="round"/><circle cx="64" cy="43" r="5" fill="#E8851C"/><circle cx="75" cy="54" r="5" fill="#E8851C"/>' ),
    "electronics": (electronics, '<rect x="24" y="24" width="48" height="48" rx="8" stroke="#DEE2E8" stroke-width="5"/><rect x="34" y="34" width="28" height="28" rx="4" stroke="#E8851C" stroke-width="5"/><path d="M29 16v8M41 16v8M53 16v8M65 16v8M29 72v8M41 72v8M53 72v8M65 72v8M16 29h8M16 41h8M16 53h8M16 65h8M72 29h8M72 41h8M72 53h8M72 65h8" stroke="#E8851C" stroke-width="5" stroke-linecap="round"/>' ),
    "folder": (folder, '<path d="M16 30h24l8 9h32v31H16V30Z" stroke="#DEE2E8" stroke-width="6" stroke-linejoin="round"/><path d="M16 30l9-10h20l7 10" stroke="#DEE2E8" stroke-width="6" stroke-linejoin="round"/>' ),
    "agent": (agent, '<circle cx="43" cy="52" r="25" stroke="#979FAA" stroke-width="5"/><circle cx="43" cy="52" r="9" fill="#E8851C"/><path d="M70 31v36M77 38v22" stroke="#DEE2E8" stroke-width="5" stroke-linecap="round"/>' ),
    "settings": (settings, '<circle cx="48" cy="48" r="18" stroke="#DEE2E8" stroke-width="6"/><circle cx="48" cy="48" r="6" fill="#E8851C"/><path d="M48 16v8M48 72v8M16 48h8M72 48h8M25 25l6 6M65 65l6 6M71 25l-6 6M31 65l-6 6" stroke="#DEE2E8" stroke-width="6" stroke-linecap="round"/>' ),
    "plus": (plus, '<path d="M48 20v56M20 48h56" stroke="#E8851C" stroke-width="7" stroke-linecap="round"/>' ),
    "chevron-down": (chevron, '<path d="m31 39 17 17 17-17" stroke="#DEE2E8" stroke-width="7" stroke-linecap="round" stroke-linejoin="round"/>' ),
    "more": (more, '<circle cx="48" cy="28" r="5" fill="#DEE2E8"/><circle cx="48" cy="48" r="5" fill="#DEE2E8"/><circle cx="48" cy="68" r="5" fill="#DEE2E8"/>' ),
    "arrow-right": (arrow, '<path d="M18 48h54M53 29l19 19-19 19" stroke="#DEE2E8" stroke-width="6" stroke-linecap="round" stroke-linejoin="round"/>' ),
    "pulse": (pulse, '<path d="M12 52h16l8-21 11 36 10-24 9 9h18" stroke="#E8851C" stroke-width="5" stroke-linecap="round" stroke-linejoin="round"/>' ),
    "minimize": (minimize, '<path d="M25 50h46" stroke="#DEE2E8" stroke-width="6" stroke-linecap="round"/>' ),
    "maximize": (maximize, '<rect x="24" y="24" width="48" height="48" rx="4" stroke="#DEE2E8" stroke-width="6"/>' ),
    "close": (close, '<path d="m27 27 42 42M69 27 27 69" stroke="#DEE2E8" stroke-width="6" stroke-linecap="round"/>' ),
}


def main() -> None:
    ROOT.mkdir(parents=True, exist_ok=True)
    for name, (draw_icon, svg_body) in ICONS.items():
        png(name, draw_icon)
        svg(name, svg_body)


if __name__ == "__main__":
    main()
