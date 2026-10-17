"""Independent check of the native clash GUI flow (no project code reused)."""
import json, sys
from pathlib import Path

here = Path(__file__).parent
doc = json.loads((here / "gui-project.obim").read_text())
project = doc["project"]
e = project["elements"]
assert e["9"]["shape"]["frame"]["origin"] == [200.0, 0.0, 0.0], "column 9 moved by -1300"
prof = project["profiles"][str(e["8"]["shape"]["profile"])]["shape"]
h, b, tw, tf = prof["height"], prof["width"], prof["web"], prof["flange"]
# I-section as three rectangles in local XY (x across width, y across height).
rects = [(-b/2, b/2, h/2 - tf, h/2), (-b/2, b/2, -h/2, -h/2 + tf), (-tw/2, tw/2, -h/2 + tf, h/2 - tf)]
dx = e["9"]["shape"]["frame"]["origin"][0] - e["8"]["shape"]["frame"]["origin"][0]
area = 0.0
for a in rects:
    for c in rects:
        w = min(a[1], c[1] + dx) - max(a[0], c[0] + dx)
        t = min(a[3], c[3]) - max(a[2], c[2])
        area += max(w, 0) * max(t, 0)
volume = area * min(e["8"]["shape"]["length"], e["9"]["shape"]["length"])
flow = json.loads((here / "gui-flow.json").read_text())
actions = [ev["action"] for ev in flow["events"]]
result = {"overlap_8_9_mm3": volume, "events": actions, "saved_revision": project["revision"],
          "header_sha256": doc["payload_sha256"]}
print(json.dumps(result, ensure_ascii=False, indent=1))
assert abs(volume - 3_600_000.0) < 1e-6
assert any(a.startswith("Отчёт коллизий: строка 1 (Пересечение #8 / #9)") for a in actions)
assert flow["events"][-1]["sha256"]
sys.exit(0)
