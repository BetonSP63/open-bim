# Phase 13: changes and the issued package (ГОСТ Р 21.101-2026, sections 7, 8; appendices К–У)

## Reference package

`crates/reports/tests/gost_release.rs` writes the reference package
(`OPEN_BIM_BLESS=1` to refresh). The control set (with document attributes)
goes through these steps:
1. КМ and КМД are issued.
2. The beam is lengthened twice.
3. Each time, one common permission changes both sets (15-26, then 16-26).

Outputs:
- `package/` — the package, byte-identical on every run;
- `package.json` — its file list;
- `project.obim` — the project after the changes.

## What the package contains

For each set, `package/<mark>/` holds:

| File | Content |
|---|---|
| `<des>.pdf` | All sheets of the set |
| `<des>-Обложка.pdf` | Cover (form 14) |
| `<des>-ТЛ.pdf` | Title page (form 12), with «(Изм. N)» |
| `<des>-ТРИ.pdf` | Change registration table (form 10) |
| `<des>-ЖИ.pdf` | Change journal (form 11) |
| `Разрешение-<№>.pdf` | Permission for the change (form 9) |
| `<des>-УЛ.pdf` | Certifying sheet (form 15): per file the SHA-256, size and time; signatures; sheets |

`package/info.xml` is the requisite part: folders, documents with stable
GUIDs, version, path, size and SHA-256 of each file.

## Independent check

`check_release.py` uses pypdf, hashlib and ElementTree. It checks:
- every PDF of a set is listed in its УЛ, with the checksum and size of the file on disk;
- info.xml matches the files;
- the cover and title page carry the fields of appendices П and Т: organisation, OBJECT, «РАБОЧАЯ ДОКУМЕНТАЦИЯ», set name, designation «(Изм. N)», volume, year;
- the registration table and journal list each change with its permission and date.

Output: `check.txt` (`RELEASE_OK`). Run with `native/package native/project.obim` to check the package written by the app (`native/check.txt`).

## Native run

Run from `native/`. It starts from the control set with attributes; nothing is issued yet:

    cp release-start.obim release.obim
    ../../../../target/release/open-bim-desktop --open release.obim \
        --size 1600x1000 --record journal.json --script script.json

| Capture | What it shows |
|---|---|
| 01-issued.png | F3, «КМ» then «КМД», «Выдать комплект» |
| 02-model-edit.png | F2, beam А/1-2 selected, «Длина» 6500, Enter |
| 03-change-recorded.png | Permission 15-26, content, «Записать изменение» for КМ and КМД |
| 04-part-change.png | Sheet КМД-2: areas 1.1–1.3 in contours with parallelograms, row «1 / 3 / - / 15-26 / date» in graphs 14–19 |
| 05-general-data-change.png | КМД «Общие данные»: «Изм. 1 (Зам.)», «Изм. 1» in the sheet list |
| 06-package.png | «Выпустить пакет ПДЭ» into `native/package` |

The app saves the project as `native/project.obim` (⌘S).

## Choices recorded

- **Change table.** It shows every change of the sheet, continued above the inscription (5.2.6).
- **Annulled sheets.** Annulled sheets keep their numbers (7.3.9). The sheet list shows «Изм. N (Аннул.)» with the name graph empty (7.5.3).
- **«Общие данные».** The sheet is replaced («Зам.») in each change, because its list changes (7.5.2).
- **Changed areas.** An area is the extent of the lines and dimensions of the changed objects in each view. It also covers the specification table when the sheet has one.
