# Phase 12: specifications and «Общие данные» (ГОСТ Р 21.101-2026, 4.3, 6, appendices Б, И; ГОСТ 21.502)

`crates/reports/tests/gost_tables.rs` writes the control set here
(`OPEN_BIM_BLESS=1` to refresh): `control-set.obim` and the sheets
`km-od`, `km-scheme`, `kmd-od`, `kmd-assembly`, `kmd-part` (.dxf/.pdf),
plus `rolled-steel.csv`.

`check_specifications.py` (ezdxf, pypdf, the .obim read as JSON) checks:
- form 7 graphs 15/60/65/10/15/20 mm, its header texts, and unit masses
  recomputed from the geometry within 0.001 kg;
- ГОСТ 21.502 totals per steel grade and overall, equal to the sums of part
  masses in tonnes;
- forms 1 and 2 of «Общие данные», sheet numbers in a row, and graph 4.

The result goes to `check.txt` (`SPECIFICATIONS_OK`).

Native run in `native/` (from that directory; the model has no drawings, so
they are made from the interface):

    ../../../../target/release/open-bim-desktop --open control-set-model.obim \
        --size 1600x1000 --record journal.json --script script.json

| Capture | What it shows |
|---|---|
| 01-general-data-km.png | ⌘P «Общие данные»: sheet 1 of КМ — sheet list, referenced documents, list of sets, general notes |
| 02-assembly-spec.png | ⌘P «Чертёж сборки»: column К1 with the form 7 specification above the inscription |
| 03-general-data-kmd.png | «Общие данные» again: sheet 1 of КМД, КМ as a referenced document |

`native/drawings/` holds the PDFs exported with ⇧⌘P; `pdf-check.txt` is
their pypdf check (`GUI_PDF_OK`).
