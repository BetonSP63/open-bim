"""Independent check of ГОСТ Р 21.101-2026 drawing conventions (5.3–5.5).

Reads the reference sheets next to this script with ezdxf and pypdf (no
code shared with Open BIM) and checks, with the limits re-entered here:

* КМ-1: every linear dimension has two 45° ticks 2–4 mm long, digits
  without units, the dimension line runs 0–3 mm past the outer ticks;
  level marks «+3,600» / «0,000» with the 45° sign (strokes 2–4 mm) on the
  elevation and a framed mark on the plan; axis bubbles 6–12 mm, labels
  1–5 and А–Г, axis lines on a dash-dot linetype; axis label font 1.5–2×
  the dimension digits; titles «Фасад 1-5», «План на отм. +3,600»,
  «Разрез 1-1» not underlined; section line strokes with the number;
* КМД-1: arrows on dimensions, no ticks.
"""
import math
import re
import sys
from pathlib import Path

import ezdxf
from pypdf import PdfReader

D = Path(__file__).resolve().parent
EPS = 0.05
failures = []


def check(ok, message):
    if not ok:
        failures.append(message)


def text(e):
    return re.sub(r"\\U\+([0-9A-Fa-f]{4})", lambda m: chr(int(m.group(1), 16)), e.dxf.text)


def segments(msp, layer):
    out = []
    for e in msp.query(f'LINE[layer=="{layer}"]'):
        out.append([(e.dxf.start.x, e.dxf.start.y), (e.dxf.end.x, e.dxf.end.y)])
    for e in msp.query(f'POLYLINE[layer=="{layer}"]'):
        pts = [(v.dxf.location.x, v.dxf.location.y) for v in e.vertices]
        out.append(pts)
    return out


def length(a, b):
    return math.hypot(b[0] - a[0], b[1] - a[1])


def angle(a, b):
    return math.degrees(math.atan2(b[1] - a[1], b[0] - a[0])) % 180


def load(name):
    doc = ezdxf.readfile(D / f"{name}.dxf")
    msp = doc.modelspace()
    texts = [(text(e), e.dxf.insert.x, e.dxf.insert.y, e.dxf.height) for e in msp.query("TEXT")]
    return doc, msp, texts


# ---- КМ-1 -------------------------------------------------------------
doc, msp, texts = load("km-1")
thin = segments(msp, "OB_THIN")
visible = segments(msp, "OB_VISIBLE")
ticks = [s for s in visible if len(s) == 2 and abs(angle(*s) - 45) < 0.5]
check(ticks, "КМ-1: нет засечек под 45°")

# Digits in the drawing field (the main inscription, 185×55 at the lower
# right of the A1 inner frame, and the binding strip are left out).
field = [t for t in texts if t[1] > 20 and not (t[1] > 841 - 5 - 185 and t[2] < 5 + 55)]
# The rolled-steel specification above the inscription (phase 12) is a
# table, not dimensions: its row numbers are left out too.
spec = [t for t in texts if t[0] == "Спецификация металлопроката"]
if spec:
    _, sx, sy, _ = spec[0]
    field = [t for t in field if not (abs(t[1] - sx) < 100 and t[2] < sy)]
dim_texts = [t for t in field if re.fullmatch(r"\d+", t[0]) and t[3] < 3.0]
check(len(dim_texts) >= 4, f"КМ-1: размерных чисел {len(dim_texts)} < 4")
dim_height = dim_texts[0][3] if dim_texts else 0
for value, x, y, h in dim_texts:
    # Dimension line: horizontal thin segment just under the digits.
    lines = [
        s for s in thin
        if len(s) == 2 and abs(s[0][1] - s[1][1]) < EPS
        and 0 <= y - s[0][1] <= 3 and min(s[0][0], s[1][0]) - EPS <= x <= max(s[0][0], s[1][0]) + EPS
    ]
    check(lines, f"КМ-1: у размера {value} нет размерной линии")
    if not lines:
        continue
    (x0, ly), (x1, _) = sorted(lines[0], key=lambda p: p[0])
    on = []
    for t in ticks:
        c = ((t[0][0] + t[1][0]) / 2, (t[0][1] + t[1][1]) / 2)
        if abs(c[1] - ly) < 0.1 and x0 - EPS <= c[0] <= x1 + EPS:
            on.append((c[0], length(*t)))
    check(len(on) == 2, f"КМ-1: размер {value}: засечек {len(on)} вместо 2")
    for _, l in on:
        check(2 - EPS <= l <= 4 + EPS, f"КМ-1: размер {value}: засечка {l:.2f} мм вне 2…4")
    if len(on) == 2:
        left, right = sorted(c for c, _ in on)
        for over in (left - x0, x1 - right):
            check(-EPS <= over <= 3 + EPS, f"КМ-1: размер {value}: выход линии {over:.2f} мм > 3")
# No units in any digits-only label next to a dimension line.
check(not any(re.search(r"\d\s*(мм|mm)", t[0]) for t in texts), "КМ-1: единицы в размерах")

# Level marks.
for mark in ("+3,600", "0,000"):
    found = [t for t in texts if t[0] == mark]
    check(found, f"КМ-1: нет отметки {mark}")
signs = [
    s for s in thin
    if len(s) == 3
    and abs(angle(s[0], s[1]) - 135) < 0.5 and abs(angle(s[1], s[2]) - 45) < 0.5
    and all(2 - EPS <= length(a, b) <= 4 + EPS for a, b in zip(s, s[1:]))
]
for value, x, y, h in texts:
    if value not in ("+3,600", "0,000"):
        continue
    near_sign = any(abs(s[1][0] - x) < 4 and 0 < y - s[1][1] < 10 for s in signs)
    framed = any(
        len(s) == 5 and s[0] == s[-1]
        and min(p[0] for p in s) <= x <= max(p[0] for p in s)
        and min(p[1] for p in s) <= y <= max(p[1] for p in s)
        for s in thin
    )
    check(near_sign or framed, f"КМ-1: отметка {value} ({x:.0f},{y:.0f}) без знака 45° или рамки")
check(len(signs) >= 4, f"КМ-1: знаков отметки {len(signs)} < 4 (фасад и разрез)")
check(
    any(len(s) == 5 and s[0] == s[-1] and any(t[0] == "+3,600" and min(p[0] for p in s) <= t[1] <= max(p[0] for p in s)
        and min(p[1] for p in s) <= t[2] <= max(p[1] for p in s) for t in texts) for s in thin),
    "КМ-1: на плане нет отметки в рамке",
)

# Axis bubbles.
bubbles = []
for s in thin:
    if len(s) < 40 or length(s[0], s[-1]) > EPS:
        continue
    xs, ys = [p[0] for p in s], [p[1] for p in s]
    c = ((max(xs) + min(xs)) / 2, (max(ys) + min(ys)) / 2)
    bubbles.append((c, max(xs) - min(xs)))
check(len(bubbles) >= 9, f"КМ-1: кружков осей {len(bubbles)} < 9")
names = set()
axis_heights = set()
for c, dia in bubbles:
    check(6 - EPS <= dia <= 12 + EPS, f"КМ-1: кружок оси ⌀{dia:.2f} вне 6…12")
    inside = [t for t in texts if length(c, (t[1], t[2])) < dia / 2]
    check(len(inside) == 1, f"КМ-1: в кружке ({c[0]:.0f},{c[1]:.0f}) {len(inside)} надписей")
    for t in inside:
        names.add(t[0])
        axis_heights.add(round(t[3], 3))
check(names == set("12345") | set("АБВГ"), f"КМ-1: марки осей {sorted(names)}")
for h in axis_heights:
    ratio = h / dim_height if dim_height else 0
    check(1.5 - EPS <= ratio <= 2 + EPS, f"КМ-1: шрифт осей / размеров = {ratio:.2f} вне 1,5…2")
layer = doc.layers.get("OB_DASHDOT")
check(layer.dxf.linetype.upper() == "DASHDOT", f"КМ-1: тип линии осей {layer.dxf.linetype}")
check(len(segments(msp, "OB_DASHDOT")) >= 9, "КМ-1: осевых линий < 9")

# View titles, not underlined; section strokes with the number.
for title in ("Фасад 1-5", "План на отм. +3,600", "Разрез 1-1"):
    found = [t for t in texts if t[0] == title]
    check(len(found) == 1, f"КМ-1: заголовок «{title}» найден {len(found)} раз")
    for _, x, y, h in found:
        under = [
            s for layer_name in ("OB_THIN", "OB_VISIBLE", "OB_FRAME") for s in segments(msp, layer_name)
            if len(s) == 2 and abs(s[0][1] - s[1][1]) < EPS and 0 < y - s[0][1] < 3
            and min(s[0][0], s[1][0]) <= x <= max(s[0][0], s[1][0])
        ]
        check(not under, f"КМ-1: заголовок «{title}» подчёркнут")
check(len(segments(msp, "OB_SECTION")) == 2, "КМ-1: штрихов линии разреза не 2")
check(sum(t[0] == "1" and t[3] > 3 for t in texts) >= 2 + 2, "КМ-1: номер разреза у штрихов")

pdf = "".join(p.extract_text() for p in PdfReader(D / "km-1.pdf").pages)
for title in ("Фасад 1-5", "План на отм. +3,600", "Разрез 1-1", "+3,600", "0,000"):
    check(title in pdf, f"КМ-1.pdf: нет «{title}»")

# ---- КМД-1 ------------------------------------------------------------
doc, msp, texts = load("kmd-1")
visible = segments(msp, "OB_VISIBLE")
check(
    not any(len(s) == 2 and abs(angle(*s) - 45) < 0.5 and length(*s) < 4.1 for s in visible),
    "КМД-1: засечки вместо стрелок",
)
arrows = [s for s in segments(msp, "OB_THIN") if len(s) == 3 and length(s[0], s[1]) < 4]
check(len(arrows) >= 4, f"КМД-1: стрелок {len(arrows)} < 4")

if failures:
    print("\n".join(failures))
    sys.exit(1)
print(
    f"КМ-1: {len(dim_texts)} размеров с засечками, {len(signs)} знаков отметки, "
    f"{len(bubbles)} кружков осей {sorted(names)}; КМД-1: {len(arrows)} стрелок"
)
print("CONVENTIONS_OK")
