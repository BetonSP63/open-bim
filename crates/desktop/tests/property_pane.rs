//! Property pane, model views and related mechanics: «— разные —» for
//! mixed values, one-transaction modify with undo, grid views with camera
//! direction and view depth, per-view work plane, model search, measuring,
//! the geometry snap switch and grid nodes at every level.
use open_bim_desktop::interaction::Workplane;
use open_bim_desktop::interaction::snap::{SnapKind, Snapper, Switches, View};
use open_bim_desktop::interaction::tools::with_default_grid;
use open_bim_desktop::tools::{Editor, ui::Action};
use open_bim_desktop::ui::property_pane::{Changes, PropertyPane, Shared, common, modify};
use open_bim_desktop::ui::views::{ViewKind, grid_views};
use open_bim_desktop::ui::{Request, Workbench, commands::Cmd};
use open_bim_model::{ElementShape, ObjectId};
use open_bim_renderer::{Bounds, Camera, DVec3, Scene};
use std::collections::BTreeSet;
use std::sync::Arc;

/// The analytic test sections these geometry checks are written for
/// (profile 2 is I 300×150×8×12), with the default grid; new models in the
/// app start with ГОСТ sections instead.
fn default_project() -> Result<open_bim_model::Project, String> {
    with_default_grid(open_bim_model::Project::default())
}

fn bench() -> Workbench {
    Workbench::new(Editor::new(default_project().unwrap()).unwrap())
}

fn apply(b: &mut Workbench, request: Option<Request>) -> bool {
    match request {
        Some(Request::Action(Action::Commit(label, commands))) => {
            b.editor.commit(&label, &commands).unwrap();
            true
        }
        _ => false,
    }
}

fn beam(b: &mut Workbench, profile: ObjectId, y: f64) -> ObjectId {
    b.execute(Cmd::ToolBeam);
    b.editor.draft.profile = profile;
    b.pick(DVec3::new(0., y, 3600.));
    let r = b.pick(DVec3::new(6000., y, 3600.));
    assert!(apply(b, r));
    b.execute(Cmd::CancelTool);
    *b.editor.document.project().elements.keys().last().unwrap()
}

fn profile_of(b: &Workbench, id: ObjectId) -> ObjectId {
    match &b.editor.document.project().elements[&id].shape {
        ElementShape::Member { profile, .. } => *profile,
        _ => panic!(),
    }
}

#[test]
fn mixed_profiles_show_varies_and_modify_is_one_undoable_transaction() {
    let mut b = bench();
    let (i300, angle, channel) = (ObjectId(2), ObjectId(3), ObjectId(4));
    let ids = [
        beam(&mut b, i300, 0.),
        beam(&mut b, angle, 6000.),
        beam(&mut b, i300, 12000.),
    ];
    let selected: BTreeSet<ObjectId> = ids.into();
    let project = b.editor.document.project();
    let c = common(project, &selected).unwrap();
    assert_eq!(c.profile, Some(Shared::Varies));
    assert_eq!(c.name, Shared::Same("Балка".into()));
    assert_eq!(c.kind, Shared::Same("Балка".into()));
    let mut pane = PropertyPane::default();
    pane.sync(&c);
    assert_eq!(pane.profile, None, "«— разные —» shown");
    pane.profile = Some(channel);
    let changes = pane.changes(&c).unwrap();
    assert_eq!(
        changes,
        Changes {
            profile: Some(channel),
            ..Changes::default()
        },
        "only the edited field is applied"
    );
    let depth = b.editor.document.history_depth().0;
    let commands = modify(project, &c.parts, &changes).unwrap();
    b.editor.commit("Изменить свойства", &commands).unwrap();
    assert_eq!(
        b.editor.document.history_depth().0,
        depth + 1,
        "one transaction"
    );
    assert!(ids.iter().all(|id| profile_of(&b, *id) == channel));
    b.editor.history(false).unwrap();
    assert_eq!(
        ids.map(|id| profile_of(&b, id)),
        [i300, angle, i300],
        "undo restores all three"
    );
    // Class colour palette.
    assert_eq!(
        open_bim_renderer::class_color("2"),
        Some([0.85, 0.15, 0.15, 1.0])
    );
    assert_eq!(open_bim_renderer::class_color("99"), None);
    let class = Changes {
        class: Some("3".into()),
        ..Changes::default()
    };
    let commands = modify(b.editor.document.project(), &ids, &class).unwrap();
    b.editor.commit("Класс", &commands).unwrap();
    let scene = Scene::from_project(b.editor.document.project()).unwrap();
    let green = open_bim_renderer::class_color("3").unwrap();
    assert!(
        scene
            .instances
            .iter()
            .filter(|i| ids.contains(&i.id))
            .all(|i| i.color == green)
    );
    assert!(modify(b.editor.document.project(), &ids, &Changes::default()).is_err());
}

#[test]
fn grid_views_have_plan_per_level_and_elevation_per_grid_line() {
    let project = default_project().unwrap();
    let views = grid_views(&project);
    let plans: Vec<_> = views
        .iter()
        .filter(|v| matches!(v.kind, ViewKind::Plan { .. }))
        .collect();
    let elevations: Vec<_> = views
        .iter()
        .filter(|v| matches!(v.kind, ViewKind::Elevation { .. }))
        .collect();
    assert_eq!(views[0].name, "3D");
    assert_eq!(plans.len(), 2);
    assert_eq!(elevations.len(), 9, "4 × 5 grid lines");
    let top = plans
        .iter()
        .find(|v| v.name == "План на отм. +3,600")
        .unwrap();
    assert!(
        (top.towards_viewer() - DVec3::Z).length() < 1e-9,
        "plan looks down"
    );
    assert!(top.orthographic);
    assert!(top.keeps(DVec3::new(5000., 5000., 3600.)));
    assert!(top.keeps(DVec3::new(5000., 5000., 2700.)));
    assert!(
        !top.keeps(DVec3::new(5000., 5000., 2500.)),
        "below the view depth"
    );
    assert!(
        !top.keeps(DVec3::new(5000., 5000., 4700.)),
        "above the view depth"
    );
    assert_eq!(top.workplane, Workplane::at_elevation(3600.));
    // Elevation along grid line А (y = 0, along X): looking from −Y.
    let a = elevations.iter().find(|v| v.name.ends_with(" А")).unwrap();
    assert!((a.towards_viewer() - DVec3::new(0., -1., 0.)).length() < 1e-9);
    assert!(a.keeps(DVec3::new(9000., 400., 1000.)) && !a.keeps(DVec3::new(9000., 600., 1000.)));
    assert!(!a.keeps(DVec3::new(9000., -600., 1000.)));
    assert_eq!(a.workplane.axes[2], DVec3::new(0., -1., 0.));
    // The camera set from a view looks in that direction.
    for v in &views {
        let camera = Camera {
            yaw: v.yaw,
            pitch: v.pitch,
            ..Camera::default()
        };
        let dir = (camera.eye() - camera.target).normalize();
        assert!((dir - v.towards_viewer()).length() < 1e-9, "{}", v.name);
    }
}

#[test]
fn switching_views_keeps_the_work_plane_per_view() {
    let mut b = bench();
    b.execute(Cmd::GridViews);
    let plan = b
        .views
        .iter()
        .position(|v| v.name == "План на отм. +3,600")
        .unwrap();
    assert!(matches!(b.apply_view(plan), Some(Request::ApplyView(_))));
    assert_eq!(b.engine.workplane, Workplane::at_elevation(3600.));
    assert!(b.clip.iter().all(Option::is_some));
    // A custom work plane in the plan is remembered when coming back.
    let custom = Workplane::at_elevation(3000.);
    b.engine.workplane = custom;
    b.apply_view(0);
    assert_eq!(b.engine.workplane, Workplane::default());
    assert!(b.clip.iter().all(Option::is_none), "3D has no view depth");
    b.apply_view(plan);
    assert_eq!(b.engine.workplane, custom);
    // Ctrl+P toggles back to 3D.
    b.execute(Cmd::PlaneOr3d);
    assert_eq!(b.active_view, 0);
}

#[test]
fn search_selects_exactly_the_beams_with_the_profile() {
    let mut b = bench();
    let i300 = [
        beam(&mut b, ObjectId(2), 0.),
        beam(&mut b, ObjectId(2), 6000.),
    ];
    let other = beam(&mut b, ObjectId(3), 12000.);
    b.search = "I 300".into();
    assert_eq!(b.search_model(), 2);
    assert_eq!(b.editor.selected, BTreeSet::from(i300));
    assert!(!b.editor.selected.contains(&other));
}

#[test]
fn measure_two_points_reports_distance_and_components() {
    let mut b = bench();
    b.execute(Cmd::Measure);
    assert!(b.picking() && b.prompt().contains("первую точку измерения"));
    b.pick(DVec3::ZERO);
    b.pick(DVec3::new(3000., 4000., 0.));
    assert_eq!(
        b.measured,
        Some((DVec3::ZERO, DVec3::new(3000., 4000., 0.)))
    );
    let m = b.tools.message.clone().unwrap();
    assert!(m.contains("5000.0") && m.contains("ΔX 3000.0"), "{m}");
    assert!(b.picking(), "measure repeats until Esc");
    b.execute(Cmd::CancelTool);
    assert!(!b.picking() && b.measured.is_none());
}

#[test]
fn geometry_switch_and_grid_nodes_at_every_level() {
    let mut b = bench();
    let id = beam(&mut b, ObjectId(2), 6000.);
    let project = b.editor.document.project().clone();
    let scene = Arc::new(Scene::from_project(&project).unwrap());
    let mut camera = Camera::default();
    camera.fit(Bounds {
        min: DVec3::new(-2000., -2000., 0.),
        max: DVec3::new(26000., 20000., 3600.),
    });
    let view = View {
        camera: &camera,
        origin: scene.origin,
        size: [1200., 800.],
    };
    // A grid node at level +3.600 (no part there) snaps exactly.
    let node = DVec3::new(18000., 12000., 3600.);
    let s = view.screen(node).unwrap();
    let mut snapper = Snapper::default();
    let hit = snapper
        .snap(
            &project,
            &scene,
            view,
            s,
            Switches::default(),
            12.,
            None,
            &Workplane::default(),
        )
        .unwrap();
    assert!(
        (hit.point - node).length() < 1e-6,
        "{:?} {:?}",
        hit.kind,
        hit.point
    );
    // Geometry off: no solid corners near the beam end, only reference points.
    let ElementShape::Member { frame, .. } = &project.elements[&id].shape else {
        panic!()
    };
    let corner = DVec3::from_array(frame.origin.0) + DVec3::new(0., 75., 150.);
    let s = view.screen(corner).unwrap();
    let off = Switches {
        geometry: false,
        ..Switches::default()
    };
    let hits = snapper.candidates(
        &project,
        &scene,
        view,
        s,
        off,
        12.,
        None,
        &Workplane::default(),
    );
    assert!(
        hits.iter().all(|h| h.reference || h.kind == SnapKind::Free),
        "{hits:?}"
    );
    let on = snapper.candidates(
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
        on.iter().any(|h| h.kind == SnapKind::EndPoint),
        "geometry corners with the switch on"
    );
}
