"""Independent checks of the end-to-end GUI run outputs.

Uses only the saved files: workflow.obim (JSON), drawings/*.pdf and the release
set (pypdf), workflow.ifc (IfcOpenShell) and the CSV lists.
"""
import csv
import json
import math
import sys
from pathlib import Path

import ifcopenshell
from pypdf import PdfReader

root = Path(sys.argv[1])
p = json.loads((root / "workflow.obim").read_text(encoding="utf-8"))["project"]


def world(frame, local):
    o, axes = frame["origin"], frame["axes"]
    return tuple(o[i] + sum(axes[j][i] * local[j] for j in range(3)) for i in range(3))


def close(a, b):
    return all(math.isclose(u, v, abs_tol=1e-6) for u, v in zip(a, b))


members = [e for e in p["elements"].values() if e["shape"]["kind"] == "member"]
columns = [e for e in members if e["shape"]["member_kind"] == "column"]
beams = [e for e in members if e["shape"]["member_kind"] == "beam"]
assert len(columns) == 3 and len(beams) == 1, (len(columns), len(beams))
nodes = {(18000.0, 12000.0), (24000.0, 12000.0), (18000.0, 18000.0)}
assert {tuple(c["shape"]["frame"]["origin"][:2]) for c in columns} == nodes
beam = beams[0]
a = world(beam["shape"]["frame"], (0, 0, 0))
b = world(beam["shape"]["frame"], (0, 0, beam["shape"]["length"]))
assert close(a, (18000, 12000, 3600)) and close(b, (18000, 18000, 3600)), (a, b)
assert beam["properties"].get("class", {}).get("value") == "2", beam["properties"]
connections = list(p.get("connections", {}).values())
assert len(connections) == 1 and connections[0]["template"]["kind"]["type"] == "base_plate", connections
plates = [e for e in p["elements"].values() if e["shape"]["kind"] == "plate"]
assert len(plates) == 1, "generated base plate"

pdf = PdfReader(root / "drawings" / "КМД-1.pdf")
assert len(pdf.pages) == 1
text = pdf.pages[0].extract_text()
# Graph 1: set designation «<base>-КМД» (ГОСТ Р 21.101, 4.2.4).
assert "-КМД" in text, text[:200]

release = json.loads((root / "release" / "release.json").read_text(encoding="utf-8"))
pdfs = sorted(f.name for f in (root / "release").glob("*.pdf"))
assert "комплект.pdf" in pdfs and len(pdfs) >= 4, pdfs
assert len(PdfReader(root / "release" / "комплект.pdf").pages) >= 4
with open(root / "release" / "ведомость-деталей.csv", encoding="utf-8-sig") as f:
    rows = list(csv.reader(f, delimiter=";"))
assert len(rows) >= 3, rows[:3]

ifc = ifcopenshell.open(str(root / "workflow.ifc"))
counts = {t: len(ifc.by_type(t)) for t in ["IfcColumn", "IfcBeam", "IfcPlate"]}
assert counts == {"IfcColumn": 3, "IfcBeam": 1, "IfcPlate": 1}, counts
assert ifc.schema == "IFC4"

journal = json.loads((root / "journal.json").read_text(encoding="utf-8"))["events"]
actions = [e["action"] for e in journal]
for needed in ["Соединение", "Нумерация и проверка коллизий", "Изменить свойства (1)", "Создать чертёж КМД-1", "Сохранить"]:
    assert needed in actions, needed
assert actions[-1].startswith("Открыть"), actions[-1]
print(f"OK: 3 columns at В4/В5/Г4, beam В4→Г4 at +3600 (handle-dragged, class 2), base plate node, "
      f"drawing PDF «-КМД», release {len(pdfs)} PDF + CSV, IFC4 {counts}, journal {len(actions)} events, reopened")
