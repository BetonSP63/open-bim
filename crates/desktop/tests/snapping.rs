use open_bim_desktop::interaction::Workplane;
use open_bim_desktop::interaction::snap::{SnapKind, Snapper, Switches, View, ortho};
use open_bim_model::{
    Element, ElementShape, Frame, GridAxis, LengthMm, MemberKind, ObjectId, Point3, Project,
};
use open_bim_renderer::{Camera, DVec3, NamedView, Scene};
use std::collections::BTreeMap;
use std::sync::Arc;

const SIZE: [f64; 2] = [1200., 800.];

/// Beam 0→6000 along X, plate 400×300 at (1000, 2000), grid lines x=8000 and y=5000.
fn fixture() -> (Project, Arc<Scene>) {
    let mut p = Project::default();
    let beam = p.allocate_id().unwrap();
    let (frame, length) = Frame::along(Point3([0.; 3]), Point3([6000., 0., 0.])).unwrap();
    p.elements.insert(
        beam,
        Element {
            id: beam,
            name: "Б".into(),
            material: ObjectId(1),
            shape: ElementShape::Member {
                member_kind: MemberKind::Beam,
                profile: ObjectId(2),
                length,
                frame,
            },
            properties: BTreeMap::new(),
        },
    );
    let plate = p.allocate_id().unwrap();
    p.elements.insert(
        plate,
        Element {
            id: plate,
            name: "П".into(),
            material: ObjectId(1),
            shape: ElementShape::Plate {
                contour: vec![[0., 0.], [400., 0.], [400., 300.], [0., 300.]],
                thickness: LengthMm::new(20.).unwrap(),
                frame: Frame {
                    origin: Point3([1000., 2000., 0.]),
                    ..Frame::default()
                },
            },
            properties: BTreeMap::new(),
        },
    );
    for (name, a, b) in [
        ("8", [8000., -1000., 0.], [8000., 9000., 0.]),
        ("Б", [-1000., 5000., 0.], [12000., 5000., 0.]),
    ] {
        let id = p.allocate_id().unwrap();
        p.axes.insert(
            id,
            GridAxis {
                id,
                name: name.into(),
                start: Point3(a),
                end: Point3(b),
            },
        );
    }
    let scene = Arc::new(Scene::from_project(&p).unwrap());
    (p, scene)
}

fn camera(scene: &Scene, view: NamedView) -> Camera {
    let mut c = Camera::default();
    c.set_view(view);
    c.fit(scene.bounds);
    c
}

fn snap_at(
    p: &Project,
    scene: &Arc<Scene>,
    cam: &Camera,
    target: DVec3,
    offset: [f64; 2],
    switches: Switches,
    previous: Option<DVec3>,
) -> open_bim_desktop::interaction::snap::Hit {
    let view = View {
        camera: cam,
        origin: scene.origin,
        size: SIZE,
    };
    let s = view.screen(target).unwrap();
    let mut snapper = Snapper::default();
    snapper
        .snap(
            p,
            scene,
            view,
            [s[0] + offset[0], s[1] + offset[1]],
            switches,
            12.,
            previous,
            &Workplane::default(),
        )
        .unwrap()
}

#[test]
fn snaps_to_exact_end_mid_and_grid_intersection() {
    let (p, scene) = fixture();
    for named in [NamedView::Top, NamedView::Isometric] {
        let cam = camera(&scene, named);
        let end = snap_at(
            &p,
            &scene,
            &cam,
            DVec3::new(6000., 0., 0.),
            [5., 0.],
            Switches::default(),
            None,
        );
        assert_eq!(end.kind, SnapKind::Reference, "{named:?}");
        assert!(end.point.distance(DVec3::new(6000., 0., 0.)) <= 1e-6);
        let mid = snap_at(
            &p,
            &scene,
            &cam,
            DVec3::new(3000., 0., 0.),
            [0., 5.],
            Switches::default(),
            None,
        );
        assert_eq!(mid.kind, SnapKind::MidPoint, "{named:?}");
        assert!(
            mid.point.distance(DVec3::new(3000., 0., 0.)) <= 1e-6,
            "{:?}",
            mid.point
        );
        let cross = snap_at(
            &p,
            &scene,
            &cam,
            DVec3::new(8000., 5000., 0.),
            [4., 4.],
            Switches::default(),
            None,
        );
        assert_eq!(cross.kind, SnapKind::Intersection, "{named:?}");
        assert!(cross.point.distance(DVec3::new(8000., 5000., 0.)) <= 1e-6);
    }
}

#[test]
fn switches_remove_their_hits_and_priority_prefers_points() {
    let (p, scene) = fixture();
    let cam = camera(&scene, NamedView::Top);
    let all = Switches {
        nearest: true,
        perpendicular: true,
        ..Switches::default()
    };
    let end = snap_at(
        &p,
        &scene,
        &cam,
        DVec3::new(6000., 0., 0.),
        [5., 0.],
        all,
        None,
    );
    assert_eq!(
        end.kind,
        SnapKind::Reference,
        "end point beats nearest-on-edge"
    );
    let off = Switches {
        reference: false,
        end: false,
        mid: false,
        intersection: false,
        ..all
    };
    let near = snap_at(
        &p,
        &scene,
        &cam,
        DVec3::new(6000., 0., 0.),
        [5., 0.],
        off,
        None,
    );
    assert_eq!(near.kind, SnapKind::Nearest);
    let none = Switches {
        nearest: false,
        perpendicular: false,
        ..off
    };
    assert_eq!(
        snap_at(
            &p,
            &scene,
            &cam,
            DVec3::new(6000., 0., 0.),
            [5., 0.],
            none,
            None
        )
        .kind,
        SnapKind::Free
    );
    let no_cross = Switches {
        intersection: false,
        ..Switches::default()
    };
    assert_ne!(
        snap_at(
            &p,
            &scene,
            &cam,
            DVec3::new(8000., 5000., 0.),
            [4., 4.],
            no_cross,
            None
        )
        .kind,
        SnapKind::Intersection
    );
    let no_mid = Switches {
        mid: false,
        ..Switches::default()
    };
    assert_ne!(
        snap_at(
            &p,
            &scene,
            &cam,
            DVec3::new(3000., 0., 0.),
            [0., 5.],
            no_mid,
            None
        )
        .kind,
        SnapKind::MidPoint
    );
}

#[test]
fn perpendicular_foot_from_previous_pick() {
    let (p, scene) = fixture();
    let cam = camera(&scene, NamedView::Top);
    // Tekla main switch «Reference» stays on (the foot lies on the beam's
    // reference line); only the perpendicular kind is enabled.
    let only = Switches {
        reference: true,
        geometry: false,
        end: false,
        mid: false,
        intersection: false,
        nearest: false,
        perpendicular: true,
        ortho: false,
    };
    let previous = DVec3::new(2000., 3000., 0.);
    let hit = snap_at(
        &p,
        &scene,
        &cam,
        DVec3::new(2000., 0., 0.),
        [4., 3.],
        only,
        Some(previous),
    );
    assert_eq!(hit.kind, SnapKind::Perpendicular);
    let along = DVec3::X;
    assert!(
        (hit.point - previous).dot(along).abs() <= 1e-9,
        "{:?}",
        hit.point
    );
    assert!(hit.point.y.abs() < 1e-6);
}

#[test]
fn ortho_rounds_to_45_degrees() {
    let wp = Workplane::default();
    let from = DVec3::new(100., 100., 0.);
    for (degrees, expected) in [(43f64, 45f64), (88., 90.), (-31., -45.), (10., 0.)] {
        let r = degrees.to_radians();
        let p = ortho(from, from + DVec3::new(r.cos(), r.sin(), 0.) * 1000., &wp);
        let a = (p.y - from.y).atan2(p.x - from.x).to_degrees();
        assert!((a - expected).abs() < 1e-9, "{degrees} → {a}");
        assert!(((p - from).length() - 1000.).abs() < 1e-9);
    }
}

/// Release benchmark: 200 snap queries over the 100 000-object corpus.
/// Run: cargo test --release -p open-bim-desktop --test snapping -- --ignored --nocapture
#[test]
#[ignore]
fn snap_queries_on_100k_corpus_are_fast() {
    let project = open_bim_desktop::synthetic::corpus(100_000).unwrap();
    let scene = Arc::new(Scene::from_project(&project).unwrap());
    let mut cam = Camera::default();
    cam.set_view(NamedView::Isometric);
    cam.fit(scene.bounds);
    let view = View {
        camera: &cam,
        origin: scene.origin,
        size: [2880., 1620.],
    };
    let mut snapper = Snapper::default();
    let mut times = Vec::new();
    let mut kinds = BTreeMap::new();
    for i in 0..200 {
        // Half the cursors sit 3 px off a member end (dense model areas).
        let cursor = if i % 2 == 0 {
            let instance = &scene.instances[(i * 4999) % scene.instances.len()];
            let s = view
                .screen(DVec3::from_array(instance.frame.origin.0))
                .unwrap();
            [s[0] + 3., s[1] - 3.]
        } else {
            [
                100. + (i * 37 % 200) as f64 * 13.,
                80. + (i * 61 % 200) as f64 * 7.,
            ]
        };
        let t = std::time::Instant::now();
        let hit = snapper.snap(
            &project,
            &scene,
            view,
            cursor,
            Switches::default(),
            12.,
            None,
            &Workplane::default(),
        );
        times.push(t.elapsed().as_secs_f64() * 1000.);
        *kinds
            .entry(format!("{:?}", hit.map(|h| h.kind)))
            .or_insert(0) += 1;
    }
    times.sort_by(f64::total_cmp);
    let p95 = times[(times.len() as f64 * 0.95) as usize - 1];
    println!(
        "SNAP_100K p50={:.3} ms p95={:.3} ms max={:.3} ms kinds={kinds:?}",
        times[100], p95, times[199]
    );
    assert!(p95 <= 5.0, "p95 {p95} ms");
}
