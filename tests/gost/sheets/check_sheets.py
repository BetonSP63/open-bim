"""Independent check of the sheets against ГОСТ Р 21.101-2026 5.1.3, 5.1.4, 5.1.11 and appendix Д.

Reads every reference PDF and DXF under tests/gost and tests/fixtures/exports
with pypdf and ezdxf (no Open BIM code):

* 5.1.3 — PDF line widths: main lines 0.5…1.4 mm, thin lines between S/3
  and S/2 of the main lines (ГОСТ 2.303); DXF linetypes are only CONTINUOUS, DASHED and DASHDOT;
* 5.1.4 — every stroke and fill colour is black; white is allowed only as
  a fill (masking areas, the sheet ground);
* 5.1.11 — every abbreviation (1–6 letters and a dot) is in table Д.1, a
  caption of the standard's forms (appendices Е, И, К, М) or a unit; words
  of four or more letters before a line end or a capital are sentence ends,
  and file names (…-ТЛ.pdf) are not words.
"""
import re
import sys
from pathlib import Path

import ezdxf
from pypdf import PdfReader
from pypdf.generic import ContentStream

ROOT = Path(__file__).resolve().parents[3]
MM = 72 / 25.4
failures = []

# Table Д.1 (abbreviations of graphic documents) and the captions of the
# standard's own forms (Е: inscriptions, И: specifications, К: permission,
# М: registration table), re-entered here.
TABLE_D1 = {
    "Альб.", "Аннул.", "оборуд.", "общ.", "отм.", "пом.", "разд.", "разм.", "Арх.", "асф.",
    "бет.", "расч.", "нагр.", "Рук.", "пр.", "гр.", "Вмест.", "Вып.", "Гл.", "инж.", "сб.",
    "спец.", "сек.", "скв.", "снег.", "диам.", "технол.", "Дир.", "Техн.", "док.", "тип.",
    "допуск.", "тр.", "Зав.", "Зам.", "изол.", "фунд.", "цем.", "констр.", "коэф.", "ч.",
    "лестн.", "Ш.", "штукат.", "Маст.", "эл.", "монт.", "эт.", "норм.", "уч.",
}
FORM_CAPTIONS = {
    "Изм.", "Подп.", "Разраб.", "Пров.", "Н.", "контр.", "Утв.", "Инв.", "подл.", "Взам.",
    "инв.", "Поз.", "Кол.", "ед.", "Нов.", "внес", "п.",
}
ALLOWED = TABLE_D1 | FORM_CAPTIONS


def check(ok, message):
    if not ok:
        failures.append(message)


pdfs = sorted(
    p for base in ("tests/gost", "tests/fixtures/exports")
    for p in (ROOT / base).rglob("*.pdf")
)
dxfs = sorted(
    p for base in ("tests/gost", "tests/fixtures/exports")
    for p in (ROOT / base).rglob("*.dxf")
)
check(len(pdfs) >= 20 and len(dxfs) >= 10, f"мало эталонов: PDF {len(pdfs)}, DXF {len(dxfs)}")
widths = set()
found = {}
for pdf in pdfs:
    reader = PdfReader(pdf)
    for page in reader.pages:
        ops = ContentStream(page.get_contents(), reader).operations
        unit = 1.0  # user unit in points, from the «cm» scale
        for operands, op in ops:
            op = op.decode() if isinstance(op, bytes) else op
            if op not in ("w", "cm", "RG", "G", "K", "rg", "g"):
                continue
            nums = [float(x) for x in operands]
            if op == "cm":
                unit *= nums[0]
            elif op == "w":
                widths.add(round(nums[0] * unit / MM, 2))
            elif op in ("RG", "G", "K"):
                check(all(v == 0 for v in nums[:3]) if op != "K" else nums[3] == 1 or all(v == 0 for v in nums),
                      f"{pdf.name}: цвет линии {op} {nums}")
            elif op in ("rg", "g"):
                check(len(set(nums)) == 1 and nums[0] in (0, 1), f"{pdf.name}: цвет заливки {op} {nums}")
    text = "\n".join(p.extract_text() for p in reader.pages)
    for m in re.finditer(r"(?<![А-Яа-яЁё\w-])([А-Яа-яЁё]{1,6}\.)(?!pdf)", text):
        word = m.group(1)
        after = text[m.end():m.end() + 2]
        # A word of four or more letters before a line end or a capital
        # starts a new sentence, it is not an abbreviation.
        if len(word) >= 5 and (after == "" or after[0] == "\n" or (after[0] == " " and after[1:].isupper())):
            continue
        found.setdefault(word, pdf.relative_to(ROOT))
# ГОСТ 2.303: main lines S = 0.5…1.4 mm, thin lines S/3…S/2 of the main
# lines used (0.7 frame and inscription, 0.5 visible contours).
main = [w for w in widths if w >= 0.45]
thin = [w for w in widths if w < 0.45]
check(main and all(0.5 - 0.02 <= w <= 1.4 for w in main), f"основные линии {main} вне 0,5…1,4 мм")
check(thin and all(max(main) / 3 - 0.02 <= w <= min(main) / 2 + 0.02 for w in thin), f"тонкие линии {thin} вне S/3…S/2")
bad = {w: f for w, f in found.items() if w not in ALLOWED}
for w, f in sorted(bad.items()):
    check(False, f"сокращение «{w}» не из таблицы Д.1 и форм стандарта ({f})")
ltypes = set()
for dxf in dxfs:
    doc = ezdxf.readfile(dxf)
    for e in doc.modelspace():
        lt = e.dxf.get("linetype", "BYLAYER").upper()
        if lt not in ("BYLAYER", "BYBLOCK"):
            ltypes.add(lt)
    for layer in doc.layers:
        ltypes.add(layer.dxf.linetype.upper())
check(ltypes <= {"CONTINUOUS", "DASHED", "DASHDOT"}, f"типы линий DXF: {sorted(ltypes)}")

if failures:
    print("\n".join(failures))
    sys.exit(1)
print(f"{len(pdfs)} PDF, {len(dxfs)} DXF: толщины {sorted(widths)} мм, только чёрный (белая заливка масок), "
      f"типы линий {sorted(ltypes)}; сокращения {sorted(found)} — из таблицы Д.1 и граф форм")
print("SHEETS_OK")
