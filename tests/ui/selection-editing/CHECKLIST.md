# Selection and direct editing (phase 6) — native run

`script.json` uses mouse clicks and drags plus Esc, Delete, Ctrl+M, Ctrl+C and ⌘S;
no typing into fields. Journal: `journal.json`. Independent check: `check_model.py` → `check.txt`.

| Step | Capture | Result |
|---|---|---|
| 3 beams by snapped picks (А1–А2, А2–А3, Б1–Б2) | 01-beams.png | 3 objects |
| Window left→right around А1–А2 | 02-window-select.png | 1 selected: А2–А3 only partly inside → not selected; blue direct-modification handles |
| Drag the end handle А2 → node Б2 | 03-handle-drag.png | only that end moved (start stays at А1) |
| Click Б1–Б2, Ctrl+M, origin Б1 → В1 | 04-moved.png | beam on В |
| Ctrl+C, origin В1 → Г1, Esc | 05-copied.png | 4 objects |
| Click the copy, Delete | 06-deleted.png | 3 objects (no dependants → no prompt) |
