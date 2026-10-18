# Performance (phase 18)

Host: MacBook, Apple M5, Metal (wgpu), macOS ARM64, display 2560×1664 px
(1470×956 points). Build: `scripts/package-macos.sh` release (LTO thin,
codegen-units 1); measurements run on the packaged app.

## Synthetic corpus — 100 000 simple steel objects

`open-bim-synthetic 100000 synthetic-100k.obim`: columns and beams on a storey
grid (6 m bays, 3.6 m storeys), four catalogue sections (I, L, C, CHS 114×6),
three beam lengths; no connections or features. File 73.8 MB.

Method (`--bench`, `crates/desktop/src/bench.rs`): window 1920×1080 points
(1440×810 window with egui zoom 0.75 because the display is smaller; framebuffer
2880×1620 px, i.e. more pixels than 1920×1080), camera fitted then orbiting
0.6 px per frame so every frame redraws the scene, 120 warm-up frames, 600
measured frame intervals (vsync on, 60 Hz), then 200 ray picks across the viewport.
Peak memory: `/usr/bin/time -l` maximum resident set size.

| Run | Objects | Open (load + scene) | Frame p50 / p95 / max | Pick p95 | Peak RSS |
|---|---|---|---|---|---|
| synthetic-100k-run1 | 100 000 | 0.26 s | 16.7 / 17.25 / 17.9 ms | 0.037 ms | 344 MiB |
| synthetic-100k-run2 | 100 000 | 0.26 s | 16.7 / 17.27 / — ms | 0.031 ms | 344 MiB |
| connections-16 (complex) | 16 drilled parts, 6 nodes | 0.64 s | — / 17.1 ms | 0.001 ms | 137 MiB |

Limits: frame p95 ≤ 33.3 ms, pick p95 ≤ 100 ms, memory ≤ 4 GB, open ≤ 30 s — met.
Frame times are vsync-bound (16.7 ms); the GPU is not saturated.

Fixes made to reach this (profiled with macOS `sample`):
1. The model tree laid out all 100 000 rows every frame (768 ms/frame) →
   virtualised `ScrollArea::show_rows` with cached labels (50 ms).
2. CHS display meshes had 128 segments (~1 000 triangles × 25 000 tubes) →
   screen-size LOD: 128/32/12 segments by projected diameter (≥48 / ≥10 px)
   (17 ms, vsync-bound). Exact geometry is unchanged; only display meshes vary.
3. Scene build: parts with features are found once (set) instead of a scan per part.

## Complex model (not covered by the synthetic benchmark)

The synthetic result does not represent detailed connection models. On the
phase-9 connection model (16 parts, 52 details): open 0.64 s; clash check ≈1.6 s
(background job); release of 14 drawings + lists ≈10 s (background job with
progress and cancel). Drilled-body meshing and drawing HLR dominate; they are
the next optimisation targets. Raw data: `tests/performance/*.json|*.log`.

## Responsiveness

Opening, clash check and release run as background jobs (`ui/jobs.rs`): the UI
keeps rendering, shows progress and «Отменить · Esc»; the model changes only when a
job finishes (`docs/ui-verification.md` §4).
