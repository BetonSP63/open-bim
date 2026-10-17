use open_bim_drawing_export::{dxf, pdf, qa, svg};
use open_bim_drawings::{
    LineStyle, Sheet, assembly_drawing, general_drawing, generate, part_drawing,
    sample::control_project,
};
use open_bim_model::{ObjectId, SheetSize};
use std::path::PathBuf;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/exports")
}

fn bless_or_compare(name: &str, bytes: &[u8]) {
    let path = dir().join(name);
    if std::env::var_os("OPEN_BIM_BLESS").is_some() {
        std::fs::write(&path, bytes).unwrap();
    }
    let expected = std::fs::read(&path).unwrap_or_else(|_| panic!("{name} missing"));
    assert!(expected == bytes, "{name} differs from reference export");
}

/// Control set: A3 part/assembly sheets of the control model and an A1
/// general arrangement of the phase-9 connection model.
fn control_set() -> Vec<(&'static str, Sheet)> {
    static SET: std::sync::OnceLock<Vec<(&'static str, Sheet)>> = std::sync::OnceLock::new();
    SET.get_or_init(build_control_set).clone()
}

fn build_control_set() -> Vec<(&'static str, Sheet)> {
    let f = control_project();
    let p = &f.project;
    let connections = open_bim_persistence_free::load();
    let mut ga = general_drawing(
        &connections,
        ObjectId(9000),
        vec![],
        "ОБ-КМ-2",
        SheetSize::A1,
    )
    .unwrap();
    ga.views[0].name = "Фасад по оси А".into();
    vec![
        (
            "beam-a3",
            generate(
                p,
                &part_drawing(p, ObjectId(900), f.beam, "ОБ-КМД-Б1").unwrap(),
            )
            .unwrap(),
        ),
        (
            "plate-a3",
            generate(
                p,
                &part_drawing(p, ObjectId(901), f.plate, "ОБ-КМД-П1").unwrap(),
            )
            .unwrap(),
        ),
        (
            "assembly-a3",
            generate(
                p,
                &assembly_drawing(p, ObjectId(902), f.assembly, "ОБ-КМД-С1").unwrap(),
            )
            .unwrap(),
        ),
        ("general-a1", generate(&connections, &ga).unwrap()),
    ]
}

mod open_bim_persistence_free {
    /// The phase-9 GUI fixture read through serde only (no persistence crate).
    pub fn load() -> open_bim_model::Project {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/connections_flows/gui-project.obim"
        ))
        .unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        open_bim_model::Project::from_json(&value["project"].to_string()).unwrap()
    }
}

#[test]
fn control_set_exports_are_deterministic_and_laid_out_cleanly() {
    let sheets = control_set();
    let mut summary = serde_json::Map::new();
    for (name, sheet) in &sheets {
        let issues = qa::check(sheet).unwrap();
        assert!(issues.is_empty(), "{name}: {issues:?}");
        bless_or_compare(&format!("{name}.svg"), svg(sheet).as_bytes());
        bless_or_compare(&format!("{name}.dxf"), dxf::render(sheet).as_bytes());
        bless_or_compare(
            &format!("{name}.pdf"),
            &pdf::render(std::slice::from_ref(sheet), name).unwrap(),
        );
        // Counts for the independent DXF reader (verify.py).
        let mut counts = serde_json::Map::new();
        for style in [
            LineStyle::Visible,
            LineStyle::Hidden,
            LineStyle::Section,
            LineStyle::Hatch,
            LineStyle::Thin,
            LineStyle::Frame,
            LineStyle::Lost,
        ] {
            let count = sheet
                .frame
                .iter()
                .chain(sheet.views.iter().flat_map(|v| &v.lines))
                .chain(sheet.dimensions.iter().flat_map(|d| &d.lines))
                .filter(|l| l.style == style && l.points.len() >= 2)
                .count();
            counts.insert(dxf::layer(style).0.into(), count.into());
        }
        let texts = sheet.views.iter().map(|v| v.labels.len()).sum::<usize>()
            + sheet
                .dimensions
                .iter()
                .map(|d| d.labels.len())
                .sum::<usize>()
            + sheet.labels.len();
        counts.insert(dxf::TEXT_LAYER.into(), texts.into());
        summary.insert(
            (*name).into(),
            serde_json::json!({"size_mm": sheet.size, "entities": counts}),
        );
    }
    bless_or_compare(
        "summary.json",
        serde_json::to_string_pretty(&summary).unwrap().as_bytes(),
    );
    // Multi-sheet PDF keeps each page size (A3 + A1).
    let all: Vec<Sheet> = sheets.into_iter().map(|(_, s)| s).collect();
    bless_or_compare(
        "control-set.pdf",
        &pdf::render(&all, "Комплект ОБ").unwrap(),
    );
}

#[test]
fn control_dimension_is_true_to_scale_in_sheet_coordinates() {
    let sheets = control_set();
    let beam = &sheets[0].1;
    let length = &beam.dimensions[0];
    assert_eq!(length.values_mm, vec![3000.]);
    // Dimension line between the arrows: 3000 mm at 1:10 = 300 mm of paper.
    let line = &length.lines[2];
    let paper = (line.points[1][0] - line.points[0][0]).abs();
    assert!((paper - 300.).abs() < 1e-9, "{paper}");
    assert_eq!(length.labels[0].text, "3000");
}

#[test]
fn layout_check_detects_clipping_and_overlap() {
    let mut sheet = control_set().remove(0).1;
    let mut far = sheet.labels[0].clone();
    far.position = [415., 100.];
    far.text = "Подпись за рамкой".into();
    sheet.views[0].labels.push(far);
    let mut twin = sheet.views[0].labels[0].clone();
    twin.text = "Наложение".into();
    sheet.views[0].labels.push(twin);
    let issues = qa::check(&sheet).unwrap();
    assert!(
        issues
            .iter()
            .any(|i| matches!(i, qa::Issue::Clipped { text } if text == "Подпись за рамкой"))
    );
    assert!(
        issues
            .iter()
            .any(|i| matches!(i, qa::Issue::Overlap { second, .. } if second == "Наложение"))
    );
}
