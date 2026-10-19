"""Independent check of an edited drawing's export (ezdxf, pypdf): the user
line, 45° hatch lines inside their area, the masking area as SOLID fills on
OB_MASK, and the sheet dimension; in the PDF the mask is filled white after
the view geometry."""
import math
import re
import sys
from pathlib import Path

import ezdxf
from pypdf import PdfReader

D = Path(__file__).resolve().parent
dxf = next(D.glob("*.dxf"))
doc = ezdxf.readfile(dxf)
msp = doc.modelspace()


def segs(layer):
    out = []
    for e in msp.query(f'LINE[layer=="{layer}"]'):
        out.append((e.dxf.start, e.dxf.end))
    for e in msp.query(f'POLYLINE[layer=="{layer}"]'):
        pts = [v.dxf.location for v in e.vertices]
        out += list(zip(pts, pts[1:]))
    return out


checks = []
thin = segs("OB_THIN")
# The test moved the line 10 mm right with the arrow key before export.
checks.append(("user line 50,40 → 130,40 on OB_THIN (after a 10 mm move)", any(
    abs(a.x - 50) < 0.01 and abs(a.y - 40) < 0.01 and abs(b.x - 130) < 0.01 and abs(b.y - 40) < 0.01 for a, b in thin)))
hatch = [(a, b) for a, b in segs("OB_HATCH") if 60 - 0.01 <= min(a.x, b.x) and max(a.x, b.x) <= 100 + 0.01 and 60 - 0.01 <= min(a.y, b.y) and max(a.y, b.y) <= 90 + 0.01]
angles = {round(math.degrees(math.atan2(b.y - a.y, b.x - a.x)) % 180, 6) for a, b in hatch}
checks.append((f"hatch: {len(hatch)} lines inside 60…100 × 60…90, angles {angles}", len(hatch) > 5 and angles == {45.0}))
solids = msp.query('SOLID[layer=="OB_MASK"]')
xs = [v.x for s in solids for v in (s.dxf.vtx0, s.dxf.vtx1, s.dxf.vtx2)]
ys = [v.y for s in solids for v in (s.dxf.vtx0, s.dxf.vtx1, s.dxf.vtx2)]
checks.append((f"mask: {len(solids)} SOLID on OB_MASK covering 150…200 × 150…190",
               len(solids) >= 2 and min(xs) == 150 and max(xs) == 200 and min(ys) == 150 and max(ys) == 190))
texts = [re.sub(r"\\U\+([0-9A-Fa-f]{4})", lambda m: chr(int(m.group(1), 16)), t.dxf.text) for t in msp.query("TEXT")]
checks.append(("dimension values present", any(t.isdigit() for t in texts)))
pdf = next(D.glob("*.pdf"))
content = PdfReader(pdf).pages[0].get_contents().get_data().decode("latin-1")
mask_at = content.find("1 1 1 rg")
checks.append(("PDF: white mask fill drawn after the first view lines", mask_at > content.find(" m\n") > 0))
for label, ok in checks:
    print(("ok   " if ok else "FAIL ") + label)
ok = all(c[1] for c in checks)
print("DRAWING_EDITING_OK" if ok else "DRAWING_EDITING_FAIL")
sys.exit(0 if ok else 1)
