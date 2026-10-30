use open_bim_commands::*;
use open_bim_model::*;
use std::collections::BTreeMap;

fn sample(name: &str) -> Family {
    sample_families()
        .into_iter()
        .find(|f| f.name == name)
        .unwrap()
}

fn seat_at(id: u64, type_name: &str) -> FamilyInstance {
    FamilyInstance {
        id: ObjectId(id),
        family: "Столик опорный".into(),
        type_name: type_name.into(),
        frame: Frame {
            origin: Point3([1000., 0., 3000.]),
            ..Frame::default()
        },
        length: 0.,
        values: BTreeMap::new(),
        children: BTreeMap::new(),
    }
}

fn placed() -> (Document, ObjectId) {
    let mut document = Document::new(Project::default()).unwrap();
    let id = ObjectId(document.project().next_id + 100);
    document
        .transact(
            "Столик",
            &[
                Command::SetFamily {
                    family: Box::new(sample("Столик опорный")),
                },
                Command::CreateFamilyInstance {
                    instance: seat_at(id.0, "С-2"),
                },
            ],
        )
        .unwrap();
    (document, id)
}

#[test]
fn placing_a_family_builds_parts_and_an_assembly_in_one_undo_step() {
    let (mut document, id) = placed();
    let p = document.project();
    let instance = &p.family_instances[&id];
    // Seat plate, two ribs, their welds, two holes and the assembly.
    assert_eq!(instance.children.len(), 8);
    let seat = &p.elements[&instance.children["body-0-0"]];
    let ElementShape::Plate {
        thickness, frame, ..
    } = &seat.shape
    else {
        panic!()
    };
    assert_eq!(thickness.value(), 16.);
    assert_eq!(frame.origin, Point3([1000., 0., 3000. - 16.]));
    assert_eq!(
        seat.properties[open_bim_model::family::FAMILY_PROPERTY],
        PropertyValue::Text("Столик опорный".into())
    );
    assert!(p.materials[&seat.material].name.starts_with("С255"));
    let assembly = &p.assemblies[&instance.children["assembly"]];
    assert_eq!(assembly.members.len(), 3);
    assert_eq!(assembly.name, "Столик опорный С-2");
    document.undo().unwrap();
    assert!(document.project().elements.is_empty());
    assert!(document.project().families.is_empty());
}

#[test]
fn changing_the_type_or_an_instance_value_rebuilds_with_the_same_ids() {
    let (mut document, id) = placed();
    let before = document.project().family_instances[&id].clone();
    let mut changed = before.clone();
    changed.type_name = "С-3".into();
    changed.values.insert("n".into(), "1".into());
    document
        .transact(
            "Типоразмер",
            &[Command::ReplaceFamilyInstance { instance: changed }],
        )
        .unwrap();
    let p = document.project();
    let after = &p.family_instances[&id];
    // One rib left: the second one is gone with its part and weld.
    assert_eq!(after.children.len(), 6);
    assert!(!after.children.contains_key("detail-0-1"));
    assert_eq!(after.children["body-0-0"], before.children["body-0-0"]);
    assert!(!p.elements.contains_key(&before.children["body-1-1"]));
    let ElementShape::Plate { thickness, .. } = &p.elements[&after.children["body-0-0"]].shape
    else {
        panic!()
    };
    assert_eq!(thickness.value(), 20.);
}

#[test]
fn a_bad_value_is_refused_and_parts_cannot_be_edited_directly() {
    let (mut document, id) = placed();
    let mut bad = document.project().family_instances[&id].clone();
    bad.values.insert("n".into(), "9".into());
    let error = document
        .transact("n", &[Command::ReplaceFamilyInstance { instance: bad }])
        .unwrap_err();
    assert!(error.to_string().contains("больше"), "{error}");
    let part = document.project().family_instances[&id].children["body-0-0"];
    let mut element = document.project().elements[&part].clone();
    element.shape.frame_mut().origin = Point3([0., 0., 0.]);
    let error = document
        .transact("Сдвиг", &[Command::ReplaceElement { element }])
        .unwrap_err();
    assert!(error.to_string().contains("семейство"), "{error}");
}

#[test]
fn editing_the_family_rebuilds_its_objects_and_deleting_a_part_removes_the_object() {
    let (mut document, id) = placed();
    let mut family = sample("Столик опорный");
    family
        .types
        .iter_mut()
        .find(|t| t.name == "С-2")
        .unwrap()
        .values
        .insert("t".into(), "25".into());
    document
        .transact(
            "Семейство",
            &[Command::SetFamily {
                family: Box::new(family),
            }],
        )
        .unwrap();
    let p = document.project();
    let part = p.family_instances[&id].children["body-0-0"];
    let ElementShape::Plate { thickness, .. } = &p.elements[&part].shape else {
        panic!()
    };
    assert_eq!(thickness.value(), 25.);
    let error = document
        .transact(
            "Убрать",
            &[Command::RemoveFamily {
                name: "Столик опорный".into(),
            }],
        )
        .unwrap_err();
    assert!(error.to_string().contains("размещено"), "{error}");
    document
        .transact(
            "Удалить",
            &[Command::Delete {
                id: part,
                policy: DeletePolicy::Cascade,
            }],
        )
        .unwrap();
    let p = document.project();
    assert!(p.family_instances.is_empty());
    assert!(p.elements.is_empty() && p.assemblies.is_empty());
}

#[test]
fn a_ladder_between_two_points_follows_model_parameters() {
    let mut document = Document::new(Project::default()).unwrap();
    let id = ObjectId(document.project().next_id + 100);
    let (frame, length) =
        FamilyInstance::between(Point3([0., 0., 0.]), Point3([0., 0., 3000.])).unwrap();
    let parametric = Parametric {
        variables: vec![open_bim_model::parametric::Variable {
            name: "Шаг".into(),
            formula: "300".into(),
        }],
        bindings: Vec::new(),
    };
    document
        .transact(
            "Стремянка",
            &[
                Command::SetParametric {
                    parametric: parametric.clone(),
                },
                Command::SetFamily {
                    family: Box::new(sample("Стремянка")),
                },
                Command::CreateFamilyInstance {
                    instance: FamilyInstance {
                        id,
                        family: "Стремянка".into(),
                        type_name: "Ш600".into(),
                        frame,
                        length,
                        values: BTreeMap::from([("шаг".into(), "Шаг".into())]),
                        children: BTreeMap::new(),
                    },
                },
            ],
        )
        .unwrap();
    let rungs = |d: &Document| {
        d.project().family_instances[&id]
            .children
            .keys()
            .filter(|k| k.starts_with("body-1-"))
            .count()
    };
    assert_eq!(rungs(&document), 10);
    let mut wider = parametric;
    wider.variables[0].formula = "450".into();
    document
        .transact("Шаг", &[Command::SetParametric { parametric: wider }])
        .unwrap();
    assert_eq!(rungs(&document), 7);
    let p = document.project();
    let top = p.family_instances[&id].children["body-0-0"];
    let (start, end, _) = p.member_reference_line(&p.elements[&top].shape).unwrap();
    assert!((start.distance(end) - 3000.).abs() < 1e-6);
    // The project saves and loads with its families.
    let json = p.to_json().unwrap();
    assert_eq!(&Project::from_json(&json).unwrap(), p);
}
