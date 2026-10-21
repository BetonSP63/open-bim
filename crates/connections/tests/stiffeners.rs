//! Column stiffeners: four plates between the column flanges at the levels
//! of the beam flanges, each welded to the column.
use open_bim_commands::{Command, Document};
use open_bim_connections::{ConnectionInstance, ConnectionTemplate};
use open_bim_model::{
    DetailKind, Element, ElementShape, Frame, MemberKind, ObjectId, Point3, Project, TemplateKind,
};

const COLUMN: ObjectId = ObjectId(8);
const BEAM: ObjectId = ObjectId(9);

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

/// Column I 300×150×8×12 along Z; beam I 300 along +Y from the column
/// flange face at the given height.
fn fixture(beam_z: f64) -> Project {
    let mut p = Project::default();
    p.elements.insert(
        COLUMN,
        member(COLUMN, MemberKind::Column, 2, [0., 0., 0.], [0., 0., 3600.]),
    );
    p.elements.insert(
        BEAM,
        member(
            BEAM,
            MemberKind::Beam,
            2,
            [0., 150., beam_z],
            [0., 6000., beam_z],
        ),
    );
    p.next_id = 10;
    p.validate().unwrap();
    p
}

fn template(chamfer: f64) -> ConnectionTemplate {
    ConnectionTemplate {
        version: 1,
        name: "Рёбра жёсткости колонны".into(),
        kind: TemplateKind::ColumnStiffeners { chamfer },
        thickness: 12.,
        edge_distance: 35.,
        bolt_diameter: 20.,
        clearance: 2.,
        bolt_standard: "ГОСТ 7798-70".into(),
        bolt_grade: "5.8".into(),
        weld_size: 6.,
    }
}

fn attempt(doc: &mut Document, t: ConnectionTemplate) -> Result<ObjectId, String> {
    let id = ObjectId(doc.project().next_id);
    doc.transact(
        "Рёбра жёсткости",
        &[Command::CreateConnection {
            connection: ConnectionInstance {
                id,
                primary: COLUMN,
                secondary: Some(BEAM),
                at_end: false,
                template: t,
                children: Default::default(),
            },
        }],
    )
    .map(|_| id)
    .map_err(|e| e.to_string())
}

/// Bottom face levels of the four plates.
fn levels(p: &Project, id: ObjectId) -> Vec<f64> {
    (0..4)
        .map(|i| {
            let plate = p.connections[&id].children[&format!("stiffener-{i}")];
            p.elements[&plate].shape.frame().origin.0[2]
        })
        .collect()
}

#[test]
fn four_plates_at_the_beam_flanges_welded_to_the_column() {
    let mut doc = Document::new(fixture(3000.)).unwrap();
    let id = attempt(&mut doc, template(20.)).unwrap();
    let p = doc.project();
    let c = &p.connections[&id];
    assert_eq!(c.children.len(), 8);
    // Flange centres at 3000 ± (150 − 6), plates 12 mm thick centred on them.
    assert_eq!(levels(p, id), vec![3138., 3138., 2850., 2850.]);
    for i in 0..4 {
        let plate = c.children[&format!("stiffener-{i}")];
        // (b − tw) / 2 × (h − 2 tf) minus two 20 mm chamfers, 12 mm thick.
        let volume = p.element_volume_mm3(plate).unwrap();
        let expected = (71. * 276. - 400.) * 12.;
        assert!((volume - expected).abs() < 1., "plate {i}: {volume}");
        // Between the flanges and on one side of the web.
        let ElementShape::Plate { frame, .. } = &p.elements[&plate].shape else {
            panic!()
        };
        assert_eq!(frame.origin.0[1], -138.);
        let x = frame.origin.0[0];
        assert!(x == 4. || x == -75., "{x}");
        let DetailKind::Weld { parts, path, .. } =
            &p.details[&c.children[&format!("weld-{i}")]].kind
        else {
            panic!()
        };
        assert!(parts.contains(&COLUMN) && parts.contains(&plate));
        assert_eq!(path.len(), 6);
    }
}

#[test]
fn moving_the_beam_moves_the_stiffeners_and_undo_restores() {
    let mut doc = Document::new(fixture(3000.)).unwrap();
    let id = attempt(&mut doc, template(0.)).unwrap();
    let plate = doc.project().connections[&id].children["stiffener-0"];
    let mut beam = doc.project().elements[&BEAM].clone();
    if let ElementShape::Member { frame, .. } = &mut beam.shape {
        frame.origin.0[2] += 100.;
    }
    doc.transact("Переместить", &[Command::ReplaceElement { element: beam }])
        .unwrap();
    let p = doc.project();
    assert_eq!(levels(p, id), vec![3238., 3238., 2950., 2950.]);
    assert_eq!(p.connections[&id].children["stiffener-0"], plate, "same ID");
    doc.undo().unwrap();
    assert_eq!(levels(doc.project(), id), vec![3138., 3138., 2850., 2850.]);
}

#[test]
fn a_sloped_beam_a_short_column_or_a_huge_chamfer_are_refused() {
    let mut p = fixture(3000.);
    p.elements.insert(
        BEAM,
        member(
            BEAM,
            MemberKind::Beam,
            2,
            [0., 150., 3000.],
            [0., 6000., 3600.],
        ),
    );
    let mut doc = Document::new(p).unwrap();
    let err = attempt(&mut doc, template(20.)).unwrap_err();
    assert!(err.contains("перпендикулярна"), "{err}");

    let mut doc = Document::new(fixture(3500.)).unwrap();
    let err = attempt(&mut doc, template(20.)).unwrap_err();
    assert!(err.contains("длину колонны"), "{err}");

    let mut doc = Document::new(fixture(3000.)).unwrap();
    assert!(attempt(&mut doc, template(80.)).is_err());
    assert!(doc.project().connections.is_empty());
}
