"""Independent check of the GUI steel release (pypdf, csv, json; no project code)."""
import csv, json, sys
from pathlib import Path
from pypdf import PdfReader

here = Path(__file__).parent
expected = json.loads((here / "../fixtures/steel_release/expected.json").read_text())
project = json.loads((here / "gui-project.obim").read_text())["project"]
assert len(project["assemblies"]) == 12 and len(project["drawings"]) == 14
members = [m for a in project["assemblies"].values() for m in a["members"]]
assert sorted(members) == sorted(int(k) for k in project["elements"]), "each part in one assembly"
for name in ["ведомость-деталей.csv", "ведомость-сборок.csv", "крепёж-и-сварка.csv"]:
    a = (here / "release" / name).read_bytes(); b = (here / "reopen" / name).read_bytes()
    assert a == b, f"{name} differs after reopen"
with open(here / "release" / "ведомость-деталей.csv", newline="", encoding="utf-8-sig") as f:
    parts = list(csv.reader(f))
assert abs(float(parts[-1][7]) - expected["total_mass_kg"]) < 1e-3
MM = 72 / 25.4
pdfs = sorted(p for p in (here / "release").glob("КМ*.pdf"))
assert len(pdfs) == 14
sizes = {}
for p in pdfs:
    page = PdfReader(p).pages[0]
    sizes[p.name] = [round(float(page.mediabox.width) / MM), round(float(page.mediabox.height) / MM)]
    assert p.stem.split("-", 1)[1] in page.extract_text().replace("\n", " ") or p.stem in page.extract_text()
whole = PdfReader(here / "release" / "комплект.pdf")
assert len(whole.pages) == 14
release = json.loads((here / "release" / "release.json").read_text())
assert release["revision"] == project["revision"]
events = [e["action"] for f in ["gui-flow-1.json", "gui-flow-2.json"]
          for e in json.loads((here / f).read_text())["events"]]
print(json.dumps({"assemblies": 12, "drawings": 14, "pdf_sizes_mm": sizes, "set_pages": 14,
                  "csv_equal_after_reopen": True, "total_mass_kg": float(parts[-1][7]),
                  "events": events}, ensure_ascii=False, indent=1))
sys.exit(0)
