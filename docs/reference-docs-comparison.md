# Open BIM vs reference documentation (2026)

Sources: 2026 user guides of a commercial steel detailing package (GEF «Get familiar», MOD «Create models»,
DRA «Create drawings», MGE, SHA, keyboard shortcuts flyer), read 2026-10-18.
Status: ✅ same logic · ⚠ partly / differs · ❌ missing.

## Interface (GEF 4)
| Reference | Open BIM | |
|---|---|---|
| 15 UI areas: model, global origin cube, work area, local coordinate symbol with clickable navigation axes, File menu, ribbon, Quick Access, user/help, Quick Launch, side pane, work plane handler, model search, snap switches, selection switches, status bar | all present; gizmo axes are clickable (plane views) | ✅ |
| Command runs until Esc / another command; Enter repeats last command; middle button/Enter/space completes | same | ✅ |
| Tooltip with hints, Ctrl+F1 for help | tooltips with label + shortcut | ⚠ |
| Quick Launch Ctrl+Q, arrows + Enter, Esc clears | field, Enter runs first; no Ctrl+Q, no arrows | ⚠ |
| Side pane: one window by default, active button blue | same | ✅ |
| Contextual toolbar next to the pointer on click (Ctrl+K) | contextual toolbar after a selecting click: profile, material, «Свойства…»; Ctrl+K toggles | ✅ |
| Property pane: Object type list starts a create command; double-click opens properties; Modify button | «Тип объекта» list starts the command; double click opens the pane; «Изменить» applies the edited fields in one transaction; «— разные —» | ✅ |

Final status after phases 5–8: the rows above reflect the implemented state;
everything still ⚠/❌ is listed in `docs/interaction.md` → «not implemented».

## Selecting (GEF 4.6, MOD 1.7)
| Reference | Open BIM | |
|---|---|---|
| Main switches: components / component objects / assemblies / objects in assemblies; type switches (parts, grids, grid lines, welds, bolt groups, single bolts, cuts…) | parts / whole assemblies only | ⚠ |
| Shift adds, Ctrl toggles, Alt+click selects assembly, area selection, Ctrl+A, F2/F3 switches | Shift adds, Ctrl toggles, Alt+click selects the assembly, window/crossing area selection, Ctrl+A; F2/F3 are mode keys here (deviation) | ⚠ |
| Handles: start yellow, others magenta (plates too) | same | ✅ |
| Direct modification (D) with blue handles | D switches it; blue handles for member ends and plate vertices, dragged with snapping | ✅ |

## Snapping (GEF 4.7, MOD 1.3)
| Reference | Open BIM | |
|---|---|---|
| Main switches Reference lines/points and Geometry lines/points (highest priority; both off = no snap) | both main switches («Опорные», «Геометрия») | ✅ |
| Points (incl. grid intersections), End, Centers, Mid, Intersections, Perpendicular, Parallel, Line extensions, Any position, Nearest, Lines | all of them: reference points, end, centre (bolts, circles), mid, intersection, perpendicular, parallel, line extension, line, nearest, free | ✅ |
| Snap zone in pixels, priority by switches, depth Plane/Auto/3D | pixel tolerance, priority; no depth setting | ⚠ |
| Orange snap symbol, snap tooltip, turquoise highlight of the snapped line | orange glyph, snap tooltip next to the cursor, turquoise highlight of the member axis or grid line | ✅ |
| Tab / Shift+Tab cycles snap candidates; right-click snap override; Shift locks direction; Ctrl+pick sets a temporary reference point; X/Y/Z lock | Tab/Shift+Tab cycle, Ctrl+pick temporary reference point (black cross), «Переопределение привязки» in the right-click menu, Shift direction lock, X/Y/Z coordinate locks | ✅ |
| Ortho (O); rubber band coloured by work plane axis | ortho; rubber band one colour | ⚠ |
| Numeric input on typing: **default relative**, `@` relative, `$` absolute (work plane), `!` global, polar `d<a<b`, one value = distance along the pointer direction, R/A/G keys | same modes and prefixes (fixed in phase 5); R/A/G letters (also К/Ф/П on the Russian layout) | ✅ |

## Navigation (MOD 1.2, shortcuts)
| Reference | Open BIM | |
|---|---|---|
| Wheel zoom about the pointer, PgUp/PgDn, Home zoom original, Shift+Space zoom selected | wheel about the pointer, PgUp/PgDn, Home, Shift+Space zoom selected | ✅ |
| Middle drag pans; Ctrl+middle rotates about the view point (V sets it); right button = context menu | same, plus a context menu; no V view point | ⚠ |
| Arrows move the view; Ctrl+arrows rotate 15°, Shift+arrows 5°; PgUp/PgDn; Home | same (fixed in phase 5) | ✅ |
| Ctrl+P plane/3D, Ctrl+1…4 rendering, Shift+X clip plane, F measure | Ctrl+P plane/3D, views by grid lines with depth, F measure; no Ctrl+1…4 rendering modes, no clip planes by pick | ⚠ |

## Modelling (MOD 1.1, 2.1, 2.2)
| Reference | Open BIM | |
|---|---|---|
| New model creates a rectangular grid + view from standard properties | phase 5: default grid 1–5 × А–Г | ✅ |
| Grid: X/Y relative («0 2*4000»), **Z absolute**, labels, extensions; pick an origin or middle click; lines at every level | dialog with X/Y relative and Z absolute; origin fixed at 0; lines at the lowest level, levels as labels | ⚠ |
| Column: one pick, **Top/Bottom levels** in properties (global z) | same; no binding to levels | ✅ |
| Beam: two picks, continuous; Position On plane / Rotation / At depth relative to the reference line | two picks; «Положение» (В плоскости / Поворот / По глубине with offsets, Dx at the ends) relative to the reference line; handles on the line | ✓ |
| Contour plate: polygon picks, middle click finishes | same | ✅ |
| Bolt «by parts and points»: pick main part, secondary parts, middle click, origin point, X-direction point; group on the work plane; cut length from parts; Array/Circle/List; holes through/blind/special | same pick sequence, length through all parts, array, circle and list shapes, through holes; no special holes, no «by face» | ⚠ |
| Weld between parts: main part, then secondary part; length from the contact | same pick order; path = nearest edge | ⚠ |
| Polygon weld, weld to part | «Сварной шов по ломаной» (main part, secondary part, points, middle click) and «Сварной шов на детали» (part, points, middle click) | ✅ |
| Shift+ribbon button opens properties first | same | ✅ |
| Twin profile: two members on one line, back to back / toes in / stacked, gap | «Сдвоенный профиль», mirrored second member, gap 0…1000 mm | ✅ |
| Plate corner chamfers: line, rounding | «Фаски углов»: straight cut by two legs, rounding by radius, written into the contour (not editable later as a chamfer) | ⚠ |
| Surface treatment on parts | treatment per part, area per treatment in the lists; not modelled as a layer on faces | ⚠ |
| Curved beam, polybeam, bent plate | not yet | ❌ |
| Inquire object, compare parts | «Запросить объект» (part, bolt, weld, component, assembly), «Сравнить детали» by the numbering data | ✅ |
| Object groups with AND/OR rules, object representation (colour, transparency, hidden), selection filter by group | same; groups and rows are kept for the session, not saved to the project or to files | ⚠ |
| Measure angle; clip plane by pick | «Измерить угол» (vertex and two sides); «Секущая плоскость по точке» facing the viewer, one plane | ⚠ |

## Drawings, management, sharing (DRA, MGE, SHA)
Reference drawings come from the model with Document manager, Master drawing catalog,
layouts and templates, and AutoDrawings. Open BIM has GA, part and assembly
drawings, revisions, and PDF/DXF export, with no Document manager, catalog or
templates. The reference package also has multi-user mode, model sharing, cloud sharing, many
formats and environments/firm folders. Open BIM has IFC4 import/export, PDF/DXF/CSV
and a single user.

## Concrete and rebar
Concrete and rebar tabs are shown disabled («Будет в следующем выпуске»).

Phase 5 fixed: numeric input modes, arrows and keys, context menu, grid Z,
column Top/Bottom, the bolt pick sequence, Shift+ribbon, handle colours, and snap
priority between grid nodes and reference points.

Conclusion (before phase 5): the core interaction logic matches the reference workflow: a command, picks with prompts
in the status bar, snap switches, the work plane, Esc, Enter to repeat, the middle
button, Quick Launch, the side pane and handles. The main differences to fix are
the default numeric input mode, the arrow keys, the right mouse button, Z
coordinates of the grid, column Top/Bottom, bolt part selection, and the missing
Tab cycling, temporary reference point, contextual toolbar, direct modification and
area selection.
