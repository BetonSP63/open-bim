//! «Плагины»: plugins found next to the project, typed inputs and
//! attributes, a background run committed as one undo step.
use open_bim_api::plugins::InputType;
use open_bim_desktop::interaction::tools::with_default_grid;
use open_bim_desktop::tools::{Editor, ui::Action};
use open_bim_desktop::ui::plugins::parse_input;
use open_bim_desktop::ui::{Request, Workbench};
use open_bim_model::{ObjectId, Project};
use serde_json::json;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("open-bim-plugins-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A plugin «Балка» in `<project dir>/plugins`: one point input, one part
/// input and a number attribute; it always answers with one beam.
fn write_plugin(project_dir: &Path) {
    let dir = project_dir.join("plugins").join("beam");
    std::fs::create_dir_all(&dir).unwrap();
    let step = json!({ "steps": [ { "insert_member": {
        "profile": "Гн 150×100×6", "material": "С255",
        "start": [0, 0, 3000], "end": [6000, 0, 3000] } } ] });
    std::fs::write(
        dir.join("run.sh"),
        format!("cat > /dev/null\necho '{step}'\n"),
    )
    .unwrap();
    let manifest = json!({
        "name": "Балка", "kind": "model", "code": "БАЛКА",
        "program": "/bin/sh", "args": ["run.sh"],
        "inputs": [
            { "name": "start", "type": "point", "prompt": "Начало" },
            { "name": "host", "type": "parts", "prompt": "Опоры" }
        ],
        "attributes": [
            { "name": "count", "label": "Количество", "type": "number", "default": 2 },
            { "name": "profile", "label": "Профиль", "type": "profile", "default": "Гн 150×100×6" }
        ],
        "timeout_s": 20
    });
    std::fs::write(dir.join("plugin.json"), manifest.to_string()).unwrap();
}

fn bench(dir: &Path) -> Workbench {
    let project = with_default_grid(Project::default()).unwrap();
    let mut b = Workbench::new(Editor::new(project).unwrap());
    b.tools.path = dir.join("model.obim").to_string_lossy().into_owned();
    b
}

/// Wait for the running job; returns the request it finished with.
fn finish(b: &mut Workbench) -> Option<Request> {
    let started = Instant::now();
    while b.job.is_some() && started.elapsed().as_secs() < 60 {
        if let Some(r) = b.poll_job() {
            return Some(r);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    None
}

#[test]
fn typed_inputs_follow_the_declared_type() {
    assert_eq!(
        parse_input(InputType::Point, "100 200,5 -3").unwrap(),
        json!([100.0, 200.5, -3.0])
    );
    assert_eq!(
        parse_input(InputType::Point, "1,2,3").unwrap(),
        json!([1.0, 2.0, 3.0])
    );
    assert!(parse_input(InputType::Point, "1 2").is_err());
    assert_eq!(
        parse_input(InputType::Points, "0 0 0; 10 0 0").unwrap(),
        json!([[0.0, 0.0, 0.0], [10.0, 0.0, 0.0]])
    );
    assert_eq!(parse_input(InputType::Part, "12").unwrap(), json!(12));
    assert!(parse_input(InputType::Part, "12 13").is_err());
    assert_eq!(
        parse_input(InputType::Parts, "12, 13 14").unwrap(),
        json!([12, 13, 14])
    );
    assert!(parse_input(InputType::Parts, "12 x").is_err());
}

#[test]
fn window_checks_inputs_and_attributes() {
    let dir = temp_dir("check");
    write_plugin(&dir);
    let mut b = bench(&dir);
    b.plugins.rescan(&b.tools.path.clone());
    assert_eq!(b.plugins.plugins.len(), 1, "{:?}", b.plugins.problems);
    assert_eq!(b.plugins.attributes["count"], "2", "defaults shown");
    assert_eq!(b.plugins.attributes["profile"], "Гн 150×100×6");

    let error = b.plugins.request().err().unwrap();
    assert!(error.contains("Начало"), "{error}");
    b.plugins.inputs.insert("start".into(), "0 0 0".into());
    b.plugins.take_selection(&[ObjectId(7), ObjectId(9)].into());
    assert_eq!(b.plugins.inputs["host"], "7 9", "parts from the selection");
    b.plugins.attributes.insert("count".into(), "три".into());
    let error = b.plugins.request().err().unwrap();
    assert!(error.contains("Количество"), "{error}");
    b.plugins.attributes.insert("count".into(), "3".into());
    let run = b.plugins.request().unwrap();
    assert_eq!(run.attributes["count"], json!(3.0));
    assert_eq!(run.inputs.len(), 2);
}

#[test]
fn run_commits_one_undo_step_and_selects_the_result() {
    let dir = temp_dir("run");
    write_plugin(&dir);
    // The plugin's part input is replaced by a point-only copy, so the
    // run needs no parts in the model.
    let manifest = dir.join("plugins/beam/plugin.json");
    let mut m: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest).unwrap()).unwrap();
    m["inputs"].as_array_mut().unwrap().pop();
    std::fs::write(&manifest, m.to_string()).unwrap();

    let mut b = bench(&dir);
    b.plugins.rescan(&b.tools.path.clone());
    b.plugins.inputs.insert("start".into(), "0 0 0".into());
    let before = b.editor.document.project().elements.len();
    let depth = b.editor.document.history_depth();
    let run = b.plugins.request().unwrap();
    b.start_plugin(run);
    assert!(b.busy());
    let Some(Request::Action(Action::Commit(label, commands))) = finish(&mut b) else {
        panic!("no commit: {:?}", b.plugins.message);
    };
    assert_eq!(label, "Плагин «Балка»");
    b.editor.commit(&label, &commands).unwrap();
    let project = b.editor.document.project();
    assert_eq!(project.elements.len(), before + 1);
    assert_eq!(b.editor.document.history_depth().0, depth.0 + 1);
    let created: BTreeSet<_> = b.editor.selected.clone();
    assert_eq!(created.len(), 1);
    assert!(project.contains_id(*created.iter().next().unwrap()));
}

#[test]
fn result_is_dropped_when_the_model_changed_meanwhile() {
    let dir = temp_dir("changed");
    write_plugin(&dir);
    let manifest = dir.join("plugins/beam/plugin.json");
    let mut m: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest).unwrap()).unwrap();
    m["inputs"] = json!([]);
    std::fs::write(&manifest, m.to_string()).unwrap();

    let mut b = bench(&dir);
    b.plugins.rescan(&b.tools.path.clone());
    let run = b.plugins.request().unwrap();
    b.start_plugin(run);
    b.editor
        .commit_data(
            "Переименовать",
            &[open_bim_commands::Command::RenameProject {
                name: "Другая".into(),
            }],
        )
        .unwrap();
    assert!(finish(&mut b).is_none());
    assert!(b.job.is_none());
    let message = b.plugins.message.clone().unwrap();
    assert!(message.contains("изменилась"), "{message}");
}

/// A placed plugin on a column: its program counts its runs and names the
/// plate by the count, so a run again is visible.
fn write_counting_plugin(project_dir: &Path) {
    let dir = project_dir.join("plugins").join("cap");
    std::fs::create_dir_all(&dir).unwrap();
    let script = r#"cat > /dev/null
n=$(cat count 2>/dev/null || echo 0); n=$((n+1)); echo $n > count
echo "{\"steps\":[{\"insert_plate\":{\"name\":\"Пластина $n\",\"material\":\"С255\",\"thickness\":20,\"outline\":[[0,0,3000],[300,0,3000],[300,300,3000],[0,300,3000]]}}]}"
"#;
    std::fs::write(dir.join("run.sh"), script).unwrap();
    let manifest = json!({
        "name": "Оголовок", "kind": "model", "program": "/bin/sh", "args": ["run.sh"],
        "inputs": [ { "name": "column", "type": "part", "prompt": "Колонна" } ],
        "attributes": [ { "name": "size", "label": "Размер", "type": "number", "default": 300 } ],
        "timeout_s": 20
    });
    std::fs::write(dir.join("plugin.json"), manifest.to_string()).unwrap();
}

#[test]
fn moving_an_input_runs_the_placed_plugin_again_in_the_same_undo_step() {
    use open_bim_commands::Command;
    use open_bim_model::{Frame, MemberKind, Point3};
    let dir = temp_dir("follow");
    write_counting_plugin(&dir);
    let mut session = open_bim_api::Session::new(Project::default()).unwrap();
    let mut column = open_bim_api::Member {
        id: None,
        kind: MemberKind::Column,
        name: "Колонна".into(),
        profile: "Гн 150×100×6".into(),
        material: "С255".into(),
        start: Point3([0.; 3]),
        end: Point3([0., 0., 3000.]),
        position: Default::default(),
        properties: Default::default(),
    };
    let column = session.insert_member(&mut column).unwrap();
    session.commit("Колонна").unwrap();
    let mut b = Workbench::new(Editor::new(session.committed().clone()).unwrap());
    b.tools.path = dir.join("model.obim").to_string_lossy().into_owned();

    b.plugins.rescan(&b.tools.path.clone());
    b.plugins
        .inputs
        .insert("column".into(), column.0.to_string());
    let run = b.plugins.request().unwrap();
    b.start_plugin(run);
    let Some(Request::Action(Action::Commit(label, commands))) = finish(&mut b) else {
        panic!("no commit: {:?}", b.plugins.message);
    };
    b.editor.commit(&label, &commands).unwrap();
    let instance = b
        .editor
        .document
        .project()
        .plugins
        .values()
        .next()
        .unwrap()
        .clone();
    let plate = *instance.outputs.iter().next().unwrap();
    let name = |b: &Workbench| b.editor.document.project().elements[&plate].name.clone();
    assert_eq!(name(&b), "Пластина 1");
    b.follow_plugins();
    assert!(b.job.is_none(), "placing the plugin changed no input");

    // The placed plugin is picked by its plate: its attributes are shown.
    b.plugins
        .follow_selection(b.editor.document.project(), &[plate].into());
    assert_eq!(b.plugins.placed, Some(instance.id));
    assert_eq!(b.plugins.placed_attributes["size"], "300");

    // Move the column: the plugin runs again, the plate keeps its ID.
    let depth = b.editor.document.history_depth();
    b.editor
        .commit(
            "Сдвиг",
            &[Command::Transform {
                ids: vec![column],
                rotation: Frame {
                    origin: Point3([1000., 0., 0.]),
                    ..Frame::default()
                },
                pivot: Point3([0.; 3]),
            }],
        )
        .unwrap();
    b.follow_plugins();
    assert!(b.job.is_some(), "a run again started");
    let Some(Request::Action(Action::Amend(commands))) = finish(&mut b) else {
        panic!("no amend: {:?}", b.plugins.message);
    };
    b.editor.amend(&commands).unwrap();
    assert_eq!(name(&b), "Пластина 2");
    assert_eq!(b.editor.document.history_depth().0, depth.0 + 1, "one step");
    b.follow_plugins();
    assert!(b.job.is_none(), "the amendment does not start another run");

    // One undo returns the column and the first plate.
    b.editor.history(false).unwrap();
    assert_eq!(name(&b), "Пластина 1");
    b.follow_plugins();
    assert!(b.job.is_none(), "undo does not run plugins");

    // «Изменить» with new attributes runs it as a new step.
    b.plugins
        .follow_selection(b.editor.document.project(), &[plate].into());
    b.plugins
        .placed_attributes
        .insert("size".into(), "400".into());
    let run = b.plugins.edit_request(b.editor.document.project()).unwrap();
    assert_eq!(run.placed, Some(instance.id));
    b.start_plugin(run);
    let Some(Request::Action(Action::Commit(label, commands))) = finish(&mut b) else {
        panic!("no commit: {:?}", b.plugins.message);
    };
    assert!(label.starts_with("Изменить плагин"), "{label}");
    b.editor.commit(&label, &commands).unwrap();
    let stored = &b.editor.document.project().plugins[&instance.id];
    assert_eq!(
        stored.attributes["size"],
        open_bim_model::PluginValue::Number(400.)
    );
    assert_eq!(stored.outputs, [plate].into());
}
