//! «Мастер чертежей»: one drawing per mark by rules, cloning a template.
use open_bim_desktop::drawing_editor::DrawingEditor;
use open_bim_desktop::drawing_editor::wizard::{self, PartRule, Settings};
use open_bim_desktop::tools::Editor;
use open_bim_model::{DrawingKind, ElementShape, SheetSize};
use std::collections::BTreeSet;

/// The release model with assemblies made by welds, as before a release.
fn editor() -> Editor {
    let project = open_bim_persistence::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/steel_release/source.obim"
    ))
    .unwrap();
    let mut editor = Editor::new(project).unwrap();
    let commands = open_bim_reports::auto_assemblies(editor.document.project()).unwrap();
    editor.commit_data("Сборки по швам", &commands).unwrap();
    editor
}

#[test]
fn one_drawing_per_mark_then_nothing_left() {
    let mut editor = editor();
    let numbering = open_bim_numbering::number(editor.document.project()).unwrap();
    let assembly_marks: BTreeSet<_> = numbering.assemblies.values().cloned().collect();
    let part_marks: BTreeSet<_> = numbering.parts.values().cloned().collect();
    assert!(!assembly_marks.is_empty());
    let settings = Settings::default();
    let plan = wizard::plan(editor.document.project(), &BTreeSet::new(), &settings).unwrap();
    assert_eq!(plan.drawings.len(), assembly_marks.len() + part_marks.len());
    assert!(plan.general_data);
    let before = editor.document.project().drawings.len();
    let revision = editor.document.project().revision;
    let created = wizard::run(&mut editor, &BTreeSet::new(), &settings).unwrap();
    assert_eq!(created, plan.drawings.len() + 1);
    assert_eq!(
        editor.document.project().revision,
        revision + 1,
        "one transaction"
    );
    let project = editor.document.project();
    assert_eq!(project.drawings.len(), before + created);
    assert!(
        project
            .drawings
            .values()
            .any(|d| d.number == format!("КМД-{}", assembly_marks.iter().next().unwrap()))
    );
    // Every mark is covered now: nothing to create, a clear message.
    let again = wizard::plan(project, &BTreeSet::new(), &settings).unwrap();
    assert!(again.drawings.is_empty());
    assert!(!again.general_data);
    assert_eq!(
        again.existing.len(),
        assembly_marks.len() + part_marks.len()
    );
    let error = wizard::run(&mut editor, &BTreeSet::new(), &settings).unwrap_err();
    assert!(
        error.starts_with("У всех марок уже есть чертежи"),
        "{error}"
    );
    // ⌘Z removes the whole batch.
    editor.history(false).unwrap();
    assert_eq!(editor.document.project().drawings.len(), before);
}

#[test]
fn rules_and_selection_narrow_the_plan() {
    let editor = editor();
    let project = editor.document.project();
    let plates = Settings {
        assemblies: false,
        part_rule: PartRule::Plates,
        ..Default::default()
    };
    let plan = wizard::plan(project, &BTreeSet::new(), &plates).unwrap();
    assert!(!plan.drawings.is_empty());
    for d in &plan.drawings {
        let DrawingKind::Part { part } = d.kind else {
            panic!("only part drawings")
        };
        assert!(matches!(
            project.elements[&part].shape,
            ElementShape::Plate { .. }
        ));
    }
    let main_parts: BTreeSet<_> = project.assemblies.values().map(|a| a.main_part).collect();
    let secondary = Settings {
        assemblies: false,
        part_rule: PartRule::Secondary,
        ..Default::default()
    };
    for d in wizard::plan(project, &BTreeSet::new(), &secondary)
        .unwrap()
        .drawings
    {
        let DrawingKind::Part { part } = d.kind else {
            unreachable!()
        };
        assert!(!main_parts.contains(&part));
    }
    // Selection: one part gives its own drawing and its assembly's.
    let assembly = project.assemblies.values().next().unwrap();
    let selected = BTreeSet::from([assembly.main_part]);
    let only = Settings {
        selection_only: true,
        ..Default::default()
    };
    let plan = wizard::plan(project, &selected, &only).unwrap();
    assert!(plan.drawings.iter().any(|d| d.kind
        == DrawingKind::Assembly {
            assembly: assembly.id
        }));
    assert!(plan.drawings.iter().any(|d| d.kind
        == DrawingKind::Part {
            part: assembly.main_part
        }));
    assert_eq!(plan.drawings.len(), 2);
}

#[test]
fn template_layout_is_cloned() {
    let mut editor = editor();
    let mut drawings = DrawingEditor::new();
    let first = editor
        .document
        .project()
        .assemblies
        .values()
        .next()
        .unwrap()
        .clone();
    editor.selected = [first.main_part].into();
    drawings.create_assembly(&mut editor).unwrap();
    let template = drawings.current.unwrap();
    drawings.set_sheet(&mut editor, SheetSize::A2).unwrap();
    drawings.set_scale(&mut editor, 15.).unwrap();
    drawings.move_view(&mut editor, 1, [150., 200.]).unwrap();
    drawings
        .add_text(&mut editor, "Сварные швы по ГОСТ 5264-80", [40., 80.], 5.)
        .unwrap();
    let settings = Settings {
        parts: false,
        general_data: false,
        assembly_template: Some(template),
        ..Default::default()
    };
    let created = wizard::run(&mut editor, &BTreeSet::new(), &settings);
    let project = editor.document.project();
    let template = &project.drawings[&template];
    let clones: Vec<_> = project
        .drawings
        .values()
        .filter(|d| d.id != template.id && matches!(d.kind, DrawingKind::Assembly { .. }))
        .collect();
    assert_eq!(created.unwrap(), clones.len());
    assert!(!clones.is_empty());
    for d in clones {
        assert_eq!(d.sheet, SheetSize::A2);
        assert_eq!(d.scale, 15.);
        assert_eq!(d.texts, template.texts);
        let front = d.views.iter().find(|v| v.id == 1).unwrap();
        assert_eq!(front.position, [150., 200.]);
    }
}
