"""Independent check of the final packaged-app regression (pypdf, IfcOpenShell, csv)."""
import csv, json, sys
from pathlib import Path
import ifcopenshell, ifcopenshell.validate
from pypdf import PdfReader

run = Path(__file__).parent / "run"
expected = json.loads((Path(__file__).parent / "../fixtures/steel_release/expected.json").read_text())
events = [e["action"] for e in json.loads((run / "journal.json").read_text())["events"]]
order = ["Нумерация и проверка коллизий", "Сборки по заводским швам", "Выпуск комплекта: 14 чертежей",
         "Создать чертёж", "Экспорт drawings/", "Экспорт IFC", "Сохранить", "Новый проект", "Импорт IFC", "Открыть"]
pos = 0
for step in order:
    pos = next(i for i, e in enumerate(events[pos:], pos) if e.startswith(step)) + 1
with open(run / "release/ведомость-деталей.csv", newline="", encoding="utf-8-sig") as f:
    parts = list(csv.reader(f))
assert abs(float(parts[-1][7]) - expected["total_mass_kg"]) < 1e-3
pdfs = sorted((run / "release").glob("КМ*.pdf"))
assert len(pdfs) == 14 and len(PdfReader(run / "release/комплект.pdf").pages) == 14
ga = next((run / "drawings").glob("*.pdf"))
page = PdfReader(ga).pages[0]
assert round(float(page.mediabox.width) * 25.4 / 72) == 841 and "Общий вид" in page.extract_text()
model = ifcopenshell.open(str(run / "project.ifc"))
log = ifcopenshell.validate.json_logger()
ifcopenshell.validate.validate(model, log, express_rules=True)
assert not log.statements, log.statements[:3]
assert len(model.by_type("IfcBeam") + model.by_type("IfcColumn") + model.by_type("IfcPlate")) == 16
project = json.loads((run / "project.obim").read_text())["project"]
assert len(project["drawings"]) == 15 and len(project["assemblies"]) == 12
sockets = (run / "network-sockets.txt").read_text().strip()
assert sockets == "", sockets
print(json.dumps({"events": len(events), "release_pdfs": len(pdfs), "ga_pdf": ga.name, "ifc_elements": 16,
                  "ifc_validator_statements": 0, "total_mass_kg": float(parts[-1][7]),
                  "drawings_saved": 15, "network_sockets": 0}, ensure_ascii=False))
sys.exit(0)
