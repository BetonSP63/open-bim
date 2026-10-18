"""Writes regression.json: the final keyboard-driven control scenario."""
import json, sys
from pathlib import Path
d = Path(sys.argv[1]).resolve()
def key(name, **m): return {"key": {"name": name, **m}}
def cmd(name): return key(name, command=True)
steps = [
    {"wait": 25},
    {"note": "⌘K — нумерация и коллизии (фоновая задача)"}, cmd("K"), "wait_idle", {"capture": str(d / "01-clash.png")},
    {"note": "⌘P «сборки по» — сборки по заводским швам"}, cmd("P"), {"wait": 3}, {"text": "сборки по"}, {"wait": 3}, key("Enter"), {"wait": 10},
    {"note": "⌘R — ведомости"}, cmd("R"), {"wait": 10}, {"capture": str(d / "02-reports.png")},
    {"note": "⇧⌘R — выпуск комплекта в папку release"}, key("R", command=True, shift=True), "wait_idle", {"capture": str(d / "03-release.png")},
    {"note": "F3, ⌘P «общий» — общий вид A1, ⇧⌘P — PDF"}, key("F3"), cmd("P"), {"wait": 3}, {"text": "общий вид"}, {"wait": 3}, key("Enter"), {"wait": 20},
    key("P", command=True, shift=True), {"wait": 10}, {"capture": str(d / "04-general.png")},
    {"note": "⌘E — экспорт IFC, ⌘S — сохранить"}, cmd("E"), {"wait": 5}, cmd("S"), {"wait": 5},
    {"note": "⌘N — новый проект, ⌘I — импорт IFC"}, cmd("N"), {"wait": 5}, cmd("I"), {"wait": 20}, key("F2"), {"wait": 5}, {"capture": str(d / "05-ifc-import.png")},
    {"note": "⌘O — повторное открытие сохранённого проекта (фоновая загрузка)"}, cmd("O"), "wait_idle", {"wait": 5}, key("F4"), cmd("R"), {"wait": 10}, {"capture": str(d / "06-reopened.png")},
    "quit",
]
(d / "regression.json").write_text(json.dumps(steps, ensure_ascii=False, indent=1))
