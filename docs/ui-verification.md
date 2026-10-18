# UI verification (phase 17)

Shell code: `crates/desktop/src/ui/` (`Workbench` panels, `commands` registry,
`palette`, `theme`, `jobs`). The native binary and the headless tests run the
same shell; only the GPU viewport is binary-specific.

## 1. Russian commands, shortcuts, search, no stubs
`ui/commands.rs`: 40 commands, each with a Russian label, category, keywords;
32 shortcuts (⌘N/⌘O/⌘S, ⌘Z/⇧⌘Z, F1–F4, F, 1–4, Alt+A/B/C/P/G/S tools, ⌘↩ create,
⌘L property, ⌘K clash check, ⌘E/⌘I IFC, ⌘D part drawing, ⌘U update, ⇧⌘P PDF,
⌘R lists, ⇧⌘R release, ⌘P palette, Esc). Command palette ⌘P searches label,
category, keywords and shortcut text. `crates/desktop/tests/ui_shell.rs`
`commands_are_named_unique_searchable_and_not_stubs` executes every command and
asserts an observable effect; F1 lists all shortcuts (`tests/ui/screens/help-*.png`).

## 2. Keyboard-only flow
`tests/ui/keyboard/` — native app, keys only: ⌘N → Alt+B → ⌘↩ (beam) → ⌘L, type
«Марка», Tab, «Б-1», Enter (property) → ⌘P «чертёж дет» Enter (drawing) → ⇧⌘P (PDF)
→ ⌘E (IFC) → ⌘S. Journal `journal.json`: 7 events in order; files `drawings/КМД-1.pdf`,
`keyboard.ifc`, `keyboard.obim`. Captures step-1..5 (tool fields, property in focus,
palette, saved, F1 help). Single-key shortcuts are ignored while a text field has
focus, so typing never triggers commands.

## 3. 1280×800 and 1920×1080
Headless test `every_control_has_an_accessible_name_and_fits_both_window_sizes`
lays out model/drawings/reports at exactly 1280×800 and 1920×1080 points and
asserts every control lies inside the window (`tests/ui/accessibility-tree.json`:
87/74/52 controls). Native captures `tests/ui/screens/*-1280x800.png` (window
1280×800) and `*-1920x1080.png`: the host display is 1470×956 points, so a
1920×1080-point layout is produced by a 1440×810 window with egui zoom 0.75
(`--size 1440x810 --zoom 0.75`); this is stated, not hidden. Defect found and fixed:
the reports table was cut at 1280×800 by the model tree → the tree is shown only in
3D mode.

## 4. Empty state, loading, cancel, file error
`tests/ui/jobs/`: empty-model/drawings/reports captures; project opening runs on a
worker thread with progress and «Отменить · Esc» (`loading.png` with a 4 s test
delay, `cancelled.png`, journal «Отмена: …»; the model is unchanged); a corrupt
`.obim` shows the parser cause and «Текущая модель не изменена» (`file-error.png`,
journal). Release export and clash check also run as cancellable jobs; a cancelled
release leaves no output folder (staging directory). Headless test
`file_error_and_cancelled_open_leave_the_model_unchanged` checks hashes. Defect
found and fixed: the first implementation lost the error cause («операция прервана»).

## 5. VoiceOver names, focus, contrast
macOS Accessibility queries from scripts are not permitted on this host (System
Events: «osascript не разрешён»), and that security setting was not changed.
VoiceOver reads the AccessKit tree that egui publishes; the headless test enables
AccessKit and requires a non-empty name (label or labelled-by) for every button,
checkbox, text input, combo box and slider. Defects found and fixed: unnamed combo
boxes (snap mode, profile, material, sheet/scale, connection), unlabelled text
fields, unnamed visibility checkboxes in the model tree. Visible focus: 2 px blue
ring (`tests/ui/focus/focus-tab3.png`). Contrast: `contrast.json`, all text pairs
≥ 4.5:1 (`text_contrast_is_at_least_wcag_aa`). Missing glyphs ⇧ ↑ ↓ ↩ in Ubuntu
Light fixed with a Hack fallback; ⌥ has no glyph in the bundled fonts, so Option is
written «Alt+» (`every_caption_glyph_exists_in_the_proportional_fonts`).

Not verified: a live VoiceOver session reading the app aloud (needs the user to
grant Accessibility/VoiceOver interaction).
