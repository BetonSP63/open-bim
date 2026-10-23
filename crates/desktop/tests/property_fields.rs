//! Property pane fields kept as properties: numbering series of the part and
//! of its assembly, deformation and the IFC class, edited for several parts
//! and applied in one undoable step.
use open_bim_desktop::tools::Editor;
use open_bim_desktop::ui::property_fields::{CAMBER, FieldBuffer, common};
use open_bim_desktop::ui::property_pane::{Shared, VARIES};
use open_bim_model::{ObjectId, PropertyValue};

fn editor() -> Editor {
    let project = open_bim_persistence::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/steel_release/source.obim"
    ))
    .unwrap();
    Editor::new(project).unwrap()
}

/// The fixture with its first two parts in one assembly.
fn editor_with_assembly() -> Editor {
    let mut project = open_bim_persistence::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/steel_release/source.obim"
    ))
    .unwrap();
    let members: std::collections::BTreeSet<ObjectId> =
        project.elements.keys().take(2).copied().collect();
    let id = ObjectId(project.next_id);
    project.next_id += 1;
    project.assemblies.insert(
        id,
        open_bim_model::Assembly {
            id,
            name: "Сборка".into(),
            main_part: *members.first().unwrap(),
            members,
            children: Default::default(),
            properties: Default::default(),
        },
    );
    Editor::new(project).unwrap()
}

#[test]
fn edited_fields_go_to_parts_and_assemblies_in_one_step() {
    let mut editor = editor_with_assembly();
    let project = editor.document.project();
    // Two parts that belong to assemblies.
    let parts: Vec<ObjectId> = project
        .assemblies
        .values()
        .flat_map(|a| a.members.iter().copied())
        .take(2)
        .collect();
    assert_eq!(parts.len(), 2);
    let shown = common(project, &parts);
    assert_eq!(shown[CAMBER], Shared::Same(String::new()));
    let mut buffer = FieldBuffer::default();
    buffer.fill(&shown);
    // Nothing edited: nothing to do.
    assert!(buffer.commands(project, &shown, &parts).unwrap().is_empty());
    buffer.values.insert(CAMBER, "12,5".into());
    buffer
        .values
        .insert(open_bim_ifc::IFC_ENTITY_PROPERTY, "IfcMember".into());
    buffer
        .values
        .insert("assembly numbering_prefix", "КС".into());
    let commands = buffer.commands(project, &shown, &parts).unwrap();
    editor.commit("Изменить свойства", &commands).unwrap();
    let project = editor.document.project();
    for id in &parts {
        let e = &project.elements[id];
        assert_eq!(e.properties.get(CAMBER), Some(&PropertyValue::Number(12.5)));
        assert_eq!(
            e.properties.get(open_bim_ifc::IFC_ENTITY_PROPERTY),
            Some(&PropertyValue::Text("IfcMember".into()))
        );
        let assembly = project
            .assemblies
            .values()
            .find(|a| a.members.contains(id))
            .unwrap();
        assert_eq!(
            assembly.properties.get(open_bim_numbering::SERIES_PREFIX),
            Some(&PropertyValue::Text("КС".into()))
        );
    }
    // Different values show «— разные —»; emptying a field removes it.
    let one = [parts[0]];
    let mut buffer = FieldBuffer::default();
    let shown = common(project, &one);
    buffer.fill(&shown);
    buffer.values.insert(CAMBER, String::new());
    let commands = buffer.commands(project, &shown, &one).unwrap();
    editor.commit("Изменить свойства", &commands).unwrap();
    let project = editor.document.project();
    assert_eq!(common(project, &parts)[CAMBER], Shared::Varies);
    let mut buffer = FieldBuffer::default();
    buffer.fill(&common(project, &parts));
    assert_eq!(buffer.values[CAMBER], VARIES);
    // One undo restores everything of the first change.
    editor.document.undo().unwrap();
    editor.document.undo().unwrap();
    assert!(
        editor.document.project().elements[&parts[0]]
            .properties
            .get(CAMBER)
            .is_none()
    );
}

#[test]
fn a_bad_number_is_refused_with_the_field_name() {
    let editor = editor();
    let project = editor.document.project();
    let parts: Vec<ObjectId> = project.elements.keys().take(1).copied().collect();
    let shown = common(project, &parts);
    let mut buffer = FieldBuffer::default();
    buffer.fill(&shown);
    buffer.values.insert(CAMBER, "много".into());
    let e = buffer.commands(project, &shown, &parts).unwrap_err();
    assert!(e.contains("Выгиб"), "{e}");
}
