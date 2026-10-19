//! «Общие данные по рабочим чертежам» (ГОСТ Р 21.101-2026, 4.3): the first
//! sheet of a set with the sheet list (form 1), referenced and attached
//! documents (form 2), the list of sets (form 2, on the КМ sheet when both
//! sets exist), the list of specifications (form 1, when there are several)
//! and general notes — top to bottom, then left to right, each under its
//! heading, without numbering of the elements.
use crate::tables::{Drawn, Row, Table, form1, form2};
use crate::{Align, Error, Label, Polyline, Status, set_mark, sheet_number, status};
use open_bim_model::{
    DetailKind, Drawing, DrawingKind, ObjectId, Project, STANDARD_SCALES, SheetSize,
};
use std::collections::{BTreeMap, BTreeSet};

/// Column width of the elements (forms 1 and 2 are 185 mm) and the gap
/// between columns.
const COLUMN: f64 = 185.;
const GAP: f64 = 10.;
/// Space between elements in a column.
const SPACING: f64 = 10.;

/// Name of a set by its mark (appendix В).
pub fn set_name(mark: &str) -> &'static str {
    match mark {
        "КМД" => "Конструкции металлические деталировочные",
        _ => "Конструкции металлические",
    }
}

/// A new «Общие данные» drawing for the set `mark` on the smallest sheet
/// (A3, A2, A1) its elements fit.
pub fn general_data_drawing(project: &Project, id: ObjectId, mark: &str) -> Result<Drawing, Error> {
    let mut drawing = Drawing {
        id,
        kind: DrawingKind::GeneralData { mark: mark.into() },
        number: format!("{mark}-ОД"),
        title: "Общие данные".into(),
        sheet: SheetSize::A3,
        scale: STANDARD_SCALES[0],
        views: Vec::new(),
        dimensions: Vec::new(),
        texts: Vec::new(),
        objects: Vec::new(),
        fingerprints: BTreeMap::new(),
        generated_revision: 0,
        issued_revision: None,
        multiple: 1,
        portrait: false,
    };
    drawing.validate().map_err(|e| e.to_string())?;
    for sheet in [SheetSize::A3, SheetSize::A2, SheetSize::A1] {
        drawing.sheet = sheet;
        let size = drawing.sheet_mm();
        let reserved = crate::title_block_area(project, &drawing)?;
        if place(&elements(project, &drawing)?, size, reserved).is_ok() {
            return Ok(crate::updated(project, &drawing));
        }
    }
    Err("Общие данные не помещаются даже на A1".into())
}

/// Drawings of the set `mark` in sheet order, «Общие данные» first.
pub fn set_drawings<'a>(project: &'a Project, mark: &str) -> Vec<&'a Drawing> {
    let mut list: Vec<&Drawing> = project
        .drawings
        .values()
        .filter(|d| set_mark(&d.kind) == mark)
        .collect();
    list.sort_by_key(|d| sheet_number(project, d).1);
    list
}

/// Elements of the sheet in the order of 4.3.1.
pub fn elements(project: &Project, drawing: &Drawing) -> Result<Vec<Table>, Error> {
    let mark = set_mark(&drawing.kind);
    let mut out = Vec::new();
    // Sheet list: every sheet of the set, this one included.
    let mut sheets: Vec<(usize, String, String)> = set_drawings(project, mark)
        .iter()
        .filter(|d| d.id != drawing.id)
        .map(|d| {
            let note = match status(project, d) {
                Status::UpToDate => String::new(),
                Status::Outdated { .. } => "Устарел".into(),
                Status::LostReferences { .. } => "Потеряны привязки".into(),
            };
            (sheet_number(project, d).1, d.title.clone(), note)
        })
        .collect();
    sheets.push((
        sheet_number(project, drawing).1,
        drawing.title.clone(),
        String::new(),
    ));
    sheets.sort_by_key(|s| s.0);
    out.push(form1(
        "Ведомость рабочих чертежей основного комплекта",
        sheets
            .into_iter()
            .map(|(n, title, note)| Row::Cells(vec![n.to_string(), title, note]))
            .collect(),
    ));
    // Referenced documents: standards of the fasteners and welds used.
    let mut referenced: BTreeSet<(String, String)> = BTreeSet::new();
    for detail in project.details.values() {
        match &detail.kind {
            DetailKind::BoltGroup { standard, .. } if !standard.trim().is_empty() => {
                referenced.insert((standard.trim().into(), "Болты".into()));
            }
            DetailKind::Weld { .. } => {
                referenced.insert((
                    "ГОСТ 5264-80".into(),
                    "Ручная дуговая сварка. Соединения сварные. Основные типы, конструктивные элементы и размеры".into(),
                ));
            }
            _ => {}
        }
    }
    let other = if mark == "КМ" { "КМД" } else { "КМ" };
    let other_exists = !set_drawings(project, other).is_empty();
    // КМД is developed from the КМ set: КМ is a referenced document.
    if mark == "КМД" && other_exists {
        referenced.insert((
            project.document.designation("КМ"),
            format!("{}. Рабочие чертежи", set_name("КМ")),
        ));
    }
    let mut documents = Vec::new();
    if !referenced.is_empty() {
        documents.push(Row::Heading("Ссылочные документы".into()));
        documents.extend(
            referenced
                .into_iter()
                .map(|(d, n)| Row::Cells(vec![d, n, String::new()])),
        );
    }
    if !documents.is_empty() {
        out.push(form2(
            "Ведомость ссылочных и прилагаемых документов",
            documents,
        ));
    }
    if mark == "КМ" && other_exists {
        out.push(form2(
            "Ведомость основных комплектов рабочих чертежей",
            ["КМ", "КМД"]
                .iter()
                .map(|m| {
                    Row::Cells(vec![
                        project.document.designation(m),
                        set_name(m).into(),
                        String::new(),
                    ])
                })
                .collect(),
        ));
    }
    // Specifications of the set: rolled steel on КМ schemes, form 7 on
    // assembly drawings; listed when there are several (4.3.1).
    let specs: Vec<Row> = set_drawings(project, mark)
        .iter()
        .filter_map(|d| {
            let name = match &d.kind {
                DrawingKind::General { .. } => "Спецификация металлопроката".to_string(),
                DrawingKind::Assembly { .. } => format!("Спецификация. {}", d.title),
                _ => return None,
            };
            Some(Row::Cells(vec![
                sheet_number(project, d).1.to_string(),
                name,
                String::new(),
            ]))
        })
        .collect();
    if specs.len() > 1 {
        out.push(form1("Ведомость спецификаций", specs));
    }
    out.push(notes(project, mark)?);
    Ok(out)
}

/// General notes: the user's items, then the program's (masses, grades,
/// welds, units).
fn notes(project: &Project, mark: &str) -> Result<Table, Error> {
    let mut items: Vec<String> = project
        .document
        .general_notes
        .iter()
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .collect();
    let parts: Vec<ObjectId> = project.elements.keys().copied().collect();
    if !parts.is_empty() {
        let steel = crate::tables::rolled_steel(project, &parts)?;
        let grades: Vec<String> = steel.by_grade.iter().map(|g| g.grade.clone()).collect();
        items.push(format!(
            "Масса металлоконструкций по модели — {} т, марки стали: {}.",
            crate::tables::mass(steel.total.total_t),
            grades.join(", ")
        ));
    }
    if project
        .details
        .values()
        .any(|d| matches!(d.kind, DetailKind::Weld { .. }))
    {
        items.push("Сварные соединения — по ГОСТ 5264-80, катеты швов — по чертежам.".into());
    }
    items.push(if mark == "КМ" {
        "Размеры — в миллиметрах, отметки — в метрах.".into()
    } else {
        "Размеры — в миллиметрах.".into()
    });
    Ok(Table {
        title: Some("Общие указания".into()),
        columns: vec![crate::tables::Column {
            title: String::new(),
            width: COLUMN,
            align: Align::Start,
        }],
        groups: Vec::new(),
        header: 0.,
        name_column: 0,
        rows: items
            .iter()
            .enumerate()
            .map(|(i, t)| Row::Cells(vec![format!("{}. {t}", i + 1)]))
            .collect(),
    })
}

/// Top-left corners of the elements: columns of 185 mm from the left, the
/// last column above the main inscription.
fn place(tables: &[Table], size: [f64; 2], reserved: [f64; 4]) -> Result<Vec<[f64; 2]>, Error> {
    let left = crate::MARGIN_LEFT + 5.;
    let top = size[1] - crate::MARGIN - 10.;
    let mut column = 0;
    let mut y = top;
    let mut out = Vec::new();
    for t in tables {
        loop {
            let x = left + column as f64 * (COLUMN + GAP);
            if x + COLUMN > size[0] - crate::MARGIN {
                return Err("Общие данные не помещаются на листе — выберите формат больше".into());
            }
            let floor = if x + COLUMN > reserved[0] {
                reserved[3] + 5.
            } else {
                crate::MARGIN + 5.
            };
            if y - t.height() >= floor {
                out.push([x, y]);
                y -= t.height() + SPACING;
                break;
            }
            if y == top {
                return Err("Элемент общих данных выше листа — выберите формат больше".into());
            }
            column += 1;
            y = top;
        }
    }
    Ok(out)
}

/// Lines and texts of the sheet.
pub fn content(
    project: &Project,
    drawing: &Drawing,
    size: [f64; 2],
    reserved: [f64; 4],
) -> Result<(Vec<Polyline>, Vec<Label>), Error> {
    let tables = elements(project, drawing)?;
    let corners = place(&tables, size, reserved)?;
    let mut lines = Vec::new();
    let mut labels = Vec::new();
    for (t, at) in tables.iter().zip(corners) {
        let Drawn {
            lines: l,
            labels: b,
            ..
        } = t.draw(at);
        // The notes table is text only: no frame.
        if t.header == 0. {
            labels.extend(b);
            continue;
        }
        lines.extend(l);
        labels.extend(b);
    }
    Ok((lines, labels))
}
