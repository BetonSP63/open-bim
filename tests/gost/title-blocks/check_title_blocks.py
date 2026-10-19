"""Independent check of the ГОСТ Р 21.101-2026 main inscriptions.

Reads the reference DXF/PDF sheets next to this script with ezdxf, pypdf
and OpenCV (none of them share code with Open BIM) and checks, with the
column sizes re-entered here from appendix Е and the frames from appendix Ж:
outer frame = format, inner frame offsets, inscription position and size,
every column line within 0.1 mm, line widths in the PDF, texts, and that the
QR code above the inscription decodes to designation;version;format.
"""
import re
import sys
from pathlib import Path

import cv2
import ezdxf
import numpy as np
from pypdf import PdfReader

D = Path(__file__).resolve().parent
TOL = 0.1
MM = 72 / 25.4

# Column grid of each form in block coordinates (x from the block's left
# edge, y from its bottom), from the dimensioned drawings of appendix Е.
LEFT = [0, 10, 20, 30, 40, 55, 65]


def form_lines(form):
    """Expected (x0, y0, x1, y1) segments of a form."""
    w, h = 185, {3: 55, 4: 55, 5: 40, 6: 15}[form]
    segs = [(0, 0, w, 0), (w, 0, w, h), (0, h, w, h), (0, 0, 0, h)]
    if form in (3, 4, 5):
        header = 30 if form == 5 else 40
        segs += [(0, y, 65, y) for y in range(5, h, 5)]
        segs += [(x, header, x, h) for x in (10, 30)]
        segs += [(x, 0, x, h) for x in (20, 40, 55, 65)]
    else:
        segs += [(0, 5, 65, 5), (0, 10, 65, 10)]
        segs += [(x, 0, x, h) for x in (10, 20, 30, 40, 55, 65, 175)]
        segs += [(175, 8, 185, 8)]
    if form == 3:
        segs += [(65, 45, 185, 45), (65, 30, 185, 30), (135, 25, 185, 25), (65, 15, 185, 15),
                 (135, 0, 135, 30), (150, 15, 150, 30), (165, 15, 165, 30)]
    if form == 4:
        segs += [(65, 40, 185, 40), (135, 35, 185, 35), (135, 20, 185, 20), (65, 15, 185, 15),
                 (135, 0, 135, 40), (150, 20, 150, 40), (165, 20, 165, 40), (155, 15, 155, 20)]
    if form == 5:
        segs += [(65, 25, 185, 25), (135, 20, 185, 20), (135, 15, 185, 15),
                 (135, 0, 135, 25), (150, 15, 150, 25), (165, 15, 165, 25)]
    return w, h, segs


def frame_offsets(size):
    # Appendix Ж: binding field 20 mm (left; top for A4 laid landscape), 5 mm elsewhere.
    if abs(size[0] - 297) < TOL and abs(size[1] - 210) < TOL:
        return 5, 5, 5, 20
    return 20, 5, 5, 5


def segments(doc):
    out = []
    for e in doc.modelspace():
        if e.dxftype() == "LINE":
            a, b = e.dxf.start, e.dxf.end
            out.append((a.x, a.y, b.x, b.y, e.dxf.layer))
        elif e.dxftype() == "POLYLINE":
            pts = [v.dxf.location for v in e.vertices]
            out += [(p.x, p.y, q.x, q.y, e.dxf.layer) for p, q in zip(pts, pts[1:])]
    return out


def has(segs, x0, y0, x1, y1):
    for a in segs:
        for p, q in (((a[0], a[1]), (a[2], a[3])), ((a[2], a[3]), (a[0], a[1]))):
            if abs(p[0] - x0) <= TOL and abs(p[1] - y0) <= TOL and abs(q[0] - x1) <= TOL and abs(q[1] - y1) <= TOL:
                return True
    # A long frame line may be drawn as one segment covering the expected one.
    for a in segs:
        ax0, ay0, ax1, ay1 = a[:4]
        if abs(y0 - y1) <= TOL and abs(ay0 - ay1) <= TOL and abs(ay0 - y0) <= TOL:
            if min(ax0, ax1) <= min(x0, x1) + TOL and max(ax0, ax1) >= max(x0, x1) - TOL:
                return True
        if abs(x0 - x1) <= TOL and abs(ax0 - ax1) <= TOL and abs(ax0 - x0) <= TOL:
            if min(ay0, ay1) <= min(y0, y1) + TOL and max(ay0, ay1) >= max(y0, y1) - TOL:
                return True
    return False


def qr_decode(doc, size):
    solids = [e for e in doc.modelspace() if e.dxftype() == "SOLID"]
    if not solids:
        return None
    px = 20  # pixels per mm
    img = np.full((int(size[1] * px), int(size[0] * px)), 255, np.uint8)
    for s in solids:
        xs = [s.dxf.vtx0.x, s.dxf.vtx1.x, s.dxf.vtx2.x, s.dxf.vtx3.x]
        ys = [s.dxf.vtx0.y, s.dxf.vtx1.y, s.dxf.vtx2.y, s.dxf.vtx3.y]
        x0, x1 = int(round(min(xs) * px)), int(round(max(xs) * px))
        y0, y1 = int(round((size[1] - max(ys)) * px)), int(round((size[1] - min(ys)) * px))
        img[y0:y1, x0:x1] = 0
    text, _, _ = cv2.QRCodeDetector().detectAndDecode(img)
    return text


SHEETS = [
    ("a3-form3", (420, 297), 3, "A3"),
    ("a1-form3", (841, 594), 3, "A1"),
    ("a4-form4", (210, 297), 4, "A4"),
    ("a4-form5", (210, 297), 5, "A4"),
    ("a4-form6", (210, 297), 6, "A4"),
    ("a4-landscape-form3", (297, 210), 3, "A4"),
]

failures = []
for name, size, form, fmt in SHEETS:
    doc = ezdxf.readfile(D / f"{name}.dxf")
    segs = segments(doc)
    left, bottom, right, top = frame_offsets(size)
    W, H = size
    checks = []
    checks.append(("outer frame", all(has(segs, *s) for s in [(0, 0, W, 0), (W, 0, W, H), (W, H, 0, H), (0, H, 0, 0)])))
    inner = [(left, bottom, W - right, bottom), (W - right, bottom, W - right, H - top),
             (W - right, H - top, left, H - top), (left, H - top, left, bottom)]
    checks.append(("inner frame offsets %s" % ((left, bottom, right, top),), all(has(segs, *s) for s in inner)))
    bw, bh, form_segs = form_lines(form)
    x0, y0 = W - right - bw, bottom
    missing = [s for s in form_segs if not has(segs, x0 + s[0], y0 + s[1], x0 + s[2], y0 + s[3])]
    checks.append((f"form {form} {bw}×{bh}: {len(form_segs)} column lines", not missing))
    # DXF R12 keeps non-ASCII text as AutoCAD «\\U+XXXX» escapes.
    texts = [
        re.sub(r"\\U\+([0-9A-Fa-f]{4})", lambda m: chr(int(m.group(1), 16)), e.dxf.text)
        for e in doc.modelspace()
        if e.dxftype() == "TEXT"
    ]
    joined = " ".join(texts)
    want = ["2345-КМД", f"Формат {fmt}", "Изм.", "Кол.уч", "№ док.", "Подп.", "Дата"]
    if form in (3, 4, 5):
        want += ["Разраб.", "Н. контр.", "Иванов", "19.10.26", "Р"]
    checks.append(("texts " + ", ".join(want), all(w in joined for w in want)))
    # Additional graphs on the binding field: inventory numbers, signature and date.
    checks.append(("binding-field graphs 20–22", all(w in joined for w in ["Инв. № подл.", "Подп. и дата", "Взам. инв. №", "1234"])))
    decoded = qr_decode(doc, size)
    checks.append((f"QR decodes «{decoded}»", decoded == f"2345-КМД;1;{fmt}"))
    pdf = PdfReader(D / f"{name}.pdf")
    page = pdf.pages[0]
    mw, mh = float(page.mediabox.width) / MM, float(page.mediabox.height) / MM
    checks.append((f"PDF page {mw:.1f}×{mh:.1f} mm", abs(mw - W) < 0.2 and abs(mh - H) < 0.2))
    content = page.get_contents().get_data().decode("latin-1")
    widths = sorted({float(w) for w in re.findall(r"([0-9.]+) w\b", content)})
    checks.append((f"PDF line widths {widths}", max(widths) >= 0.5 and min(widths) <= 0.35))
    print(f"{name}:")
    for label, ok in checks:
        print(f"  {'ok ' if ok else 'FAIL'} {label}")
        if not ok:
            failures.append(f"{name}: {label}")
    if missing:
        print("    missing:", missing[:5])

print("TITLE_BLOCKS_OK" if not failures else f"FAILURES: {failures}")
sys.exit(1 if failures else 0)
