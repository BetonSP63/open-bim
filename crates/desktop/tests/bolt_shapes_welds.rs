//! Bolt group shapes «Окружность» and «Список», welds along picked points
//! («шов по ломаной» between two parts, «шов на детали» on one part).
use open_bim_desktop::interaction::tools::with_default_grid;
use open_bim_desktop::tools::detailing_ui::{BoltShape, coordinates};
use open_bim_desktop::tools::{Editor, ui::Action};
use open_bim_desktop::ui::{Request, Workbench, commands::Cmd};
use open_bim_model::{
    DetailKind, Element, ElementShape, Frame, LengthMm, ObjectId, Point3, Project,
};
use open_bim_renderer::DVec3;
use std::collections::BTreeSet;
use std::f64::consts::PI;

fn bench() -> Workbench {
    let project = with_default_grid(Project::default()).unwrap();
    Workbench::new(Editor::new(project).unwrap())
}

fn apply(b: &mut Workbench, request: Option<Request>) -> bool {
    match request {
        Some(Request::Action(Action::Commit(label, commands))) => {
            if let Err(e) = b.editor.commit(&label, &commands) {
                panic!("{label}: {e}");
            }
            true
        }
        _ => false,
    }
}

fn p(b: &Workbench) -> &Project {
    b.editor.document.project()
}

/// A 400 × 400 × 20 plate lying at height `z` with its corner at the origin.
fn plate(b: &mut Workbench, z: f64) -> ObjectId {
    let id = ObjectId(p(b).next_id);
    let element = Element {
        id,
        name: "Пластина".into(),
        material: ObjectId(1),
        shape: ElementShape::Plate {
            contour: vec![[0., 0.], [400., 0.], [400., 400.], [0., 400.]],
            thickness: LengthMm::new(20.).unwrap(),
            frame: Frame {
                origin: Point3([0., 0., z]),
                ..Frame::default()
            },
        },
        properties: Default::default(),
    };
    b.editor
        .commit(
            "Пластина",
            &[open_bim_commands::Command::Create { element }],
        )
        .unwrap();
    id
}

/// Bolts through two plates by the bolt command, origin at (200, 200).
fn bolts(b: &mut Workbench) -> (ObjectId, ObjectId, Vec<[f64; 2]>) {
    let (lower, upper) = (plate(b, 0.), plate(b, 20.));
    b.execute(Cmd::ToolBolt);
    b.pick_object(Some(lower));
    b.pick_object(Some(upper));
    b.execute(Cmd::FinishPicks);
    b.pick(DVec3::new(200., 200., 40.));
    let r = b.pick(DVec3::new(300., 200., 40.));
    assert!(apply(b, r));
    let points = p(b)
        .details
        .values()
        .find_map(|d| match &d.kind {
            DetailKind::BoltGroup { points, .. } => Some(points.clone()),
            _ => None,
        })
        .unwrap();
    (lower, upper, points)
}

fn drilled(b: &Workbench, id: ObjectId) -> f64 {
    400. * 400. * 20. - p(b).element_volume_mm3(id).unwrap()
}

#[test]
fn circle_group_drills_every_part_on_the_circle() {
    let mut b = bench();
    let d = &mut b.tools.detailing;
    d.bolt_shape = BoltShape::Circle;
    d.circle_count = "6".into();
    d.circle_diameter = "200".into();
    let (lower, upper, points) = bolts(&mut b);
    assert_eq!(points.len(), 6);
    for q in &points {
        assert!((q[0].hypot(q[1]) - 100.).abs() < 1e-9);
    }
    // Six Ø22 holes through each 20 mm plate.
    let hole = PI * 11. * 11. * 20.;
    for id in [lower, upper] {
        assert!(
            (drilled(&b, id) - 6. * hole).abs() < 1.,
            "{}",
            drilled(&b, id)
        );
    }
    // Bolt list: 6 bolts, nuts and washers.
    let reports = open_bim_reports::generate(p(&b)).unwrap();
    let bolts = reports.fasteners.iter().find(|r| r.kind == "Болт").unwrap();
    assert_eq!(bolts.quantity, 6);
    // Saved and opened again.
    let json = p(&b).to_json().unwrap();
    let reopened = Project::from_json(&json).unwrap();
    assert_eq!(reopened.details, p(&b).details);
    // Too many bolts for the circle: refused with a reason.
    b.tools.detailing.circle_count = "40".into();
    assert!(b.tools.detailing.bolt_layout().is_ok());
    let (lower2, upper2) = (plate(&mut b, 100.), plate(&mut b, 120.));
    b.execute(Cmd::ToolBolt);
    b.pick_object(Some(lower2));
    b.pick_object(Some(upper2));
    b.execute(Cmd::FinishPicks);
    b.pick(DVec3::new(200., 200., 140.));
    assert!(b.pick(DVec3::new(300., 200., 140.)).is_none());
}

#[test]
fn list_group_takes_bolt_coordinates_one_by_one() {
    assert_eq!(coordinates("75 -50 2*0").unwrap(), [75., -50., 0., 0.]);
    let mut b = bench();
    let d = &mut b.tools.detailing;
    d.bolt_shape = BoltShape::List;
    d.steps_x = "-100 0 120".into();
    d.steps_y = "-100 50 0".into();
    let (lower, _, points) = bolts(&mut b);
    assert_eq!(points, vec![[-100., -100.], [0., 50.], [120., 0.]]);
    let hole = PI * 11. * 11. * 20.;
    assert!((drilled(&b, lower) - 3. * hole).abs() < 1.);
    // One Y for all bolts; unequal lists are refused.
    let d = &mut b.tools.detailing;
    d.steps_y = "30".into();
    assert_eq!(
        d.bolt_layout().unwrap().2,
        vec![[-100., 30.], [0., 30.], [120., 30.]]
    );
    d.steps_y = "30 40".into();
    assert!(d.bolt_layout().is_err());
    // «Повернуть» swaps X and Y of each bolt.
    d.steps_y = "30".into();
    d.rotate = true;
    assert_eq!(d.bolt_layout().unwrap().2[0], [30., -100.]);
}

#[test]
fn weld_along_picked_points_between_two_parts() {
    let mut b = bench();
    let (lower, upper) = (plate(&mut b, 0.), plate(&mut b, 20.));
    b.tools.detailing.shop = true;
    b.execute(Cmd::ToolWeldPolygon);
    assert!(b.prompt().contains("к которой приваривают"));
    assert!(b.pick_object(Some(lower)).is_none());
    assert!(b.prompt().contains("привариваемую"));
    assert!(b.pick_object(Some(upper)).is_none());
    assert!(b.prompt().contains("начало шва"));
    for q in [[0., 0.], [400., 0.], [400., 400.]] {
        assert!(b.pick(DVec3::new(q[0], q[1], 20.)).is_none());
    }
    let r = b.execute(Cmd::FinishPicks);
    assert!(apply(&mut b, r));
    let weld = p(&b)
        .details
        .values()
        .find_map(|d| match &d.kind {
            DetailKind::Weld { parts, path, .. } => Some((parts.clone(), path.clone())),
            _ => None,
        })
        .unwrap();
    assert_eq!(weld.0, BTreeSet::from([lower, upper]));
    assert_eq!(weld.1.len(), 3);
    // Shop weld: one assembly, the first part is the main part.
    let a: Vec<_> = p(&b).assemblies.values().collect();
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].main_part, lower);
    // The command goes on for the next weld; 800 mm of weld in the list.
    assert!(b.prompt().contains("к которой приваривают"));
    let reports = open_bim_reports::generate(p(&b)).unwrap();
    let row = reports.fasteners.iter().find(|r| r.kind != "Болт").unwrap();
    assert!((row.length_mm - 800.).abs() < 1e-6, "{row:?}");
}

#[test]
fn weld_on_one_part_and_bad_points() {
    let mut b = bench();
    let part = plate(&mut b, 0.);
    b.execute(Cmd::ToolWeldOnPart);
    assert!(b.prompt().contains("на которой"));
    b.pick_object(Some(part));
    b.pick(DVec3::new(0., 200., 20.));
    // One point is not a weld.
    assert!(b.execute(Cmd::FinishPicks).is_none());
    b.pick(DVec3::new(400., 200., 20.));
    let r = b.execute(Cmd::FinishPicks);
    assert!(apply(&mut b, r));
    let parts = p(&b)
        .details
        .values()
        .find_map(|d| match &d.kind {
            DetailKind::Weld { parts, .. } => Some(parts.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(parts, BTreeSet::from([part]));
    assert!(p(&b).assemblies.is_empty());
    // Two equal points in a row are refused.
    b.pick_object(Some(part));
    b.pick(DVec3::new(0., 100., 20.));
    b.pick(DVec3::new(0., 100., 20.));
    assert!(b.execute(Cmd::FinishPicks).is_none());
    assert!(b.tools.message.is_some() || !b.prompt().is_empty());
}
