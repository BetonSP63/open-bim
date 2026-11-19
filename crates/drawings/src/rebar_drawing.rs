//! Reinforcement sheet of one slab («КЖ»). Plans, the section, the detail
//! schedule and the steel-consumption schedule are regenerated from the
//! model on every `generate`, on the same sheet, frame and title block as
//! the steel drawings. Nothing drawn is stored.
//!
//! Conventions follow ГОСТ 21.501-2018 for monolithic reinforced-concrete
//! drawings (separate plans of the bottom and top meshes, a section with
//! the cover, thick bar lines, position callouts) and ГОСТ Р 21.101-2026
//! for the sheet itself (frame, main inscription, axes, ticks).
use std::collections::BTreeMap;

use open_bim_model::concrete::ZoneRole;
use open_bim_model::rebar::{self, DetailRow, ShapeSketch};
use open_bim_model::{
    Anchor, AnchorPoint, DetailKind, DimensionAxis, DimensionDef, Drawing, DrawingKind,
    DrawingView, ElementShape, NodeRegion, ObjectId, Point3, Project, RebarSet, SheetSize,
    ViewDirection,
};

use crate::conventions;
use crate::projection::ViewBasis;
use crate::tables::{self, Row, Table};
use crate::{
    Align, DimensionGraphic, Error, Label, LineStyle, MARGIN, MARGIN_LEFT, Polyline, TEXT_HEIGHT,
    ViewContext, ViewGraphic,
};

const CALLOUT_W: f64 = 78.;
const AXIS_LEFT: f64 = 26.;
const UNDER_PLAN: f64 = 32.;
const NOTES_H: f64 = 62.;
const BREAK_GAP: f64 = 14.;

struct SlabGeom {
    frame: open_bim_model::Frame,
    contour: Vec<[f64; 2]>,
    thickness: f64,
    /// Through openings, in the plate frame.
    openings: Vec<Vec<[f64; 2]>>,
    min: [f64; 2],
    max: [f64; 2],
}

impl SlabGeom {
    fn width(&self) -> f64 {
        self.max[0] - self.min[0]
    }
    fn depth(&self) -> f64 {
        self.max[1] - self.min[1]
    }
}

struct Layout {
    sheet: SheetSize,
    scale: f64,
    section_scale: f64,
    /// View centres: bottom plan, top plan, section node.
    at: [[f64; 2]; 3],
    window: Window,
}

struct Window {
    node_u: f64,
    node_r: f64,
    /// Plate X where the far-edge fragment starts. None — the crop already
    /// reaches that edge.
    right_u0: Option<f64>,
    right_len: f64,
    /// Cutting plane, local Y.
    y_cut: f64,
}

/// A new КЖ sheet for one slab: bottom and top plans, a section, overall
/// dimensions. Schedules and notes are added when the sheet is generated.
pub fn reinforcement_drawing(
    project: &Project,
    id: ObjectId,
    part: ObjectId,
    number: &str,
) -> Result<Drawing, Error> {
    let plate = project
        .elements
        .get(&part)
        .is_some_and(|e| matches!(e.shape, ElementShape::Plate { .. }));
    if !plate {
        return crate::member_rebar::member_drawing(project, id, part, number);
    }
    let geom = slab_geom(project, part)?;
    let sets = hosted(project, part);
    let lay = layout(project, part, &geom, &sets)?;
    let kind = DrawingKind::Reinforcement { part };
    let basis = crate::drawing_basis(project, &kind)?;
    let y_world = geom
        .frame
        .world(Point3([geom.min[0], lay.window.y_cut, 0.]));
    let depth = ViewBasis::new(&basis, ViewDirection::Bottom).project(y_world.0)[2];
    let name = project
        .elements
        .get(&part)
        .map(|e| e.name.clone())
        .unwrap_or_else(|| "плита".into());
    let mut views = vec![
        view(
            1,
            "План нижней арматуры",
            ViewDirection::Front,
            lay.at[0],
            None,
        ),
        view(
            2,
            "План верхней арматуры",
            ViewDirection::Front,
            lay.at[1],
            None,
        ),
        view(
            3,
            "1-1",
            ViewDirection::Bottom,
            lay.at[2],
            Some(lay.section_scale),
        ),
    ];
    views[2].section = Some(depth);
    views[2].section_host = Some(1);
    views[2].node = Some(NodeRegion {
        centre: [lay.window.node_u, geom.thickness / 2.],
        radius: lay.window.node_r,
    });
    let anchor = |index| Anchor {
        object: part,
        point: AnchorPoint::Vertex { index },
    };
    let vertices: Vec<Anchor> = (0..geom.contour.len().min(200)).map(anchor).collect();
    let dimensions = if vertices.len() >= 2 {
        vec![
            DimensionDef {
                id: 1,
                view: 1,
                anchors: vertices.clone(),
                axis: DimensionAxis::Horizontal,
                offset: -14.,
            },
            DimensionDef {
                id: 2,
                view: 1,
                anchors: vertices,
                axis: DimensionAxis::Vertical,
                offset: -14.,
            },
        ]
    } else {
        Vec::new()
    };
    Ok(crate::updated(
        project,
        &Drawing {
            id,
            kind,
            number: number.into(),
            title: format!("Армирование {name}"),
            sheet: lay.sheet,
            scale: lay.scale,
            views,
            dimensions,
            texts: Vec::new(),
            objects: Vec::new(),
            fingerprints: BTreeMap::new(),
            generated_revision: 0,
            issued_revision: None,
            multiple: 1,
            portrait: false,
        },
    ))
}

fn view(
    id: u32,
    name: &str,
    direction: ViewDirection,
    position: [f64; 2],
    scale: Option<f64>,
) -> DrawingView {
    DrawingView {
        id,
        name: name.into(),
        direction,
        section: None,
        position,
        scale,
        node: None,
        source: None,
        section_host: None,
        appearance: Default::default(),
        unfolded: false,
    }
}

fn window_of(project: &Project, part: ObjectId, geom: &SlabGeom, sets: &[&RebarSet]) -> Window {
    let cover = rebar::host_cover(project, part);
    let lan = sets
        .iter()
        .find(|s| s.role == Some(ZoneRole::Edge))
        .and_then(|s| s.shape_params.get("A").copied())
        .unwrap_or(400.)
        .clamp(80., geom.width() * 0.45);
    let right_len = (lan + cover + 60.).clamp(160., geom.width() * 0.48);
    let min_r = geom.thickness / 2. + 20.;
    let left_len = (lan + 700.)
        .max(min_r * 2.)
        .min(geom.width() * 0.72)
        .max((min_r * 2.).min(geom.width()));
    let node_r = (left_len / 2.).max(min_r).min(geom.width() / 2.);
    let node_u = geom.min[0] + node_r;
    let right_u0 = geom.max[0] - right_len;
    let separate = right_u0 > node_u + node_r + 20.;
    Window {
        node_u,
        node_r,
        right_u0: separate.then_some(right_u0),
        right_len,
        y_cut: cutting_y(geom, sets),
    }
}

fn layout(
    project: &Project,
    part: ObjectId,
    geom: &SlabGeom,
    sets: &[&RebarSet],
) -> Result<Layout, Error> {
    let window = window_of(project, part, geom, sets);
    let tables = schedule_tables(project, part);
    let table_w = tables.iter().map(Table::width).fold(185., f64::max);
    for sheet in [SheetSize::A3, SheetSize::A2, SheetSize::A1, SheetSize::A0] {
        let [sw, sh] = sheet.landscape_mm();
        let left = MARGIN_LEFT + AXIS_LEFT;
        let right = sw - MARGIN - table_w - 8.;
        let top = sh - MARGIN - 8.;
        let bottom = MARGIN + NOTES_H;
        if right - left < 80. || top - bottom < 80. {
            continue;
        }
        for &scale in crate::SCALES.iter() {
            for &section_scale in crate::SCALES.iter() {
                let band = geom.thickness / section_scale;
                if !(32. ..=78.).contains(&band) {
                    continue;
                }
                let Some(at) = place(
                    geom,
                    scale,
                    section_scale,
                    window.node_r,
                    window.right_len,
                    window.right_u0.is_some(),
                    left,
                    right,
                    top,
                    bottom,
                ) else {
                    continue;
                };
                return Ok(Layout {
                    sheet,
                    scale,
                    section_scale,
                    at,
                    window,
                });
            }
        }
    }
    Err("Плита не помещается на лист A0 даже в масштабе 1:400".into())
}

#[allow(clippy::too_many_arguments)]
fn place(
    geom: &SlabGeom,
    scale: f64,
    section_scale: f64,
    node_r: f64,
    right_len: f64,
    separate: bool,
    left: f64,
    right: f64,
    top: f64,
    bottom: f64,
) -> Option<[[f64; 2]; 3]> {
    let plan = [geom.width() / scale, geom.depth() / scale];
    let node = node_r * 2. / section_scale;
    let frag = if separate {
        right_len / section_scale
    } else {
        0.
    };
    let sec_w = node + if separate { BREAK_GAP + frag } else { 0. } + CALLOUT_W;
    let sec_h = node;
    let row_w = plan[0] + CALLOUT_W;
    let field_w = right - left;
    let side = row_w * 2. + 10. <= field_w && sec_w <= field_w;
    let need_h = if side {
        plan[1] + UNDER_PLAN + 8. + sec_h + 34.
    } else {
        plan[1] * 2. + UNDER_PLAN + 8. + sec_h + 42.
    };
    let need_w = if side {
        row_w * 2. + 10.
    } else {
        row_w.max(sec_w)
    };
    if need_w > field_w || need_h > top - bottom {
        return None;
    }
    let plan_y = top - plan[1] / 2.;
    let (bottom_at, top_at, lower) = if side {
        (
            [left + plan[0] / 2., plan_y],
            [left + row_w + 10. + plan[0] / 2., plan_y],
            plan_y - plan[1] / 2.,
        )
    } else {
        let y2 = plan_y - plan[1] - UNDER_PLAN - 8.;
        (
            [left + plan[0] / 2., plan_y],
            [left + plan[0] / 2., y2],
            y2 - plan[1] / 2.,
        )
    };
    let section_at = [left + node / 2., lower - UNDER_PLAN - 8. - sec_h / 2.];
    let square_bottom = section_at[1] - sec_h / 2.;
    if square_bottom < bottom + 18. {
        return None;
    }
    Some([bottom_at, top_at, section_at])
}

fn cutting_y(geom: &SlabGeom, sets: &[&RebarSet]) -> f64 {
    let mid = (geom.min[1] + geom.max[1]) / 2.;
    let blocked = |y: f64| {
        geom.openings.iter().any(|ring| {
            let (lo, hi) = ring
                .iter()
                .fold((f64::MAX, f64::MIN), |(l, h), p| (l.min(p[1]), h.max(p[1])));
            y > lo + 1. && y < hi - 1.
        })
    };
    let mut frogs = Vec::new();
    for set in sets.iter().filter(|s| s.role == Some(ZoneRole::Frog)) {
        for bar in &set.bars {
            if let Some(y) = mean_local_y(&geom.frame, bar) {
                frogs.push(y);
            }
        }
    }
    frogs.sort_by(f64::total_cmp);
    frogs.dedup_by(|a, b| (*a - *b).abs() < 1.);
    frogs
        .iter()
        .filter(|y| !blocked(**y))
        .min_by(|a, b| (*a - mid).abs().total_cmp(&(*b - mid).abs()))
        .copied()
        .unwrap_or(mid)
}

/// Bar graphics, axes, zone marks and position callouts.
pub(super) fn annotate(
    project: &Project,
    drawing: &Drawing,
    part: ObjectId,
    contexts: &BTreeMap<u32, ViewContext>,
    dimensions: &[DimensionGraphic],
    views: &mut [ViewGraphic],
) {
    let Ok(geom) = slab_geom(project, part) else {
        crate::member_rebar::annotate(project, drawing, part, contexts, views);
        return;
    };
    let sets = hosted(project, part);
    let rows = host_rows(project, part);
    for view in drawing.views.iter().filter(|v| v.section.is_none()) {
        let Some(ctx) = contexts.get(&view.id) else {
            continue;
        };
        let Some(graphic) = views.iter_mut().find(|g| g.id == view.id) else {
            continue;
        };
        let top = view.id != drawing.views.first().map(|v| v.id).unwrap_or(view.id);
        draw_layer(graphic, ctx, &sets, top);
        draw_openings(graphic, ctx, &geom);
        if !top {
            draw_zone(project, graphic, ctx, part);
        }
        let reach = dimensions
            .iter()
            .filter(|d| d.view == view.id)
            .flat_map(|d| d.lines.iter().flat_map(|l| l.points.iter()))
            .fold(graphic.bounds[0], |r, p| [r[0].min(p[0]), r[1].min(p[1])]);
        conventions::axes(project, view, ctx, graphic, reach, true);
        let notes = layer_callouts(&sets, &rows, top);
        place_callouts(graphic, ctx, &notes);
    }
    if let Some(view) = drawing.views.iter().find(|v| v.section.is_some())
        && let Some(ctx) = contexts.get(&view.id)
        && let Some(graphic) = views.iter_mut().find(|g| g.id == view.id)
    {
        draw_section(project, part, graphic, ctx, view, &geom, &sets, &rows);
    }
}

/// Detail schedule, steel consumption and the general notes, above the
/// inscription and along the lower edge.
pub(super) fn schedules_and_notes(
    project: &Project,
    part: ObjectId,
    size: [f64; 2],
    title_area: [f64; 4],
) -> (Vec<Polyline>, Vec<Label>) {
    if slab_geom(project, part).is_err() {
        return crate::member_rebar::schedules_and_notes(project, part, size, title_area);
    }
    let mut pair = paint_schedules(project, part, size, title_area);
    pair.1.extend(general_notes(project, part, size));
    pair
}

/// Detail schedule and steel consumption of one host, above the inscription.
pub(crate) fn paint_schedules(
    project: &Project,
    part: ObjectId,
    size: [f64; 2],
    title_area: [f64; 4],
) -> (Vec<Polyline>, Vec<Label>) {
    let mut lines = Vec::new();
    let mut labels = Vec::new();
    let tables = schedule_tables(project, part);
    let width = tables.iter().map(Table::width).fold(0., f64::max);
    let x = size[0] - MARGIN - width;
    let mut top = title_area[3] + 3.;
    for table in tables.iter().rev() {
        top += table.height();
        let at = [x, top.min(size[1] - MARGIN - 2.)];
        let drawn = table.draw(at);
        if table.title.as_deref() == Some("Ведомость деталей") {
            let sketches = host_rows(project, part);
            for (cell, row) in table.column_cells(at, 1).into_iter().zip(&sketches) {
                let (sl, st) = draw_sketch(cell, &row.sketch);
                lines.extend(sl);
                labels.extend(st);
            }
        }
        lines.extend(drawn.lines);
        labels.extend(drawn.labels);
        top += 4.;
    }
    (lines, labels)
}

pub(crate) fn schedule_width(project: &Project, part: ObjectId) -> f64 {
    schedule_tables(project, part)
        .iter()
        .map(Table::width)
        .fold(188., f64::max)
}

fn schedule_tables(project: &Project, part: ObjectId) -> Vec<Table> {
    let rows = host_rows(project, part);
    vec![detail_table(&rows), steel_table(project, part, &rows)]
}

fn detail_table(rows: &[DetailRow]) -> Table {
    let body = if rows.is_empty() {
        vec![Row::Cells(vec![
            "—".into(),
            "\n\n\n".into(),
            "—".into(),
            "—".into(),
            "—".into(),
            "Стержней нет".into(),
            "—".into(),
            "—".into(),
            "—".into(),
            "—".into(),
        ])]
    } else {
        rows.iter()
            .map(|r| {
                Row::Cells(vec![
                    r.mark.to_string(),
                    "\n\n\n".into(),
                    r.code.clone(),
                    mm(r.diameter),
                    r.grade.clone(),
                    portions(r),
                    mm(r.length_mm),
                    r.count.to_string(),
                    kg(r.unit_mass_kg),
                    kg(r.total_mass_kg),
                ])
            })
            .collect()
    };
    let mut cells = body;
    if !rows.is_empty() {
        let n: usize = rows.iter().map(|r| r.count).sum();
        let mass: f64 = rows.iter().map(|r| r.total_mass_kg).sum();
        cells.push(Row::Cells(vec![
            "Итого".into(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            n.to_string(),
            String::new(),
            kg(mass),
        ]));
    }
    Table {
        title: Some("Ведомость деталей".into()),
        columns: vec![
            tables::column("Поз.", 12., Align::Middle),
            tables::column("Эскиз", 32., Align::Middle),
            tables::column("Форма", 14., Align::Middle),
            tables::column("Ø", 12., Align::Middle),
            tables::column("Класс", 18., Align::Middle),
            tables::column("Участки", 40., Align::Start),
            tables::column("L, мм", 16., Align::Middle),
            tables::column("Кол.", 12., Align::Middle),
            tables::column("m, кг", 16., Align::Middle),
            tables::column("Σ, кг", 16., Align::Middle),
        ],
        groups: Vec::new(),
        header: tables::HEADER,
        name_column: 5,
        rows: cells,
    }
}

fn steel_table(project: &Project, part: ObjectId, rows: &[DetailRow]) -> Table {
    let name = project
        .elements
        .get(&part)
        .map(|e| e.name.clone())
        .unwrap_or_else(|| "Плита".into());
    let mut groups: BTreeMap<(String, i64), (f64, usize, f64, f64)> = BTreeMap::new();
    for row in rows {
        let key = (row.grade.clone(), (row.diameter * 1000.).round() as i64);
        let entry = groups.entry(key).or_insert((row.diameter, 0, 0., 0.));
        entry.1 += row.count;
        entry.2 += row.count as f64 * row.length_mm / 1000.;
        entry.3 += row.total_mass_kg;
    }
    let mut body = Vec::new();
    let mut total_n = 0usize;
    let mut total_l = 0.;
    let mut total_m = 0.;
    for ((grade, _), (diameter, n, length, mass)) in &groups {
        total_n += n;
        total_l += length;
        total_m += mass;
        body.push(Row::Cells(vec![
            name.clone(),
            grade.clone(),
            mm(*diameter),
            n.to_string(),
            kg(*length),
            kg(*mass),
        ]));
    }
    if body.is_empty() {
        body.push(Row::Cells(vec![
            name,
            "—".into(),
            "—".into(),
            "0".into(),
            "0".into(),
            "0".into(),
        ]));
    }
    body.push(Row::Cells(vec![
        "Итого".into(),
        String::new(),
        String::new(),
        total_n.to_string(),
        kg(total_l),
        kg(total_m),
    ]));
    Table {
        title: Some("Ведомость расхода стали".into()),
        columns: vec![
            tables::column("Элемент", 40., Align::Start),
            tables::column("Класс", 18., Align::Middle),
            tables::column("Ø", 12., Align::Middle),
            tables::column("Кол.", 14., Align::Middle),
            tables::column("Длина, м", 20., Align::Middle),
            tables::column("Масса, кг", 20., Align::Middle),
        ],
        groups: Vec::new(),
        header: tables::HEADER,
        name_column: 0,
        rows: body,
    }
}

fn general_notes(project: &Project, part: ObjectId, _size: [f64; 2]) -> Vec<Label> {
    let sets = hosted(project, part);
    let cover = mm(rebar::host_cover(project, part));
    let material = project
        .elements
        .get(&part)
        .and_then(|e| project.materials.get(&e.material))
        .map(|m| m.name.clone())
        .unwrap_or_else(|| "бетон".into());
    let mut grades: Vec<String> = sets.iter().map(|s| s.grade.clone()).collect();
    grades.sort();
    grades.dedup();
    let mesh = |role: ZoneRole| {
        sets.iter().find(|s| s.role == Some(role)).map(|s| {
            format!(
                "Ø{} {} шаг {}",
                mm(s.diameter),
                s.grade,
                mm(s.rule.as_ref().map(|r| r.spacing).unwrap_or(0.))
            )
        })
    };
    let step = |role: ZoneRole| {
        sets.iter()
            .find(|s| s.role == Some(role))
            .and_then(|s| s.rule.as_ref().map(|r| mm(r.spacing)))
            .unwrap_or_else(|| "—".into())
    };
    let mut lines = vec![
        "Общие указания".into(),
        "Чертёж по ГОСТ 21.501-2018. Штамп по ГОСТ Р 21.101-2026.".into(),
        format!("Бетон — {material}. Защитный слой {cover} мм."),
        format!(
            "Арматура {} по ГОСТ 34028-2016.",
            if grades.is_empty() {
                "не задана".into()
            } else {
                grades.join(", ")
            }
        ),
        format!(
            "Нижняя сетка: {}. Верхняя сетка: {}.",
            mesh(ZoneRole::BottomMain).unwrap_or_else(|| "нет".into()),
            mesh(ZoneRole::TopMain).unwrap_or_else(|| "нет".into())
        ),
        format!(
            "П-стержни по контуру и проёмам, шаг {} мм.",
            step(ZoneRole::Edge)
        ),
        format!("Лягушки между сетками, шаг {} мм.", step(ZoneRole::Frog)),
        "Анкеровка и нахлёст — по СП 63.13330.".into(),
        "Длины заготовок и масса — в ведомостях на этом листе.".into(),
    ];
    for note in &project.document.general_notes {
        let note = note.trim();
        if !note.is_empty() {
            lines.push(note.to_string());
        }
    }
    let count = lines.len();
    lines
        .into_iter()
        .enumerate()
        .map(|(i, text)| Label {
            position: [MARGIN_LEFT + 2., MARGIN + 4. + (count - 1 - i) as f64 * 5.2],
            text,
            height: TEXT_HEIGHT,
            align: Align::Start,
            rotation: 0.,
            object: None,
            lost: false,
        })
        .collect()
}

fn draw_sketch(cell: [f64; 4], sketch: &ShapeSketch) -> (Vec<Polyline>, Vec<Label>) {
    let mut lines = Vec::new();
    let mut labels = Vec::new();
    if sketch.points.len() < 2 {
        return (lines, labels);
    }
    let [x0, y0, x1, y1] = sketch.bounds;
    let (bw, bh) = ((x1 - x0).max(1.), (y1 - y0).max(1.));
    let (w, h) = (cell[2] - cell[0] - 2., cell[3] - cell[1] - 2.);
    if w < 2. || h < 2. {
        return (lines, labels);
    }
    let scale = (w / bw).min(h / bh);
    let ox = (cell[0] + cell[2]) / 2. - (x0 + x1) / 2. * scale;
    let oy = (cell[1] + cell[3]) / 2. - (y0 + y1) / 2. * scale;
    let map = |p: [f64; 2]| [ox + p[0] * scale, oy + p[1] * scale];
    lines.push(poly(
        sketch.points.iter().copied().map(map).collect(),
        LineStyle::Visible,
    ));
    for mark in &sketch.marks {
        if mark.text.chars().count() > 3 {
            continue;
        }
        let at = map(mark.at);
        if at[0] < cell[0] || at[0] > cell[2] || at[1] < cell[1] || at[1] > cell[3] {
            continue;
        }
        labels.push(label(at, mark.text.clone(), 1.8, Align::Middle));
    }
    (lines, labels)
}

fn draw_layer(graphic: &mut ViewGraphic, ctx: &ViewContext, sets: &[&RebarSet], top: bool) {
    for set in sets.iter().filter(|s| shows(s, top)) {
        for bar in &set.bars {
            let pts: Vec<[f64; 2]> = bar.iter().map(|p| plan_paper(ctx, p.0)).collect();
            if pts.len() >= 2 {
                graphic.lines.push(poly(pts, LineStyle::Visible));
            }
        }
    }
}

fn shows(set: &RebarSet, top: bool) -> bool {
    match set.role {
        Some(ZoneRole::BottomMain | ZoneRole::BottomCross | ZoneRole::Frog) => !top,
        Some(ZoneRole::TopMain | ZoneRole::TopCross) => top,
        _ => true,
    }
}

fn draw_openings(graphic: &mut ViewGraphic, ctx: &ViewContext, geom: &SlabGeom) {
    for ring in &geom.openings {
        if ring.is_empty() {
            continue;
        }
        let (min, max) = ring
            .iter()
            .fold(([f64::MAX; 2], [f64::MIN; 2]), |(l, h), p| {
                (
                    [l[0].min(p[0]), l[1].min(p[1])],
                    [h[0].max(p[0]), h[1].max(p[1])],
                )
            });
        let centre = [(min[0] + max[0]) / 2., (min[1] + max[1]) / 2.];
        let at = ctx.paper(centre);
        graphic.labels.push(label(
            [at[0], at[1] + 3.],
            format!("проём {}×{}", mm(max[0] - min[0]), mm(max[1] - min[1])),
            TEXT_HEIGHT,
            Align::Middle,
        ));
    }
}

fn draw_zone(project: &Project, graphic: &mut ViewGraphic, ctx: &ViewContext, part: ObjectId) {
    for zone in project.concrete.zones.iter().filter(|z| z.host == part) {
        let q = ctx.basis.project(zone.mark_at.0);
        let at = ctx.paper([q[0], q[1]]);
        graphic
            .lines
            .push(conventions::circle(at, 4., LineStyle::Thin));
        graphic.labels.push(label(
            [at[0] + 6., at[1] + 1.],
            format!("Зона {}: {}", zone.number, zone.mark),
            TEXT_HEIGHT,
            Align::Start,
        ));
    }
}

fn layer_callouts(sets: &[&RebarSet], rows: &[DetailRow], top: bool) -> Vec<(String, [f64; 3])> {
    let mut out = Vec::new();
    for set in sets.iter().copied().filter(|s| shows(s, top)) {
        let text = callout_of(set, rows);
        let at = set
            .bars
            .first()
            .and_then(|b| b.get(b.len() / 2))
            .map(|p| p.0)
            .unwrap_or([0.; 3]);
        if out.iter().any(|(t, _)| t == &text) {
            continue;
        }
        out.push((text, at));
    }
    out
}

fn place_callouts(graphic: &mut ViewGraphic, ctx: &ViewContext, notes: &[(String, [f64; 3])]) {
    if notes.is_empty() {
        return;
    }
    let x = graphic.bounds[1][0] + 4.;
    let mut y = graphic.bounds[1][1];
    for (text, world) in notes {
        let at = plan_paper(ctx, *world);
        graphic
            .lines
            .push(poly(vec![at, [x - 1., y + 1.]], LineStyle::Thin));
        graphic
            .labels
            .push(label([x, y], text.clone(), TEXT_HEIGHT, Align::Start));
        y -= 6.5;
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_section(
    project: &Project,
    part: ObjectId,
    graphic: &mut ViewGraphic,
    ctx: &ViewContext,
    view: &DrawingView,
    geom: &SlabGeom,
    sets: &[&RebarSet],
    rows: &[DetailRow],
) {
    let Some(depth) = view.section else { return };
    let Some(node) = view.node else { return };
    let window = window_of(project, part, geom, sets);
    let u0 = node.centre[0] - node.radius;
    let u1 = node.centre[0] + node.radius;
    let separate = window.right_u0.is_some();
    let right_u0 = window.right_u0.unwrap_or(u1);
    let shift = if separate {
        let node_right = ctx.paper([u1, 0.])[0];
        node_right + BREAK_GAP - ctx.paper([right_u0, 0.])[0]
    } else {
        0.
    };
    if separate {
        draw_fragment(graphic, ctx, geom, right_u0, shift);
        draw_break(ctx, graphic, u1, geom.thickness);
    }
    for set in sets {
        let tol = set.rule.as_ref().map(|r| r.spacing).unwrap_or(40.) / 2. + 2.;
        for bar in &set.bars {
            let pts: Vec<[f64; 3]> = bar.iter().map(|p| ctx.basis.project(p.0)).collect();
            if pts.len() < 2 {
                continue;
            }
            let (d0, d1) = pts
                .iter()
                .fold((f64::MAX, f64::MIN), |(l, h), p| (l.min(p[2]), h.max(p[2])));
            if d1 - d0 < 40. {
                let mean = pts.iter().map(|p| p[2]).sum::<f64>() / pts.len() as f64;
                if (mean - depth).abs() > tol {
                    continue;
                }
                for (range, extra) in ranges(u0, u1, right_u0, geom.max[0], separate, shift) {
                    for run in clip_u(&pts, range.0, range.1) {
                        let paper = run
                            .into_iter()
                            .map(|p| {
                                let mut q = ctx.paper([p[0], p[1]]);
                                q[0] += extra;
                                q
                            })
                            .collect();
                        graphic.lines.push(poly(paper, LineStyle::Visible));
                    }
                }
            } else {
                for w in pts.windows(2) {
                    if (w[0][2] - depth) * (w[1][2] - depth) > 0. {
                        continue;
                    }
                    let denom = w[1][2] - w[0][2];
                    if denom.abs() < 1e-9 {
                        continue;
                    }
                    let t = (depth - w[0][2]) / denom;
                    let u = w[0][0] + (w[1][0] - w[0][0]) * t;
                    let v = w[0][1] + (w[1][1] - w[0][1]) * t;
                    let (extra, ok) = if (u0..=u1).contains(&u) {
                        (0., true)
                    } else if separate && (right_u0..=geom.max[0]).contains(&u) {
                        (shift, true)
                    } else {
                        (0., false)
                    };
                    if !ok {
                        continue;
                    }
                    let mut at = ctx.paper([u, v]);
                    at[0] += extra;
                    let r = (set.diameter / 2. / ctx.scale).clamp(0.6, 2.2);
                    graphic
                        .lines
                        .push(conventions::circle(at, r, LineStyle::Visible));
                }
            }
        }
    }
    let cover = rebar::host_cover(project, part);
    let x = ctx.paper([geom.min[0], 0.])[0] + 3.;
    let y0 = ctx.paper([0., 0.])[1];
    let y1 = ctx.paper([0., cover])[1];
    graphic
        .lines
        .push(poly(vec![[x, y0], [x, y1]], LineStyle::Thin));
    graphic
        .lines
        .push(poly(vec![[x - 1.2, y0], [x + 1.2, y0]], LineStyle::Thin));
    graphic
        .lines
        .push(poly(vec![[x - 1.2, y1], [x + 1.2, y1]], LineStyle::Thin));
    graphic.labels.push(label(
        ctx.paper([node.centre[0], geom.thickness + 12.]),
        format!("защитный слой {}", mm(cover)),
        TEXT_HEIGHT,
        Align::Middle,
    ));
    let mut notes = Vec::new();
    push_role(&mut notes, sets, rows, ZoneRole::BottomMain, "низ");
    push_role(&mut notes, sets, rows, ZoneRole::BottomCross, "низ");
    push_role(&mut notes, sets, rows, ZoneRole::TopMain, "верх");
    push_role(&mut notes, sets, rows, ZoneRole::TopCross, "верх");
    push_role(&mut notes, sets, rows, ZoneRole::Edge, "П");
    push_role(&mut notes, sets, rows, ZoneRole::Opening, "П");
    push_role(&mut notes, sets, rows, ZoneRole::Frog, "лягушка");
    notes.dedup();
    let end_x = if separate {
        ctx.paper([geom.max[0], 0.])[0] + shift
    } else {
        ctx.paper([u1, 0.])[0]
    };
    let mut y = ctx.paper([0., geom.thickness])[1];
    for text in notes {
        graphic
            .labels
            .push(label([end_x + 4., y], text, TEXT_HEIGHT, Align::Start));
        y -= 6.5;
    }
}

fn ranges(
    u0: f64,
    u1: f64,
    right_u0: f64,
    max_x: f64,
    separate: bool,
    shift: f64,
) -> Vec<((f64, f64), f64)> {
    let mut out = vec![((u0, u1), 0.)];
    if separate {
        out.push(((right_u0, max_x), shift));
    }
    out
}

fn draw_fragment(
    graphic: &mut ViewGraphic,
    ctx: &ViewContext,
    geom: &SlabGeom,
    u0: f64,
    shift: f64,
) {
    let corner = |u: f64, v: f64| {
        let mut p = ctx.paper([u, v]);
        p[0] += shift;
        p
    };
    let rect = [
        corner(u0, 0.),
        corner(geom.max[0], 0.),
        corner(geom.max[0], geom.thickness),
        corner(u0, geom.thickness),
        corner(u0, 0.),
    ];
    graphic.lines.push(poly(rect.to_vec(), LineStyle::Section));
    let step = 2. * std::f64::consts::SQRT_2;
    let (x0, y0) = (rect[0][0], rect[0][1]);
    let (x1, y1) = (rect[2][0], rect[2][1]);
    let mut o = y0 - x1;
    let end = y1 - x0;
    while o < end {
        if let Some((a, b)) = clip_diag(o, x0, y0, x1, y1) {
            graphic.lines.push(poly(vec![a, b], LineStyle::Hatch));
        }
        o += step;
    }
}

fn clip_diag(o: f64, x0: f64, y0: f64, x1: f64, y1: f64) -> Option<([f64; 2], [f64; 2])> {
    let mut pts = Vec::new();
    let push = |pts: &mut Vec<[f64; 2]>, p: [f64; 2]| {
        if pts.iter().all(|q| (q[0] - p[0]).hypot(q[1] - p[1]) > 0.2) {
            pts.push(p);
        }
    };
    let y = x0 + o;
    if (y0..=y1).contains(&y) {
        push(&mut pts, [x0, y]);
    }
    let y = x1 + o;
    if (y0..=y1).contains(&y) {
        push(&mut pts, [x1, y]);
    }
    let x = y0 - o;
    if (x0..=x1).contains(&x) {
        push(&mut pts, [x, y0]);
    }
    let x = y1 - o;
    if (x0..=x1).contains(&x) {
        push(&mut pts, [x, y1]);
    }
    (pts.len() >= 2).then(|| (pts[0], pts[1]))
}

fn draw_break(ctx: &ViewContext, graphic: &mut ViewGraphic, u1: f64, thickness: f64) {
    let x = ctx.paper([u1, 0.])[0] + BREAK_GAP / 2.;
    let y0 = ctx.paper([0., 0.])[1];
    let y1 = ctx.paper([0., thickness])[1];
    let mid = (y0 + y1) / 2.;
    graphic.lines.push(poly(
        vec![
            [x, y0],
            [x, mid - 3.],
            [x - 2., mid - 1.],
            [x + 2., mid + 1.],
            [x, mid + 3.],
            [x, y1],
        ],
        LineStyle::Thin,
    ));
}

fn clip_u(pts: &[[f64; 3]], u0: f64, u1: f64) -> Vec<Vec<[f64; 2]>> {
    let inside = |u: f64| (u0 - 0.5..=u1 + 0.5).contains(&u);
    let cross = |a: [f64; 3], b: [f64; 3], edge: f64| -> [f64; 2] {
        let t = (edge - a[0]) / (b[0] - a[0]);
        [edge, a[1] + (b[1] - a[1]) * t]
    };
    let mut runs = Vec::new();
    let mut run: Vec<[f64; 2]> = Vec::new();
    for w in pts.windows(2) {
        let (a_in, b_in) = (inside(w[0][0]), inside(w[1][0]));
        if a_in && b_in {
            if run.is_empty() {
                run.push([w[0][0], w[0][1]]);
            }
            run.push([w[1][0], w[1][1]]);
        } else if a_in && !b_in {
            if run.is_empty() {
                run.push([w[0][0], w[0][1]]);
            }
            let edge = if w[1][0] > u1 { u1 } else { u0 };
            run.push(cross(w[0], w[1], edge));
            if run.len() >= 2 {
                runs.push(std::mem::take(&mut run));
            }
        } else if !a_in && b_in {
            let edge = if w[0][0] < u0 { u0 } else { u1 };
            run = vec![cross(w[0], w[1], edge), [w[1][0], w[1][1]]];
        } else if (w[0][0] < u0 && w[1][0] > u1) || (w[1][0] < u0 && w[0][0] > u1) {
            let (e0, e1) = if w[0][0] < w[1][0] {
                (u0, u1)
            } else {
                (u1, u0)
            };
            runs.push(vec![cross(w[0], w[1], e0), cross(w[0], w[1], e1)]);
        }
    }
    if run.len() >= 2 {
        runs.push(run);
    }
    runs
}

fn push_role(
    out: &mut Vec<String>,
    sets: &[&RebarSet],
    rows: &[DetailRow],
    role: ZoneRole,
    name: &str,
) {
    for set in sets.iter().filter(|s| s.role == Some(role)) {
        out.push(format!("{name} {}", callout_of(set, rows)));
    }
}

pub(crate) fn callout_of(set: &RebarSet, rows: &[DetailRow]) -> String {
    let mut pos: Vec<u32> = rows
        .iter()
        .filter(|r| r.sets.contains(&set.number))
        .map(|r| r.mark)
        .collect();
    if pos.is_empty() {
        pos.extend(set.position);
    }
    pos.sort();
    pos.dedup();
    let list = if pos.is_empty() {
        "—".into()
    } else {
        pos.iter()
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let step = set
        .rule
        .as_ref()
        .map(|r| format!(" шаг {}", mm(r.spacing)))
        .unwrap_or_default();
    format!("поз. {list} Ø{} {}{step}", mm(set.diameter), set.grade)
}

pub(crate) fn host_rows(project: &Project, part: ObjectId) -> Vec<DetailRow> {
    rebar::detail_schedule(project)
        .into_iter()
        .filter_map(|mut row| {
            let count = project
                .concrete
                .rebars
                .iter()
                .filter(|s| row.sets.contains(&s.number) && s.host == Some(part))
                .map(|s| s.bars.len())
                .sum::<usize>();
            if count == 0 {
                return None;
            }
            row.total_mass_kg = row.unit_mass_kg * count as f64;
            row.count = count;
            Some(row)
        })
        .collect()
}

fn hosted(project: &Project, part: ObjectId) -> Vec<&RebarSet> {
    project
        .concrete
        .rebars
        .iter()
        .filter(|s| s.host == Some(part))
        .collect()
}

fn portions(row: &DetailRow) -> String {
    let mut out = Vec::new();
    for seg in &row.segments {
        let text = format!("{} {}", seg.letter, mm(seg.length_mm));
        if out.last() != Some(&text) {
            out.push(text);
        }
    }
    out.join(" ")
}

fn slab_geom(project: &Project, part: ObjectId) -> Result<SlabGeom, Error> {
    let element = project
        .elements
        .get(&part)
        .ok_or_else(|| format!("Деталь #{} не найдена", part.0))?;
    let ElementShape::Plate {
        contour,
        thickness,
        frame,
    } = &element.shape
    else {
        return Err("Чертёж КЖ строится по плите".into());
    };
    if contour.len() < 3 {
        return Err("Контур плиты не замкнут".into());
    }
    let thickness = thickness.value();
    let mut openings = Vec::new();
    for detail in project.details.values() {
        let DetailKind::PrismCut {
            part: host,
            frame: cut,
            contour: ring,
            length,
        } = &detail.kind
        else {
            continue;
        };
        if *host != part || *length + 1. < thickness || ring.len() < 3 {
            continue;
        }
        openings.push(
            ring.iter()
                .map(|p| {
                    let w = cut.world(Point3([p[0], p[1], 0.]));
                    let l = frame.local(w);
                    [l.0[0], l.0[1]]
                })
                .collect(),
        );
    }
    let (min, max) = contour
        .iter()
        .fold(([f64::MAX; 2], [f64::MIN; 2]), |(l, h), p| {
            (
                [l[0].min(p[0]), l[1].min(p[1])],
                [h[0].max(p[0]), h[1].max(p[1])],
            )
        });
    Ok(SlabGeom {
        frame: *frame,
        contour: contour.clone(),
        thickness,
        openings,
        min,
        max,
    })
}

fn mean_local_y(frame: &open_bim_model::Frame, bar: &[Point3]) -> Option<f64> {
    if bar.is_empty() {
        return None;
    }
    Some(bar.iter().map(|p| frame.local(*p).0[1]).sum::<f64>() / bar.len() as f64)
}

fn plan_paper(ctx: &ViewContext, world: [f64; 3]) -> [f64; 2] {
    let q = ctx.basis.project(world);
    ctx.paper([q[0], q[1]])
}

fn poly(points: Vec<[f64; 2]>, style: LineStyle) -> Polyline {
    Polyline {
        points,
        style,
        object: None,
    }
}

pub(crate) fn label(position: [f64; 2], text: String, height: f64, align: Align) -> Label {
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

pub(crate) fn mm(value: f64) -> String {
    crate::format_mm(value)
}

fn kg(value: f64) -> String {
    format!("{:.3}", value).replace('.', ",")
}
