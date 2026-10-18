# Final audit — Open BIM 0.1.0 (2026-10-18)

**Verdict: Open BIM is not a full functional equivalent of Tekla Structures.**
The verified scope is a first steel workflow on macOS ARM64 (model → connections
→ numbering/clashes → drawings → lists → IFC4 subset) with a hardened UI. 14 of 34
parity rows remain open (below). Phases 15 (concrete) and 16 (rebar) of the
approved 18-phase plan were **not implemented**: the user decided on 2026-10-18 to
skip them and continue with phases 17–18.

## Tekla-style interaction run (2026-10-19)

**Verdict unchanged:** Open BIM is still not a full equivalent of Tekla Structures.
13 of 35 parity rows are open (grid-levels is now verified; the new
row «tekla-interaction» is verified).
This run makes the way of working and the interface Tekla-like:
- **Window:** light chrome with ribbon, quick launch, side pane, toolbars and the blue prompt.
- **Viewport:** gradient, origin symbol, gizmo, work area, grid bubbles, snap symbols, rubber band and handles.
- **Creating:** objects by picks; grid strings; columns with bottom/top; bolts by parts and points; welds between parts.
- **Editing:** window/crossing selection with switches; direct modification; move, copy, mirror and rotate by picks.
- **Properties and views:** property pane with «— разные —», contextual toolbar, views along grid lines with view depth, measuring.

It was checked against the user-supplied Tekla 2026 manuals (`docs/tekla-docs-comparison.md`). The mechanics still missing are listed in `docs/tekla-interaction.md`.

| Check (final tree) | Result |
|---|---|
| 4 mandatory commands | exit 0, 0 warnings; 151 tests passed, 2 ignored release benchmarks (run separately below) |
| End-to-end GUI scenario `tests/ui/tekla-workflow` (dev build and packaged `.app`) | 17 journal events; JSON/pypdf/IfcOpenShell checks pass (`check.txt`, `check-packaged.txt`) |
| GUI runs of phases 5 and 6 (only picks and drags) re-run | Python coordinate checks pass |
| Earlier keyboard/screens/focus/jobs scenarios re-run on the new chrome | same journals; captures updated |
| 100 000 objects, all selected, packaged app | frame p95 16.78 ms, pick p95 0.015 ms, open 0.45 s, peak RSS 345 MiB (`tests/ui/tekla-workflow/bench-100k.json`) |
| Snap query on 100 000 objects (release) | p95 2.9 ms (`work/tekla-audit-snap-bench.txt`), limit 5 ms |
| Deliverables vs baseline `b31b406` | 22/22 present |

The audit found these gaps and fixed them in round 1:
1. **Missing deliverable:** the ribbon lived in `chrome.rs`. It was extracted to `ui/ribbon.rs`.
2. **Evidence links:** documentation rows that cited only «native viewport» now point to captures in `tests/ui/navigation/`.
3. **Snap regression:** p95 was 91 ms (limit 5 ms). Deduplicating snap candidates was quadratic, and the intersection search ran over all lines. Both now stay near the cursor and run in linear time.
4. **Frame-time regression with a huge selection:** p95 was 33–47 ms. Handles were drawn for 2000 objects, and the selection was copied every frame. Handles are now capped at 500 objects and the selection is shared.

Coverage: 46 of 49 ROADMAP criteria were re-verified by tests, scripts or measurements. The other 3 visual capture criteria rely on the earlier verification. With the 22 deliverables included, coverage is 95.8 %.

## Method (steel workflow run, 2026-10-18)

The audit re-read the original `ROADMAP.md` criteria (not phase self-reports),
re-ran the mandatory commands on the final tree, re-ran every independent
verification script, re-measured the benchmark on the packaged app, and checked
deliverables against the repository baseline `8cbcc77` with `repo-state.sh`.

| Check | Result |
|---|---|
| `cargo build --workspace --locked` | exit 0, 0 warnings |
| `cargo test --workspace --locked` | exit 0, 108 passed, 1 ignored (subprocess crash harness, run by its parent test) |
| `cargo fmt --all --check` | exit 0 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | exit 0 |
| Independent scripts (Python csv/ElementTree; pypdf 5.1.0; ezdxf 1.3.5; IfcOpenShell 0.8.4) | 8/8 pass: drawings SVG, exports PDF/DXF, steel lists, coordination GUI, drawing GUI, release GUI, IFC, final packaged run |
| Benchmark 100 000 objects (packaged app) | frame p95 16.8 ms, pick p95 0.013 ms, peak RSS 344 MiB, open 0.28 s |
| Deliverables | 46 present; 4 missing = phases 15–16 (user-deferred) |
| Cleanliness (diff vs baseline) | 0 `dbg!`/TODO/FIXME; prints only CLI/diagnostics; vendored truck-shapeops tests unchanged upstream code |

## Criteria (original ROADMAP)

`re` = re-verified in this audit (tests, scripts, files, measurement);
`prior` = native GUI evidence from the phase run (captures and journals kept in
`tests/*_flows`, `tests/ui`), not re-driven during the audit.

| Phase | Criteria | Status | Evidence |
|---|---|---|---|
| 1 Contract | 5 | 5 re | Cargo.toml/lock, LICENSE, parity.csv, architecture.md, CI (GitHub runs success) |
| 2 Geometry | 5 | 5 re | `crates/geometry/tests/*` analytic corpus |
| 3 Model | 5 | 5 re | `crates/model/tests/domain.rs` |
| 4 Commands | 5 | 5 re | `crates/commands/tests/transactions.rs` |
| 5 Persistence | 5 | 5 re | `crates/persistence/tests/storage.rs` (crash harness ignored as standalone, run by parent) |
| 6 Viewport | 5 | 2 re, 3 prior | `crates/renderer/tests/viewport.rs`; `tests/viewport` captures |
| 7 Modeling | 5 | 3 re, 2 prior | `crates/desktop/tests/modeling.rs`; `tests/modeling_flows` |
| 8 Steel detailing | 5 | 4 re, 1 prior | geometry/steel tests; `tests/steel_flows` |
| 9 Connections | 5 | 4 re, 1 prior | connections tests; `tests/connections_flows` |
| 10 Clashes/numbering | 5 | 5 re | coordination/numbering/desktop tests, `tests/coordination_flows/verify.py` |
| 11 Drawing engine | 5 | 5 re | `crates/drawings/tests/engine.rs`, reference SVG, `verify.py` |
| 12 Drawing editor/export | 5 | 5 re | drawing_editor/exports tests, pypdf/ezdxf `verify.py` |
| 13 Steel lists | 5 | 5 re | `crates/reports/tests/release.rs`, `steel_release_flow/verify.py` |
| 14 IFC | 5 | 5 re | `crates/ifc/tests/roundtrip.rs`, IfcOpenShell `verify.py` |
| 15 Concrete | 5 | **not done** | deferred by user decision |
| 16 Rebar | 5 | **not done** | deferred by user decision |
| 17 UI | 5 | 4 re, 1 prior | `crates/desktop/tests/ui_shell.rs`; keyboard flow `tests/ui/keyboard` |
| 18 Polish & Harden | 6 | 6 re | this audit, `tests/final`, `tests/performance`, malformed corpus |

Totals for implemented phases: 81 criteria, 73 re-verified, 8 trust-prior
(audit coverage 90 %). 10 criteria of phases 15–16 are open.

## Known limits (not hidden)

* Live VoiceOver speech was not exercised: macOS accessibility automation is not
  permitted for scripts on this host and the setting was not changed; names were
  verified through the AccessKit tree the app publishes.
* 1920×1080-point screenshots use egui zoom 0.75 in a 1440×810 window (display is
  1470×956 points); the exact size is also checked headlessly.
* Benchmark frames are vsync-bound; detailed connection models (drilled bodies,
  drawing HLR) are slower and are measured separately in `docs/performance.md`.
* Profiles are analytic sharp-corner sections, not full regional catalogues.
* The app is unsigned and not notarized.

## Open parity rows

concrete, rebar, advanced-rebar,
pour-management, nc-dstv, ifc43, reference-models, dwg, native-tekla, catalogs,
collaboration, api, windows-linux. Each needs an executable scenario and
reference before it may be marked verified.
