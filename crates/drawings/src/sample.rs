//! Control model for drawing/export tests, examples and the GUI demo.
use open_bim_model::{
    Assembly, Detail, DetailKind, Element, ElementShape, Frame, LengthMm, MemberKind, ObjectId,
    Point3, Project,
};
use std::collections::{BTreeMap, BTreeSet};

pub const H: f64 = 300.;
pub const B: f64 = 150.;
pub const TW: f64 = 8.;
pub const TF: f64 = 12.;
pub const HOLES_Z: [f64; 2] = [500., 1250.];

pub struct Fixture {
    pub project: Project,
    pub beam: ObjectId,
    pub plate: ObjectId,
    pub assembly: ObjectId,
}

/// I300x150x8x12 beam 3000 mm along world X with two web holes, end plate
/// 400x300x20 with two holes, assembled with the beam as main part.
pub fn control_project() -> Fixture {
    let mut p = Project::default();
    let beam = p.allocate_id().expect("control model");
    p.elements.insert(
        beam,
        Element {
            id: beam,
            name: "Балка Б1".into(),
            material: ObjectId(1),
            shape: ElementShape::Member {
                member_kind: MemberKind::Beam,
                profile: ObjectId(2),
                length: LengthMm::new(3000.).expect("control model"),
                frame: Frame {
                    origin: Point3([0., 0., 3000.]),
                    axes: [[0., 1., 0.], [0., 0., 1.], [1., 0., 0.]],
                },
            },
            properties: BTreeMap::new(),
        },
    );
    for z in HOLES_Z {
        let id = p.allocate_id().expect("control model");
        p.details.insert(
            id,
            Detail {
                id,
                kind: DetailKind::Hole {
                    part: beam,
                    frame: Frame {
                        origin: Point3([-20., 40., z]),
                        axes: [[0., 1., 0.], [0., 0., 1.], [1., 0., 0.]],
                    },
                    diameter: 22.,
                    depth: 40.,
                },
            },
        );
    }
    let plate = p.allocate_id().expect("control model");
    p.elements.insert(
        plate,
        Element {
            id: plate,
            name: "Пластина П1".into(),
            material: ObjectId(1),
            shape: ElementShape::Plate {
                contour: vec![[0., 0.], [400., 0.], [400., 300.], [0., 300.]],
                thickness: LengthMm::new(20.).expect("control model"),
                frame: Frame {
                    origin: Point3([3000., -200., 2850.]),
                    axes: [[0., 1., 0.], [0., 0., 1.], [1., 0., 0.]],
                },
            },
            properties: BTreeMap::new(),
        },
    );
    for (x, y) in [(60., 80.), (340., 220.)] {
        let id = p.allocate_id().expect("control model");
        p.details.insert(
            id,
            Detail {
                id,
                kind: DetailKind::Hole {
                    part: plate,
                    frame: Frame {
                        origin: Point3([x, y, 0.]),
                        ..Frame::default()
                    },
                    diameter: 22.,
                    depth: 20.,
                },
            },
        );
    }
    let assembly = p.allocate_id().expect("control model");
    p.assemblies.insert(
        assembly,
        Assembly {
            id: assembly,
            name: "Балка в сборе Б1".into(),
            main_part: beam,
            members: BTreeSet::from([beam, plate]),
            children: BTreeSet::new(),
            properties: BTreeMap::new(),
        },
    );
    debug_assert!(p.validate().is_ok());
    Fixture {
        project: p,
        beam,
        plate,
        assembly,
    }
}

/// Control building for КМ schemes: grid 1–5 × А–Г (6000 mm), levels 0,000
/// and +3,600, columns at every grid node, beams along rows А–Г at +3,600.
pub fn control_building() -> Project {
    use open_bim_model::numbering_rules::{axis_letter, level_mark};
    use open_bim_model::{GridAxis, Level};
    let mut p = Project {
        name: "Контрольное здание".into(),
        ..Project::default()
    };
    let add_axis = |p: &mut Project, name: String, start: [f64; 3], end: [f64; 3]| {
        let id = p.allocate_id().expect("ids");
        p.axes.insert(
            id,
            GridAxis {
                id,
                name,
                start: Point3(start),
                end: Point3(end),
            },
        );
    };
    for i in 0..5 {
        let x = 6000. * i as f64;
        add_axis(
            &mut p,
            (i + 1).to_string(),
            [x, -2000., 0.],
            [x, 20000., 0.],
        );
    }
    for j in 0..4 {
        let y = 6000. * j as f64;
        add_axis(&mut p, axis_letter(j), [-2000., y, 0.], [26000., y, 0.]);
    }
    for z in [0., 3600.] {
        let id = p.allocate_id().expect("ids");
        p.levels.insert(
            id,
            Level {
                id,
                name: level_mark(z),
                elevation_mm: z,
            },
        );
    }
    let member = |p: &mut Project, kind: MemberKind, name: &str, a: [f64; 3], b: [f64; 3]| {
        let (frame, length) = Frame::along(Point3(a), Point3(b)).expect("member");
        let id = p.allocate_id().expect("ids");
        p.elements.insert(
            id,
            Element {
                id,
                name: name.into(),
                material: ObjectId(1),
                shape: ElementShape::Member {
                    member_kind: kind,
                    profile: ObjectId(2),
                    length,
                    frame,
                },
                properties: Default::default(),
            },
        );
    };
    for j in 0..4 {
        let y = 6000. * j as f64;
        for i in 0..5 {
            let x = 6000. * i as f64;
            member(
                &mut p,
                MemberKind::Column,
                "Колонна",
                [x, y, 0.],
                [x, y, 3600.],
            );
        }
        for i in 0..4 {
            let x = 6000. * i as f64;
            member(
                &mut p,
                MemberKind::Beam,
                "Балка",
                [x, y, 3600.],
                [x + 6000., y, 3600.],
            );
        }
    }
    p
}
