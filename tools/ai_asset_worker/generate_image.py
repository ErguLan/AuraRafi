#!/usr/bin/env python3
"""On-demand image asset worker for AuraRafi.

The editor writes a request file and starts this script only for an explicit
asset-generation action. No server, watcher, or persistent Python process is
created. Credentials remain in environment variables, never in project files.
"""

import argparse
import base64
import hashlib
import json
import os
import struct
import sys
import tempfile
import urllib.error
import urllib.request
import zlib
from datetime import datetime, timezone
from pathlib import Path
from urllib.parse import urlparse

MAX_IMAGE_BYTES = 32 * 1024 * 1024


def write_result(path, ok, error=None):
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = {"ok": ok}
    if error:
        payload["error"] = error
    path.write_text(json.dumps(payload, indent=2), encoding="utf-8")


def fetch_image_bytes(response_data: dict) -> bytes:
    entries = response_data.get("data") or []
    if not entries:
        message = (response_data.get("choices") or [{}])[0].get("message") or {}
        entries = message.get("images") or []
        if not entries and isinstance(message.get("content"), list):
            entries = [part for part in message["content"] if part.get("type") == "image_url"]
    if not entries:
        raise RuntimeError("provider response has no image data")
    entry = entries[0]
    encoded = entry.get("b64_json")
    if encoded:
        image = base64.b64decode(encoded, validate=True)
        if len(image) > MAX_IMAGE_BYTES:
            raise RuntimeError("generated image exceeds 32 MiB")
        return image
    url = entry.get("url") or (entry.get("image_url") or {}).get("url")
    if not url:
        raise RuntimeError("provider response has no image payload")
    if url.startswith("data:image/"):
        return fetch_image_bytes({"data": [{"b64_json": url.split(",", 1)[1]}]})
    if urlparse(url).scheme != "https":
        raise RuntimeError("image download URL must use HTTPS")
    with urllib.request.urlopen(url, timeout=180) as response:
        data = response.read(MAX_IMAGE_BYTES + 1)
        if len(data) > MAX_IMAGE_BYTES:
            raise RuntimeError("generated image exceeds 32 MiB")
        return data


def atomic_write(path: Path, payload: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(delete=False, dir=path.parent, suffix=".tmp") as handle:
        handle.write(payload)
        temporary = Path(handle.name)
    temporary.replace(path)


def parse_size(value):
    try:
        width_text, height_text = value.lower().split("x", 1)
        width, height = int(width_text), int(height_text)
    except (AttributeError, ValueError) as error:
        raise RuntimeError("local PNG size must be <width>x<height>") from error
    if width < 16 or height < 16 or width > 2048 or height > 2048:
        raise RuntimeError("local PNG size is outside the supported range")
    return width, height


def fill_rect(pixels, width, height, left, top, right, bottom, color) -> None:
    left = max(0, min(width, int(left)))
    right = max(0, min(width, int(right)))
    top = max(0, min(height, int(top)))
    bottom = max(0, min(height, int(bottom)))
    if left >= right or top >= bottom:
        return
    row = bytes(color) * (right - left)
    for y in range(top, bottom):
        start = (y * width + left) * 4
        pixels[start : start + len(row)] = row


def fill_circle(pixels, width, height, center_x, center_y, radius, color) -> None:
    radius = max(1, int(radius))
    center_x, center_y = int(center_x), int(center_y)
    radius_squared = radius * radius
    for y in range(max(0, center_y - radius), min(height, center_y + radius + 1)):
        delta_y = y - center_y
        delta_x = int(max(0, radius_squared - delta_y * delta_y) ** 0.5)
        fill_rect(
            pixels,
            width,
            height,
            center_x - delta_x,
            y,
            center_x + delta_x + 1,
            y + 1,
            color,
        )


def fill_rounded_rect(pixels, width, height, left, top, right, bottom, radius, color) -> None:
    left, top, right, bottom = int(left), int(top), int(right), int(bottom)
    radius = max(0, min(int(radius), (right - left) // 2, (bottom - top) // 2))
    if radius == 0:
        fill_rect(pixels, width, height, left, top, right, bottom, color)
        return
    for y in range(top, bottom):
        edge_distance = min(y - top, bottom - 1 - y)
        inset = 0
        if edge_distance < radius:
            inset = radius - int(max(0, radius * radius - (radius - edge_distance) ** 2) ** 0.5)
        fill_rect(pixels, width, height, left + inset, y, right - inset, y + 1, color)


def draw_line(pixels, width, height, start_x, start_y, end_x, end_y, thickness, color) -> None:
    delta_x = end_x - start_x
    delta_y = end_y - start_y
    steps = max(1, int(max(abs(delta_x), abs(delta_y))))
    radius = max(1, int(thickness) // 2)
    for step in range(steps + 1):
        progress = step / steps
        fill_circle(
            pixels,
            width,
            height,
            start_x + delta_x * progress,
            start_y + delta_y * progress,
            radius,
            color,
        )


def png_chunk(kind: bytes, data: bytes) -> bytes:
    return (
        struct.pack(">I", len(data))
        + kind
        + data
        + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
    )


def encode_png(width: int, height: int, pixels: bytearray) -> bytes:
    rows = bytearray()
    stride = width * 4
    for y in range(height):
        rows.append(0)
        rows.extend(pixels[y * stride : (y + 1) * stride])
    header = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + png_chunk(b"IHDR", header)
        + png_chunk(b"IDAT", zlib.compress(bytes(rows), level=9))
        + png_chunk(b"IEND", b"")
    )


def local_palette(prompt):
    digest = hashlib.sha256(prompt.encode("utf-8")).digest()
    orange = (224 + digest[0] % 24, 102 + digest[1] % 58, 18 + digest[2] % 38, 255)
    highlight = (255, min(224, orange[1] + 58), min(150, orange[2] + 56), 255)
    return orange, highlight


def draw_local_icon(pixels, width, height, prompt: str, style: str, transparent: bool) -> None:
    orange, highlight = local_palette(prompt)
    dark = (16, 18, 21, 245)
    muted = (47, 53, 61, 255)
    if not transparent:
        fill_rect(pixels, width, height, 0, 0, width, height, (11, 13, 16, 255))

    side = min(width, height)
    left = (width - side) // 2
    top = (height - side) // 2
    inset = max(8, side // 10)
    panel_left = left + inset
    panel_top = top + inset
    panel_right = left + side - inset
    panel_bottom = top + side - inset
    panel_radius = max(8, side // 9)
    fill_rounded_rect(
        pixels,
        width,
        height,
        panel_left,
        panel_top,
        panel_right,
        panel_bottom,
        panel_radius,
        dark,
    )

    center_x = (panel_left + panel_right) // 2
    center_y = (panel_top + panel_bottom) // 2
    unit = max(4, side // 16)
    terms = prompt.lower()

    if style == "texture":
        tile = max(4, side // 14)
        seed = hashlib.sha256(prompt.encode("utf-8")).digest()
        for row, y in enumerate(range(panel_top + unit, panel_bottom - unit, tile)):
            for column, x in enumerate(range(panel_left + unit, panel_right - unit, tile)):
                tone = seed[(row * 13 + column * 7) % len(seed)]
                color = orange if tone % 3 else highlight
                if tone % 5 == 0:
                    color = muted
                fill_rounded_rect(
                    pixels,
                    width,
                    height,
                    x,
                    y,
                    x + tile - 1,
                    y + tile - 1,
                    max(1, tile // 5),
                    color,
                )
        return

    if style == "badge":
        fill_rounded_rect(
            pixels,
            width,
            height,
            center_x - unit * 5,
            center_y - unit * 3,
            center_x + unit * 5,
            center_y + unit * 3,
            unit,
            orange,
        )
        fill_circle(pixels, width, height, center_x - unit * 2, center_y, unit, dark)
        fill_circle(pixels, width, height, center_x + unit * 2, center_y, unit, highlight)
        return

    if style == "sprite":
        for row in range(7):
            for column in range(7):
                if (row * 5 + column * 3 + len(prompt)) % 7 not in (0, 1):
                    color = orange if (row + column) % 3 else highlight
                    fill_rect(
                        pixels,
                        width,
                        height,
                        center_x + (column - 3) * unit,
                        center_y + (row - 3) * unit,
                        center_x + (column - 2) * unit,
                        center_y + (row - 2) * unit,
                        color,
                    )
        return

    if any(term in terms for term in ("camera", "photo", "image", "preview")):
        fill_rounded_rect(
            pixels,
            width,
            height,
            center_x - unit * 5,
            center_y - unit * 3,
            center_x + unit * 5,
            center_y + unit * 3,
            unit,
            orange,
        )
        fill_rect(
            pixels,
            width,
            height,
            center_x - unit * 2,
            center_y - unit * 4,
            center_x + unit * 2,
            center_y - unit * 2,
            highlight,
        )
        fill_circle(pixels, width, height, center_x, center_y, unit * 2, dark)
        fill_circle(pixels, width, height, center_x, center_y, unit, highlight)
    elif any(term in terms for term in ("folder", "asset", "project", "file")):
        fill_rounded_rect(
            pixels,
            width,
            height,
            center_x - unit * 5,
            center_y - unit,
            center_x + unit * 5,
            center_y + unit * 4,
            unit,
            orange,
        )
        fill_rounded_rect(
            pixels,
            width,
            height,
            center_x - unit * 4,
            center_y - unit * 3,
            center_x,
            center_y,
            unit,
            highlight,
        )
    elif any(term in terms for term in ("circuit", "electronics", "chip", "node")):
        draw_line(pixels, width, height, center_x - unit * 4, center_y, center_x + unit * 4, center_y, unit, orange)
        draw_line(pixels, width, height, center_x, center_y - unit * 4, center_x, center_y + unit * 4, unit, orange)
        for x, y in ((-4, 0), (4, 0), (0, -4), (0, 4), (0, 0)):
            fill_circle(pixels, width, height, center_x + x * unit, center_y + y * unit, unit, highlight)
    elif any(term in terms for term in ("eye", "visible", "view", "select")):
        fill_circle(pixels, width, height, center_x, center_y, unit * 5, orange)
        fill_circle(pixels, width, height, center_x, center_y, unit * 3, dark)
        fill_circle(pixels, width, height, center_x, center_y, unit, highlight)
    else:
        fill_rounded_rect(
            pixels,
            width,
            height,
            center_x - unit * 4,
            center_y - unit * 4,
            center_x + unit * 4,
            center_y + unit * 4,
            unit,
            orange,
        )
        draw_line(
            pixels,
            width,
            height,
            center_x - unit * 3,
            center_y - unit,
            center_x + unit * 3,
            center_y - unit,
            unit,
            highlight,
        )
        draw_line(
            pixels,
            width,
            height,
            center_x - unit * 3,
            center_y + unit * 2,
            center_x + unit * 3,
            center_y + unit * 2,
            unit,
            highlight,
        )


def generate_local_png(request: dict) -> None:
    width, height = parse_size(request["size"])
    transparent = bool(request.get("transparent"))
    pixels = bytearray(bytes((0, 0, 0, 0)) * (width * height))
    draw_local_icon(
        pixels,
        width,
        height,
        request["prompt"],
        request.get("local_style", "icon"),
        transparent,
    )
    output_path = Path(request["output_path"])
    metadata_path = Path(request["metadata_path"])
    atomic_write(output_path, encode_png(width, height, pixels))
    metadata = {
        "kind": "generated_image",
        "provider": "local_procedural_png",
        "model": request["model"],
        "generation_mode": "local_png",
        "local_style": request.get("local_style", "icon"),
        "prompt": request["prompt"],
        "size": request["size"],
        "transparent": transparent,
        "created_at": datetime.now(timezone.utc).isoformat(),
        "image_file": output_path.name,
    }
    metadata_path.parent.mkdir(parents=True, exist_ok=True)
    metadata_path.write_text(json.dumps(metadata, indent=2), encoding="utf-8")


def provider_json(url: str, api_key: str, payload=None):
    body = None if payload is None else json.dumps(payload).encode("utf-8")
    request = urllib.request.Request(url, data=body, headers={
        "Authorization": f"Bearer {api_key}", "Content-Type": "application/json",
    }, method="GET" if payload is None else "POST")
    try:
        with urllib.request.urlopen(request, timeout=240) as response:
            return json.loads(response.read(64 * 1024 * 1024).decode("utf-8"))
    except urllib.error.HTTPError as error:
        detail = error.read(2048).decode("utf-8", errors="replace").replace(api_key, "[redacted]")
        raise RuntimeError(f"image provider HTTP {error.code}: {detail[:400]}") from error


def openrouter_image_contract(base_url: str, api_key: str, model: str):
    # Discovery is authoritative: never infer modalities from a model's name.
    catalog = provider_json(f"{base_url}/models", api_key)
    general = next((entry for entry in catalog.get("data", []) if entry.get("id") == model), None)
    images = provider_json(f"{base_url}/images/models", api_key)
    dedicated = next((entry for entry in images.get("data", []) if entry.get("id") == model), None)
    selected = dedicated or general
    if not selected or "image" not in selected.get("architecture", {}).get("output_modalities", []):
        raise RuntimeError("Selected model does not advertise image output. Use local PNG or the external studio; an image-only model may be explicitly supplied to this tool.")
    return ("images" if dedicated else "chat/completions"), selected


def generate_remote_image(request: dict) -> None:
    api_key = os.environ.get("RAF_IMAGE_API_KEY", os.environ.get("OPENAI_API_KEY", "")).strip()
    if not api_key:
        raise RuntimeError("An enabled provider with an API key is required for image generation")

    base_url = os.environ.get("RAF_IMAGE_BASE_URL", os.environ.get("RAF_AI_IMAGE_BASE_URL", "https://api.openai.com/v1")).rstrip("/")
    parsed = urlparse(base_url)
    if parsed.scheme != "https" and not (parsed.scheme == "http" and parsed.hostname in ("localhost", "127.0.0.1", "::1")):
        raise RuntimeError("Provider endpoint must use HTTPS or local HTTP")
    endpoint = os.environ.get("RAF_AI_IMAGE_ENDPOINT", f"{base_url}/images/generations")
    provider = os.environ.get("RAF_IMAGE_PROVIDER", "openai")
    capabilities = {"status": "unknown", "reason": "Provider does not publish output modalities via its models API"}
    payload = {
        "model": request["model"],
        "prompt": request["prompt"],
        "size": request["size"],
        "output_format": "png",
    }
    if request.get("transparent"):
        payload["background"] = "transparent"
    if provider == "openrouter":
        protocol, model_info = openrouter_image_contract(base_url, api_key, request["model"])
        capabilities = model_info.get("architecture", {})
        endpoint = f"{base_url}/{protocol}"
        if protocol == "chat/completions":
            payload = {"model": request["model"], "messages": [{"role": "user", "content": request["prompt"]}],
                       "modalities": [modality for modality in ("text", "image") if modality in capabilities.get("output_modalities", [])]}
        else:
            supported = model_info.get("supported_parameters", {})
            payload = {"model": request["model"], "prompt": request["prompt"]}
            for key, value in (("size", request["size"]), ("output_format", "png"), ("background", "transparent" if request.get("transparent") else "opaque")):
                if key in supported:
                    descriptor = supported[key]
                    if descriptor.get("type") != "enum" or value in descriptor.get("values", []):
                        payload[key] = value

    response_data = provider_json(endpoint, api_key, payload)

    image_bytes = fetch_image_bytes(response_data)
    if not image_bytes.startswith(b"\x89PNG\r\n\x1a\n"):
        raise RuntimeError("image provider did not return PNG data")

    output_path = Path(request["output_path"])
    metadata_path = Path(request["metadata_path"])
    atomic_write(output_path, image_bytes)
    metadata = {
        "kind": "generated_image",
        "provider": provider,
        "capabilities": capabilities,
        "model": request["model"],
        "generation_mode": "remote",
        "prompt": request["prompt"],
        "size": request["size"],
        "transparent": bool(request.get("transparent")),
        "created_at": datetime.now(timezone.utc).isoformat(),
        "image_file": output_path.name,
    }
    metadata_path.parent.mkdir(parents=True, exist_ok=True)
    metadata_path.write_text(json.dumps(metadata, indent=2), encoding="utf-8")


def generate(request: dict) -> None:
    if request.get("generation_mode") == "local_png":
        generate_local_png(request)
    else:
        generate_remote_image(request)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--request", required=True)
    parser.add_argument("--result", required=True)
    args = parser.parse_args()
    result_path = Path(args.result)
    try:
        request = json.loads(Path(args.request).read_text(encoding="utf-8"))
        generate(request)
        write_result(result_path, True)
        return 0
    except Exception as error:
        write_result(result_path, False, str(error))
        return 1


if __name__ == "__main__":
    sys.exit(main())
