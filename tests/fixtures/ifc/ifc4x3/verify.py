"""Independent check of the IFC4.3 export with IfcOpenShell 0.9 (schema + EXPRESS
rules, units, geometry kernel). No project code is used.

* every *.ifc here validates with zero statements against the schema it names;
* the three models have the same element volumes as their IFC4 files one
  folder up (verification-results.json there, checked by ../verify.py);
* types.ifc holds IFC4.3 types natively, types-as-ifc4.ifc as USERDEFINED.
"""
import json, sys
from pathlib import Path
import ifcopenshell, ifcopenshell.validate, ifcopenshell.geom, ifcopenshell.util.unit

here = Path(__file__).parent
ifc4 = json.loads((here.parent / "verification-results.json").read_text())


def volume(settings, element):
    shape = ifcopenshell.geom.create_shape(settings, element)
    v, f = shape.geometry.verts, shape.geometry.faces
    vol = 0.0
    for i in range(0, len(f), 3):
        a, b, c = (v[3 * f[i + k]:3 * f[i + k] + 3] for k in range(3))
        vol += (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                + a[2] * (b[0] * c[1] - b[1] * c[0])) / 6
    return vol * 1e9


settings = ifcopenshell.geom.settings()
settings.set("use-world-coords", True)
result = {}
for path in sorted(here.glob("*.ifc")):
    f = ifcopenshell.open(str(path))
    expected = "IFC4" if path.stem.endswith("as-ifc4") else "IFC4X3_ADD2"
    assert f.schema_identifier == expected, (path.name, f.schema_identifier)
    logger = ifcopenshell.validate.json_logger()
    # By path: the parser reports bad enumerations and attribute counts
    # that an opened file no longer shows (negative control: 3 statements).
    ifcopenshell.validate.validate(str(path), logger, express_rules=True)
    print(path.name, f.schema_identifier, "validator statements:", len(logger.statements),
          [s["message"][:200] for s in logger.statements][:8])
    assert not logger.statements, (path.name, logger.statements[:5])
    assert abs(ifcopenshell.util.unit.calculate_unit_scale(f) - 0.001) < 1e-12
    elements = f.by_type("IfcBuiltElement" if expected != "IFC4" else "IfcBuildingElement")
    shapes = {e.GlobalId: {"type": e.is_a(), "predefined": e.PredefinedType,
                           "object_type": e.ObjectType,
                           "volume_mm3": round(volume(settings, e), 1)} for e in elements}
    result[path.name] = {"schema": f.schema_identifier, "elements": len(elements),
                         "schema_errors": 0, "shapes": shapes}
    reference = ifc4.get(path.name)
    if reference:
        assert len(shapes) == reference["elements"], path.name
        for guid, s in shapes.items():
            ref = reference["shapes"][guid]["volume_mm3"]
            assert abs(s["volume_mm3"] - ref) <= 1e-4 * max(ref, 1), (path.name, guid, s, ref)
        print(path.name, "volumes equal to the IFC4 file:", len(shapes))

types = result["types.ifc"]["shapes"]
assert sorted((s["type"], s["predefined"]) for s in types.values()
              if s["predefined"] in ("BASE_PLATE", "POT")) == [("IfcBearing", "POT"), ("IfcPlate", "BASE_PLATE")]
as4 = result["types-as-ifc4.ifc"]["shapes"]
assert sorted((s["type"], s["predefined"], s["object_type"]) for s in as4.values()
              if s["predefined"] == "USERDEFINED") == [
    ("IfcBuildingElementProxy", "USERDEFINED", "POT"), ("IfcPlate", "USERDEFINED", "BASE_PLATE")]
(here / "verification-results.json").write_text(json.dumps(result, ensure_ascii=False, indent=1))
print("IfcOpenShell", ifcopenshell.version, "OK:", {k: v["elements"] for k, v in result.items()})
sys.exit(0)
