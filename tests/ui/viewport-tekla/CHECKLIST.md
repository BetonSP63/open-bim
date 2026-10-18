# Viewport presentation (phase 4)

Overlay geometry: `crates/renderer/src/overlay.rs` (screen-space marks, unit tested);
painting: `crates/desktop/src/viewport.rs`; the scene pass clears transparent and the
composite blends over an egui gradient. Native captures (real app, Metal, `--script`):

| Cue | Capture | Notes |
|---|---|---|
| Grey-blue vertical gradient (light theme) | overview-1280x800.png | dark theme uses a dark gradient |
| Model/work plane origin symbol X/Y/Z | overview-1280x800.png | follows the work plane (3-point plane moves it) |
| Axis gizmo bottom-right, rotates with camera | overview-1280x800.png | test `gizmo_matches_the_camera_matrix_in_named_views` |
| Work area box, green dashed, toggle Вид → «Рабочая область» | overview-1280x800.png | margin 500 mm, test `work_area_is_bounds_plus_margin` |
| Grid dotted with bubble labels at both ends, level labels | overview-1280x800.png | «Основание +0», «Верх +3000» |
| Selection handles: start yellow, end magenta, plate vertices | overview-1280x800.png | test `handles_sit_on_member_ends_and_plate_vertices` (≤0.5 px) |
| Rubber band + live length during beam pick | beam-pick-1280x800.png | «4786 мм», status «Укажите конечную точку балки» |
| Snap symbol per kind | end-snap-1280x800.png | reference glyph at beam end, 6373 мм; distinct glyphs test |

Benchmark `bench-100k-overlay.json`: 100 000 objects, all selected (handles bounded to
2000 objects), 1920×1080-pt layout, 600 frames: frame p50 16.66 / p95 16.97 ms (vsync),
pick p95 0.015 ms, peak RSS 210 MiB. Limits 33.3 ms / 100 ms — met.
