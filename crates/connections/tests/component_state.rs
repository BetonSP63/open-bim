//! A component keeps working with the model the way the parts change: it is
//! rebuilt, or it stays in the model «не создан» when the parts no longer
//! allow it, and is built again when they do. «Общие»: blocking and class.
use open_bim_commands::{Command, Document};
use open_bim_connections::{ConnectionInstance, ConnectionTemplate};
use open_bim_model::{
    ComponentGeneral, Element, ElementShape, Frame, MemberKind, ObjectId, Point3, Project,
    PropertyValue, TemplateKind,
};

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

fn splice(doc: &mut Document, general: ComponentGeneral) -> ObjectId {
    let id = ObjectId(doc.project().next_id);
    doc.transact(
        "Стык балок",
        &[Command::CreateConnection {
            connection: ConnectionInstance {
                id,
                primary: LEFT,
                secondary: Some(RIGHT),
                at_end: true,
                template: template(2),
                children: Default::default(),
                general,
            },
        }],
    )
    .unwrap();
    id
}

fn move_right(doc: &mut Document, gap: f64) -> Result<(), String> {
    let element = beam(RIGHT, 2, [0., 3000. + gap, 3000.], [0., 6000., 3000.]);
    doc.transact("Переместить", &[Command::ReplaceElement { element }])
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[test]
fn moved_part_leaves_the_component_not_created_and_back() {
    let mut doc = Document::new(fixture(10., 0.)).unwrap();
    let id = splice(&mut doc, Default::default());
    let made: Vec<_> = doc.project().connections[&id]
        .children
        .values()
        .copied()
        .collect();
    assert!(!made.is_empty());
    // 500 mm apart: a splice cannot be built, but the move is not refused.
    move_right(&mut doc, 500.).unwrap();
    let p = doc.project();
    let c = &p.connections[&id];
    assert!(c.children.is_empty(), "no objects of a failed component");
    for child in &made {
        assert!(!p.elements.contains_key(child) && !p.details.contains_key(child));
    }
    // Saved and opened again as it is.
    let reopened = Project::from_json(&p.to_json().unwrap()).unwrap();
    assert!(reopened.connections[&id].children.is_empty());
    // Moved back: built again.
    move_right(&mut doc, 10.).unwrap();
    assert_eq!(
        doc.project().connections[&id].children.len(),
        made.len(),
        "rebuilt with all its objects"
    );
    // Undo goes back through the failed state.
    doc.undo().unwrap();
    assert!(doc.project().connections[&id].children.is_empty());
}

#[test]
fn new_or_edited_component_that_cannot_be_built_is_refused() {
    let mut doc = Document::new(fixture(500., 0.)).unwrap();
    let id = ObjectId(doc.project().next_id);
    let refused = doc.transact(
        "Стык балок",
        &[Command::CreateConnection {
            connection: ConnectionInstance {
                id,
                primary: LEFT,
                secondary: Some(RIGHT),
                at_end: true,
                template: template(2),
                children: Default::default(),
                general: Default::default(),
            },
        }],
    );
    assert!(refused.is_err());
    assert!(doc.project().connections.is_empty());
}

#[test]
fn locked_component_refuses_changes_until_unlocked() {
    let mut doc = Document::new(fixture(10., 0.)).unwrap();
    let locked = ComponentGeneral {
        locked: true,
        ..Default::default()
    };
    let id = splice(&mut doc, locked);
    let mut c = doc.project().connections[&id].clone();
    c.template.thickness = 12.;
    let err = doc
        .transact(
            "Изменить",
            &[Command::ReplaceConnection {
                connection: c.clone(),
            }],
        )
        .unwrap_err()
        .to_string();
    assert!(err.contains("заблокирован"), "{err}");
    // Unlocking and changing in one step is allowed.
    c.general.locked = false;
    doc.transact("Изменить", &[Command::ReplaceConnection { connection: c }])
        .unwrap();
    assert_eq!(doc.project().connections[&id].template.thickness, 12.);
    // The parts still adapt while the component is locked.
    let mut c = doc.project().connections[&id].clone();
    c.general.locked = true;
    doc.transact(
        "Заблокировать",
        &[Command::ReplaceConnection { connection: c }],
    )
    .unwrap();
    move_right(&mut doc, 20.).unwrap();
    assert!(!doc.project().connections[&id].children.is_empty());
}

#[test]
fn class_of_the_component_goes_to_its_parts() {
    let mut doc = Document::new(fixture(10., 0.)).unwrap();
    let general = ComponentGeneral {
        class: "5".into(),
        code: "СБ-1".into(),
        ..Default::default()
    };
    let id = splice(&mut doc, general);
    let p = doc.project();
    let c = &p.connections[&id];
    let parts: Vec<_> = c
        .children
        .values()
        .filter_map(|id| p.elements.get(id))
        .collect();
    assert!(!parts.is_empty());
    for e in parts {
        assert_eq!(
            e.properties.get("class"),
            Some(&PropertyValue::Text("5".into()))
        );
    }
    // The connected beams keep their own class.
    assert!(!p.elements[&LEFT].properties.contains_key("class"));
    let json = p.to_json().unwrap();
    assert!(json.contains("СБ-1"));
}
