use open_bim_commands::{Command, Document};
use open_bim_connections::{ConnectionInstance, ConnectionTemplate};
use open_bim_model::{
    DetailKind, Element, ElementShape, Frame, LengthMm, MemberKind, ObjectId, Point3, Profile,
    ProfileShape, Project,
};

fn fixture(end_plate: bool) -> Project {
    let mut p = Project::default();
    p.profiles.insert(
        ObjectId(30),
        Profile {
            id: ObjectId(30),
            name: "I600 test support".into(),
            shape: ProfileShape::ISection {
                height: 600.,
                width: 320.,
                web: 12.,
                flange: 20.,
            },
            source: "Independent sharp-corner dimensions".into(),
            standard: None,
            area_mm2: None,
        },
    );
    p.elements.insert(
        ObjectId(8),
        Element {
            id: ObjectId(8),
            name: "I300 beam".into(),
            material: ObjectId(1),
            properties: Default::default(),
            shape: ElementShape::Member {
                member_kind: MemberKind::Beam,
                profile: ObjectId(2),
                length: LengthMm::new(3000.).unwrap(),
                frame: Frame {
                    origin: Point3([0., if end_plate { -3320. } else { -3300. }, 1500.]),
                    axes: [[-1., 0., 0.], [0., 0., 1.], [0., 1., 0.]],
                },
                position: Default::default(),
            },
        },
    );
    p.elements.insert(
        ObjectId(9),
        Element {
            id: ObjectId(9),
            name: "I600 column".into(),
            material: ObjectId(1),
            properties: Default::default(),
            shape: ElementShape::Member {
                member_kind: MemberKind::Column,
                profile: ObjectId(30),
                length: LengthMm::new(3000.).unwrap(),
                frame: Frame::default(),
                position: Default::default(),
            },
        },
    );
    p.next_id = 31;
    p.validate().unwrap();
    p
}

#[test]
fn beam_connections_drill_only_the_contacting_column_flange() {
    for (end_plate, bytes, count) in [
        (
            true,
            include_bytes!("../../../tests/fixtures/connections/end-plate.json").as_slice(),
            4,
        ),
        (
            false,
            include_bytes!("../../../tests/fixtures/connections/clip-angle.json").as_slice(),
            2,
        ),
    ] {
        let mut doc = Document::new(fixture(end_plate)).unwrap();
        let template = ConnectionTemplate::from_json(bytes).unwrap();
        doc.transact(
            "Beam to column",
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
        .unwrap();
        let support = ObjectId(9);
        let holes: Vec<_> = doc
            .project()
            .details
            .values()
            .filter_map(|d| {
                if let DetailKind::Hole { part, depth, .. } = d.kind {
                    (part == support).then_some(depth)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(holes.len(), count);
        assert!(holes.iter().all(|d| *d == 42.));
        // I600 area = 2*320*20 + 12*(600-40); only the near20mm flange is cut.
        let expected = 19520. * 3000. - count as f64 * std::f64::consts::PI * 121. * 20.;
        let body = doc.project().element_body(support).unwrap();
        assert!((body.volume_mm3().unwrap() / expected - 1.).abs() < 1e-6);
        let (p, t) = body.tessellation_f64().unwrap();
        let volume: f64 = t
            .iter()
            .map(|f| {
                let [a, b, c] = f.map(|i| p[i as usize]);
                (a[0] * (b[1] * c[2] - b[2] * c[1])
                    + a[1] * (b[2] * c[0] - b[0] * c[2])
                    + a[2] * (b[0] * c[1] - b[1] * c[0]))
                    / 6.
            })
            .sum();
        assert!((volume / expected - 1.).abs() < 1e-6);
    }
}
