//! Snapping beyond points: centres, parallel, line extensions, lines,
//! the right-click override, Shift and X/Y/Z locks and R/A/G numeric modes.
use open_bim_desktop::interaction::Workplane;
use open_bim_desktop::interaction::numeric::{mode_prefix, parse};
use open_bim_desktop::interaction::snap::{Hit, Locks, SnapKind, Snapper, Switches, View};
use open_bim_model::{
    Construction, ConstructionKind, Detail, DetailKind, Element, ElementShape, Frame, GridAxis,
    MemberKind, ObjectId, Point3, Project,
};
use open_bim_renderer::{Camera, DVec3, NamedView, Scene};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

const SIZE: [f64; 2] = [1200., 800.];

/// Beam 0→6000 along X; grid lines x=8000 (y −1000…9000) and y=5000; bolts
/// at (2000, 3000) and (2400, 3000); a circle centred at (4000, 7000); a 45°
/// construction line from (0, 6000) to (3000, 9000).
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
                position: Default::default(),
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
    for kind in [
        ConstructionKind::Circle {
            center: Point3([4000., 7000., 0.]),
            normal: [0., 0., 1.],
            radius: 1500.,
        },
        ConstructionKind::Line {
            a: Point3([0., 6000., 0.]),
            b: Point3([3000., 9000., 0.]),
        },
    ] {
        let id = p.allocate_id().unwrap();
        p.construction.insert(id, Construction { id, kind });
    }
    // The bolt group has no holes, so it stays out of the display scene;
    // snapping reads bolt centres from the model.
    let scene = Arc::new(Scene::from_project(&p).unwrap());
    let bolts = p.allocate_id().unwrap();
    p.details.insert(
        bolts,
        Detail {
            id: bolts,
            kind: DetailKind::BoltGroup {
                parts: BTreeSet::from([beam]),
                holes: BTreeSet::new(),
                frame: Frame {
                    origin: Point3([2000., 3000., 0.]),
                    ..Frame::default()
                },
                diameter: 20.,
                clearance: 2.,
                steps_x: vec![400.],
                steps_y: vec![],
                standard: "ГОСТ 7798".into(),
                grade: "5.8".into(),
            },
        },
    );
    (p, scene)
}

fn top(scene: &Scene) -> Camera {
    let mut c = Camera::default();
    c.set_view(NamedView::Top);
    c.fit(open_bim_renderer::Bounds {
        min: DVec3::new(-2000., -2000., 0.),
        max: DVec3::new(12000., 10000., 0.),
    });
    let _ = scene;
    c
}

struct Probe<'a> {
    p: &'a Project,
    scene: &'a Arc<Scene>,
    cam: Camera,
    snapper: Snapper,
}

impl Probe<'_> {
    fn at(
        &mut self,
        target: DVec3,
        offset: [f64; 2],
        switches: Switches,
        previous: Option<DVec3>,
    ) -> Hit {
        let view = View {
            camera: &self.cam,
            origin: self.scene.origin,
            size: SIZE,
        };
        let s = view.screen(target).unwrap();
        self.snapper
            .snap(
                self.p,
                self.scene,
                view,
                [s[0] + offset[0], s[1] + offset[1]],
                switches,
                12.,
                previous,
                &Workplane::default(),
            )
            .unwrap()
    }
}

fn probe<'a>(p: &'a Project, scene: &'a Arc<Scene>) -> Probe<'a> {
    Probe {
        p,
        scene,
        cam: top(scene),
        snapper: Snapper::default(),
    }
}

#[test]
fn centres_of_bolts_and_circles() {
    let (p, scene) = fixture();
    let mut probe = probe(&p, &scene);
    for target in [
        DVec3::new(2000., 3000., 0.),
        DVec3::new(2400., 3000., 0.),
        DVec3::new(4000., 7000., 0.),
    ] {
        let hit = probe.at(target, [4., -3.], Switches::default(), None);
        assert_eq!(hit.kind, SnapKind::Center, "{target}");
        assert!(hit.point.distance(target) < 1e-9, "{:?}", hit.point);
    }
    let off = Switches {
        center: false,
        ..Switches::default()
    };
    assert_ne!(
        probe
            .at(DVec3::new(2400., 3000., 0.), [4., -3.], off, None)
            .kind,
        SnapKind::Center
    );
}

#[test]
fn line_extension_after_passing_over_the_beam_axis() {
    let (p, scene) = fixture();
    let mut probe = probe(&p, &scene);
    let switches = Switches {
        extension: true,
        ..Switches::default()
    };
    // Past the beam end, before passing over it: nothing to extend.
    let before = probe.at(DVec3::new(7000., 0., 0.), [0., 4.], switches, None);
    assert_ne!(before.kind, SnapKind::Extension);
    // Pass over the beam axis, then move beyond its end.
    probe.at(DVec3::new(3000., 0., 0.), [0., 0.], switches, None);
    let hit = probe.at(DVec3::new(7000., 0., 0.), [0., 4.], switches, None);
    assert_eq!(hit.kind, SnapKind::Extension);
    assert!(hit.point.y.abs() < 1e-6, "{:?}", hit.point);
    assert!((hit.point.x - 7000.).abs() < 100., "{:?}", hit.point);
    let (from, to) = hit.guide.unwrap();
    assert!(from.distance(DVec3::new(6000., 0., 0.)) < 1e-6);
    assert!(to.distance(hit.point) < 1e-9);
    // Where the extension meets grid line 8: an exact intersection.
    let cross = probe.at(DVec3::new(8000., 0., 0.), [4., 4.], switches, None);
    assert_eq!(cross.kind, SnapKind::Intersection);
    assert!(cross.point.distance(DVec3::new(8000., 0., 0.)) < 1e-6);
    // Grid lines extend without passing over them first.
    let grid = probe.at(DVec3::new(8000., 9800., 0.), [4., 0.], switches, None);
    assert_eq!(grid.kind, SnapKind::Extension);
    assert!((grid.point.x - 8000.).abs() < 1e-6);
    // Switch off: no extension.
    let off = probe.at(
        DVec3::new(7000., 0., 0.),
        [0., 4.],
        Switches::default(),
        None,
    );
    assert_ne!(off.kind, SnapKind::Extension);
}

#[test]
fn parallel_to_the_line_passed_over() {
    let (p, scene) = fixture();
    let mut probe = probe(&p, &scene);
    let switches = Switches {
        parallel: true,
        ..Switches::default()
    };
    let previous = DVec3::new(10000., 0., 0.);
    // Pass over the 45° construction line.
    probe.at(
        DVec3::new(1500., 7500., 0.),
        [3., 0.],
        switches,
        Some(previous),
    );
    let hit = probe.at(
        previous + DVec3::new(1000., 1000., 0.),
        [5., 0.],
        switches,
        Some(previous),
    );
    assert_eq!(hit.kind, SnapKind::Parallel);
    let d = hit.point - previous;
    assert!((d.x - d.y).abs() < 1e-6, "{d}");
    assert!(d.length() > 1000.);
    assert_eq!(hit.guide.unwrap().0, previous);
    // No previous pick: no parallel.
    let free = probe.at(
        previous + DVec3::new(1000., 1000., 0.),
        [5., 0.],
        switches,
        None,
    );
    assert_ne!(free.kind, SnapKind::Parallel);
}

#[test]
fn lines_snap_anywhere_on_grid_lines() {
    let (p, scene) = fixture();
    let mut probe = probe(&p, &scene);
    let target = DVec3::new(10000., 5000., 0.);
    assert_eq!(
        probe.at(target, [0., 5.], Switches::default(), None).kind,
        SnapKind::Free
    );
    let lines = Switches {
        line: true,
        ..Switches::default()
    };
    let hit = probe.at(target, [0., 5.], lines, None);
    assert_eq!(hit.kind, SnapKind::Line);
    assert!((hit.point.y - 5000.).abs() < 1e-6, "{:?}", hit.point);
}

#[test]
fn right_click_override_takes_only_one_kind() {
    let (p, scene) = fixture();
    let mut probe = probe(&p, &scene);
    let defaults = Switches::default();
    let end = DVec3::new(6000., 0., 0.);
    assert_eq!(
        probe.at(end, [3., 0.], defaults, None).kind,
        SnapKind::Reference
    );
    // «Середина» only: the beam end no longer attracts, the middle does.
    let mid = defaults.only(SnapKind::MidPoint);
    assert_ne!(probe.at(end, [3., 0.], mid, None).kind, SnapKind::Reference);
    let hit = probe.at(DVec3::new(3000., 0., 0.), [3., 0.], mid, None);
    assert_eq!(hit.kind, SnapKind::MidPoint);
    // Overrides work even with switches the toolbar has off.
    let perpendicular = defaults.none().only(SnapKind::Perpendicular);
    let hit = probe.at(
        DVec3::new(2500., 0., 0.),
        [0., 4.],
        perpendicular,
        Some(DVec3::new(2500., 2000., 0.)),
    );
    assert_eq!(hit.kind, SnapKind::Perpendicular);
    assert!(hit.point.distance(DVec3::new(2500., 0., 0.)) < 1e-6);
    // «Любая точка»: free work plane point next to a snap point.
    assert_eq!(
        probe
            .at(end, [3., 0.], defaults.only(SnapKind::Free), None)
            .kind,
        SnapKind::Free
    );
}

#[test]
fn shift_and_coordinate_locks() {
    let wp = Workplane::default();
    let from = DVec3::new(1000., 1000., 0.);
    // Shift: the direction from the previous pick is held.
    let locks = Locks {
        direction: Some(DVec3::new(1., 1., 0.)),
        ..Locks::default()
    };
    let p = locks.apply(DVec3::new(3000., 2000., 0.), Some(from), &wp);
    assert!((p.x - p.y).abs() < 1e-9, "{p}");
    assert!((p.x - 2500.).abs() < 1e-9, "{p}");
    assert_eq!(locks.describe().unwrap(), "Фиксация: направление");
    // X: the X coordinate stays at the value it had when the key was pressed.
    let mut locks = Locks::default();
    assert!(locks.toggle(0, DVec3::new(4200., 0., 0.), &wp));
    let p = locks.apply(DVec3::new(9000., 3000., 0.), Some(from), &wp);
    assert_eq!(p, DVec3::new(4200., 3000., 0.));
    assert!(locks.toggle(2, DVec3::new(0., 0., 3600.), &wp));
    let p = locks.apply(DVec3::new(9000., 3000., 0.), None, &wp);
    assert_eq!(p, DVec3::new(4200., 3000., 3600.));
    assert_eq!(locks.describe().unwrap(), "Фиксация: X, Z");
    // The same key releases the lock.
    assert!(!locks.toggle(0, DVec3::ZERO, &wp));
    assert_eq!(locks.coordinates, [None, None, Some(3600.)]);
    // Locks follow a rotated work plane: local x along world y here.
    let rotated = Workplane {
        origin: DVec3::new(100., 0., 0.),
        axes: [DVec3::Y, -DVec3::X, DVec3::Z],
    };
    let mut locks = Locks::default();
    locks.toggle(0, DVec3::new(0., 500., 0.), &rotated);
    let p = locks.apply(DVec3::new(-300., 2000., 0.), None, &rotated);
    assert_eq!(p, DVec3::new(-300., 500., 0.));
}

#[test]
fn numeric_modes_r_a_g_on_both_layouts() {
    let wp = Workplane {
        origin: DVec3::new(0., 0., 3000.),
        ..Workplane::default()
    };
    let previous = Some(DVec3::new(1000., 1000., 3000.));
    for (text, expected) in [
        ("R500,0", DVec3::new(1500., 1000., 3000.)),
        ("r 500,0", DVec3::new(1500., 1000., 3000.)),
        ("к500,0", DVec3::new(1500., 1000., 3000.)),
        ("A500,200", DVec3::new(500., 200., 3000.)),
        ("ф500,200", DVec3::new(500., 200., 3000.)),
        ("G500,200,0", DVec3::new(500., 200., 0.)),
        ("П500,200,0", DVec3::new(500., 200., 0.)),
        ("R1000<90", DVec3::new(1000., 2000., 3000.)),
    ] {
        let p = parse(text, &wp, previous, None).unwrap();
        assert!(p.distance(expected) < 1e-9, "{text}: {p}");
    }
    // R before the first pick: from the work plane origin (the default).
    let p = parse("R500,0", &wp, None, None).unwrap();
    assert!(p.distance(DVec3::new(500., 0., 3000.)) < 1e-9);
    // Grid nodes are not modes.
    assert_eq!(mode_prefix("К/2"), None);
    assert_eq!(mode_prefix("Б2"), None);
    assert_eq!(mode_prefix("К1000,0"), Some(('@', "1000,0")));
}
