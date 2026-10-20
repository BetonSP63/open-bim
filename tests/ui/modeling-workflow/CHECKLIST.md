# End-to-end modeling workflow (phase 8)

The script `script.json` drives the real app (and the packaged `.app`) through:
- **Model:** a new model with the default grid; 3 columns at В4/В5/Г4 picked on snapped grid nodes; a beam on the column tops; its end handle dragged to the Г4 top (direct modification).
- **Detailing and checks:** a base plate node on column В4 (side pane «Узлы»); ⌘K numbering and clash check; the property pane sets class 2 on the beam.
- **Output:** ⌘D part drawing and ⇧⌘P PDF; ⌘R lists; ⇧⌘R release set; ⌘E IFC; ⌘S save; ⌘O reopen.

It uses only the ribbon, picks, snapping, a handle drag, the property pane and shortcuts. The only text typed is the class value «2» in the property pane.

| Evidence | File |
|---|---|
| Journal (17 events) | journal.json |
| Captures | 01-new-model … 12-reopened.png |
| Independent checks (JSON geometry, pypdf, IfcOpenShell IFC4 counts, CSV) | check_outputs.py → check.txt |
| The same scenario on the packaged `dist/Open BIM.app` | check-packaged.txt |
| 100 000 objects, all selected, packaged app | bench-100k.json: frame p50 16.67 / p95 16.78 ms, pick p95 0.015 ms, open 0.45 s, peak RSS 345 MiB (`/usr/bin/time -l`) |

Defect found by the benchmark and fixed: with 100 000 selected objects, drawing direct-modification handles for 2000 of them pushed the frame p95 to 33–47 ms. Handles are now drawn for the first 500 selected objects, and the selection set is shared with the renderer instead of copied every frame.
