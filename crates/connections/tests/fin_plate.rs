//! Fin plate («шарнир на пластине»): plate welded to the support face,
//! bolted to the beam web, with a gap between the beam end and the support.
use open_bim_commands::{Command, Document};
use open_bim_connections::{ConnectionInstance, ConnectionTemplate};
use open_bim_model::{
    DetailKind, Element, ElementShape, Frame, LengthMm, MemberKind, ObjectId, Point3, Project,
    TemplateKind,
};

const GAP: f64 = 15.;

/// Beam I 300×150×8×12 along Z from 0 to 3000; a 30 mm support plate
/// across its axis, `GAP` beyond the chosen end.
fn fixture(at_end: bool) -> Project {
    let mut p = Project::default();
    p.elements.insert(
        ObjectId(8),
        Element {
            id: ObjectId(8),
            name: "Балка".into(),
            material: ObjectId(1),
            properties: Default::default(),
            shape: ElementShape::Member {
                member_kind: MemberKind::Beam,
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
            name: "Опора".into(),
            material: ObjectId(1),
            properties: Default::default(),
            shape: ElementShape::Plate {
                contour: vec![[0., 0.], [600., 0.], [600., 600.], [0., 600.]],
                thickness: LengthMm::new(30.).unwrap(),
                frame: Frame {
                    origin: Point3([-300., -300., if at_end { 3000. + GAP } else { -30. - GAP }]),
                    ..Frame::default()
                },
            },
        },
    );
    p.next_id = 10;
    p.validate().unwrap();
    p
}

fn template(rows: u32) -> ConnectionTemplate {
    ConnectionTemplate {
        version: 1,
        name: "Шарнир на пластине".into(),
        kind: TemplateKind::FinPlate {
            height: 230.,
            lap: 90.,
            gap: GAP,
            pitch: 70.,
            rows,
        },
        thickness: 10.,
        edge_distance: 35.,
        bolt_diameter: 20.,
        clearance: 2.,
        bolt_standard: "ГОСТ 7798-70".into(),
        bolt_grade: "5.8".into(),
        weld_size: 6.,
    }
}

fn create(doc: &mut Document, at_end: bool, rows: u32) -> ObjectId {
    let id = ObjectId(doc.project().next_id);
    doc.transact(
        "Шарнир на пластине",
        &[Command::CreateConnection {
            connection: ConnectionInstance {
                id,
                primary: ObjectId(8),
                secondary: Some(ObjectId(9)),
                at_end,
                template: template(rows),
                children: Default::default(),
            },
        }],
    )
    .unwrap();
    id
}

#[test]
fn plate_bolts_and_weld_at_either_end() {
    for at_end in [true, false] {
        let mut doc = Document::new(fixture(at_end)).unwrap();
        let web_before = doc.project().element_volume_mm3(ObjectId(8)).unwrap();
        let id = create(&mut doc, at_end, 2);
        let p = doc.project();
        let c = &p.connections[&id];
        assert_eq!(c.children.len(), 3 + 2 * 2);
        let plate = c.children["plate"];
        // Plate: (lap + gap) × height × t, minus two holes Ø22.
        let hole = std::f64::consts::PI * 11. * 11. * 10.;
        let volume = p.element_volume_mm3(plate).unwrap();
        assert!(
            (volume - (105. * 230. * 10. - 2. * hole)).abs() < 1.,
            "plate {volume}"
        );
        // The beam web loses two Ø22 holes through 8 mm.
        let web_after = p.element_volume_mm3(ObjectId(8)).unwrap();
        let drilled = web_before - web_after;
        let expected = 2. * std::f64::consts::PI * 11. * 11. * 8.;
        assert!((drilled - expected).abs() < 1., "web {drilled}");
        let DetailKind::BoltGroup { parts, steps_y, .. } = &p.details[&c.children["bolts"]].kind
        else {
            panic!()
        };
        assert_eq!(parts.len(), 2);
        assert_eq!(steps_y, &vec![70.]);
        let DetailKind::Weld { parts, path, .. } = &p.details[&c.children["weld"]].kind else {
            panic!()
        };
        assert!(parts.contains(&ObjectId(9)) && parts.contains(&plate));
        // The weld runs on the support face.
        let face = if at_end { 3000. + GAP } else { -GAP };
        assert!(
            path.iter().all(|q| (q.0[2] - face).abs() < 1e-6),
            "{path:?}"
        );
    }
}

#[test]
fn more_bolts_regenerate_with_the_same_plate_and_undo_restores() {
    let mut doc = Document::new(fixture(true)).unwrap();
    let id = create(&mut doc, true, 2);
    let plate = doc.project().connections[&id].children["plate"];
    let mut c = doc.project().connections[&id].clone();
    c.template = template(3);
    doc.transact(
        "Изменить узел",
        &[Command::ReplaceConnection { connection: c }],
    )
    .unwrap();
    let p = doc.project();
    let c = &p.connections[&id];
    assert_eq!(c.children.len(), 3 + 2 * 3);
    assert_eq!(c.children["plate"], plate, "the plate keeps its ID");
    doc.undo().unwrap();
    assert_eq!(doc.project().connections[&id].children.len(), 3 + 2 * 2);
}

#[test]
fn support_away_from_the_gap_or_bad_parameters_are_refused() {
    let mut p = fixture(true);
    if let ElementShape::Plate { frame, .. } = &mut p.elements.get_mut(&ObjectId(9)).unwrap().shape
    {
        frame.origin.0[2] += 40.;
    }
    let mut doc = Document::new(p).unwrap();
    let id = ObjectId(doc.project().next_id);
    let attempt = |doc: &mut Document, t: ConnectionTemplate| {
        doc.transact(
            "Шарнир на пластине",
            &[Command::CreateConnection {
                connection: ConnectionInstance {
                    id,
                    primary: ObjectId(8),
                    secondary: Some(ObjectId(9)),
                    at_end: true,
                    template: t,
                    children: Default::default(),
                },
            }],
        )
    };
    let err = attempt(&mut doc, template(2)).unwrap_err().to_string();
    assert!(err.contains("не доходит"), "{err}");
    // Four bolts at 70 mm do not fit in a 230 mm plate with 35 mm edges.
    assert!(attempt(&mut doc, template(4)).is_err());
    assert!(doc.project().connections.is_empty());
}
