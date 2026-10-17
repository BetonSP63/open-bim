# Steel release (phase 13)

Workflow: frame → parametric connections → assemblies → numbering → drawing set →
part/assembly/fastener lists. Everything is regenerated from the model.

* `crates/reports`: `generate()` returns part rows (position, name, section,
  material, quantity, length, unit/total net mass), assembly rows (mark, main part,
  quantity, parts per assembly, masses) and fastener rows (bolts by diameter,
  standard and grade; welds by type, leg and shop/site with total length).
  Totals are summed before rounding. `auto_assemblies()` joins parts connected by
  shop welds; the main part is the heaviest member.
* CSV (`csv.rs`): RFC 4180, comma, CRLF, quotes doubled, fields with commas,
  quotes or line breaks quoted, UTF-8 with BOM.
* Release (`release.rs`): creates missing drawings (GA A1, one per assembly
  and part position), refreshes outdated ones, refuses export while any
  drawing is outdated or has lost bindings; writes one PDF per drawing,
  `комплект.pdf`, three CSV lists and `release.json`.
* GUI: **F4 Ведомости** (tables, stale warning, CSV, release set).
  CLI: `open-bim-release`.

Evidence: `crates/reports/tests/release.rs` (independent totals from
`tests/fixtures/steel_release/expected.py`, CSV escaping, regeneration after a
deleted node, outdated refusal), `tests/fixtures/steel_release/verify.py`
(Python csv reader), native flow `tests/steel_release_flow/` (two sessions:
release + save, reopen + CSV export; identical CSV; 14 PDFs read by pypdf).

Limits: no bolt length selection, washers/nuts lists, paint areas, NC/DSTV,
cutting optimisation, or report templates; positions use the deterministic
`P0001`/`A0001` scheme without user prefixes.
