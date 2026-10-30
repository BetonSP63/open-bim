use open_bim_api::{ApiError, Box3, Member, ObjectKind, Plate, Query, Session, batch};
use open_bim_model::{Frame, MemberKind, Point3, Project, PropertyValue};

const BEAM: &str = "30Б1 (Р 57837)";

fn beam(start: [f64; 3], end: [f64; 3]) -> Member {
    Member {
        id: None,
        kind: MemberKind::Beam,
        name: "Балка".into(),
        profile: BEAM.into(),
        material: "С255".into(),
        start: Point3(start),
        end: Point3(end),
        position: Default::default(),
        properties: Default::default(),
    }
}

fn close(a: Point3, b: [f64; 3]) -> bool {
    (0..3).all(|i| (a.0[i] - b[i]).abs() < 1e-6)
}

fn temp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("open-bim-api-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn object_values_change_the_model_only_through_insert_and_commit() {
    let mut s = Session::new(Project::default()).unwrap();
    let mut b = beam([0., 0., 3000.], [6000., 0., 3000.]);
    b.end = Point3([7000., 0., 3000.]);
    assert!(
        s.project().elements.is_empty(),
        "editing a value is not an operation"
    );

    let id = s.insert_member(&mut b).unwrap();
    assert_eq!(b.id, Some(id));
    assert!(
        s.project().elements.contains_key(&id),
        "queries see staged changes"
    );
    assert!(s.committed().elements.is_empty(), "nothing committed yet");
    assert_eq!(s.staged(), 3, "catalogue profile, steel and the beam");

    let event = s.commit("Балка").unwrap().unwrap();
    assert_eq!(event.revision, 1);
    assert_eq!(s.staged(), 0);
    assert!(s.commit("пусто").unwrap().is_none());

    let back = s.select_member(id).unwrap();
    assert!(close(back.start, [0., 0., 3000.]) && close(back.end, [7000., 0., 3000.]));
    assert_eq!(back.profile, BEAM);
    assert!(back.material.starts_with("С255"));

    // One undo removes the whole commit, catalogue entries included.
    s.undo().unwrap();
    assert!(s.project().elements.is_empty());
    assert!(!s.project().profiles.values().any(|p| p.name == BEAM));
}

#[test]
fn insert_and_modify_check_their_arguments() {
    let mut s = Session::new(Project::default()).unwrap();
    let mut same = beam([0.; 3], [0.; 3]);
    assert!(matches!(
        s.insert_member(&mut same),
        Err(ApiError::Invalid(_))
    ));
    let mut unknown = beam([0.; 3], [1000., 0., 0.]);
    unknown.profile = "Нет такого".into();
    assert!(matches!(
        s.insert_member(&mut unknown),
        Err(ApiError::Invalid(_))
    ));
    assert_eq!(s.staged(), 0, "a refused insert stages nothing");

    let fresh = beam([0.; 3], [1000., 0., 0.]);
    assert_eq!(s.modify_member(&fresh), Err(ApiError::NotInserted));

    let mut b = fresh.clone();
    let id = s.insert_member(&mut b).unwrap();
    assert!(matches!(s.insert_member(&mut b), Err(ApiError::Invalid(_))));
    b.end = Point3([2500., 0., 0.]);
    b.kind = MemberKind::Column;
    s.modify_member(&b).unwrap();
    s.commit("").unwrap();
    let back = s.select_member(id).unwrap();
    assert!(close(back.end, [2500., 0., 0.]));
    assert_eq!(back.kind, MemberKind::Column);
}

#[test]
fn save_is_separate_from_commit() {
    let dir = temp("save");
    let file = dir.join("model.obim");
    let mut s = Session::new(Project::default()).unwrap();
    s.insert_member(&mut beam([0.; 3], [3000., 0., 0.]))
        .unwrap();
    assert_eq!(s.save(Some(&file)), Err(ApiError::Uncommitted(3)));
    assert!(matches!(s.for_export(), Err(ApiError::Uncommitted(3))));
    s.commit("Балка").unwrap();
    s.save(Some(&file)).unwrap();

    let mut reopened = Session::open(&file).unwrap();
    assert_eq!(reopened.committed(), s.committed());
    reopened
        .insert_member(&mut beam([0.; 3], [0., 3000., 0.]))
        .unwrap();
    assert_eq!(
        reopened.rollback(),
        1,
        "profile and steel are already in the model"
    );
    assert_eq!(reopened.project(), s.committed());
    reopened.save(None).unwrap();
}

#[test]
fn queries_by_kind_profile_property_and_box() {
    let mut s = Session::new(Project::default()).unwrap();
    let a = s
        .insert_member(&mut beam([0., 0., 3000.], [6000., 0., 3000.]))
        .unwrap();
    let mut col = beam([0., 0., 0.], [0., 0., 3000.]);
    col.kind = MemberKind::Column;
    col.profile = "Гн 150×100×6".into();
    let c = s.insert_member(&mut col).unwrap();
    s.set_property(c, "этап", Some(PropertyValue::Text("1".into())))
        .unwrap();

    let all = s.query(&Query::default()).unwrap();
    assert_eq!(all, vec![a, c]);
    let columns = Query {
        kinds: vec![ObjectKind::Column],
        ..Default::default()
    };
    assert_eq!(s.query(&columns).unwrap(), vec![c]);
    let by_profile = Query {
        profile: Some(BEAM.into()),
        ..Default::default()
    };
    assert_eq!(s.query(&by_profile).unwrap(), vec![a]);
    let by_property = Query {
        property: Some(("этап".into(), PropertyValue::Text("1".into()))),
        ..Default::default()
    };
    assert_eq!(s.query(&by_property).unwrap(), vec![c]);
    let by_steel = Query {
        material: Some("С255".into()),
        ..Default::default()
    };
    assert_eq!(s.query(&by_steel).unwrap().len(), 2);

    let low = Box3 {
        min: Point3([-500., -500., -10.]),
        max: Point3([500., 500., 2500.]),
    };
    let inside = Query {
        inside: Some(low),
        ..Default::default()
    };
    assert!(
        s.query(&inside).unwrap().is_empty(),
        "the column reaches 3000 mm"
    );
    let touching = Query {
        touching: Some(low),
        ..Default::default()
    };
    assert_eq!(s.query(&touching).unwrap(), vec![c]);
}

#[test]
fn copy_and_move_by_vector_and_between_frames() {
    let mut s = Session::new(Project::default()).unwrap();
    let a = s
        .insert_member(&mut beam([0., 0., 3000.], [6000., 0., 3000.]))
        .unwrap();
    let copies = s.copy_by(&[a], [0., 6000., 0.]).unwrap();
    assert_eq!(copies.len(), 1);
    assert_ne!(copies[0], a);
    let copy = s.select_member(copies[0]).unwrap();
    assert!(close(copy.start, [0., 6000., 3000.]) && close(copy.end, [6000., 6000., 3000.]));

    s.move_by(&[a], [0., 0., 600.]).unwrap();
    let moved = s.select_member(a).unwrap();
    assert!(close(moved.start, [0., 0., 3600.]));

    // From the world frame to a frame turned 90° about z at (10000, 0, 0).
    let turned = Frame {
        origin: Point3([10000., 0., 0.]),
        axes: [[0., 1., 0.], [-1., 0., 0.], [0., 0., 1.]],
    };
    let rotated = s.copy_between(&[a], &Frame::default(), &turned).unwrap();
    let r = s.select_member(rotated[0]).unwrap();
    assert!(close(r.start, [10000., 0., 3600.]), "{:?}", r.start);
    assert!(close(r.end, [10000., 6000., 3600.]), "{:?}", r.end);
    s.commit("Копии").unwrap();
    assert_eq!(s.committed().elements.len(), 3);
}

#[test]
fn plate_from_a_world_outline() {
    let mut s = Session::new(Project::default()).unwrap();
    // Vertical plate in the XZ plane.
    let outline = [
        [0., 0., 0.],
        [400., 0., 0.],
        [400., 0., 300.],
        [0., 0., 300.],
    ];
    let mut plate = Plate {
        id: None,
        name: "Пластина".into(),
        material: "С255".into(),
        thickness: 12.,
        outline: outline.iter().map(|p| Point3(*p)).collect(),
        properties: Default::default(),
    };
    let id = s.insert_plate(&mut plate).unwrap();
    s.commit("Пластина").unwrap();
    let back = s.select_plate(id).unwrap();
    assert_eq!(back.thickness, 12.);
    for (p, q) in back.outline.iter().zip(outline) {
        assert!(close(*p, q), "{p:?} vs {q:?}");
    }

    let mut bent = plate.clone();
    bent.id = None;
    bent.outline[2] = Point3([400., 50., 300.]);
    assert!(matches!(
        s.insert_plate(&mut bent),
        Err(ApiError::Invalid(_))
    ));
}

#[test]
fn batch_macro_runs_steps_and_writes_files() {
    let dir = temp("batch");
    let text = format!(
        r#"{{ "steps": [
            {{ "insert_member": {{ "profile": "{BEAM}", "material": "С255",
                "start": [0, 0, 3000], "end": [6000, 0, 3000] }} }},
            {{ "copy": {{ "vector": [0, 6000, 0], "count": 3 }} }},
            {{ "set_property": {{ "key": "этап", "value": {{ "type": "text", "value": "2" }} }} }},
            {{ "commit": "Балки перекрытия" }},
            {{ "select": {{ "property": ["этап", {{ "type": "text", "value": "2" }}] }} }},
            {{ "move": {{ "vector": [0, 0, 300] }} }},
            {{ "commit": "Поднять" }},
            {{ "export_ifc": "out/model.ifc" }},
            {{ "export_ifc43": "out/model-IFC4X3.ifc" }},
            {{ "export_reports": "out/reports" }},
            {{ "save": "out/model.obim" }}
        ] }}"#
    );
    let script = batch::parse(&text).unwrap();
    let (session, report) = batch::run(&script, &dir).unwrap();
    assert_eq!(report.parts, 4);
    assert_eq!(report.revision, 2);
    assert_eq!(report.staged, 0);
    assert_eq!(report.steps[1].ids.len(), 3);
    assert_eq!(
        report.steps[4].ids.len(),
        3,
        "only the copies got the property"
    );
    assert!(dir.join("out/model.ifc").exists());
    let ifc43 = std::fs::read_to_string(dir.join("out/model-IFC4X3.ifc")).unwrap();
    assert!(ifc43.contains("FILE_SCHEMA(('IFC4X3_ADD2'));"));
    assert_eq!(report.steps[8].action, "export_ifc43");
    assert!(dir.join("out/reports/parts.csv").exists());
    let saved = open_bim_persistence::load(dir.join("out/model.obim")).unwrap();
    assert_eq!(&saved, session.committed());
    let raised = session.select_member(report.steps[4].ids[0]).unwrap();
    assert!(close(raised.start, [0., 6000., 3300.]));

    // A failing step names its number; saving with staged changes fails.
    let failing = batch::parse(
        r#"{ "steps": [
            { "insert_member": { "profile": "30Б1 (Р 57837)", "material": "С255",
                "start": [0, 0, 0], "end": [1000, 0, 0] } },
            { "save": "x.obim" } ] }"#,
    )
    .unwrap();
    let error = match batch::run(&failing, &dir) {
        Err(error) => error,
        Ok(_) => panic!("save with staged changes must fail"),
    };
    assert_eq!(error.step, 2);
    assert!(matches!(error.error, ApiError::Uncommitted(_)));
    assert!(batch::parse(r#"{ "steps": [ { "explode": 1 } ] }"#).is_err());
}
