"""Ripple metric: share of part pixels whose luminance deviates by more than 25
from the median of their 5×5 neighbourhood (dark triangles bleeding through
faces). Background (light gradient, luminance > 150) is excluded.

Usage: ripple_metric.py image.png [x0 y0 x1 y1]   (crop in pixels, optional)
"""
import sys
import numpy as np
from matplotlib import image

img = image.imread(sys.argv[1])[..., :3]
if img.dtype != np.uint8:
    img = (img * 255).astype(np.float32)
if len(sys.argv) == 6:
    x0, y0, x1, y1 = map(int, sys.argv[2:])
    img = img[y0:y1, x0:x1]
lum = 0.2126 * img[..., 0] + 0.7152 * img[..., 1] + 0.0722 * img[..., 2]
pad = np.pad(lum, 2, mode="edge")
windows = np.lib.stride_tricks.sliding_window_view(pad, (5, 5))
median = np.median(windows.reshape(*lum.shape, 25), axis=-1)
part = lum < 150
noise = (np.abs(lum - median) > 25) & part
share = noise.sum() / max(part.sum(), 1) * 100
print(f"{sys.argv[1]}: part pixels {part.sum()}, ripple {share:.2f} %")
