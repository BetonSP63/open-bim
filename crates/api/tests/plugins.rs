use open_bim_api::plugins::{self, InputValue, Manifest, Plugin};
use open_bim_api::{ApiError, Session, batch};
use open_bim_model::{ElementShape, MemberKind, Point3, Project};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const COLUMNS: &str = env!("CARGO_BIN_EXE_open-bim-plugin-columns");

/// The POSIX shell that runs the test scripts: `/bin/sh`, or on Windows the
/// `sh.exe` of Git for Windows found on `PATH`.
fn shell() -> String {
    if !cfg!(windows) {
        return "/bin/sh".into();
    }
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .map(|d| d.join("sh.exe"))
        .find(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files\Git\bin\sh.exe"))
        .to_string_lossy()
        .into_owned()
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("open-bim-api-pl-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The sample manifest of the repository, pointed at the built program.
fn columns_plugin(root: &Path) -> Plugin {
    let sample = Path::new(env!("CARGO_MANIFEST_DIR")).join("plugins/columns/plugin.json");
    let mut manifest: Manifest =
        serde_json::from_str(&std::fs::read_to_string(sample).unwrap()).unwrap();
    manifest.program = COLUMNS.into();
    let dir = root.join("columns");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(plugins::MANIFEST),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();
    plugins::load(&dir).unwrap()
}

/// A plugin whose program is a shell script.
fn script_plugin(root: &Path, name: &str, body: &str, timeout_s: u64) -> Plugin {
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("run.sh"), format!("cat > /dev/null\n{body}\n")).unwrap();
    let manifest = serde_json::json!({
        "name": name, "kind": "model", "program": shell(), "args": ["run.sh"],
        "timeout_s": timeout_s
    });
    std::fs::write(dir.join(plugins::MANIFEST), manifest.to_string()).unwrap();
    plugins::load(&dir).unwrap()
}

fn points(a: [f64; 3], b: [f64; 3]) -> BTreeMap<String, InputValue> {
    [
        ("start".to_string(), InputValue::Point(Point3(a))),
        ("end".to_string(), InputValue::Point(Point3(b))),
    ]
    .into()
}

#[test]
fn sample_manifest_is_valid_and_found() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("plugins");
    let (found, problems) = plugins::discover(&[root]);
    assert!(problems.is_empty(), "{problems:?}");
    assert_eq!(found.len(), 2);
    let m = &found
        .iter()
        .find(|p| p.manifest.name == "Ряд колонн")
        .unwrap()
        .manifest;
    assert_eq!(m.name, "Ряд колонн");
    assert_eq!(m.inputs.len(), 2);
    assert_eq!(m.code.as_deref(), Some("КОЛ-РЯД"));
}

#[test]
fn plugin_run_stages_its_objects_together() {
    let root = temp("run");
    let plugin = columns_plugin(&root);
    let mut s = Session::new(Project::default()).unwrap();
    let ids = s
        .run_plugin(
            &plugin,
            &points([0.; 3], [18000., 0., 0.]),
            &BTreeMap::new(),
        )
        .unwrap();
    assert_eq!(ids.len(), 4, "default count");
    s.commit("Ряд колонн").unwrap();
    let last = s.select_member(ids[3]).unwrap();
    assert_eq!(last.kind, MemberKind::Column);
    assert!((last.start.0[0] - 18000.).abs() < 1e-6);
    assert!((last.end.0[2] - 3600.).abs() < 1e-6);

    // Attributes override defaults.
    let attrs = [("count".to_string(), serde_json::json!(2))].into();
    let two = s
        .run_plugin(
            &plugin,
            &points([0., 6000., 0.], [6000., 6000., 0.]),
            &attrs,
        )
        .unwrap();
    assert_eq!(two.len(), 2);

    // A bad answer stages nothing: an unknown profile fails the 3rd step.
    s.rollback();
    let bad = [("profile".to_string(), serde_json::json!("Нет такого"))].into();
    let error = s
        .run_plugin(&plugin, &points([0.; 3], [9000., 0., 0.]), &bad)
        .unwrap_err();
    assert!(error.to_string().contains("Ряд колонн"), "{error}");
    assert_eq!(s.staged(), 0);
    assert_eq!(s.project(), s.committed());

    // Inputs and attributes are checked before the program starts.
    let mut missing = points([0.; 3], [1.; 3]);
    missing.remove("end");
    assert!(matches!(
        s.run_plugin(&plugin, &missing, &BTreeMap::new()),
        Err(ApiError::Invalid(_))
    ));
    let wrong = [("count".to_string(), serde_json::json!("четыре"))].into();
    assert!(
        s.run_plugin(&plugin, &points([0.; 3], [1.; 3]), &wrong)
            .is_err()
    );
}

#[test]
fn manifest_limits() {
    let base: Manifest = serde_json::from_value(serde_json::json!({
        "name": "Тест", "kind": "connection", "program": "x"
    }))
    .unwrap();
    base.validate().unwrap();
    let mut many = base.clone();
    many.inputs = (0..11)
        .map(|i| {
            serde_json::from_value(serde_json::json!({ "name": format!("p{i}"), "type": "point" }))
                .unwrap()
        })
        .collect();
    assert!(many.validate().is_err(), "at most 10 inputs");
    many.inputs.truncate(10);
    many.validate().unwrap();
    let mut code = base.clone();
    code.code = Some("А".repeat(23));
    assert!(code.validate().is_err(), "code at most 22 characters");
    code.code = Some("А".repeat(22));
    code.validate().unwrap();
    let mut twice = base.clone();
    twice.inputs = serde_json::from_value(serde_json::json!([
        { "name": "a", "type": "part" }, { "name": "a", "type": "parts" }
    ]))
    .unwrap();
    assert!(twice.validate().is_err());
}

#[test]
fn errors_forbidden_steps_and_time_limit() {
    let root = temp("errors");
    let mut s = Session::new(Project::default()).unwrap();
    let none = BTreeMap::new();
    let no = BTreeMap::new();
    let said = script_plugin(&root, "Ошибка", r#"echo '{"error": "нет балок"}'"#, 10);
    assert!(
        s.run_plugin(&said, &none, &no)
            .unwrap_err()
            .to_string()
            .contains("нет балок")
    );
    let save = script_plugin(
        &root,
        "Сохранение",
        r#"echo '{"steps": [{"save": null}]}'"#,
        10,
    );
    assert!(
        s.run_plugin(&save, &none, &no)
            .unwrap_err()
            .to_string()
            .contains("недоступен")
    );
    let crash = script_plugin(&root, "Сбой", "echo 'сломалось' >&2; exit 3", 10);
    assert!(
        s.run_plugin(&crash, &none, &no)
            .unwrap_err()
            .to_string()
            .contains("сломалось")
    );
    let slow = script_plugin(&root, "Долгий", "sleep 5", 1);
    let started = std::time::Instant::now();
    assert!(
        s.run_plugin(&slow, &none, &no)
            .unwrap_err()
            .to_string()
            .contains("не уложился")
    );
    assert!(started.elapsed().as_secs() < 4);
    assert_eq!(s.staged(), 0);
}

#[test]
fn batch_runs_a_plugin_by_name() {
    let root = temp("batch");
    columns_plugin(&root.join("plugins"));
    let text = r#"{ "steps": [
        { "run_plugin": { "plugin": "Ряд колонн",
            "inputs": { "start": [0, 0, 0], "end": [12000, 0, 0] },
            "attributes": { "count": 3 } } },
        { "set_property": { "key": "ряд", "value": { "type": "text", "value": "А" } } },
        { "commit": "Колонны" }
    ] }"#;
    let (session, report) = batch::run(&batch::parse(text).unwrap(), &root).unwrap();
    assert_eq!(report.steps[0].ids.len(), 3);
    assert_eq!(report.steps[1].ids, report.steps[0].ids);
    let columns = session
        .committed()
        .elements
        .values()
        .filter(|e| {
            matches!(
                e.shape,
                ElementShape::Member {
                    member_kind: MemberKind::Column,
                    ..
                }
            )
        })
        .count();
    assert_eq!(columns, 3);

    let unknown = r#"{ "steps": [ { "run_plugin": { "plugin": "Нет такого" } } ] }"#;
    assert!(batch::run(&batch::parse(unknown).unwrap(), &root).is_err());
}

#[test]
fn local_frame_of_the_first_input() {
    let defs: Vec<plugins::InputDef> = serde_json::from_value(serde_json::json!([
        { "name": "start", "type": "point" },
        { "name": "end", "type": "point" }
    ]))
    .unwrap();
    let p = Project::default();
    let f = plugins::input_frame(&p, &defs, &points([1000., 0., 0.], [1000., 5000., 0.]));
    assert_eq!(f.origin, Point3([1000., 0., 0.]));
    let close = |a: [f64; 3], b: [f64; 3]| (0..3).all(|i| (a[i] - b[i]).abs() < 1e-9);
    assert!(close(f.axes[0], [0., 1., 0.]), "x towards the second point");
    assert!(close(f.axes[2], [0., 0., 1.]), "z up");
    f.validate().unwrap();
    let none = plugins::input_frame(&p, &defs, &BTreeMap::new());
    assert_eq!(none, Default::default());

    // A vertical column: origin at its start, x up, z along world x.
    let mut s = Session::new(Project::default()).unwrap();
    let mut column = open_bim_api::Member {
        id: None,
        kind: MemberKind::Column,
        name: String::new(),
        profile: "Гн 150×100×6".into(),
        material: "С255".into(),
        start: Point3([0., 0., 0.]),
        end: Point3([0., 0., 3000.]),
        position: Default::default(),
        properties: Default::default(),
    };
    let id = s.insert_member(&mut column).unwrap();
    let defs: Vec<plugins::InputDef> =
        serde_json::from_value(serde_json::json!([{ "name": "c", "type": "part" }])).unwrap();
    let inputs = [("c".to_string(), InputValue::Part(id))].into();
    let f = plugins::input_frame(s.project(), &defs, &inputs);
    assert!(close(f.axes[0], [0., 0., 1.]));
    assert!(close(f.axes[2], [1., 0., 0.]));
    f.validate().unwrap();

    // The frame goes to the program with the request.
    let dir = temp("frame").join("probe");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("run.sh"),
        "if grep -q '\"frame\":{\"origin\"'; then echo '{\"steps\":[]}'; \
         else echo '{\"error\":\"нет frame\"}'; fi\n",
    )
    .unwrap();
    let manifest = serde_json::json!({
        "name": "Проба", "kind": "model", "program": shell(), "args": ["run.sh"]
    });
    std::fs::write(dir.join(plugins::MANIFEST), manifest.to_string()).unwrap();
    let probe = plugins::load(&dir).unwrap();
    s.run_plugin(&probe, &BTreeMap::new(), &BTreeMap::new())
        .unwrap();
}
