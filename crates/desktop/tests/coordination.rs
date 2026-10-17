use open_bim_coordination::ConflictKind;
use open_bim_desktop::tools::{
    Edit, Editor,
    coordination_ui::{CoordinationUi, row_objects},
};
use std::time::Instant;

fn fixture() -> open_bim_model::Project {
    open_bim_persistence::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/connections_flows/gui-project.obim"
    ))
    .unwrap()
}

#[test]
fn clash_rows_select_parts_follow_revisions_and_reopen() {
    let mut editor = Editor::new(fixture()).unwrap();
    let mut ui = CoordinationUi::default();
    let started = Instant::now();
    ui.run(editor.document.project()).unwrap();
    assert!(started.elapsed().as_secs() < 120);
    let report = ui.report.clone().unwrap();
    let count = |kind| report.conflicts.iter().filter(|c| c.kind == kind).count();
    // Phase-9 GUI fixture: 6 nodes, contact faces and fitted connection bolts only.
    assert_eq!(count(ConflictKind::Touching), 12);
    assert_eq!(count(ConflictKind::AllowedBoltHole), 40);
    assert_eq!(count(ConflictKind::Intersection), 0);
    let bolts = report
        .conflicts
        .iter()
        .filter(|c| c.kind == ConflictKind::AllowedBoltHole)
        .count();
    assert!(bolts > 0, "connection bolts must be recognised as fitted");
    // Rows filter: only intersections by default, every row when informative.
    assert_eq!(ui.visible_rows().len(), report.blocking().count());
    ui.show_informative = true;
    assert_eq!(ui.visible_rows().len(), report.conflicts.len());
    let (row, conflict) = report
        .conflicts
        .iter()
        .enumerate()
        .find(|(_, c)| c.kind == ConflictKind::AllowedBoltHole)
        .unwrap();
    ui.select_row(row, &mut editor).unwrap();
    assert_eq!(
        editor.selected,
        row_objects(editor.document.project(), conflict)
    );
    assert!(editor.selected.contains(&conflict.second));
    assert!(!ui.stale(editor.document.project()));
    // Any revision makes the report stale until it is recomputed.
    let part = *editor.document.project().elements.keys().next().unwrap();
    editor.selected = [part].into();
    editor
        .edit(&Edit::Move(open_bim_model::Point3([0., 0., 5.])))
        .unwrap();
    assert!(ui.stale(editor.document.project()));
    ui.run(editor.document.project()).unwrap();
    assert!(!ui.stale(editor.document.project()));
    // Numbering and clash rows are reproducible after save and reopen.
    let dir = std::env::temp_dir().join(format!("open-bim-clash-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("model.obim");
    editor.save(&path).unwrap();
    let mut reopened = CoordinationUi::default();
    reopened
        .run(&open_bim_persistence::load(&path).unwrap())
        .unwrap();
    assert_eq!(reopened.numbering, ui.numbering);
    assert_eq!(reopened.report, ui.report);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn native_gui_saved_clash_matches_independent_volume() {
    let project = open_bim_persistence::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/coordination_flows/gui-project.obim"
    ))
    .unwrap();
    let mut ui = CoordinationUi::default();
    ui.run(&project).unwrap();
    let report = ui.report.unwrap();
    let rows: Vec<_> = report.blocking().collect();
    assert_eq!(rows.len(), 6);
    let columns = rows
        .iter()
        .find(|c| (c.first.0, c.second.0) == (8, 9))
        .unwrap();
    // tests/coordination_flows/verify.py: 2 flange overlaps 50x12 mm over 3000 mm.
    assert!((columns.overlap_mm3 - 3_600_000.).abs() < 1e-3);
    assert_eq!(columns.first_position, columns.second_position);
}
