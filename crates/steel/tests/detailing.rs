use open_bim_commands::{Command, DeletePolicy, Document, project_hash};
use open_bim_model::{
    Detail, DetailKind, Element, ElementShape, Frame, LengthMm, ObjectId, Point3, Project, WeldType,
};
use open_bim_steel::{BoltPattern, FabricationRow, bolt_commands, fabrication_rows};
use std::collections::{BTreeMap, BTreeSet};

fn plates() -> Project {
    let mut p = Project::default();
    for (id, z) in [(8, 0.), (9, 20.)] {
        p.elements.insert(
            ObjectId(id),
            Element {
                id: ObjectId(id),
                name: format!("Plate {id}"),
                material: ObjectId(1),
                shape: ElementShape::Plate {
                    contour: vec![[0., 0.], [200., 0.], [200., 200.], [0., 200.]],
                    thickness: LengthMm::new(20.).unwrap(),
                    frame: Frame {
                        origin: Point3([0., 0., z]),
                        ..Frame::default()
                    },
                },
                properties: BTreeMap::new(),
            },
        );
    }
    p.next_id = 10;
    p.validate().unwrap();
    p
}
fn pattern() -> BoltPattern {
    BoltPattern {
        parts: BTreeSet::from([ObjectId(8), ObjectId(9)]),
        frame: Frame {
            origin: Point3([40., 40., -1.]),
            ..Frame::default()
        },
        diameter: 20.,
        clearance: 2.,
        hole_depth: 42.,
        steps_x: vec![50., 50.],
        steps_y: vec![40.],
        standard: "ISO 4014".into(),
        grade: "8.8".into(),
        points: Vec::new(),
    }
}

#[test]
fn bolts_welds_roundtrip_history_and_deletion_dependencies() {
    let p = plates();
    let start = project_hash(&p).unwrap();
    let mut doc = Document::new(p.clone()).unwrap();
    let mut commands = bolt_commands(&p, &pattern()).unwrap();
    let id = ObjectId(p.next_id + commands.len() as u64);
    commands.push(Command::CreateDetail {
        detail: Detail {
            id,
            kind: DetailKind::Weld {
                parts: pattern().parts,
                weld_type: WeldType::Fillet,
                size: 6.,
                shop: true,
                sides: open_bim_model::WeldSides::One,
                around: false,
                path: vec![
                    Point3([0., 0., 20.]),
                    Point3([30., 40., 20.]),
                    Point3([30., 40., 40.]),
                ],
                process: Default::default(),
                gost: Default::default(),
            },
        },
    });
    let event = doc.transact("Bolts and weld", &commands).unwrap();
    assert!(event.touched.contains(&ObjectId(8)) && event.touched.contains(&ObjectId(9)));
    let rows = fabrication_rows(doc.project()).unwrap();
    assert!(
        matches!(&rows[0],FabricationRow::Bolts{count:6,diameter_mm:20.,standard,grade,..} if standard=="ISO 4014" && grade=="8.8")
    );
    assert!(matches!(
        &rows[1],
        FabricationRow::Weld {
            length_mm: 70.,
            size_mm: 6.,
            shop: true,
            ..
        }
    ));
    assert_eq!(doc.project().details.len(), 14);
    let saved = open_bim_persistence::encode(doc.project()).unwrap();
    let loaded = open_bim_persistence::decode(&saved).unwrap();
    assert_eq!(&loaded, doc.project());
    let after = project_hash(doc.project()).unwrap();
    doc.undo().unwrap();
    assert_eq!(project_hash(doc.project()).unwrap(), start);
    doc.redo().unwrap();
    assert_eq!(project_hash(doc.project()).unwrap(), after);
    assert!(
        doc.transact(
            "Reject linked deletion",
            &[Command::Delete {
                id: ObjectId(8),
                policy: DeletePolicy::RejectDependencies
            }]
        )
        .is_err()
    );
    assert_eq!(project_hash(doc.project()).unwrap(), after);
    doc.transact(
        "Cascade",
        &[Command::Delete {
            id: ObjectId(8),
            policy: DeletePolicy::Cascade,
        }],
    )
    .unwrap();
    assert!(doc.project().details.is_empty());
    assert!(doc.project().elements.contains_key(&ObjectId(9)));
    doc.undo().unwrap();
    assert_eq!(project_hash(doc.project()).unwrap(), after);
}

#[test]
fn invalid_pattern_and_missing_hole_leave_document_unchanged() {
    let p = plates();
    let hash = project_hash(&p).unwrap();
    for changed in [f64::NAN, -1., 0., 50.] {
        let mut b = pattern();
        b.diameter = changed;
        assert!(bolt_commands(&p, &b).is_err());
    }
    let mut b = pattern();
    b.steps_x = vec![10.];
    assert!(bolt_commands(&p, &b).is_err());
    let commands = bolt_commands(&p, &pattern()).unwrap();
    let mut doc = Document::new(p.clone()).unwrap();
    assert!(
        doc.transact("Missing holes", &commands[commands.len() - 1..])
            .is_err()
    );
    assert_eq!(project_hash(doc.project()).unwrap(), hash);
    doc.transact("Complete", &commands).unwrap();
    let before = project_hash(doc.project()).unwrap();
    let hole = doc
        .project()
        .details
        .values()
        .find(|d| matches!(d.kind, DetailKind::Hole { .. }))
        .unwrap();
    let mut broken = hole.clone();
    if let DetailKind::Hole { diameter, .. } = &mut broken.kind {
        *diameter = 25.;
    }
    assert!(
        doc.transact("Mismatch", &[Command::ReplaceDetail { detail: broken }])
            .is_err()
    );
    assert_eq!(project_hash(doc.project()).unwrap(), before);
}

#[test]
fn empty_detailing_preserves_legacy_canonical_hash() {
    let p = open_bim_model::example_steel_frame().unwrap();
    let old = std::fs::read_to_string("../../tests/fixtures/model/steel-frame.json").unwrap();
    let loaded = Project::from_json(&old).unwrap();
    assert_eq!(project_hash(&loaded).unwrap(), project_hash(&p).unwrap());
}

#[test]
fn bolt_pattern_rejects_displaced_duplicate_and_tilted_holes() {
    let mut doc = Document::new(plates()).unwrap();
    let commands = bolt_commands(doc.project(), &pattern()).unwrap();
    doc.transact("Pattern", &commands).unwrap();
    let original = doc.project().clone();
    let holes: Vec<_> = original
        .details
        .values()
        .filter(|d| matches!(d.kind, DetailKind::Hole { .. }))
        .map(|d| d.id)
        .collect();
    for mode in 0..3 {
        let mut p = original.clone();
        let DetailKind::Hole { frame: other, .. } = &original.details[&holes[1]].kind else {
            unreachable!()
        };
        let DetailKind::Hole { frame, .. } = &mut p.details.get_mut(&holes[0]).unwrap().kind else {
            unreachable!()
        };
        match mode {
            0 => frame.origin.0[0] += 1.,
            1 => *frame = *other,
            _ => frame.axes = [[0., 1., 0.], [0., 0., 1.], [1., 0., 0.]],
        }
        assert!(p.validate().is_err(), "invalid mode {mode}");
    }
}
