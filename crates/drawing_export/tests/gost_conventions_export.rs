//! Reference КМ and КМД sheets for the independent conventions check in
//! tests/gost/conventions.
use open_bim_drawing_export::{dxf, pdf};
use open_bim_drawings::sample::{control_building, control_project};
use open_bim_drawings::{general_drawing, generate, part_drawing, svg};
use open_bim_model::{DrawingView, ObjectId, SheetSize, ViewDirection};
use std::path::PathBuf;

#[test]
fn reference_sheets_for_conventions() {
    let p = control_building();
    let mut ga = general_drawing(&p, ObjectId(9000), vec![], "КМ-1", SheetSize::A1).unwrap();
    ga.views.push(DrawingView {
        id: 3,
        name: "Разрез".into(),
        direction: ViewDirection::Front,
        section: Some(-3000.),
        position: [620., 500.],
        scale: Some(100.),
        node: None,
        source: None,
    });
    // «Узел 1»: the joint of column 1/А and the beam at +3,600 at 1:20.
    ga.views.push(DrawingView {
        id: 4,
        name: "Узел".into(),
        direction: ViewDirection::Front,
        section: None,
        position: [700., 330.],
        scale: Some(20.),
        node: Some(open_bim_model::NodeRegion {
            centre: [0., 3600.],
            radius: 500.,
        }),
        source: None,
    });
    let f = control_project();
    let part = part_drawing(&f.project, ObjectId(9100), f.beam, "КМД-1").unwrap();
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/gost/conventions");
    for (name, sheet) in [
        ("km-1", generate(&p, &ga).unwrap()),
        ("kmd-1", generate(&f.project, &part).unwrap()),
    ] {
        for (ext, bytes) in [
            ("dxf", dxf::render(&sheet).into_bytes()),
            (
                "pdf",
                pdf::render(std::slice::from_ref(&sheet), name).unwrap(),
            ),
            ("svg", svg::render(&sheet).into_bytes()),
        ] {
            let path = dir.join(format!("{name}.{ext}"));
            if std::env::var_os("OPEN_BIM_BLESS").is_some() {
                std::fs::write(&path, &bytes).unwrap();
            }
            let stored = std::fs::read(&path).unwrap_or_else(|_| panic!("{name}.{ext} missing"));
            assert!(stored == bytes, "{name}.{ext} differs from the reference");
        }
    }
}
