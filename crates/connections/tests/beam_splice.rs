//! Beam splice («стык балок на накладках»): two beams end to end, plates on
//! both sides of the web and outside both flanges, bolted to each beam.
use open_bim_commands::{Command, Document};
use open_bim_connections::{ConnectionInstance, ConnectionTemplate};
use open_bim_model::{
    DetailKind, Element, ElementShape, Frame, MemberKind, ObjectId, Point3, Project, TemplateKind,
    beam_splice_roles,
};
use std::f64::consts::PI;

const LEFT: ObjectId = ObjectId(8);
const RIGHT: ObjectId = ObjectId(9);

fn beam(id: ObjectId, profile: u64, start: [f64; 3], end: [f64; 3]) -> Element {
    let (frame, length) = Frame::along(Point3(start), Point3(end)).unwrap();
    Element {
        id,
        name: "Балка".into(),
        material: ObjectId(1),
        properties: Default::default(),
        shape: ElementShape::Member {
            member_kind: MemberKind::Beam,
            profile: ObjectId(profile),
            length,
            frame,
            position: Default::default(),
        },
    }
}

/// Two I 300×150×8×12 beams along +Y at Z = 3000 with a 10 mm gap at Y = 3000.
fn fixture(gap: f64, offset: f64) -> Project {
    let mut p = Project::default();
    p.elements
        .insert(LEFT, beam(LEFT, 2, [0., 0., 3000.], [0., 3000., 3000.]));
    p.elements.insert(
        RIGHT,
        beam(
            RIGHT,
            2,
            [offset, 3000. + gap, 3000.],
            [offset, 6000., 3000.],
        ),
    );
    p.next_id = 10;
    p.validate().unwrap();
    p
}

fn template(rows: u32) -> ConnectionTemplate {
    ConnectionTemplate {
        version: 1,
        name: "Стык балок".into(),
        kind: TemplateKind::BeamSplice { rows, pitch: 70. },
        thickness: 10.,
        edge_distance: 35.,
        bolt_diameter: 20.,
        clearance: 2.,
        bolt_standard: "ГОСТ 7798-70".into(),
        bolt_grade: "5.8".into(),
        weld_size: 6.,
    }
}

fn attempt(
    doc: &mut Document,
    primary: ObjectId,
    secondary: ObjectId,
    at_end: bool,
    t: ConnectionTemplate,
) -> Result<ObjectId, String> {
    let id = ObjectId(doc.project().next_id);
    doc.transact(
        "Стык балок",
        &[Command::CreateConnection {
            connection: ConnectionInstance {
                id,
                primary,
                secondary: Some(secondary),
                at_end,
                template: t,
                children: Default::default(),
            },
        }],
    )
    .map(|_| id)
    .map_err(|e| e.to_string())
}

/// Ø22 hole through `depth` mm.
fn hole(depth: f64) -> f64 {
    PI * 11. * 11. * depth
}

#[test]
fn plates_and_bolts_from_either_beam() {
    // Lap = 2 × 35 + 70 = 140; plates span 2 × 140 + 10 = 290 mm.
    for (primary, secondary, at_end) in [(LEFT, RIGHT, true), (RIGHT, LEFT, false)] {
        let mut doc = Document::new(fixture(10., 0.)).unwrap();
        let before = |doc: &Document, id| doc.project().element_volume_mm3(id).unwrap();
        let (left, right) = (before(&doc, LEFT), before(&doc, RIGHT));
        let id = attempt(&mut doc, primary, secondary, at_end, template(2)).unwrap();
        let p = doc.project();
        let c = &p.connections[&id];
        assert_eq!(c.children.len(), beam_splice_roles(2).len());
        let volume = |role: &str| p.element_volume_mm3(c.children[role]).unwrap();
        // Web plates 140 × 290 × 10 with 8 holes; flange plates 290 × 150.
        for role in ["web-plate-0", "web-plate-1"] {
            let expected = 140. * 290. * 10. - 8. * hole(10.);
            assert!((volume(role) - expected).abs() < 1., "{role}");
        }
        for role in ["flange-plate-top", "flange-plate-bottom"] {
            let expected = 290. * 150. * 10. - 8. * hole(10.);
            assert!((volume(role) - expected).abs() < 1., "{role}");
        }
        // Each beam: 4 web holes through 8 mm, 8 flange holes through 12 mm.
        let drilled = 4. * hole(8.) + 8. * hole(12.);
        for (beam, was) in [(LEFT, left), (RIGHT, right)] {
            let lost = was - p.element_volume_mm3(beam).unwrap();
            assert!((lost - drilled).abs() < 1., "beam {beam}: {lost}");
        }
        let DetailKind::BoltGroup { parts, steps_y, .. } =
            &p.details[&c.children["web-bolts-b"]].kind
        else {
            panic!()
        };
        assert!(parts.contains(&secondary) && !parts.contains(&primary));
        assert_eq!(steps_y, &vec![70.]);
    }
}

#[test]
fn more_web_bolts_keep_the_plates_and_undo_restores() {
    let mut doc = Document::new(fixture(10., 0.)).unwrap();
    let id = attempt(&mut doc, LEFT, RIGHT, true, template(2)).unwrap();
    let plate = doc.project().connections[&id].children["web-plate-0"];
    let mut c = doc.project().connections[&id].clone();
    c.template = template(3);
    doc.transact(
        "Изменить узел",
        &[Command::ReplaceConnection { connection: c }],
    )
    .unwrap();
    let p = doc.project();
    assert_eq!(
        p.connections[&id].children.len(),
        beam_splice_roles(3).len()
    );
    assert_eq!(p.connections[&id].children["web-plate-0"], plate);
    doc.undo().unwrap();
    assert_eq!(
        doc.project().connections[&id].children.len(),
        beam_splice_roles(2).len()
    );
}

#[test]
fn misaligned_far_apart_or_different_beams_are_refused() {
    let mut doc = Document::new(fixture(10., 5.)).unwrap();
    let err = attempt(&mut doc, LEFT, RIGHT, true, template(2)).unwrap_err();
    assert!(err.contains("одной оси"), "{err}");

    let mut doc = Document::new(fixture(60., 0.)).unwrap();
    let err = attempt(&mut doc, LEFT, RIGHT, true, template(2)).unwrap_err();
    assert!(err.contains("до 50 мм"), "{err}");

    let mut p = fixture(10., 0.);
    p.elements.insert(
        RIGHT,
        beam(RIGHT, 4, [0., 3010., 3000.], [0., 6000., 3000.]),
    );
    let mut doc = Document::new(p).unwrap();
    let err = attempt(&mut doc, LEFT, RIGHT, true, template(2)).unwrap_err();
    assert!(err.contains("одно сечение"), "{err}");
    assert!(doc.project().connections.is_empty());
}
