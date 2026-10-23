# Ribbon chrome — 15 areas of the reference screenshot

Native captures (real app, `--script`, Metal): `chrome-1280x800.png` (window
1280×800 pt), `chrome-1920x1080.png` and `edit-tab-1920x1080.png` (1920×1080-pt
layout: 1440×810 window with `--zoom 0.75`, the host display is 1470×956 pt),
`file-menu-1280x800.png`, `quick-launch-1280x800.png`, `beam-prompt-1280x800.png`,
`steel-tooltip-1280x800.png`, `concrete-disabled-1280x800.png`.
Scripts: `script-1280.json`, `script-1920.json`, `script-tooltip.json`; logs `run-*.log`.
Icons are original vector drawings (`ui/icons.rs`), no third-party artwork.

| # | Area (reference) | Open BIM | Evidence |
|---|---|---|---|
| 1 | Work plane cross | Not in this phase: viewport overlay is phase 4 (`renderer/src/overlay.rs`). Work plane itself exists (Правка → «По 3 точкам», «Глобальная XY», toolbar «Система координат»). | phase 4 |
| 2 | Origin symbol | Phase 4 viewport overlay. | phase 4 |
| 3 | Work area box | Phase 4 viewport overlay; «Вписать» (F) fits the model now. | phase 4 |
| 4 | Axis gizmo | Phase 4 viewport overlay. | phase 4 |
| 5 | File menu | «Файл» menu: new, open, save, control frame, IFC export/import, release set. | file-menu-1280x800.png |
| 6 | Selection/snap switch group | Bottom toolbars: selection switches + snap switches (icons, toggles). | chrome-*.png |
| 7 | Title + quick access | «OB» badge, Save/Undo/Redo icons, «Open BIM — project (file)». | chrome-*.png |
| 8 | Help/profile | «?» help (F1, all shortcuts). No user profile: single-user desktop app (n/a). | chrome-*.png |
| 9 | Quick launch | «Быстрый запуск» field, Russian search over the command registry, Enter runs first match. | quick-launch-1280x800.png, beam-prompt-1280x800.png, test `quick_launch_runs_the_first_match_on_enter` |
| 10 | Right side pane strip | Icon strip: Свойства, Правка, Болты/швы, Узлы, Нумерация и коллизии, Дерево модели, Чертежи, Ведомости. | chrome-*.png |
| 11 | Coordinate system selector | «Начало координат модели» / «Рабочая плоскость по 3 точкам…». | chrome-*.png |
| 12 | Model search | «Поиск в модели»: name/profile/material/#ID, selects matches. | test `model_search_selects_matching_parts` |
| 13 | Snap switches toolbar | Опорная, конечная, середина, пересечение, перпендикуляр, ближайшая, ортогонально (O), допуск px. | chrome-*.png |
| 14 | Selection switches toolbar | Детали / сборки целиком, «Добавлять к выбору». | test `selection_switches_pick_parts_assemblies_or_nothing` |
| 15 | Blue status bar prompt | Blue bar: current pick prompt («Укажите начальную точку балки»), snap kind, selection count. | beam-prompt-1280x800.png |

Ribbon tabs: МЕТАЛЛ, БЕТОН, АРМАТУРА, ПРАВКА, ВИД, ЧЕРТЕЖИ И ОТЧЁТЫ, УПРАВЛЕНИЕ,
РАСЧЁТ И ПРОЕКТИРОВАНИЕ, ОПАЛУБКА. Groups and buttons follow the reference ribbon;
tools not built yet are shown disabled with the tooltip «Будет в следующем выпуске»
(concrete, rebar, analysis and formwork tabs entirely). Long captions wrap onto two
lines. When the window is too narrow the rightmost groups fold into one button that
opens a list of their tools (`ribbon::collapse`). Every enabled ribbon button is
clicked by mouse at 1280×800, opening its folded group first when needed, in
`every_ribbon_button_runs_its_registry_command_by_mouse` (`ribbon-commands.json`);
light-theme contrast in `contrast-light.json`.

Defects found and fixed while capturing: ribbon groups stacked vertically and ran
past 1280 pt (centred group captions took full width); selected disabled tab text was
grey on blue; quick launch lost keyboard focus in scripted runs (window not key) —
the script now keeps egui focus; theme pinned for both egui theme slots.
