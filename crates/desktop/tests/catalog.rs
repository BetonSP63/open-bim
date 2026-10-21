//! ГОСТ catalogue: picking a section or a steel copies it into the project
//! in the same transaction as the change; the mass follows the catalogue
//! area, and the specification names the standard.
use open_bim_desktop::interaction::tools::default_project;
use open_bim_desktop::tools::{Editor, ui::Action};
use open_bim_desktop::ui::catalog_picker::{Pick, material_by_grade, profile_by_name};
use open_bim_desktop::ui::property_pane::{PropertyPane, common, modify};
use open_bim_desktop::ui::{Request, Workbench, commands::Cmd};
use open_bim_model::{ElementShape, ObjectId};
use open_bim_renderer::DVec3;
use std::collections::BTreeSet;

fn bench() -> Workbench {
    Workbench::new(Editor::new(default_project().unwrap()).unwrap())
}

fn beam(b: &mut Workbench) -> ObjectId {
    b.execute(Cmd::ToolBeam);
    b.pick(DVec3::new(0., 0., 3600.));
    match b.pick(DVec3::new(6000., 0., 3600.)) {
        Some(Request::Action(Action::Commit(label, commands))) => {
            b.editor.commit(&label, &commands).unwrap();
        }
        _ => panic!("no beam"),
    }
    b.execute(Cmd::CancelTool);
    *b.editor.document.project().elements.keys().last().unwrap()
}

#[test]
fn catalogue_profile_and_steel_are_added_with_the_change_and_undone_with_it() {
    let mut b = bench();
    let id = beam(&mut b);
    let profiles_before = b.editor.document.project().profiles.len();
    let selected = BTreeSet::from([id]);
    let c = common(b.editor.document.project(), &selected).unwrap();
    let mut pane = PropertyPane::default();
    pane.sync(&c);
    let project = b.editor.document.project();
    let Some(Pick::New(profile)) = profile_by_name(project, "20Б1") else {
        panic!("20Б1 is not in the new project yet");
    };
    let Some(Pick::New(steel)) = material_by_grade(project, "С345") else {
        panic!("С345 is not in the new project yet");
    };
    pane.profile = Some(profile.id);
    pane.pending_profile = Some(profile);
    pane.material = Some(steel.id);
    pane.pending_material = Some(steel);
    let commands = modify(project, &c.parts, &pane.changes(&c).unwrap()).unwrap();
    b.editor.commit("Изменить свойства", &commands).unwrap();

    let project = b.editor.document.project();
    assert_eq!(project.profiles.len(), profiles_before + 1);
    let e = &project.elements[&id];
    let ElementShape::Member { profile, .. } = &e.shape else {
        panic!()
    };
    assert_eq!(project.profiles[profile].name, "20Б1");
    assert_eq!(
        project.materials[&e.material].name,
        "С345 · ГОСТ 27772-2021"
    );
    // 28.49 cm² × 6 m × 7850 kg/m³ = 134.2 kg (ГОСТ: 22.4 kg/m).
    let mass = project.element_mass_kg(id).unwrap();
    assert!((mass - 134.19).abs() < 0.05, "{mass}");
    let (kind, size) = open_bim_drawings::tables::assortment(project, id);
    assert_eq!(
        (kind.as_str(), size.as_str()),
        ("Двутавры ГОСТ 26020-83", "20Б1")
    );
    assert_eq!(open_bim_drawings::tables::grade(project, id), "С345");

    // Picking it again reuses the project's copy.
    assert!(matches!(
        profile_by_name(project, "20Б1"),
        Some(Pick::Project(_))
    ));
    // Saved and loaded with the standard and the catalogue area.
    let loaded = open_bim_model::Project::from_json(&project.to_json().unwrap()).unwrap();
    assert_eq!(loaded.profiles[profile], project.profiles[profile]);

    // One undo removes the change and the catalogue records.
    b.editor.history(false).unwrap();
    assert_eq!(b.editor.document.project().profiles.len(), profiles_before);
}

#[test]
fn a_new_model_starts_with_gost_steel_and_sections() {
    let mut b = bench();
    let project = b.editor.document.project();
    let names: Vec<&str> = project
        .materials
        .values()
        .map(|m| m.name.as_str())
        .collect();
    assert_eq!(names, ["С255 · ГОСТ 27772-2021"]);
    for name in open_bim_desktop::interaction::tools::STARTER_SECTIONS {
        let p = project.profiles.values().find(|p| p.name == name).unwrap();
        assert!(p.standard.as_deref().unwrap().starts_with("ГОСТ"), "{name}");
    }
    assert!(
        project
            .profiles
            .values()
            .all(|p| !p.name.is_ascii() && !p.source.contains("тестовый"))
    );
    // The beam tool takes the first one: 30Б1, 32,0 kg/m.
    let id = beam(&mut b);
    let mass = b.editor.document.project().element_mass_kg(id).unwrap();
    assert!((mass - 6.0 * 32.0).abs() < 2.0, "{mass}");
}
