"""Independent check of the native drawing-editor flow (ezdxf, pypdf, json)."""
import json, sys
from pathlib import Path
from ezdxf import recover
from pypdf import PdfReader

here = Path(__file__).parent
project = json.loads((here / "gui-project.obim").read_text())["project"]
(drawing,) = project["drawings"].values()
assert drawing["sheet"] == "a2" and drawing["scale"] == 20.0, drawing["sheet"]
top = next(v for v in drawing["views"] if v["id"] == 2)
assert top["position"][1] < 150, top["position"]  # dragged down on the sheet
assert drawing["texts"][0]["text"] == "Швы по ГОСТ 5264-80, катет 6 мм"
assert drawing["issued_revision"] is not None
events = [e["action"] for e in json.loads((here / "gui-flow.json").read_text())["events"]]
order = ["Создать чертёж", "лист A2", "масштаб 1:20", "вид 2 перемещён", "текст добавлен",
         "Отменить", "Повторить", "Переместить", "обновлён по модели", "выдан", "Сохранить"]
position = 0
for step in order:
    position = next(i for i, e in enumerate(events[position:], position) if step in e) + 1
MM = 72 / 25.4
pdf = PdfReader(here / "export" / "КМД-1.pdf")
page = pdf.pages[0]
size = [round(float(page.mediabox.width) / MM, 2), round(float(page.mediabox.height) / MM, 2)]
assert size == [594.0, 420.0], size
text = page.extract_text()
assert "КМД-1" in text and "Балка Б1" in text and "1:20" in text, text[:200]
doc, auditor = recover.readfile(here / "export" / "КМД-1.dxf")
assert not auditor.has_errors and doc.header.get("$INSUNITS") == 4
texts = [e.dxf.text for e in doc.modelspace().query("TEXT")]
assert "Балка Б1" in texts and "3000" in texts
svg = (here / "export" / "КМД-1.svg").read_text()
assert 'viewBox="0 0 594 420"' in svg and "Вид спереди 1:20" in svg
print(json.dumps({"sheet": drawing["sheet"], "scale": drawing["scale"], "top_view": top["position"],
                  "issued_revision": drawing["issued_revision"], "pdf_mm": size,
                  "dxf_texts": len(texts), "events": events}, ensure_ascii=False, indent=1))
sys.exit(0)
