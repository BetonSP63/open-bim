# Native IFC flow (phase 14)

Real Group BIM app, scripted input (`gui-script.json`, log `gui-run.log` exit 0,
journal `gui-flow.json`). `model.obim` = phase-9 connection model.
step-1: «Экспорт IFC4» → `model.ifc` (byte-identical to `tests/fixtures/ifc/connections.ifc`,
validated by IfcOpenShell). step-2: «Новый проект» (0 objects). step-3: «Импорт IFC4» →
16 elements (6 beams, 6 columns, 4 plates) and 40 openings restored, report shown in the panel.
