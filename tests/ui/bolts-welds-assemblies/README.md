# Phase 9: bolts, welds, assemblies, gusset and stiffener

Native run (from this directory, example project):

    ../../../target/release/open-bim-desktop --example --script "$PWD/script.json" \
      --project-path "$PWD/bolts-welds.obim" --record "$PWD/journal.json"

| Capture | What it shows |
|---|---|
| 01-bolt-preview.png | bolt command after the start point: the group preview (green), the «Свойства болтов» pane with the steps |
| 02-bolts-created.png | the bolt group on the base plate |
| 03-weld-properties.png | weld command: type, one/both sides, leg, all around, shop/site, ГОСТ 5264-80 designation |
| 04-weld-created.png | the weld between the column and the base plate (red line with «Т1-Δ6») |
| 05-stiffener-properties.png | stiffener command: what it builds and its sizes |
| 06-stiffener-zoom.png | the stiffener between the flanges with the corner snips (transparent view) |

The ribbon «Металл → Сборки и узлы» shows the assembly commands as small
buttons in every capture. Journal: bolt group, weld, stiffener, save.
