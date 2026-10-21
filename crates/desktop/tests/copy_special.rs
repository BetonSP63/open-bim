//! Copy special (linear N times, rotate with dZ, mirror, to another
//! plane), move special, Alt+P previous selection, the selection filter
//! (Ctrl+G), drag and drop, and the rollover switch.
use open_bim_desktop::interaction::Workplane;
use open_bim_desktop::interaction::selection::{PropertyFilter, wildcard};
use open_bim_desktop::interaction::special::{self, Dialog, MAX_COPIES, PickTarget, Special, Tab};
use open_bim_desktop::interaction::tools::default_project;
use open_bim_desktop::tools::{Editor, ui::Action};
use open_bim_desktop::ui::{Request, Workbench, commands::Cmd};
use open_bim_model::{DetailKind, ElementShape, ObjectId, Point3};
use open_bim_renderer::DVec3;
use std::collections::BTreeSet;

fn bench() -> Workbench {
    Workbench::new(Editor::new(default_project().unwrap()).unwrap())
}

fn apply(b: &mut Workbench, request: Option<Request>) -> bool {
    match request {
        Some(Request::Action(Action::Commit(label, commands))) => {
            b.editor.commit(&label, &commands).unwrap();
            true
        }
        Some(Request::Action(Action::Edit(edit))) => {
            b.editor.edit(&edit).unwrap();
            true
        }
        _ => false,
    }
}

fn beam(b: &mut Workbench, a: [f64; 3], c: [f64; 3]) -> ObjectId {
    b.execute(Cmd::ToolBeam);
    b.pick(DVec3::from_array(a));
    let r = b.pick(DVec3::from_array(c));
    assert!(apply(b, r));
    b.execute(Cmd::CancelTool);
    *b.editor.document.project().elements.keys().last().unwrap()
}

fn ends(b: &Workbench, id: ObjectId) -> (DVec3, DVec3) {
    let e = &b.editor.document.project().elements[&id];
    let ElementShape::Member { length, frame, .. } = &e.shape else {
        panic!()
    };
    (
        DVec3::from_array(frame.origin.0),
        DVec3::from_array(frame.world(Point3([0., 0., length.value()])).0),
    )
}

fn close(a: DVec3, b: DVec3) -> bool {
    (a - b).length() < 1e-6
}

fn ids(b: &Workbench) -> BTreeSet<ObjectId> {
    b.editor
        .document
        .project()
        .elements
        .keys()
        .copied()
        .collect()
}

#[test]
fn linear_copy_makes_n_copies_in_one_undo_step() {
    let mut b = bench();
    let id = beam(&mut b, [0., 0., 0.], [0., 6000., 0.]);
    let before = ids(&b);
    b.editor.selected = BTreeSet::from([id]);
    b.execute(Cmd::CopySpecial);
    assert_eq!(b.special_open, Some(true));
    b.special.tab = Tab::Linear;
    b.special.delta = ["6000".into(), "0".into(), "0".into()];
    b.special.copies = "4".into();
    let r = b.run_special(true);
    assert!(apply(&mut b, r));
    let new: Vec<_> = ids(&b).difference(&before).copied().collect();
    assert_eq!(new.len(), 4);
    let starts: BTreeSet<i64> = new.iter().map(|n| ends(&b, *n).0.x as i64).collect();
    assert_eq!(starts, BTreeSet::from([6000, 12000, 18000, 24000]));
    assert_eq!(
        b.editor.selected,
        BTreeSet::from([id]),
        "original stays selected"
    );
    // One undo removes all four copies.
    b.editor.history(false).unwrap();
    assert_eq!(ids(&b), before);
    // Comma decimals are accepted, zero and too many copies are refused.
    b.special.delta = ["1500,5".into(), "0".into(), "0".into()];
    b.special.copies = "0".into();
    assert!(b.run_special(true).is_none());
    b.special.copies = format!("{}", MAX_COPIES + 1);
    assert!(b.run_special(true).is_none());
    b.special.copies = "1".into();
    let r = b.run_special(true);
    assert!(apply(&mut b, r));
    let last = *ids(&b).last().unwrap();
    assert!((ends(&b, last).0.x - 1500.5).abs() < 1e-9);
}

#[test]
fn copy_brings_bolts_and_holes_of_the_copied_parts() {
    let mut b = bench();
    b.execute(Cmd::ToolPlate);
    for q in [[0., 0.], [400., 0.], [400., 400.], [0., 400.]] {
        b.pick(DVec3::new(q[0], q[1], 0.));
    }
    let r = b.execute(Cmd::FinishPicks);
    assert!(apply(&mut b, r));
    b.execute(Cmd::CancelTool);
    let plate = *ids(&b).last().unwrap();
    b.editor.selected = BTreeSet::from([plate]);
    b.execute(Cmd::ToolBolt);
    b.pick_object(Some(plate));
    b.execute(Cmd::FinishPicks);
    b.pick(DVec3::new(100., 200., 0.));
    let r = b.pick(DVec3::new(300., 200., 0.));
    assert!(apply(&mut b, r));
    let project = b.editor.document.project();
    let bolts = |p: &open_bim_model::Project| {
        p.details
            .values()
            .filter(|d| matches!(d.kind, DetailKind::BoltGroup { .. }))
            .count()
    };
    assert_eq!(bolts(project), 1);
    let (commands, created) = special::copy_commands(
        project,
        &BTreeSet::from([plate]),
        &Special::Rotate {
            pivot: DVec3::ZERO,
            axis: DVec3::Z,
            degrees: 90.,
            dz: 0.,
        },
        3,
    )
    .unwrap();
    assert_eq!(created.len(), 3);
    b.editor.commit("Копировать", &commands).unwrap();
    assert_eq!(
        bolts(b.editor.document.project()),
        4,
        "each copy has its bolt"
    );
}

#[test]
fn rotate_copy_turns_and_rises_each_copy() {
    let mut b = bench();
    let id = beam(&mut b, [1000., 0., 0.], [4000., 0., 0.]);
    let before = ids(&b);
    let op = Special::Rotate {
        pivot: DVec3::ZERO,
        axis: DVec3::Z,
        degrees: 90.,
        dz: 300.,
    };
    let (commands, _) =
        special::copy_commands(b.editor.document.project(), &BTreeSet::from([id]), &op, 3).unwrap();
    b.editor.commit("Копировать", &commands).unwrap();
    let new: Vec<_> = ids(&b).difference(&before).copied().collect();
    let got: Vec<(DVec3, DVec3)> = new.iter().map(|n| ends(&b, *n)).collect();
    let want = [
        (DVec3::new(0., 1000., 300.), DVec3::new(0., 4000., 300.)),
        (DVec3::new(-1000., 0., 600.), DVec3::new(-4000., 0., 600.)),
        (DVec3::new(0., -1000., 900.), DVec3::new(0., -4000., 900.)),
    ];
    for ((a, e), (wa, we)) in got.iter().zip(want) {
        assert!(close(*a, wa) && close(*e, we), "{a:?} {e:?}");
    }
    // Move special with the same settings turns the original once.
    let commands =
        special::move_commands(b.editor.document.project(), &BTreeSet::from([id]), &op).unwrap();
    b.editor.commit("Переместить", &commands).unwrap();
    let (a, e) = ends(&b, id);
    assert!(close(a, DVec3::new(0., 1000., 300.)) && close(e, DVec3::new(0., 4000., 300.)));
}

#[test]
fn mirror_copy_from_the_dialog_and_from_picks() {
    let mut b = bench();
    let id = beam(&mut b, [1000., 0., 0.], [1000., 6000., 0.]);
    b.editor.selected = BTreeSet::from([id]);
    b.execute(Cmd::CopySpecial);
    b.special.tab = Tab::Mirror;
    // Mirror line x = 5000 by two picks on the work plane.
    b.special.start_pick(PickTarget::MirrorLine);
    assert!(b.picking() && b.prompt().contains("линии зеркала"));
    assert!(b.pick(DVec3::new(5000., 0., 0.)).is_none());
    assert!(b.pick(DVec3::new(5000., 3000., 0.)).is_none());
    assert!(!b.picking());
    assert_eq!(
        b.special.mirror_origin,
        ["5000".to_string(), "0".to_string()]
    );
    assert_eq!(b.special.mirror_angle, "90");
    let before = ids(&b);
    let r = b.run_special(true);
    assert!(apply(&mut b, r));
    let new = *ids(&b).difference(&before).next().unwrap();
    let (a, e) = ends(&b, new);
    assert!(
        (a.x - 9000.).abs() < 1e-6 && (e.x - 9000.).abs() < 1e-6,
        "{a:?}"
    );
    let ys = [a.y.min(e.y), a.y.max(e.y)];
    assert!((ys[0]).abs() < 1e-6 && (ys[1] - 6000.).abs() < 1e-6);
    assert_eq!(ends(&b, id).0.x, 1000., "original untouched");
    // Mirror gives one copy whatever the count says.
    b.special.copies = "5".into();
    assert_eq!(b.special.copies().unwrap(), 1);
}

#[test]
fn copy_to_another_plane_keeps_the_position_relative_to_the_planes() {
    let mut b = bench();
    // A beam lying 500 mm along X from the origin of the source plane.
    let id = beam(&mut b, [500., 0., 0.], [2500., 0., 0.]);
    b.editor.selected = BTreeSet::from([id]);
    b.execute(Cmd::CopySpecial);
    b.special.tab = Tab::ToPlane;
    assert!(b.run_special(true).is_none(), "planes are required");
    b.special.start_pick(PickTarget::Planes);
    // Source: the global XY plane. Target: vertical plane at y = 3000 whose
    // X axis is the global Y and whose Y axis is the global Z.
    for q in [
        [0., 0., 0.],
        [1000., 0., 0.],
        [0., 1000., 0.],
        [0., 3000., 0.],
        [0., 4000., 0.],
        [0., 3000., 1000.],
    ] {
        assert!(b.pick(DVec3::from_array(q)).is_none());
    }
    assert!(b.special.planes.is_some() && !b.picking());
    let before = ids(&b);
    let r = b.run_special(true);
    assert!(apply(&mut b, r));
    let new = *ids(&b).difference(&before).next().unwrap();
    let (a, e) = ends(&b, new);
    assert!(close(a, DVec3::new(0., 3500., 0.)), "{a:?}");
    assert!(close(e, DVec3::new(0., 5500., 0.)), "{e:?}");
}

#[test]
fn to_plane_maps_frames_exactly() {
    let from = Workplane::default();
    let to = Workplane::from_points(
        DVec3::new(100., 200., 300.),
        DVec3::new(100., 200., 1300.),
        DVec3::new(1100., 200., 300.),
    )
    .unwrap();
    let op = Special::ToPlane { from, to };
    let open_bim_desktop::tools::Edit::Transform(f) = op.step(1).unwrap() else {
        panic!()
    };
    let w = |p: DVec3| DVec3::from_array(f.world(Point3(p.to_array())).0);
    assert!(close(w(DVec3::ZERO), to.origin));
    assert!(close(w(DVec3::X), to.origin + to.axes[0]));
    assert!(close(w(DVec3::Y), to.origin + to.axes[1]));
    assert!(close(w(DVec3::Z), to.origin + to.axes[2]));
}

#[test]
fn linear_vector_by_two_picks_and_move_special() {
    let mut b = bench();
    let id = beam(&mut b, [0., 0., 0.], [6000., 0., 0.]);
    b.editor.selected = BTreeSet::from([id]);
    b.execute(Cmd::MoveSpecial);
    assert_eq!(b.special_open, Some(false));
    b.special.tab = Tab::Linear;
    b.special.start_pick(PickTarget::Vector);
    b.pick(DVec3::new(0., 0., 0.));
    // Esc-free undo of a pick: Backspace removes the last point.
    b.execute(Cmd::UndoPick);
    assert!(b.special.picks.is_empty());
    b.pick(DVec3::new(1000., 1000., 0.));
    b.pick(DVec3::new(1000., 4000., 0.));
    assert_eq!(
        b.special.delta,
        ["0".to_string(), "3000".to_string(), "0".to_string()]
    );
    let r = b.run_special(false);
    assert!(apply(&mut b, r));
    let (a, _) = ends(&b, id);
    assert!(close(a, DVec3::new(0., 3000., 0.)));
    // Esc cancels a pick in progress.
    b.special.start_pick(PickTarget::Pivot);
    b.execute(Cmd::CancelTool);
    assert!(b.special.picking.is_none());
    assert_eq!(
        b.editor.selected,
        BTreeSet::from([id]),
        "Esc cancelled the pick only"
    );
}

#[test]
fn dialog_defaults_parse_and_report_errors() {
    let d = Dialog::default();
    let wp = Workplane::default();
    assert!(matches!(d.special(&wp).unwrap(), Special::Linear { .. }));
    assert_eq!(d.copies().unwrap(), 1);
    let mut d = Dialog {
        tab: Tab::Rotate,
        angle: "0".into(),
        ..Dialog::default()
    };
    assert!(
        d.special(&wp).unwrap().step(1).is_err(),
        "no angle and no dZ"
    );
    d.angle = "abc".into();
    assert!(d.special(&wp).is_err());
}

#[test]
fn alt_p_returns_to_the_previous_selection() {
    let mut b = bench();
    let one = beam(&mut b, [0., 0., 0.], [6000., 0., 0.]);
    let two = beam(&mut b, [0., 3000., 0.], [6000., 3000., 0.]);
    use open_bim_desktop::interaction::selection::Click;
    b.select_picked(Some(one), Click::Replace);
    b.select_picked(Some(two), Click::Add);
    b.select_picked(Some(two), Click::Replace);
    b.execute(Cmd::SelectPrevious);
    assert_eq!(b.editor.selected, BTreeSet::from([one, two]));
    // Alt+P again swaps back (toggles between the last two).
    b.execute(Cmd::SelectPrevious);
    assert_eq!(b.editor.selected, BTreeSet::from([two]));
    // Esc clears; Alt+P brings the cleared selection back.
    b.execute(Cmd::CancelTool);
    assert!(b.editor.selected.is_empty());
    b.execute(Cmd::SelectPrevious);
    assert_eq!(b.editor.selected, BTreeSet::from([two]));
    let spec = open_bim_desktop::ui::commands::ALL
        .iter()
        .find(|s| s.cmd == Cmd::SelectPrevious)
        .unwrap();
    assert_eq!(
        spec.shortcut,
        Some(eframe::egui::KeyboardShortcut::new(
            eframe::egui::Modifiers::ALT,
            eframe::egui::Key::P
        ))
    );
}

#[test]
fn selection_filter_limits_clicks_area_and_select_all() {
    assert!(wildcard("I20*", "I20Б1"));
    assert!(!wildcard("I20*", "I30Б1"));
    assert!(wildcard("б1", "I20Б1"), "substring, case ignored");
    assert!(wildcard("?20*", "I20Б1"));
    assert!(!wildcard("I20", "I3"));
    let mut b = bench();
    let beam_id = beam(&mut b, [0., 0., 0.], [6000., 0., 0.]);
    b.execute(Cmd::ToolColumn);
    let r = b.pick(DVec3::new(0., 3000., 0.));
    assert!(apply(&mut b, r));
    b.execute(Cmd::CancelTool);
    let column = *ids(&b).last().unwrap();
    let project = b.editor.document.project();
    let name = project.elements[&beam_id].name.clone();
    assert_ne!(name, project.elements[&column].name);
    b.execute(Cmd::SelectionFilter);
    assert!(b.filter_open);
    b.filter = PropertyFilter {
        enabled: true,
        name: name.clone(),
        ..Default::default()
    };
    use open_bim_desktop::interaction::selection::Click;
    b.select_picked(Some(column), Click::Replace);
    assert!(b.editor.selected.is_empty(), "the column is filtered out");
    b.select_picked(Some(beam_id), Click::Replace);
    assert_eq!(b.editor.selected, BTreeSet::from([beam_id]));
    b.execute(Cmd::SelectAll);
    let project = b.editor.document.project();
    assert!(
        b.editor
            .selected
            .iter()
            .filter(|id| project.elements.contains_key(id))
            .all(|id| project.elements[id].name.contains(&name)),
        "select all honours the filter"
    );
    // Switched off, the filter lets everything through.
    b.filter.enabled = false;
    assert!(b.filter.allows(project, column));
}

#[test]
fn drag_and_drop_moves_or_copies_the_selection() {
    let mut b = bench();
    let id = beam(&mut b, [0., 0., 0.], [6000., 0., 0.]);
    b.editor.selected = BTreeSet::from([id]);
    let r = b.drop_selection(
        DVec3::new(6000., 0., 0.),
        DVec3::new(6000., 2000., 0.),
        false,
    );
    assert!(apply(&mut b, r));
    assert!(close(ends(&b, id).0, DVec3::new(0., 2000., 0.)));
    let before = ids(&b);
    let r = b.drop_selection(DVec3::ZERO, DVec3::new(0., 3000., 0.), true);
    assert!(apply(&mut b, r));
    assert_eq!(ids(&b).len(), before.len() + 1);
    assert!(
        b.drop_selection(DVec3::ZERO, DVec3::ZERO, false).is_none(),
        "a click is not a drop"
    );
    b.execute(Cmd::ToggleDragDrop);
    assert!(!b.drag_drop);
    b.execute(Cmd::ToggleRollover);
    assert!(!b.rollover);
}
