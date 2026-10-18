# Releases

## 0.1.0 — first steel release (local, macOS ARM64)

Package: `scripts/package-macos.sh` → `dist/Open BIM.app` and
`dist/Open-BIM-0.1.0-macos-arm64.zip`. **Not code-signed and not notarized**
(no Developer ID certificate): on first launch use right-click → «Открыть».
Contents: `open-bim-desktop` (GUI), `open-bim-release` (CLI release set),
`open-bim-synthetic` (benchmark corpus), examples (steel release model, drawing
control model), README, LICENSE. Works offline: no network code; the final run
opened no sockets (`tests/final/run/network-sockets.txt` empty).

Included (verified, see `docs/parity.csv`): steel modelling (members, plates,
grids), cuts/holes, bolts/welds, three parametric connections, numbering, clash
check, drawings (GA/part/assembly, HLR, sections, associative dimensions,
status), drawing editor, PDF/SVG/DXF export, steel lists and CSV, IFC4 subset
exchange, keyboard/command palette UI, background jobs, 100k-object viewport.

Not included: concrete (phase 15) and rebar (phase 16) — deferred by the user's
decision of 2026-10-18; IFC4.3, DWG, NC/DSTV, native Tekla formats,
collaboration, API, Windows/Linux, full section catalogues, capacity design.
This release is **not** a full equivalent of Tekla Structures.

Final regression on the packaged app: `tests/final/VERIFICATION.md`.
