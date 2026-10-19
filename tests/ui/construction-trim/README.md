# Phase 8: construction lines, model dimensions, fitting and cutting

Native run (from this directory):

    ../../../target/release/open-bim-desktop --script "$PWD/script.json" \
      --project-path "$PWD/construction-trim.obim" --record "$PWD/journal.json"

Commands are started from quick launch (⌘P) by name; points come from
snapping or typed «!x,y,z» coordinates.

| Capture | What it shows |
|---|---|
| 01-construction-line.png | construction line x = 13000 (dashed magenta), ribbon group «Построения» |
| 02-fit-and-dimension.png | beam 1 fitted to the construction line, model dimension 13000 tied to both ends |
| 03-cuts-transparent.png | beam 1 cut by a line at x = 10000, beam 2 cut by the column (transparent view) |
| 04-cut-by-part-zoom.png | the column/beam 2 joint |

`check_ifc_volumes.py` (IfcOpenShell geometry, independent of Open BIM):
beam 1 volume = 5808 mm² × 10000 mm exactly, beam 2 reduced by the column
overlap, two openings, construction objects not exported → `check.txt`.
