//! ГОСТ specifications and «Общие данные»: form 7 on the assembly drawing
//! with the model masses, the ГОСТ 21.502 rolled-steel specification with
//! totals per grade, the sheet lists of «Общие данные» (ГОСТ Р 21.101-2026,
//! 4.3, appendix Б) and reference exports for the independent check in
//! tests/gost/specifications.
use open_bim_drawing_export::{dxf, pdf};
use open_bim_drawings::sample::control_set;
use open_bim_drawings::tables::{Row, assembly_specification, mass};
use open_bim_drawings::{generate, sheet_number};
use open_bim_model::{DrawingKind, ObjectId, Project};
use std::collections::BTreeMap;

fn drawing_of(p: &Project, f: impl Fn(&DrawingKind) -> bool) -> &open_bim_model::Drawing {
    p.drawings.values().find(|d| f(&d.kind)).unwrap()
}

fn texts(sheet: &open_bim_drawings::Sheet) -> Vec<String> {
    sheet.labels.iter().map(|l| l.text.clone()).collect()
}

#[test]
fn assembly_drawing_carries_form_7_with_model_masses() {
    let p = control_set().unwrap();
    let d = drawing_of(&p, |k| matches!(k, DrawingKind::Assembly { .. }));
    let DrawingKind::Assembly { assembly } = d.kind else {
        unreachable!()
    };
    let rows = assembly_specification(&p, assembly).unwrap();
    assert_eq!(rows[0], Row::Heading("Детали".into()));
    let sheet = generate(&p, d).unwrap();
    let all = texts(&sheet);
    for header in [
        "Спецификация",
        "Поз.",
        "Обозначение",
        "Наименование",
        "Кол.",
        "Масса",
        "ед., кг",
    ] {
        assert!(all.iter().any(|t| t == header), "{header}");
    }
    // Each part row: count and unit mass of the model.
    let numbering = open_bim_numbering::number(&p).unwrap();
    for id in &p.assemblies[&assembly].members {
        let position = &numbering.parts[id];
        let row = rows
            .iter()
            .find_map(|r| match r {
                Row::Cells(c) if &c[0] == position => Some(c.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(row[4], mass(p.element_mass_kg(*id).unwrap()));
        assert!(all.contains(&row[4]), "{position}");
    }
    // The table sits above the inscription, clear of the views.
    let qa = open_bim_drawing_export::qa::check(&sheet).unwrap();
    assert!(qa.is_empty(), "{qa:?}");
}

#[test]
fn rolled_steel_totals_per_grade_equal_part_masses() {
    let p = control_set().unwrap();
    let rs = open_bim_reports::gost_tables::rolled_steel(&p).unwrap();
    let mut by_grade: BTreeMap<String, f64> = BTreeMap::new();
    for id in p.elements.keys() {
        *by_grade
            .entry(open_bim_drawings::tables::grade(&p, *id))
            .or_default() += p.element_mass_kg(*id).unwrap() / 1000.;
    }
    assert_eq!(rs.by_grade.len(), by_grade.len());
    for g in &rs.by_grade {
        assert!((g.total_t - by_grade[&g.grade]).abs() < 1e-9, "{}", g.grade);
    }
    let total: f64 = by_grade.values().sum();
    assert!((rs.total.total_t - total).abs() < 1e-9);
    // The base plate welded to a column counts as column steel.
    let columns = rs.types.iter().position(|t| *t == "Колонны").unwrap();
    let plate = rs
        .lines
        .iter()
        .find(|l| l.assortment == "Прокат листовой ГОСТ 19903-2015")
        .unwrap();
    assert!(plate.masses_t[columns] > 0.);
    let csv = open_bim_reports::gost_tables::rolled_steel_csv(&rs);
    assert!(csv.contains("Всего масса металла"));
    // The scheme sheet shows the table with the same totals.
    let ga = drawing_of(&p, |k| matches!(k, DrawingKind::General { .. }));
    let all = texts(&generate(&p, ga).unwrap());
    assert!(all.iter().any(|t| t == "Спецификация металлопроката"));
    assert!(all.contains(&mass(rs.total.total_t)));
}

#[test]
fn general_data_lists_every_sheet_with_continuous_numbers() {
    let p = control_set().unwrap();
    for mark in ["КМ", "КМД"] {
        let od = drawing_of(
            &p,
            |k| matches!(k, DrawingKind::GeneralData { mark: m } if m == mark),
        );
        assert_eq!(sheet_number(&p, od).1, 1, "{mark}: «Общие данные» — лист 1");
        let set: Vec<_> = open_bim_drawings::general_data::set_drawings(&p, mark);
        let numbers: Vec<usize> = set.iter().map(|d| sheet_number(&p, d).1).collect();
        assert_eq!(numbers, (1..=set.len()).collect::<Vec<_>>(), "{mark}");
        let tables = open_bim_drawings::general_data::elements(&p, od).unwrap();
        assert_eq!(
            tables[0].title.as_deref(),
            Some("Ведомость рабочих чертежей основного комплекта")
        );
        let rows: Vec<Vec<String>> = tables[0]
            .rows
            .iter()
            .map(|r| match r {
                Row::Cells(c) => c.clone(),
                Row::Heading(h) => vec![h.clone()],
            })
            .collect();
        assert_eq!(rows.len(), set.len());
        for (row, d) in rows.iter().zip(&set) {
            assert_eq!(row[0], sheet_number(&p, d).1.to_string());
            assert_eq!(row[1], d.title);
            assert_eq!(row[2], "", "актуальный лист — без примечания");
        }
        let sheet = generate(&p, od).unwrap();
        let all = texts(&sheet);
        // Graph 4 of the inscription: «Общие данные».
        assert!(all.iter().any(|t| t == "Общие данные"));
        assert!(all.iter().any(|t| t == "Общие указания"));
        let qa = open_bim_drawing_export::qa::check(&sheet).unwrap();
        assert!(qa.is_empty(), "{mark}: {qa:?}");
    }
    // КМ lists both sets; КМД attaches КМ; welds refer to ГОСТ 5264-80.
    let km = drawing_of(
        &p,
        |k| matches!(k, DrawingKind::GeneralData { mark } if mark == "КМ"),
    );
    let km_tables = open_bim_drawings::general_data::elements(&p, km).unwrap();
    let titles: Vec<_> = km_tables.iter().filter_map(|t| t.title.clone()).collect();
    assert!(titles.contains(&"Ведомость основных комплектов рабочих чертежей".to_string()));
    assert!(titles.contains(&"Ведомость ссылочных и прилагаемых документов".to_string()));
    let all = texts(&generate(&p, km).unwrap());
    assert!(all.iter().any(|t| t.contains("ГОСТ 5264-80")));
    assert!(all.iter().any(|t| t == "Ссылочные документы"));
    // A changed model marks the sheets «Устарел» in the list.
    let mut changed = p.clone();
    let beam = changed
        .elements
        .values()
        .find(|e| e.name == "Балка")
        .unwrap()
        .id;
    if let open_bim_model::ElementShape::Member { length, .. } =
        &mut changed.elements.get_mut(&beam).unwrap().shape
    {
        *length = open_bim_model::LengthMm::new(5000.).unwrap();
    }
    let tables = open_bim_drawings::general_data::elements(&changed, km).unwrap();
    assert!(
        tables[0]
            .rows
            .iter()
            .any(|r| matches!(r, Row::Cells(c) if c[2] == "Устарел"))
    );
}

#[test]
fn reference_sheets_for_the_independent_check() {
    let p = control_set().unwrap();
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/gost/specifications");
    let bless = std::env::var_os("OPEN_BIM_BLESS").is_some();
    if bless {
        std::fs::create_dir_all(dir.join("native")).unwrap();
    }
    // The model without drawings for the native run, where the drawings
    // are created from the interface.
    let mut model = p.clone();
    model.drawings.clear();
    let mut files: Vec<(String, Vec<u8>)> = vec![
        (
            "control-set.obim".into(),
            open_bim_persistence::encode(&p).unwrap(),
        ),
        (
            "native/control-set-model.obim".into(),
            open_bim_persistence::encode(&model).unwrap(),
        ),
    ];
    let names: BTreeMap<ObjectId, &str> = p
        .drawings
        .values()
        .map(|d| {
            let name = match &d.kind {
                DrawingKind::GeneralData { mark } if mark == "КМ" => "km-od",
                DrawingKind::GeneralData { .. } => "kmd-od",
                DrawingKind::General { .. } => "km-scheme",
                DrawingKind::Assembly { .. } => "kmd-assembly",
                DrawingKind::Part { .. } => "kmd-part",
            };
            (d.id, name)
        })
        .collect();
    for (id, name) in &names {
        let sheet = generate(&p, &p.drawings[id]).unwrap();
        files.push((format!("{name}.dxf"), dxf::render(&sheet).into_bytes()));
        files.push((
            format!("{name}.pdf"),
            pdf::render(std::slice::from_ref(&sheet), name).unwrap(),
        ));
    }
    let rs = open_bim_reports::gost_tables::rolled_steel(&p).unwrap();
    files.push((
        "rolled-steel.csv".into(),
        open_bim_reports::gost_tables::rolled_steel_csv(&rs).into_bytes(),
    ));
    for (name, bytes) in files {
        let path = dir.join(&name);
        if bless {
            std::fs::write(&path, &bytes).unwrap();
        }
        let stored = std::fs::read(&path).unwrap_or_else(|_| panic!("{name} missing"));
        assert!(stored == bytes, "{name} differs from the reference");
    }
}
