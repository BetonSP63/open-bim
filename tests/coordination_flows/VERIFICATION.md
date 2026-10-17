# Native clash/numbering workflow verification (phase 10)

Real Group BIM application (release build, macOS ARM64, Apple M5, Metal), opened
`tests/connections_flows/gui-project.obim` (phase-9 fixture: 16 parts, 6 nodes).
Input came from `gui-script.json` through `--script`: pointer moves, button presses,
keys and text are injected as ordinary egui raw input events in `raw_input_hook`,
so they pass through the same widgets as manual input. No model data was edited
outside the GUI. Run log: `gui-run.log` (exit 0), journal: `gui-flow.json`.

| Step | Capture | Result |
|---|---|---|
| Select `Base column B #9` in tree, X = -1300, «Переместить» | step-1.png | column and its base plate regenerated at X=200 |
| «Нумеровать и проверить» | step-2.png | 7 part positions, 64 candidates, 6 intersections |
| Click row 1 | step-3.png | #8 and #9 highlighted in tree and viewport |
| Click row 21 | step-4.png | anchor bolt #46.3 through plate #49: plates #41/#49 highlighted |
| ⌘S | gui-project.obim | revision 10 saved |

Independent check `verify.py` (own I-section rectangle overlap, no project code):
#8/#9 overlap 3 600 000 mm³ = report value. `crates/desktop/tests/coordination.rs`
re-runs the checker on the saved file and asserts the same 6 rows and volume.
On the unmodified fixture the checker reports 12 touching faces, 40 fitted bolt
holes and no intersections.

Limits: bolt shafts are 32-gon prisms inscribed in the nominal diameter minus
0.0002 mm; nuts/washers/heads are not modelled; the view is not zoomed to the
selected row. No claim of full Tekla clash-check parity.
