# Phase 11: ГОСТ Р 21.101-2026 drawing conventions (5.3–5.5)

`crates/drawing_export/tests/gost_conventions_export.rs` writes the reference
sheets here (`OPEN_BIM_BLESS=1` to refresh):

- `km-1.*` — КМ scheme of the control building (grid 1–5 × А–Г, levels 0,000
  and +3,600): «Фасад 1-5» with the automatic chain over the row-А columns,
  «План на отм. +3,600», «Разрез 1-1» with its section line on the plan;
- `kmd-1.*` — КМД part drawing of a beam (ГОСТ 2.307 arrows, no ticks).

`check_conventions.py` (ezdxf, pypdf; limits re-entered by hand) checks
ticks 45° 2–4 mm, digits without units, line overshoot ≤ 3 mm, level signs
and the framed plan mark, bubbles 6–12 mm with labels 1–5/А–Г on a dash-dot
linetype, axis/dimension font ratio 1.5–2, titles not underlined, section
strokes → `check.txt` (`CONVENTIONS_OK`).

Native run in `native/` (from that directory):

    ../../../../target/release/open-bim-desktop --open control-building.obim \
        --size 1600x1000 --record journal.json --script script.json

| Capture | What it shows |
|---|---|
| 01-ga-sheet.png | ⌘P «Общий вид A1»: elevation with ticks, axes, levels; plan with framed mark |
| 02-part-sheet.png | F2, click on column 1/А, ⌘D: КМД part drawing with arrows |

`native/drawings/` holds the PDFs exported from the app (⇧⌘P);
`pdf-check.txt` is the pypdf check of their texts (`GUI_PDF_OK`).

Font sizes are ГОСТ 2.304 sizes (capital-letter height): digits 2.5, axis
labels, marks and titles 5, level values 3.5.
