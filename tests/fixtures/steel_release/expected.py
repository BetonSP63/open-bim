"""Independent expected totals for the steel release fixture (no project code).

Masses: phase-9 independent references (tests/connections_flows/verification-results.json)
for drilled parts; parts without features use profile area x length x density.
"""
import json
from pathlib import Path
from collections import defaultdict

here = Path(__file__).parent
project = json.loads((here / "source.obim").read_text())["project"]
refs = {r["part"]: r["mass_kg"] for r in json.loads(
    (here / "../../connections_flows/verification-results.json").read_text())["reference_masses"]}

def area(shape):
    if shape["kind"] == "i_section":
        h, b, tw, tf = shape["height"], shape["width"], shape["web"], shape["flange"]
        return 2 * b * tf + (h - 2 * tf) * tw
    raise ValueError(shape["kind"])

groups = defaultdict(lambda: {"quantity": 0, "mass_kg": 0.0})
total = 0.0
for key, e in project["elements"].items():
    pid = int(key)
    shape = e["shape"]
    density = project["materials"][str(e["material"])]["density_kg_m3"]
    if shape["kind"] == "member":
        profile = project["profiles"][str(shape["profile"])]
        section, length = profile["name"], shape["length"]
    else:
        xs = [p[0] for p in shape["contour"]]; ys = [p[1] for p in shape["contour"]]
        a, b = max(xs) - min(xs), max(ys) - min(ys)
        t = shape["thickness"]
        section, length = f"—{t:g}×{min(a, b):g}", max(a, b)
    has_features = any(d["kind"].get("part") == pid for d in project.get("details", {}).values())
    if pid in refs:
        mass = refs[pid]
    else:
        assert not has_features, f"part {pid} needs an independent reference"
        mass = area(profile["shape"]) * length * density * 1e-9
    total += mass
    g = groups[f"{section}|{length:g}"]
    g["quantity"] += 1
    g["mass_kg"] += mass
out = {"parts": len(project["elements"]), "total_mass_kg": total,
       "by_section_length": {k: v for k, v in sorted(groups.items())},
       "bolts": sum((len(d["kind"]["steps_x"]) + 1) * (len(d["kind"]["steps_y"]) + 1)
                    for d in project["details"].values() if d["kind"]["kind"] == "bolt_group"),
       "shop_welds": sum(1 for d in project["details"].values()
                         if d["kind"]["kind"] == "weld" and d["kind"]["shop"])}
(here / "expected.json").write_text(json.dumps(out, ensure_ascii=False, indent=1))
print(json.dumps(out, ensure_ascii=False, indent=1))
