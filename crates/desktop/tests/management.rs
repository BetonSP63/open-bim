//! Phases, lots, sequences and locks: lists in the project, numbers and the
//! lock on the parts, each change one undo step.
use open_bim_commands::{Command, DeletePolicy};
use open_bim_desktop::tools::Editor;
use open_bim_desktop::ui::management::{
    ManagementUi, Outcome, add_entry, assign, is_locked, members, phase_of, remove_entry,
    remove_sequence, sequence,
};
use open_bim_model::management::{LOCK_PROPERTY, LOT_PROPERTY, PHASE_PROPERTY};
use open_bim_model::{ObjectId, PropertyValue};
use std::collections::BTreeSet;

fn editor() -> Editor {
    let project = open_bim_persistence::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/steel_release/source.obim"
    ))
    .unwrap();
    Editor::new(project).unwrap()
}

fn parts(editor: &Editor, n: usize) -> Vec<ObjectId> {
    editor
        .document
        .project()
        .elements
        .keys()
        .take(n)
        .copied()
        .collect()
}

#[test]
fn phases_number_parts_and_new_parts_take_the_current_phase() {
    let mut editor = editor();
    let p = editor.document.project();
    let all = p.elements.len();
    // Without a phase every part is in phase 1.
    assert_eq!(members(p, false, 1).len(), all);
    let m = add_entry(&p.management, false, "Монтаж").unwrap();
    assert_eq!(m.phases[1].number, 2);
    let ids: BTreeSet<ObjectId> = parts(&editor, 2).into_iter().collect();
    let mut commands = vec![Command::SetManagement { management: m }];
    commands.extend(assign(
        p,
        &ids,
        PHASE_PROPERTY,
        Some(PropertyValue::Number(2.)),
    ));
    editor.commit("Стадия 2", &commands).unwrap();
    let p = editor.document.project();
    assert_eq!(members(p, false, 2), ids);
    assert_eq!(members(p, false, 1).len(), all - 2);
    // A phase in use cannot be removed; the current one neither.
    assert!(remove_entry(p, false, 2).is_err());
    assert!(remove_entry(p, false, 1).is_err());
    // Current phase 2: a copied part is created in phase 2.
    let mut m = p.management.clone();
    m.current_phase = 2;
    editor
        .commit("Текущая", &[Command::SetManagement { management: m }])
        .unwrap();
    let other = *editor
        .document
        .project()
        .elements
        .keys()
        .find(|id| !ids.contains(id))
        .unwrap();
    editor.selected = [other].into();
    editor
        .edit(&open_bim_desktop::tools::Edit::Copy(
            open_bim_model::Point3([0., 0., 5000.]),
        ))
        .unwrap();
    let p = editor.document.project();
    let copy = *p.elements.keys().last().unwrap();
    assert_eq!(phase_of(p, copy), Some(2));
    assert_eq!(phase_of(p, other), Some(1));
    // Undo takes the list back.
    editor.history_steps(false, 2).unwrap();
    assert_eq!(editor.document.project().management.current_phase, 1);
}

#[test]
fn lots_are_assigned_and_selected() {
    let mut editor = editor();
    let p = editor.document.project();
    let m = add_entry(&p.management, true, "").unwrap();
    assert_eq!(m.lots[0].name, "Партия 1");
    let ids: BTreeSet<ObjectId> = parts(&editor, 3).into_iter().collect();
    let mut commands = vec![Command::SetManagement { management: m }];
    commands.extend(assign(
        p,
        &ids,
        LOT_PROPERTY,
        Some(PropertyValue::Number(1.)),
    ));
    editor.commit("Партия", &commands).unwrap();
    assert_eq!(members(editor.document.project(), true, 1), ids);
    // Assigning again changes nothing.
    assert!(
        assign(
            editor.document.project(),
            &ids,
            LOT_PROPERTY,
            Some(PropertyValue::Number(1.))
        )
        .is_empty()
    );
}

#[test]
fn locked_parts_refuse_changes_and_deletion_until_unlocked() {
    let mut editor = editor();
    let id = parts(&editor, 1)[0];
    let ids = BTreeSet::from([id]);
    let lock = assign(
        editor.document.project(),
        &ids,
        LOCK_PROPERTY,
        Some(PropertyValue::Boolean(true)),
    );
    editor.commit("Заблокировать", &lock).unwrap();
    assert!(is_locked(editor.document.project(), id));
    editor.selected = ids.clone();
    let err = editor
        .edit(&open_bim_desktop::tools::Edit::Move(
            open_bim_model::Point3([100., 0., 0.]),
        ))
        .unwrap_err();
    assert!(err.contains("заблокирована"), "{err}");
    let delete = editor.commit(
        "Удалить",
        &[Command::Delete {
            id,
            policy: DeletePolicy::Cascade,
        }],
    );
    assert!(delete.is_err());
    // Assigning a phase leaves locked parts out.
    assert!(
        assign(
            editor.document.project(),
            &ids,
            PHASE_PROPERTY,
            Some(PropertyValue::Number(1.))
        )
        .is_empty()
    );
    let unlock = assign(editor.document.project(), &ids, LOCK_PROPERTY, None);
    editor.commit("Разблокировать", &unlock).unwrap();
    editor
        .edit(&open_bim_desktop::tools::Edit::Move(
            open_bim_model::Point3([100., 0., 0.]),
        ))
        .unwrap();
}

#[test]
fn sequence_numbers_follow_the_pick_order_and_go_with_the_sequence() {
    let mut editor = editor();
    let mut m = editor.document.project().management.clone();
    m.sequences.push("Монтаж".into());
    editor
        .commit(
            "Последовательность",
            &[Command::SetManagement { management: m }],
        )
        .unwrap();
    let mut ui = ManagementUi {
        picking: Some(("Монтаж".into(), 1)),
        ..Default::default()
    };
    assert!(ui.picks_objects());
    let order = parts(&editor, 3);
    for id in [order[2], order[0], order[1]] {
        match ui.pick(editor.document.project(), id) {
            Some(Outcome::Commit(label, commands)) => editor.commit(&label, &commands).unwrap(),
            other => panic!("{other:?}"),
        };
    }
    assert_eq!(
        sequence(editor.document.project(), "Монтаж"),
        vec![(1, order[2]), (2, order[0]), (3, order[1])]
    );
    assert!(ui.cancel());
    let commands = remove_sequence(editor.document.project(), "Монтаж");
    editor.commit("Удалить", &commands).unwrap();
    let p = editor.document.project();
    assert!(p.management.sequences.is_empty());
    assert!(sequence(p, "Монтаж").is_empty());
}
