"""Independent IFC check with IfcOpenShell 0.8.4 (schema validation + geometry kernel).

Run with work/verify-venv/bin/python. No project code is used.
"""
import json, sys
from pathlib import Path
import ifcopenshell, ifcopenshell.validate, ifcopenshell.geom, ifcopenshell.util.unit
import ifcopenshell.util.element

here = Path(__file__).parent
result = {}
for path in sorted(here.glob("*.ifc")):
    f = ifcopenshell.open(str(path))
    assert f.schema == "IFC4", f.schema
    logger = ifcopenshell.validate.json_logger()
    ifcopenshell.validate.validate(f, logger, express_rules=True)
    errors = [s for s in logger.statements if str(s.get("level")).lower() == "error"]
    print(path.name, "validator statements:", len(logger.statements), "errors:", len(errors), [s["message"][:200] for s in logger.statements][:8])
    assert not logger.statements, (path.name, logger.statements[:5])
    scale = ifcopenshell.util.unit.calculate_unit_scale(f)  # metres per file unit
    assert abs(scale - 0.001) < 1e-12, scale
    settings = ifcopenshell.geom.settings()
    settings.set("use-world-coords", True)
    elements = f.by_type("IfcBeam") + f.by_type("IfcColumn") + f.by_type("IfcPlate")
    shapes = {}
    for e in elements:
        shape = ifcopenshell.geom.create_shape(settings, e)
        verts = shape.geometry.verts
        faces = shape.geometry.faces
        # Signed tetrahedron volume of the triangulated (openings applied) solid, metres^3.
        vol = 0.0
        for i in range(0, len(faces), 3):
            a, b, c = (verts[3 * faces[i + k]:3 * faces[i + k] + 3] for k in range(3))
            vol += (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0])) / 6
        mats = ifcopenshell.util.element.get_materials(e)
        psets = ifcopenshell.util.element.get_psets(e)
        shapes[e.GlobalId] = {"type": e.is_a(), "name": e.Name, "volume_mm3": round(vol * 1e9, 1),
                               "material": mats[0].Name if mats else None,
                               "openings": len(e.HasOpenings), "psets": sorted(psets)}
    result[path.name] = {"elements": len(elements), "schema_errors": 0,
                         "length_unit_m": scale, "shapes": shapes}
(here / "verification-results.json").write_text(json.dumps(result, ensure_ascii=False, indent=1))
print(json.dumps({k: v["elements"] for k, v in result.items()}, ensure_ascii=False))

# Geometry cross-check against phase-9 independent reference volumes (connections model).
refs = json.loads((here / "../../connections_flows/verification-results.json").read_text())["reference_masses"]
source = json.loads((here / "../steel_release/source.obim").read_text())["project"]["elements"]
by_name = {}
for r in refs:
    by_name.setdefault(source[str(r["part"])]["name"], []).append(r["volume_mm3"])
checked = 0
for g, s in result["connections.ifc"]["shapes"].items():
    for ref in by_name.get(s["name"], []):
        rel = abs(s["volume_mm3"] - ref) / ref
        if rel < 2e-3:
            checked += 1
            break
    else:
        assert s["name"] not in by_name, (s["name"], s["volume_mm3"], by_name[s["name"]])
assert checked == 12, checked
print("IfcOpenShell volumes match independent references:", checked)
sys.exit(0)
