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
