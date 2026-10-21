//! Welded haunch («вута»): a triangular web plate and a sloped flange plate
//! under the beam end, welded to the beam and to the support face.
use open_bim_commands::{Command, Document};
use open_bim_connections::{ConnectionInstance, ConnectionTemplate};
use open_bim_model::{
    DetailKind, Element, ElementShape, Frame, HAUNCH_ROLES, MemberKind, ObjectId, Point3, Project,
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

fn template(length: f64, depth: f64) -> ConnectionTemplate {
    ConnectionTemplate {
        version: 1,
        name: "Вута".into(),
        kind: TemplateKind::Haunch { length, depth },
        thickness: 10.,
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
        "Вута",
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
fn web_and_flange_plates_under_either_beam_end() {
    for (start, end, at_end) in [(150., 6000., false), (6000., 150., true)] {
        let mut doc = Document::new(fixture(start, end)).unwrap();
        let id = attempt(&mut doc, at_end, template(600., 300.)).unwrap();
        let p = doc.project();
        let c = &p.connections[&id];
        assert_eq!(c.children.len(), HAUNCH_ROLES.len());
        let volume = |role: &str| p.element_volume_mm3(c.children[role]).unwrap();
        // Triangle 600 × 300, 10 mm; flange along the 670.8 mm slope, 150 × 12.
        assert!((volume("web-plate") - 600. * 300. / 2. * 10.).abs() < 1.);
        let slope = (600f64 * 600. + 300. * 300.).sqrt();
        assert!((volume("flange-plate") - slope * 150. * 12.).abs() < 1.);
        let DetailKind::Weld { parts, path, .. } = &p.details[&c.children["weld-support"]].kind
        else {
            panic!()
        };
        assert!(parts.contains(&COLUMN));
        // From the beam bottom flange down to the tip, on the column face.
        assert!(
            path.iter().all(|q| (q.0[1] - 150.).abs() < 1e-6),
            "{path:?}"
        );
        let mut z: Vec<f64> = path.iter().map(|q| q.0[2]).collect();
        z.sort_by(f64::total_cmp);
        assert!(
            (z[0] - 2550.).abs() < 1e-6 && (z[1] - 2850.).abs() < 1e-6,
            "{z:?}"
        );
    }
}

#[test]
fn a_deeper_haunch_keeps_its_plates_and_undo_restores() {
    let mut doc = Document::new(fixture(150., 6000.)).unwrap();
    let id = attempt(&mut doc, false, template(600., 300.)).unwrap();
    let web = doc.project().connections[&id].children["web-plate"];
    let mut c = doc.project().connections[&id].clone();
    c.template = template(600., 400.);
    doc.transact(
        "Изменить узел",
        &[Command::ReplaceConnection { connection: c }],
    )
    .unwrap();
    let volume = |doc: &Document| doc.project().element_volume_mm3(web).unwrap();
    assert!((volume(&doc) - 600. * 400. / 2. * 10.).abs() < 1.);
    assert_eq!(doc.project().connections[&id].children["web-plate"], web);
    doc.undo().unwrap();
    assert!((volume(&doc) - 600. * 300. / 2. * 10.).abs() < 1.);
}

#[test]
fn a_beam_short_of_the_support_or_a_long_haunch_are_refused() {
    let mut doc = Document::new(fixture(165., 6000.)).unwrap();
    let err = attempt(&mut doc, false, template(600., 300.)).unwrap_err();
    assert!(err.contains("не доходит"), "{err}");

    let mut doc = Document::new(fixture(150., 1000.)).unwrap();
    let err = attempt(&mut doc, false, template(600., 300.)).unwrap_err();
    assert!(err.contains("половины"), "{err}");
    assert!(doc.project().connections.is_empty());
}
