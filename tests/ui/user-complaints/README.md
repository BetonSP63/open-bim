# User complaints — final re-check (phase 14)

All captures come from the real app built from this tree, driven by `--script`.
- `script-new.json`: a new project with the default grid, run with `--record journal.json`.
- `script-example.json`: the control steel frame, run with `--example`.

Run both from this directory:

    ../../../target/release/open-bim-desktop --script "$PWD/script-new.json" --record "$PWD/journal.json"
    ../../../target/release/open-bim-desktop --example --script "$PWD/script-example.json"

## First list (run C request)

| Complaint | Capture | Tests / independent checks |
|---|---|---|
| «при приближении модель пропадает» | 01-zoom-in-kept.png; ../viewport-depth/after-zoom-00…40.png | renderer depth tests (`crates/renderer/tests/depth_shading.rs`, `viewport.rs`); metrics.txt (64–90 % model pixels after 40 steps, 1 % before) |
| «странно выглядит элемент, как будто рябит» | 06-no-ripple-closeup.png | metrics.txt (ripple 0.02 %, user report 2.56 %); reversed-Z clip in `crates/renderer` |
| «нельзя выделить оси» | 02-axis-selected.png (axis 2, its pane and handles); ../grid-selection/02-crossing-axes.png | `crates/desktop/tests/grid_selection.rs` |
| «пластины рисуются плохо, не понятно как их создавать» | 03-plate-preview.png (numbered vertices, side lengths, filled preview, hint) | `crates/desktop/tests/plate_creation.rs` |
| «при пересечении границы пластины ничего не происходит» | 04-plate-crossing-refused.png (status: «Сторона 3–4 пересекает сторону 1–2: точка не принята»); 05-plate-created.png (concave plate) | same test; ../plate-creation/journal.json |
| «управлять без мышки нереально» | ../keyboard-only/01…07 (script has 0 pointer steps) | ../keyboard-only/check.txt (KEYBOARD_ONLY_OK); `crates/desktop/tests/keyboard_only.rs` |
| «реализовать все требования ГОСТ» | ../../gost/conventions/native, ../../gost/specifications/native, ../../gost/release/native, ../../gost/title-blocks/native | `docs/gost/gost-r-21.101-2026.md` (276 clauses, `check_matrix.py --final` → docs/gost/matrix-check.txt); tests/gost/*/check.txt |

## Second list (16 remarks)

| Remark | Evidence |
|---|---|
| Нет размеров | ../construction-trim (model dimensions); ../drawing-editing (dimension by clicks) |
| Сетку нельзя редактировать | ../edit-members-views/08-grid-double-click.png; ../grid-selection |
| Уровни нельзя выбрать | ../edit-members-views/07-level-selected.png |
| Логика болтов; создание болтов | ../bolts-welds-assemblies; `docs/help/bolts-welds.md` |
| Чертежи нельзя редактировать | ../drawing-editing/native (DRAWING_EDITING_OK) |
| Пластина, косынка, ребро — разница | ../bolts-welds-assemblies; F1 help «Стальные детали» |
| Нет сборок на ленте | ../bolts-welds-assemblies (ribbon «Сборки и узлы») |
| Сварка, выбор типа шва | ../bolts-welds-assemblies (types, sides, around; ГОСТ 5264-80) |
| Центр вращения в курсоре; трекпад | ../edit-members-views/06-trackpad-orbit-pivot.png |
| Линии в модели; подрезка по типовому рабочему процессу | ../construction-trim (IFC_VOLUMES_OK) |
| Длина детали, начало/конец/оба | ../edit-members-views/01–02 |
| Режимы отображения, прозрачность | ../edit-members-views/03–05 |
| Маскировка, штриховка, линии, размеры на чертеже | ../drawing-editing/native/01–05 |
