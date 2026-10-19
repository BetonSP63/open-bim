//! Reference sheets of the ГОСТ Р 21.101-2026 main inscriptions (forms 3–6)
//! on A3, A1 and A4 for the independent check in tests/gost/title-blocks.
use open_bim_drawing_export::{dxf, pdf};
use open_bim_drawings::title_block::{self, Form, SheetInfo};
use open_bim_drawings::{Sheet, Status};
use open_bim_model::{DocumentInfo, ObjectId, Person};
use std::path::PathBuf;

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
        checker: Person {
            name: "Смирнов".into(),
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
        approvals: vec![Person {
            position: "КЖ".into(),
            name: "Орлов".into(),
            date: "18.10.26".into(),
        }],
        inventory_number: "1234".into(),
        ..Default::default()
    }
}

fn blank(
    info: &DocumentInfo,
    name: &str,
    size: [f64; 2],
    form: Form,
    no: usize,
    of: usize,
    format: &str,
) -> Sheet {
    let tb = title_block::build(&SheetInfo {
        info,
        size,
        form,
        designation: info.designation("КМД"),
        title: format!("Контрольный лист {name}"),
        sheet_no: no,
        sheets: of,
        format: format.into(),
        material: Some("С255 ГОСТ 27772-2021".into()),
        mass_kg: Some(123.4),
        scale: Some(10.),
        version: 1,
    })
    .unwrap();
    Sheet {
        drawing: ObjectId(1),
        size,
        status: Status::UpToDate,
        views: Vec::new(),
        dimensions: Vec::new(),
        frame: tb.lines,
        labels: tb.labels,
        fills: tb.fills,
        title_area: tb.reserved,
    }
}

#[test]
fn reference_sheets_for_forms_3_to_6() {
    let info = info();
    let sheets = [
        ("a3-form3", [420., 297.], Form::F3, 1, 3, "A3"),
        ("a1-form3", [841., 594.], Form::F3, 2, 3, "A1"),
        ("a4-form4", [210., 297.], Form::F4, 1, 2, "A4"),
        ("a4-form5", [210., 297.], Form::F5, 1, 2, "A4"),
        ("a4-form6", [210., 297.], Form::F6, 2, 2, "A4"),
        ("a4-landscape-form3", [297., 210.], Form::F3, 1, 1, "A4"),
    ];
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/gost/title-blocks");
    for (name, size, form, no, of, format) in sheets {
        let sheet = blank(&info, name, size, form, no, of, format);
        let dxf = dxf::render(&sheet);
        let pdf = pdf::render(std::slice::from_ref(&sheet), name).unwrap();
        for (ext, bytes) in [("dxf", dxf.as_bytes()), ("pdf", pdf.as_slice())] {
            let path = dir.join(format!("{name}.{ext}"));
            if std::env::var_os("OPEN_BIM_BLESS").is_some() {
                std::fs::write(&path, bytes).unwrap();
            }
            let stored = std::fs::read(&path).unwrap_or_else(|_| panic!("{name}.{ext} missing"));
            assert!(stored == bytes, "{name}.{ext} differs from the reference");
        }
    }
}
