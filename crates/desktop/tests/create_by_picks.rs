//! Creating objects by picking points: snapped beam picks, a
//! continuous beam command, column bottom/top, contour plates, the grid
//! coordinate strings, bolts «by parts and points» and welds between parts.
use open_bim_coordination::{ConflictKind, check};
use open_bim_desktop::interaction::Workplane;
use open_bim_desktop::interaction::snap::{SnapKind, Snapper, Switches, View};
use open_bim_desktop::interaction::tools::{
    GridSpec, coordinates, default_project, elevations, grid_commands, labels,
};
use open_bim_desktop::tools::{Editor, ui::Action};
use open_bim_desktop::ui::{Request, Workbench, commands::Cmd};
use open_bim_model::{DetailKind, ElementShape, ObjectId, Point3};
use open_bim_numbering::number;
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
        _ => false,
    }
}

fn pick(b: &mut Workbench, p: DVec3) -> bool {
    let r = b.pick(p);
    apply(b, r)
}

fn axis_x(b: &Workbench, name: &str) -> f64 {
    b.editor
        .document
        .project()
        .axes
        .values()
        .find(|a| a.name == name)
        .unwrap()
        .start
        .0[0]
}
fn axis_y(b: &Workbench, name: &str) -> f64 {
    b.editor
        .document
        .project()
        .axes
        .values()
        .find(|a| a.name == name)
        .unwrap()
        .start
        .0[1]
}

#[test]
fn grid_coordinate_strings_relative_and_absolute() {
    assert_eq!(
        coordinates("0 4*6000").unwrap(),
        vec![0., 6000., 12000., 18000., 24000.]
    );
    assert_eq!(
        coordinates("0 4000 4000").unwrap(),
        coordinates("0 2*4000").unwrap()
    );
    assert_eq!(coordinates("2*3000").unwrap(), vec![0., 3000., 6000.]);
    assert_eq!(coordinates("-1500 3000").unwrap(), vec![-1500., 1500.]);
    for bad in ["", "0 a", "0 0*6000", "0 -3000", "0 x*6000", "0 1000*6000"] {
        assert!(coordinates(bad).is_err(), "{bad:?} must be rejected");
    }
    // Z values are absolute: «0 3600 7200» are elevations, not spacings.
    assert_eq!(elevations("0 3600 7200").unwrap(), vec![0., 3600., 7200.]);
    assert!(elevations("0 3600 3600").is_err());
    assert!(elevations("").is_err());
}

#[test]
fn grid_labels_continue_and_map_to_lines() {
    assert_eq!(labels("А", 4).unwrap(), ["А", "Б", "В", "Г"]);
    assert_eq!(labels("1 2", 5).unwrap(), ["1", "2", "3", "4", "5"]);
    // ГОСТ 21.101 skips З, Й, О: И → К.
    assert_eq!(labels("Ж", 3).unwrap(), ["Ж", "И", "К"]);
    assert_eq!(labels("H", 2).unwrap(), ["H", "J"]);
    assert_eq!(labels("", 2).unwrap(), ["1", "2"]);
    assert!(labels("А Б В", 2).is_err());
    assert!(labels("А А", 2).is_err());
    let p = open_bim_model::Project::default();
    let spec = GridSpec {
        x: "0 2*6000".into(),
        y: "0 6000".into(),
        z: "0 3600 7200".into(),
        labels_x: "1".into(),
        labels_y: "А".into(),
        labels_z: "0 +3.6 +7.2".into(),
        extension: "1000".into(),
    };
    let commands = grid_commands(&p, &spec).unwrap();
    let mut doc = open_bim_commands::Document::new(p).unwrap();
    doc.transact("Сетка", &commands).unwrap();
    let p = doc.project();
    assert_eq!(p.axes.len(), 5);
    assert_eq!(p.levels.len(), 3);
    let line = |n: &str| p.axes.values().find(|a| a.name == n).unwrap();
    assert_eq!(line("3").start.0, [12000., -1000., 0.]);
    assert_eq!(line("3").end.0, [12000., 7000., 0.]);
    assert_eq!(line("Б").start.0, [-1000., 6000., 0.]);
    assert!(
        p.levels
            .values()
            .any(|l| l.name == "+7.2" && l.elevation_mm == 7200.)
    );
    assert_eq!(doc.history_depth().0, 1, "one transaction");
}

#[test]
fn new_model_has_the_default_grid() {
    let p = default_project().unwrap();
    let names: BTreeSet<&str> = p.axes.values().map(|a| a.name.as_str()).collect();
    assert_eq!(
        names,
        BTreeSet::from(["1", "2", "3", "4", "5", "А", "Б", "В", "Г"])
    );
    assert_eq!(p.levels.len(), 2);
    assert_eq!(p.revision, 0);
    assert!(p.elements.is_empty());
}

#[test]
fn beam_by_two_snapped_grid_picks_is_exact_and_each_beam_is_one_undo_step() {
    let mut b = bench();
    let project = b.editor.document.project().clone();
    let scene = Arc::new(Scene::from_project(&project).unwrap());
    let mut camera = Camera::default();
    camera.set_view(NamedView::Top);
    camera.fit(Bounds {
        min: DVec3::new(-2000., -2000., 0.),
        max: DVec3::new(26000., 20000., 0.),
    });
    let size = [1200., 800.];
    let mut snapper = Snapper::default();
    let mut snapped = |target: DVec3| {
        let view = View {
            camera: &camera,
            origin: scene.origin,
            size,
        };
        let s = view.screen(target).unwrap();
        // 4 px off the intersection, like a real click.
        let hit = snapper
            .snap(
                &project,
                &scene,
                view,
                [s[0] + 4., s[1] - 3.],
                Switches::default(),
                12.,
                None,
                &Workplane::default(),
            )
            .unwrap();
        assert_eq!(hit.kind, SnapKind::Intersection);
        hit.point
    };
    let (x1, x2, y) = (axis_x(&b, "1"), axis_x(&b, "2"), axis_y(&b, "Б"));
    let a = snapped(DVec3::new(x1, y, 0.));
    let c = snapped(DVec3::new(x2, y, 0.));
    assert!((a - DVec3::new(0., 6000., 0.)).length() <= 1e-6);
    assert!((c - DVec3::new(6000., 6000., 0.)).length() <= 1e-6);
    b.execute(Cmd::ToolBeam);
    assert!(!pick(&mut b, a));
    assert!(pick(&mut b, c));
    let beam = b
        .editor
        .document
        .project()
        .elements
        .values()
        .next()
        .unwrap()
        .clone();
    let ElementShape::Member { frame, length, .. } = &beam.shape else {
        panic!()
    };
    let end = frame.world(Point3([0., 0., length.value()]));
    assert!((DVec3::from_array(frame.origin.0) - a).length() <= 1e-6);
    assert!((DVec3::from_array(end.0) - c).length() <= 1e-6);
    // The command stays active: two more beams in the same command.
    assert!(b.engine.active());
    let d = DVec3::new(12000., 6000., 0.);
    assert!(!pick(&mut b, c));
    assert!(pick(&mut b, d));
    assert!(!pick(&mut b, d));
    assert!(pick(&mut b, DVec3::new(18000., 6000., 0.)));
    assert_eq!(b.editor.document.project().elements.len(), 3);
    assert_eq!(
        b.editor.document.history_depth().0,
        3,
        "one undo step per beam"
    );
}

#[test]
fn column_uses_bottom_and_top_and_plate_contour_equals_picks() {
    let mut b = bench();
    b.execute(Cmd::ToolColumn);
    b.editor.draft.bottom = "0".into();
    b.editor.draft.top = "4800".into();
    assert!(pick(&mut b, DVec3::new(6000., 12000., 0.)));
    let column = b
        .editor
        .document
        .project()
        .elements
        .values()
        .next()
        .unwrap()
        .clone();
    let ElementShape::Member { frame, length, .. } = column.shape else {
        panic!()
    };
    assert_eq!(frame.origin.0, [6000., 12000., 0.]);
    assert!((length.value() - 4800.).abs() < 1e-9);

    let picks = [
        DVec3::new(0., 0., 0.),
        DVec3::new(6000., 0., 0.),
        DVec3::new(6000., 3000., 0.),
        DVec3::new(3000., 4500., 0.),
        DVec3::new(0., 3000., 0.),
    ];
    b.execute(Cmd::ToolPlate);
    for p in picks {
        assert!(!pick(&mut b, p));
    }
    let r = b.execute(Cmd::FinishPicks);
    assert!(apply(&mut b, r));
    let project = b.editor.document.project();
    let plate = project
        .elements
        .values()
        .find(|e| matches!(e.shape, ElementShape::Plate { .. }))
        .unwrap();
    let ElementShape::Plate { contour, frame, .. } = &plate.shape else {
        panic!()
    };
    let world: Vec<DVec3> = contour
        .iter()
        .map(|c| DVec3::from_array(frame.world(Point3([c[0], c[1], 0.])).0))
        .collect();
    assert_eq!(world.len(), 5);
    assert!(
        (world[0] - picks[0]).length() < 1e-9,
        "first pick is vertex 0"
    );
    for p in picks {
        assert!(world.iter().any(|w| (*w - p).length() < 1e-9), "{p:?}");
    }
}

fn plate(b: &mut Workbench, z: f64) -> ObjectId {
    b.editor.draft.thickness = "10".into();
    b.engine.workplane = Workplane::at_elevation(z);
    b.execute(Cmd::ToolPlate);
    for p in [[0., 0.], [200., 0.], [200., 200.], [0., 200.]] {
        pick(b, DVec3::new(p[0], p[1], z));
    }
    let r = b.execute(Cmd::FinishPicks);
    assert!(apply(b, r));
    b.execute(Cmd::CancelTool);
    *b.editor.document.project().elements.keys().last().unwrap()
}

#[test]
fn bolt_by_parts_and_points_drills_both_plates_as_allowed_holes() {
    let mut b = bench();
    let lower = plate(&mut b, 0.);
    let upper = plate(&mut b, 10.);
    b.engine.workplane = Workplane::at_elevation(20.);
    b.tools.detailing.steps_x = "80".into();
    b.tools.detailing.steps_y = "".into();
    b.execute(Cmd::ToolBolt);
    assert!(b.picks_objects());
    assert!(b.prompt().contains("основную деталь"));
    assert!(b.pick_object(Some(upper)).is_none());
    assert!(b.pick_object(Some(lower)).is_none());
    assert!(b.prompt().contains("присоединяемые"));
    b.execute(Cmd::FinishPicks);
    assert!(!b.picks_objects() && b.picking());
    assert!(!pick(&mut b, DVec3::new(60., 100., 20.)));
    assert!(pick(&mut b, DVec3::new(160., 100., 20.)));
    let p = b.editor.document.project().clone();
    let holes: Vec<_> = p
        .details
        .values()
        .filter_map(|d| match d.kind {
            DetailKind::Hole { part, .. } => Some(part),
            _ => None,
        })
        .collect();
    assert_eq!(holes.iter().filter(|h| **h == lower).count(), 2);
    assert_eq!(holes.iter().filter(|h| **h == upper).count(), 2);
    assert!(p.details.values().any(|d| matches!(&d.kind, DetailKind::BoltGroup { parts, .. } if parts == &BTreeSet::from([lower, upper]))));
    let report = check(&p, &number(&p).unwrap()).unwrap();
    let kinds = |k| report.conflicts.iter().filter(|c| c.kind == k).count();
    assert_eq!(
        kinds(ConflictKind::AllowedBoltHole),
        4,
        "2 bolts × 2 plates"
    );
    assert_eq!(kinds(ConflictKind::Intersection), 0);
    assert_eq!(report.blocking().count(), 0);
    assert_eq!(
        b.editor.document.history_depth().0,
        3,
        "plates + bolt group"
    );
}

#[test]
fn weld_between_main_and_secondary_part() {
    let mut b = bench();
    let lower = plate(&mut b, 0.);
    let upper = plate(&mut b, 10.);
    b.execute(Cmd::ToolWeld);
    assert!(b.pick_object(Some(lower)).is_none());
    assert!(b.prompt().contains("привариваемую"));
    let r = b.pick_object(Some(upper));
    assert!(apply(&mut b, r));
    let p = b.editor.document.project();
    let weld = p
        .details
        .values()
        .find_map(|d| match &d.kind {
            DetailKind::Weld { parts, path, .. } => Some((parts.clone(), path.clone())),
            _ => None,
        })
        .unwrap();
    assert_eq!(weld.0, BTreeSet::from([lower, upper]));
    assert_eq!(weld.1.len(), 2);
    // The command re-arms for the next weld; the same part twice is refused.
    assert!(b.picks_objects());
    b.pick_object(Some(lower));
    assert!(b.pick_object(Some(lower)).is_none());
}

#[test]
fn every_column_top_snaps_in_the_default_3d_view() {
    let mut b = bench();
    b.execute(Cmd::ToolColumn);
    let nodes = [
        [18000., 12000.],
        [24000., 12000.],
        [18000., 18000.],
        [24000., 18000.],
    ];
    for n in nodes {
        pick(&mut b, DVec3::new(n[0], n[1], 0.));
    }
    let project = b.editor.document.project().clone();
    let scene = Arc::new(Scene::from_project(&project).unwrap());
    let mut camera = Camera::default();
    camera.fit(open_bim_renderer::overlay::model_bounds(&scene, &project).unwrap());
    let size = [1100., 700.];
    let mut snapper = Snapper::default();
    for n in nodes {
        let top = DVec3::new(n[0], n[1], 3600.);
        let view = View {
            camera: &camera,
            origin: scene.origin,
            size,
        };
        let s = view.screen(top).unwrap();
        let hit = snapper
            .snap(
                &project,
                &scene,
                view,
                s,
                Switches::default(),
                12.,
                Some(DVec3::new(18000., 12000., 3600.)),
                &Workplane::default(),
            )
            .unwrap();
        assert!(
            (hit.point - top).length() < 1e-6,
            "{n:?}: {:?} {:?}",
            hit.kind,
            hit.point
        );
    }
}

#[test]
fn nearer_grid_intersection_beats_a_reference_point_a_few_pixels_away() {
    // Found in the GUI run: in the default 3D view the top of column В4 lies
    // ~5 px from the grid node Г5; the node under the cursor must win.
    let mut b = bench();
    b.execute(Cmd::ToolColumn);
    pick(&mut b, DVec3::new(18000., 12000., 0.));
    let project = b.editor.document.project().clone();
    let scene = Arc::new(Scene::from_project(&project).unwrap());
    let mut camera = Camera::default();
    camera.fit(open_bim_renderer::overlay::model_bounds(&scene, &project).unwrap());
    let view = View {
        camera: &camera,
        origin: scene.origin,
        size: [1100., 700.],
    };
    let node = DVec3::new(24000., 18000., 0.);
    let top = DVec3::new(18000., 12000., 3600.);
    let (s, t) = (view.screen(node).unwrap(), view.screen(top).unwrap());
    let apart = ((s[0] - t[0]).powi(2) + (s[1] - t[1]).powi(2)).sqrt();
    assert!(
        apart < 12.,
        "fixture: both within the snap zone ({apart:.1} px)"
    );
    let hit = Snapper::default()
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
}
