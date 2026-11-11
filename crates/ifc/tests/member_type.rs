use open_bim_ifc::export;
use open_bim_model::{ElementShape, MemberKind, MemberType, ObjectId, TypeUse};

#[test]
fn a_member_type_is_an_ifc_beam_type() {
    let mut project = open_bim_model::example_steel_frame().unwrap();
    let beam = *project
        .elements
        .iter()
        .find(|(_, e)| {
            matches!(
                e.shape,
                ElementShape::Member {
                    member_kind: MemberKind::Beam,
                    ..
                }
            )
        })
        .unwrap()
        .0;
    let ElementShape::Member {
        profile, position, ..
    } = &project.elements[&beam].shape
    else {
        unreachable!();
    };
    let id = ObjectId(project.next_id);
    project.next_id += 1;
    project.member_types.types.insert(
        id,
        MemberType {
            id,
            name: "Б-30".into(),
            kind: MemberKind::Beam,
            profile: *profile,
            material: project.elements[&beam].material,
            class: String::new(),
            prefix: String::new(),
            position: *position,
        },
    );
    project.member_types.uses.push(TypeUse {
        part: beam,
        type_id: id,
        overrides: Default::default(),
    });
    project.validate().unwrap();
    let (text, _) = export(&project).unwrap();
    assert!(text.contains("IFCBEAMTYPE"), "{text}");
    assert!(text.contains("IFCRELDEFINESBYTYPE"));
    assert!(text.contains("Б-30") || text.contains("\\X2\\"));
}
