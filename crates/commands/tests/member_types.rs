//! Changing a member type rewrites every instance except overridden fields,
//! in one undo step.
use open_bim_commands::{Command, Document};
use open_bim_model::{
    Element, ElementShape, Frame, LengthMm, MemberKind, MemberType, ObjectId, PartPosition,
    TypeField,
};
use std::collections::BTreeSet;

fn beam(id: ObjectId, profile: ObjectId) -> Element {
    Element {
        id,
        name: format!("Б{id}"),
        material: ObjectId(1),
        properties: Default::default(),
        shape: ElementShape::Member {
            member_kind: MemberKind::Beam,
            profile,
            length: LengthMm::new(6000.).unwrap(),
            frame: Frame::along(
                open_bim_model::Point3([0., 0., 0.]),
                open_bim_model::Point3([6000., 0., 0.]),
            )
            .unwrap()
            .0,
            position: PartPosition::default(),
        },
    }
}

#[test]
fn editing_a_type_updates_instances_and_keeps_overrides() {
    let mut doc = Document::new(open_bim_model::Project::default()).unwrap();
    let (a, b) = (ObjectId(8), ObjectId(9));
    let type_id = ObjectId(40);
    doc.transact(
        "Балки",
        &[
            Command::Create {
                element: beam(a, ObjectId(2)),
            },
            Command::Create {
                element: beam(b, ObjectId(2)),
            },
        ],
    )
    .unwrap();
    let ty = MemberType {
        id: type_id,
        name: "Б-1".into(),
        kind: MemberKind::Beam,
        profile: ObjectId(2),
        material: ObjectId(1),
        class: "2".into(),
        prefix: "Б".into(),
        position: PartPosition::default(),
    };
    doc.transact(
        "Тип",
        &[
            Command::DefineMemberType {
                member_type: ty.clone(),
            },
            Command::SetMemberUse {
                part: a,
                type_id: Some(type_id),
                overrides: BTreeSet::new(),
                apply: true,
            },
            Command::SetMemberUse {
                part: b,
                type_id: Some(type_id),
                overrides: BTreeSet::from([TypeField::Class]),
                apply: true,
            },
        ],
    )
    .unwrap();
    // The second beam keeps its own class (empty) while the type says «2».
    let mut edited = ty.clone();
    edited.class = "4".into();
    edited.material = ObjectId(1);
    doc.transact(
        "Правка типа",
        &[Command::DefineMemberType {
            member_type: edited,
        }],
    )
    .unwrap();
    let project = doc.project();
    assert_eq!(
        project.elements[&a].properties.get("class"),
        Some(&open_bim_model::PropertyValue::Text("4".into()))
    );
    assert!(
        !project.elements[&b].properties.contains_key("class"),
        "переопределённый класс не затёрт"
    );
    doc.undo().unwrap();
    assert_eq!(
        doc.project().elements[&a].properties.get("class"),
        Some(&open_bim_model::PropertyValue::Text("2".into()))
    );
}
