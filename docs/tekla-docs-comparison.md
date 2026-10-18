# Open BIM vs Tekla Structures 2026 documentation

Sources: Tekla Structures 2026 user guides (GEF «Get familiar», MOD «Create models»,
DRA «Create drawings», MGE, SHA, keyboard shortcuts flyer), read 2026-10-18.
Status: ✅ same logic · ⚠ partly / differs · ❌ missing.

## Interface (GEF 4)
| Tekla | Open BIM | |
|---|---|---|
| 15 UI areas: model, global origin cube, work area, local coordinate symbol with clickable navigation axes, File menu, ribbon, Quick Access, user/help, Quick Launch, side pane, work plane handler, model search, snap switches, selection switches, status bar | all present (phases 3–4); gizmo is not clickable yet | ⚠ |
| Command runs until Esc / another command; Enter repeats last command; middle button/Enter/space completes | same | ✅ |
| Tooltip with hints, Ctrl+F1 for help | tooltips with label + shortcut | ⚠ |
| Quick Launch Ctrl+Q, arrows + Enter, Esc clears | field, Enter runs first; no Ctrl+Q, no arrows | ⚠ |
| Side pane: one window by default, active button blue | same | ✅ |
| Contextual toolbar next to the pointer on click (Ctrl+K) | — | ❌ |
| Property pane: Object type list starts a create command; double-click opens properties; Modify button | properties panel; no object-type start, no double-click | ⚠ |

## Selecting (GEF 4.6, MOD 1.7)
| Tekla | Open BIM | |
|---|---|---|
| Main switches: components / component objects / assemblies / objects in assemblies; type switches (parts, grids, grid lines, welds, bolt groups, single bolts, cuts…) | parts / whole assemblies only | ⚠ |
| Shift adds, Ctrl toggles, Alt+click selects assembly, area selection, Ctrl+A, F2/F3 switches | Shift/⌘ add; no toggle, no area selection, no Alt | ⚠ |
| Handles: start yellow, others magenta (plates too) | same | ✅ |
| Direct modification (D) with blue handles | — (phase 6) | ❌ |

## Snapping (GEF 4.7, MOD 1.3)
| Tekla | Open BIM | |
|---|---|---|
| Main switches Reference lines/points and Geometry lines/points (highest priority; both off = no snap) | reference only, geometry edges always on | ⚠ |
| Points (incl. grid intersections), End, Centers, Mid, Intersections, Perpendicular, Parallel, Line extensions, Any position, Nearest, Lines | end, mid, intersection, perpendicular, nearest, free | ⚠ (no centers, parallel, extensions, lines) |
| Snap zone in pixels, priority by switches, depth Plane/Auto/3D | pixel tolerance, priority; no depth setting | ⚠ |
| Orange snap symbol, snap tooltip, turquoise highlight of the snapped line | orange glyph, kind in status bar; no tooltip, no line highlight | ⚠ |
| Tab / Shift+Tab cycles snap candidates; right-click snap override; Shift locks direction; Ctrl+pick sets a temporary reference point; X/Y/Z lock | — | ❌ |
| Ortho (O); rubber band coloured by work plane axis | ortho; rubber band one colour | ⚠ |
| Numeric input on typing: **default relative**, `@` relative, `$` absolute (work plane), `!` global, polar `d<a<b`, one value = distance along the pointer direction, R/A/G keys | same modes and prefixes (fixed in phase 5); no R/A/G keys | ✅ |

## Navigation (MOD 1.2, shortcuts)
| Tekla | Open BIM | |
|---|---|---|
| Wheel zoom about the pointer, PgUp/PgDn, Home zoom original, Shift+Space zoom selected | wheel about pointer, +/− | ⚠ |
| Middle drag pans; Ctrl+middle rotates about the view point (V sets it); right button = context menu | same, plus a context menu; no V view point | ⚠ |
| Arrows move the view; Ctrl+arrows rotate 15°, Shift+arrows 5°; PgUp/PgDn; Home | same (fixed in phase 5) | ✅ |
| Ctrl+P plane/3D, Ctrl+1…4 rendering, Shift+X clip plane, F measure | P projection; section height; no measure | ⚠ |

## Modelling (MOD 1.1, 2.1, 2.2)
| Tekla | Open BIM | |
|---|---|---|
| New model creates a rectangular grid + view from standard properties | phase 5: default grid 1–5 × А–Г | ✅ |
| Grid: X/Y relative («0 2*4000»), **Z absolute**, labels, extensions; pick an origin or middle click; lines at every level | dialog with X/Y relative and Z absolute; origin fixed at 0; lines at the lowest level, levels as labels | ⚠ |
| Column: one pick, **Top/Bottom levels** in properties (global z) | same; no binding to levels | ✅ |
| Beam: two picks, continuous; Position On plane / Rotation / At depth relative to the reference line | two picks, profile centred on the line; no position settings | ⚠ |
| Contour plate: polygon picks, middle click finishes | same | ✅ |
| Bolt «by parts and points»: pick main part, secondary parts, middle click, origin point, X-direction point; group on the work plane; cut length from parts; Array/Circle/List; holes through/blind/special | same pick sequence, length through all parts, array, through holes; no circle/list, no special holes, no «by face» | ⚠ |
| Weld between parts: main part, then secondary part; length from the contact | same pick order; path = nearest edge | ⚠ |
| Polygon weld, weld to part | — | ❌ |
| Shift+ribbon button opens properties first | same | ✅ |

## Drawings, management, sharing (DRA, MGE, SHA)
Tekla drawings come from the model with Document manager, Master drawing catalog,
layouts and templates, and AutoDrawings. Open BIM has GA, part and assembly
drawings, revisions, and PDF/DXF export, with no Document manager, catalog or
templates. Tekla also has multi-user mode, model sharing, Trimble Connect, many
formats and environments/firm folders. Open BIM has IFC4 import/export, PDF/DXF/CSV
and a single user.

## Concrete and rebar
Concrete and rebar tabs are shown disabled («Будет в следующем выпуске»).

Phase 5 fixed: numeric input modes, arrows and keys, context menu, grid Z,
column Top/Bottom, the bolt pick sequence, Shift+ribbon, handle colours, and snap
priority between grid nodes and reference points.

Conclusion (before phase 5): the core interaction logic matches Tekla: a command, picks with prompts
in the status bar, snap switches, the work plane, Esc, Enter to repeat, the middle
button, Quick Launch, the side pane and handles. The main differences to fix are
the default numeric input mode, the arrow keys, the right mouse button, Z
coordinates of the grid, column Top/Bottom, bolt part selection, and the missing
Tab cycling, temporary reference point, contextual toolbar, direct modification and
area selection.
