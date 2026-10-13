use open_bim_commands::{Command, DeletePolicy, Document, project_hash};
use open_bim_connections::{ConnectionInstance, ConnectionTemplate};
use open_bim_model::{
    DetailKind, Element, ElementShape, Frame, LengthMm, MemberKind, ObjectId, Point3, Profile,
    ProfileShape, Project, TemplateKind,
};

fn fixture(at_end: bool) -> Project {
    let mut p = Project::default();
    p.elements.insert(
        ObjectId(8),
        Element {
            id: ObjectId(8),
            name: "I300".into(),
            material: ObjectId(1),
            properties: Default::default(),
            shape: ElementShape::Member {
                member_kind: MemberKind::Beam,
                profile: ObjectId(2),
                length: LengthMm::new(3000.).unwrap(),
                frame: Frame::default(),
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
                    origin: Point3([-300., -300., if at_end { 3000. } else { -30. }]),
                    ..Frame::default()
                },
            },
        },
    );
    p.profiles.insert(
        ObjectId(30),
        Profile {
            id: ObjectId(30),
            name: "I320 reference".into(),
            shape: ProfileShape::ISection {
                height: 320.,
                width: 160.,
                web: 10.,
                flange: 14.,
            },
            source: "Analytical test section".into(),
        },
    );
    p.next_id = 31;
    p.validate().unwrap();
    p
}

#[test]
fn actual_angle_drills_both_legs_and_parents_without_profile_or_id_leaks() {
    let reference: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/connections/clip-masses.json"
    ))
    .unwrap();
    for at_end in [true, false] {
        let p = fixture(at_end);
        let before = project_hash(&p).unwrap();
        let profiles = p.profiles.len();
        let template = ConnectionTemplate::from_json(include_bytes!(
            "../../../tests/fixtures/connections/clip-angle.json"
        ))
        .unwrap();
        let mut doc = Document::new(p).unwrap();
        doc.transact(
            "Clip angle",
            &[Command::CreateConnection {
                connection: ConnectionInstance {
                    id: ObjectId(10),
                    primary: ObjectId(8),
                    secondary: Some(ObjectId(9)),
                    at_end,
                    template,
                    children: Default::default(),
                },
            }],
        )
        .unwrap();
        let instance = doc.project().connections[&ObjectId(10)].clone();
        let original = project_hash(doc.project()).unwrap();
        let mut foreign = doc.project().elements[&instance.children["angle"]].clone();
        foreign.id = ObjectId(doc.project().next_id);
        assert!(
            doc.transact(
                "Borrow owned profile",
                &[Command::Create { element: foreign }]
            )
            .unwrap_err()
            .to_string()
            .contains("Профиль принадлежит")
        );
        assert_eq!(project_hash(doc.project()).unwrap(), original);
        let mut malformed = doc.project().clone();
        let profile = instance.children["angle-profile"];
        malformed.profiles.get_mut(&profile).unwrap().shape = ProfileShape::Angle {
            height: 120.,
            width: 120.,
            thickness: 20.,
        };
        assert!(malformed.validate().is_err());
        let mut malformed = doc.project().clone();
        let value = malformed
            .connections
            .get_mut(&ObjectId(10))
            .unwrap()
            .children
            .remove("angle-hole-0")
            .unwrap();
        malformed
            .connections
            .get_mut(&ObjectId(10))
            .unwrap()
            .children
            .insert("unknown-role".into(), value);
        assert!(malformed.validate().is_err());
        assert_eq!(instance.children.len(), 12);
        assert_eq!(doc.project().profiles.len(), profiles + 1);
        assert_eq!(doc.project().elements.len(), 3);
        assert_eq!(doc.project().details.len(), 10);
        for (name, part) in [
            ("angle", instance.children["angle"]),
            ("primary", ObjectId(8)),
            ("support", ObjectId(9)),
        ] {
            let expected = reference[name]["volume_mm3"].as_f64().unwrap();
            let body = doc.project().element_body(part).unwrap();
            assert!(
                (body.volume_mm3().unwrap() / expected - 1.).abs() < 1e-6,
                "{name}"
            );
            let (p, t) = body.tessellation_f64().unwrap();
            let v: f64 = t
                .iter()
                .map(|f| {
                    let [a, b, c] = f.map(|i| p[i as usize]);
                    (a[0] * (b[1] * c[2] - b[2] * c[1])
                        + a[1] * (b[2] * c[0] - b[0] * c[2])
                        + a[2] * (b[0] * c[1] - b[1] * c[0]))
                        / 6.
                })
                .sum();
            assert!((v / expected - 1.).abs() < 1e-6, "{name} mesh");
            assert!(
                (doc.project().element_mass_kg(part).unwrap()
                    / reference[name]["mass_kg"].as_f64().unwrap()
                    - 1.)
                    .abs()
                    < 1e-6
            );
        }
        for role in ["primary-bolts", "support-bolts"] {
            let DetailKind::BoltGroup {
                parts,
                holes,
                steps_x,
                steps_y,
                ..
            } = &doc.project().details[&instance.children[role]].kind
            else {
                panic!()
            };
            assert_eq!(parts.len(), 2);
            assert_eq!(holes.len(), 4);
            assert_eq!((steps_x.len() + 1) * (steps_y.len() + 1), 2);
        }
        let created = project_hash(doc.project()).unwrap();
        let next = doc.project().next_id;
        let bytes = open_bim_persistence::encode(doc.project()).unwrap();
        assert_eq!(
            open_bim_persistence::decode(&bytes).unwrap(),
            *doc.project()
        );
        doc.undo().unwrap();
        assert_eq!(project_hash(doc.project()).unwrap(), before);
        doc.redo().unwrap();
        assert_eq!(project_hash(doc.project()).unwrap(), created);
        let mut primary = doc.project().elements[&ObjectId(8)].clone();
        primary.shape.frame_mut().origin.0[0] += 10.;
        doc.transact(
            "Move primary",
            &[Command::ReplaceElement { element: primary }],
        )
        .unwrap();
        assert_eq!(
            doc.project().connections[&ObjectId(10)].children,
            instance.children
        );
        assert_eq!(doc.project().next_id, next);
        let mut primary = doc.project().elements[&ObjectId(8)].clone();
        if let ElementShape::Member { profile, .. } = &mut primary.shape {
            *profile = ObjectId(30);
        }
        doc.transact(
            "Section change",
            &[Command::ReplaceElement { element: primary }],
        )
        .unwrap();
        assert_eq!(
            doc.project().connections[&ObjectId(10)].children,
            instance.children
        );
        assert_eq!(doc.project().next_id, next);
        let expected = (7400. * 3000. - 2. * std::f64::consts::PI * 121. * 10.) * 7850. / 1e9;
        assert!((doc.project().element_mass_kg(ObjectId(8)).unwrap() / expected - 1.).abs() < 1e-6);
        let mut edited = doc.project().connections[&ObjectId(10)].clone();
        edited.template.kind = TemplateKind::ClipAngle {
            leg: 100.,
            height: 190.,
            pitch: 80.,
        };
        doc.transact(
            "Angle height",
            &[Command::ReplaceConnection { connection: edited }],
        )
        .unwrap();
        assert_eq!(
            doc.project().connections[&ObjectId(10)].children,
            instance.children
        );
        assert_eq!(doc.project().next_id, next);
        assert_eq!(doc.project().profiles.len(), profiles + 1);
        let expected = ((2. * 100. * 20. - 400.) * 190. - 4. * std::f64::consts::PI * 121. * 20.)
            * 7850.
            / 1e9;
        assert!(
            (doc.project()
                .element_mass_kg(instance.children["angle"])
                .unwrap()
                / expected
                - 1.)
                .abs()
                < 1e-6
        );
        let mut impossible = doc.project().connections[&ObjectId(10)].clone();
        impossible.template.kind = TemplateKind::ClipAngle {
            leg: 100.,
            height: 500.,
            pitch: 80.,
        };
        let valid = project_hash(doc.project()).unwrap();
        let history = doc.history_depth();
        assert!(
            doc.transact(
                "Too tall",
                &[Command::ReplaceConnection {
                    connection: impossible
                }]
            )
            .unwrap_err()
            .to_string()
            .contains("не помещается")
        );
        assert_eq!(project_hash(doc.project()).unwrap(), valid);
        assert_eq!(doc.history_depth(), history);
        doc.transact(
            "Remove support",
            &[Command::Delete {
                id: ObjectId(9),
                policy: DeletePolicy::Cascade,
            }],
        )
        .unwrap();
        assert!(doc.project().connections.is_empty());
        assert_eq!(doc.project().profiles.len(), profiles);
        assert!(
            instance
                .children
                .values()
                .all(|id| !doc.project().contains_id(*id))
        );
        doc.undo().unwrap();
        assert_eq!(project_hash(doc.project()).unwrap(), valid);
    }
}

#[test]
fn bounding_box_contact_without_material_on_face_is_rejected_atomically() {
    let mut p = fixture(true);
    let frame = p.elements.get_mut(&ObjectId(8)).unwrap().shape.frame_mut();
    *frame = Frame {
        origin: Point3([-3075., 0., 1500.]),
        axes: [[0., 1., 0.], [0., 0., 1.], [1., 0., 0.]],
    };
    p.elements.get_mut(&ObjectId(9)).unwrap().shape = ElementShape::Member {
        member_kind: MemberKind::Column,
        profile: ObjectId(2),
        length: LengthMm::new(3000.).unwrap(),
        frame: Frame::default(),
    };
    let before = project_hash(&p).unwrap();
    let mut doc = Document::new(p).unwrap();
    let template = ConnectionTemplate::from_json(include_bytes!(
        "../../../tests/fixtures/connections/clip-angle.json"
    ))
    .unwrap();
    let error = doc
        .transact(
            "Invalid face",
            &[Command::CreateConnection {
                connection: ConnectionInstance {
                    id: ObjectId(10),
                    primary: ObjectId(8),
                    secondary: Some(ObjectId(9)),
                    at_end: true,
                    template,
                    children: Default::default(),
                },
            }],
        )
        .unwrap_err();
    assert!(error.to_string().contains("за плоскую грань"));
    assert_eq!(project_hash(doc.project()).unwrap(), before);
    assert_eq!(doc.history_depth(), (0, 0));
}
