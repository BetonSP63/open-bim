//! Property sets of the create dialogs and per-tool properties.
use open_bim_desktop::interaction::tools::default_project;
use open_bim_desktop::tools::presets::{Preset, PresetStore};
use open_bim_desktop::tools::{Editor, ToolKind};
use open_bim_model::{AtDepth, PartPosition};

#[test]
fn each_tool_keeps_its_own_properties() {
    let mut editor = Editor::new(default_project().unwrap()).unwrap();
    editor.begin(ToolKind::Beam);
    editor.draft.position.depth = AtDepth::Behind;
    editor.draft.name = "Балка Б1".into();
    editor.begin(ToolKind::Column);
    editor.draft.name = "Колонна К1".into();
    editor.draft.position = PartPosition::default();
    editor.begin(ToolKind::Beam);
    assert_eq!(editor.draft.name, "Балка Б1");
    assert_eq!(editor.draft.position.depth, AtDepth::Behind);
    editor.begin(ToolKind::Column);
    assert_eq!(editor.draft.name, "Колонна К1");
    assert_eq!(editor.draft.position.depth, AtDepth::Middle);
}

#[test]
fn saved_set_loads_into_another_project_with_catalogue_profile_and_steel() {
    let dir = std::env::temp_dir().join(format!("open-bim-presets-{}", std::process::id()));
    let path = dir.join("properties.json");
    let _ = std::fs::remove_dir_all(&dir);

    // Build the set in a project that already has 30Б1 and С345.
    let mut source = default_project().unwrap();
    let mut editor = Editor::new(source.clone()).unwrap();
    editor.begin(ToolKind::Beam);
    let p = open_bim_model::library::section("30Б1")
        .unwrap()
        .profile(open_bim_model::ObjectId(source.next_id));
    let m = open_bim_model::library::steel(open_bim_model::ObjectId(source.next_id + 1), "С345")
        .unwrap();
    source.profiles.insert(p.id, p.clone());
    source.materials.insert(m.id, m.clone());
    source.next_id += 2;
    editor.draft.profile = p.id;
    editor.draft.material = m.id;
    editor.draft.name = "Балка перекрытия".into();
    editor.draft.position.depth = AtDepth::Behind;
    let preset = Preset::from_draft("Б-30", &editor.draft, &source).unwrap();
    let mut store = PresetStore::default();
    store.put(preset.clone());
    store.save(&path).unwrap();
    assert_eq!(PresetStore::load(&path).unwrap(), store);
    assert!(Preset::from_draft("  ", &editor.draft, &source).is_err());

    // Load it in a fresh project: both records come from the catalogue.
    let mut fresh = Editor::new(default_project().unwrap()).unwrap();
    fresh.begin(ToolKind::Beam);
    let loaded = PresetStore::load(&path).unwrap();
    let set = loaded.for_tool(ToolKind::Beam).next().unwrap();
    let commands = set
        .apply(&mut fresh.draft, fresh.document.project())
        .unwrap();
    assert_eq!(commands.len(), 2, "profile and steel are copied");
    fresh.commit("Свойства", &commands).unwrap();
    let project = fresh.document.project();
    assert_eq!(project.profiles[&fresh.draft.profile].name, "30Б1");
    assert_eq!(
        project.materials[&fresh.draft.material].name,
        "С345 · ГОСТ 27772-2015"
    );
    assert_eq!(fresh.draft.name, "Балка перекрытия");
    assert_eq!(fresh.draft.position.depth, AtDepth::Behind);
    // Loading again reuses the copies.
    let again = set
        .apply(&mut fresh.draft, fresh.document.project())
        .unwrap();
    assert!(again.is_empty());
    // A beam set does not load into the column tool.
    fresh.begin(ToolKind::Column);
    assert!(
        set.apply(&mut fresh.draft, fresh.document.project())
            .is_err()
    );
    let _ = std::fs::remove_dir_all(&dir);
}
