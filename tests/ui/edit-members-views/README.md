# Phase 7: members, levels, grid, navigation, display

Native runs (from this directory):

    ../../../target/release/open-bim-desktop --example --script "$PWD/script-members.json" --record "$PWD/journal-members.json"
    ../../../target/release/open-bim-desktop --script "$PWD/script-levels-grid.json"

| Capture | What it shows |
|---|---|
| 01-beam-length.png | selected beam: «L = 6000» in the view, «Длина, мм» and «Изменять» in the pane |
| 02-length-7000.png | length set to 7000 from the keyboard (F6, Tab, Enter), mode «Конец»: start stays at 0 |
| 03-wireframe.png | Ctrl+1: feature edges only |
| 04-transparent-50.png | Ctrl+2: 50 % transparency, the column shows through the beam |
| 05-shaded.png | Ctrl+4: shaded |
| 06-trackpad-orbit-pivot.png | two-finger scroll orbits about the column under the cursor (orange pivot mark stays put) |
| 07-level-selected.png | click on the +3,600 level line: level selected, level pane |
| 08-grid-double-click.png | double-click on grid line 2: the grid dialog opens with the project grid for editing |

`transparency.txt` — pixel metric (`transparency_metric.py`): the transparent and
wireframe captures differ from the shaded one, and the selected beam's pixels
let the background through. `bench-100k-*.json` — 100 000 selected parts per
display mode.
