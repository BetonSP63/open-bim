use open_bim_commands::{Command, DeletePolicy, Document, project_hash};
use open_bim_connections::TemplateKind;
use open_bim_connections::{ConnectionInstance, ConnectionTemplate};
use open_bim_model::{
    DetailKind, Element, ElementShape, Frame, LengthMm, MemberKind, ObjectId, Point3, Project,
};

fn fixture(at_end: bool) -> Project {
    let mut p = Project::default();
    p.elements.insert(
        ObjectId(8),
        Element {
            id: ObjectId(8),
            name: "Primary I300".into(),
            material: ObjectId(1),
            properties: Default::default(),
            shape: ElementShape::Member {
                member_kind: MemberKind::Column,
                profile: ObjectId(2),
                length: LengthMm::new(3000.).unwrap(),
                frame: Frame::default(),
                position: Default::default(),
            },
        },
    );
    p.elements.insert(
        ObjectId(9),
        Element {
            id: ObjectId(9),
            name: "Support".into(),
            material: ObjectId(1),
            properties: Default::default(),
            shape: ElementShape::Plate {
                contour: vec![[0., 0.], [600., 0.], [600., 600.], [0., 600.]],
                thickness: LengthMm::new(30.).unwrap(),
                frame: Frame {
                    origin: Point3([-300., -300., if at_end { 3020. } else { -50. }]),
                    ..Frame::default()
                },
            },
        },
    );
    p.next_id = 10;
    p.validate().unwrap();
    p
}

#[test]
fn changing_end_to_base_preserves_shared_ids_and_removes_support_holes() {
    let template = ConnectionTemplate::from_json(include_bytes!(
        "../../../tests/fixtures/connections/end-plate.json"
    ))
    .unwrap();
    let mut doc = Document::new(fixture(true)).unwrap();
    doc.transact(
        "End",
        &[Command::CreateConnection {
            connection: ConnectionInstance {
                id: ObjectId(10),
                primary: ObjectId(8),
                secondary: Some(ObjectId(9)),
                at_end: true,
                template,
                children: Default::default(),
                general: Default::default(),
            },
        }],
    )
    .unwrap();
    let original = doc.project().connections[&ObjectId(10)].clone();
    let hash = project_hash(doc.project()).unwrap();
    let next = doc.project().next_id;
    let mut base = original.clone();
    base.secondary = None;
    base.template.kind = TemplateKind::BasePlate { overhang: 70. };
    doc.transact(
        "Change template",
        &[Command::ReplaceConnection { connection: base }],
    )
    .unwrap();
    assert_eq!(doc.project().connections[&ObjectId(10)].children.len(), 7);
    assert_eq!(doc.project().details.len(), 6);
    assert_eq!(doc.project().next_id, next);
    for (role, id) in &original.children {
        if role.starts_with("support-") {
            assert!(!doc.project().contains_id(*id));
        } else {
            assert_eq!(doc.project().connections[&ObjectId(10)].children[role], *id);
        }
    }
    assert_eq!(
        doc.project().element_volume_mm3(ObjectId(9)).unwrap(),
        600. * 600. * 30.
    );
    let base_hash = project_hash(doc.project()).unwrap();
    doc.undo().unwrap();
    assert_eq!(project_hash(doc.project()).unwrap(), hash);
    doc.redo().unwrap();
    assert_eq!(project_hash(doc.project()).unwrap(), base_hash);
}

#[test]
fn exported_template_applies_to_different_parents_without_id_or_object_leaks() {
    let mut p = fixture(true);
    for (old, new) in [(8, 20), (9, 21)] {
        let mut copy = p.elements[&ObjectId(old)].clone();
        copy.id = ObjectId(new);
        copy.shape.frame_mut().origin.0[0] += 1000.;
        p.elements.insert(copy.id, copy);
    }
    p.next_id = 22;
    p.validate().unwrap();
    let template = ConnectionTemplate::from_json(include_bytes!(
        "../../../tests/fixtures/connections/end-plate.json"
    ))
    .unwrap();
    let exported = template.to_json().unwrap();
    let reused = ConnectionTemplate::from_json(exported.as_bytes()).unwrap();
    let mut doc = Document::new(p).unwrap();
    doc.transact(
        "Apply twice",
        &[
            Command::CreateConnection {
                connection: ConnectionInstance {
                    id: ObjectId(22),
                    primary: ObjectId(8),
                    secondary: Some(ObjectId(9)),
                    at_end: true,
                    template,
                    children: Default::default(),
                    general: Default::default(),
                },
            },
            Command::CreateConnection {
                connection: ConnectionInstance {
                    id: ObjectId(23),
                    primary: ObjectId(20),
                    secondary: Some(ObjectId(21)),
                    at_end: true,
                    template: reused,
                    children: Default::default(),
                    general: Default::default(),
                },
            },
        ],
    )
    .unwrap();
    assert_eq!(doc.project().elements.len(), 6);
    assert_eq!(doc.project().details.len(), 20);
    let a = &doc.project().connections[&ObjectId(22)];
    let b = &doc.project().connections[&ObjectId(23)];
    assert_eq!(a.template, b.template);
    assert!(
        a.children
            .values()
            .all(|id| !b.children.values().any(|other| other == id))
    );
    let x = doc.project().elements[&a.children["plate"]]
        .shape
        .frame()
        .origin
        .0[0];
    let other = doc.project().elements[&b.children["plate"]]
        .shape
        .frame()
        .origin
        .0[0];
    assert_eq!(other - x, 1000.);
    let bytes = open_bim_persistence::encode(doc.project()).unwrap();
    assert_eq!(
        open_bim_persistence::decode(&bytes).unwrap(),
        *doc.project()
    );
    doc.undo().unwrap();
    assert_eq!(doc.project().elements.len(), 4);
    assert!(doc.project().details.is_empty());
    assert!(doc.project().connections.is_empty());
    doc.redo().unwrap();
    assert_eq!(doc.project().elements.len(), 6);
    assert_eq!(doc.project().details.len(), 20);
}

#[test]
fn end_plate_drills_both_parts_tracks_both_parents_and_preserves_history() {
    for at_end in [true, false] {
        let p = fixture(at_end);
        let before = project_hash(&p).unwrap();
        let template = ConnectionTemplate::from_json(include_bytes!(
            "../../../tests/fixtures/connections/end-plate.json"
        ))
        .unwrap();
        let mut doc = Document::new(p).unwrap();
        doc.transact(
            "End plate",
            &[Command::CreateConnection {
                connection: ConnectionInstance {
                    id: ObjectId(10),
                    primary: ObjectId(8),
                    secondary: Some(ObjectId(9)),
                    at_end,
                    template,
                    children: Default::default(),
                    general: Default::default(),
                },
            }],
        )
        .unwrap();
        let instance = doc.project().connections[&ObjectId(10)].clone();
        assert_eq!(instance.children.len(), 11);
        assert_eq!(doc.project().elements.len(), 3);
        assert_eq!(doc.project().details.len(), 10);
        let plate = instance.children["plate"];
        let ElementShape::Plate { contour, .. } = &doc.project().elements[&plate].shape else {
            panic!()
        };
        assert_eq!(contour[2], [290., 440.]);
        let DetailKind::BoltGroup {
            parts,
            holes,
            steps_x,
            steps_y,
            ..
        } = &doc.project().details[&instance.children["bolts"]].kind
        else {
            panic!()
        };
        assert_eq!(parts.len(), 2);
        assert_eq!(holes.len(), 8);
        assert_eq!((steps_x.len() + 1) * (steps_y.len() + 1), 4);
        let expected_plate = (290. * 440. - 4. * std::f64::consts::PI * 121.) * 20.;
        let expected_support = (600. * 600. - 4. * std::f64::consts::PI * 121.) * 30.;
        for (part, expected) in [(plate, expected_plate), (ObjectId(9), expected_support)] {
            assert!((doc.project().element_volume_mm3(part).unwrap() / expected - 1.).abs() < 1e-6);
        }
        let created = project_hash(doc.project()).unwrap();
        let saved = open_bim_persistence::encode(doc.project()).unwrap();
        assert_eq!(
            project_hash(&open_bim_persistence::decode(&saved).unwrap()).unwrap(),
            created
        );
        doc.undo().unwrap();
        assert_eq!(project_hash(doc.project()).unwrap(), before);
        doc.redo().unwrap();
        assert_eq!(project_hash(doc.project()).unwrap(), created);
        let next = doc.project().next_id;
        let mut parent = doc.project().elements[&ObjectId(8)].clone();
        parent.shape.frame_mut().origin.0[0] += 10.;
        doc.transact(
            "Primary move",
            &[Command::ReplaceElement { element: parent }],
        )
        .unwrap();
        assert_eq!(
            doc.project().connections[&ObjectId(10)].children,
            instance.children
        );
        assert_eq!(doc.project().next_id, next);
        let mut support = doc.project().elements[&ObjectId(9)].clone();
        support.shape.frame_mut().origin.0[0] += 10.;
        doc.transact(
            "Support move",
            &[Command::ReplaceElement { element: support }],
        )
        .unwrap();
        assert_eq!(doc.project().next_id, next);
        let valid = project_hash(doc.project()).unwrap();
        let history = doc.history_depth();
        let mut child = doc.project().elements[&plate].clone();
        child.shape.frame_mut().origin.0[0] += 1.;
        assert!(
            doc.transact(
                "Direct generated edit",
                &[Command::ReplaceElement { element: child }]
            )
            .unwrap_err()
            .to_string()
            .contains("управляется соединением")
        );
        assert_eq!(project_hash(doc.project()).unwrap(), valid);
        assert_eq!(doc.history_depth(), history);
        let mut support = doc.project().elements[&ObjectId(9)].clone();
        support.shape.frame_mut().origin.0[2] += 1.;
        // A gap no longer refuses the move of the support: the end plate
        // stays in the model «не создан» without its objects; undo brings it back.
        doc.transact("Gap", &[Command::ReplaceElement { element: support }])
            .unwrap();
        let failed = doc.project().connections.values().next().unwrap();
        assert!(failed.children.is_empty());
        assert!(!doc.project().elements.contains_key(&plate));
        doc.undo().unwrap();
        assert_eq!(project_hash(doc.project()).unwrap(), valid);
        assert_eq!(doc.history_depth().0, history.0);
        doc.transact(
            "Delete support",
            &[Command::Delete {
                id: ObjectId(9),
                policy: DeletePolicy::Cascade,
            }],
        )
        .unwrap();
        assert!(doc.project().connections.is_empty());
        assert!(
            instance
                .children
                .values()
                .all(|id| !doc.project().contains_id(*id))
        );
        assert_eq!(doc.project().elements.len(), 1);
        doc.undo().unwrap();
        assert_eq!(project_hash(doc.project()).unwrap(), valid);
    }
}
