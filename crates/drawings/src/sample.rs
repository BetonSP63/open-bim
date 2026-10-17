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
