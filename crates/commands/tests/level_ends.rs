//! Raising a level moves every column end bound to it, and the nodes on
//! those columns, in the same undo step.
use open_bim_commands::{Command, Document};
use open_bim_model::{
    ConnectionInstance, ConnectionTemplate, Element, ElementShape, Frame, LengthMm, Level,
    LevelBinding, MemberEnd, MemberKind, ObjectId, Point3,
};

fn ends(doc: &Document, id: ObjectId) -> (f64, f64) {
    let shape = &doc.project().elements[&id].shape;
    let (bottom, top, _) = open_bim_model::level_ends::column_ends(doc.project(), shape).unwrap();
    (bottom.0[2], top.0[2])
}

fn column(id: ObjectId, x: f64) -> Element {
    Element {
        id,
        name: format!("К{id}"),
        material: ObjectId(1),
        properties: Default::default(),
        shape: ElementShape::Member {
            member_kind: MemberKind::Column,
            profile: ObjectId(2),
            length: LengthMm::new(3600.).unwrap(),
            frame: Frame {
                origin: Point3([x, 0., 0.]),
                ..Frame::default()
            },
            position: Default::default(),
        },
    }
}

#[test]
fn raising_a_level_moves_columns_and_nodes_in_one_undo() {
    let mut doc = Document::new(open_bim_model::Project::default()).unwrap();
    let (low, high) = (ObjectId(20), ObjectId(21));
    let (a, b) = (ObjectId(8), ObjectId(9));
    doc.transact(
        "Уровни и колонны",
        &[
            Command::CreateLevel {
                level: Level {
                    id: low,
                    name: "0".into(),
                    elevation_mm: 0.,
                },
            },
            Command::CreateLevel {
                level: Level {
                    id: high,
                    name: "+3,600".into(),
                    elevation_mm: 3600.,
                },
            },
            Command::Create {
                element: column(a, 0.),
            },
            Command::Create {
                element: column(b, 6000.),
            },
            Command::SetLevelEnd {
                part: a,
                end: MemberEnd::Bottom,
                binding: Some(LevelBinding {
                    level: low,
                    offset_mm: -50.,
                }),
            },
            Command::SetLevelEnd {
                part: a,
                end: MemberEnd::Top,
                binding: Some(LevelBinding {
                    level: high,
                    offset_mm: 0.,
                }),
            },
            Command::SetLevelEnd {
                part: b,
                end: MemberEnd::Bottom,
                binding: Some(LevelBinding {
                    level: low,
                    offset_mm: -50.,
                }),
            },
            Command::SetLevelEnd {
                part: b,
                end: MemberEnd::Top,
                binding: Some(LevelBinding {
                    level: high,
                    offset_mm: 0.,
                }),
            },
        ],
    )
    .unwrap();
    assert_eq!(ends(&doc, a), (-50., 3600.));
    assert_eq!(ends(&doc, b), (-50., 3600.));

    let template = ConnectionTemplate::from_json(include_bytes!(
        "../../../tests/fixtures/connections/base-plate.json"
    ))
    .unwrap();
    doc.transact(
        "Опорная плита",
        &[Command::CreateConnection {
            connection: ConnectionInstance {
                id: ObjectId(30),
                primary: a,
                secondary: None,
                at_end: false,
                template,
                children: Default::default(),
                general: Default::default(),
            },
        }],
    )
    .unwrap();
    let plate = doc.project().connections[&ObjectId(30)].children["plate"];
    let plate_z = doc.project().elements[&plate].shape.frame().origin.0[2];
    assert!(
        (plate_z - (-50. - 20.)).abs() < 1e-6,
        "плита под низом колонны, было {plate_z}"
    );

    doc.transact(
        "Поднять уровень",
        &[Command::ReplaceLevel {
            level: Level {
                id: low,
                name: "+0,300".into(),
                elevation_mm: 300.,
            },
        }],
    )
    .unwrap();
    assert_eq!(doc.history_depth().0, 3, "один шаг на подъём уровня");
    assert_eq!(ends(&doc, a).0, 250., "низ: 300 + смещение −50");
    assert_eq!(ends(&doc, b).0, 250.);
    assert_eq!(ends(&doc, a).1, 3600., "верх привязан к другому уровню");
    let plate_after = doc.project().elements[&plate].shape.frame().origin.0[2];
    assert!(
        (plate_after - plate_z - 300.).abs() < 1e-6,
        "узел сдвинулся на 300 мм: {plate_z} → {plate_after}"
    );
    // The binding itself did not change.
    let binding = doc.project().level_ends.get(a, MemberEnd::Bottom).unwrap();
    assert_eq!(binding.offset_mm, -50.);

    doc.undo().unwrap();
    assert_eq!(ends(&doc, a), (-50., 3600.));
    assert_eq!(ends(&doc, b), (-50., 3600.));
    assert!(
        (doc.project().elements[&plate].shape.frame().origin.0[2] - plate_z).abs() < 1e-6,
        "один undo возвращает и колонны, и узел"
    );
    assert_eq!(doc.project().levels[&low].elevation_mm, 0.);
}

#[test]
fn dragging_a_bound_end_updates_the_offset() {
    let mut doc = Document::new(open_bim_model::Project::default()).unwrap();
    let low = ObjectId(20);
    let id = ObjectId(8);
    doc.transact(
        "Колонна",
        &[
            Command::CreateLevel {
                level: Level {
                    id: low,
                    name: "0".into(),
                    elevation_mm: 0.,
                },
            },
            Command::Create {
                element: column(id, 0.),
            },
            Command::SetLevelEnd {
                part: id,
                end: MemberEnd::Bottom,
                binding: Some(LevelBinding {
                    level: low,
                    offset_mm: 0.,
                }),
            },
        ],
    )
    .unwrap();
    let mut element = doc.project().elements[&id].clone();
    element.shape = doc
        .project()
        .replace_member(
            &element.shape,
            Some((Point3([0., 0., 120.]), Point3([0., 0., 3600.]))),
            None,
            None,
        )
        .unwrap();
    doc.transact("Ручка", &[Command::ReplaceElement { element }])
        .unwrap();
    let binding = doc.project().level_ends.get(id, MemberEnd::Bottom).unwrap();
    assert!(
        (binding.offset_mm - 120.).abs() < 1e-6,
        "привязка осталась, смещение стало {}",
        binding.offset_mm
    );
    doc.transact(
        "Поднять",
        &[Command::ReplaceLevel {
            level: Level {
                id: low,
                name: "+0,300".into(),
                elevation_mm: 300.,
            },
        }],
    )
    .unwrap();
    assert!((ends(&doc, id).0 - 420.).abs() < 1e-6);
}

#[test]
fn legacy_properties_become_level_bindings() {
    use open_bim_model::{PropertyValue, level_ends};
    let mut project = open_bim_model::Project::default();
    project.levels.insert(
        ObjectId(20),
        Level {
            id: ObjectId(20),
            name: "0".into(),
            elevation_mm: 0.,
        },
    );
    let mut element = column(ObjectId(8), 0.);
    element.properties.insert(
        level_ends::LEGACY_BOTTOM_LEVEL.into(),
        PropertyValue::Number(20.),
    );
    element.properties.insert(
        level_ends::LEGACY_BOTTOM_OFFSET.into(),
        PropertyValue::Number(-80.),
    );
    project.elements.insert(element.id, element);
    project.next_id = 21;
    let doc = Document::new(project).unwrap();
    let binding = doc
        .project()
        .level_ends
        .get(ObjectId(8), MemberEnd::Bottom)
        .unwrap();
    assert_eq!(binding.level, ObjectId(20));
    assert_eq!(binding.offset_mm, -80.);
    assert!(
        !doc.project().elements[&ObjectId(8)]
            .properties
            .contains_key(level_ends::LEGACY_BOTTOM_LEVEL)
    );
}
