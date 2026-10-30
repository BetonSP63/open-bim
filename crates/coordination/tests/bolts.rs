use open_bim_commands::Command;
use open_bim_coordination::{BOLT_SHAFT_MARGIN_MM, ConflictKind, check};
use open_bim_model::{Element, ElementShape, Frame, LengthMm, ObjectId, Point3, Project};
use open_bim_numbering::number;
use open_bim_steel::{BoltPattern, bolt_commands};
use std::collections::{BTreeMap, BTreeSet};

fn plate(p: &mut Project, z: f64, thickness: f64) -> ObjectId {
    let id = p.allocate_id().unwrap();
    p.elements.insert(
        id,
        Element {
            id,
            name: "PL".into(),
            material: ObjectId(1),
            shape: ElementShape::Plate {
                contour: vec![[0., 0.], [100., 0.], [100., 100.], [0., 100.]],
                thickness: LengthMm::new(thickness).unwrap(),
                frame: Frame {
                    origin: Point3([0., 0., z]),
                    ..Frame::default()
                },
            },
            properties: BTreeMap::new(),
        },
    );
    id
}

/// Two 10 mm plates bolted by a 2x2 M20 group with 2 mm clearance holes.
fn bolted(hole_depth: f64) -> (Project, ObjectId, ObjectId, ObjectId) {
    let mut p = Project::default();
    let a = plate(&mut p, 0., 10.);
    let b = plate(&mut p, 10., 10.);
    let pattern = BoltPattern {
        parts: BTreeSet::from([a, b]),
        frame: Frame {
            origin: Point3([25., 25., 0.]),
            ..Frame::default()
        },
        diameter: 20.,
        clearance: 2.,
        hole_depth,
        steps_x: vec![50.],
        steps_y: vec![50.],
        standard: "ГОСТ 7798".into(),
        grade: "8.8".into(),
        points: Vec::new(),
        slot: 0.0,
    };
    let mut group = ObjectId(0);
    for command in bolt_commands(&p, &pattern).unwrap() {
        let Command::CreateDetail { detail } = command else {
            panic!("unexpected command");
        };
        if matches!(detail.kind, open_bim_model::DetailKind::BoltGroup { .. }) {
            group = detail.id;
        }
        p.next_id = p.next_id.max(detail.id.0 + 1);
        p.details.insert(detail.id, detail);
    }
    p.validate().unwrap();
    (p, a, b, group)
}

#[test]
fn fitted_bolts_are_allowed_holes_and_plates_only_touch() {
    let (p, a, b, group) = bolted(20.);
    let report = check(&p, &number(&p).unwrap()).unwrap();
    let kinds = |kind| report.conflicts.iter().filter(move |c| c.kind == kind);
    assert_eq!(kinds(ConflictKind::Intersection).count(), 0);
    let touch: Vec<_> = kinds(ConflictKind::Touching).collect();
    assert_eq!(touch.len(), 1);
    assert_eq!((touch[0].first, touch[0].second), (a, b));
    let fits: Vec<_> = kinds(ConflictKind::AllowedBoltHole).collect();
    assert_eq!(fits.len(), 8, "4 bolts x 2 plates");
    for part in [a, b] {
        let bolts: BTreeSet<_> = fits
            .iter()
            .filter(|c| c.second == part)
            .map(|c| c.bolt.unwrap())
            .collect();
        assert_eq!(bolts, BTreeSet::from([0, 1, 2, 3]));
    }
    assert!(fits.iter().all(|c| c.first == group && c.overlap_mm3 == 0.));
    let centres: BTreeSet<_> = fits
        .iter()
        .map(|c| {
            (
                c.location[0] as i64,
                c.location[1] as i64,
                c.location[2] as i64,
            )
        })
        .collect();
    assert_eq!(
        centres,
        BTreeSet::from([(25, 25, 10), (25, 75, 10), (75, 25, 10), (75, 75, 10)])
    );
    assert_eq!(report.blocking().count(), 0);
}

#[test]
fn bolt_through_foreign_part_without_holes_is_an_intersection() {
    // Holes 30 mm deep: the shafts continue into a third plate that has no holes.
    let (mut p, _, _, group) = bolted(30.);
    let foreign = plate(&mut p, 20., 10.);
    let report = check(&p, &number(&p).unwrap()).unwrap();
    let hits: Vec<_> = report
        .conflicts
        .iter()
        .filter(|c| c.kind == ConflictKind::Intersection)
        .collect();
    assert_eq!(hits.len(), 4);
    // Independent analytic area of the inscribed 32-gon shaft cross-section.
    let r = 10. - BOLT_SHAFT_MARGIN_MM;
    let n = 32.;
    let expected = n / 2. * r * r * (std::f64::consts::TAU / n).sin() * 10.;
    for hit in hits {
        assert_eq!((hit.first, hit.second), (group, foreign));
        assert!(
            (hit.overlap_mm3 - expected).abs() < 1e-6,
            "{} vs {expected}",
            hit.overlap_mm3
        );
        assert!((hit.location[2] - 15.).abs() < 1e-9);
    }
    assert_eq!(
        report
            .conflicts
            .iter()
            .filter(|c| c.kind == ConflictKind::AllowedBoltHole)
            .count(),
        8
    );
}
