//! Brace gusset («фасонка связи»): a plate behind the angle leg, bolted to
//! the brace and welded to the support face it reaches.
use open_bim_commands::{Command, Document};
use open_bim_connections::{ConnectionInstance, ConnectionTemplate};
use open_bim_model::{
    DetailKind, Element, ElementShape, Frame, MemberKind, ObjectId, Point3, Project, TemplateKind,
    gusset_roles,
};
use std::f64::consts::PI;

const COLUMN: ObjectId = ObjectId(8);
const BRACE: ObjectId = ObjectId(9);

fn member(id: ObjectId, kind: MemberKind, profile: u64, start: [f64; 3], end: [f64; 3]) -> Element {
    let (frame, length) = Frame::along(Point3(start), Point3(end)).unwrap();
    Element {
        id,
        name: "Деталь".into(),
        material: ObjectId(1),
        properties: Default::default(),
        shape: ElementShape::Member {
            member_kind: kind,
            profile: ObjectId(profile),
            length,
            frame,
            position: Default::default(),
        },
    }
}

/// Column I 300×150×8×12 along Z, flange faces at Y = ±150; brace
/// L 100×75×8 in the YZ plane between `start` and `end`.
fn fixture(start: [f64; 3], end: [f64; 3]) -> Project {
    let mut p = Project::default();
    p.elements.insert(
        COLUMN,
        member(COLUMN, MemberKind::Column, 2, [0., 0., 0.], [0., 0., 3600.]),
    );
    p.elements
        .insert(BRACE, member(BRACE, MemberKind::Beam, 3, start, end));
    p.next_id = 10;
    p.validate().unwrap();
    p
}

fn template(bolts: u32) -> ConnectionTemplate {
    ConnectionTemplate {
        version: 1,
        name: "Фасонка связи".into(),
        kind: TemplateKind::Gusset { bolts, pitch: 70. },
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
        "Фасонка",
        &[Command::CreateConnection {
            connection: ConnectionInstance {
                id,
                primary: BRACE,
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

const NEAR: [f64; 3] = [0., 400., 300.];
const FAR: [f64; 3] = [0., 3000., 3000.];

#[test]
fn gusset_reaches_the_column_face_from_either_brace_end() {
    for (start, end, at_end) in [(NEAR, FAR, false), (FAR, NEAR, true)] {
        let mut doc = Document::new(fixture(start, end)).unwrap();
        let before = doc.project().element_volume_mm3(BRACE).unwrap();
        let id = attempt(&mut doc, at_end, template(2)).unwrap();
        let p = doc.project();
        let c = &p.connections[&id];
        assert_eq!(c.children.len(), gusset_roles(2).len());
        // Two Ø22 holes through the 8 mm leg.
        let drilled = before - p.element_volume_mm3(BRACE).unwrap();
        assert!((drilled - 2. * PI * 121. * 8.).abs() < 1., "{drilled}");
        let DetailKind::Weld { parts, path, .. } = &p.details[&c.children["weld"]].kind else {
            panic!()
        };
        assert!(parts.contains(&COLUMN));
        // On the flange face Y = 150, in the gusset mid-plane 42.5 mm off the axis.
        for q in path {
            assert!((q.0[1] - 150.).abs() < 1e-6, "{path:?}");
            assert!((q.0[0].abs() - 42.5).abs() < 1e-6, "{path:?}");
        }
        assert!(path[0].distance(path[1]) > 100.);
        let gusset = c.children["gusset"];
        assert!(p.element_volume_mm3(gusset).unwrap() > 0.);
    }
}

#[test]
fn more_bolts_keep_the_gusset_and_undo_restores() {
    let mut doc = Document::new(fixture(NEAR, FAR)).unwrap();
    let id = attempt(&mut doc, false, template(2)).unwrap();
    let gusset = doc.project().connections[&id].children["gusset"];
    let mut c = doc.project().connections[&id].clone();
    c.template = template(3);
    doc.transact(
        "Изменить узел",
        &[Command::ReplaceConnection { connection: c }],
    )
    .unwrap();
    let p = doc.project();
    assert_eq!(p.connections[&id].children.len(), gusset_roles(3).len());
    assert_eq!(p.connections[&id].children["gusset"], gusset);
    doc.undo().unwrap();
    assert_eq!(
        doc.project().connections[&id].children.len(),
        gusset_roles(2).len()
    );
}

#[test]
fn a_brace_into_the_column_or_too_far_away_is_refused() {
    let mut doc = Document::new(fixture([0., 160., 300.], FAR)).unwrap();
    let err = attempt(&mut doc, false, template(2)).unwrap_err();
    assert!(err.contains("заходит"), "{err}");

    let mut doc = Document::new(fixture([0., 1500., 300.], FAR)).unwrap();
    let err = attempt(&mut doc, false, template(2)).unwrap_err();
    assert!(err.contains("дальше 1 м"), "{err}");
    assert!(doc.project().connections.is_empty());
}
