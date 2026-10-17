# Native steel release verification (phase 13)

Real Group BIM app (release build, Apple M5, Metal), scripted GUI input (`session-1.json`,
`session-2.json`), journals `gui-flow-1/2.json`, logs `gui-run-1/2.log` (exit 0).

Session 1 opened `tests/fixtures/steel_release/source.obim` (phase-9 GUI model: 16 parts,
6 connections): F4 → «Сборки по заводским швам» (12 assemblies, step-1..3: parts,
assemblies, fasteners tabs) → «Выпустить комплект» into `release/` (step-4: 14 drawings,
18 files) → F3 drawing list (step-5) → ⌘S `gui-project.obim`.

Session 2 reopened `gui-project.obim`: F4 → «Сформировать ведомости» → CSV export into
`reopen/` (step-6), F3 drawings still listed and up to date (step-7).

`verify.py` (pypdf 5.1.0, csv; no project code): CSV from both sessions byte-identical,
total 2757.979 kg equals the independent expected total, 12 assemblies cover all 16 parts
once, 14 PDFs + 14-page set readable with correct sheet sizes.
