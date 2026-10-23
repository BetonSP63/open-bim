//! «Копировать в буфер» / «Вставить» and «Пакетный редактор».
use open_bim_commands::{Command, DeletePolicy};
use open_bim_desktop::interaction::clipboard::Clipboard;
use open_bim_desktop::tools::Editor;
use open_bim_desktop::ui::batch::{Field, Operation, plan};
use open_bim_model::{ElementShape, ObjectId, PropertyValue};
use open_bim_renderer::DVec3;
use std::collections::BTreeSet;

fn editor() -> Editor {
    let project = open_bim_persistence::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/steel_release/source.obim"
    ))
    .unwrap();
    Editor::new(project).unwrap()
}

#[test]
fn pasted_copies_land_at_the_point_and_survive_deleting_the_originals() {
    let mut editor = editor();
    let p = editor.document.project();
    // Every part: bolts and welds between them come along.
    let parts: BTreeSet<ObjectId> = p.elements.keys().copied().collect();
    let details = p.details.len();
    assert!(Clipboard::copy(p, &BTreeSet::new()).is_none());
    let clip = Clipboard::copy(p, &parts).unwrap();
    assert_eq!(clip.len(), parts.len());
    // Delete the originals: the clipboard still pastes.
    for id in &parts {
        if editor.document.project().elements.contains_key(id) {
            let delete = Command::Delete {
                id: *id,
                policy: DeletePolicy::Cascade,
            };
            editor.commit("Удалить", &[delete]).unwrap();
        }
    }
    assert!(editor.document.project().elements.is_empty());
    let at = clip.base + DVec3::new(10_000., 0., 0.);
    let commands = clip.paste(editor.document.project(), at).unwrap();
    editor.commit("Вставить", &commands).unwrap();
    let p = editor.document.project();
    assert_eq!(p.elements.len(), parts.len());
    assert_eq!(p.details.len(), details, "cuts, bolts and welds pasted too");
    // The base of the first copied object is at the picked point.
    let first = *p.elements.keys().next().unwrap();
    let start = open_bim_renderer::overlay::handle_points(p, first)[0].1;
    assert!(start.distance(at) < 1e-6, "{start:?} {at:?}");
    // Pasting twice gives fresh ids.
    let again = clip.paste(p, at + DVec3::new(0., 10_000., 0.)).unwrap();
    editor.commit("Вставить", &again).unwrap();
    assert_eq!(editor.document.project().elements.len(), 2 * parts.len());
}

#[test]
fn batch_editor_sets_or_replaces_on_all_selected_parts_in_one_step() {
    let mut editor = editor();
    let p = editor.document.project();
    let parts: BTreeSet<ObjectId> = p
        .elements
        .iter()
        .filter(|(_, e)| matches!(e.shape, ElementShape::Member { .. }))
        .map(|(id, _)| *id)
        .take(3)
        .collect();
    // Replace text in names.
    let first = p.elements[parts.iter().next().unwrap()].name.clone();
    let find: String = first.chars().take(3).collect();
    let (rows, commands) = plan(p, &parts, &Field::Name, Operation::Replace, "Б-", &find).unwrap();
    assert!(!rows.is_empty());
    assert!(
        rows.iter()
            .all(|(_, old, new)| new == &old.replace(&find, "Б-"))
    );
    // Numbers are checked.
    assert!(plan(p, &parts, &Field::Start, Operation::Set, "abc", "").is_err());
    let (_, numbers) = plan(p, &parts, &Field::Start, Operation::Set, "10", "").unwrap();
    let depth = editor.document.history_depth().0;
    let mut all = commands;
    all.extend(numbers);
    editor.commit("Пакетно", &all).unwrap();
    assert_eq!(editor.document.history_depth().0, depth + 1);
    let p = editor.document.project();
    for id in &parts {
        assert_eq!(
            p.elements[id]
                .properties
                .get(open_bim_numbering::SERIES_START),
            Some(&PropertyValue::Number(10.))
        );
    }
    // Nothing to change: no rows.
    let (rows, _) = plan(p, &parts, &Field::Start, Operation::Set, "10", "").unwrap();
    assert!(rows.is_empty());
    // An empty value removes a property.
    let (_, remove) = plan(
        p,
        &parts,
        &Field::Property(open_bim_numbering::SERIES_START.into()),
        Operation::Set,
        "",
        "",
    )
    .unwrap();
    assert_eq!(remove.len(), parts.len());
}
