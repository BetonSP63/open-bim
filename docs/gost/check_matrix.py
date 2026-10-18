"""Check the ГОСТ Р 21.101-2026 matrix against the standard's PDF.

Usage: python -I check_matrix.py <ГОСТ PDF> [--final]

Extracts every numbered clause (sections 1–8 and appendices А–У) from the PDF
text and checks that each one has a row in gost-r-21.101-2026.md; every row
has an applicability, and «применяется» / «поле/процедура» rows have a phase.
With --final, every «применяется» row must also name its evidence.
"""
import re
import sys
from pathlib import Path

from pypdf import PdfReader

APPENDICES = "АБВГДЕЖИКЛМНПРСТУ"
CLAUSE = re.compile(
    r"^\s*((?:[1-8З]|[" + APPENDICES + r"])(?:\.\s?[\dЗ]+){1,3})\.?(?:\s|_|$)"
)


def pdf_clauses(path):
    text = "\n".join(page.extract_text() or "" for page in PdfReader(path).pages)
    lines = text.split("\n")
    # Skip the table of contents: start at the first «1 Область применения».
    start = next(i for i, l in enumerate(lines) if re.match(r"^\s*1\s+Область применения\s*$", l))
    found = []
    for line in lines[start + 1:]:
        m = CLAUSE.match(line)
        if not m:
            continue
        n = m.group(1).replace(" ", "")
        # The PDF text layer sometimes has Cyrillic «З» for the digit 3.
        n = ("3" if n[0] == "З" else n[0]) + n[1:].replace("З", "3")
        if n not in found:
            found.append(n)
    return ["1", "2"] + found + list(APPENDICES)


def matrix_rows(path):
    rows = {}
    for line in path.read_text().splitlines():
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) != 5 or cells[0] in ("Пункт", "---"):
            continue
        rows[cells[0]] = cells
    return rows


def main():
    pdf = Path(sys.argv[1])
    final = "--final" in sys.argv
    rows = matrix_rows(Path(__file__).with_name("gost-r-21.101-2026.md"))
    clauses = pdf_clauses(pdf)
    missing = [c for c in clauses if c not in rows]
    extra = [r for r in rows if r not in clauses]
    problems = []
    for key, (_, req, app, phase, evidence) in rows.items():
        if not req or not app:
            problems.append(f"{key}: пусто")
        kind = app.split(" — ")[0]
        if kind not in ("применяется", "поле/процедура", "не применяется", "справочно"):
            problems.append(f"{key}: применимость «{app}»")
        if kind in ("применяется", "поле/процедура") and not re.search(r"\d", phase):
            problems.append(f"{key}: нет этапа")
        if final and kind == "применяется" and evidence in ("", "—"):
            problems.append(f"{key}: нет доказательства")
    counts = {}
    for r in rows.values():
        k = r[2].split(" — ")[0]
        counts[k] = counts.get(k, 0) + 1
    print(f"PDF clauses: {len(clauses)}; matrix rows: {len(rows)}")
    print("by applicability:", ", ".join(f"{k} {v}" for k, v in sorted(counts.items())))
    print(f"missing: {len(missing)} {missing[:20]}")
    print(f"extra rows: {len(extra)} {extra[:20]}")
    print(f"row problems: {len(problems)} {problems[:20]}")
    if missing or extra or problems:
        sys.exit(1)
    print("MATRIX_OK" + (" (final)" if final else ""))


if __name__ == "__main__":
    main()
