"""Pixel check of the display modes: inside the 3D view, how many pixels
differ between the shaded capture and the transparent / wireframe captures,
and whether pixels covered by the selected beam let the background through."""
import sys
from pathlib import Path

import numpy as np
from PIL import Image

D = Path(__file__).resolve().parent
# 3D view area in the 2560×1600 captures (logical 10..905 × 130..720, ×2).
BOX = (20, 262, 1810, 1440)


def load(name):
    return np.asarray(Image.open(D / name).convert("RGB").crop(BOX)).astype(int)


shaded = load("05-shaded.png")
results = {}
for name in ("04-transparent-50.png", "03-wireframe.png"):
    other = load(name)
    diff = np.abs(other - shaded).sum(axis=2) > 30
    results[name] = diff.mean() * 100
    print(f"{name}: {results[name]:.2f} % of view pixels differ from shaded")
# Beam pixels: strongly orange in the shaded capture (selected colour).
r, g, b = shaded[..., 0], shaded[..., 1], shaded[..., 2]
beam = (r > 180) & (g < 140) & (b < 80)
transparent = load("04-transparent-50.png")
mean_shaded = shaded[beam].mean(axis=0)
mean_clear = transparent[beam].mean(axis=0)
print(f"selected beam pixels: {beam.sum()}, mean RGB shaded {mean_shaded.round()}, transparent {mean_clear.round()}")
ok = (
    results["04-transparent-50.png"] > 0.5
    and results["03-wireframe.png"] > 1.0
    and beam.sum() > 1000
    and np.abs(mean_clear - mean_shaded).sum() > 40
)
print("TRANSPARENCY_OK" if ok else "TRANSPARENCY_FAIL")
sys.exit(0 if ok else 1)
