"""Independent check of the ГОСТ specifications and «Общие данные».

Reads the reference sheets next to this script with ezdxf and pypdf and the
project file as plain JSON (no Open BIM code), with the limits re-entered
here from ГОСТ Р 21.101-2026:

* form 7 on the assembly drawing (appendix И): graphs 15/60/65/10/15/20 mm
  (±0.1), header texts, unit masses equal to masses recomputed from the
  .obim geometry (section area × length or contour area × thickness ×
  density) within 0.001 kg;
* ГОСТ 21.502 specification on the КМ scheme: the total of each steel grade
  equals the sum of part masses of that grade in tonnes (3 decimals), the
  overall total likewise;
* «Общие данные» (4.3, appendix Б): form 1 graphs 15/140/30, form 2 graphs
  60/95/30, every sheet of the set listed with continuous numbers, graph 4
  «Общие данные», sheet 1 of the set.
"""
import json
import math
import re
import sys
from collections import defaultdict
from pathlib import Path

import ezdxf
from pypdf import PdfReader

D = Path(__file__).resolve().parent
TOL = 0.1
failures = []


def check(ok, message):
    if not ok:
        failures.append(message)


def decode(s):
    return re.sub(r"\\U\+([0-9A-Fa-f]{4})", lambda m: chr(int(m.group(1), 16)), s)


def load(name):
    msp = ezdxf.readfile(D / f"{name}.dxf").modelspace()
    texts = []
    for e in msp.query("TEXT"):
        p = e.dxf.align_point if e.dxf.get("halign", 0) else e.dxf.insert
        texts.append((decode(e.dxf.text), p.x, p.y))
    segs, thin = [], []
    for layer, out in (("OB_FRAME", segs), ("OB_THIN", thin)):
        for e in msp.query(f'LINE[layer=="{layer}"]'):
            out.append(((e.dxf.start.x, e.dxf.start.y), (e.dxf.end.x, e.dxf.end.y)))
        for e in msp.query(f'POLYLINE[layer=="{layer}"]'):
            pts = [(v.dxf.location.x, v.dxf.location.y) for v in e.vertices]
            out.extend(zip(pts, pts[1:]))
    return texts, segs, thin


def table_columns(texts, segs, title):
    """Graph widths of the table under the title text."""
    found = [t for t in texts if t[0] == title]
    check(found, f"нет таблицы «{title}»")
    if not found:
        return [], 0, 0
    _, tx, ty = found[0]
    # Top line of the table: the horizontal frame line just below the title.
    tops = [
        s for s in segs
        if abs(s[0][1] - s[1][1]) < 1e-6 and 0 < ty - s[0][1] < 8
        and min(s[0][0], s[1][0]) - 1 <= tx <= max(s[0][0], s[1][0]) + 1
    ]
    check(tops, f"«{title}»: нет верхней линии")
    if not tops:
        return [], 0, 0
    top = max(tops, key=lambda s: s[0][1])
    y = top[0][1]
    x0, x1 = sorted((top[0][0], top[1][0]))
    xs = sorted({
        round(s[0][0], 3) for s in segs
        if abs(s[0][0] - s[1][0]) < 1e-6 and x0 - 1e-6 <= s[0][0] <= x1 + 1e-6
        and abs(max(s[0][1], s[1][1]) - y) < 1e-6
    })
    return [b - a for a, b in zip(xs, xs[1:])], (x0, x1), y


def widths_ok(name, got, expected):
    check(
        len(got) == len(expected) and all(abs(a - b) <= TOL for a, b in zip(got, expected)),
        f"{name}: графы {[round(g, 2) for g in got]} вместо {expected}",
    )


def rows_in(texts, lines, x_range, top):
    """Texts of each table row below the header line `top`: rows are the
    bands between horizontal lines spanning the table; cells ordered by x,
    wrapped lines of a cell joined."""
    ys = sorted({
        round(a[1], 3) for a, b in lines
        if abs(a[1] - b[1]) < 1e-6 and a[1] <= top + 1e-6
        and abs(min(a[0], b[0]) - x_range[0]) < 1e-3 and abs(max(a[0], b[0]) - x_range[1]) < 1e-3
    }, reverse=True)
    rows = []
    for hi, lo in zip(ys, ys[1:]):
        cells = defaultdict(list)
        for t, x, y in texts:
            if x_range[0] <= x <= x_range[1] and lo < y < hi:
                cells[round(x, 1)].append((-y, t))
        rows.append([(x, " ".join(t for _, t in sorted(v))) for x, v in sorted(cells.items())])
    return rows


def number(s):
    return float(s.replace(",", "."))


# ---- Masses recomputed from the project file --------------------------
project = json.loads((D / "control-set.obim").read_text(encoding="utf-8"))["project"]
materials = project["materials"]
profiles = project["profiles"]


def area(shape):
    k = shape["kind"]
    if k == "i_section":
        return 2 * shape["width"] * shape["flange"] + (shape["height"] - 2 * shape["flange"]) * shape["web"]
    if k == "channel":
        return 2 * shape["width"] * shape["flange"] + (shape["height"] - 2 * shape["flange"]) * shape["web"]
    raise ValueError(k)


def polygon(points):
    return abs(sum(a[0] * b[1] - b[0] * a[1] for a, b in zip(points, points[1:] + points[:1]))) / 2


holes = {d["kind"]["part"] for d in project["details"].values() if d["kind"]["kind"] == "hole"}
masses = {}
for key, e in project["elements"].items():
    check(int(key) not in holes, f"деталь {key} с отверстиями — проверка без вычитания")
    s = e["shape"]
    density = materials[str(e["material"])]["density_kg_m3"]
    if s["kind"] == "member":
        volume = area(profiles[str(s["profile"])]["shape"]) * s["length"]
        name = f'{profiles[str(s["profile"])]["name"]}, L={s["length"]:g}'
    else:
        volume = polygon(s["contour"]) * s["thickness"]
        xs = [p[0] for p in s["contour"]]
        ys = [p[1] for p in s["contour"]]
        a, b = max(xs) - min(xs), max(ys) - min(ys)
        name = f'—{s["thickness"]:g}×{min(a, b):g}, L={max(a, b):g}'
    grade = materials[str(e["material"])]["name"].split(" · ")[0]
    masses[key] = (name, grade, volume / 1e9 * density)

# ---- Form 7 on the assembly drawing ------------------------------------
texts, segs, thin = load("kmd-assembly")
widths, xr, top = table_columns(texts, segs, "Спецификация")
widths_ok("Форма 7", widths, [15, 60, 65, 10, 15, 20])
header = {t for t, x, y in texts if xr and xr[0] <= x <= xr[1] and top - 15 < y < top}
for h in ("Поз.", "Обозначение", "Наименование", "Кол.", "Масса", "ед., кг", "Приме-", "чание"):
    check(h in header, f"Форма 7: нет заголовка «{h}»")
checked = 0
for row in rows_in(texts, segs + thin, xr, top - 15):
    cells = [t for _, t in row]
    if len(cells) < 4 or not re.fullmatch(r"\d+,\d{3}", cells[-2]):
        continue
    name, shown = cells[1], number(cells[-2])
    model = [m for m in masses.values() if m[0] == name]
    check(model, f"Форма 7: «{name}» нет в модели")
    if model:
        check(abs(shown - model[0][2]) <= 0.001 + 5e-4, f"Форма 7: {name}: {shown} ≠ {model[0][2]:.4f}")
        checked += 1
check(checked == 2, f"Форма 7: проверено строк {checked} вместо 2")

# ---- ГОСТ 21.502 on the КМ scheme --------------------------------------
texts, segs, thin = load("km-scheme")
title = [t for t in texts if t[0] == "Спецификация металлопроката"]
check(title, "КМ: нет спецификации металлопроката")
by_grade = defaultdict(float)
for _, grade, kg in masses.values():
    by_grade[grade] += kg / 1000
if title:
    _, xr, top = table_columns(texts, segs, "Спецификация металлопроката")
    header_line = max(
        a[1] for a, b in segs
        if abs(a[1] - b[1]) < 1e-6 and a[1] < top - 1
        and abs(min(a[0], b[0]) - xr[0]) < 1e-3 and abs(max(a[0], b[0]) - xr[1]) < 1e-3
    )
    rows = rows_in(texts, segs + thin, xr, header_line)
    start = next(i for i, r in enumerate(rows) if any(t.startswith("В том числе") for _, t in r))
    total_row = next(r for r in rows if any(t == "Всего масса металла" for _, t in r))
    shown_total = number(total_row[-1][1])
    check(abs(shown_total - round(sum(by_grade.values()), 3)) < 1e-9,
          f"КМ: всего {shown_total} ≠ {sum(by_grade.values()):.3f}")
    seen = {}
    for r in rows[start + 1:]:
        cells = [t for _, t in r]
        if cells and cells[0] in by_grade:
            seen[cells[0]] = number(cells[-1])
    check(set(seen) == set(by_grade), f"КМ: марки {sorted(seen)} вместо {sorted(by_grade)}")
    for g, t in seen.items():
        check(abs(t - round(by_grade[g], 3)) < 1e-9, f"КМ: {g}: {t} ≠ {by_grade[g]:.3f}")

# ---- «Общие данные» ----------------------------------------------------
for name, mark, sheets in (("km-od", "КМ", 2), ("kmd-od", "КМД", 3)):
    texts, segs, thin = load(name)
    widths, xr, top = table_columns(texts, segs, "Ведомость рабочих чертежей основного комплекта")
    widths_ok(f"{mark}: форма 1", widths, [15, 140, 30])
    widths, _, _ = table_columns(texts, segs, "Ведомость ссылочных и прилагаемых документов")
    widths_ok(f"{mark}: форма 2", widths, [60, 95, 30])
    listed = [
        int(r[0][1]) for r in rows_in(texts, segs + thin, xr, top - 15)
        if r and r[0][1].isdigit()
    ][:sheets]
    check(listed == list(range(1, sheets + 1)), f"{mark}: листы {listed}")
    pdf = "".join(p.extract_text() for p in PdfReader(D / f"{name}.pdf").pages)
    check("Общие данные" in pdf and "Общие указания" in pdf, f"{mark}: нет «Общие данные»/«Общие указания»")
    check(any(t == "Общие данные" for t, x, y in texts if y < 60), f"{mark}: графа 4 не «Общие данные»")

if failures:
    print("\n".join(failures))
    sys.exit(1)
print(
    "Форма 7: графы 15/60/65/10/15/20, массы = модель; ГОСТ 21.502: "
    + ", ".join(f"{g} {t:.3f} т" for g, t in sorted(by_grade.items()))
    + "; общие данные КМ и КМД — формы 1/2, листы подряд"
)
print("SPECIFICATIONS_OK")
