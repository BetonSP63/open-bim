# Tekla-style interaction

Reference: Tekla Structures 2025 public documentation and the annotated
screenshot of its main window. Open BIM reproduces the working mechanics; it does
not copy Tekla artwork or claim parity.

## Mouse and keyboard

| Tekla | Open BIM | Evidence |
|---|---|---|
| Wheel — zoom about the cursor | Wheel zooms keeping the point under the cursor fixed (`Camera::zoom_at`) | `tests/interaction.rs` wheel_zoom_keeps_the_point_under_the_cursor |
| Middle drag — pan | Middle drag pans | native viewport |
| Ctrl + middle drag — rotate | Ctrl/Shift/⌘ + middle drag rotates (right drag also rotates for trackpads) | native viewport |
| Middle click / Enter — finish polygon | Middle click or Enter finishes plates/contours | escape_leaves_model_unchanged_enter_finishes_polygon_and_repeats |
| Esc — interrupt command | Esc drops picks, model unchanged | same test |
| Enter in idle — repeat last command | Enter repeats the last creation command | same test |
| Commands repeat until Esc | Beam/column/plate commands re-arm after each object | beam_needs_two_picks_creates_once_and_rearms |
| Status bar prompt «Укажите начальную точку балки» | Russian prompt per pick step in the status bar | every_tool_has_russian_prompts_per_step |
| Numeric location input | Typing a digit, «@» or «-» opens «Ввод координат»: `x,y,z` absolute (work plane), `@dx,dy,dz` relative, `d` along the rubber band | numeric_location_forms_and_errors |
| Work plane | Picks are projected to the work plane (default XY, elevation field, 3 points) | tilted_work_plane_projects_rays_exactly |

Coordinates are separated by commas; decimals use a dot.
