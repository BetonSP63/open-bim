//! Changes and the issued package (ГОСТ Р 21.101-2026, sections 7 and 8):
//! an edited issued sheet reissued as change 1 with its change row and
//! contour, a second change appending row 2, the registration table and
//! journal, annulment keeping sheet numbers, and the package written to
//! tests/gost/release for the independent check.
use open_bim_commands::{Command, Document};
use open_bim_drawings::sample::control_set;
use open_bim_drawings::{generate, sheet_number};
use open_bim_model::{DrawingKind, ElementShape, LengthMm, ObjectId, Person, SheetChangeKind};
use open_bim_reports::gost_release::{
    ChangeRequest, annul_sheet, change_journal, export_package, issue_set, record_change,
    registration_table,
};

fn document() -> Document {
    let mut p = control_set().unwrap();
    let d = &mut p.document;
    d.base_designation = "2345-26".into();
    d.organisation = "ООО «Проект»".into();
    d.object = "Производственный корпус".into();
    d.building = "Каркас здания".into();
    d.inventory_number = "1234".into();
    d.developer = Person {
        position: String::new(),
        name: "Иванов".into(),
        date: "19.10.26".into(),
    };
    d.checker = Person {
        position: String::new(),
        name: "Смирнов".into(),
        date: "19.10.26".into(),
    };
    d.norm_control = Person {
        position: String::new(),
        name: "Петров".into(),
        date: "20.10.26".into(),
    };
    d.approver = Person {
        position: "ГИП".into(),
        name: "Сидоров".into(),
        date: "20.10.26".into(),
    };
    Document::new(p).unwrap()
}

fn beam(doc: &Document) -> ObjectId {
    let p = doc.project();
    let DrawingKind::Part { part } = p
        .drawings
        .values()
        .find(|d| matches!(d.kind, DrawingKind::Part { .. }))
        .unwrap()
        .kind
    else {
        unreachable!()
    };
    part
}

fn lengthen(doc: &mut Document, part: ObjectId, by: f64) {
    let mut element = doc.project().elements[&part].clone();
    if let ElementShape::Member { length, .. } = &mut element.shape {
        *length = LengthMm::new(length.value() + by).unwrap();
    }
    doc.transact("Длина", &[Command::ReplaceElement { element }])
        .unwrap();
}

fn request(permission: &str, date: &str, text: &str) -> ChangeRequest {
    ChangeRequest {
        mark: "КМД".into(),
        permission: permission.into(),
        date: date.into(),
        description: text.into(),
        reason: Some(4),
        author: "Иванов".into(),
        replace: false,
    }
}

/// Two changes of КМД: the beam lengthened twice.
fn changed_twice() -> Document {
    let mut doc = document();
    for mark in ["КМ", "КМД"] {
        let issue = issue_set(doc.project(), mark).unwrap();
        doc.transact("Выдать", &issue).unwrap();
    }
    let part = beam(&doc);
    // The beam is on the КМ scheme too: one common permission changes both
    // sets (7.1.4, 7.2.4).
    for (by, permission, date, text) in [
        (500., "15-26", "21.10.26", "Длина балки 3500"),
        (250., "16-26", "22.10.26", "Длина балки 3750"),
    ] {
        lengthen(&mut doc, part, by);
        let commands: Vec<Command> = ["КМД", "КМ"]
            .iter()
            .map(|mark| {
                let mut r = request(permission, date, text);
                r.mark = (*mark).into();
                record_change(doc.project(), &r).unwrap()
            })
            .collect();
        doc.transact(format!("Изм. по {permission}"), &commands)
            .unwrap();
    }
    doc
}

fn cell_texts(sheet: &open_bim_drawings::Sheet, rect: [f64; 4]) -> Vec<String> {
    sheet
        .labels
        .iter()
        .filter(|l| {
            l.position[0] >= rect[0] - 0.5
                && l.position[0] <= rect[2] + 0.5
                && l.position[1] >= rect[1] - 0.5
                && l.position[1] <= rect[3] + 0.5
        })
        .map(|l| l.text.clone())
        .collect()
}

#[test]
fn reissue_records_change_rows_and_contours() {
    let mut doc = document();
    // Not issued yet: no change can be recorded.
    assert!(record_change(doc.project(), &request("15-26", "21.10.26", "x")).is_err());
    for mark in ["КМ", "КМД"] {
        let issue = issue_set(doc.project(), mark).unwrap();
        doc.transact("Выдать", &issue).unwrap();
    }
    let part = beam(&doc);
    lengthen(&mut doc, part, 500.);
    let c1 = record_change(
        doc.project(),
        &request("15-26", "21.10.26", "Длина балки 3500"),
    )
    .unwrap();
    doc.transact("Изм. 1", &[c1]).unwrap();
    let p = doc.project();
    let change = &p.changes[0];
    assert_eq!((change.mark.as_str(), change.number), ("КМД", 1));
    let drawing = p
        .drawings
        .values()
        .find(|d| matches!(d.kind, DrawingKind::Part { .. }))
        .unwrap();
    let drawing_id = drawing.id;
    let entry = change
        .sheets
        .iter()
        .find(|s| s.drawing == drawing.id)
        .unwrap();
    let SheetChangeKind::Changed { areas } = &entry.kind else {
        panic!(
            "лист детали должен быть изменён участками: {:?}",
            entry.kind
        )
    };
    assert!(!areas.is_empty());
    // «Общие данные» replaced, assembly sheet untouched.
    assert!(
        change
            .sheets
            .iter()
            .any(|s| s.kind == SheetChangeKind::Replaced)
    );
    assert_eq!(change.sheets.len(), 2);
    // Graphs 14–19 of the reissued sheet: Изм. 1, Кол.уч, «-», № док., date.
    let sheet = generate(p, drawing).unwrap();
    assert_eq!(
        open_bim_drawings::status(p, drawing),
        open_bim_drawings::Status::UpToDate
    );
    let block = open_bim_drawings::title_block_area(p, drawing).unwrap();
    let (x0, y0) = (block[0], block[1]);
    let row = |n: f64| [x0, y0 + 40. + 5. * n, x0 + 65., y0 + 45. + 5. * n];
    let first = cell_texts(&sheet, row(1.));
    for t in ["1", &areas.len().to_string(), "-", "15-26", "21.10.26"] {
        assert!(first.iter().any(|x| x == t), "{t} in {first:?}");
    }
    // The contour and the parallelogram designation «1.1».
    assert!(sheet.labels.iter().any(|l| l.text == "1.1"));
    let contour = areas[0];
    assert!(sheet.frame.iter().any(|l| {
        l.points.len() == 5
            && (l.points[0][0] - contour[0]).abs() < 1e-9
            && (l.points[2][1] - contour[3]).abs() < 1e-9
    }));
    // The sheet list notes «Изм. 1».
    let od = p
        .drawings
        .values()
        .find(|d| matches!(&d.kind, DrawingKind::GeneralData { mark } if mark == "КМД"))
        .unwrap();
    let all: Vec<String> = generate(p, od)
        .unwrap()
        .labels
        .into_iter()
        .map(|l| l.text)
        .collect();
    assert!(all.iter().any(|t| t == "Изм. 1"));
    assert!(all.iter().any(|t| t == "Изм. 1 (Зам.)"), "ОД заменён");

    // Second change on the same sheet appends row 2.
    lengthen(&mut doc, part, 250.);
    let c2 = record_change(
        doc.project(),
        &request("16-26", "22.10.26", "Длина балки 3750"),
    )
    .unwrap();
    doc.transact("Изм. 2", &[c2]).unwrap();
    let p = doc.project();
    let drawing = &p.drawings[&drawing_id];
    let sheet = generate(p, drawing).unwrap();
    assert!(cell_texts(&sheet, row(1.)).iter().any(|t| t == "15-26"));
    let second = cell_texts(&sheet, row(2.));
    assert!(second.iter().any(|t| t == "2") && second.iter().any(|t| t == "16-26"));
    assert!(sheet.labels.iter().any(|l| l.text == "2.1"));
    assert_eq!(open_bim_drawings::set_version(p, "КМД"), 3);
    // A third change continues the table above the inscription.
    lengthen(&mut doc, part, 100.);
    let c3 = record_change(
        doc.project(),
        &request("17-26", "23.10.26", "Длина балки 3850"),
    )
    .unwrap();
    doc.transact("Изм. 3", &[c3]).unwrap();
    let p = doc.project();
    let sheet = generate(p, &p.drawings[&drawing_id]).unwrap();
    assert!(cell_texts(&sheet, row(3.)).iter().any(|t| t == "17-26"));
    let area = open_bim_drawings::title_block_area(p, &p.drawings[&drawing_id]).unwrap();
    assert!(area[3] >= y0 + 60. - 1e-9, "строка 3 над основной надписью");
    // Undo removes the change and restores the previous issue.
    doc.undo().unwrap();
    assert_eq!(doc.project().changes.len(), 2);
}

#[test]

fn registration_table_and_journal_list_both_changes() {
    let doc = changed_twice();
    let p = doc.project();
    let texts = |s: open_bim_drawings::Sheet| -> Vec<String> {
        s.labels.into_iter().map(|l| l.text).collect()
    };
    let part_sheet = p
        .drawings
        .values()
        .find(|d| matches!(d.kind, DrawingKind::Part { .. }))
        .map(|d| sheet_number(p, d).1.to_string())
        .unwrap();
    let table = texts(registration_table(p, "КМД"));
    for t in [
        "Таблица регистрации изменений",
        "15-26",
        "16-26",
        "21.10.26",
        "22.10.26",
    ] {
        assert!(table.iter().any(|x| x == t), "{t}");
    }
    assert!(table.iter().any(|x| x == &part_sheet), "лист {part_sheet}");
    let journal = texts(change_journal(p, "КМД")).join(" ");
    for t in [
        "Длина балки 3500",
        "Длина балки 3750",
        "Разрешение 15-26",
        "Разрешение 16-26",
    ] {
        assert!(journal.contains(t), "{t}");
    }
    assert!(journal.contains(&format!("листы 1, {part_sheet}")));
}

#[test]
fn annulled_sheet_keeps_the_numbers_of_later_sheets() {
    let mut doc = document();
    let issue = issue_set(doc.project(), "КМД").unwrap();
    doc.transact("Выдать", &issue).unwrap();
    let p = doc.project();
    let set = open_bim_drawings::general_data::set_drawings(p, "КМД");
    let numbers: Vec<usize> = set.iter().map(|d| sheet_number(p, d).1).collect();
    assert_eq!(numbers, [1, 2, 3]);
    let (assembly, part) = (set[1].id, set[2].id);
    let commands = annul_sheet(
        p,
        assembly,
        &request("18-26", "24.10.26", "Лист аннулировать"),
    )
    .unwrap();
    doc.transact("Аннулировать", &commands).unwrap();
    let p = doc.project();
    assert!(!p.drawings.contains_key(&assembly));
    assert_eq!(sheet_number(p, &p.drawings[&part]).1, 3, "номер сохранён");
    let od = p
        .drawings
        .values()
        .find(|d| {
            matches!(d.kind, DrawingKind::GeneralData { .. })
                && open_bim_drawings::set_mark(&d.kind) == "КМД"
        })
        .unwrap();
    let all: Vec<String> = generate(p, od)
        .unwrap()
        .labels
        .into_iter()
        .map(|l| l.text)
        .collect();
    assert!(all.iter().any(|t| t == "Изм. 1 (Аннул.)"));
}

#[test]
fn package_for_the_independent_check() {
    let doc = changed_twice();
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/gost/release/package");
    let scratch = std::env::temp_dir().join(format!("open-bim-package-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    let package = export_package(doc.project(), &scratch, "22.10.26 12:00").unwrap();
    assert_eq!(package.certifying_sheets.len(), 2);
    let names: Vec<&str> = package.files.iter().map(|f| f.path.as_str()).collect();
    for n in [
        "КМ/2345-26-КМ.pdf",
        "КМ/2345-26-КМ-Обложка.pdf",
        "КМ/2345-26-КМ-ТЛ.pdf",
        "КМ/2345-26-КМ-УЛ.pdf",
        "КМД/2345-26-КМД.pdf",
        "КМД/2345-26-КМД-ТРИ.pdf",
        "КМД/2345-26-КМД-ЖИ.pdf",
        "КМД/Разрешение-15-26.pdf",
        "КМД/Разрешение-16-26.pdf",
        "КМД/2345-26-КМД-УЛ.pdf",
    ] {
        assert!(names.contains(&n), "{n}");
    }
    // Deterministic: the stored reference package is byte-identical.
    let bless = std::env::var_os("OPEN_BIM_BLESS").is_some();
    let mut files: Vec<String> = package.files.iter().map(|f| f.path.clone()).collect();
    files.push("info.xml".into());
    if bless {
        let _ = std::fs::remove_dir_all(&dir);
        for f in &files {
            let to = dir.join(f);
            std::fs::create_dir_all(to.parent().unwrap()).unwrap();
            std::fs::copy(scratch.join(f), &to).unwrap();
        }
        std::fs::write(
            dir.join("../package.json"),
            serde_json::to_string_pretty(&package).unwrap(),
        )
        .unwrap();
        // Starting project of the native run: the control set with the
        // document attributes, nothing issued.
        std::fs::create_dir_all(dir.join("../native")).unwrap();
        std::fs::write(
            dir.join("../native/release-start.obim"),
            open_bim_persistence::encode(document().project()).unwrap(),
        )
        .unwrap();
        std::fs::write(
            dir.join("../project.obim"),
            open_bim_persistence::encode(doc.project()).unwrap(),
        )
        .unwrap();
    }
    for f in &files {
        let stored = std::fs::read(dir.join(f)).unwrap_or_else(|_| panic!("{f} missing"));
        assert!(
            stored == std::fs::read(scratch.join(f)).unwrap(),
            "{f} differs"
        );
    }
    let _ = std::fs::remove_dir_all(&scratch);
}
