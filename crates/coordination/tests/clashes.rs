use open_bim_coordination::check;
use open_bim_model::{
    Detail, DetailKind, Element, ElementShape, Frame, LengthMm, ObjectId, Point3, Project,
};
use open_bim_numbering::number;
use std::collections::BTreeMap;

#[test]
fn hole_void_and_real_penetration_are_not_bounding_box_contacts() {
    let mut p = Project::default();
    let first = plate(&mut p, [0.; 3]);
    let pin = plate(&mut p, [45., 45., 0.]);
    if let ElementShape::Plate { contour, .. } = &mut p.elements.get_mut(&pin).unwrap().shape {
        *contour = vec![[0., 0.], [10., 0.], [10., 10.], [0., 10.]];
    }
    let hole = p.allocate_id().unwrap();
    p.details.insert(
        hole,
        Detail {
            id: hole,
            kind: DetailKind::Hole {
                part: first,
                frame: Frame {
                    origin: Point3([50., 50., 0.]),
                    ..Frame::default()
                },
                diameter: 22.,
                depth: 10.,
                slot: 0.0,
            },
        },
    );
    let report = check(&p, &number(&p).unwrap()).unwrap();
    assert_eq!(report.candidates, 1);
    assert!(report.conflicts.is_empty());
    p.elements.get_mut(&pin).unwrap().shape.frame_mut().origin.0[0] += 10.;
    let report = check(&p, &number(&p).unwrap()).unwrap();
    assert_eq!(report.conflicts.len(), 1);
    assert_eq!(
        report.conflicts[0].kind,
        open_bim_coordination::ConflictKind::Intersection
    );
    assert!(report.conflicts[0].overlap_mm3 > 0.);
}

#[test]
fn crossing_long_parts_and_containment_have_independent_volumes() {
    let mut p = Project::default();
    let a = plate(&mut p, [0.; 3]);
    let b = plate(&mut p, [0.; 3]);
    for id in [a, b] {
        if let ElementShape::Plate { contour, .. } = &mut p.elements.get_mut(&id).unwrap().shape {
            *contour = vec![[-100., -10.], [100., -10.], [100., 10.], [-100., 10.]];
        }
    }
    p.elements.get_mut(&b).unwrap().shape.frame_mut().axes =
        [[0., 1., 0.], [-1., 0., 0.], [0., 0., 1.]];
    let report = check(&p, &number(&p).unwrap()).unwrap();
    assert!((report.conflicts[0].overlap_mm3 - 4000.).abs() < 1e-5);
    p.elements.get_mut(&b).unwrap().shape.frame_mut().axes = Frame::default().axes;
    if let ElementShape::Plate { contour, .. } = &mut p.elements.get_mut(&b).unwrap().shape {
        *contour = vec![[-5., -5.], [5., -5.], [5., 5.], [-5., 5.]];
    }
    let report = check(&p, &number(&p).unwrap()).unwrap();
    assert!((report.conflicts[0].overlap_mm3 - 1000.).abs() < 1e-5);
}

fn plate(p: &mut Project, origin: [f64; 3]) -> ObjectId {
    let id = p.allocate_id().unwrap();
    p.elements.insert(
        id,
        Element {
            id,
            name: "100x100x10".into(),
            material: ObjectId(1),
            shape: ElementShape::Plate {
                contour: vec![[0., 0.], [100., 0.], [100., 100.], [0., 100.]],
                thickness: LengthMm::new(10.).unwrap(),
                frame: Frame {
                    origin: Point3(origin),
                    ..Frame::default()
                },
            },
            properties: BTreeMap::new(),
        },
    );
    id
}

#[test]
fn independent_box_fixtures_distinguish_contact_and_positive_volume() {
    let cases: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/numbering/clashes.json"
    ))
    .unwrap();
    for c in cases.as_array().unwrap() {
        let mut p = Project::default();
        let first = plate(&mut p, [0.; 3]);
        let offset = std::array::from_fn(|i| c["offset"][i].as_f64().unwrap());
        let second = plate(&mut p, offset);
        let n = number(&p).unwrap();
        let report = check(&p, &n).unwrap_or_else(|e| panic!("{}: {e}", c["name"]));
        if c["kind"].is_null() {
            assert!(report.conflicts.is_empty(), "{}", c["name"]);
            continue;
        }
        assert_eq!(report.conflicts.len(), 1, "{}", c["name"]);
        let conflict = &report.conflicts[0];
        assert_eq!(
            serde_json::to_value(conflict.kind).unwrap(),
            c["kind"],
            "{}",
            c["name"]
        );
        assert_eq!((conflict.first, conflict.second), (first, second));
        let expected = c["volume"].as_f64().unwrap();
        assert!(
            (conflict.overlap_mm3 - expected).abs() < 1e-5,
            "{}: {} vs{expected}",
            c["name"],
            conflict.overlap_mm3
        );
        assert_eq!(conflict.first_position.as_ref(), n.parts.get(&first));
    }
}
