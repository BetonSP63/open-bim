# Interaction

Reference: the 2026 user guides of a commercial steel detailing package (GEF, MOD and the shortcuts
flyer) and the annotated screenshot of its main window. Open BIM follows the
standard detailing workflow. It does not copy third-party artwork and does not claim parity. Differences
are listed in `docs/reference-docs-comparison.md`.

## Mouse and keyboard

| Reference workflow | Open BIM | Evidence |
|---|---|---|
| Wheel zooms about the pointer; Page Up/Down zoom; Home zooms to the original view | wheel zooms about the cursor (`Camera::zoom_at`); PgUp/PgDn; Home fits the model and the grid (F is now measure) | wheel_zoom_keeps_the_point_under_the_cursor; `tests/ui/navigation/05-pageup-zoom.png`, `07-home-fit.png` |
| Middle drag pans | same | `tests/ui/navigation/06-middle-drag-pan.png` |
| Ctrl + middle drag rotates | Ctrl/⌘ + middle drag rotates (same orbit as Ctrl+arrows) | `tests/ui/navigation/03-ctrl-arrow-rotate-15.png` |
| Arrows move the view; Ctrl+arrows rotate 15°, Shift+arrows rotate 5° | same | `tests/ui/navigation/02…04-*.png` |
| Right button opens the context menu (Interrupt…) | context menu: Прервать, Завершить/повторить, Свойства, Переместить, Копировать, Удалить, Скрыть, Показать только выбранные, Показать все, Вписать выбранные, Чертёж детали | `tests/ui/navigation/08-context-menu.png`, context_menu_commands_all_act |
| Middle click / Enter finishes a polygon or a part selection | same | escape_leaves_model_unchanged_enter_finishes_polygon_and_repeats |
| Esc interrupts the command | Esc drops the picks; the model is unchanged | escape_leaves_model_unchanged_enter_finishes_polygon_and_repeats |
| Enter in idle repeats the last command | same | escape_leaves_model_unchanged_enter_finishes_polygon_and_repeats |
| A command runs until Esc or another command | beam, column, plate and weld commands re-arm after each object | beam_by_two_snapped_grid_picks_is_exact_and_each_beam_is_one_undo_step |
| Status bar prompt | a Russian prompt for each pick step | every_tool_has_russian_prompts_per_step |
| Shift + click on a ribbon tool opens its properties | same (part properties; bolt/weld → «Болты, отверстия и швы») | `crates/desktop/src/ui/ribbon.rs`; every_ribbon_button_runs_its_registry_command_by_mouse |
| Numeric location input; the default mode is **relative** | typing a digit, `@ $ ! -` opens «Ввод координат». `x,y,z` is relative to the last pick, or from the work plane origin when there is none. `@` = relative, `$` = absolute in the work plane, `!` = global, `d<a<b` = polar, a single value `d` = distance along the pointer direction | numeric_location_forms_and_errors |
| Work plane | picks lie on the work plane (default XY, elevation field, plane by 3 points) | tilted_work_plane_projects_rays_exactly |

## Creating objects (MOD 2.1–2.2)

| Reference workflow | Open BIM | Evidence |
|---|---|---|
| A new model comes with a rectangular grid | grid 1–5 × А–Г at 6000 mm, levels 0.000 and +3.600 | new_model_has_the_default_grid |
| Grid: X/Y relative («0 4*6000»), Z absolute, labels, line extension | ribbon «Сетка» dialog, one transaction; labels continue (А→Б, 1→2, letters per ГОСТ 21.101) | grid_coordinate_strings_relative_and_absolute, grid_labels_continue_and_map_to_lines |
| Column: one pick, Top/Bottom as global Z | «Низ»/«Верх» in the properties | column_uses_bottom_and_top_and_plate_contour_equals_picks |
| Beam: two picks; contour plate: polygon + middle click | same, one undo step per object; the first pick is the yellow start handle | column_uses_bottom_and_top_and_plate_contour_equals_picks |
| Bolt «by parts and points»: main part, secondary parts, middle click, origin, X direction | same; the bolt axis is the work plane normal, holes go through all selected parts | bolt_by_parts_and_points_drills_both_plates_as_allowed_holes |
| Weld between parts: the part to weld to, then the part to be welded | same; the weld path is the nearest edge of the secondary part | weld_between_main_and_secondary_part |
| Snap priority: highest class wins, then the nearest point | reference points and grid intersections share the highest class | nearer_grid_intersection_beats_a_reference_point_a_few_pixels_away |
| Part position: On plane, Rotation, At depth with offsets, end offsets Dx; handles on the reference line; changing the profile keeps the line | «Положение» in the create dialog and the property pane: «В плоскости» (Посередине/Слева/Справа), «Поворот» (Спереди/Сверху/Сзади/Снизу + угол), «По глубине» (Посередине/Спереди/Сзади), offsets in mm, Dx at both ends. «Слева» and «Спереди» are +x and +y of the member's local axes (for a beam: left looking from start to end, and up). The default keeps the old placement (centred) | crates/model/src/position.rs tests; crates/desktop/tests/part_position.rs |

## Selection and editing (MOD 1.6–1.8)

| Reference workflow | Open BIM | Evidence |
|---|---|---|
| Click selects; Shift adds; Ctrl toggles; Alt+click selects the assembly; Esc clears | same (⌘ also adds) | window_selects_inside_crossing_selects_touching_and_switches_filter |
| Area: left→right = objects fully inside, right→left = touching (dashed box) | same | window_selects_inside_crossing_selects_touching_and_switches_filter, `tests/ui/selection-editing/02-window-select.png` |
| Selection switches: parts, assemblies, bolt groups, welds, grid lines | toolbar toggles; Ctrl+A selects all allowed | window_selects_inside_crossing_selects_touching_and_switches_filter, `tests/ui/selection-editing/02-window-select.png` |
| Direct modification (D): blue handles; drag a handle with snapping | member ends and plate vertices; one undo step | dragging_a_beam_end_handle_to_a_snapped_node_moves_only_that_end, plate_vertex_handle_moves_the_vertex |
| Move Ctrl+M / Copy Ctrl+C by origin + destination; copy repeats | same; mirror by 2 points, rotate by centre + 2 directions | move_copy_mirror_and_rotate_by_picks |
| Delete asks before removing dependants | confirmation dialog when holes/bolts/welds/nodes depend | delete_with_dependants_asks_first |
| Right-click menu | Прервать, Завершить, Свойства, Переместить, Копировать, Удалить, Скрыть, Показать только выбранные, Показать все, Вписать выбранные (⇧Пробел), Чертёж детали | context_menu_commands_all_act |
| Tab / Shift+Tab cycle snap points; Ctrl+pick sets a temporary reference point (black cross); snap tooltip; turquoise line of the snapped object | same | tab_cycles_snap_candidates_and_ctrl_pick_sets_a_reference |

## Properties and views (GEF 5, MOD 1.1)

| Reference workflow | Open BIM | Evidence |
|---|---|---|
| Property pane: common properties of the selection, «Varies», «Modify» applies the edited fields | «— разные —»; «Изменить» = one transaction for all selected; «Получить» rereads | mixed_profiles_show_varies_and_modify_is_one_undoable_transaction |
| Nothing selected: object type list starts a create command | «Тип объекта» list (beam, column, plate, bolt, weld, axis, grid) | `tests/ui/modeling-workflow/12-reopened.png` |
| Double click opens properties; contextual toolbar next to the pointer (Ctrl+K) | same; the contextual toolbar changes profile and material immediately | `tests/ui/property-views/01-…` |
| Class colours | classes 1–14 with an approximate industry-standard palette | `tests/ui/property-views/01-…` |
| Create views along grid lines: a plan per level, elevations per grid line, view depth | same (plan ±1000 mm, elevation ±500 mm); work plane kept per view; Ctrl+I views pane, Ctrl+P 3D/plane | grid_views_have_plan_per_level_and_elevation_per_grid_line, switching_views_keeps_the_work_plane_per_view |
| Model search toolbar | name / profile / material / #ID | search_selects_exactly_the_beams_with_the_profile |
| Measure (F); Home zooms to the original view | F = measure with ΔX/ΔY/ΔZ; Home = fit | measure_two_points_reports_distance_and_components |
| Navigation axes clickable | click X/Y/Z on the gizmo → right / front / top view | gizmo_matches_the_camera_matrix_in_named_views (axis tips) |
| Main snap switches «Reference» and «Geometry»; grid at every level | both switches; grid nodes snap at every level | geometry_switch_and_grid_nodes_at_every_level |

The native run `tests/ui/create-by-picks/` uses only mouse clicks on snapped grid
nodes and column tops, plus Enter, Esc and ⌘S. It builds:
- 4 columns at the nodes В4, В5, Г4, Г5;
- 4 beams on the column tops;
- a plate on А1–А3–В3–В1;
- a bolt group at Б2 pointing towards Б3.

`check_model.py` checks the saved `.obim` against the grid values independently
(`check.txt`). Separate coordinates with commas; use a dot for decimals.

## Reference mechanics not implemented (open)

These come from the 2026 reference manuals and are not in Open BIM yet. None of them
is claimed. See `docs/reference-docs-comparison.md` for each row's status.

- **Snapping:**
  - snap switches centres, parallel, line extensions and lines;
  - snap depth Plane/Auto/3D and the plane type list;
  - right-click snap override;
  - Shift direction lock;
  - X/Y/Z coordinate lock;
  - R/A/G keys for the numeric input mode.
- **Navigation:** view rotation point (V), rotate continuously (Shift+T), fly (Shift+F), Ctrl+R mouse rotate, F8.
- **Rendering:** wireframe/shaded/grayscale/rendered modes (Ctrl+1…4, Shift+1…4), «show only selected» rendering modes.
- **Columns:** binding to levels.
- **Grids:** radial grids, picking the grid origin, editing a single grid line, grid lines drawn at every level as objects. Today they are drawn and snapped but stored once.
- **Bolts:** «by face» and «by parts and face»; circle/list shapes; slotted, oversized, tapped and blind holes; Auto bolt.
- **Welds:** polygon welds, weld to a part, weld preparation, weld marks in drawings.
- **Selection:**
  - selection filters (Ctrl+G);
  - Alt+P select previous;
  - rollover highlight (H);
  - component and component-object switches;
  - single-bolt, cuts, points and planes switches.
- **Editing:**
  - clipboard copy/paste between models;
  - Smart select;
  - drag & drop;
  - copy special (linear, rotate, mirror, to another plane);
  - swap handles.
- **Property pane:** saving and loading property files, UDA dialogs, copying properties from another object.
- **Other reference features:**
  - Applications & components catalog;
  - custom components;
  - Organizer;
  - phase manager;
  - Document manager;
  - Master drawing catalog;
  - multi-user mode;
  - cloud model sharing.
- **Not part of this release:** concrete and rebar (their ribbon tabs are visible but disabled).

Shortcut deviations (macOS):
- ⌘ replaces Ctrl for the application commands: ⌘S, ⌘Z, ⌘N, ⌘O, ⌘P palette.
- The usual detailing Ctrl commands use the Control key: Ctrl+M, Ctrl+C, Ctrl+A, Ctrl+P, Ctrl+I, Ctrl+K.
- F2/F3/F4 switch the model, drawings and lists modes here. In the reference workflow they are selection and snap switches; here those are toolbar buttons.
- Redo is ⇧⌘Z instead of Ctrl+Y.
