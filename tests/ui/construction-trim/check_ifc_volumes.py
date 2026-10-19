"""Independent check of the cut beams in the exported IFC with IfcOpenShell:
geometry is rebuilt by its OpenCASCADE kernel (openings subtracted) and the
volumes are compared with the analytic values for the I 300×150×8×12 section."""
import json
import sys
from pathlib import Path

import ifcopenshell
import ifcopenshell.geom
import ifcopenshell.util.shape

D = Path(__file__).resolve().parent
AREA = 2 * 150 * 12 + (300 - 2 * 12) * 8  # mm², the analytic section
settings = ifcopenshell.geom.settings()
settings.set("use-world-coords", True)
ifc = ifcopenshell.open(str(D / "construction-trim.ifc"))
project = json.loads((D / "construction-trim.obim").read_text())["project"]
beams = {}
for beam in ifc.by_type("IfcBeam"):
    shape = ifcopenshell.geom.create_shape(settings, beam)
    # IfcOpenShell returns SI metres; the model works in millimetres.
    volume = ifcopenshell.util.shape.get_volume(shape.geometry) * 1e9
    ys = shape.geometry.verts[1::3]
    beams[round(sum(ys) / len(ys) * 1000)] = volume  # keyed by mean y (6000 / 12000)
print("IFC beams (mean y → volume mm³):", {k: round(v) for k, v in beams.items()})
openings = len(ifc.by_type("IfcOpeningElement"))
v1, v2 = beams[6000], beams[12000]
expected1 = AREA * 10000
checks = [
    (f"beam 1: fitted to 13000, cut at x = 10000 → {v1:.0f} vs {expected1} (±0.5 %)", abs(v1 - expected1) / expected1 < 0.005),
    (f"beam 2: cut by the column, {AREA * 18000 - v2:.0f} mm³ removed", 0 < AREA * 18000 - v2 < AREA * 18000 * 0.05),
    (f"openings in IFC: {openings}", openings == 2),
    ("construction line kept in the model, not in IFC", len(project["construction"]) == 1 and not ifc.by_type("IfcAnnotation")),
    ("model dimension tied to both beam ends", all(p["at"] == "part_end" for p in next(iter(project["dimensions"].values())).values() if isinstance(p, dict))),
]
for label, ok in checks:
    print(("ok   " if ok else "FAIL ") + label)
ok = all(c[1] for c in checks)
print("IFC_VOLUMES_OK" if ok else "IFC_VOLUMES_FAIL")
sys.exit(0 if ok else 1)
