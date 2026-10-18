"""Independent check of the GUI-created model against the grid values.

Reads the saved .obim JSON only (no Open BIM code): grid lines from the
file, then columns at grid intersections В4 В5 Г4 Г5 from 0 to the top level,
beams between column tops, the plate on intersections А1 А3 В3 В1, and the bolt
group at Б2 directed towards Б3 with holes in the plate.
"""
import json
import math
import sys

path = sys.argv[1]
p = json.load(open(path, encoding="utf-8"))["project"]
x = {a["name"]: a["start"][0] for a in p["axes"].values() if a["start"][0] == a["end"][0]}
y = {a["name"]: a["start"][1] for a in p["axes"].values() if a["start"][1] == a["end"][1]}
top = max(level["elevation_mm"] for level in p["levels"].values())
assert sorted(x) == ["1", "2", "3", "4", "5"] and sorted(y) == ["А", "Б", "В", "Г"], (x, y)


def node(name):
    return (x[name[1]], y[name[0]])


def world(frame, local):
    o, axes = frame["origin"], frame["axes"]
    return tuple(o[i] + sum(axes[j][i] * local[j] for j in range(3)) for i in range(3))


def close(a, b):
    return all(math.isclose(u, v, abs_tol=1e-6) for u, v in zip(a, b))


members = [e for e in p["elements"].values() if e["shape"]["kind"] == "member"]
columns = [e for e in members if e["shape"]["member_kind"] == "column"]
beams = [e for e in members if e["shape"]["member_kind"] == "beam"]
plates = [e for e in p["elements"].values() if e["shape"]["kind"] == "plate"]
assert (len(columns), len(beams), len(plates)) == (4, 4, 1), (len(columns), len(beams), len(plates))

ends = lambda e: (world(e["shape"]["frame"], (0, 0, 0)), world(e["shape"]["frame"], (0, 0, e["shape"]["length"])))
column_nodes = {node(n) for n in ["В4", "В5", "Г4", "Г5"]}
assert {ends(c)[0][:2] for c in columns} == column_nodes, "one column per node"
for c in columns:
    a, b = ends(c)
    assert (a[0], a[1]) in column_nodes and math.isclose(a[2], 0) and close(b, (a[0], a[1], top)), (a, b)
tops = {(cx, cy, top) for cx, cy in column_nodes}
pairs = set()
for beam in beams:
    a, b = ends(beam)
    match = lambda q: next(t for t in tops if close(q, t))
    pairs.add(frozenset([match(a), match(b)]))
assert len(pairs) == 4, pairs

plate = plates[0]
corners = [world(plate["shape"]["frame"], (c[0], c[1], 0)) for c in plate["shape"]["contour"]]
expected = [node(n) + (0.0,) for n in ["А1", "А3", "В3", "В1"]]
assert all(any(close(c, e) for c in corners) for e in expected), corners

bolts = [d["kind"] for d in p["details"].values() if d["kind"]["kind"] == "bolt_group"]
assert len(bolts) == 1
bolt = bolts[0]
assert close(bolt["frame"]["origin"], node("Б2") + (0.0,)), bolt["frame"]["origin"]
assert close(bolt["frame"]["axes"][0], (1, 0, 0)), "X towards Б3"
assert bolt["parts"] == [plate["id"]] and len(bolt["holes"]) == 4
holes = [d["kind"] for d in p["details"].values() if d["kind"]["kind"] == "hole"]
assert all(h["part"] == plate["id"] for h in holes) and len(holes) == 4
print(f"OK: grid 1–5 × А–Г, 4 columns 0…{top:.0f}, 4 beams on column tops, plate А1–В3, bolt group at Б2 (4 holes)")
