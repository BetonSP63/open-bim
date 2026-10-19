//! Main inscriptions, additional graphs and frames of ГОСТ Р 21.101-2026
//! (appendices Е and Ж): forms 3–6 with their column grid, graphs 1–27
//! filled from the project's document attributes, the binding-field graphs,
//! the sheet frame and the QR code above the inscription (5.2.9).
use crate::font::Font;
use crate::{Align, Label, LineStyle, Polyline, qr};
use open_bim_model::DocumentInfo;

/// Main inscription forms of appendix Е.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    /// Sheets of working-drawing sets (КМ, КМД), first sheets of text
    /// documents issued without a title page; 185×55.
    F3,
    /// First sheet of a building-product drawing; 185×55.
    F4,
    /// Title sheet of text documents, first sheet of a non-standard product
    /// sketch; 185×40.
    F5,
    /// Later sheets of product drawings and text documents; 185×15.
    F6,
}

impl Form {
    pub fn size(self) -> [f64; 2] {
        match self {
            Self::F3 | Self::F4 => [185., 55.],
            Self::F5 => [185., 40.],
            Self::F6 => [185., 15.],
        }
    }
}

/// What kind of document a sheet belongs to (selects the form, 5.2.2–5.2.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentClass {
    /// Main set of working drawings: form 3 on every sheet.
    WorkingSet,
    /// Building-product drawing: form 4, then 6.
    ProductDrawing,
    /// Text document with a title page: form 5, then 6.
    TextDocument,
    /// Text document issued without a title page (e.g. a specification):
    /// form 3, then 6.
    TextWithoutTitle,
}

pub fn form_for(class: DocumentClass, sheet_no: usize) -> Form {
    let first = sheet_no <= 1;
    match class {
        DocumentClass::WorkingSet => Form::F3,
        DocumentClass::ProductDrawing if first => Form::F4,
        DocumentClass::TextDocument if first => Form::F5,
        DocumentClass::TextWithoutTitle if first => Form::F3,
        _ => Form::F6,
    }
}

/// Font heights of ГОСТ 2.304 used on sheets, mm (1.8 only where a value
/// does not fit a narrow graph at 2.5, e.g. a date in graph 13).
pub const FONT_SIZES: [f64; 8] = [1.8, 2.5, 3.5, 5., 7., 10., 14., 20.];

/// Inner frame offsets [left, bottom, right, top] (appendix Ж): 20 mm
/// binding field on the left, 5 mm elsewhere; an A4 sheet laid landscape has
/// the binding field at the top (Ж.1 в).
pub fn margins(size: [f64; 2]) -> [f64; 4] {
    if (size[0] - 297.).abs() < 0.01 && (size[1] - 210.).abs() < 0.01 {
        [5., 5., 5., 20.]
    } else {
        [20., 5., 5., 5.]
    }
}

/// What one sheet's inscription shows.
pub struct SheetInfo<'a> {
    pub info: &'a DocumentInfo,
    pub size: [f64; 2],
    pub form: Form,
    /// Graph 1.
    pub designation: String,
    /// Graph 4 (form 3) or 5 (forms 4, 5): names of the images / product.
    pub title: String,
    pub sheet_no: usize,
    pub sheets: usize,
    /// Graph 26: «A3», «A3×3».
    pub format: String,
    /// Graph 23 (form 4, part drawings).
    pub material: Option<String>,
    /// Graph 24 (form 4), kg.
    pub mass_kg: Option<f64>,
    /// Graph 25 (form 4), denominator.
    pub scale: Option<f64>,
    /// Document version for the QR code.
    pub version: u32,
}

/// A numbered graph and its rectangle on the sheet [x0, y0, x1, y1].
#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub graph: u8,
    pub rect: [f64; 4],
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TitleBlock {
    pub lines: Vec<Polyline>,
    pub labels: Vec<Label>,
    /// Filled black rectangles (QR modules).
    pub fills: Vec<[f64; 4]>,
    pub cells: Vec<Cell>,
    /// The inscription rectangle (without graph 27 and the QR code).
    pub block: [f64; 4],
    /// Area the views must keep free: inscription, graph 27 and QR code.
    pub reserved: [f64; 4],
}

/// Text placed in a graph.
struct Text {
    graph: u8,
    text: String,
    /// Preferred height; reduced along FONT_SIZES to fit.
    height: f64,
    align: Align,
}

/// Lines of a form in block coordinates (origin at the block's lower-left
/// corner): (x0, y0, x1, y1, thick).
fn grid(form: Form) -> Vec<(f64, f64, f64, f64, bool)> {
    let [w, h] = form.size();
    let mut out = vec![
        (0., 0., w, 0., true),
        (w, 0., w, h, true),
        (w, h, 0., h, true),
        (0., h, 0., 0., true),
    ];
    let h_line =
        |y: f64, x0: f64, x1: f64, thick: bool, out: &mut Vec<_>| out.push((x0, y, x1, y, thick));
    match form {
        Form::F3 | Form::F4 | Form::F5 => {
            // Left part: 5 mm rows; the header row («Изм.») has a thick
            // bottom line; the change table has its own columns.
            let header = if form == Form::F5 { 30. } else { 40. };
            let mut y = 5.;
            while y < h - 0.01 {
                h_line(y, 0., 65., (y - header).abs() < 0.01, &mut out);
                y += 5.;
            }
            for x in [10., 30.] {
                out.push((x, header, x, h, true));
            }
            for x in [20., 40., 55., 65.] {
                out.push((x, 0., x, h, true));
            }
        }
        Form::F6 => {
            for y in [5., 10.] {
                h_line(y, 0., 65., y == 5., &mut out);
            }
            for x in [10., 20., 30., 40., 55., 65., 175.] {
                out.push((x, 0., x, h, true));
            }
            h_line(8., 175., 185., false, &mut out);
        }
    }
    match form {
        Form::F3 => {
            h_line(45., 65., 185., true, &mut out);
            h_line(30., 65., 185., true, &mut out);
            h_line(25., 135., 185., false, &mut out);
            h_line(15., 65., 185., true, &mut out);
            out.push((135., 0., 135., 30., true));
            for x in [150., 165.] {
                out.push((x, 15., x, 30., true));
            }
        }
        Form::F4 => {
            h_line(40., 65., 185., true, &mut out);
            h_line(35., 135., 185., false, &mut out);
            h_line(20., 135., 185., true, &mut out);
            h_line(15., 65., 185., true, &mut out);
            out.push((135., 0., 135., 40., true));
            for x in [150., 165.] {
                out.push((x, 20., x, 40., true));
            }
            out.push((155., 15., 155., 20., true));
        }
        Form::F5 => {
            h_line(25., 65., 185., true, &mut out);
            h_line(20., 135., 185., false, &mut out);
            h_line(15., 135., 185., true, &mut out);
            out.push((135., 0., 135., 25., true));
            for x in [150., 165.] {
                out.push((x, 15., x, 25., true));
            }
        }
        Form::F6 => {}
    }
    out
}

/// Graph rectangles of a form in block coordinates.
fn graphs(form: Form) -> Vec<(u8, [f64; 4])> {
    let mut out = Vec::new();
    let cols = [
        (0., 10.),
        (10., 20.),
        (20., 30.),
        (30., 40.),
        (40., 55.),
        (55., 65.),
    ];
    // Change table rows (14–19) and the header row.
    let (change_rows, header): (Vec<f64>, f64) = match form {
        Form::F3 | Form::F4 => (vec![45., 50.], 40.),
        Form::F5 => (vec![35.], 30.),
        Form::F6 => (vec![5., 10.], 0.),
    };
    for (i, (x0, x1)) in cols.iter().enumerate() {
        out.push((
            14 + i as u8,
            [*x0, change_rows[0], *x1, change_rows[0] + 5.],
        ));
    }
    if form != Form::F6 {
        let rows = if form == Form::F5 { 6 } else { 8 };
        for r in 0..rows {
            let top = header - 5. * r as f64;
            for (g, x0, x1) in [
                (10, 0., 20.),
                (11, 20., 40.),
                (12, 40., 55.),
                (13, 55., 65.),
            ] {
                out.push((g, [x0, top - 5., x1, top]));
            }
        }
    }
    match form {
        Form::F3 => out.extend([
            (1, [65., 45., 185., 55.]),
            (2, [65., 30., 185., 45.]),
            (3, [65., 15., 135., 30.]),
            (6, [135., 15., 150., 25.]),
            (7, [150., 15., 165., 25.]),
            (8, [165., 15., 185., 25.]),
            (4, [65., 0., 135., 15.]),
            (9, [135., 0., 185., 15.]),
        ]),
        Form::F4 => out.extend([
            (1, [65., 40., 185., 55.]),
            (5, [65., 15., 135., 40.]),
            (6, [135., 20., 150., 35.]),
            (24, [150., 20., 165., 35.]),
            (25, [165., 20., 185., 35.]),
            (7, [135., 15., 155., 20.]),
            (8, [155., 15., 185., 20.]),
            (23, [65., 0., 135., 15.]),
            (9, [135., 0., 185., 15.]),
        ]),
        Form::F5 => out.extend([
            (1, [65., 25., 185., 40.]),
            (5, [65., 0., 135., 25.]),
            (6, [135., 15., 150., 20.]),
            (7, [150., 15., 165., 20.]),
            (8, [165., 15., 185., 20.]),
            (9, [135., 0., 185., 15.]),
        ]),
        Form::F6 => out.extend([(1, [65., 0., 175., 15.]), (7, [175., 0., 185., 8.])]),
    }
    out
}

/// Fixed captions of a form: (text, rect) in block coordinates.
fn captions(form: Form, info: &DocumentInfo) -> Vec<(String, [f64; 4])> {
    let cols = [
        ("Изм.", 0., 10.),
        ("Кол.уч", 10., 20.),
        ("Лист", 20., 30.),
        ("№ док.", 30., 40.),
        ("Подп.", 40., 55.),
        ("Дата", 55., 65.),
    ];
    let header = match form {
        Form::F3 | Form::F4 => 40.,
        Form::F5 => 30.,
        Form::F6 => 0.,
    };
    let mut out: Vec<(String, [f64; 4])> = cols
        .iter()
        .map(|(t, x0, x1)| (t.to_string(), [*x0, header, *x1, header + 5.]))
        .collect();
    if form != Form::F6 {
        let rows = if form == Form::F5 { 6 } else { 8 };
        for (r, (position, _)) in info.signature_rows(rows).iter().enumerate() {
            let top = header - 5. * r as f64;
            out.push((position.clone(), [0., top - 5., 20., top]));
        }
    }
    let heads: &[(&str, [f64; 4])] = match form {
        Form::F3 => &[
            ("Стадия", [135., 25., 150., 30.]),
            ("Лист", [150., 25., 165., 30.]),
            ("Листов", [165., 25., 185., 30.]),
        ],
        Form::F4 => &[
            ("Стадия", [135., 35., 150., 40.]),
            ("Масса", [150., 35., 165., 40.]),
            ("Масштаб", [165., 35., 185., 40.]),
        ],
        Form::F5 => &[
            ("Стадия", [135., 20., 150., 25.]),
            ("Лист", [150., 20., 165., 25.]),
            ("Листов", [165., 20., 185., 25.]),
        ],
        Form::F6 => &[("Лист", [175., 8., 185., 15.])],
    };
    out.extend(heads.iter().map(|(t, r)| (t.to_string(), *r)));
    out
}

/// Fit `text` into a cell: the largest ГОСТ 2.304 height not above
/// `preferred` whose lines (wrapped on spaces) fit the cell; 2.5 mm text is
/// cut with «…» as a last resort.
fn fit(font: &Font, text: &str, preferred: f64, w: f64, h: f64) -> (Vec<String>, f64) {
    let pad = 0.5;
    let avail = w - 2. * pad;
    for &size in FONT_SIZES.iter().rev().filter(|s| **s <= preferred + 1e-9) {
        let max_lines = ((h - pad) / (size * 1.45)).floor().max(1.) as usize;
        let mut lines: Vec<String> = Vec::new();
        let mut current = String::new();
        for word in text.split_whitespace() {
            let candidate = if current.is_empty() {
                word.to_string()
            } else {
                format!("{current} {word}")
            };
            if font.width(&candidate, size) <= avail {
                current = candidate;
            } else {
                if !current.is_empty() {
                    lines.push(std::mem::take(&mut current));
                }
                current = word.to_string();
            }
        }
        if !current.is_empty() {
            lines.push(current);
        }
        if lines.len() <= max_lines && lines.iter().all(|l| font.width(l, size) <= avail) {
            return (lines, size);
        }
    }
    let mut cut: String = text.to_string();
    while !cut.is_empty() && font.width(&format!("{cut}…"), 1.8) > avail {
        cut.pop();
    }
    (vec![format!("{cut}…")], 1.8)
}

fn label(position: [f64; 2], text: String, height: f64, align: Align, rotation: f64) -> Label {
    Label {
        position,
        text,
        height,
        align,
        rotation,
        object: None,
        lost: false,
    }
}

/// Put fitted text in a rectangle (sheet coordinates); `rotation` 90 turns
/// it to read upwards (binding field).
fn place(
    font: &Font,
    labels: &mut Vec<Label>,
    rect: [f64; 4],
    text: &str,
    preferred: f64,
    align: Align,
    rotation: f64,
) {
    if text.trim().is_empty() {
        return;
    }
    let vertical = rotation != 0.;
    let (w, h) = if vertical {
        (rect[3] - rect[1], rect[2] - rect[0])
    } else {
        (rect[2] - rect[0], rect[3] - rect[1])
    };
    let (lines, size) = fit(font, text, preferred, w, h);
    let step = size * 1.45;
    let total = step * (lines.len() as f64 - 1.);
    for (i, line) in lines.iter().enumerate() {
        // Baseline offset from the cell's centre line across the text.
        let across = -0.35 * size + total / 2. - step * i as f64;
        let along = match align {
            Align::Start => 1.,
            Align::Middle => w / 2.,
            Align::End => w - 1.,
        };
        let position = if vertical {
            [rect[0] + h / 2. - across, rect[1] + along]
        } else {
            [rect[0] + along, rect[1] + h / 2. + across]
        };
        labels.push(label(position, line.clone(), size, align, rotation));
    }
}

fn mass_text(kg: f64) -> String {
    let s = if kg >= 10. {
        format!("{kg:.1}")
    } else {
        format!("{kg:.2}")
    };
    s.replace('.', ",")
}

/// Scale denominator as written in graph 25: «1:10», «1:2,5».
pub fn scale_text(denominator: f64) -> String {
    let d = if denominator.fract().abs() < 1e-9 {
        format!("{}", denominator as i64)
    } else {
        format!("{denominator}").replace('.', ",")
    };
    format!("1:{d}")
}

/// QR contents (5.2.9): designation, version, format.
pub fn qr_text(designation: &str, version: u32, format: &str) -> String {
    format!("{designation};{version};{format}")
}

pub const QR_MODULE: f64 = 0.5;

/// Build the frame, inscription, binding-field graphs and QR code of a sheet.
pub fn build(sheet: &SheetInfo) -> Result<TitleBlock, String> {
    let font = Font::load()?;
    let [width, height] = sheet.size;
    let [ml, mb, mr, mt] = margins(sheet.size);
    let top_binding = mt > mr;
    let [bw, bh] = sheet.form.size();
    let (x0, y0) = (width - mr - bw, mb);
    let at = |x: f64, y: f64| [x0 + x, y0 + y];
    let mut tb = TitleBlock {
        block: [x0, y0, x0 + bw, y0 + bh],
        ..Default::default()
    };
    let line = |a: [f64; 2], b: [f64; 2], style| Polyline {
        points: vec![a, b],
        style,
        object: None,
    };
    let rect = |r: [f64; 4], style| Polyline {
        points: vec![
            [r[0], r[1]],
            [r[2], r[1]],
            [r[2], r[3]],
            [r[0], r[3]],
            [r[0], r[1]],
        ],
        style,
        object: None,
    };
    // Outer sheet edge (thin) and inner frame (thick, 5.2.5).
    tb.lines
        .push(rect([0., 0., width, height], LineStyle::Thin));
    tb.lines
        .push(rect([ml, mb, width - mr, height - mt], LineStyle::Frame));
    for (a0, b0, a1, b1, thick) in grid(sheet.form) {
        let style = if thick {
            LineStyle::Frame
        } else {
            LineStyle::Thin
        };
        tb.lines.push(line(at(a0, b0), at(a1, b1), style));
    }
    for (graph, r) in graphs(sheet.form) {
        let a = at(r[0], r[1]);
        let b = at(r[2], r[3]);
        tb.cells.push(Cell {
            graph,
            rect: [a[0], a[1], b[0], b[1]],
        });
    }
    let cell = |graph: u8| -> Option<[f64; 4]> {
        tb.cells.iter().find(|c| c.graph == graph).map(|c| c.rect)
    };
    let mut labels = Vec::new();
    for (text, r) in captions(sheet.form, sheet.info) {
        let a = at(r[0], r[1]);
        let b = at(r[2], r[3]);
        place(
            &font,
            &mut labels,
            [a[0], a[1], b[0], b[1]],
            &text,
            2.5,
            Align::Start,
            0.,
        );
    }
    let info = sheet.info;
    let mut texts = vec![
        Text {
            graph: 1,
            text: sheet.designation.clone(),
            height: 5.,
            align: Align::Middle,
        },
        Text {
            graph: 9,
            text: info.organisation.clone(),
            height: 3.5,
            align: Align::Middle,
        },
    ];
    match sheet.form {
        Form::F3 => texts.extend([
            Text {
                graph: 2,
                text: info.object.clone(),
                height: 3.5,
                align: Align::Middle,
            },
            Text {
                graph: 3,
                text: info.building.clone(),
                height: 3.5,
                align: Align::Middle,
            },
            Text {
                graph: 4,
                text: sheet.title.clone(),
                height: 3.5,
                align: Align::Middle,
            },
        ]),
        Form::F4 | Form::F5 => texts.push(Text {
            graph: 5,
            text: sheet.title.clone(),
            height: 3.5,
            align: Align::Middle,
        }),
        Form::F6 => {}
    }
    if sheet.form != Form::F6 {
        texts.push(Text {
            graph: 6,
            text: info.stage.code().into(),
            height: 3.5,
            align: Align::Middle,
        });
    }
    // Graph 7: sheet number, empty for a one-sheet document; graph 8: sheet
    // count on the first sheet only. Form 4 writes the captions in the same
    // 5 mm row («Лист 1», «Листов 2»).
    let inline = |caption: &str, value: String| {
        if sheet.form == Form::F4 {
            format!("{caption} {value}").trim().to_string()
        } else {
            value
        }
    };
    let number = if sheet.sheets > 1 {
        sheet.sheet_no.to_string()
    } else {
        String::new()
    };
    texts.push(Text {
        graph: 7,
        text: inline("Лист", number),
        height: 3.5,
        align: Align::Middle,
    });
    if sheet.form != Form::F6 {
        let count = if sheet.sheet_no <= 1 {
            sheet.sheets.to_string()
        } else {
            String::new()
        };
        texts.push(Text {
            graph: 8,
            text: inline("Листов", count),
            height: 3.5,
            align: Align::Middle,
        });
    }
    if sheet.form == Form::F4 {
        if let Some(m) = &sheet.material {
            texts.push(Text {
                graph: 23,
                text: m.clone(),
                height: 3.5,
                align: Align::Middle,
            });
        }
        if let Some(kg) = sheet.mass_kg {
            texts.push(Text {
                graph: 24,
                text: mass_text(kg),
                height: 3.5,
                align: Align::Middle,
            });
        }
        if let Some(s) = sheet.scale {
            texts.push(Text {
                graph: 25,
                text: scale_text(s),
                height: 3.5,
                align: Align::Middle,
            });
        }
    }
    for t in &texts {
        if let Some(r) = cell(t.graph) {
            place(&font, &mut labels, r, &t.text, t.height, t.align, 0.);
        }
    }
    // Graphs 10–13: names and dates of the signing people.
    if sheet.form != Form::F6 {
        let rows = if sheet.form == Form::F5 { 6 } else { 8 };
        let name_cells: Vec<[f64; 4]> = tb
            .cells
            .iter()
            .filter(|c| c.graph == 11)
            .map(|c| c.rect)
            .collect();
        let date_cells: Vec<[f64; 4]> = tb
            .cells
            .iter()
            .filter(|c| c.graph == 13)
            .map(|c| c.rect)
            .collect();
        for (i, (_, person)) in info.signature_rows(rows).iter().enumerate() {
            place(
                &font,
                &mut labels,
                name_cells[i],
                &person.name,
                2.5,
                Align::Start,
                0.,
            );
            place(
                &font,
                &mut labels,
                date_cells[i],
                &person.date,
                2.5,
                Align::Start,
                0.,
            );
        }
    }
    // Graph 27 (customer) above the inscription, only when set.
    let mut top = y0 + bh;
    if !info.customer.trim().is_empty() && sheet.form != Form::F6 {
        let r = [x0 + 65., top, x0 + bw, top + 10.];
        tb.lines.push(rect(r, LineStyle::Frame));
        tb.cells.push(Cell { graph: 27, rect: r });
        place(
            &font,
            &mut labels,
            r,
            &info.customer,
            3.5,
            Align::Middle,
            0.,
        );
        top += 10.;
    }
    // Graph 26 under the inner frame, right part.
    let r26 = [width - mr - 50., 0., width - mr, mb];
    tb.cells.push(Cell {
        graph: 26,
        rect: r26,
    });
    labels.push(label(
        [r26[0] + 1., 1.2],
        format!("Формат {}", sheet.format),
        2.5,
        Align::Start,
        0.,
    ));
    // Binding-field graphs 20–22 and «Согласовано» (10–13): measured along
    // the binding edge from the inscription corner (u) and outwards from the
    // inner frame (v).
    let map = |u: f64, v: f64| -> [f64; 2] {
        if top_binding {
            [ml + u, height - mt + v]
        } else {
            [ml - v, mb + u]
        }
    };
    let area = |u0: f64, u1: f64, v0: f64, v1: f64| -> [f64; 4] {
        let (a, b) = (map(u0, v0), map(u1, v1));
        [
            a[0].min(b[0]),
            a[1].min(b[1]),
            a[0].max(b[0]),
            a[1].max(b[1]),
        ]
    };
    let rotation = if top_binding { 0. } else { 90. };
    let bind_cell = |graph: u8,
                     caption: &str,
                     value: &str,
                     u0: f64,
                     u1: f64,
                     tb: &mut TitleBlock,
                     labels: &mut Vec<Label>| {
        let cap = area(u0, u1, 7., 12.);
        let val = area(u0, u1, 0., 7.);
        tb.lines.push(rect(cap, LineStyle::Frame));
        tb.lines.push(rect(val, LineStyle::Frame));
        tb.cells.push(Cell { graph, rect: val });
        place(&font, labels, cap, caption, 2.5, Align::Middle, rotation);
        place(&font, labels, val, value, 2.5, Align::Middle, rotation);
    };
    bind_cell(
        20,
        "Инв. № подл.",
        &info.inventory_number,
        0.,
        25.,
        &mut tb,
        &mut labels,
    );
    bind_cell(21, "Подп. и дата", "", 25., 60., &mut tb, &mut labels);
    bind_cell(
        22,
        "Взам. инв. №",
        &info.replaced_inventory,
        60.,
        85.,
        &mut tb,
        &mut labels,
    );
    if sheet.sheet_no <= 1 && !info.approvals.is_empty() {
        // Two approvals per 65 mm block, continued further along the edge.
        for (k, pair) in info.approvals.chunks(2).enumerate() {
            let u = 85. + 65. * k as f64;
            let caption = area(u, u + 65., 10., 15.);
            tb.lines.push(rect(caption, LineStyle::Frame));
            place(
                &font,
                &mut labels,
                caption,
                "Согласовано",
                2.5,
                Align::Middle,
                rotation,
            );
            for (j, p) in pair.iter().enumerate() {
                let (v0, v1) = (5. - 5. * j as f64, 10. - 5. * j as f64);
                let mut u0 = u;
                for (graph, len, text) in [
                    (10u8, 20., p.position.as_str()),
                    (11, 20., p.name.as_str()),
                    (12, 15., ""),
                    (13, 10., p.date.as_str()),
                ] {
                    let r = area(u0, u0 + len, v0, v1);
                    tb.lines.push(rect(r, LineStyle::Thin));
                    tb.cells.push(Cell { graph, rect: r });
                    place(&font, &mut labels, r, text, 2.5, Align::Start, rotation);
                    u0 += len;
                }
            }
        }
    }
    // QR code above the inscription at the right (5.2.9), with a quiet zone.
    let code = qr::encode(&qr_text(&sheet.designation, sheet.version, &sheet.format))?;
    let side = code.size as f64 * QR_MODULE;
    let quiet = 4. * QR_MODULE;
    let (qx, qy) = (width - mr - quiet - side, top + quiet);
    for y in 0..code.size {
        for x in 0..code.size {
            if code.dark(x, y) {
                let px = qx + x as f64 * QR_MODULE;
                let py = qy + side - (y + 1) as f64 * QR_MODULE;
                tb.fills.push([px, py, px + QR_MODULE, py + QR_MODULE]);
            }
        }
    }
    tb.reserved = [x0, y0, x0 + bw, qy + side + quiet];
    tb.labels = labels;
    Ok(tb)
}
