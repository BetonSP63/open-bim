use open_bim_drawings::{general_drawing, generate};
use open_bim_model::{LinePen, LineType, ObjectId, SheetSize};

#[test]
fn custom_view_pen_is_exported_with_width_and_dash_pattern() {
    let f = open_bim_drawings::sample::control_project();
    let mut d = general_drawing(
        &f.project,
        ObjectId(900),
        vec![f.beam],
        "УЗ-1",
        SheetSize::A3,
    )
    .unwrap();
    d.views[0].appearance.visible = LinePen {
        line_type: LineType::DashDot,
        width: 1.2,
    };
    let sheet = generate(&f.project, &d).unwrap();
    let dxf = open_bim_drawing_export::dxf::render(&sheet);
    assert!(dxf.contains(" 40\n1.2\n 41\n1.2\n"));
    assert!(dxf.contains("OB_DASHDOT"));
    let pdf = open_bim_drawing_export::pdf::render(&[sheet], "Узел").unwrap();
    // PDF content streams are compressed; inspect the actual operators.
    use std::io::Read;
    let mut contents = Vec::new();
    for (i, _) in pdf
        .windows(7)
        .enumerate()
        .filter(|(_, w)| *w == b"stream\n")
    {
        let start = i + 7;
        if let Some(end) = pdf[start..].windows(9).position(|w| w == b"endstream") {
            let mut z = flate2::read::ZlibDecoder::new(&pdf[start..start + end]);
            let mut decoded = Vec::new();
            if z.read_to_end(&mut decoded).is_ok() {
                contents.extend(decoded);
            }
        }
    }
    let content = String::from_utf8_lossy(&contents);
    assert!(content.contains("1.2 w [12 3 1 3] 0 d"));
}
