//! Main inscriptions and frames of ГОСТ Р 21.101-2026 (appendices Е, Ж):
//! form geometry, form choice per sheet, graphs 1–27, sheet counting,
//! formats, scales, line widths and font heights.
use open_bim_drawings::sample::control_project;
use open_bim_drawings::title_block::{
    self, DocumentClass, FONT_SIZES, Form, SheetInfo, form_for, margins,
};
use open_bim_drawings::{LineStyle, generate, part_drawing, sheet_number, svg};
use open_bim_model::{DocumentInfo, ObjectId, Person, SheetSize, standard_scale};

fn info() -> DocumentInfo {
    DocumentInfo {
        base_designation: "2345".into(),
        object: "Производственный корпус".into(),
        building: "Каркас пролёта 1-5".into(),
        organisation: "ООО «Проект»".into(),
        customer: "АО «Заказчик»".into(),
        developer: Person {
            name: "Иванов".into(),
            date: "19.10.26".into(),
            ..Default::default()
        },
        norm_control: Person {
            name: "Петров".into(),
            date: "20.10.26".into(),
            ..Default::default()
        },
        approver: Person {
            position: "ГИП".into(),
            name: "Сидоров".into(),
            date: "21.10.26".into(),
        },
        inventory_number: "1234".into(),
        ..Default::default()
    }
}

fn sheet(info: &DocumentInfo, size: [f64; 2], form: Form, no: usize, of: usize) -> SheetInfo<'_> {
    SheetInfo {
        info,
        size,
        form,
        designation: info.designation("КМД"),
        title: "Балка Б1".into(),
        sheet_no: no,
        sheets: of,
        format: "A3".into(),
        material: Some("С255".into()),
        mass_kg: Some(123.456),
        scale: Some(10.),
        version: 1,
    }
}

fn cell(tb: &title_block::TitleBlock, graph: u8) -> [f64; 4] {
    tb.cells.iter().find(|c| c.graph == graph).unwrap().rect
}

fn text_in(tb: &title_block::TitleBlock, r: [f64; 4]) -> String {
    tb.labels
        .iter()
        .filter(|l| {
            l.position[0] >= r[0] - 1e-6
                && l.position[0] <= r[2] + 1e-6
                && l.position[1] >= r[1] - 1e-6
                && l.position[1] <= r[3] + 1e-6
        })
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn form_3_columns_sit_at_appendix_e_coordinates() {
    let i = info();
    let tb = title_block::build(&sheet(&i, [420., 297.], Form::F3, 1, 1)).unwrap();
    // Inner frame 20/5/5/5, inscription 185×55 at its lower right corner.
    assert_eq!(margins([420., 297.]), [20., 5., 5., 5.]);
    assert_eq!(tb.block, [230., 5., 415., 60.]);
    let x0 = 230.;
    for (graph, rect) in [
        (1, [65., 45., 185., 55.]),
        (2, [65., 30., 185., 45.]),
        (3, [65., 15., 135., 30.]),
        (4, [65., 0., 135., 15.]),
        (6, [135., 15., 150., 25.]),
        (7, [150., 15., 165., 25.]),
        (8, [165., 15., 185., 25.]),
        (9, [135., 0., 185., 15.]),
        (14, [0., 45., 10., 50.]),
        (19, [55., 45., 65., 50.]),
    ] {
        let r = cell(&tb, graph);
        let want = [x0 + rect[0], 5. + rect[1], x0 + rect[2], 5. + rect[3]];
        for k in 0..4 {
            assert!(
                (r[k] - want[k]).abs() < 1e-9,
                "graph {graph}: {r:?} vs {want:?}"
            );
        }
    }
    // Eight signature rows of graphs 10–13 below the header.
    assert_eq!(tb.cells.iter().filter(|c| c.graph == 11).count(), 8);
    // Graph 27 above the inscription when a customer is set; QR above that.
    assert_eq!(cell(&tb, 27), [x0 + 65., 60., 415., 70.]);
    assert!(tb.fills.iter().all(|f| f[1] > 70. && f[2] <= 415.));
    assert!(!tb.fills.is_empty());
}

#[test]
fn forms_4_5_6_have_their_own_grids() {
    let i = info();
    let f4 = title_block::build(&sheet(&i, [210., 297.], Form::F4, 1, 2)).unwrap();
    assert_eq!(f4.block, [20., 5., 205., 60.]);
    assert_eq!(cell(&f4, 5), [85., 20., 155., 45.]);
    assert_eq!(cell(&f4, 23), [85., 5., 155., 20.]);
    assert!(text_in(&f4, cell(&f4, 23)).contains("С255"));
    assert!(text_in(&f4, cell(&f4, 24)).contains("123,5"));
    assert!(text_in(&f4, cell(&f4, 25)).contains("1:10"));
    let f5 = title_block::build(&sheet(&i, [210., 297.], Form::F5, 1, 3)).unwrap();
    assert_eq!(f5.block, [20., 5., 205., 45.]);
    assert_eq!(f5.cells.iter().filter(|c| c.graph == 11).count(), 6);
    let f6 = title_block::build(&sheet(&i, [210., 297.], Form::F6, 2, 3)).unwrap();
    assert_eq!(f6.block, [20., 5., 205., 20.]);
    assert_eq!(cell(&f6, 1), [85., 5., 195., 20.]);
    assert_eq!(cell(&f6, 7), [195., 5., 205., 13.]);
    assert_eq!(text_in(&f6, cell(&f6, 7)), "2");
    assert!(
        f6.cells.iter().all(|c| c.graph != 8),
        "no sheet count on later sheets"
    );
}

#[test]
fn form_choice_follows_5_2_2() {
    assert_eq!(form_for(DocumentClass::WorkingSet, 1), Form::F3);
    assert_eq!(form_for(DocumentClass::WorkingSet, 7), Form::F3);
    assert_eq!(form_for(DocumentClass::ProductDrawing, 1), Form::F4);
    assert_eq!(form_for(DocumentClass::ProductDrawing, 2), Form::F6);
    assert_eq!(form_for(DocumentClass::TextDocument, 1), Form::F5);
    assert_eq!(form_for(DocumentClass::TextDocument, 2), Form::F6);
    assert_eq!(form_for(DocumentClass::TextWithoutTitle, 1), Form::F3);
    assert_eq!(form_for(DocumentClass::TextWithoutTitle, 3), Form::F6);
}

#[test]
fn three_sheet_set_counts_sheets_and_fills_graphs() {
    let mut f = control_project();
    f.project.document = info();
    let ids = [ObjectId(9001), ObjectId(9002), ObjectId(9003)];
    for (i, id) in ids.iter().enumerate() {
        let d = part_drawing(&f.project, *id, f.beam, &format!("КМД-{}", i + 1)).unwrap();
        f.project.drawings.insert(*id, d);
    }
    for (i, id) in ids.iter().enumerate() {
        let d = &f.project.drawings[id];
        let (designation, no, of) = sheet_number(&f.project, d);
        assert_eq!((designation.as_str(), no, of), ("2345-КМД", i + 1, 3));
        let s = generate(&f.project, d).unwrap();
        let texts: Vec<&str> = s.labels.iter().map(|l| l.text.as_str()).collect();
        assert!(texts.contains(&"Листов"));
        assert!(texts.contains(&"2345-КМД"));
        assert!(texts.contains(&"Р"), "graph 6: stage Р");
        assert!(texts.contains(&"Формат A3"), "graph 26");
        assert!(texts.contains(&"19.10.26") && texts.contains(&"Иванов"));
        assert!(texts.contains(&"ГИП") && texts.contains(&"Сидоров"));
        // Graph 7 on every sheet, graph 8 («3») only on the first.
        let label_at = |g: u8| {
            let tb = title_block::build(&SheetInfo {
                info: &f.project.document,
                size: s.size,
                form: Form::F3,
                designation: designation.clone(),
                title: d.title.clone(),
                sheet_no: no,
                sheets: of,
                format: "A3".into(),
                material: None,
                mass_kg: None,
                scale: None,
                version: 1,
            })
            .unwrap();
            text_in(&tb, cell(&tb, g))
        };
        assert_eq!(label_at(7), (i + 1).to_string());
        assert_eq!(
            label_at(8),
            if i == 0 {
                "3".to_string()
            } else {
                String::new()
            }
        );
    }
    // One-sheet document: graph 7 stays empty.
    let i1 = info();
    let one = title_block::build(&sheet(&i1, [420., 297.], Form::F3, 1, 1)).unwrap();
    assert_eq!(text_in(&one, cell(&one, 7)), "");
    assert_eq!(text_in(&one, cell(&one, 8)), "1");
}

#[test]
fn formats_scales_lines_and_fonts() {
    // A4 landscape: binding field at the top (Ж.1 в); multiples per ГОСТ 2.301.
    assert_eq!(margins([297., 210.]), [5., 5., 5., 20.]);
    let f = control_project();
    let mut d = part_drawing(&f.project, ObjectId(9100), f.beam, "КМД-1").unwrap();
    d.multiple = 3;
    assert_eq!(d.sheet_mm(), [891., 420.]);
    assert_eq!(d.format_label(), "A3×3");
    d.multiple = 9;
    assert!(d.validate().is_err(), "A3×9 is not a ГОСТ 2.301 format");
    d.multiple = 1;
    d.sheet = SheetSize::A4;
    d.portrait = true;
    assert_eq!(d.sheet_mm(), [210., 297.]);
    // Scales: only the ГОСТ 2.302 series.
    for ok in [
        1., 2., 2.5, 4., 5., 10., 15., 20., 25., 40., 50., 75., 100., 0.5,
    ] {
        assert!(standard_scale(ok), "{ok}");
    }
    for bad in [3., 30., 60., 0.3] {
        assert!(!standard_scale(bad), "{bad}");
    }
    let mut bad = part_drawing(&f.project, ObjectId(9101), f.beam, "КМД-2").unwrap();
    bad.scale = 3.;
    assert!(bad.validate().is_err(), "1:3 rejected");
    // Frame and inscription outlines thick, inner rows thin; text heights
    // from the ГОСТ 2.304 series.
    let s = generate(
        &f.project,
        &part_drawing(&f.project, ObjectId(9102), f.beam, "КМД-3").unwrap(),
    )
    .unwrap();
    let width = |style| svg::stroke(style).1;
    assert!(width(LineStyle::Frame) >= 0.5);
    assert!(width(LineStyle::Thin) <= width(LineStyle::Frame) / 2.);
    assert!(
        s.frame
            .iter()
            .filter(|l| l.style == LineStyle::Frame)
            .count()
            > 20
    );
    assert!(s.frame.iter().any(|l| l.style == LineStyle::Thin));
    for l in &s.labels {
        assert!(
            FONT_SIZES.iter().any(|h| (h - l.height).abs() < 1e-9),
            "«{}» height {}",
            l.text,
            l.height
        );
    }
}
