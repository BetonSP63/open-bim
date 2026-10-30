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
                position: Default::default(),
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
                    slot: 0.0,
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
                    slot: 0.0,
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
/// and +3,600, I-section columns of a second test grade at every grid
/// node, channel beams along rows А–Г at +3,600.
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
    let s355 = p.allocate_id().expect("ids");
    p.materials.insert(
        s355,
        open_bim_model::Material {
            id: s355,
            name: "S355 · тестовый материал".into(),
            density_kg_m3: 7850.,
        },
    );
    let member = |p: &mut Project, kind: MemberKind, name: &str, a: [f64; 3], b: [f64; 3]| {
        let (material, profile) = match kind {
            MemberKind::Column => (s355, ObjectId(2)),
            MemberKind::Beam => (ObjectId(1), ObjectId(4)),
        };
        let (frame, length) = Frame::along(Point3(a), Point3(b)).expect("member");
        let id = p.allocate_id().expect("ids");
        p.elements.insert(
            id,
            Element {
                id,
                name: name.into(),
                material,
                shape: ElementShape::Member {
                    member_kind: kind,
                    profile,
                    length,
                    frame,
                    position: Default::default(),
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

/// Control documentation set: the control building with a base plate
/// welded to column 1/А (assembly «Колонна К1»); drawings КМ — «Общие
/// данные» and the arrangement scheme; КМД — «Общие данные», the column
/// assembly drawing and a beam part drawing.
pub fn control_set() -> Result<Project, crate::Error> {
    let mut p = control_building();
    let column = p
        .elements
        .values()
        .find(|e| {
            matches!(
                e.shape,
                ElementShape::Member {
                    member_kind: MemberKind::Column,
                    ..
                }
            ) && e.shape.frame().origin.0 == [0., 0., 0.]
        })
        .map(|e| e.id)
        .ok_or("нет колонны 1/А")?;
    let beam = p
        .elements
        .values()
        .find(|e| e.name == "Балка")
        .map(|e| e.id)
        .ok_or("нет балки")?;
    let next = |p: &mut Project| p.allocate_id().map_err(|e| e.to_string());
    let plate = next(&mut p)?;
    p.elements.insert(
        plate,
        Element {
            id: plate,
            name: "Плита базы".into(),
            material: ObjectId(1),
            shape: ElementShape::Plate {
                contour: vec![[0., 0.], [400., 0.], [400., 400.], [0., 400.]],
                thickness: LengthMm::new(20.).map_err(|e| e.to_string())?,
                frame: Frame {
                    origin: Point3([-200., -200., -20.]),
                    ..Frame::default()
                },
            },
            properties: BTreeMap::new(),
        },
    );
    let weld = next(&mut p)?;
    p.details.insert(
        weld,
        Detail {
            id: weld,
            kind: DetailKind::Weld {
                parts: BTreeSet::from([column, plate]),
                weld_type: open_bim_model::WeldType::Fillet,
                size: 6.,
                path: vec![Point3([-75., -150., 0.]), Point3([75., -150., 0.])],
                shop: true,
                sides: Default::default(),
                around: true,
                process: Default::default(),
                gost: Default::default(),
            },
        },
    );
    let assembly = next(&mut p)?;
    p.assemblies.insert(
        assembly,
        Assembly {
            id: assembly,
            name: "Колонна К1".into(),
            main_part: column,
            members: BTreeSet::from([column, plate]),
            children: BTreeSet::new(),
            properties: BTreeMap::new(),
        },
    );
    let ga = next(&mut p)?;
    let d = crate::general_drawing(&p, ga, Vec::new(), "КМ-1", open_bim_model::SheetSize::A1)?;
    p.drawings.insert(ga, d);
    let ad = next(&mut p)?;
    let d = crate::assembly_drawing(&p, ad, assembly, "КМД-1")?;
    p.drawings.insert(ad, d);
    let pd = next(&mut p)?;
    let d = crate::part_drawing(&p, pd, beam, "КМД-2")?;
    p.drawings.insert(pd, d);
    for mark in ["КМ", "КМД"] {
        let od = next(&mut p)?;
        let d = crate::general_data_drawing(&p, od, mark)?;
        p.drawings.insert(od, d);
    }
    // Fingerprints taken after every object exists.
    let drawings: Vec<_> = p.drawings.values().cloned().collect();
    for d in drawings {
        p.drawings.insert(d.id, crate::updated(&p, &d));
    }
    p.validate().map_err(|e| e.to_string())?;
    Ok(p)
}
