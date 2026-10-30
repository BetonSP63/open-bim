# Releases

## Application branding and native packaging

The default application logo is the structural letter O, shared by all native
packages. The source and generated ICNS/ICO/PNG assets live in `assets/icons`.
The macOS bundle declares `CFBundleIconFile`; the Windows build embeds the ICO
using `winresource`; the native viewport embeds the PNG. Linux ships hicolor
icons and `org.openbim.OpenBIM.desktop`, matching the window's application ID.

Native packaging entry points:

- macOS: `scripts/package-macos.sh` → `.app` and `.zip`.
- Windows (PowerShell 7): `scripts/package-windows.ps1` → `.zip` with EXEs.
- Linux: `scripts/package-linux.sh` → `.tar.gz`; the included `install.sh`
  installs binaries, icons and the desktop entry under `~/.local` by default.

The `Native application packages` workflow builds these on their respective
GitHub runners when manually dispatched. Windows/Linux support in the original
0.1.0 audit below remains unverified until native builds and runtime checks pass.

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
