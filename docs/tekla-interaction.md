# Tekla-style interaction

Reference: the Tekla Structures 2026 user guides (GEF, MOD and the shortcuts
flyer) and the annotated screenshot of its main window. Open BIM reproduces how
Tekla works. It does not copy Tekla artwork and does not claim parity. Differences
are listed in `docs/tekla-docs-comparison.md`.

## Mouse and keyboard

| Tekla | Open BIM | Evidence |
|---|---|---|
| Wheel zooms about the pointer; Page Up/Down zoom; Home zooms to the original view | wheel zooms about the cursor (`Camera::zoom_at`); PgUp/PgDn; Home fits the model and the grid | `tests/interaction.rs` wheel_zoom_keeps_the_point_under_the_cursor |
| Middle drag pans | same | native viewport |
| Ctrl + middle drag rotates | Ctrl/⌘ + middle drag rotates | native viewport |
| Arrows move the view; Ctrl+arrows rotate 15°, Shift+arrows rotate 5° | same | native viewport |
| Right button opens the context menu (Interrupt…) | context menu: Прервать, Завершить/повторить, Свойства, Вписать, Скрыть, Показать все | native viewport |
| Middle click / Enter finishes a polygon or a part selection | same | escape_leaves_model_unchanged_enter_finishes_polygon_and_repeats |
| Esc interrupts the command | Esc drops the picks; the model is unchanged | same test |
| Enter in idle repeats the last command | same | same test |
| A command runs until Esc or another command | beam, column, plate and weld commands re-arm after each object | beam_by_two_snapped_grid_picks_is_exact_and_each_beam_is_one_undo_step |
| Status bar prompt | a Russian prompt for each pick step | every_tool_has_russian_prompts_per_step |
| Shift + click on a ribbon tool opens its properties | same (part properties; bolt/weld → «Болты, отверстия и швы») | ribbon test |
| Numeric location input; the default mode is **relative** | typing a digit, `@ $ ! -` opens «Ввод координат». `x,y,z` is relative to the last pick, or from the work plane origin when there is none. `@` = relative, `$` = absolute in the work plane, `!` = global, `d<a<b` = polar, a single value `d` = distance along the pointer direction | numeric_location_forms_and_errors |
| Work plane | picks lie on the work plane (default XY, elevation field, plane by 3 points) | tilted_work_plane_projects_rays_exactly |

## Creating objects (MOD 2.1–2.2)

| Tekla | Open BIM | Evidence |
|---|---|---|
| A new model comes with a rectangular grid | grid 1–5 × А–Г at 6000 mm, levels 0.000 and +3.600 | new_model_has_the_default_grid |
| Grid: X/Y relative («0 4*6000»), Z absolute, labels, line extension | ribbon «Сетка» dialog, one transaction; labels continue (А→Б, 1→2, letters per ГОСТ 21.101) | grid_coordinate_strings_like_tekla, grid_labels_continue_and_map_to_lines |
| Column: one pick, Top/Bottom as global Z | «Низ»/«Верх» in the properties | column_uses_bottom_and_top_and_plate_contour_equals_picks |
| Beam: two picks; contour plate: polygon + middle click | same, one undo step per object; the first pick is the yellow start handle | same tests |
| Bolt «by parts and points»: main part, secondary parts, middle click, origin, X direction | same; the bolt axis is the work plane normal, holes go through all selected parts | bolt_by_parts_and_points_drills_both_plates_as_allowed_holes |
| Weld between parts: the part to weld to, then the part to be welded | same; the weld path is the nearest edge of the secondary part | weld_between_main_and_secondary_part |
| Snap priority: highest class wins, then the nearest point | reference points and grid intersections share the highest class | nearer_grid_intersection_beats_a_reference_point_a_few_pixels_away |

The native run `tests/ui/create-by-picks/` uses only mouse clicks on snapped grid
nodes and column tops, plus Enter, Esc and ⌘S. It builds:
- 4 columns at the nodes В4, В5, Г4, Г5;
- 4 beams on the column tops;
- a plate on А1–А3–В3–В1;
- a bolt group at Б2 pointing towards Б3.

`check_model.py` checks the saved `.obim` against the grid values independently
(`check.txt`). Separate coordinates with commas; use a dot for decimals.
