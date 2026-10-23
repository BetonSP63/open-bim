//! Drawings tab: «Свойства чертежа», «Каталог чертежей-прототипов» and
//! «Изменить номер».
use open_bim_desktop::drawing_editor::DrawingEditor;
use open_bim_desktop::drawing_editor::properties::{Props, apply};
use open_bim_desktop::drawing_editor::prototypes;
use open_bim_desktop::interaction::change_number::change_number;
use open_bim_desktop::tools::Editor;
use open_bim_drawings::sample::control_project;
use open_bim_model::{DrawingKind, SheetSize};
use std::collections::BTreeSet;

#[test]
fn properties_change_format_orientation_and_scale_in_one_step() {
    let f = control_project();
    let mut editor = Editor::new(f.project.clone()).unwrap();
    let mut drawings = DrawingEditor::new();
    editor.selected = [f.beam].into();
    drawings.create_part(&mut editor).unwrap();
    let beam = drawings.current.unwrap();
    editor.selected = [f.plate].into();
    drawings.create_part(&mut editor).unwrap();
    let plate = drawings.current.unwrap();
    let project = editor.document.project();
    let mut props = Props::of(&project.drawings[&beam]);
    props.number = "КМД-7".into();
    props.sheet = SheetSize::A3;
    props.multiple = 3;
    props.portrait = true;
    props.scale = 20.;
    let commands = apply(project, &BTreeSet::from([beam]), &props).unwrap();
    editor.commit_data("Свойства", &commands).unwrap();
    let d = &editor.document.project().drawings[&beam];
    assert_eq!(
        (d.number.as_str(), d.multiple, d.portrait),
        ("КМД-7", 3, true)
    );
    assert_eq!(d.format_label(), "A3×3");
    let [w, h] = d.sheet_mm();
    assert!(
        d.views
            .iter()
            .all(|v| v.position[0] <= w && v.position[1] <= h)
    );
    // Several drawings: designations stay, formats change together.
    let both = BTreeSet::from([beam, plate]);
    props.multiple = 1;
    props.portrait = false;
    props.sheet = SheetSize::A2;
    let commands = apply(editor.document.project(), &both, &props).unwrap();
    assert_eq!(commands.len(), 2);
    editor.commit_data("Свойства", &commands).unwrap();
    let p = editor.document.project();
    assert_eq!(p.drawings[&beam].number, "КМД-7");
    assert_ne!(p.drawings[&plate].number, "КМД-7");
    assert!(both.iter().all(|id| p.drawings[id].sheet == SheetSize::A2));
    // A multiple outside ГОСТ 2.301 is refused.
    props.multiple = 2;
    assert!(apply(p, &both, &props).is_err());
    assert!(apply(p, &BTreeSet::new(), &props).is_err());
}

#[test]
fn prototypes_are_saved_listed_and_clone_new_drawings() {
    let f = control_project();
    let mut editor = Editor::new(f.project.clone()).unwrap();
    let mut drawings = DrawingEditor::new();
    editor.selected = [f.beam].into();
    drawings.create_part(&mut editor).unwrap();
    let beam = drawings.current.unwrap();
    // Make the prototype distinctive.
    let mut props = Props::of(&editor.document.project().drawings[&beam]);
    props.sheet = SheetSize::A2;
    props.scale = 25.;
    let commands = apply(editor.document.project(), &BTreeSet::from([beam]), &props).unwrap();
    editor.commit_data("Свойства", &commands).unwrap();

    let dir = std::env::temp_dir().join(format!("obim-prototypes-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let project_path = dir.join("модель.obim");
    let folder = prototypes::folder(project_path.to_str().unwrap());
    assert_eq!(folder, dir.join(prototypes::FOLDER));
    let prototype = editor.document.project().drawings[&beam].clone();
    prototypes::save(&folder, "Деталь А2", &prototype).unwrap();
    assert!(prototypes::save(&folder, "../плохо", &prototype).is_err());
    assert_eq!(prototypes::list(&folder), vec!["Деталь А2".to_string()]);
    let loaded = prototypes::load(&folder, "Деталь А2").unwrap();
    assert_eq!(loaded, prototype);

    // The plate has no drawing yet: one is cloned from the prototype.
    let before = editor.document.project().drawings.len();
    let n = prototypes::create_from(&mut editor, &BTreeSet::from([f.plate]), &loaded).unwrap();
    assert_eq!(n, 1);
    let p = editor.document.project();
    assert_eq!(p.drawings.len(), before + 1);
    let new = p
        .drawings
        .values()
        .find(|d| matches!(d.kind, DrawingKind::Part { part } if part == f.plate))
        .unwrap();
    assert_eq!((new.sheet, new.scale), (SheetSize::A2, 25.));
    // Nothing selected: refused.
    assert!(prototypes::create_from(&mut editor, &BTreeSet::new(), &loaded).is_err());
    prototypes::remove(&folder, "Деталь А2").unwrap();
    assert!(prototypes::list(&folder).is_empty());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn change_number_gives_the_selected_mark_a_number_that_numbering_keeps() {
    let f = control_project();
    let mut editor = Editor::new(f.project.clone()).unwrap();
    let before = open_bim_numbering::number(editor.document.project()).unwrap();
    let old = before.parts[&f.plate].clone();
    let (commands, list) = change_number(
        editor.document.project(),
        &BTreeSet::from([f.plate]),
        false,
        50,
    )
    .unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].0, old);
    editor.commit_data("Изменить номер", &commands).unwrap();
    let after = open_bim_numbering::number(editor.document.project()).unwrap();
    assert!(after.parts[&f.plate].ends_with("50"), "{:?}", after.parts);
    for (id, mark) in &before.parts {
        if *mark != old {
            assert_eq!(&after.parts[id], mark, "other marks stay");
        }
    }
    // A mark already used by another part is refused.
    let other = after
        .parts
        .iter()
        .find(|(id, m)| **id != f.plate && **m != after.parts[&f.plate])
        .map(|(id, m)| (*id, m.clone()));
    if let Some((_, mark)) = other
        && let Ok(n) = mark
            .trim_start_matches(|c: char| !c.is_ascii_digit())
            .parse::<u64>()
    {
        assert!(
            change_number(
                editor.document.project(),
                &BTreeSet::from([f.plate]),
                false,
                n
            )
            .is_err()
        );
    }
    assert!(change_number(editor.document.project(), &BTreeSet::new(), false, 3).is_err());
    assert!(
        change_number(
            editor.document.project(),
            &BTreeSet::from([f.plate]),
            false,
            0
        )
        .is_err()
    );
}
