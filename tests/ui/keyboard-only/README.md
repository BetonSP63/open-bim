# Keyboard-only scenario

`script.json` contains only key and text steps (no pointer events). Run from
this directory so the PDF lands in `drawings/`:

    ../../../target/release/open-bim-desktop --script "$PWD/script.json" \
      --project-path "$PWD/keyboard-only.obim" --record "$PWD/journal.json"

Steps: ⌘N → ⌥C, «А/1» Enter, «Б/2» Enter → Esc → ⌥B, «А/1/+3,600» Enter,
Alt+→ ×2, Space → Esc → ⌥P, «В/3», «В/4», «Г/4», «Г/3», Enter → Esc →
Alt+→, Space (select the first part) → F6 ×3 (first field of the property
pane) → Tab ×7, «К-1», Enter → F6 ×3 (back to the view) → ⌘D → ⇧⌘P → ⌘S.

`check_keyboard_only.py` (run with the verify venv) checks the script has no
pointer steps, the journal order, part coordinates against the grid, the saved
file against the journal and the PDF; output in `check.txt`.

Keys that differ from the first plan: the keyboard cursor uses Alt+arrows
because plain arrows pan the view (Tekla); Space picks or selects, Alt+Space
adds/removes from the selection (Shift+Space fits the selection), Enter closes
a contour.
