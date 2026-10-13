# Native connection workflow verification

Actual Group BIM application on macOS ARM64, Apple M5, Metal. The source project has10authored source parts and an analytical I600 support section. It has no connections. All six resulting nodes were created using the actual connection panel; no script inserted nodes into the saved model.

Session1 journal records9events: base plate creation, save, thickness20->25, save, export/import/reapply to another column, save, undo/redo, endplate creation. Three real framebuffer captures are step-1..3.png. The first native launch and retry exited101 with an application input deadlock. Retry backtrace points to Response::dragged_by inside Context::input in main.rs. The fix evaluates response state before entering the input closure. Journals and captures remain intact. recovered-session-1.obim restores the last recorded revision4; this recovery file is reconstructed from the actual journal, not described as a GUI save.

Session2 resumed that model after the fix. It records12events: save/reapply end template to another beam/column, create angle, update height170->190, save/reapply exported angle to another beam/column, move base-column parent100mm, save, new project, open. The process exited normally0; actual viewport pointer, keyboard orbit/pan and all further panel operations ran without panic. Eight real framebuffer files are retained overall. The first three session2 capture attempts failed because its output directory was missing; after creating that directory, successful captures are session-2/step-4..8.png. No missing capture is counted as evidence.

| Capture | Result |
|---|---|
| step-1.png | First base node |
| step-2.png | Base thickness25mm and exported template |
| step-3.png | Reapplied base template |
| session-2/step-4.png | Actual angle and linked source parts |
| session-2/step-5.png | Height500mm rejected; previous valid model preserved |
| session-2/step-6.png | Height190mm and exported angle template |
| session-2/step-7.png | Second angle from imported template |
| session-2/step-8.png | Final model reopened from saved file |

Independent Python JSON/SHA verification confirms saved/reopened model equality and base undo/redo hashes. Final canonical payload SHA-256:74ff9a2c0cc5f09709f2b3ba3d4615fa72da72f76e373ecc21d654eacac8ce09. Six nodes,16parts,52details,60unique owned child IDs,nextID106. Two nodes of each template family have equal exported parameters and different parent bindings. Base parent8 moved toX100; its plate41 moved to(-45,-220,-25) with the same ID. Independent mass formulas are recorded in verification-results.json and checked against the actual saved model by crates/connections/tests/gui_fixture.rs.

Performance remains unfinished: debug scene rebuilding of this16part detailed fixture can exceed the16.7second accessibility observation timeout. The same live app was re-polled; actions were not repeated or processes restarted for observation timeouts. Final UI/performance phases must address this latency. This verification does not claim capacity calculations, rolled fillets, arbitrary support robustness or full Tekla parity.
