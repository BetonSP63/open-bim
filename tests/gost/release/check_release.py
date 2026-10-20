"""Independent check of the issued package (ГОСТ Р 21.101-2026, 8.2, appendices П, Т, У).

Reads package/ next to this script with pypdf, hashlib and xml.etree (no
Open BIM code) and the project file as plain JSON:

* every certifying sheet (form 15) lists every other PDF of its set folder
  and the SHA-256 and size it states match the file on disk;
* info.xml lists every file with the same SHA-256 and size and a GUID per
  document;
* cover (form 14) and title page (form 12) carry the organisation, the object
  name in capitals (graphs 2 and 3), «РАБОЧАЯ ДОКУМЕНТАЦИЯ», the set name, the
  designation (with «(Изм. N)» on the title page), the volume and the year;
* registration table (form 10) and journal (form 11) list each change of the
  project with its permission and date.
"""
import hashlib
import json
import re
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

from pypdf import PdfReader

D = Path(__file__).resolve().parent
# Optional arguments: package folder and project file (the native run).
P = Path(sys.argv[1]) if len(sys.argv) > 1 else D / "package"
PROJECT = Path(sys.argv[2]) if len(sys.argv) > 2 else D / "project.obim"
failures = []


def check(ok, message):
    if not ok:
        failures.append(message)


def text(path):
    return "\n".join(p.extract_text() for p in PdfReader(path).pages)


project = json.loads(PROJECT.read_text(encoding="utf-8"))["project"]
doc = project["document"]
base = doc["base_designation"]
changes = project.get("changes", [])

checked = 0
for folder in sorted(p for p in P.iterdir() if p.is_dir()):
    mark = folder.name
    des = f"{base}-{mark}"
    ul = folder / f"{des}-УЛ.pdf"
    check(ul.exists(), f"{mark}: нет удостоверяющего листа")
    if not ul.exists():
        continue
    ul_text = re.sub(r"\s+", " ", text(ul))
    for f in sorted(folder.glob("*.pdf")):
        if f == ul:
            continue
        data = f.read_bytes()
        digest = hashlib.sha256(data).hexdigest()
        rel = f"{mark}/{f.name}"
        check(rel in ul_text, f"УЛ {mark}: нет файла {rel}")
        # The sheet prints the checksum in two halves of 32 characters.
        check(f"{digest[:32]} {digest[32:]}" in ul_text, f"УЛ {mark}: SHA-256 {rel} не совпадает")
        check(str(len(data)) in ul_text, f"УЛ {mark}: размер {rel}")
        checked += 1
    check(f"{des}-УЛ" in ul_text and "SHA-256" in ul_text, f"УЛ {mark}: обозначение или алгоритм")
    for name in ("Иванов", "Петров"):
        check(name in ul_text, f"УЛ {mark}: нет подписи {name}")

    n_changes = sum(1 for c in changes if c["mark"] == mark)
    year = "20" + project["changes"][-1]["date"][-2:] if changes else None
    for kind, f in (("обложка", f"{des}-Обложка.pdf"), ("титульный лист", f"{des}-ТЛ.pdf")):
        t = re.sub(r"\s+", " ", text(folder / f))
        for need in (
            doc["organisation"],
            f'{doc["object"]}. {doc["building"]}'.upper(),
            "РАБОЧАЯ ДОКУМЕНТАЦИЯ",
            "Основной комплект рабочих чертежей",
            "Том ",
        ):
            check(need in t, f"{mark} {kind}: нет «{need}»")
        check(year is None or year in t, f"{mark} {kind}: нет года")
        if n_changes:
            check(f"{des} (Изм. {n_changes})" in t, f"{mark} {kind}: без «(Изм. {n_changes})» (7.1.6)")
        else:
            check(des in t, f"{mark} {kind}: нет обозначения")
    if n_changes:
        reg = re.sub(r"\s+", " ", text(folder / f"{des}-ТРИ.pdf"))
        jour = re.sub(r"\s+", " ", text(folder / f"{des}-ЖИ.pdf"))
        for c in changes:
            if c["mark"] != mark:
                continue
            for t, name in ((reg, "таблица регистрации"), (jour, "журнал")):
                check(c["permission"] in t and c["date"] in t, f"{mark} {name}: изм. {c['number']}")
            check(c["description"] in jour, f"{mark} журнал: содержание изм. {c['number']}")
            check((folder / f"Разрешение-{c['permission']}.pdf").exists(), f"{mark}: нет разрешения {c['permission']}")

# info.xml: every file with its checksum and size, a GUID per document.
root = ET.parse(P / "info.xml").getroot()
guids = set()
listed = 0
for d in root.iter("document"):
    guid = d.get("guid")
    check(re.fullmatch(r"[0-9a-f]{8}-[0-9a-f]{4}-5[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}", guid or ""), f"info.xml: GUID {guid}")
    guids.add(guid)
    for f in d.iter("file"):
        data = (P / f.get("path")).read_bytes()
        check(hashlib.sha256(data).hexdigest() == f.get("sha256"), f"info.xml: SHA-256 {f.get('path')}")
        check(int(f.get("size")) == len(data), f"info.xml: размер {f.get('path')}")
        listed += 1
on_disk = sum(1 for _ in P.rglob("*.pdf"))
check(listed == on_disk, f"info.xml: файлов {listed}, на диске {on_disk}")
check(len(guids) == listed, "info.xml: GUID не уникальны")

if failures:
    print("\n".join(failures))
    sys.exit(1)
print(f"УЛ: {checked} файлов с SHA-256 и размером совпадают; info.xml: {listed} документов; "
      f"обложки и титульные листы с полями П/Т; изменений: {len(changes)}")
print("RELEASE_OK")
