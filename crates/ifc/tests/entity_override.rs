//! «Экспорт в IFC» of the property pane: the class, predefined type and
//! user type chosen on a part go to the IFC file and survive re-import.
use open_bim_ifc::{
    IFC_ENTITY_PROPERTY, IFC_OBJECT_TYPE_PROPERTY, IFC_PREDEFINED_PROPERTY, export, import,
};
use open_bim_model::{ElementShape, MemberKind, PropertyValue};

fn project() -> open_bim_model::Project {
    open_bim_persistence::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/steel_release/source.obim"
    ))
    .unwrap()
}

fn text(t: &str) -> Option<PropertyValue> {
    Some(PropertyValue::Text(t.into()))
}

#[test]
fn chosen_class_and_types_are_exported_and_member_survives_import() {
    let mut p = project();
    let beam = *p
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
    let column = *p
        .elements
        .iter()
        .find(|(_, e)| {
            matches!(
                e.shape,
                ElementShape::Member {
                    member_kind: MemberKind::Column,
                    ..
                }
            )
        })
        .unwrap()
        .0;
    let set = |p: &mut open_bim_model::Project, id, key: &str, v: Option<PropertyValue>| {
        p.elements
            .get_mut(&id)
            .unwrap()
            .properties
            .insert(key.into(), v.unwrap());
    };
    set(&mut p, beam, IFC_ENTITY_PROPERTY, text("IfcMember"));
    set(&mut p, beam, IFC_PREDEFINED_PROPERTY, text("BRACE"));
    set(&mut p, column, IFC_PREDEFINED_PROPERTY, text("USERDEFINED"));
    set(
        &mut p,
        column,
        IFC_OBJECT_TYPE_PROPERTY,
        text("Стойка фахверка"),
    );
    let (step, _) = export(&p).unwrap();
    assert!(step.contains(".BRACE.)"), "member brace exported");
    assert!(step.contains("IFCMEMBER('"));
    assert!(
        step.contains(",.USERDEFINED.)"),
        "user-defined column exported"
    );
    // The choice is not repeated in the property set.
    assert!(!step.contains("'ifc_entity'"));
    let (back, _) = import(&step).unwrap();
    let members = back
        .elements
        .values()
        .filter(|e| e.properties.get(IFC_ENTITY_PROPERTY) == text("IfcMember").as_ref())
        .count();
    assert_eq!(members, 1);
}

#[test]
fn predefined_type_not_allowed_for_the_class_falls_back_to_notdefined() {
    let mut p = project();
    let id = *p.elements.keys().next().unwrap();
    let e = p.elements.get_mut(&id).unwrap();
    e.properties.insert(
        IFC_ENTITY_PROPERTY.into(),
        PropertyValue::Text("IfcColumn".into()),
    );
    e.properties.insert(
        IFC_PREDEFINED_PROPERTY.into(),
        PropertyValue::Text("SHEET".into()),
    );
    let (step, _) = export(&p).unwrap();
    assert!(step.contains(".NOTDEFINED.)"));
    assert_eq!(
        open_bim_ifc::predefined_types("ifccolumn").unwrap()[0],
        "COLUMN"
    );
}
