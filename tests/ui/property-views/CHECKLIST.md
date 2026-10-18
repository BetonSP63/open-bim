# Property pane and views (phase 7) — native captures (`--example`, 1280×800)

| Capture | Shows |
|---|---|
| 01-contextual-toolbar.png | click on a beam: contextual toolbar next to the pointer (profile, material, «Свойства…»), property pane «Свойства: Балка (1)» |
| 02-property-pane-mixed.png | Ctrl+A: 7 parts, «— разные —» for the name and size, blue direct-modification handles |
| 03-views-list.png | Ctrl+I views pane after «Виды по линиям сетки»: 3D, План Основание, План Верх, Фасад по оси А/Б |
| 04-plan-top-depth.png | «План Верх»: orthographic top view, view depth ±1000 mm (columns cut away), work plane height 3000 kept for the view |
| 05-search-i300.png | Ctrl+P back to 3D/plan, model search «I 300» → «Найдено и выбрано: 6» |

Scripts: `script.json` (01–02) and `script-views.json` (03–05). Headless tests: `crates/desktop/tests/property_pane.rs`.
