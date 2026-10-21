//! «Серии нумерации» and «Нумеровать»: marks are fixed in the model and kept.
use open_bim_desktop::tools::Editor;
use open_bim_desktop::tools::coordination_ui::{CoordinationUi, series_commands};
use open_bim_model::{ElementShape, MemberKind, example_steel_frame};
use std::collections::BTreeSet;

#[test]
fn series_and_stored_marks_through_the_editor() {
    let mut editor = Editor::new(example_steel_frame().unwrap()).unwrap();
    let project = editor.document.project();
    let columns: BTreeSet<_> = project
        .elements
        .values()
        .filter(|e| {
            matches!(
                &e.shape,
                ElementShape::Member {
                    member_kind: MemberKind::Column,
                    ..
                }
            )
        })
        .map(|e| e.id)
        .collect();
    let commands = series_commands(project, &columns, Some(("К", 201.)), None);
    assert_eq!(commands.len(), columns.len() * 2);
    editor.commit_data("Серия нумерации", &commands).unwrap();

    let mut ui = CoordinationUi::default();
    let (numbering, report) = CoordinationUi::compute(editor.document.project(), false).unwrap();
    ui.accept(numbering, report);
    ui.store(&mut editor).unwrap();
    let stored = editor.document.project().numbering.clone();
    for id in &columns {
        assert_eq!(stored[id], "К201");
    }
    assert!(
        !ui.stale(editor.document.project()),
        "storing marks keeps the report"
    );
    assert_eq!(ui.outdated_count(editor.document.project()), 0);
    // Nothing new to store: no extra transaction.
    let revision = editor.document.project().revision;
    ui.store(&mut editor).unwrap();
    assert_eq!(editor.document.project().revision, revision);
    // Undo removes the stored marks again.
    editor.document.undo().unwrap();
    assert!(editor.document.project().numbering.is_empty());

    // Reset puts the parts back into the default series.
    let reset = series_commands(editor.document.project(), &columns, Some(("", 1.)), None);
    editor.commit_data("Сброс", &reset).unwrap();
    assert!(columns.iter().all(|id| {
        !editor.document.project().elements[id]
            .properties
            .contains_key(open_bim_numbering::SERIES_PREFIX)
    }));
}
