use open_bim_api::{ApiError, DrawingEdit, DrawingRules, Member, Note, Session, batch};
use open_bim_model::{DrawingKind, MemberKind, ObjectId, Point3, Project};

fn beam(y: f64) -> Member {
    Member {
        id: None,
        kind: MemberKind::Beam,
        name: "Балка".into(),
        profile: "30Б1 (Р 57837)".into(),
        material: "С255".into(),
        start: Point3([0., y, 3000.]),
        end: Point3([6000., y, 3000.]),
        position: Default::default(),
        properties: Default::default(),
    }
}

fn temp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("open-bim-api-dr-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Two equal beams and a column, committed.
fn model() -> (Session, [ObjectId; 3]) {
    let mut s = Session::new(Project::default()).unwrap();
    let a = s.insert_member(&mut beam(0.)).unwrap();
    let b = s.insert_member(&mut beam(6000.)).unwrap();
    let mut col = beam(0.);
    col.kind = MemberKind::Column;
    col.end = Point3([0., 0., 0.]);
    let c = s.insert_member(&mut col).unwrap();
    s.commit("Каркас").unwrap();
    (s, [a, b, c])
}

fn invalid<T: std::fmt::Debug>(r: Result<T, ApiError>) -> String {
    match r {
        Err(ApiError::Invalid(m)) => m,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn wizard_creates_one_drawing_per_mark() {
    let (mut s, _) = model();
    let rules = DrawingRules {
        assemblies: false,
        ..Default::default()
    };
    let ids = s.create_drawings(&rules).unwrap();
    s.commit("Чертежи").unwrap();
    let list = s.drawings();
    let parts = list
        .iter()
        .filter(|d| matches!(d.kind, DrawingKind::Part { .. }))
        .count();
    assert_eq!(parts, 2, "equal beams share a drawing: {list:#?}");
    assert!(
        list.iter()
            .any(|d| matches!(&d.kind, DrawingKind::GeneralData { mark } if mark == "КМД"))
    );
    assert_eq!(ids.len(), list.len());
    assert!(list.iter().all(|d| d.up_to_date && !d.issued));
    // A second run has nothing left to create.
    assert!(invalid(s.create_drawings(&rules)).contains("уже есть"));
}

#[test]
fn only_the_open_drawing_is_edited_and_saving_is_explicit() {
    let (mut s, [a, b, _]) = model();
    let first = s
        .create_drawing(DrawingKind::Part { part: a }, None)
        .unwrap();
    let general = s
        .create_drawing(DrawingKind::General { objects: vec![] }, Some("КМ-1"))
        .unwrap();
    s.commit("Чертежи").unwrap();
    assert!(s.drawings()[0].number.starts_with("КМД-"));

    let edit = DrawingEdit {
        title: Some("Балка перекрытия".into()),
        scale: Some(20.),
        notes: vec![Note {
            text: "Сварка по ГОСТ 14771".into(),
            position: [30., 40.],
            height: 3.5,
        }],
        ..Default::default()
    };
    assert!(invalid(s.edit_drawing(&edit)).contains("Нет открытого"));

    s.open_drawing(first).unwrap();
    assert_eq!(s.active_drawing(), Some(first));
    assert!(invalid(s.open_drawing(general)).contains("закройте"));
    assert!(invalid(s.create_drawing(DrawingKind::Part { part: b }, None)).contains("закройте"));
    assert!(invalid(s.delete_drawing(first)).contains("открыт"));
    assert!(invalid(s.update_drawings(&[first])).contains("открыт"));
    assert!(invalid(s.undo()).contains("закройте"));

    // Closing without saving drops the edits.
    s.edit_drawing(&edit).unwrap();
    assert!(!s.close_drawing(false).unwrap());
    assert_eq!(
        s.project().drawings[&first].scale,
        s.committed().drawings[&first].scale
    );
    assert_eq!(s.staged(), 0);

    // A bad edit is refused and leaves the open drawing as it was.
    s.open_drawing(first).unwrap();
    let bad = DrawingEdit {
        scale: Some(7.),
        ..Default::default()
    };
    assert!(s.edit_drawing(&bad).is_err());
    s.edit_drawing(&edit).unwrap();
    assert!(s.close_drawing(true).unwrap());
    assert_eq!(s.staged(), 1, "one replacement for the whole session");
    s.commit("Оформление").unwrap();
    let d = &s.committed().drawings[&first];
    assert_eq!(d.title, "Балка перекрытия");
    assert_eq!(d.scale, 20.);
    assert_eq!(d.texts.last().unwrap().text, "Сварка по ГОСТ 14771");

    s.delete_drawing(general).unwrap();
    s.commit("Удалить").unwrap();
    assert!(!s.committed().drawings.contains_key(&general));
}

#[test]
fn update_issue_and_export_follow_the_model_state() {
    let dir = temp("export");
    let (mut s, [a, _, _]) = model();
    let d = s
        .create_drawing(DrawingKind::Part { part: a }, None)
        .unwrap();
    assert!(matches!(
        s.export_pdf(&dir.join("x.pdf"), &[]),
        Err(ApiError::Uncommitted(1))
    ));
    s.commit("Чертёж").unwrap();

    // The part changes: the drawing becomes outdated and cannot be issued.
    let mut changed = s.select_member(a).unwrap();
    changed.end = Point3([7000., 0., 3000.]);
    s.modify_member(&changed).unwrap();
    s.commit("Длиннее").unwrap();
    assert!(!s.drawings()[0].up_to_date);
    assert!(
        s.issue_drawings(&[]).unwrap().is_empty(),
        "outdated ones are skipped"
    );
    assert!(invalid(s.issue_drawings(&[d])).contains("нельзя выдать"));

    assert_eq!(s.update_drawings(&[]).unwrap(), vec![d]);
    assert_eq!(s.issue_drawings(&[]).unwrap(), vec![d]);
    s.commit("Выдача").unwrap();
    assert!(s.drawings()[0].issued);

    let pdf = dir.join("set.pdf");
    assert_eq!(s.export_pdf(&pdf, &[]).unwrap(), 1);
    assert!(std::fs::read(&pdf).unwrap().starts_with(b"%PDF"));
    assert_eq!(s.export_dxf(&dir.join("dxf"), &[d]).unwrap(), 1);
    assert_eq!(std::fs::read_dir(dir.join("dxf")).unwrap().count(), 1);
}

#[test]
fn batch_drawing_steps() {
    let dir = temp("batch");
    let text = r#"{ "steps": [
        { "insert_member": { "profile": "30Б1 (Р 57837)", "material": "С255",
            "start": [0, 0, 3000], "end": [6000, 0, 3000] } },
        { "commit": "Балка" },
        { "create_drawing": { "part": 10, "number": "КМД-Б1" } },
        { "open_drawing": null },
        { "edit_drawing": { "title": "Балка Б1", "sheet": "a3" } },
        { "close_drawing": true },
        { "commit": "Чертёж" },
        { "issue_drawings": [] },
        { "commit": "Выдача" },
        { "export_pdf": { "path": "out/set.pdf" } },
        { "export_dxf": { "dir": "out/dxf" } }
    ] }"#;
    let script = batch::parse(text).unwrap();
    let (session, report) = batch::run(&script, &dir).unwrap();
    let drawing = session.drawings().remove(0);
    assert_eq!(drawing.number, "КМД-Б1");
    assert_eq!(drawing.title, "Балка Б1");
    assert!(drawing.issued);
    assert_eq!(report.steps[7].ids, vec![drawing.id]);
    assert!(dir.join("out/set.pdf").exists());
    assert!(dir.join("out/dxf/КМД-Б1.dxf").exists());

    let two_kinds = r#"{ "steps": [ { "create_drawing": { "part": 1, "assembly": 2 } } ] }"#;
    let error = batch::run(&batch::parse(two_kinds).unwrap(), &dir)
        .err()
        .unwrap();
    assert_eq!(error.step, 1);
}
