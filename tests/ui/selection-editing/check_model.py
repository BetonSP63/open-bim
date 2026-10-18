"""Independent check of the selection/editing GUI run (reads the .obim JSON only).

Expected after the run: А1–А2 has its end dragged to Б2, А2–А3 is untouched,
Б1–Б2 is moved to В1–В2, and its copy on Г was deleted.
"""
import json
import math
import sys

p = json.load(open(sys.argv[1], encoding="utf-8"))["project"]


def world(frame, local):
    o, axes = frame["origin"], frame["axes"]
    return tuple(o[i] + sum(axes[j][i] * local[j] for j in range(3)) for i in range(3))


beams = []
for e in p["elements"].values():
    s = e["shape"]
    if s["kind"] == "member":
        a = world(s["frame"], (0, 0, 0))
        b = world(s["frame"], (0, 0, s["length"]))
        beams.append(frozenset([tuple(round(v, 6) for v in a), tuple(round(v, 6) for v in b)]))
expected = {
    frozenset([(0.0, 0.0, 0.0), (6000.0, 6000.0, 0.0)]),
    frozenset([(6000.0, 0.0, 0.0), (12000.0, 0.0, 0.0)]),
    frozenset([(0.0, 12000.0, 0.0), (6000.0, 12000.0, 0.0)]),
}
assert set(beams) == expected and len(beams) == 3, beams
print("OK: handle dragged to Б2, А2–А3 unchanged, Б1–Б2 moved to В, copy on Г deleted")
