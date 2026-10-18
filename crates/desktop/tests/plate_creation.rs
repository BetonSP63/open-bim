//! Contour plate creation: concave contours tessellate exactly, crossing picks
//! are refused with a message, Backspace removes the last point, rectangle
//! modes and the plate position across its thickness.
use open_bim_desktop::interaction::polygon::{PlateMode, PlatePosition, check_next, triangulate};
use open_bim_desktop::interaction::tools::default_project;
use open_bim_desktop::interaction::{Outcome, Workplane};
use open_bim_desktop::tools::{Editor, ui::Action};
use open_bim_desktop::ui::{Request, Workbench, commands::Cmd};
use open_bim_model::{ElementShape, Point3};
use open_bim_renderer::{DVec3, Scene};

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

fn last_plate(b: &Workbench) -> (Vec<DVec3>, f64, f64) {
    let p = b.editor.document.project();
    let e = p
        .elements
        .values()
        .rev()
        .find(|e| matches!(e.shape, ElementShape::Plate { .. }))
        .unwrap();
    let ElementShape::Plate {
        contour,
        thickness,
        frame,
    } = &e.shape
    else {
        unreachable!()
    };
    let corners = contour
        .iter()
        .map(|c| DVec3::from_array(frame.world(Point3([c[0], c[1], 0.])).0))
        .collect();
    (corners, frame.origin.0[2], thickness.value())
}

fn area(points: &[[f64; 2]]) -> f64 {
    (0..points.len())
        .map(|i| {
            let (a, b) = (points[i], points[(i + 1) % points.len()]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        .abs()
        / 2.
}

#[test]
fn concave_plate_tessellates_to_its_exact_area() {
    let mut b = bench();
    let l_shape = [
        [0., 0.],
        [3000., 0.],
        [3000., 1000.],
        [1000., 1000.],
        [1000., 2500.],
        [0., 2500.],
    ];
    b.execute(Cmd::ToolPlate);
    for q in l_shape {
        b.pick(DVec3::new(q[0], q[1], 0.));
    }
    let r = b.execute(Cmd::FinishPicks);
    assert!(apply(&mut b, r));
    let project = b.editor.document.project();
    let scene = Scene::from_project(project).unwrap();
    let inst = scene.instances.last().unwrap();
    let mesh = &scene.meshes[inst.mesh];
    let t = 20.;
    let top: f64 = mesh
        .triangles
        .iter()
        .map(|tri| tri.map(|i| mesh.positions[i as usize]))
        .filter(|p| p.iter().all(|v| (v.z - t).abs() < 1e-9))
        .map(|p| (p[1] - p[0]).cross(p[2] - p[0]).length() / 2.)
        .sum();
    let expected = area(&l_shape);
    assert!(
        (top - expected).abs() / expected < 1e-6,
        "top face {top} vs {expected}"
    );
    // The preview triangulation (ear clipping) covers the same area.
    let tris = triangulate(&l_shape);
    let tri_area: f64 = tris
        .iter()
        .map(|t| area(&[l_shape[t[0]], l_shape[t[1]], l_shape[t[2]]]))
        .sum();
    assert!((tri_area - expected).abs() < 1e-6);
}

#[test]
fn crossing_pick_is_refused_and_backspace_removes_a_point() {
    let mut b = bench();
    b.execute(Cmd::ToolPlate);
    for q in [[0., 0.], [3000., 0.], [3000., 2000.]] {
        b.pick(DVec3::new(q[0], q[1], 0.));
    }
    // Side 3→4 to (1500, -1000) crosses side 1–2.
    let r = b.pick(DVec3::new(1500., -1000., 0.));
    assert!(r.is_none());
    assert_eq!(b.engine.picks.len(), 3, "point not added");
    let message = b.tools.message.clone().unwrap();
    assert!(message.contains("пересекает"), "{message}");
    assert!(check_next(&[[0., 0.], [3000., 0.], [3000., 2000.]], [1500., -1000.]).is_err());
    b.execute(Cmd::UndoPick);
    assert_eq!(b.engine.picks.len(), 2);
    assert!(b.prompt().contains("Backspace"));
    // A closing side that crosses is refused on Enter too.
    let mut e = open_bim_desktop::interaction::Engine::default();
    e.start(open_bim_desktop::tools::ToolKind::Plate);
    for q in [[0., 0.], [2000., 0.], [2000., 2000.], [500., -500.]] {
        let _ = e.pick(DVec3::new(q[0], q[1], 0.));
    }
    // (500,-500) itself crosses side 1–2, so only 3 points are kept: a valid triangle.
    assert_eq!(e.picks.len(), 3);
    assert!(matches!(e.finish(), Outcome::Complete { .. }));
}

#[test]
fn rectangles_by_two_and_three_points_are_exact() {
    let mut b = bench();
    b.execute(Cmd::ToolPlate);
    b.editor.draft.plate_mode = PlateMode::Rectangle;
    let r = b.pick(DVec3::new(1000., 500., 0.));
    assert!(!apply(&mut b, r));
    let r = b.pick(DVec3::new(4000., 2500., 0.));
    assert!(apply(&mut b, r));
    let (c, _, _) = last_plate(&b);
    for q in [[1000., 500.], [4000., 500.], [4000., 2500.], [1000., 2500.]] {
        assert!(
            c.iter()
                .any(|p| (*p - DVec3::new(q[0], q[1], 0.)).length() < 1e-6),
            "{q:?}"
        );
    }
    b.execute(Cmd::CancelTool);
    b.execute(Cmd::ToolPlate);
    b.editor.draft.plate_mode = PlateMode::RotatedRectangle;
    let (s, co) = 30f64.to_radians().sin_cos();
    let a = DVec3::new(10000., 0., 0.);
    let along = DVec3::new(co, s, 0.) * 2000.;
    let across = DVec3::new(-s, co, 0.) * 800.;
    b.pick(a);
    b.pick(a + along);
    let r = b.pick(a + along + across + along * 0.3);
    assert!(apply(&mut b, r));
    let (c, _, _) = last_plate(&b);
    for q in [a, a + along, a + along + across, a + across] {
        assert!(c.iter().any(|p| (*p - q).length() < 1e-6), "{q:?} in {c:?}");
    }
}

#[test]
fn plate_position_across_the_thickness() {
    for (pos, z) in [
        (PlatePosition::Up, 0.),
        (PlatePosition::Middle, -10.),
        (PlatePosition::Down, -20.),
    ] {
        let mut b = bench();
        b.engine.workplane = Workplane::default();
        b.execute(Cmd::ToolPlate);
        b.editor.draft.plate_mode = PlateMode::Rectangle;
        b.editor.draft.plate_position = pos;
        b.pick(DVec3::new(0., 0., 0.));
        let r = b.pick(DVec3::new(500., 400., 0.));
        assert!(apply(&mut b, r));
        let (_, origin_z, t) = last_plate(&b);
        assert_eq!(t, 20.);
        assert!((origin_z - z).abs() < 1e-9, "{pos:?}: {origin_z}");
    }
}
