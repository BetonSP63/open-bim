//! Seat («опорный столик»): a plate under the beam end with a triangular
//! stiffener, welded to the support and bolted to the beam bottom flange.
use open_bim_commands::{Command, Document};
use open_bim_connections::{ConnectionInstance, ConnectionTemplate};
use open_bim_model::{
    DetailKind, Element, ElementShape, Frame, MemberKind, ObjectId, Point3, Project, SEAT_ROLES,
    TemplateKind,
};

const COLUMN: ObjectId = ObjectId(8);
const BEAM: ObjectId = ObjectId(9);

fn member(id: ObjectId, kind: MemberKind, start: [f64; 3], end: [f64; 3]) -> Element {
    let (frame, length) = Frame::along(Point3(start), Point3(end)).unwrap();
    Element {
        id,
        name: "Деталь".into(),
        material: ObjectId(1),
        properties: Default::default(),
        shape: ElementShape::Member {
            member_kind: kind,
            profile: ObjectId(2),
            length,
            frame,
            position: Default::default(),
        },
    }
}

/// Column I 300×150×8×12 along Z, flange faces at Y = ±150; beam I 300 at
/// Z = 3000 between `start` and `end` along Y.
fn fixture(start: f64, end: f64) -> Project {
    let mut p = Project::default();
    p.elements.insert(
        COLUMN,
        member(COLUMN, MemberKind::Column, [0., 0., 0.], [0., 0., 3600.]),
    );
    p.elements.insert(
        BEAM,
        member(BEAM, MemberKind::Beam, [0., start, 3000.], [0., end, 3000.]),
    );
    p.next_id = 10;
    p.validate().unwrap();
    p
}

fn template(length: f64, width: f64) -> ConnectionTemplate {
    ConnectionTemplate {
        version: 1,
        name: "Столик".into(),
        kind: TemplateKind::Seat {
            length,
            width,
            depth: 150.,
        },
        thickness: 16.,
        edge_distance: 35.,
        bolt_diameter: 20.,
        clearance: 2.,
        bolt_standard: "ГОСТ 7798-70".into(),
        bolt_grade: "5.8".into(),
        weld_size: 6.,
    }
}

fn attempt(doc: &mut Document, at_end: bool, t: ConnectionTemplate) -> Result<ObjectId, String> {
    let id = ObjectId(doc.project().next_id);
    doc.transact(
        "Столик",
        &[Command::CreateConnection {
            connection: ConnectionInstance {
                id,
                primary: BEAM,
                secondary: Some(COLUMN),
                at_end,
                template: t,
                children: Default::default(),
                general: Default::default(),
            },
        }],
    )
    .map(|_| id)
    .map_err(|e| e.to_string())
}

#[test]
fn seat_plate_stiffener_bolts_and_welds_under_either_beam_end() {
    for (start, end, at_end) in [(150., 6000., false), (6000., 150., true)] {
        let mut doc = Document::new(fixture(start, end)).unwrap();
        let id = attempt(&mut doc, at_end, template(150., 200.)).unwrap();
        let p = doc.project();
        let c = &p.connections[&id];
        assert_eq!(c.children.len(), SEAT_ROLES.len());
        let volume = |role: &str| p.element_volume_mm3(c.children[role]).unwrap();
        let holes = 2. * std::f64::consts::PI * 11. * 11. * 16.;
        let v = volume("seat-plate");
        assert!((v - (150. * 200. * 16. - holes)).abs() < 0.01 * v, "{v}");
        assert!((volume("stiffener") - 150. * 150. / 2. * 16.).abs() < 1.);
        let DetailKind::BoltGroup { parts, steps_x, .. } = &p.details[&c.children["bolts"]].kind
        else {
            panic!()
        };
        assert!(parts.contains(&BEAM) && parts.contains(&c.children["seat-plate"]));
        // Flange 150 wide, 35 mm to each edge.
        assert_eq!(steps_x, &vec![80.]);
        let DetailKind::Weld { parts, path, .. } = &p.details[&c.children["weld-seat"]].kind else {
            panic!()
        };
        assert!(parts.contains(&COLUMN));
        // Under the beam bottom flange (Z 2850) less the seat, on the column face.
        assert!(
            path.iter()
                .all(|q| (q.0[1] - 150.).abs() < 1e-6 && (q.0[2] - 2834.).abs() < 1e-6),
            "{path:?}"
        );
        // The beam flange is drilled for both bolts.
        let holes = p
            .details
            .values()
            .filter(|d| matches!(d.kind, DetailKind::Hole { part, .. } if part == BEAM))
            .count();
        assert_eq!(holes, 2);
    }
}

#[test]
fn a_longer_seat_keeps_its_plates_and_undo_restores() {
    let mut doc = Document::new(fixture(150., 6000.)).unwrap();
    let id = attempt(&mut doc, false, template(150., 200.)).unwrap();
    let seat = doc.project().connections[&id].children["seat-plate"];
    let mut c = doc.project().connections[&id].clone();
    c.template = template(200., 200.);
    doc.transact(
        "Изменить узел",
        &[Command::ReplaceConnection { connection: c }],
    )
    .unwrap();
    let volume = |doc: &Document| doc.project().element_volume_mm3(seat).unwrap();
    let longer = volume(&doc);
    assert!(
        (longer - 200. * 200. * 16.).abs() < 0.02 * longer,
        "{longer}"
    );
    assert_eq!(doc.project().connections[&id].children["seat-plate"], seat);
    doc.undo().unwrap();
    let shorter = volume(&doc);
    assert!(
        (longer - shorter - 50. * 200. * 16.).abs() < 1.,
        "{longer} {shorter}"
    );
}

#[test]
fn a_short_beam_a_narrow_seat_or_a_gap_are_refused() {
    let mut doc = Document::new(fixture(165., 6000.)).unwrap();
    let err = attempt(&mut doc, false, template(150., 200.)).unwrap_err();
    assert!(err.contains("не доходит"), "{err}");

    let mut doc = Document::new(fixture(150., 6000.)).unwrap();
    let err = attempt(&mut doc, false, template(150., 120.)).unwrap_err();
    assert!(err.contains("уже полки"), "{err}");

    let mut doc = Document::new(fixture(150., 250.)).unwrap();
    let err = attempt(&mut doc, false, template(150., 200.)).unwrap_err();
    assert!(err.contains("половины"), "{err}");
    assert!(doc.project().connections.is_empty());
}
