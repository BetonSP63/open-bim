//! «Положение» of beams and columns: the profile is placed relative to
//! the picked reference line, handles stay on that line, and changing the
//! profile or the position keeps the line.
use open_bim_desktop::interaction::selection::move_handle;
use open_bim_desktop::interaction::tools::default_project;
use open_bim_desktop::tools::{Editor, ui::Action};
use open_bim_desktop::ui::property_pane::{PropertyPane, common, modify};
use open_bim_desktop::ui::{Request, Workbench, commands::Cmd};
use open_bim_model::{AtDepth, ElementShape, ObjectId, OnPlane, PartPosition, Point3};
use open_bim_renderer::DVec3;
use open_bim_renderer::overlay::{Handle, HandleKind};
use std::collections::BTreeSet;

const I300: ObjectId = ObjectId(2);
const C200: ObjectId = ObjectId(4);

fn bench() -> Workbench {
    Workbench::new(Editor::new(default_project().unwrap()).unwrap())
}

fn beam(b: &mut Workbench, position: PartPosition) -> ObjectId {
    b.execute(Cmd::ToolBeam);
    b.editor.draft.profile = I300;
    b.editor.draft.position = position;
    b.pick(DVec3::new(0., 0., 3600.));
    match b.pick(DVec3::new(6000., 0., 3600.)) {
        Some(Request::Action(Action::Commit(label, commands))) => {
            b.editor.commit(&label, &commands).unwrap();
        }
        _ => panic!("no beam"),
    }
    b.execute(Cmd::CancelTool);
    *b.editor.document.project().elements.keys().last().unwrap()
}

fn axis(b: &Workbench, id: ObjectId) -> ([f64; 3], f64) {
    match &b.editor.document.project().elements[&id].shape {
        ElementShape::Member { frame, length, .. } => (frame.origin.0, length.value()),
        _ => panic!(),
    }
}

fn line(b: &Workbench, id: ObjectId) -> (Point3, Point3) {
    let project = b.editor.document.project();
    let (s, e, _) = project
        .member_reference_line(&project.elements[&id].shape)
        .unwrap();
    (s, e)
}

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() < 1e-6)
}

#[test]
fn beam_behind_hangs_below_the_picked_line() {
    let mut b = bench();
    let id = beam(
        &mut b,
        PartPosition {
            depth: AtDepth::Behind,
            ..Default::default()
        },
    );
    let (origin, length) = axis(&b, id);
    assert!(near(origin, [0., 0., 3450.]), "axis {origin:?}");
    assert_eq!(length, 6000.);
    let (s, e) = line(&b, id);
    assert!(near(s.0, [0., 0., 3600.]) && near(e.0, [6000., 0., 3600.]));
}

#[test]
fn pane_changes_position_and_profile_on_the_same_reference_line() {
    let mut b = bench();
    let id = beam(
        &mut b,
        PartPosition {
            depth: AtDepth::Behind,
            ..Default::default()
        },
    );
    let selected = BTreeSet::from([id]);
    let mut pane = PropertyPane::default();
    let c = common(b.editor.document.project(), &selected).unwrap();
    pane.sync(&c);
    assert_eq!(pane.position.depth, Some(AtDepth::Behind));
    pane.position.plane = Some(OnPlane::Left);
    pane.position.start_dx = "100".into();
    pane.position.end_dx = "-50".into();
    pane.profile = Some(C200);
    let changes = pane.changes(&c).unwrap();
    let commands = modify(b.editor.document.project(), &c.parts, &changes).unwrap();
    b.editor.commit("Изменить свойства", &commands).unwrap();
    // C 200×75: top flange on the line, 37.5 mm to the left (+Y), start
    // shortened by 100 and end by 50.
    let (origin, length) = axis(&b, id);
    assert!(near(origin, [100., 37.5, 3500.]), "axis {origin:?}");
    assert!((length - 5850.).abs() < 1e-9);
    let (s, e) = line(&b, id);
    assert!(near(s.0, [0., 0., 3600.]) && near(e.0, [6000., 0., 3600.]));
    // A bad number is reported, not ignored.
    pane.sync(&common(b.editor.document.project(), &selected).unwrap());
    pane.position.depth_offset = "abc".into();
    let c = common(b.editor.document.project(), &selected).unwrap();
    assert!(pane.changes(&c).is_err());
    // Undo restores the first placement.
    b.editor.history(false).unwrap();
    assert!(near(axis(&b, id).0, [0., 0., 3450.]));
}

#[test]
fn end_handle_moves_the_reference_line_and_keeps_the_position() {
    let mut b = bench();
    let id = beam(
        &mut b,
        PartPosition {
            depth: AtDepth::Behind,
            end_dx: 200.,
            ..Default::default()
        },
    );
    let handle = Handle {
        object: id,
        kind: HandleKind::End,
        world: DVec3::new(6000., 0., 3600.),
        screen: [0., 0.],
    };
    let commands = move_handle(
        b.editor.document.project(),
        &handle,
        DVec3::new(7000., 0., 3600.),
    )
    .unwrap();
    b.editor.commit("Переместить ручку", &commands).unwrap();
    let (s, e) = line(&b, id);
    assert!(near(s.0, [0., 0., 3600.]) && near(e.0, [7000., 0., 3600.]));
    let (origin, length) = axis(&b, id);
    assert!(near(origin, [0., 0., 3450.]));
    assert!((length - 7200.).abs() < 1e-9, "Dx kept: {length}");
}
