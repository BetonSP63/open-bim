# Releases

## 1.0 — in progress (plan of 2026-10-30)

Target: steel (КМ/КМД) and reinforced concrete (КЖ) design with ГОСТ documentation,
on macOS, Linux and Windows. Rows with `release=1.0` in `docs/parity.csv` are the
scope; the phase order is in `docs/backlog.md`.

Already on `main` since 0.1.0 (PRs #1–#72, not yet packaged as a release):
part position, ГОСТ section tables and steel grades, property files, full snapping,
copy special and selection filters, component catalogue with 8 connections and
auto-connections, custom components, bolts by circle/list and polyline welds,
ГОСТ welds with ГОСТ 2.312 signs, numbering series and control numbers, document
manager, drawing wizard and organizer, NC/DSTV `.nc1` export (subset), IFC import
from other programs, object API, batch mode and plugins, full reference-style
ribbon and panes, analysis model (no solver), concrete/rebar/formwork basics,
user and welded sections, parametric families.

Decisions (2026-10-30): collaboration in 1.0 is a shared folder with locks (a
server comes in 1.1); DXF is the primary CAD exchange and DWG goes through the
unmodified open-source `acadrust` crate (MPL-2.0); macOS signing and notarization
are postponed.

Still open for 1.0: КЖ by ГОСТ (bar shapes, positions, schedules, drawings, IFC
rebar, BVBS, pours), full catalogue check, NC cuts and unfoldings, IFC4.3,
reference models and BCF, DWG, Linux/Windows, collaboration, public API 1.0.

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
decision of 2026-10-18; IFC4.3, DWG, NC/DSTV, proprietary native formats,
collaboration, API, Windows/Linux, full section catalogues, capacity design.
This release is **not** a full equivalent of commercial steel detailing software.

Final regression on the packaged app: `tests/final/VERIFICATION.md`.
