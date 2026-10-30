# Drawing engine (phase 11)

`crates/drawings` regenerates sheets from the model; the model stores only
`Drawing` definitions (`crates/model/src/drawing.rs`): kind (general / part /
assembly), designation, title, sheet A4–A0, scale 1:N, views (direction, optional
section depth, sheet position), associative dimension chains (anchors = object
ID + Start/End/Vertex/Hole), notes, object fingerprints and generated/issued
revisions. Create/replace/delete and undo/redo go through ordinary transactions.

* Views: orthographic first-angle projection (ГОСТ 2.305) of exact-body
  tessellations in the drawing frame (member axis horizontal, plate plane, world
  for GA). Feature edges = boundary, crease >15°, view silhouettes.
* Hidden lines: rays toward the viewer against per-part BVH meshes (parry3d);
  transitions found by bisection; hidden parts covered by visible lines are dropped.
* Sections: plane cut loops, 45° hatching at 2 mm, geometry in front removed.
* Dimensions and marks are recomputed from anchors on every generation;
  a missing anchor object becomes a lost binding (red, `data-lost`).
* Status: fingerprints (SHA-256 of element, details, profile, material) →
  `UpToDate`, `Outdated{changed}`, `LostReferences{missing}`. `issue()` refuses
  anything but `UpToDate`; `updated()` refreshes fingerprints.
* SVG: deterministic, paper millimetres, UTF-8 Cyrillic text, title block based on
  ГОСТ 2.104 form 1 (simplified: designation, title, material, mass, scale, sheet,
  status).

Reference sheets: `tests/fixtures/drawings/*.svg`, checked byte-for-byte by
`crates/drawings/tests/engine.rs` (re-bless with `OPEN_BIM_BLESS=1`) and read
independently by `tests/fixtures/drawings/verify.py`.

Limits: no curved-edge dimensions, radius/angle dimensions, weld symbols, bolt
symbols, automatic mark collision avoidance or multi-sheet sets yet.

## Editor and export (phase 12)

* GUI: «Чертежи · F3» mode. Panel `crates/desktop/src/drawing_editor/`: create
  part/assembly/GA drawings, sheet A4–A0, scale, view position (field or mouse
  drag on the sheet), dimension chains (new chain from selection, add/remove
  anchors, offset), notes, update, issue, delete, export. Each edit is one
  `ReplaceDrawing` transaction (⌘Z/⇧⌘Z).
* `crates/drawing_export`: PDF 1.7 in millimetre user space with embedded
  Ubuntu Light (Cyrillic, Identity-H + ToUnicode), DXF R12 (layers OB_*,
  DASHED hidden lines, $INSUNITS=4, \U+XXXX text), SVG; `qa::check` reports
  labels outside the frame, overlapping labels and missing glyphs.
* Control set `tests/fixtures/exports/` (A3 part/plate/assembly, A1 GA):
  byte-stable references, PDFKit raster snapshots, independent check
  `verify.py` (pypdf, ezdxf).

## Менеджер документов

«Менеджер документов» (Ctrl+L, палитра команд, кнопка в панели «Чертежи»)
показывает все чертежи проекта одной таблицей: комплект (КМ, КМД), номер
листа, обозначение, наименование, тип, марку детали или сборки, формат,
масштаб, статус и выдачу. Фильтры: текст (обозначение, наименование, марка,
тип), тип чертежа и статус (актуальные, устаревшие, потеряны привязки,
выданные, не выданные). Над отмеченными строками работают «Обновить по
модели», «Выдать» (только актуальные и ещё не выданные), «PDF» и «DXF» (файл на
каждый чертёж), «PDF комплекта» (все отмеченные листы одним файлом в порядке
комплекта и листов) и «Удалить». Каждая массовая правка — одна транзакция,
⌘Z отменяет её целиком. Логика — `crates/desktop/src/drawing_editor/manager.rs`,
тесты — `crates/desktop/tests/document_manager.rs`.

## Мастер чертежей и клонирование

«Мастер чертежей» (палитра команд, кнопка в панели «Чертежи») создаёт чертежи
сборок и деталей по правилам: вся модель или выбранные детали; сборочные
чертежи; чертежи деталей — все, только пластины или кроме главных деталей
сборок; «Общие данные» КМД, если их нет. На каждую марку создаётся один
чертёж (одинаковые детали и сборки делят чертёж), марки, у которых чертёж уже
есть, пропускаются и перечисляются. До создания мастер показывает, сколько
чертежей и каких марок получится. Все чертежи создаются одной транзакцией
(⌘Z отменяет). Если выбран образец, с него клонируется раскладка: формат,
масштаб, положение и масштабы видов (по направлению), примечания и графика
листа; размеры остаются построенными по своим объектам. Логика —
`crates/desktop/src/drawing_editor/wizard.rs`, тесты —
`crates/desktop/tests/drawing_wizard.rs`.

Правила мастера (`plan`, `build`, `clone_layout`) находятся в
`crates/drawings/src/wizard.rs` и общие для интерфейса и пакетного режима.
Чертежи через API и макросы (создание по типу, открытый чертёж, обновление,
выдача, PDF/DXF) — `crates/api/src/drawings.rs`, `docs/help/batch-api.md`.

## Гнутые детали

Параметры гнутых деталей участвуют в отслеживании изменений чертежа, а в спецификации сборки приводится длина по оси или длина развёртки гнутого листа. Создание стандартного чертежа гнутой детали пока недоступно: существующие виды и привязки размеров рассчитаны на прямые элементы. Графический чертёж развёртки пока не формируется.
