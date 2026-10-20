//! Changes and the issued set (ГОСТ Р 21.101-2026, sections 7 and 8).
//!
//! * `issue_set` issues every sheet of a set (КМ or КМД);
//! * `record_change` makes the next change of an issued set:
//!   - changed sheets get their changed areas (the parts whose model data
//!     changed, appendix Л), or «Зам.» when issued as a replacement;
//!   - sheets added since the last issue are «Нов.»;
//!   - «Общие данные» is replaced, because its list changes;
//! * `export_package` writes the package of electronic documents (8.2):
//!   - a folder per set with one PDF of all its sheets, named by the
//!     designation (8.2.7), and the cover (form 14) and title page (form 12);
//!   - for a changed set, the change registration table (form 10), the
//!     change journal (form 11) and a permission (form 9) per change;
//!   - the certifying sheet (form 15) with the SHA-256 checksum, size and
//!     time of every file;
//!   - `info.xml` with stable document GUIDs (8.2.3, 8.2.4).
use crate::Error;
use open_bim_commands::Command;
use open_bim_drawings::conventions::em;
use open_bim_drawings::tables::{self, Column, Group, Row, Table};
use open_bim_drawings::{
    Align, Label, LineStyle, MARGIN, MARGIN_LEFT, Polyline, Sheet, Status, general_data, generate,
    set_mark, sheet_number, status, updated,
};
use open_bim_model::{
    Change, Drawing, DrawingKind, ObjectId, Project, SET_MARKS, SheetChange, SheetChangeKind,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::Path;

/// Issue every sheet of the set `mark`: outdated sheets are updated first.
pub fn issue_set(project: &Project, mark: &str) -> Result<Vec<Command>, Error> {
    let sheets = general_data::set_drawings(project, mark);
    if sheets.is_empty() {
        return Err(format!("В комплекте {mark} нет листов"));
    }
    sheets
        .into_iter()
        .map(|d| {
            let mut next = updated(project, d);
            if let Status::LostReferences { .. } = status(project, &next) {
                return Err(format!("Лист {}: потеряны привязки", d.number));
            }
            next.issued_revision = Some(project.revision);
            Ok(Command::ReplaceDrawing { drawing: next })
        })
        .collect()
}

/// What the user states for a change: the permission, date, content and
/// whether changed sheets are issued as replacements (no area marks).
#[derive(Clone, Debug, PartialEq)]
pub struct ChangeRequest {
    pub mark: String,
    pub permission: String,
    pub date: String,
    pub description: String,
    pub reason: Option<u8>,
    pub author: String,
    pub replace: bool,
}

fn rect_of(points: impl Iterator<Item = [f64; 2]>) -> Option<[f64; 4]> {
    points.fold(None, |r, p| {
        Some(match r {
            None => [p[0], p[1], p[0], p[1]],
            Some(r) => [
                r[0].min(p[0]),
                r[1].min(p[1]),
                r[2].max(p[0]),
                r[3].max(p[1]),
            ],
        })
    })
}

fn overlap(a: &[f64; 4], b: &[f64; 4]) -> bool {
    a[0] <= b[2] && b[0] <= a[2] && a[1] <= b[3] && b[1] <= a[3]
}

/// Changed areas of a regenerated sheet: per view, the extent of the lines
/// and dimensions of the changed objects; the specification table when the
/// sheet carries one. Areas are widened by 3 mm, kept inside the frame and
/// merged where they touch.
pub fn changed_areas(
    sheet: &Sheet,
    changed: &BTreeSet<ObjectId>,
    spec: Option<[f64; 4]>,
) -> Vec<[f64; 4]> {
    let mut areas: Vec<[f64; 4]> = Vec::new();
    for view in &sheet.views {
        let points = view
            .lines
            .iter()
            .filter(|l| l.object.is_some_and(|o| changed.contains(&o)))
            .flat_map(|l| l.points.iter().copied())
            .chain(
                sheet
                    .dimensions
                    .iter()
                    .filter(|d| d.view == view.id && d.objects.iter().any(|o| changed.contains(o)))
                    .flat_map(|d| d.lines.iter().flat_map(|l| l.points.iter().copied())),
            );
        if let Some(r) = rect_of(points) {
            areas.push(r);
        }
    }
    if !areas.is_empty()
        && let Some(s) = spec
    {
        areas.push(s);
    }
    let [w, h] = sheet.size;
    let mut out: Vec<[f64; 4]> = areas
        .into_iter()
        .map(|r| {
            [
                (r[0] - 3.).max(MARGIN_LEFT + 1.),
                (r[1] - 3.).max(MARGIN + 1.),
                (r[2] + 3.).min(w - MARGIN - 1.),
                (r[3] + 3.).min(h - MARGIN - 1.),
            ]
        })
        .collect();
    loop {
        let mut merged = false;
        'outer: for i in 0..out.len() {
            for j in i + 1..out.len() {
                if overlap(&out[i], &out[j]) {
                    let (a, b) = (out[i], out[j]);
                    out[i] = [
                        a[0].min(b[0]),
                        a[1].min(b[1]),
                        a[2].max(b[2]),
                        a[3].max(b[3]),
                    ];
                    out.remove(j);
                    merged = true;
                    break 'outer;
                }
            }
        }
        if !merged {
            return out;
        }
    }
}

/// Rectangle of the specification table on a sheet, as placed by the
/// drawing engine.
fn specification_rect(project: &Project, drawing: &Drawing) -> Result<Option<[f64; 4]>, Error> {
    let Some(table) = open_bim_drawings::specification(project, &drawing.kind) else {
        return Ok(None);
    };
    let area = open_bim_drawings::title_block_area(project, drawing)?;
    let w = drawing.sheet_mm()[0];
    let top = area[3] + 3. + table.height();
    Ok(Some([
        w - MARGIN - table.width(),
        top - table.height(),
        w - MARGIN,
        top,
    ]))
}

/// The next change of an issued set and the reissued drawings.
pub fn record_change(project: &Project, request: &ChangeRequest) -> Result<Command, Error> {
    let mark = request.mark.as_str();
    let sheets = general_data::set_drawings(project, mark);
    if !sheets.iter().any(|d| d.issued_revision.is_some()) {
        return Err(format!(
            "Комплект {mark} ещё не выдан: сначала «Выдать комплект»"
        ));
    }
    let mut changes = Vec::new();
    let mut drawings = Vec::new();
    let mut general = None;
    for d in &sheets {
        if matches!(d.kind, DrawingKind::GeneralData { .. }) {
            general = Some(*d);
            continue;
        }
        let kind = if d.issued_revision.is_none() {
            SheetChangeKind::New
        } else {
            match status(project, d) {
                Status::UpToDate => continue,
                Status::LostReferences { .. } => {
                    return Err(format!(
                        "Лист {}: потеряны привязки — исправьте модель",
                        d.number
                    ));
                }
                Status::Outdated { changed } if !request.replace => {
                    let next = updated(project, d);
                    let sheet = generate(project, &next)?;
                    let changed: BTreeSet<ObjectId> = changed.into_iter().collect();
                    let areas =
                        changed_areas(&sheet, &changed, specification_rect(project, &next)?);
                    if areas.is_empty() {
                        SheetChangeKind::Replaced
                    } else {
                        SheetChangeKind::Changed { areas }
                    }
                }
                Status::Outdated { .. } => SheetChangeKind::Replaced,
            }
        };
        let mut next = updated(project, d);
        next.issued_revision = Some(project.revision);
        changes.push(SheetChange {
            drawing: d.id,
            kind,
        });
        drawings.push(next);
    }
    if changes.is_empty() {
        return Err(format!(
            "В комплекте {mark} нет изменённых или новых листов"
        ));
    }
    // The sheet list of «Общие данные» changes with every change (7.5.2).
    if let Some(g) = general {
        let mut next = updated(project, g);
        next.issued_revision = Some(project.revision);
        changes.insert(
            0,
            SheetChange {
                drawing: g.id,
                kind: SheetChangeKind::Replaced,
            },
        );
        drawings.insert(0, next);
    }
    let number = project.changes.iter().filter(|c| c.mark == mark).count() as u32 + 1;
    Ok(Command::RecordChange {
        change: Box::new(Change {
            mark: mark.into(),
            number,
            permission: request.permission.trim().into(),
            date: request.date.trim().into(),
            description: request.description.trim().into(),
            reason: request.reason,
            author: request.author.trim().into(),
            sheets: changes,
        }),
        drawings,
    })
}

// ---- Sheets of the accompanying documents -------------------------------

const A4: [f64; 2] = [210., 297.];
const A3: [f64; 2] = [420., 297.];

fn line(points: Vec<[f64; 2]>, style: LineStyle) -> Polyline {
    Polyline {
        points,
        style,
        object: None,
    }
}

fn label(position: [f64; 2], text: impl Into<String>, size: f64, align: Align) -> Label {
    Label {
        position,
        text: text.into(),
        height: em(size),
        align,
        rotation: 0.,
        object: None,
        lost: false,
    }
}

fn rect(r: [f64; 4], style: LineStyle) -> Polyline {
    line(
        vec![
            [r[0], r[1]],
            [r[2], r[1]],
            [r[2], r[3]],
            [r[0], r[3]],
            [r[0], r[1]],
        ],
        style,
    )
}

/// A sheet with the sheet edge and the inner frame (20 mm binding margin).
fn blank(size: [f64; 2]) -> Sheet {
    Sheet {
        drawing: ObjectId(0),
        size,
        status: Status::UpToDate,
        views: Vec::new(),
        dimensions: Vec::new(),
        frame: vec![
            rect([0., 0., size[0], size[1]], LineStyle::Thin),
            rect(
                [MARGIN_LEFT, MARGIN, size[0] - MARGIN, size[1] - MARGIN],
                LineStyle::Frame,
            ),
        ],
        labels: Vec::new(),
        fills: Vec::new(),
        title_area: [0.; 4],
        objects: Vec::new(),
    }
}

/// Text centred in a cell, wrapped to its width (font 2.5).
fn cell_text(sheet: &mut Sheet, r: [f64; 4], text: &str, align: Align) {
    cell_text_sized(sheet, r, text, align, 2.5);
}

/// Text in a cell at font `size` (ГОСТ 2.304), wrapped to its width.
fn cell_text_sized(sheet: &mut Sheet, r: [f64; 4], text: &str, align: Align, size: f64) {
    if text.is_empty() {
        return;
    }
    let lines = tables::wrap_text(text, (r[2] - r[0] - 2.) * 2.5 / size);
    let n = lines.len() as f64;
    let pitch = size * 1.6;
    let first = (r[1] + r[3]) / 2. + (n - 1.) * pitch / 2. - size / 2.;
    for (i, l) in lines.into_iter().enumerate() {
        let x = match align {
            Align::Start => r[0] + 1.,
            Align::Middle => (r[0] + r[2]) / 2.,
            Align::End => r[2] - 1.,
        };
        sheet
            .labels
            .push(label([x, first - pitch * i as f64], l, size, align));
    }
}

fn draw_table(sheet: &mut Sheet, table: &Table, top_left: [f64; 2]) {
    let drawn = table.draw(top_left);
    sheet.frame.extend(drawn.lines);
    sheet.labels.extend(drawn.labels);
}

fn column(title: &str, width: f64, align: Align) -> Column {
    Column {
        title: title.into(),
        width,
        align,
    }
}

fn set_changes<'a>(project: &'a Project, mark: &str) -> Vec<&'a Change> {
    project.changes.iter().filter(|c| c.mark == mark).collect()
}

fn sheet_numbers(project: &Project, change: &Change, pick: fn(&SheetChangeKind) -> bool) -> String {
    let mut numbers: Vec<usize> = change
        .sheets
        .iter()
        .filter(|s| pick(&s.kind))
        .filter_map(|s| match &s.kind {
            SheetChangeKind::Annulled { sheet, .. } => Some(*sheet),
            _ => project
                .drawings
                .get(&s.drawing)
                .map(|d| sheet_number(project, d).1),
        })
        .collect();
    numbers.sort();
    if numbers.is_empty() {
        "-".into()
    } else {
        numbers
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Change registration table, form 10 (appendix М), on A4.
pub fn registration_table(project: &Project, mark: &str) -> Sheet {
    let mut sheet = blank(A4);
    let rows = set_changes(project, mark)
        .into_iter()
        .map(|c| {
            Row::Cells(vec![
                c.number.to_string(),
                sheet_numbers(project, c, |k| matches!(k, SheetChangeKind::Changed { .. })),
                sheet_numbers(project, c, |k| matches!(k, SheetChangeKind::Replaced)),
                sheet_numbers(project, c, |k| matches!(k, SheetChangeKind::New)),
                sheet_numbers(project, c, |k| {
                    matches!(k, SheetChangeKind::Annulled { .. })
                }),
                general_data::set_drawings(project, mark).len().to_string(),
                c.permission.clone(),
                String::new(),
                c.date.clone(),
            ])
        })
        .collect();
    let table = Table {
        title: Some("Таблица регистрации изменений".into()),
        columns: vec![
            column("Изм.", 12., Align::Middle),
            column("изменён-\nных", 22., Align::Middle),
            column("заменён-\nных", 22., Align::Middle),
            column("новых", 18., Align::Middle),
            column("аннули-\nрованных", 22., Align::Middle),
            column("Всего листов в док.", 22., Align::Middle),
            column("Номер док.", 25., Align::Middle),
            column("Подп.", 20., Align::Middle),
            column("Дата", 22., Align::Middle),
        ],
        groups: vec![Group {
            from: 1,
            to: 5,
            title: "Номера листов (страниц)".into(),
            height: 8.,
        }],
        header: 20.,
        name_column: 0,
        rows,
    };
    draw_table(&mut sheet, &table, [MARGIN_LEFT, A4[1] - MARGIN - 10.]);
    let des = project.document.designation(mark);
    sheet.labels.push(label(
        [A4[0] - MARGIN - 2., MARGIN + 3.],
        format!("{des} · таблица регистрации изменений"),
        2.5,
        Align::End,
    ));
    sheet
}

/// Change journal, form 11 (appendix Н), on A3.
pub fn change_journal(project: &Project, mark: &str) -> Sheet {
    let mut sheet = blank(A3);
    let des = project.document.designation(mark);
    let mut rows = vec![Row::Cells((1..=10).map(|n| n.to_string()).collect())];
    for c in set_changes(project, mark) {
        let sheets: Vec<String> = c
            .sheets
            .iter()
            .filter_map(|s| match &s.kind {
                SheetChangeKind::Annulled { sheet, .. } => Some(sheet.to_string()),
                _ => project
                    .drawings
                    .get(&s.drawing)
                    .map(|d| sheet_number(project, d).1.to_string()),
            })
            .collect();
        rows.push(Row::Cells(vec![
            c.number.to_string(),
            c.date.clone(),
            "-".into(),
            format!("{des}, листы {}", sheets.join(", ")),
            "-".into(),
            c.description.clone(),
            format!("{} {}", c.author, c.date).trim().to_string(),
            "-".into(),
            format!("Разрешение {} от {}", c.permission, c.date),
            c.reason
                .and_then(|r| open_bim_model::REASONS.iter().find(|x| x.0 == r))
                .map(|r| format!("Код {}: {}", r.0, r.1))
                .unwrap_or_default(),
        ]));
    }
    let table = Table {
        title: Some(format!("Журнал изменений {des}")),
        columns: vec![
            column("Изм.", 12., Align::Middle),
            column("Дата записи", 20., Align::Middle),
            column("Номер тома", 16., Align::Middle),
            column("Обозначение документа, номер листа", 45., Align::Start),
            column("Содержание замечаний экспертизы", 40., Align::Start),
            column("Содержание (описание) изменения", 75., Align::Start),
            column(
                "Должность, фамилия лица, внёсшего изменения, дата",
                45.,
                Align::Start,
            ),
            column("Отметка о согласовании изменений", 35., Align::Middle),
            column(
                "Отметка о внесении изменений в подлинники",
                45.,
                Align::Start,
            ),
            column("Примечание", 45., Align::Start),
        ],
        groups: Vec::new(),
        header: 25.,
        name_column: 5,
        rows,
    };
    draw_table(&mut sheet, &table, [MARGIN_LEFT + 5., A3[1] - MARGIN - 10.]);
    sheet
}

/// Permission for a change, form 9 (appendix К), on A4.
pub fn permission(project: &Project, change: &Change) -> Sheet {
    let mut sheet = blank(A4);
    let des = project.document.designation(&change.mark);
    let x0 = MARGIN_LEFT;
    let x1 = A4[0] - MARGIN;
    let top = A4[1] - MARGIN;
    let cols = [x0, x0 + 15., x0 + 30., x0 + 130., x0 + 145., x1];
    // Rows: «Разрешение | Обозначение | (2)» 10 mm, «(1) | Наименование
    // объекта | (3)» 20 mm, column captions 15 mm, content, signatures 20 mm.
    let r1 = top - 10.;
    let r2 = r1 - 20.;
    let r3 = r2 - 15.;
    let bottom = MARGIN + 20.;
    let frame = |s: &mut Sheet, r: [f64; 4]| s.frame.push(rect(r, LineStyle::Frame));
    frame(&mut sheet, [x0, bottom, x1, top]);
    for (a, b, text) in [
        (x0, x0 + 30., "Разрешение"),
        (x0 + 30., x0 + 60., "Обозначение"),
    ] {
        frame(&mut sheet, [a, r1, b, top]);
        cell_text(&mut sheet, [a, r1, b, top], text, Align::Middle);
    }
    frame(&mut sheet, [x0 + 60., r1, x1, top]);
    cell_text(&mut sheet, [x0 + 60., r1, x1, top], &des, Align::Middle);
    frame(&mut sheet, [x0, r2, x0 + 30., r1]);
    cell_text(
        &mut sheet,
        [x0, r2, x0 + 30., r1],
        &change.permission,
        Align::Middle,
    );
    frame(&mut sheet, [x0 + 30., r2, x0 + 60., r1]);
    cell_text(
        &mut sheet,
        [x0 + 30., r2, x0 + 60., r1],
        "Наименование объекта строительства",
        Align::Middle,
    );
    frame(&mut sheet, [x0 + 60., r2, x1, r1]);
    let object = [
        project.document.object.as_str(),
        project.document.building.as_str(),
    ]
    .iter()
    .filter(|s| !s.is_empty())
    .copied()
    .collect::<Vec<_>>()
    .join(". ");
    cell_text(&mut sheet, [x0 + 60., r2, x1, r1], &object, Align::Middle);
    for (i, text) in ["Изм.", "Лист", "Содержание изменения", "Код", "Примечание"]
        .iter()
        .enumerate()
    {
        frame(&mut sheet, [cols[i], r3, cols[i + 1], r2]);
        cell_text(
            &mut sheet,
            [cols[i], r3, cols[i + 1], r2],
            text,
            Align::Middle,
        );
        sheet.frame.push(line(
            vec![[cols[i + 1], r3], [cols[i + 1], bottom]],
            LineStyle::Frame,
        ));
    }
    let sheets: Vec<String> = change
        .sheets
        .iter()
        .filter_map(|s| match &s.kind {
            SheetChangeKind::Annulled { sheet, .. } => Some(sheet.to_string()),
            _ => project
                .drawings
                .get(&s.drawing)
                .map(|d| sheet_number(project, d).1.to_string()),
        })
        .collect();
    let body_top = r3 - 2.;
    cell_text(
        &mut sheet,
        [cols[0], body_top - 8., cols[1], body_top],
        &change.number.to_string(),
        Align::Middle,
    );
    let mut y = body_top;
    for chunk in sheets.chunks(2) {
        cell_text(
            &mut sheet,
            [cols[1], y - 8., cols[2], y],
            &chunk.join(", "),
            Align::Middle,
        );
        y -= 5.;
    }
    let mut content = change.description.clone();
    for s in &change.sheets {
        let what = match &s.kind {
            SheetChangeKind::Changed { areas } => format!("изменены участки: {}", areas.len()),
            SheetChangeKind::Replaced => "лист заменён".into(),
            SheetChangeKind::New => "лист выпущен вновь".into(),
            SheetChangeKind::Annulled { .. } => "лист аннулировать".into(),
        };
        let n = match &s.kind {
            SheetChangeKind::Annulled { sheet, .. } => *sheet,
            _ => project
                .drawings
                .get(&s.drawing)
                .map_or(0, |d| sheet_number(project, d).1),
        };
        content.push_str(&format!("\nЛист {n}: {what}."));
    }
    let mut y = body_top;
    for paragraph in content.lines() {
        for l in tables::wrap_text(paragraph, 98.) {
            sheet
                .labels
                .push(label([cols[2] + 1., y - 4.], l, 2.5, Align::Start));
            y -= 4.5;
        }
    }
    if let Some(r) = change.reason {
        cell_text(
            &mut sheet,
            [cols[3], body_top - 8., cols[4], body_top],
            &r.to_string(),
            Align::Middle,
        );
    }
    // Signatures: «Изм. внес», «Составил», «ГИП», «Утв.» (graphs 9–11).
    let people = [
        ("Изм. внес", change.author.as_str()),
        ("Составил", project.document.developer.name.as_str()),
        ("ГИП", project.document.approver.name.as_str()),
        ("Утв.", ""),
    ];
    for (i, (role, name)) in people.iter().enumerate() {
        let y1 = bottom - 5. * i as f64;
        let y0 = y1 - 5.;
        let date = if i == 0 { change.date.as_str() } else { "" };
        for (a, b, text, size) in [
            (x0, x0 + 20., *role, 2.5),
            (x0 + 20., x0 + 40., *name, 2.5),
            (x0 + 40., x0 + 55., "", 2.5),
            (x0 + 55., x0 + 65., date, 1.8),
        ] {
            sheet.frame.push(rect([a, y0, b, y1], LineStyle::Thin));
            cell_text_sized(&mut sheet, [a, y0, b, y1], text, Align::Start, size);
        }
    }
    // Graphs 17, 18: sheet and sheet count.
    for (a, b, caption, value) in [
        (x1 - 25., x1 - 15., "Лист", "1"),
        (x1 - 15., x1, "Листов", "1"),
    ] {
        sheet
            .frame
            .push(rect([a, MARGIN + 10., b, bottom], LineStyle::Thin));
        sheet
            .frame
            .push(rect([a, MARGIN, b, MARGIN + 10.], LineStyle::Thin));
        cell_text_sized(
            &mut sheet,
            [a, MARGIN + 10., b, bottom],
            caption,
            Align::Middle,
            1.8,
        );
        cell_text(
            &mut sheet,
            [a, MARGIN, b, MARGIN + 10.],
            value,
            Align::Middle,
        );
    }
    sheet
        .frame
        .push(rect([x0, MARGIN, x1, bottom], LineStyle::Frame));
    sheet
}

/// Year of a «ДД.ММ.ГГ» date.
fn year(date: &str) -> String {
    open_bim_model::parse_date(date)
        .map(|(_, _, y)| format!("20{y:02}"))
        .unwrap_or_default()
}

/// Object name for field 4: graphs 2 and 3 of the inscription, in capitals.
fn object_name(project: &Project) -> String {
    [
        project.document.object.as_str(),
        project.document.building.as_str(),
    ]
    .iter()
    .filter(|s| !s.trim().is_empty())
    .map(|s| s.trim().to_uppercase())
    .collect::<Vec<_>>()
    .join(". ")
}

/// Volume number of a set in the package: КМ 1, КМД 2.
pub fn volume(mark: &str) -> usize {
    SET_MARKS
        .iter()
        .position(|m| *m == mark)
        .map_or(1, |i| i + 1)
}

fn set_title(mark: &str) -> String {
    format!(
        "{}. Основной комплект рабочих чертежей",
        general_data::set_name(mark)
    )
}

/// Designation with the last change number in brackets (П.1 field 7).
fn designation_with_change(project: &Project, mark: &str) -> String {
    match set_changes(project, mark).len() {
        0 => project.document.designation(mark),
        n => format!("{} (Изм. {n})", project.document.designation(mark)),
    }
}

/// Cover, form 14 (appendix Т): fields without lines, centred.
pub fn cover(project: &Project, mark: &str, date: &str) -> Sheet {
    let mut sheet = blank(A4);
    sheet.frame.truncate(1);
    let d = &project.document;
    let cx = A4[0] / 2. + 5.;
    let mut add = |y: f64, text: String, size: f64, align: Align, x: f64| {
        if !text.trim().is_empty() {
            for (i, l) in tables::wrap_text(&text, 160. * 2.5 / size)
                .into_iter()
                .enumerate()
            {
                sheet
                    .labels
                    .push(label([x, y - i as f64 * size * 1.6], l, size, align));
            }
        }
    };
    add(270., d.organisation.clone(), 5., Align::Middle, cx);
    if !d.inventory_number.is_empty() {
        add(
            245.,
            format!("Инв. № {}", d.inventory_number),
            3.5,
            Align::End,
            190.,
        );
    }
    add(205., object_name(project), 7., Align::Middle, cx);
    add(165., "РАБОЧАЯ ДОКУМЕНТАЦИЯ".into(), 5., Align::Middle, cx);
    add(145., set_title(mark), 5., Align::Middle, cx);
    // The fact of a change is shown on the cover too (7.1.6).
    add(
        120.,
        designation_with_change(project, mark),
        7.,
        Align::Middle,
        cx,
    );
    add(105., format!("Том {}", volume(mark)), 5., Align::Middle, cx);
    add(25., year(date), 5., Align::Middle, cx);
    sheet
}

/// Title page, form 12 (appendix П): fields without lines; designation with
/// the change number in brackets; responsible persons with signature and
/// date places.
pub fn title_page(project: &Project, mark: &str, date: &str) -> Sheet {
    let mut sheet = blank(A4);
    let d = &project.document;
    let cx = (MARGIN_LEFT + A4[0] - MARGIN) / 2.;
    let mut add = |y: f64, text: String, size: f64, align: Align, x: f64| {
        if !text.trim().is_empty() {
            for (i, l) in tables::wrap_text(&text, 160. * 2.5 / size)
                .into_iter()
                .enumerate()
            {
                sheet
                    .labels
                    .push(label([x, y - i as f64 * size * 1.6], l, size, align));
            }
        }
    };
    add(275., d.organisation.clone(), 5., Align::Middle, cx);
    add(215., object_name(project), 7., Align::Middle, cx);
    add(180., "РАБОЧАЯ ДОКУМЕНТАЦИЯ".into(), 5., Align::Middle, cx);
    add(165., set_title(mark), 5., Align::Middle, cx);
    add(
        140.,
        designation_with_change(project, mark),
        7.,
        Align::Middle,
        cx,
    );
    add(125., format!("Том {}", volume(mark)), 5., Align::Middle, cx);
    let mut y = 95.;
    for p in [&d.approver, &d.developer] {
        if p.name.is_empty() {
            continue;
        }
        let position = if p.position.is_empty() {
            "Разработал".to_string()
        } else {
            p.position.clone()
        };
        add(y, position, 3.5, Align::Start, MARGIN_LEFT + 10.);
        add(y, "(подпись, дата)".into(), 2.5, Align::Middle, cx + 5.);
        add(y, p.name.clone(), 3.5, Align::End, A4[0] - MARGIN - 10.);
        y -= 15.;
    }
    add(25., year(date), 5., Align::Middle, cx);
    // Field 13: graphs 20–22 of the binding field.
    if !d.inventory_number.is_empty() {
        sheet.labels.push(Label {
            rotation: 90.,
            ..label(
                [12., 60.],
                format!("Инв. № подл. {}", d.inventory_number),
                2.5,
                Align::Start,
            )
        });
    }
    sheet
}

/// One file of the package with what the certifying sheet states.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct PackageFile {
    /// Path relative to the package root, «/» separated.
    pub path: String,
    pub designation: String,
    pub title: String,
    pub sha256: String,
    pub bytes: usize,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Package {
    pub files: Vec<PackageFile>,
    pub certifying_sheets: Vec<String>,
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Stable document GUID from the project and document designation: the same
/// document keeps it in every new version of the package (8.2.4).
pub fn document_guid(project: &Project, designation: &str) -> String {
    let h = sha256(format!("open-bim:{}:{designation}", project.name).as_bytes());
    let h = h.as_bytes();
    let s = |a: usize, b: usize| std::str::from_utf8(&h[a..b]).unwrap_or("0").to_string();
    // RFC 4122 layout, version 5-style bits from a SHA-256 name hash.
    format!(
        "{}-{}-5{}-{}{}-{}",
        s(0, 8),
        s(8, 12),
        s(13, 16),
        ["8", "9", "a", "b"][(h[16] % 4) as usize],
        s(17, 20),
        s(20, 32)
    )
}

/// Folder name of a set (8.2.5): its mark, invalid characters as «_».
fn folder(mark: &str) -> String {
    mark.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Certifying sheet, form 15 (appendix У), for the files of one set: a
/// block per file (graphs 1–9), the signatures (10–13) on the last page and
/// the designation with sheet and sheet count (14–16) on every page.
pub fn certifying_sheet(
    project: &Project,
    mark: &str,
    files: &[PackageFile],
    issued_at: &str,
) -> Vec<Sheet> {
    let d = &project.document;
    let x0 = MARGIN_LEFT;
    let top = A4[1] - MARGIN - 5.;
    let version = open_bim_drawings::set_version(project, mark);
    let last = set_changes(project, mark).len();
    let row = |sheet: &mut Sheet, y: &mut f64, h: f64, cells: &[(f64, String, bool)]| {
        let mut x = x0;
        for (cw, text, head) in cells {
            let r = [x, *y - h, x + cw, *y];
            sheet.frame.push(rect(r, LineStyle::Thin));
            cell_text(
                sheet,
                r,
                text,
                if *head { Align::Middle } else { Align::Start },
            );
            x += cw;
        }
        *y -= h;
    };
    let s = |t: &str| t.to_string();
    const BLOCK: f64 = 13. + 14. + 12. + 10. + 8. + 3.;
    const SIGNATURES: f64 = 8. * 5.;
    const FOOTER: f64 = 12.;
    let floor = MARGIN + FOOTER;
    let per_page = ((top - floor) / BLOCK).floor().max(1.) as usize;
    let mut pages: Vec<Vec<(usize, &PackageFile)>> = Vec::new();
    for (k, f) in files.iter().enumerate() {
        match pages.last_mut() {
            Some(p) if p.len() < per_page => p.push((k, f)),
            _ => pages.push(vec![(k, f)]),
        }
    }
    // The signatures go on the last page; a new page when they do not fit.
    if pages.is_empty()
        || top - floor - BLOCK * (pages.last().map_or(0, Vec::len) as f64) < SIGNATURES
    {
        pages.push(Vec::new());
    }
    let count = pages.len();
    let mut out = Vec::new();
    for (n, page) in pages.into_iter().enumerate() {
        let mut sheet = blank(A4);
        let mut y = top;
        for (k, f) in page {
            row(
                &mut sheet,
                &mut y,
                13.,
                &[
                    (45., s("Версия / номер изменения"), true),
                    (35., s("Обозначение документа"), true),
                    (70., s("Наименование документа"), true),
                    (35., s("Номер последнего изменения (версии)"), true),
                ],
            );
            row(
                &mut sheet,
                &mut y,
                14.,
                &[
                    (45., format!("{}. Версия {version}", k + 1), false),
                    (35., f.designation.clone(), false),
                    (70., f.title.clone(), false),
                    (35., format!("{last} (версия {version})"), false),
                ],
            );
            // The checksum in two halves so it fits its graph.
            let (a, b) = f.sha256.split_at(f.sha256.len() / 2);
            row(
                &mut sheet,
                &mut y,
                12.,
                &[
                    (40., s("SHA-256"), false),
                    (145., format!("{a} {b}"), false),
                ],
            );
            row(
                &mut sheet,
                &mut y,
                10.,
                &[
                    (90., s("Наименование файла"), true),
                    (50., s("Дата и время последнего изменения файла"), true),
                    (45., s("Размер файла, байт"), true),
                ],
            );
            row(
                &mut sheet,
                &mut y,
                8.,
                &[
                    (90., f.path.clone(), false),
                    (50., issued_at.to_string(), false),
                    (45., f.bytes.to_string(), false),
                ],
            );
            y -= 3.;
        }
        if n + 1 == count {
            row(
                &mut sheet,
                &mut y,
                8.,
                &[
                    (65., s("Характер работы"), true),
                    (40., s("Фамилия"), true),
                    (40., s("Подпись"), true),
                    (40., s("Дата подписания"), true),
                ],
            );
            for (role, p) in [
                ("Разработал", &d.developer),
                ("Проверил", &d.checker),
                ("Н. контроль", &d.norm_control),
                ("Утвердил", &d.approver),
            ] {
                row(
                    &mut sheet,
                    &mut y,
                    8.,
                    &[
                        (65., s(role), false),
                        (40., p.name.clone(), false),
                        (40., String::new(), false),
                        (40., p.date.clone(), false),
                    ],
                );
            }
        }
        // Graphs 14–16 on every page; 15 and 16 only for several pages,
        // the sheet count on the first.
        let (sheet_no, sheets) = if count > 1 {
            (
                format!("Лист {}", n + 1),
                if n == 0 {
                    format!("Листов {count}")
                } else {
                    "Листов".into()
                },
            )
        } else {
            ("Лист".into(), "Листов".into())
        };
        let mut yb = MARGIN + FOOTER;
        row(
            &mut sheet,
            &mut yb,
            FOOTER,
            &[
                (60., s("Удостоверяющий лист"), true),
                (85., format!("{}-УЛ", d.designation(mark)), true),
                (20., sheet_no, true),
                (20., sheets, true),
            ],
        );
        sheet
            .frame
            .push(rect([x0, MARGIN, x0 + 185., top], LineStyle::Frame));
        out.push(sheet);
    }
    out
}

fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Write the package of electronic documents into `directory` (8.2):
/// every set with issued sheets, its accompanying documents, a certifying
/// sheet per set and `info.xml`. `issued_at` is the release date and time
/// written in the certifying sheets («ДД.ММ.ГГ ЧЧ:ММ»).
pub fn export_package(
    project: &Project,
    directory: &Path,
    issued_at: &str,
) -> Result<Package, Error> {
    let date = issued_at.split_whitespace().next().unwrap_or("");
    let mut package = Package {
        files: Vec::new(),
        certifying_sheets: Vec::new(),
    };
    for mark in SET_MARKS {
        let sheets = general_data::set_drawings(project, mark);
        if sheets.is_empty() {
            continue;
        }
        if let Some(d) = sheets
            .iter()
            .find(|d| status(project, d) != Status::UpToDate)
        {
            return Err(format!(
                "Пакет не выпущен: {} — {}",
                d.number,
                status(project, d).label()
            ));
        }
        let dir = directory.join(folder(mark));
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let des = project.document.designation(mark);
        let mut docs: Vec<(String, String, String, Vec<u8>)> = Vec::new();
        let rendered = sheets
            .iter()
            .map(|d| generate(project, d))
            .collect::<Result<Vec<_>, _>>()?;
        let pdf = |s: &[Sheet], t: &str| open_bim_drawing_export::pdf::render(s, t);
        docs.push((
            format!("{des}.pdf"),
            des.clone(),
            set_title(mark),
            pdf(&rendered, &des)?,
        ));
        docs.push((
            format!("{des}-Обложка.pdf"),
            des.clone(),
            format!("Обложка. {}", set_title(mark)),
            pdf(&[cover(project, mark, date)], &des)?,
        ));
        docs.push((
            format!("{des}-ТЛ.pdf"),
            des.clone(),
            format!("Титульный лист. {}", set_title(mark)),
            pdf(&[title_page(project, mark, date)], &des)?,
        ));
        let changes = set_changes(project, mark);
        if !changes.is_empty() {
            docs.push((
                format!("{des}-ТРИ.pdf"),
                des.clone(),
                "Таблица регистрации изменений".into(),
                pdf(&[registration_table(project, mark)], &des)?,
            ));
            docs.push((
                format!("{des}-ЖИ.pdf"),
                des.clone(),
                "Журнал изменений".into(),
                pdf(&[change_journal(project, mark)], &des)?,
            ));
            for c in &changes {
                let name = format!("Разрешение-{}.pdf", folder(&c.permission));
                docs.push((
                    name,
                    c.permission.clone(),
                    format!("Разрешение на внесение изменений. Изм. {}", c.number),
                    pdf(&[permission(project, c)], &c.permission)?,
                ));
            }
        }
        let mut listed = Vec::new();
        for (name, designation, title, bytes) in docs {
            std::fs::write(dir.join(&name), &bytes).map_err(|e| e.to_string())?;
            listed.push(PackageFile {
                path: format!("{}/{name}", folder(mark)),
                designation,
                title,
                sha256: sha256(&bytes),
                bytes: bytes.len(),
            });
        }
        let ul = certifying_sheet(project, mark, &listed, issued_at);
        let ul_name = format!("{des}-УЛ.pdf");
        let ul_bytes = pdf(&ul, &format!("{des}-УЛ"))?;
        std::fs::write(dir.join(&ul_name), &ul_bytes).map_err(|e| e.to_string())?;
        package
            .certifying_sheets
            .push(format!("{}/{ul_name}", folder(mark)));
        listed.push(PackageFile {
            path: format!("{}/{ul_name}", folder(mark)),
            designation: format!("{des}-УЛ"),
            title: "Удостоверяющий лист".into(),
            sha256: sha256(&ul_bytes),
            bytes: ul_bytes.len(),
        });
        package.files.extend(listed);
    }
    if package.files.is_empty() {
        return Err("Нет комплектов для выпуска".into());
    }
    // Requisite part (8.2.3): package, documents with GUID, version, files.
    let mut info = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    info.push_str(&format!(
        "<package name=\"{}\" designation=\"{}\" issued=\"{}\">\n",
        xml(&project.name),
        xml(project.document.base_designation.trim()),
        xml(issued_at)
    ));
    for mark in SET_MARKS {
        let files: Vec<&PackageFile> = package
            .files
            .iter()
            .filter(|f| f.path.starts_with(&format!("{}/", folder(mark))))
            .collect();
        if files.is_empty() {
            continue;
        }
        let des = project.document.designation(mark);
        info.push_str(&format!(
            "  <folder name=\"{}\" title=\"{}\">\n",
            xml(&folder(mark)),
            xml(&set_title(mark))
        ));
        for f in files {
            info.push_str(&format!(
                "    <document guid=\"{}\" designation=\"{}\" title=\"{}\" version=\"{}\">\n      <file path=\"{}\" size=\"{}\" sha256=\"{}\"/>\n    </document>\n",
                document_guid(project, &format!("{des}:{}", f.title)),
                xml(&f.designation),
                xml(&f.title),
                open_bim_drawings::set_version(project, mark),
                xml(&f.path),
                f.bytes,
                f.sha256
            ));
        }
        info.push_str("  </folder>\n");
    }
    info.push_str("</package>\n");
    std::fs::write(directory.join("info.xml"), info).map_err(|e| e.to_string())?;
    Ok(package)
}

/// Annul a sheet of an issued set: the drawing is removed, its number and
/// title stay in the change (7.3.9, 7.4.4).
pub fn annul_sheet(
    project: &Project,
    drawing: ObjectId,
    request: &ChangeRequest,
) -> Result<Vec<Command>, Error> {
    let d = project
        .drawings
        .get(&drawing)
        .ok_or_else(|| format!("Чертёж #{} не найден", drawing.0))?;
    if d.issued_revision.is_none() {
        return Err("Невыданный лист удаляют без аннулирования".into());
    }
    let mark = set_mark(&d.kind);
    let (_, no, _) = sheet_number(project, d);
    let mut staged = project.clone();
    staged.drawings.remove(&drawing);
    let general = general_data::set_drawings(&staged, mark)
        .into_iter()
        .find(|g| matches!(g.kind, DrawingKind::GeneralData { .. }))
        .cloned();
    let mut sheets = vec![SheetChange {
        drawing,
        kind: SheetChangeKind::Annulled {
            sheet: no,
            title: d.title.clone(),
        },
    }];
    let mut drawings = Vec::new();
    if let Some(g) = general {
        let mut next = updated(&staged, &g);
        next.issued_revision = Some(project.revision);
        sheets.insert(
            0,
            SheetChange {
                drawing: g.id,
                kind: SheetChangeKind::Replaced,
            },
        );
        drawings.push(next);
    }
    let number = project.changes.iter().filter(|c| c.mark == mark).count() as u32 + 1;
    Ok(vec![
        Command::Delete {
            id: drawing,
            policy: open_bim_commands::DeletePolicy::Cascade,
        },
        Command::RecordChange {
            change: Box::new(Change {
                mark: mark.into(),
                number,
                permission: request.permission.trim().into(),
                date: request.date.trim().into(),
                description: request.description.trim().into(),
                reason: request.reason,
                author: request.author.trim().into(),
                sheets,
            }),
            drawings,
        },
    ])
}
