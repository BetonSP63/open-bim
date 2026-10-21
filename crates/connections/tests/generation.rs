use open_bim_commands::{Command, DeletePolicy, Document, project_hash};
use open_bim_connections::{ConnectionInstance, ConnectionTemplate};
use open_bim_model::{ElementShape, ObjectId, Point3, TemplateKind, example_steel_frame};

#[test]
fn base_plate_transaction_regenerates_stable_ids_and_rolls_back_failure() {
    let p = example_steel_frame().unwrap();
    let before = project_hash(&p).unwrap();
    let id = ObjectId(p.next_id);
    let instance = ConnectionInstance {
        id,
        primary: ObjectId(8),
        secondary: None,
        at_end: false,
        template: ConnectionTemplate::from_json(include_bytes!(
            "../../../tests/fixtures/connections/base-plate.json"
        ))
        .unwrap(),
        children: Default::default(),
        general: Default::default(),
    };
    let mut doc = Document::new(p).unwrap();
    doc.transact(
        "Base plate",
        &[Command::CreateConnection {
            connection: instance,
        }],
    )
    .unwrap();
    let children = doc.project().connections[&id].children.clone();
    assert_eq!(children.len(), 7);
    assert_eq!(doc.project().elements.len(), 8);
    assert_eq!(doc.project().details.len(), 6);
    let initial = project_hash(doc.project()).unwrap();
    let saved = open_bim_persistence::encode(doc.project()).unwrap();
    let reopened = open_bim_persistence::decode(&saved).unwrap();
    assert_eq!(project_hash(&reopened).unwrap(), initial);
    assert_eq!(reopened.connections[&id].children, children);
    let next_id = doc.project().next_id;
    doc.undo().unwrap();
    assert_eq!(project_hash(doc.project()).unwrap(), before);
    doc.redo().unwrap();
    assert_eq!(project_hash(doc.project()).unwrap(), initial);
    for n in 0..5 {
        let mut parent = doc.project().elements[&ObjectId(8)].clone();
        parent.shape.frame_mut().origin = Point3([n as f64 * 100., 200., 300.]);
        if let ElementShape::Member { profile, .. } = &mut parent.shape {
            *profile = ObjectId(if n % 2 == 0 { 2 } else { 7 });
        }
        doc.transact(
            "Parent edit",
            &[Command::ReplaceElement { element: parent }],
        )
        .unwrap();
        assert_eq!(doc.project().connections[&id].children, children);
        assert_eq!(doc.project().next_id, next_id);
        assert_eq!(doc.project().elements.len(), 8);
        assert_eq!(doc.project().details.len(), 6);
        let ElementShape::Plate { contour, .. } = &doc.project().elements[&children["plate"]].shape
        else {
            panic!()
        };
        assert_eq!(
            contour[2],
            if n % 2 == 0 {
                [290., 440.]
            } else {
                [190., 240.]
            }
        );
        assert_eq!(
            doc.project().elements[&children["plate"]]
                .shape
                .frame()
                .origin
                .0[2],
            280.
        );
    }
    let valid = project_hash(doc.project()).unwrap();
    let history = doc.history_depth();
    let mut impossible = doc.project().connections[&id].clone();
    impossible.template.kind = TemplateKind::BasePlate { overhang: 1.0 };
    assert!(
        doc.transact(
            "Impossible",
            &[Command::ReplaceConnection {
                connection: impossible
            }]
        )
        .unwrap_err()
        .to_string()
        .contains("параметры")
    );
    assert_eq!(project_hash(doc.project()).unwrap(), valid);
    assert_eq!(doc.history_depth(), history);
    doc.transact(
        "Delete parent",
        &[Command::Delete {
            id: ObjectId(8),
            policy: DeletePolicy::Cascade,
        }],
    )
    .unwrap();
    assert!(doc.project().connections.is_empty());
    assert!(children.values().all(|id| !doc.project().contains_id(*id)));
    doc.undo().unwrap();
    assert_eq!(project_hash(doc.project()).unwrap(), valid);
}
