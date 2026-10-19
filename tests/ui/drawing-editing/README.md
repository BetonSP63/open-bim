# Phase 10: editing drawings

Headless test `drawing_editing.rs` writes `КМД-1.dxf/.pdf/.svg` here;
`check_editing_export.py` (ezdxf, pypdf) checks the user line, the 45° hatch
inside its area, the mask as white SOLIDs on OB_MASK and the dimensions →
`check.txt`.

Native run in `native/` (example project, ⌘D on beam 1, then the toolbar
above the sheet):

| Capture | What it shows |
|---|---|
| 01-dimension.png | «Размер»: two clicks on the beam ends, Enter — a new associative chain |
| 02-hatch-mask-line.png | hatch (metal 45°) on the plan view, mask hiding the end view, a line |
| 03-line-selected-moved.png | the line selected by a click and moved 20 mm with ⇧↑ |
| 04-line-deleted.png / 05-undo.png | Delete, then ⌘Z restores it |

The two-row toolbar keeps the sheet in place when a tool with options is
chosen.
