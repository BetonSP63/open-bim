//! Tekla-style selection and direct editing: click modes, window/crossing
//! area selection with selection switches, handle dragging, move/copy/mirror/
//! rotate by picks, delete with a dependency prompt, context menu commands,
//! Tab snap cycling and the temporary reference point.
use open_bim_desktop::interaction::Workplane;
use open_bim_desktop::interaction::selection::{self, Click, Filter};
use open_bim_desktop::interaction::snap::{Snapper, Switches, View};
use open_bim_desktop::interaction::tools::default_project;
use open_bim_desktop::tools::{Editor, ui::Action};
use open_bim_desktop::ui::{Request, Workbench, commands::Cmd};
use open_bim_model::{ElementShape, ObjectId, Point3};
use open_bim_renderer::overlay::{HandleKind, Projector, handles};
use open_bim_renderer::{Bounds, Camera, DVec3, NamedView, Scene};
use std::collections::BTreeSet;
use std::sync::Arc;

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

fn top_camera() -> Camera {
    let mut c = Camera::default();
    c.set_view(NamedView::Top);
    c.fit(Bounds {
        min: DVec3::new(-2000., -2000., 0.),
        max: DVec3::new(26000., 20000., 0.),
    });
    c
}

const SIZE: [f64; 2] = [1200., 800.];

fn projector(c: &Camera) -> Projector<'_> {
    Projector {
        camera: c,
        origin: DVec3::ZERO,
        min: [0., 0.],
        size: SIZE,
    }
}

#[test]
fn window_selects_inside_crossing_selects_touching_and_switches_filter() {
    let mut b = bench();
    let inside = beam(&mut b, [1000., 1000., 0.], [5000., 1000., 0.]);
    let partly = beam(&mut b, [4000., 3000., 0.], [14000., 3000., 0.]);
    let far = beam(&mut b, [20000., 15000., 0.], [24000., 15000., 0.]);
    let c = top_camera();
    let p = projector(&c);
    let s = |x: f64, y: f64| p.screen(DVec3::new(x, y, 0.)).unwrap();
    let (lo, hi) = (s(0., -500.), s(8000., 4000.));
    let (left, right) = ([lo[0], hi[1]], [hi[0], lo[1]]);
    let project = b.editor.document.project().clone();
    let none = BTreeSet::new();
    let parts = Filter {
        grids: false,
        ..Filter::default()
    };
    // Left → right: window, fully inside only.
    let window = selection::area(&project, &p, &none, left, right, parts);
    assert_eq!(window, BTreeSet::from([inside]));
    // Right → left: crossing, everything touching.
    let crossing = selection::area(&project, &p, &none, right, left, parts);
    assert_eq!(crossing, BTreeSet::from([inside, partly]));
    assert!(!crossing.contains(&far));
    // Switches: parts off → nothing; grids on → grid lines crossing the box.
    let off = Filter {
        parts: false,
        assemblies: false,
        ..parts
    };
    assert!(selection::area(&project, &p, &none, right, left, off).is_empty());
    let grids = Filter {
        parts: false,
        grids: true,
        ..parts
    };
    let lines: BTreeSet<String> = selection::area(&project, &p, &none, right, left, grids)
        .iter()
        .map(|id| project.axes[id].name.clone())
        .collect();
    assert_eq!(lines, BTreeSet::from(["1".into(), "2".into(), "А".into()]));
    // Hidden objects are never selected.
    let hidden = BTreeSet::from([inside]);
    assert_eq!(
        selection::area(&project, &p, &hidden, right, left, parts),
        BTreeSet::from([partly])
    );
    // Workbench: Shift adds, Ctrl toggles, Esc clears.
    b.select_area(&p, left, right, Click::Replace);
    assert_eq!(b.editor.selected, BTreeSet::from([inside]));
    b.select_picked(Some(far), Click::Add);
    assert_eq!(b.editor.selected, BTreeSet::from([inside, far]));
    b.select_picked(Some(inside), Click::Toggle);
    assert_eq!(b.editor.selected, BTreeSet::from([far]));
    b.execute(Cmd::CancelTool);
    assert!(b.editor.selected.is_empty(), "Esc clears the selection");
}

#[test]
fn dragging_a_beam_end_handle_to_a_snapped_node_moves_only_that_end() {
    let mut b = bench();
    let id = beam(&mut b, [0., 0., 0.], [6000., 0., 0.]);
    let (start, _) = ends(&b, id);
    let c = top_camera();
    let p = projector(&c);
    let selected = BTreeSet::from([id]);
    let project = b.editor.document.project().clone();
    let hs = handles(&project, &selected, &p);
    let end = selection::handle_at(&hs, p.screen(DVec3::new(6000., 0., 0.)).unwrap(), 8.).unwrap();
    assert_eq!(end.kind, HandleKind::End);
    assert_eq!(selection::anchor(&project, &end), Some(start));
    // Snap the target to grid node 3/Б like the viewport does.
    let scene = Arc::new(Scene::from_project(&project).unwrap());
    let view = View {
        camera: &c,
        origin: scene.origin,
        size: SIZE,
    };
    let s = view.screen(DVec3::new(12000., 6000., 0.)).unwrap();
    let hit = Snapper::default()
        .snap(
            &project,
            &scene,
            view,
            [s[0] + 3., s[1] + 2.],
            Switches::default(),
            12.,
            Some(start),
            &Workplane::default(),
        )
        .unwrap();
    let commands = selection::move_handle(&project, &end, hit.point).unwrap();
    b.editor.commit("Переместить ручку", &commands).unwrap();
    let (a, e) = ends(&b, id);
    assert!((a - start).length() <= 1e-9, "start unchanged");
    assert!((e - DVec3::new(12000., 6000., 0.)).length() <= 1e-6);
    b.editor.history(false).unwrap();
    let (a, e) = ends(&b, id);
    assert!((a - start).length() <= 1e-9 && (e - DVec3::new(6000., 0., 0.)).length() <= 1e-9);
}

#[test]
fn plate_vertex_handle_moves_the_vertex() {
    let mut b = bench();
    b.execute(Cmd::ToolPlate);
    for q in [[0., 0.], [4000., 0.], [4000., 3000.], [0., 3000.]] {
        b.pick(DVec3::new(q[0], q[1], 0.));
    }
    let r = b.execute(Cmd::FinishPicks);
    assert!(apply(&mut b, r));
    let id = *b.editor.document.project().elements.keys().last().unwrap();
    let c = top_camera();
    let p = projector(&c);
    let project = b.editor.document.project().clone();
    let hs = handles(&project, &BTreeSet::from([id]), &p);
    let corner = hs
        .iter()
        .find(|h| (h.world - DVec3::new(4000., 3000., 0.)).length() < 1e-9)
        .unwrap();
    let commands = selection::move_handle(&project, corner, DVec3::new(5000., 3500., 0.)).unwrap();
    b.editor.commit("Ручка", &commands).unwrap();
    let ElementShape::Plate { contour, .. } = &b.editor.document.project().elements[&id].shape
    else {
        panic!()
    };
    assert!(contour.contains(&[5000., 3500.]));
    assert!(contour.contains(&[4000., 0.]), "other vertices stay");
}

#[test]
fn move_copy_mirror_and_rotate_by_picks() {
    let mut b = bench();
    let id = beam(&mut b, [0., 0., 0.], [6000., 0., 0.]);
    b.editor.selected = BTreeSet::from([id]);
    b.execute(Cmd::MoveByPicks);
    assert!(b.picking() && b.prompt().contains("исходную"));
    let r = b.pick(DVec3::ZERO);
    assert!(!apply(&mut b, r));
    let r = b.pick(DVec3::new(1000., 0., 0.));
    assert!(apply(&mut b, r));
    let (a, e) = ends(&b, id);
    assert_eq!(
        (a, e),
        (DVec3::new(1000., 0., 0.), DVec3::new(7000., 0., 0.))
    );
    assert!(!b.picking(), "move finishes after one destination");

    // Copy repeats from the same origin: two copies with new IDs.
    let before: BTreeSet<ObjectId> = b
        .editor
        .document
        .project()
        .elements
        .keys()
        .copied()
        .collect();
    b.editor.selected = BTreeSet::from([id]);
    b.execute(Cmd::CopyByPicks);
    b.pick(DVec3::ZERO);
    let r = b.pick(DVec3::new(0., 6000., 0.));
    assert!(apply(&mut b, r));
    b.editor.selected = BTreeSet::from([id]);
    let r = b.pick(DVec3::new(0., 12000., 0.));
    assert!(apply(&mut b, r));
    b.execute(Cmd::CancelTool);
    let after: BTreeSet<ObjectId> = b
        .editor
        .document
        .project()
        .elements
        .keys()
        .copied()
        .collect();
    let new: Vec<_> = after.difference(&before).copied().collect();
    assert_eq!(new.len(), 2);
    let starts: BTreeSet<[i64; 3]> = new
        .iter()
        .map(|n| ends(&b, *n).0.to_array().map(|v| v as i64))
        .collect();
    assert_eq!(starts, BTreeSet::from([[1000, 6000, 0], [1000, 12000, 0]]));

    // Mirror about the line x = 10000 (two picks on the work plane).
    b.editor.selected = BTreeSet::from([id]);
    b.execute(Cmd::MirrorByPicks);
    b.pick(DVec3::new(10000., 0., 0.));
    let r = b.pick(DVec3::new(10000., 5000., 0.));
    assert!(apply(&mut b, r));
    // The mirrored member spans x 13000…19000 (the model keeps the start at the
    // lower x so the profile stays right-handed).
    let (a, e) = ends(&b, id);
    let xs = [a.x.min(e.x), a.x.max(e.x)];
    assert!(
        (xs[0] - 13000.).abs() < 1e-6 && (xs[1] - 19000.).abs() < 1e-6,
        "{a:?} {e:?}"
    );

    // Rotate 90° about the start: centre, from, to.
    b.editor.selected = BTreeSet::from([id]);
    b.execute(Cmd::RotateByPicks);
    b.pick(a);
    b.pick(a + (e - a).normalize() * 1000.);
    let r = b.pick(a + DVec3::new(0., -1000., 0.));
    assert!(apply(&mut b, r));
    let start = a;
    let (a, e) = ends(&b, id);
    assert!((a - start).length() < 1e-6);
    assert!(
        (e - (start + DVec3::new(0., -6000., 0.))).length() < 1e-6,
        "{e:?}"
    );
}

#[test]
fn delete_with_dependants_asks_first() {
    let mut b = bench();
    let plain = beam(&mut b, [0., 0., 0.], [6000., 0., 0.]);
    b.editor.selected = BTreeSet::from([plain]);
    let r = b.execute(Cmd::DeleteSelected);
    assert!(apply(&mut b, r), "no dependants: deleted at once");
    // A plate with a bolt group: holes and the bolt depend on it.
    b.execute(Cmd::ToolPlate);
    for q in [[0., 0.], [400., 0.], [400., 400.], [0., 400.]] {
        b.pick(DVec3::new(q[0], q[1], 0.));
    }
    let r = b.execute(Cmd::FinishPicks);
    apply(&mut b, r);
    b.execute(Cmd::CancelTool);
    let plate = *b.editor.document.project().elements.keys().last().unwrap();
    b.execute(Cmd::ToolBolt);
    b.pick_object(Some(plate));
    b.execute(Cmd::FinishPicks);
    b.pick(DVec3::new(100., 100., 0.));
    let r = b.pick(DVec3::new(300., 100., 0.));
    assert!(apply(&mut b, r));
    b.editor.selected = BTreeSet::from([plate]);
    assert!(b.execute(Cmd::DeleteSelected).is_none());
    let (ids, total) = b.delete_confirm.clone().expect("confirmation asked");
    assert_eq!(ids, BTreeSet::from([plate]));
    assert!(total > 1);
    b.execute(Cmd::CancelTool);
    assert!(b.delete_confirm.is_none());
    assert!(
        b.editor.document.project().elements.contains_key(&plate),
        "Esc keeps the plate"
    );
}

#[test]
fn context_menu_commands_all_act() {
    let mut b = bench();
    let id = beam(&mut b, [0., 0., 0.], [6000., 0., 0.]);
    for cmd in [
        Cmd::PanelProperties,
        Cmd::MoveByPicks,
        Cmd::CopyByPicks,
        Cmd::DeleteSelected,
        Cmd::HideSelected,
        Cmd::ShowOnlySelected,
        Cmd::ShowAll,
        Cmd::FitSelected,
        Cmd::PartDrawing,
        Cmd::CancelTool,
        Cmd::FinishPicks,
    ] {
        b.editor.selected = BTreeSet::from([id]);
        b.tools.message = None;
        let before = (
            b.side_panel,
            b.hidden.len(),
            b.picking(),
            b.editor.events.len(),
            b.mode,
        );
        let r = b.execute(cmd);
        let after = (
            b.side_panel,
            b.hidden.len(),
            b.picking(),
            b.editor.events.len(),
            b.mode,
        );
        assert!(
            r.is_some() || before != after || b.tools.message.is_some(),
            "{cmd:?}"
        );
        b.execute(Cmd::CancelTool);
        b.edit_pick.cancel();
    }
}

#[test]
fn tab_cycles_snap_candidates_and_ctrl_pick_sets_a_reference() {
    let mut b = bench();
    beam(&mut b, [0., 6000., 0.], [6000., 6000., 0.]);
    let project = b.editor.document.project().clone();
    let scene = Arc::new(Scene::from_project(&project).unwrap());
    let c = top_camera();
    let view = View {
        camera: &c,
        origin: scene.origin,
        size: SIZE,
    };
    let s = view.screen(DVec3::new(6000., 6000., 0.)).unwrap();
    let all = Snapper::default().candidates(
        &project,
        &scene,
        view,
        s,
        Switches::default(),
        12.,
        None,
        &Workplane::default(),
    );
    assert!(
        all.len() >= 2,
        "beam end, grid node and flange corners: {}",
        all.len()
    );
    let distinct: BTreeSet<[i64; 3]> = all
        .iter()
        .map(|h| h.point.to_array().map(|v| (v * 1000.) as i64))
        .collect();
    assert_eq!(distinct.len(), all.len(), "candidates are distinct points");
    // Temporary reference point: relative input starts from it.
    b.execute(Cmd::ToolBeam);
    b.engine.reference = Some(DVec3::new(6000., 6000., 0.));
    b.pick_numeric("1000,0,0");
    assert_eq!(b.engine.picks, vec![DVec3::new(7000., 6000., 0.)]);
    assert!(b.engine.reference.is_none(), "cleared by the pick");
}
