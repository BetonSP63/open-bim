#!/usr/bin/env python3
"""Encode platform icons from the approved RGBA master (requires Pillow).

Run: uv run --python 3.13 --with pillow==12.0.0 scripts/generate-icons.py
The generated files are committed; normal Rust builds do not require Python.
"""

from pathlib import Path

from PIL import Image


def main():
    icons = Path(__file__).resolve().parents[1] / "assets" / "icons"
    with Image.open(icons / "master.png") as source:
        master = source.convert("RGBA")
    if master.width != master.height or master.width < 1024:
        raise ValueError("master.png must be square and at least 1024 pixels")
    if master.getchannel("A").getextrema() != (0, 255):
        raise ValueError("master.png must contain transparent and opaque pixels")
    for size in (16, 24, 32, 48, 64, 128, 256, 512, 1024):
        master.resize((size, size), Image.Resampling.LANCZOS).save(
            icons / f"{size}x{size}.png", optimize=True
        )
    master.save(
        icons / "open-bim.ico",
        sizes=[(size, size) for size in (16, 24, 32, 48, 64, 128, 256)],
    )
    master.resize((1024, 1024), Image.Resampling.LANCZOS).save(
        icons / "open-bim.icns"
    )
    print(f"Generated PNG, ICO and ICNS icons in {icons}")


if __name__ == "__main__":
    main()
