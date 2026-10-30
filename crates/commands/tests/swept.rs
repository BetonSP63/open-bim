use open_bim_commands::{Command, DeletePolicy, Document};
use open_bim_model::{
    Element, ElementShape, Frame, LengthMm, MemberKind, ObjectId, Point3, Project, SweptPart,
};
use std::collections::BTreeMap;

fn part() -> Element {
    Element {
        id: ObjectId(8),
        name: "Гнутая балка".into(),
        material: ObjectId(1),
        properties: BTreeMap::new(),
        shape: ElementShape::Member {
            member_kind: MemberKind::Beam,
            profile: ObjectId(7),
            length: LengthMm::new(1000.).unwrap(),
            frame: Frame::default(),
            position: Default::default(),
        },
    }
}
fn sweep() -> SweptPart {
    SweptPart::CurvedBeam {
        center: [0., 0.],
        radius: 1000.,
        start_angle: 0.,
        sweep_angle: 1.,
    }
}

#[test]
fn swept_part_creation_transformation_deletion_and_history_are_atomic() {
    let mut doc = Document::new(Project::default()).unwrap();
    let id = ObjectId(8);
    doc.transact(
        "Создание",
        &[Command::CreateSwept {
            element: part(),
            swept: sweep(),
        }],
    )
    .unwrap();
    assert_eq!(doc.project().swept_parts.get(&id), Some(&sweep()));
    doc.undo().unwrap();
    assert!(!doc.project().elements.contains_key(&id));
    assert!(!doc.project().swept_parts.contains_key(&id));
    doc.redo().unwrap();
    assert_eq!(doc.project().swept_parts.get(&id), Some(&sweep()));
    doc.transact(
        "Сдвиг",
        &[Command::Transform {
            ids: vec![id],
            rotation: Frame {
                origin: Point3([10., 20., 30.]),
                ..Default::default()
            },
            pivot: Point3([0.; 3]),
        }],
    )
    .unwrap();
    assert_eq!(
        doc.project().elements[&id].shape.frame().origin,
        Point3([10., 20., 30.])
    );
    assert_eq!(doc.project().swept_parts.get(&id), Some(&sweep()));
    doc.transact(
        "Удаление",
        &[Command::Delete {
            id,
            policy: DeletePolicy::Cascade,
        }],
    )
    .unwrap();
    assert!(!doc.project().swept_parts.contains_key(&id));
    doc.undo().unwrap();
    assert_eq!(doc.project().swept_parts.get(&id), Some(&sweep()));
    assert_eq!(
        doc.project().elements[&id].shape.frame().origin,
        Point3([10., 20., 30.])
    );
    doc.redo().unwrap();
    assert!(!doc.project().swept_parts.contains_key(&id));
}

#[test]
fn incompatible_changes_and_invalid_creation_roll_back() {
    let mut doc = Document::new(Project::default()).unwrap();
    let invalid = SweptPart::CurvedBeam {
        center: [0., 0.],
        radius: -1.,
        start_angle: 0.,
        sweep_angle: 1.,
    };
    assert!(
        doc.transact(
            "Ошибка",
            &[Command::CreateSwept {
                element: part(),
                swept: invalid
            }]
        )
        .is_err()
    );
    assert!(doc.project().elements.is_empty());
    assert!(doc.project().swept_parts.is_empty());
    doc.transact(
        "Создание",
        &[Command::CreateSwept {
            element: part(),
            swept: sweep(),
        }],
    )
    .unwrap();
    let before = doc.project().clone();
    let mut changed = part();
    if let ElementShape::Member { length, .. } = &mut changed.shape {
        *length = LengthMm::new(2000.).unwrap();
    }
    assert!(
        doc.transact("Изменение", &[Command::ReplaceElement { element: changed }])
            .is_err()
    );
    assert_eq!(doc.project(), &before);
}
