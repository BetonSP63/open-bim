//! Placed plugins depend on their inputs: a change of an input part runs
//! the plugin again in the same undo step, outputs keep their IDs.
use open_bim_api::plugins::{self, InputValue, Manifest, Plugin};
use open_bim_api::{Member, Session};
use open_bim_model::{MemberKind, ObjectId, PluginInput, PluginValue, Point3, Project};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const CAP: &str = env!("CARGO_BIN_EXE_open-bim-plugin-column-cap");

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("open-bim-api-dep-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The sample «Оголовок колонны» pointed at the built program.
fn cap_plugin(root: &Path) -> Plugin {
    let sample = Path::new(env!("CARGO_MANIFEST_DIR")).join("plugins/column-cap/plugin.json");
    let mut manifest: Manifest =
        serde_json::from_str(&std::fs::read_to_string(sample).unwrap()).unwrap();
    manifest.program = CAP.into();
    let dir = root.join("column-cap");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(plugins::MANIFEST),
        serde_json::to_string(&manifest).unwrap(),
    )
    .unwrap();
    plugins::load(&dir).unwrap()
}

/// A session with one committed 3 m column at `x`.
fn column_session(x: f64) -> (Session, ObjectId) {
    let mut s = Session::new(Project::default()).unwrap();
    let mut column = Member {
        id: None,
        kind: MemberKind::Column,
        name: "Колонна".into(),
        profile: "Гн 150×100×6".into(),
        material: "С255".into(),
        start: Point3([x, 0., 0.]),
        end: Point3([x, 0., 3000.]),
        position: Default::default(),
        properties: Default::default(),
    };
    let id = s.insert_member(&mut column).unwrap();
    s.commit("Колонна").unwrap();
    (s, id)
}

fn place_cap(s: &mut Session, plugin: &Plugin, column: ObjectId) -> ObjectId {
    let inputs = [("column".to_string(), InputValue::Part(column))].into();
    let created = s.run_plugin(plugin, &inputs, &BTreeMap::new()).unwrap();
    assert_eq!(created.len(), 1);
    s.commit("Оголовок").unwrap();
    created[0]
}

/// Centre of the plate outline.
fn centre(s: &Session, plate: ObjectId) -> [f64; 3] {
    let outline = s.select_plate(plate).unwrap().outline;
    let n = outline.len() as f64;
    std::array::from_fn(|k| outline.iter().map(|p| p.0[k]).sum::<f64>() / n)
}

#[test]
fn placed_plugin_is_stored_with_inputs_attributes_and_outputs() {
    let root = temp("stored");
    let plugin = cap_plugin(&root);
    let (mut s, column) = column_session(0.);
    let plate = place_cap(&mut s, &plugin, column);
    let instance = s.plugin_instances().next().unwrap().clone();
    assert_eq!(instance.plugin, "Оголовок колонны");
    assert_eq!(instance.inputs["column"], PluginInput::Part(column));
    assert_eq!(instance.attributes["size"], PluginValue::Number(300.));
    assert_eq!(
        instance.attributes["material"],
        PluginValue::Text("С255".into())
    );
    assert_eq!(instance.outputs, [plate].into());
    let c = centre(&s, plate);
    assert!((c[2] - 3000.).abs() < 1e-6, "{c:?}");

    // Saved and read back with the model.
    let json = s.committed().to_json().unwrap();
    let back = Project::from_json(&json).unwrap();
    assert_eq!(back.plugins[&instance.id], instance);
}

#[test]
fn moving_the_input_runs_the_plugin_again_in_one_undo_step() {
    let root = temp("move");
    let plugin = cap_plugin(&root);
    let (mut s, column) = column_session(0.);
    s.set_plugins(vec![plugin.clone()]);
    let plate = place_cap(&mut s, &plugin, column);

    s.move_by(&[column], [1000., 0., 500.]).unwrap();
    s.commit("Сдвиг колонны").unwrap();
    assert!(s.plugin_problems().is_empty(), "{:?}", s.plugin_problems());
    let c = centre(&s, plate);
    assert!(
        (c[0] - 1000.).abs() < 1e-6 && (c[2] - 3500.).abs() < 1e-6,
        "{c:?}"
    );
    assert_eq!(
        s.plugin_instances().next().unwrap().outputs,
        [plate].into(),
        "the plate keeps its ID"
    );

    // One undo returns the column and the plate together.
    s.undo().unwrap();
    let c = centre(&s, plate);
    assert!(c[0].abs() < 1e-6 && (c[2] - 3000.).abs() < 1e-6, "{c:?}");
}

#[test]
fn new_attributes_run_the_plugin_again() {
    let root = temp("attrs");
    let plugin = cap_plugin(&root);
    let (mut s, column) = column_session(0.);
    let plate = place_cap(&mut s, &plugin, column);
    let instance = s.plugin_instances().next().unwrap().id;
    let attrs = [("size".to_string(), serde_json::json!(500))].into();
    let created = s.rerun_plugin(&plugin, instance, Some(&attrs)).unwrap();
    assert_eq!(created, vec![plate]);
    s.commit("Размер оголовка").unwrap();
    let outline = s.select_plate(plate).unwrap().outline;
    let width = outline.iter().map(|p| p.0[0]).fold(f64::MIN, f64::max)
        - outline.iter().map(|p| p.0[0]).fold(f64::MAX, f64::min);
    assert!((width - 500.).abs() < 1e-6, "{width}");
    let stored = &s.plugin_instances().next().unwrap().attributes;
    assert_eq!(stored["size"], PluginValue::Number(500.));
    assert_eq!(stored["thickness"], PluginValue::Number(20.), "others kept");
}

#[test]
fn deleting_the_input_removes_the_plugin_and_its_outputs() {
    let root = temp("delete");
    let plugin = cap_plugin(&root);
    let (mut s, column) = column_session(0.);
    let plate = place_cap(&mut s, &plugin, column);
    // The plate belongs to the plugin: alone it is refused.
    assert!(s.delete(plate, false).is_err());
    s.delete(column, true).unwrap();
    s.commit("Удалить").unwrap();
    let p = s.committed();
    assert!(p.plugins.is_empty());
    assert!(!p.elements.contains_key(&plate));
}

#[test]
fn missing_plugin_keeps_old_outputs_and_reports() {
    let root = temp("missing");
    let plugin = cap_plugin(&root);
    let (mut s, column) = column_session(0.);
    let plate = place_cap(&mut s, &plugin, column);
    // No plugins known to the session.
    s.move_by(&[column], [1000., 0., 0.]).unwrap();
    s.commit("Сдвиг").unwrap();
    assert_eq!(s.plugin_problems().len(), 1);
    assert!(s.plugin_problems()[0].contains("не найден"));
    assert!(centre(&s, plate)[0].abs() < 1e-6, "old plate stays");
}

#[test]
fn batch_runs_dependent_plugins_found_in_the_macro_folders() {
    let root = temp("batch");
    cap_plugin(&root.join("plugins"));
    let (mut s, column) = column_session(0.);
    s.save(Some(&root.join("frame.obim"))).unwrap();
    let script: open_bim_api::batch::Macro = serde_json::from_value(serde_json::json!({
        "open": "frame.obim",
        "steps": [
            { "run_plugin": { "plugin": "Оголовок колонны", "inputs": { "column": column.0 } } },
            { "commit": "Оголовок" },
            { "select": { "kinds": ["column"] } },
            { "move": { "vector": [0, 2000, 0] } },
            { "commit": "Сдвиг" }
        ]
    }))
    .unwrap();
    let (s, _) = open_bim_api::batch::run(&script, &root).unwrap();
    let instance = s.plugin_instances().next().unwrap();
    let plate = *instance.outputs.iter().next().unwrap();
    assert!((centre(&s, plate)[1] - 2000.).abs() < 1e-6);
}
