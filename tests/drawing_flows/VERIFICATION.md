# Native drawing editor verification (phase 12)

Real Group BIM app (release, Apple M5, Metal) opened `tests/fixtures/drawings/control.obim`.
`gui-script.json` drives it through `--script` (ordinary egui input events: clicks,
drag, keys, text; the physical mouse is ignored while the script runs). Run log
`gui-run.log` exit 0, journal `gui-flow.json`, saved `gui-project.obim`.

| Capture | Step |
|---|---|
| step-1.png | F3, beam selected in tree, «Чертёж детали» → КМД-1 (A3, 1:10) |
| step-2.png | sheet A2, scale 1:20 from combo boxes |
| step-3.png | top view dragged down on the sheet with the mouse |
| (export/) | PDF, SVG, DXF exported from the panel |
| step-4.png | note «Швы по ГОСТ 5264-80, катет 6 мм» added at (60, 120) |
| step-5.png | ⌘Z removes the note; ⇧⌘Z restores it |
| step-6.png | beam moved 500 mm in 3D mode → drawing «УСТАРЕЛ», «Выдать» disabled |
| step-7.png | «Обновить по модели» → «Актуален», then «Выдать» |

`verify.py` (ezdxf 1.3.5, pypdf 5.1.0 in work/verify-venv, no project code) checks the
saved drawing record, the journal order and the exported files: PDF page 594×420 mm
with Cyrillic text extracted through ToUnicode, DXF audit clean with $INSUNITS=4,
SVG viewBox. Results in `verification-results.json`.

## Независимая проверка DWG (AC1018)

GNU LibreDWG 0.13.4 и ezdxf 1.4.2 прочитали `beam-a3`, `plate-a3`,
`assembly-a3` и `general-a1`. Совпали слои, число сущностей по слоям,
координаты рамок, контрольный размер 3000 мм при 1:10 и кириллица.
Аудит ezdxf ошибок не обнаружил. LibreDWG при чтении DWG сообщает о
служебных объектах и ограничениях чтения. Сообщения сохранены в отчёте
`tests/fixtures/exports/verification-results.json`.

Установите `dwg2dxf` из GNU LibreDWG (например, соберите исходники из
[выпуска 0.13.4](https://github.com/LibreDWG/libredwg/releases/tag/0.13.4))
и `python3 -m pip install ezdxf==1.4.2` в отдельной среде. Запустите
`python3 tests/fixtures/exports/verify_dwg.py` из корня проекта.
