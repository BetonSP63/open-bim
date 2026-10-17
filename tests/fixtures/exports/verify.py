"""Independent verification of exported sheets.

Run with work/verify-venv/bin/python (ezdxf 1.3.5, pypdf 5.1.0). No project code.
"""
import json, sys
from pathlib import Path
import ezdxf
from ezdxf import recover
from pypdf import PdfReader

here = Path(__file__).parent
summary = json.loads((here / "summary.json").read_text())
MM = 72 / 25.4
result = {}
for name, info in summary.items():
    out = {}
    # PDF: page size, Cyrillic text through ToUnicode, control dimension position.
    reader = PdfReader(here / f"{name}.pdf")
    page = reader.pages[0]
    size = [float(page.mediabox.width) / MM, float(page.mediabox.height) / MM]
    assert all(abs(a - b) < 0.01 for a, b in zip(size, info["size_mm"])), (name, size)
    found = []
    def visitor(text, cm, tm, font, size_):
        if text.strip():
            # cm scales mm→pt; tm holds the text origin in mm.
            found.append((text, tm[4], tm[5]))
    text = page.extract_text(visitor_text=visitor)
    assert "ОБ-" in text and ("Вид" in text or "Фасад" in text), name
    out["page_mm"] = [round(v, 3) for v in size]
    out["cyrillic_sample"] = next(t for t, _, _ in found if any("А" <= c <= "я" for c in t))
    if name == "beam-a3":
        dims = [(t, x, y) for t, x, y in found if t == "3000"]
        assert dims, "control dimension text in PDF"
        _, x, y = dims[0]
        out["pdf_dimension_3000_at_mm"] = [round(x, 3), round(y, 3)]
    # DXF: audit, units, layers and entity counts per layer.
    doc, auditor = recover.readfile(here / f"{name}.dxf")
    assert not auditor.has_errors, (name, [str(e) for e in auditor.errors])
    assert doc.header.get("$INSUNITS") == 4, name
    layers = {l.dxf.name for l in doc.layers}
    msp = doc.modelspace()
    counts = {}
    for e in msp:
        counts[e.dxf.layer] = counts.get(e.dxf.layer, 0) + 1
    for layer, expected in info["entities"].items():
        assert layer in layers, (name, layer)
        assert counts.get(layer, 0) == expected, (name, layer, counts.get(layer, 0), expected)
    texts = [e.dxf.text for e in msp.query("TEXT")]
    assert any("ОБ-" in t for t in texts), (name, texts[:5])
    out["dxf_layers"] = sorted(layers)
    out["dxf_counts"] = counts
    if name == "beam-a3":
        # Control dimension: the 3000 mm dimension line is 300 mm of paper at 1:10.
        lines = [e for e in msp.query("LINE[layer=='OB_THIN']")
                 if abs(e.dxf.start.y - e.dxf.end.y) < 1e-9 and abs(abs(e.dxf.end.x - e.dxf.start.x) - 300) < 0.01]
        assert lines, "300 mm dimension line in DXF"
        t = next(e for e in msp.query("TEXT") if e.dxf.text == "3000")
        ref = t.dxf.align_point if t.dxf.hasattr("align_point") else t.dxf.insert
        out["dxf_dimension_3000_at_mm"] = [round(ref.x, 3), round(ref.y, 3)]
        assert abs(ref.x - (lines[0].dxf.start.x + lines[0].dxf.end.x) / 2) < 0.01
    result[name] = out
pdf = PdfReader(here / "control-set.pdf")
result["control-set.pdf"] = [[round(float(p.mediabox.width) / MM, 2), round(float(p.mediabox.height) / MM, 2)] for p in pdf.pages]
assert result["control-set.pdf"] == [[420, 297]] * 3 + [[841, 594]]
print(json.dumps(result, ensure_ascii=False, indent=1))
sys.exit(0)
