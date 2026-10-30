//! Tables drawn on sheets: sheet lists of «Общие данные» (ГОСТ Р 21.101-2026,
//! appendix Б, forms 1 and 2), specifications (appendix И, forms 7 and 8) and
//! the rolled-steel specification of КМ sets (ГОСТ 21.502). Column widths are
//! the dimensioned widths of the forms; rows are 8 mm, taller when a cell
//! wraps; texts use font 2.5 (ГОСТ 2.304).
use crate::conventions::em;
use crate::font::Font;
use crate::{Align, Error, Label, LineStyle, Polyline};
use open_bim_model::{DetailKind, ElementShape, MemberKind, ObjectId, ProfileShape, Project};
use std::collections::{BTreeMap, BTreeSet};

/// Row height of the forms («не менее 8»).
pub const ROW: f64 = 8.;
/// Header height of forms 1, 2, 7 and 8.
pub const HEADER: f64 = 15.;
/// Cell text: font 2.5.
pub const TEXT: f64 = em(2.5);
/// Table titles above the tables: font 3.5.
pub const TITLE: f64 = em(3.5);
/// Distance from a title's baseline to the table top.
pub const TITLE_GAP: f64 = 4.;
/// Line pitch of wrapped cell text.
const PITCH: f64 = 4.;

#[derive(Clone, Debug, PartialEq)]
pub struct Column {
    /// Header text; «\n» breaks lines.
    pub title: String,
    pub width: f64,
    pub align: Align,
}

/// Header cell over columns `from..to` with the column titles below it
/// (form 8 «Кол.», the mass columns of ГОСТ 21.502).
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub from: usize,
    pub to: usize,
    pub title: String,
    pub height: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    Cells(Vec<String>),
    /// Section heading in the name column, underlined (И.3, 4.3.3).
    Heading(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    pub title: Option<String>,
    pub columns: Vec<Column>,
    pub groups: Vec<Group>,
    pub header: f64,
    /// Column of section headings.
    pub name_column: usize,
    pub rows: Vec<Row>,
}

/// Lines and texts of a drawn table; `size` includes the title.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Drawn {
    pub lines: Vec<Polyline>,
    pub labels: Vec<Label>,
    pub size: [f64; 2],
}

fn width(font: Option<&Font>, text: &str, height: f64) -> f64 {
    font.map_or(text.chars().count() as f64 * height * 0.55, |f| {
        f.width(text, height)
    })
}

/// Lines of font-2.5 cell text no wider than `room` millimetres.
pub fn wrap_text(text: &str, room: f64) -> Vec<String> {
    let font = Font::load().ok();
    wrap(font.as_ref(), text, room)
}

/// Greedy word wrap into lines no wider than `room`.
fn wrap(font: Option<&Font>, text: &str, room: f64) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut current = String::new();
        for word in paragraph.split_whitespace() {
            let candidate = if current.is_empty() {
                word.to_string()
            } else {
                format!("{current} {word}")
            };
            if !current.is_empty() && width(font, &candidate, TEXT) > room {
                lines.push(std::mem::replace(&mut current, word.to_string()));
            } else {
                current = candidate;
            }
        }
        lines.push(current);
    }
    lines
}

fn line(points: Vec<[f64; 2]>, style: LineStyle) -> Polyline {
    Polyline {
        points,
        style,
        object: None,
    }
}

fn label(position: [f64; 2], text: String, height: f64, align: Align) -> Label {
    Label {
        position,
        text,
        height,
        align,
        rotation: 0.,
        object: None,
        lost: false,
    }
}

/// Baselines of `n` lines of font-2.5 text centred in a cell [bottom, top].
fn baselines(n: usize, bottom: f64, top: f64) -> Vec<f64> {
    let cap = TEXT * crate::conventions::CAP_RATIO;
    let block = (n.max(1) - 1) as f64 * PITCH + cap;
    let first = (bottom + top) / 2. + block / 2. - cap;
    (0..n).map(|k| first - k as f64 * PITCH).collect()
}

impl Table {
    pub fn width(&self) -> f64 {
        self.columns.iter().map(|c| c.width).sum()
    }

    fn lines_of(&self, font: Option<&Font>) -> Vec<Vec<Vec<String>>> {
        self.rows
            .iter()
            .map(|row| match row {
                Row::Cells(cells) => self
                    .columns
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        let text = cells.get(i).map_or("", String::as_str);
                        if text.is_empty() {
                            Vec::new()
                        } else {
                            wrap(font, text, c.width - 2.)
                        }
                    })
                    .collect(),
                Row::Heading(text) => {
                    let room = self.columns[self.name_column].width - 2.;
                    vec![wrap(font, text, room)]
                }
            })
            .collect()
    }

    fn row_heights(&self, font: Option<&Font>) -> Vec<f64> {
        self.lines_of(font)
            .iter()
            .map(|cells| {
                let n = cells.iter().map(Vec::len).max().unwrap_or(1);
                ROW.max(n as f64 * PITCH + 2.)
            })
            .collect()
    }

    /// Header lines of each column and group, wrapped to their widths.
    fn header_lines(&self, font: Option<&Font>) -> (Vec<Vec<String>>, Vec<Vec<String>>) {
        let columns = self
            .columns
            .iter()
            .map(|c| {
                if c.title.is_empty() {
                    Vec::new()
                } else {
                    wrap(font, &c.title, c.width - 1.)
                }
            })
            .collect();
        let groups = self
            .groups
            .iter()
            .map(|g| {
                let w: f64 = self.columns[g.from..g.to].iter().map(|c| c.width).sum();
                wrap(font, &g.title, w - 1.)
            })
            .collect();
        (columns, groups)
    }

    /// Header height and the height of each group cell: the form's height,
    /// more when wrapped titles need it.
    fn header_heights(&self, font: Option<&Font>) -> (f64, Vec<f64>) {
        if self.header == 0. {
            return (0., Vec::new());
        }
        let need = |n: usize| n as f64 * PITCH + 2.;
        let (columns, groups) = self.header_lines(font);
        let group_heights: Vec<f64> = self
            .groups
            .iter()
            .zip(&groups)
            .map(|(g, lines)| g.height.max(need(lines.len())))
            .collect();
        let mut header = self.header;
        for (i, lines) in columns.iter().enumerate() {
            let above = self
                .groups
                .iter()
                .zip(&group_heights)
                .find(|(g, _)| (g.from..g.to).contains(&i))
                .map_or(0., |(_, h)| *h);
            header = header.max(above + need(lines.len()));
        }
        (header, group_heights)
    }

    fn title_height(&self) -> f64 {
        if self.title.is_some() {
            TITLE * crate::conventions::CAP_RATIO + TITLE_GAP
        } else {
            0.
        }
    }

    /// Height including the title.
    pub fn height(&self) -> f64 {
        let font = Font::load().ok();
        self.title_height()
            + self.header_heights(font.as_ref()).0
            + self.row_heights(font.as_ref()).iter().sum::<f64>()
    }

    /// Draw with the top-left corner of the title (or table) at `top_left`.
    pub fn draw(&self, top_left: [f64; 2]) -> Drawn {
        let font = Font::load().ok();
        let font = font.as_ref();
        let mut out = Drawn::default();
        let w = self.width();
        let x0 = top_left[0];
        let mut top = top_left[1];
        if let Some(title) = &self.title {
            top -= TITLE * crate::conventions::CAP_RATIO;
            out.labels.push(label(
                [x0 + w / 2., top],
                title.clone(),
                TITLE,
                Align::Middle,
            ));
            top -= TITLE_GAP;
        }
        let edges: Vec<f64> = std::iter::once(x0)
            .chain(self.columns.iter().scan(x0, |x, c| {
                *x += c.width;
                Some(*x)
            }))
            .collect();
        let heights = self.row_heights(font);
        let (header, group_heights) = self.header_heights(font);
        let (column_titles, group_titles) = self.header_lines(font);
        let bottom = top - header - heights.iter().sum::<f64>();
        let head = top - header;
        let group_of = |i: usize| {
            self.groups
                .iter()
                .zip(&group_heights)
                .find(|(g, _)| (g.from..g.to).contains(&i))
                .map(|(_, h)| *h)
        };
        // Header texts; grouped columns sit under their group cell.
        for (i, parts) in column_titles.iter().enumerate() {
            let cell_top = top - group_of(i).unwrap_or(0.);
            for (text, y) in parts.iter().zip(baselines(parts.len(), head, cell_top)) {
                out.labels.push(label(
                    [(edges[i] + edges[i + 1]) / 2., y],
                    text.clone(),
                    TEXT,
                    Align::Middle,
                ));
            }
        }
        for ((g, parts), h) in self.groups.iter().zip(&group_titles).zip(&group_heights) {
            let (a, b) = (edges[g.from], edges[g.to]);
            for (text, y) in parts.iter().zip(baselines(parts.len(), top - h, top)) {
                out.labels
                    .push(label([(a + b) / 2., y], text.clone(), TEXT, Align::Middle));
            }
            out.lines
                .push(line(vec![[a, top - h], [b, top - h]], LineStyle::Frame));
        }
        // Frame, header line and column lines are main lines; rows thin.
        out.lines.push(line(
            vec![
                [x0, top],
                [x0 + w, top],
                [x0 + w, bottom],
                [x0, bottom],
                [x0, top],
            ],
            LineStyle::Frame,
        ));
        out.lines
            .push(line(vec![[x0, head], [x0 + w, head]], LineStyle::Frame));
        for (i, x) in edges.iter().enumerate().take(edges.len() - 1).skip(1) {
            let inner = self
                .groups
                .iter()
                .zip(&group_heights)
                .find(|(g, _)| g.from < i && i < g.to);
            let from = inner.map_or(top, |(_, h)| top - h);
            out.lines
                .push(line(vec![[*x, from], [*x, bottom]], LineStyle::Frame));
        }
        let mut y = head;
        for (k, (row, cells)) in self.rows.iter().zip(self.lines_of(font)).enumerate() {
            let h = heights[k];
            match row {
                Row::Cells(_) => {
                    for (i, lines) in cells.iter().enumerate() {
                        let c = &self.columns[i];
                        let x = match c.align {
                            Align::Start => edges[i] + 1.,
                            Align::Middle => (edges[i] + edges[i + 1]) / 2.,
                            Align::End => edges[i + 1] - 1.,
                        };
                        for (text, by) in lines.iter().zip(baselines(lines.len(), y - h, y)) {
                            out.labels.push(label([x, by], text.clone(), TEXT, c.align));
                        }
                    }
                }
                Row::Heading(_) => {
                    let i = self.name_column;
                    let centre = (edges[i] + edges[i + 1]) / 2.;
                    let lines = &cells[0];
                    for (text, by) in lines.iter().zip(baselines(lines.len(), y - h, y)) {
                        let tw = width(font, text, TEXT);
                        out.labels
                            .push(label([centre, by], text.clone(), TEXT, Align::Middle));
                        out.lines.push(line(
                            vec![[centre - tw / 2., by - 0.8], [centre + tw / 2., by - 0.8]],
                            LineStyle::Thin,
                        ));
                    }
                }
            }
            y -= h;
            if k + 1 < self.rows.len() {
                out.lines
                    .push(line(vec![[x0, y], [x0 + w, y]], LineStyle::Thin));
            }
        }
        out.size = [w, top_left[1] - bottom];
        out
    }
}

fn column(title: &str, width: f64, align: Align) -> Column {
    Column {
        title: title.into(),
        width,
        align,
    }
}

/// Form 1: sheet list or list of specifications (Лист 15, Наименование 140,
/// Примечание 30).
pub fn form1(title: &str, rows: Vec<Row>) -> Table {
    Table {
        title: Some(title.into()),
        columns: vec![
            column("Лист", 15., Align::Middle),
            column("Наименование", 140., Align::Start),
            column("Примечание", 30., Align::Start),
        ],
        groups: Vec::new(),
        header: HEADER,
        name_column: 1,
        rows,
    }
}

/// Form 2: referenced and attached documents, list of sets. The graphs are
/// «Обозначение», «Наименование», «Примечание» (Б.7, Б.8); a designation
/// does not fit 15 mm, so the first graph is 60 mm and the name 95 mm (Б.9).
pub fn form2(title: &str, rows: Vec<Row>) -> Table {
    Table {
        title: Some(title.into()),
        columns: vec![
            column("Обозначение", 60., Align::Start),
            column("Наименование", 95., Align::Start),
            column("Примечание", 30., Align::Start),
        ],
        groups: Vec::new(),
        header: HEADER,
        name_column: 1,
        rows,
    }
}

/// Form 7 specification: Поз. 15, Обозначение 60, Наименование 65, Кол. 10,
/// Масса ед., кг 15, Примечание 20.
pub fn form7(title: Option<&str>, rows: Vec<Row>) -> Table {
    Table {
        title: title.map(Into::into),
        columns: vec![
            column("Поз.", 15., Align::Middle),
            column("Обозначение", 60., Align::Start),
            column("Наименование", 65., Align::Start),
            column("Кол.", 10., Align::Middle),
            column("Масса\nед., кг", 15., Align::Middle),
            column("Приме-\nчание", 20., Align::Start),
        ],
        groups: Vec::new(),
        header: HEADER,
        name_column: 2,
        rows,
    }
}

/// Form 8 group specification: quantities by schemes (or floors) in 10 mm
/// columns under «Кол. <by>», then «Всего» 12 mm.
pub fn form8(title: Option<&str>, by: &str, groups: &[String], rows: Vec<Row>) -> Table {
    let mut columns = vec![
        column("Поз.", 15., Align::Middle),
        column("Обозначение", 60., Align::Start),
        column("Наименование", 65., Align::Start),
    ];
    columns.extend(groups.iter().map(|g| column(g, 10., Align::Middle)));
    columns.push(column("Всего", 12., Align::Middle));
    columns.push(column("Масса\nед., кг", 15., Align::Middle));
    columns.push(column("Приме-\nчание", 20., Align::Start));
    Table {
        title: title.map(Into::into),
        groups: vec![Group {
            from: 3,
            to: 4 + groups.len(),
            title: format!("Кол. {by}"),
            height: 7.,
        }],
        columns,
        header: HEADER,
        name_column: 2,
        rows,
    }
}

/// Mass with three decimals and a decimal comma.
pub fn mass(value: f64) -> String {
    format!("{value:.3}").replace('.', ",")
}

/// Parts of an assembly with its sub-assemblies.
fn assembly_members(project: &Project, assembly: ObjectId) -> BTreeSet<ObjectId> {
    let mut parts = BTreeSet::new();
    let mut stack = vec![assembly];
    let mut seen = BTreeSet::new();
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        if let Some(a) = project.assemblies.get(&id) {
            parts.extend(
                a.members
                    .iter()
                    .filter(|m| project.elements.contains_key(m)),
            );
            stack.extend(&a.children);
        }
    }
    parts
}

/// Section and length of a part for the name graph: «I 300×150×8×12,
/// L=3000», «—20×300, L=400».
fn part_name(project: &Project, id: ObjectId) -> Result<String, Error> {
    let element = project
        .elements
        .get(&id)
        .ok_or_else(|| format!("Деталь #{} не найдена", id.0))?;
    if let Some(swept) = project.swept_parts.get(&id) {
        let length = project
            .part_developed_length(id)
            .map_err(|e| e.to_string())?;
        return match (swept, &element.shape) {
            (
                open_bim_model::SweptPart::BentPlate { width, .. },
                ElementShape::Plate { thickness, .. },
            ) => Ok(format!(
                "Гнутый лист —{}×{}, развёртка L={}",
                crate::format_mm(thickness.value()),
                crate::format_mm(*width),
                crate::format_mm(length)
            )),
            (_, ElementShape::Member { profile, .. }) => {
                let name = project
                    .profiles
                    .get(profile)
                    .map_or_else(|| format!("профиль #{}", profile.0), |p| p.name.clone());
                Ok(format!("{name}, развёртка L={}", crate::format_mm(length)))
            }
            _ => Err("Несовместимая форма гнутой детали".into()),
        };
    }
    Ok(match &element.shape {
        ElementShape::Member {
            profile, length, ..
        } => {
            let name = project
                .profiles
                .get(profile)
                .map_or_else(|| format!("профиль #{}", profile.0), |p| p.name.clone());
            format!("{name}, L={}", crate::format_mm(length.value()))
        }
        ElementShape::Plate {
            contour, thickness, ..
        } => {
            let span = |i: usize| {
                let (lo, hi) = contour
                    .iter()
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                        (lo.min(p[i]), hi.max(p[i]))
                    });
                hi - lo
            };
            let (a, b) = (span(0), span(1));
            format!(
                "—{}×{}, L={}",
                crate::format_mm(thickness.value()),
                crate::format_mm(a.min(b)),
                crate::format_mm(a.max(b))
            )
        }
    })
}

/// Steel grade: the material name up to « · ».
pub fn grade(project: &Project, id: ObjectId) -> String {
    project
        .elements
        .get(&id)
        .and_then(|e| project.materials.get(&e.material))
        .map(|m| {
            m.name
                .split(" · ")
                .next()
                .unwrap_or(&m.name)
                .trim()
                .to_string()
        })
        .unwrap_or_default()
}

/// Form 7 rows of an assembly drawing: section «Детали» by position (count
/// per assembly, unit mass), then «Стандартные изделия» — bolts joining only
/// parts of the assembly (shop bolts).
/// Family objects shown as one item line («изделие») inside `assembly`:
/// (line name, parts) of each one nested in it.
fn family_items(project: &Project, assembly: ObjectId) -> Vec<(String, BTreeSet<ObjectId>)> {
    project
        .family_instances
        .values()
        .filter(|i| {
            project
                .families
                .get(&i.family)
                .is_some_and(|f| f.spec == open_bim_model::SpecMode::Item)
        })
        .filter_map(|i| {
            let own = *i.children.get("assembly")?;
            (own != assembly).then(|| {
                let name = if i.type_name.is_empty() {
                    i.family.clone()
                } else {
                    format!("{} {}", i.family, i.type_name)
                };
                (name, assembly_members(project, own))
            })
        })
        .collect()
}

pub fn assembly_specification(project: &Project, assembly: ObjectId) -> Result<Vec<Row>, Error> {
    let mut parts = assembly_members(project, assembly);
    // Items count as one line each; their parts leave «Детали» (the
    // bolt groups still see every part).
    let all_parts = parts.clone();
    let mut items: BTreeMap<String, (usize, f64)> = BTreeMap::new();
    for (name, members) in family_items(project, assembly) {
        if members.is_empty() || !members.is_subset(&parts) {
            continue;
        }
        let mut unit = 0.;
        for id in &members {
            unit += project.element_mass_kg(*id).map_err(|e| e.to_string())?;
            parts.remove(id);
        }
        let entry = items.entry(name).or_insert((0, unit));
        entry.0 += 1;
    }
    let numbering = open_bim_numbering::number(project).map_err(|e| e.to_string())?;
    let mut by_position: BTreeMap<String, Vec<ObjectId>> = BTreeMap::new();
    for id in &parts {
        let position = numbering
            .parts
            .get(id)
            .cloned()
            .unwrap_or_else(|| format!("#{}", id.0));
        by_position.entry(position).or_default().push(*id);
    }
    let mut rows = vec![Row::Heading("Детали".into())];
    for (position, ids) in &by_position {
        // A welded section: its plates as positions «5.1», «5.2», …
        if let Some(pieces) = project.welded_pieces(ids[0]).map_err(|e| e.to_string())? {
            for (i, piece) in pieces.iter().enumerate() {
                rows.push(Row::Cells(vec![
                    format!("{position}.{}", i + 1),
                    String::new(),
                    format!(
                        "{}: —{}×{}, L={}",
                        piece.name,
                        crate::format_mm(piece.thickness),
                        crate::format_mm(piece.breadth),
                        crate::format_mm(piece.length)
                    ),
                    ids.len().to_string(),
                    mass(piece.mass_kg),
                    grade(project, ids[0]),
                ]));
            }
            continue;
        }
        let unit = project.element_mass_kg(ids[0]).map_err(|e| e.to_string())?;
        rows.push(Row::Cells(vec![
            position.clone(),
            String::new(),
            part_name(project, ids[0])?,
            ids.len().to_string(),
            mass(unit),
            grade(project, ids[0]),
        ]));
    }
    let mut bolts: BTreeMap<String, usize> = BTreeMap::new();
    for detail in project.details.values() {
        if let DetailKind::BoltGroup {
            parts: joined,
            diameter,
            standard,
            grade,
            ..
        } = &detail.kind
            && !joined.is_empty()
            && joined.is_subset(&all_parts)
        {
            // Each bolt comes with one nut and one washer.
            let count = detail.kind.bolt_positions().len();
            let d = crate::format_mm(*diameter);
            let (nut, washer) = open_bim_model::bolt_set(standard);
            for name in [
                format!("Болт М{d}, класс прочности {grade}, {standard}"),
                format!("Гайка М{d}, {nut}"),
                format!("Шайба {d}, {washer}"),
            ] {
                *bolts.entry(name).or_default() += count;
            }
        }
    }
    if !items.is_empty() {
        rows.push(Row::Heading("Прочие изделия".into()));
        for (name, (count, unit)) in items {
            rows.push(Row::Cells(vec![
                String::new(),
                String::new(),
                name,
                count.to_string(),
                mass(unit),
                String::new(),
            ]));
        }
    }
    if !bolts.is_empty() {
        rows.push(Row::Heading("Стандартные изделия".into()));
        for (name, count) in bolts {
            rows.push(Row::Cells(vec![
                String::new(),
                String::new(),
                name,
                count.to_string(),
                String::new(),
                String::new(),
            ]));
        }
    }
    Ok(rows)
}

/// Mass of an element in tonnes.
fn tonnes(project: &Project, id: ObjectId) -> Result<f64, Error> {
    Ok(project.element_mass_kg(id).map_err(|e| e.to_string())? / 1000.)
}

/// Element type a part counts under in ГОСТ 21.502: the kind of the main
/// part of its top assembly (a plate welded to a column is column steel),
/// otherwise its own kind.
pub fn element_type(project: &Project, id: ObjectId) -> &'static str {
    let children: BTreeSet<ObjectId> = project
        .assemblies
        .values()
        .flat_map(|a| a.children.iter().copied())
        .collect();
    let main = project
        .assemblies
        .values()
        .filter(|a| !children.contains(&a.id))
        .find(|a| assembly_members(project, a.id).contains(&id))
        .map_or(id, |a| a.main_part);
    match project.elements.get(&main).map(|e| &e.shape) {
        Some(ElementShape::Member {
            member_kind: MemberKind::Column,
            ..
        }) => "Колонны",
        Some(ElementShape::Member {
            member_kind: MemberKind::Beam,
            ..
        }) => "Балки",
        _ => "Прочие",
    }
}

/// Assortment kind and size of a part: «Двутавры» / «I 300×150×8×12»,
/// «Прокат листовой ГОСТ 19903-2015» / «—20».
pub fn assortment(project: &Project, id: ObjectId) -> (String, String) {
    let Some(element) = project.elements.get(&id) else {
        return (String::new(), String::new());
    };
    match &element.shape {
        ElementShape::Member { profile, .. } => {
            let Some(p) = project.profiles.get(profile) else {
                return ("Профили".into(), format!("#{}", profile.0));
            };
            let kind = match p.shape {
                ProfileShape::ISection { .. } => "Двутавры",
                ProfileShape::Angle { .. } => "Уголки",
                ProfileShape::Channel { .. } => "Швеллеры",
                ProfileShape::CircularHollow { .. } => "Трубы круглые",
                ProfileShape::RectangularHollow { .. } => "Трубы профильные",
                ProfileShape::Rectangle { .. } => "Прокат прямоугольный",
                ProfileShape::Polygon { .. } => "Профили по контуру",
                // Listed by plates in the specification (`rolled_steel`).
                ProfileShape::Welded { .. } => "Сечения сварные",
            };
            // ГОСТ 21.502: «Наименование профиля, ГОСТ, ТУ».
            let kind = match &p.standard {
                Some(standard) => format!("{kind} {standard}"),
                None => kind.into(),
            };
            (kind, p.name.clone())
        }
        ElementShape::Plate { thickness, .. } => (
            format!(
                "Прокат листовой {}",
                open_bim_model::library::PLATE_STANDARD
            ),
            format!("—{}", crate::format_mm(thickness.value())),
        ),
    }
}

/// One line of the rolled-steel specification: masses in tonnes per
/// element type (in `RolledSteel::types` order) and their sum.
#[derive(Clone, Debug, PartialEq)]
pub struct SteelLine {
    pub assortment: String,
    pub grade: String,
    pub size: String,
    pub masses_t: Vec<f64>,
    pub total_t: f64,
}

/// ГОСТ 21.502 specification of rolled steel: lines by assortment, grade
/// and size, subtotals per assortment, totals per grade and overall.
#[derive(Clone, Debug, PartialEq)]
pub struct RolledSteel {
    pub types: Vec<&'static str>,
    pub lines: Vec<SteelLine>,
    pub subtotals: Vec<SteelLine>,
    pub by_grade: Vec<SteelLine>,
    pub total: SteelLine,
}

const TYPE_ORDER: [&str; 3] = ["Колонны", "Балки", "Прочие"];

pub fn rolled_steel(project: &Project, parts: &[ObjectId]) -> Result<RolledSteel, Error> {
    let mut raw: BTreeMap<(String, String, String), BTreeMap<&'static str, f64>> = BTreeMap::new();
    for id in parts {
        // A welded section counts as the plates it is made of.
        if let Some(pieces) = project.welded_pieces(*id).map_err(|e| e.to_string())? {
            for piece in pieces {
                let entry = raw
                    .entry((
                        format!(
                            "Прокат листовой {}",
                            open_bim_model::library::PLATE_STANDARD
                        ),
                        grade(project, *id),
                        format!("—{}", crate::format_mm(piece.thickness)),
                    ))
                    .or_default();
                *entry.entry(element_type(project, *id)).or_default() += piece.mass_kg / 1000.;
            }
            continue;
        }
        let (kind, size) = assortment(project, *id);
        let entry = raw.entry((kind, grade(project, *id), size)).or_default();
        *entry.entry(element_type(project, *id)).or_default() += tonnes(project, *id)?;
    }
    let present: BTreeSet<&'static str> = raw.values().flat_map(|m| m.keys().copied()).collect();
    let types: Vec<&'static str> = TYPE_ORDER
        .iter()
        .copied()
        .filter(|t| present.contains(t))
        .collect();
    let line = |assortment: &str, grade: &str, size: &str, by: &BTreeMap<&'static str, f64>| {
        let masses_t: Vec<f64> = types
            .iter()
            .map(|t| by.get(t).copied().unwrap_or(0.))
            .collect();
        SteelLine {
            assortment: assortment.into(),
            grade: grade.into(),
            size: size.into(),
            total_t: masses_t.iter().sum(),
            masses_t,
        }
    };
    let add = |into: &mut BTreeMap<&'static str, f64>, by: &BTreeMap<&'static str, f64>| {
        for (t, m) in by {
            *into.entry(t).or_default() += m;
        }
    };
    let mut lines = Vec::new();
    let mut per_kind: BTreeMap<String, BTreeMap<&'static str, f64>> = BTreeMap::new();
    let mut per_grade: BTreeMap<String, BTreeMap<&'static str, f64>> = BTreeMap::new();
    let mut all = BTreeMap::new();
    for ((kind, grade, size), by) in &raw {
        lines.push(line(kind, grade, size, by));
        add(per_kind.entry(kind.clone()).or_default(), by);
        add(per_grade.entry(grade.clone()).or_default(), by);
        add(&mut all, by);
    }
    Ok(RolledSteel {
        subtotals: per_kind
            .iter()
            .map(|(k, by)| line(k, "", "Итого", by))
            .collect(),
        by_grade: per_grade
            .iter()
            .map(|(g, by)| line("", g, "", by))
            .collect(),
        total: line("Всего масса металла", "", "", &all),
        lines,
        types,
    })
}

fn tonnes_text(value: f64) -> String {
    if value.abs() < 5e-4 {
        String::new()
    } else {
        mass(value)
    }
}

/// The rolled-steel specification as a table; the mass columns sit under
/// «Масса металла по элементам конструкций, т». Widths are this program's
/// layout (ГОСТ 21.502 is not reproduced in the repository).
pub fn rolled_steel_table(rs: &RolledSteel) -> Table {
    let mut columns = vec![
        column("Наименование\nпрофиля,\nГОСТ, ТУ", 35., Align::Start),
        column("Наименование\nили марка\nметалла", 30., Align::Start),
        column("Номер или\nразмеры\nпрофиля, мм", 30., Align::Start),
        column("№\nп.п.", 10., Align::Middle),
    ];
    columns.extend(rs.types.iter().map(|t| column(t, 16., Align::Middle)));
    columns.push(column("Общая\nмасса, т", 18., Align::Middle));
    let mut rows = Vec::new();
    let mut n = 0;
    let mut push = |rows: &mut Vec<Row>, first: &str, l: &SteelLine| {
        n += 1;
        let mut cells = vec![
            first.to_string(),
            l.grade.clone(),
            l.size.clone(),
            n.to_string(),
        ];
        cells.extend(l.masses_t.iter().map(|m| tonnes_text(*m)));
        cells.push(tonnes_text(l.total_t));
        rows.push(Row::Cells(cells));
    };
    for sub in &rs.subtotals {
        let mut first = true;
        for l in rs.lines.iter().filter(|l| l.assortment == sub.assortment) {
            push(&mut rows, if first { &l.assortment } else { "" }, l);
            first = false;
        }
        push(&mut rows, "", sub);
    }
    push(&mut rows, &rs.total.assortment, &rs.total);
    rows.push(Row::Cells(vec!["В том числе по маркам:".into()]));
    for g in &rs.by_grade {
        push(&mut rows, "", g);
    }
    let first_mass = 4;
    Table {
        title: Some("Спецификация металлопроката".into()),
        groups: vec![Group {
            from: first_mass,
            to: first_mass + rs.types.len(),
            title: "Масса металла по элементам конструкций, т".into(),
            height: 10.,
        }],
        columns,
        header: 20.,
        name_column: 0,
        rows,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bent_plate_name_uses_developed_length() {
        let fixture = crate::sample::control_project();
        let mut project = fixture.project;
        project.swept_parts.insert(
            fixture.plate,
            open_bim_model::SweptPart::BentPlate {
                width: 200.,
                first_length: 100.,
                second_length: 80.,
                radius: 10.,
                angle: std::f64::consts::FRAC_PI_2,
            },
        );
        let expected = project.part_developed_length(fixture.plate).unwrap();
        let text = part_name(&project, fixture.plate).unwrap();
        assert!(text.contains("Гнутый лист"));
        assert!(text.contains(&format!("развёртка L={}", crate::format_mm(expected))));
    }

    #[test]
    fn form_widths_follow_the_appendices() {
        let w = |t: &Table| t.columns.iter().map(|c| c.width).collect::<Vec<_>>();
        assert_eq!(w(&form1("", vec![])), [15., 140., 30.]);
        assert_eq!(w(&form7(None, vec![])), [15., 60., 65., 10., 15., 20.]);
        let f8 = form8(None, "по схеме", &["1".into(), "2".into()], vec![]);
        assert_eq!(w(&f8), [15., 60., 65., 10., 10., 12., 15., 20.]);
        assert_eq!(form7(None, vec![]).width(), 185.);
    }

    #[test]
    fn long_cells_wrap_and_rows_grow() {
        let rows = vec![Row::Cells(vec![
            "1".into(),
            "очень длинное наименование изображения на листе, которое не помещается в одну строку графы шириной сто сорок миллиметров".into(),
        ])];
        let t = form1("Ведомость", rows);
        let d = t.draw([0., 100.]);
        assert!(t.height() > TITLE_GAP + HEADER + ROW);
        assert!(d.labels.len() >= 5);
        assert!((d.size[1] - t.height()).abs() < 1e-9);
    }
}
