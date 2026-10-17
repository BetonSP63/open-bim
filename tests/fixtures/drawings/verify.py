"""Independent reader for reference SVG sheets (Python xml.etree, no project code)."""
import json, sys
import xml.etree.ElementTree as ET
from pathlib import Path

NS = "{http://www.w3.org/2000/svg}"
here = Path(__file__).parent
expect = {
    "beam-part.svg": {"number": "ОБ-КМД-Б1", "scale": 10, "length": 3000.0, "dims": ["3000", "500", "750", "1750"]},
    "plate-part.svg": {"number": "ОБ-КМД-П1", "scale": 2, "length": 400.0, "dims": ["60", "280", "80", "140"]},
    "assembly.svg": {"number": "ОБ-КМД-С1", "scale": 10, "length": None, "dims": ["3000"]},
    "general.svg": {"number": "ОБ-КМ-1", "scale": None, "length": None, "dims": []},
}
report = {}
for name, e in expect.items():
    root = ET.parse(here / name).getroot()
    assert root.tag == NS + "svg", name
    w, h = float(root.get("viewBox").split()[2]), float(root.get("viewBox").split()[3])
    assert (w, h) == (420.0, 297.0), name  # A3 landscape
    texts = [t.text for t in root.iter(NS + "text")]
    assert e["number"] in texts, name
    if e["scale"]:
        assert f"1:{e['scale']}" in texts, name
    dims = [t.text for g in root.iter(NS + "g") if g.get("class") == "dimension" for t in g.iter(NS + "text")]
    for d in e["dims"]:
        assert d in dims, (name, d, dims)
    frames = [p for p in root.iter(NS + "polyline") if p.get("class") == "Frame"]
    assert len(frames) >= 3, name
    visible = 0.0
    if e["length"]:
        view = next(g for g in root.iter(NS + "g") if g.get("data-view") == "1")
        xs = [float(pt.split(",")[0]) for p in view.iter(NS + "polyline") if p.get("class") == "Visible"
              for pt in p.get("points").split()]
        visible = (max(xs) - min(xs)) * e["scale"]
        assert abs(visible - e["length"]) <= 0.01 + 0.0005 * e["scale"], (name, visible)
    report[name] = {"texts": len(texts), "dimension_texts": dims, "measured_length_mm": visible}
print(json.dumps(report, ensure_ascii=False, indent=1))
sys.exit(0)
