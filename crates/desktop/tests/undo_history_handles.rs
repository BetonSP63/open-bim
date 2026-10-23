//! Undo history list (several steps at once) and picked handles: «Переместить»
//! with handles picked moves only those points.
use open_bim_desktop::tools::{Edit, Editor};
use open_bim_model::{ElementShape, ObjectId, Point3};
use open_bim_renderer::DVec3;
use open_bim_renderer::overlay::{HandleKind, handle_points};

fn editor() -> Editor {
    let project = open_bim_persistence::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/steel_release/source.obim"
    ))
    .unwrap();
    Editor::new(project).unwrap()
}

fn first_member(editor: &Editor) -> ObjectId {
    *editor
        .document
        .project()
        .elements
        .iter()
        .find(|(_, e)| matches!(e.shape, ElementShape::Member { .. }))
        .unwrap()
        .0
}

fn points(editor: &Editor, id: ObjectId) -> (DVec3, DVec3) {
    let p = handle_points(editor.document.project(), id);
    let get = |k: HandleKind| p.iter().find(|(kind, _)| *kind == k).unwrap().1;
    (get(HandleKind::Start), get(HandleKind::End))
}

#[test]
fn history_list_undoes_and_redoes_several_steps_at_once() {
    let mut editor = editor();
    let id = first_member(&editor);
    editor.selected = [id].into();
    for k in 1..=3 {
        editor
            .edit(&Edit::Move(Point3([0., 100. * k as f64, 0.])))
            .unwrap();
    }
    let (undo, redo) = editor.document.history_labels();
    assert_eq!(undo.len(), 3);
    assert!(redo.is_empty());
    let start = points(&editor, id).0;
    editor.history_steps(false, 2).unwrap();
    assert_eq!(editor.document.history_depth(), (1, 2));
    assert!(
        points(&editor, id)
            .0
            .distance(start - DVec3::new(0., 500., 0.))
            < 1e-6
    );
    editor.history_steps(true, 2).unwrap();
    assert_eq!(editor.document.history_depth(), (3, 0));
    assert!(points(&editor, id).0.distance(start) < 1e-6);
    // More steps than there are: refused, nothing changes.
    assert!(editor.history_steps(true, 1).is_err());
    assert_eq!(editor.document.history_depth(), (3, 0));
}

#[test]
fn move_with_picked_handles_moves_only_those_points_in_one_step() {
    let mut editor = editor();
    let id = first_member(&editor);
    editor.selected = [id].into();
    let (start, end) = points(&editor, id);
    editor.handles = vec![(id, HandleKind::End)];
    assert_eq!(editor.selected_handles().len(), 1);
    let depth = editor.document.history_depth().0;
    editor.edit(&Edit::Move(Point3([0., 0., 250.]))).unwrap();
    assert_eq!(editor.document.history_depth().0, depth + 1);
    let (s2, e2) = points(&editor, id);
    assert!(s2.distance(start) < 1e-6, "{s2:?} {start:?}");
    assert!(e2.distance(end + DVec3::new(0., 0., 250.)) < 1e-6, "{e2:?}");
    // Both handles: the whole member moves.
    editor.handles = vec![(id, HandleKind::Start), (id, HandleKind::End)];
    editor.edit(&Edit::Move(Point3([100., 0., 0.]))).unwrap();
    let (s3, e3) = points(&editor, id);
    assert!(s3.distance(s2 + DVec3::X * 100.) < 1e-6);
    assert!(e3.distance(e2 + DVec3::X * 100.) < 1e-6);
    // A handle of an object that is no longer selected does not count:
    // the whole selection moves again.
    let other = *editor
        .document
        .project()
        .elements
        .keys()
        .find(|k| **k != id)
        .unwrap();
    editor.selected = [other].into();
    assert!(editor.selected_handles().is_empty());
}
