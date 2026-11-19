//! КЖ sheet of one beam, column or wall. The elevation, the sections, the
//! detail schedule and the steel-consumption schedule are regenerated from
//! the model on every `generate`, on the same sheet, frame and title block
//! as the slab sheet. Nothing drawn is stored.
//!
//! ГОСТ 21.501-2018: elevation of the cage or the meshes, sections with the
//! cover, thick bar lines, position callouts. ГОСТ Р 21.101-2026: the sheet.
use std::collections::BTreeMap;

use open_bim_model::rebar::{self, DetailRow};
use open_bim_model::{
    Anchor, AnchorPoint, DimensionAxis, DimensionDef, Drawing, DrawingKind, DrawingView, Frame,
    NodeRegion, ObjectId, Point3, Project, RebarKind, RebarSet, SheetSize, ViewDirection,
};

use crate::projection::{Basis, ViewBasis};
use crate::rebar_drawing::{self, callout_of, host_rows, label, mm};
use crate::{
    Align, Error, Label, LineStyle, MARGIN, MARGIN_LEFT, Polyline, TEXT_HEIGHT, ViewContext,
    ViewGraphic,
};

const CALLOUT: f64 = 84.;
const NOTES_H: f64 = 64.;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    Beam,
    Column,
    Wall,
}

struct MemberBox {
    frame: Frame,
    width: f64,
    height: f64,
    length: f64,
}

struct Cut {
    direction: ViewDirection,
    depth: f64,
    node: Option<NodeRegion>,
    /// Model width and height of what the section shows, mm.
    size: [f64; 2],
}

struct Plan {
    sheet: SheetSize,
    scale: f64,
    section_scale: f64,
    elev: [f64; 2],
    sections: Vec<[f64; 2]>,
}

/// A new КЖ sheet for one beam, column or wall.
pub fn member_drawing(
    project: &Project,
    id: ObjectId,
    part: ObjectId,
    number: &str,
) -> Result<Drawing, Error> {
    let class = class_of(project, part)?;
    let (frame, width, height, length) = rebar::rect_member(project, part)
        .map_err(|_| "Чертёж КЖ строится по прямоугольной балке, колонне или стене".to_string())?;
    let member = MemberBox {
        frame,
        width,
        height,
        length,
    };
    if hosted(project, part).is_empty() {
        return Err("У детали нет арматуры".into());
    }
    let kind = DrawingKind::Reinforcement { part };
    let basis = crate::drawing_basis(project, &kind)?;
    let cuts = cuts_of(project, part, class, &member, &basis)?;
    let elev_size = elevation_size(class, member.width, member.height, member.length);
    let sec_sizes: Vec<[f64; 2]> = cuts.iter().map(|c| c.size).collect();
    let plan = fit(project, part, elev_size, &sec_sizes)?;
    let name = project
        .elements
        .get(&part)
        .map(|e| e.name.clone())
        .unwrap_or_else(|| "элемент".into());
    let mut views = vec![view(
        1,
        "Фасад",
        ViewDirection::Front,
        plan.elev,
        Some(plan.scale),
    )];
    for (i, cut) in cuts.iter().enumerate() {
        let mut v = view(
            (i + 2) as u32,
            &format!("{}-{}", i + 1, i + 1),
            cut.direction,
            plan.sections[i],
            Some(plan.section_scale),
        );
        v.section = Some(cut.depth);
        v.section_host = Some(1);
        v.node = cut.node;
        views.push(v);
    }
    let axis = if class == Class::Column {
        DimensionAxis::Vertical
    } else {
        DimensionAxis::Horizontal
    };
    let dimensions = vec![DimensionDef {
        id: 1,
        view: 1,
        anchors: vec![
            Anchor {
                object: part,
                point: AnchorPoint::Start,
            },
            Anchor {
                object: part,
                point: AnchorPoint::End,
            },
        ],
        axis,
        offset: if class == Class::Column { 16. } else { -16. },
    }];
    Ok(crate::updated(
        project,
        &Drawing {
            id,
            kind,
            number: number.into(),
            title: format!("Армирование {name}"),
            sheet: plan.sheet,
            scale: plan.scale,
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

fn class_of(project: &Project, part: ObjectId) -> Result<Class, Error> {
    match project.concrete_type(part) {
        Some("beam") => Ok(Class::Beam),
        Some("column") => Ok(Class::Column),
        Some("panel") => Ok(Class::Wall),
        _ => Err("Чертёж КЖ строится по плите, балке, колонне или стене".into()),
    }
}

fn elevation_size(class: Class, width: f64, height: f64, length: f64) -> [f64; 2] {
    match class {
        Class::Beam => [length, height],
        Class::Column => [width, length],
        Class::Wall => {
            let tall = width.max(height);
            [length, tall]
        }
    }
}

fn cuts_of(
    project: &Project,
    part: ObjectId,
    class: Class,
    member: &MemberBox,
    basis: &Basis,
) -> Result<Vec<Cut>, Error> {
    let MemberBox {
        frame,
        width,
        height,
        length,
    } = member;
    let at = |local: [f64; 3]| frame.world(Point3(local)).0;
    match class {
        Class::Beam => {
            let support = end_zone(project, part).unwrap_or(length * 0.15);
            let z0 = (support * 0.5).clamp(40., length * 0.4);
            Ok(vec![
                plain_cut(
                    basis,
                    ViewDirection::Left,
                    at([0., 0., z0]),
                    [*width, *height],
                ),
                plain_cut(
                    basis,
                    ViewDirection::Left,
                    at([0., 0., length * 0.5]),
                    [*width, *height],
                ),
            ])
        }
        Class::Column => Ok(vec![
            plain_cut(
                basis,
                ViewDirection::Top,
                at([0., 0., length * 0.5]),
                [*width, *height],
            ),
            plain_cut(
                basis,
                ViewDirection::Top,
                at([0., 0., (length - 30.).max(length * 0.85)]),
                [*width, *height],
            ),
        ]),
        Class::Wall => wall_cuts(project, part, basis),
    }
}

fn plain_cut(basis: &Basis, direction: ViewDirection, world: [f64; 3], size: [f64; 2]) -> Cut {
    Cut {
        direction,
        depth: depth_at(basis, direction, world),
        node: None,
        size,
    }
}

fn wall_cuts(project: &Project, part: ObjectId, basis: &Basis) -> Result<Vec<Cut>, Error> {
    let host = rebar::area_host(project, part)?;
    let ring = host.openings.iter().max_by(|a, b| {
        ring_span(a)
            .0
            .hypot(ring_span(a).1)
            .total_cmp(&ring_span(b).0.hypot(ring_span(b).1))
    });
    let points = match ring {
        Some(ring) => face_points(&host.frame, host.thickness, ring),
        None => {
            let (min, max) = ring_bounds(&host.contour);
            let (cx, cy) = ((min[0] + max[0]) / 2., (min[1] + max[1]) / 2.);
            let sample = [
                [cx - 300., cy - 300.],
                [cx + 300., cy - 300.],
                [cx + 300., cy + 300.],
                [cx - 300., cy + 300.],
            ];
            face_points(&host.frame, host.thickness, &sample)
        }
    };
    let mut out = Vec::new();
    for direction in [ViewDirection::Top, ViewDirection::Left] {
        let node = node_around(basis, direction, &points, 80.);
        let side = node.radius * 2.;
        let centre = points
            .iter()
            .fold([0.; 3], |s, p| [s[0] + p[0], s[1] + p[1], s[2] + p[2]]);
        let n = points.len() as f64;
        let world = [centre[0] / n, centre[1] / n, centre[2] / n];
        out.push(Cut {
            direction,
            depth: depth_at(basis, direction, world),
            node: Some(node),
            size: [side, side],
        });
    }
    Ok(out)
}

fn face_points(frame: &Frame, thickness: f64, ring: &[[f64; 2]]) -> Vec<[f64; 3]> {
    ring.iter()
        .flat_map(|p| {
            [0., thickness]
                .into_iter()
                .map(|z| frame.world(Point3([p[0], p[1], z])).0)
        })
        .collect()
}

fn ring_bounds(ring: &[[f64; 2]]) -> ([f64; 2], [f64; 2]) {
    ring.iter()
        .fold(([f64::MAX; 2], [f64::MIN; 2]), |(l, h), p| {
            (
                [l[0].min(p[0]), l[1].min(p[1])],
                [h[0].max(p[0]), h[1].max(p[1])],
            )
        })
}

fn ring_span(ring: &[[f64; 2]]) -> (f64, f64) {
    let (l, h) = ring_bounds(ring);
    (h[0] - l[0], h[1] - l[1])
}

fn node_around(
    basis: &Basis,
    direction: ViewDirection,
    points: &[[f64; 3]],
    pad: f64,
) -> NodeRegion {
    let vb = ViewBasis::new(basis, direction);
    let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
    for p in points {
        let q = vb.project(*p);
        lo[0] = lo[0].min(q[0]);
        lo[1] = lo[1].min(q[1]);
        hi[0] = hi[0].max(q[0]);
        hi[1] = hi[1].max(q[1]);
    }
    if !lo[0].is_finite() {
        return NodeRegion {
            centre: [0., 0.],
            radius: 200.,
        };
    }
    let radius = ((hi[0] - lo[0]).max(hi[1] - lo[1]) / 2. + pad).clamp(140., 480.);
    NodeRegion {
        centre: [(lo[0] + hi[0]) / 2., (lo[1] + hi[1]) / 2.],
        radius,
    }
}

fn depth_at(basis: &Basis, direction: ViewDirection, world: [f64; 3]) -> f64 {
    ViewBasis::new(basis, direction).project(world)[2]
}

fn end_zone(project: &Project, part: ObjectId) -> Option<f64> {
    let cage = project.concrete.cages.iter().find(|c| c.host == part)?;
    let ty = project
        .concrete
        .cage_types
        .iter()
        .find(|t| t.id == cage.type_id)?;
    ty.end_length.filter(|l| *l > 1.)
}

fn fit(
    project: &Project,
    part: ObjectId,
    elev: [f64; 2],
    sections: &[[f64; 2]],
) -> Result<Plan, Error> {
    let table_w = rebar_drawing::schedule_width(project, part);
    for sheet in [SheetSize::A3, SheetSize::A2, SheetSize::A1, SheetSize::A0] {
        let [sw, sh] = sheet.landscape_mm();
        let left = MARGIN_LEFT + 6.;
        let right = sw - MARGIN - table_w - 6.;
        let top = sh - MARGIN - 6.;
        let bottom = MARGIN + NOTES_H;
        let field_w = right - left;
        let field_h = top - bottom;
        if field_w < 80. || field_h < 80. {
            continue;
        }
        for &scale in crate::SCALES.iter().rev() {
            for &section_scale in crate::SCALES.iter() {
                let ew = elev[0] / scale;
                let eh = elev[1] / scale;
                if ew < 36. || eh < 18. || ew > field_w - 4. {
                    continue;
                }
                let drawn: Vec<[f64; 2]> = sections
                    .iter()
                    .map(|s| [s[0] / section_scale, s[1] / section_scale])
                    .collect();
                if drawn.iter().any(|s| s[0] < 26. || s[1] < 26.) {
                    continue;
                }
                let stack_w = drawn.iter().map(|s| s[0]).fold(0., f64::max);
                let stack_h: f64 = drawn.iter().map(|s| s[1] + 20.).sum::<f64>() + 4.;
                let side =
                    ew + CALLOUT + 8. + stack_w + CALLOUT <= field_w && eh.max(stack_h) <= field_h;
                let below = ew.max(stack_w) + CALLOUT <= field_w && eh + 18. + stack_h <= field_h;
                if !side && !below {
                    continue;
                }
                let elev_at = [left + ew / 2., top - eh / 2.];
                let mut sections_at = Vec::new();
                if side {
                    let x = left + ew + CALLOUT + 8.;
                    let mut y = top;
                    for s in &drawn {
                        sections_at.push([x + s[0] / 2., y - s[1] / 2.]);
                        y -= s[1] + 20.;
                    }
                } else {
                    let mut y = elev_at[1] - eh / 2. - 22.;
                    for s in &drawn {
                        y -= s[1] / 2.;
                        sections_at.push([left + s[0] / 2., y]);
                        y -= s[1] / 2. + 20.;
                    }
                }
                return Ok(Plan {
                    sheet,
                    scale,
                    section_scale,
                    elev: elev_at,
                    sections: sections_at,
                });
            }
        }
    }
    Err("Элемент не помещается на лист A0".into())
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

/// Bars, stirrup-zone steps, opening marks and position callouts.
pub(crate) fn annotate(
    project: &Project,
    drawing: &Drawing,
    part: ObjectId,
    contexts: &BTreeMap<u32, ViewContext>,
    views: &mut [ViewGraphic],
) {
    let Ok(class) = class_of(project, part) else {
        return;
    };
    let Ok((frame, _, _, _)) = rebar::rect_member(project, part) else {
        return;
    };
    let sets = hosted(project, part);
    let rows = host_rows(project, part);
    let wall = class == Class::Wall;
    for view in &drawing.views {
        let Some(ctx) = contexts.get(&view.id) else {
            continue;
        };
        let Some(graphic) = views.iter_mut().find(|g| g.id == view.id) else {
            continue;
        };
        if view.section.is_none() {
            draw_elevation(graphic, ctx, &sets, wall);
            if !wall {
                draw_steps(graphic, ctx, &frame, &sets, class == Class::Column);
            } else {
                draw_openings(project, part, graphic, ctx);
            }
            let notes = elev_callouts(&sets, &rows, wall);
            place_callouts(graphic, ctx, &notes);
        } else {
            draw_cut(graphic, ctx, view, &sets);
            draw_cover(graphic, ctx, project, part, view);
            let notes = elev_callouts(&sets, &rows, wall);
            place_callouts(graphic, ctx, &notes);
        }
    }
}

pub(crate) fn schedules_and_notes(
    project: &Project,
    part: ObjectId,
    size: [f64; 2],
    title_area: [f64; 4],
) -> (Vec<Polyline>, Vec<Label>) {
    let mut pair = rebar_drawing::paint_schedules(project, part, size, title_area);
    pair.1.extend(notes(project, part));
    pair
}

fn notes(project: &Project, part: ObjectId) -> Vec<Label> {
    let class = class_of(project, part).ok();
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
    let find = |kind: RebarKind| sets.iter().copied().find(|s| s.kind == kind);
    let long = find(RebarKind::Longitudinal);
    let stirrup = find(RebarKind::Stirrup);
    let step = |set: Option<&RebarSet>| {
        set.and_then(|s| s.rule.as_ref())
            .map(|r| mm(r.spacing))
            .unwrap_or_else(|| "—".into())
    };
    let mesh = |role| {
        sets.iter()
            .find(|s| s.role == Some(role))
            .map(|s| format!("Ø{} {} шаг {}", mm(s.diameter), s.grade, step(Some(s))))
    };
    use open_bim_model::concrete::ZoneRole;
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
    ];
    match class {
        Some(Class::Beam) | Some(Class::Column) => {
            lines.push(format!(
                "Продольные: {}.",
                long.map(|s| format!("Ø{} {} ", mm(s.diameter), s.grade))
                    .unwrap_or_else(|| "нет ".into())
            ));
            let ends = end_zone(project, part)
                .map(|l| format!(", у опор шаг на {} мм", mm(l)))
                .unwrap_or_default();
            let stirrup_mark = stirrup
                .map(|s| format!("Ø{} {}", mm(s.diameter), s.grade))
                .unwrap_or_else(|| "нет".into());
            let stirrup_step = step(stirrup);
            lines.push(format!("Хомуты: {stirrup_mark} шаг {stirrup_step}{ends}."));
            if sets.iter().any(|s| s.kind == RebarKind::Starter) {
                lines.push("Выпуски — длина нахлёстки по СП 63.13330.".into());
            }
        }
        Some(Class::Wall) => {
            lines.push(format!(
                "Ближняя грань: вертикальные {}, горизонтальные {}.",
                mesh(ZoneRole::BottomMain).unwrap_or_else(|| "нет".into()),
                mesh(ZoneRole::BottomCross).unwrap_or_else(|| "нет".into())
            ));
            lines.push(format!(
                "Дальняя грань: вертикальные {}, горизонтальные {}.",
                mesh(ZoneRole::TopMain).unwrap_or_else(|| "нет".into()),
                mesh(ZoneRole::TopCross).unwrap_or_else(|| "нет".into())
            ));
            lines.push(format!(
                "П-стержни по свободным кромкам и проёмам, шаг {} мм.",
                step(
                    sets.iter()
                        .copied()
                        .find(|s| s.role == Some(ZoneRole::Edge))
                )
            ));
        }
        None => {}
    }
    lines.push("Анкеровка и нахлёст — по СП 63.13330.".into());
    lines.push("Длины заготовок и масса — в ведомостях на этом листе.".into());
    let count = lines.len();
    lines
        .into_iter()
        .enumerate()
        .map(|(i, text)| {
            label(
                [MARGIN_LEFT + 2., MARGIN + 4. + (count - 1 - i) as f64 * 5.2],
                text,
                TEXT_HEIGHT,
                Align::Start,
            )
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

fn draw_elevation(graphic: &mut ViewGraphic, ctx: &ViewContext, sets: &[&RebarSet], wall: bool) {
    for set in sets {
        let far = wall
            && matches!(
                set.role,
                Some(
                    open_bim_model::concrete::ZoneRole::TopMain
                        | open_bim_model::concrete::ZoneRole::TopCross
                )
            );
        for bar in &set.bars {
            let pts: Vec<[f64; 2]> = bar
                .iter()
                .map(|p| {
                    let mut q = paper(ctx, p.0);
                    if far {
                        q[1] += 1.2;
                    }
                    q
                })
                .collect();
            if pts.len() >= 2 {
                graphic.lines.push(poly(pts, LineStyle::Visible));
            }
        }
    }
}

fn draw_steps(
    graphic: &mut ViewGraphic,
    ctx: &ViewContext,
    frame: &Frame,
    sets: &[&RebarSet],
    vertical: bool,
) {
    let Some(stirrup) = sets.iter().find(|s| s.kind == RebarKind::Stirrup) else {
        return;
    };
    let mut stations = Vec::new();
    for bar in &stirrup.bars {
        let zs: Vec<f64> = bar.iter().map(|p| frame.local(*p).0[2]).collect();
        let (lo, hi) = zs
            .iter()
            .fold((f64::MAX, f64::MIN), |(l, h), z| (l.min(*z), h.max(*z)));
        if hi - lo < 80. {
            stations.push((lo + hi) / 2.);
        }
    }
    stations.sort_by(f64::total_cmp);
    stations.dedup_by(|a, b| (*a - *b).abs() < 1.);
    if stations.len() < 2 {
        return;
    }
    let mut i = 0;
    let mut band = 0;
    while i + 1 < stations.len() {
        let step = stations[i + 1] - stations[i];
        let mut j = i + 1;
        while j + 1 < stations.len() {
            let next = stations[j + 1] - stations[j];
            if (next - step).abs() > step * 0.25 + 15. {
                break;
            }
            j += 1;
        }
        let mid = (stations[i] + stations[j]) / 2.;
        let world = frame.world(Point3([0., 0., mid])).0;
        let at = paper(ctx, world);
        let pos = if vertical {
            [graphic.bounds[1][0] + 2., at[1] + band as f64]
        } else {
            [at[0], graphic.bounds[1][1] + 4. + band as f64 * 5.]
        };
        graphic.labels.push(label(
            pos,
            format!("шаг {}", mm(step)),
            TEXT_HEIGHT,
            Align::Start,
        ));
        band += 1;
        i = j;
    }
}

fn draw_openings(project: &Project, part: ObjectId, graphic: &mut ViewGraphic, ctx: &ViewContext) {
    let Ok(host) = rebar::area_host(project, part) else {
        return;
    };
    for ring in &host.openings {
        let (min, max) = ring_bounds(ring);
        let centre = host
            .frame
            .world(Point3([(min[0] + max[0]) / 2., (min[1] + max[1]) / 2., 0.]))
            .0;
        let at = paper(ctx, centre);
        graphic.labels.push(label(
            [at[0], at[1] + 3.],
            format!("проём {}×{}", mm(max[0] - min[0]), mm(max[1] - min[1])),
            TEXT_HEIGHT,
            Align::Middle,
        ));
    }
}

fn elev_callouts(sets: &[&RebarSet], rows: &[DetailRow], wall: bool) -> Vec<(String, [f64; 3])> {
    let mut out = Vec::new();
    for set in sets {
        let body = callout_of(set, rows);
        let text = match (wall, set.kind, set.role) {
            (
                true,
                _,
                Some(
                    open_bim_model::concrete::ZoneRole::BottomMain
                    | open_bim_model::concrete::ZoneRole::BottomCross,
                ),
            ) => {
                format!("ближн. {body}")
            }
            (
                true,
                _,
                Some(
                    open_bim_model::concrete::ZoneRole::TopMain
                    | open_bim_model::concrete::ZoneRole::TopCross,
                ),
            ) => {
                format!("дальн. {body}")
            }
            (
                _,
                _,
                Some(
                    open_bim_model::concrete::ZoneRole::Edge
                    | open_bim_model::concrete::ZoneRole::Opening,
                ),
            ) => {
                format!("П {body}")
            }
            (_, RebarKind::Longitudinal, _) => format!("прод. {body}"),
            (_, RebarKind::Stirrup, _) => format!("хомут {body}"),
            (_, RebarKind::Starter, _) => format!("выпуск {body}"),
            _ => body,
        };
        if out.iter().any(|(t, _)| t == &text) {
            continue;
        }
        let at = set
            .bars
            .first()
            .and_then(|b| b.get(b.len() / 2))
            .map(|p| p.0)
            .unwrap_or([0.; 3]);
        out.push((text, at));
    }
    out
}

fn place_callouts(graphic: &mut ViewGraphic, ctx: &ViewContext, notes: &[(String, [f64; 3])]) {
    let x = graphic.bounds[1][0] + 4.;
    let mut y = graphic.bounds[1][1];
    for (text, world) in notes {
        let at = paper(ctx, *world);
        graphic
            .lines
            .push(poly(vec![at, [x - 1., y + 1.]], LineStyle::Thin));
        graphic
            .labels
            .push(label([x, y], text.clone(), TEXT_HEIGHT, Align::Start));
        y -= 5.4;
    }
}

fn draw_cut(graphic: &mut ViewGraphic, ctx: &ViewContext, view: &DrawingView, sets: &[&RebarSet]) {
    let Some(depth) = view.section else { return };
    for set in sets {
        let tol = set.rule.as_ref().map(|r| r.spacing).unwrap_or(80.) / 2. + set.diameter + 8.;
        for bar in &set.bars {
            let pts: Vec<[f64; 3]> = bar.iter().map(|p| ctx.basis.project(p.0)).collect();
            if pts.len() < 2 {
                continue;
            }
            let (d0, d1) = pts
                .iter()
                .fold((f64::MAX, f64::MIN), |(l, h), p| (l.min(p[2]), h.max(p[2])));
            if d1 - d0 < 50. {
                let mean = pts.iter().map(|p| p[2]).sum::<f64>() / pts.len() as f64;
                if (mean - depth).abs() > tol {
                    continue;
                }
                let uv: Vec<[f64; 2]> = pts.iter().map(|p| [p[0], p[1]]).collect();
                for run in clip_node(&uv, view.node) {
                    if run.len() >= 2 {
                        graphic.lines.push(poly(
                            run.into_iter().map(|p| ctx.paper(p)).collect(),
                            LineStyle::Visible,
                        ));
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
                    let uv = [
                        w[0][0] + (w[1][0] - w[0][0]) * t,
                        w[0][1] + (w[1][1] - w[0][1]) * t,
                    ];
                    if !inside(uv, view.node) {
                        continue;
                    }
                    let at = ctx.paper(uv);
                    let r = (set.diameter / 2. / ctx.scale).clamp(0.7, 2.4);
                    graphic
                        .lines
                        .push(crate::conventions::circle(at, r, LineStyle::Visible));
                }
            }
        }
    }
}

fn draw_cover(
    graphic: &mut ViewGraphic,
    ctx: &ViewContext,
    project: &Project,
    part: ObjectId,
    view: &DrawingView,
) {
    let Ok((frame, width, height, length)) = rebar::rect_member(project, part) else {
        return;
    };
    let cover = rebar::host_cover(project, part);
    let class = class_of(project, part).unwrap_or(Class::Beam);
    let (a, b) = if class == Class::Wall {
        let Ok(host) = rebar::area_host(project, part) else {
            return;
        };
        let c = if let Some(ring) = host.openings.first() {
            let (min, max) = ring_bounds(ring);
            [(min[0] + max[0]) / 2., (min[1] + max[1]) / 2.]
        } else {
            let (min, max) = ring_bounds(&host.contour);
            [(min[0] + max[0]) / 2., (min[1] + max[1]) / 2.]
        };
        let near = host.frame.world(Point3([c[0], c[1], 0.])).0;
        let far = host.frame.world(Point3([c[0], c[1], host.thickness])).0;
        let (na, fa) = (
            {
                let q = ctx.basis.project(near);
                [q[0], q[1]]
            },
            {
                let q = ctx.basis.project(far);
                [q[0], q[1]]
            },
        );
        let (du, dv) = (fa[0] - na[0], fa[1] - na[1]);
        if dv.abs() >= du.abs() {
            let low = if na[1] <= fa[1] { na } else { fa };
            (low, [low[0], low[1] + cover])
        } else {
            let low = if na[0] <= fa[0] { na } else { fa };
            (low, [low[0] + cover, low[1]])
        }
    } else {
        let z = length * 0.5;
        let corner = |x: f64, y: f64| {
            let q = ctx.basis.project(frame.world(Point3([x, y, z])).0);
            [q[0], q[1]]
        };
        let pts = [
            corner(-width / 2., -height / 2.),
            corner(width / 2., -height / 2.),
            corner(width / 2., height / 2.),
            corner(-width / 2., height / 2.),
        ];
        let bottom = pts
            .into_iter()
            .min_by(|p, q| p[1].total_cmp(&q[1]))
            .unwrap_or([0., 0.]);
        (bottom, [bottom[0], bottom[1] + cover])
    };
    if !inside(a, view.node) && !inside(b, view.node) {
        graphic.labels.push(label(
            [graphic.bounds[1][0] + 2., graphic.bounds[1][1] + 2.],
            format!("защитный слой {}", mm(cover)),
            TEXT_HEIGHT,
            Align::Start,
        ));
        return;
    }
    let pa = ctx.paper(a);
    let pb = ctx.paper(b);
    graphic.lines.push(poly(vec![pa, pb], LineStyle::Thin));
    graphic.labels.push(label(
        [pb[0] + 2., pb[1] + 1.],
        format!("защитный слой {}", mm(cover)),
        TEXT_HEIGHT,
        Align::Start,
    ));
}

fn inside(uv: [f64; 2], node: Option<NodeRegion>) -> bool {
    let Some(n) = node else {
        return true;
    };
    (uv[0] - n.centre[0]).abs() <= n.radius + 2. && (uv[1] - n.centre[1]).abs() <= n.radius + 2.
}

fn clip_node(pts: &[[f64; 2]], node: Option<NodeRegion>) -> Vec<Vec<[f64; 2]>> {
    let Some(n) = node else {
        return vec![pts.to_vec()];
    };
    let (x0, x1) = (n.centre[0] - n.radius, n.centre[0] + n.radius);
    let (y0, y1) = (n.centre[1] - n.radius, n.centre[1] + n.radius);
    let inside = |p: [f64; 2]| (x0..=x1).contains(&p[0]) && (y0..=y1).contains(&p[1]);
    let mut runs = Vec::new();
    let mut run = Vec::new();
    for w in pts.windows(2) {
        let (a_in, b_in) = (inside(w[0]), inside(w[1]));
        if a_in && b_in {
            if run.is_empty() {
                run.push(w[0]);
            }
            run.push(w[1]);
        } else if a_in || b_in {
            if a_in && run.is_empty() {
                run.push(w[0]);
            }
            if run.len() >= 2 {
                runs.push(std::mem::take(&mut run));
            } else {
                run.clear();
            }
        }
    }
    if run.len() >= 2 {
        runs.push(run);
    }
    runs
}

fn paper(ctx: &ViewContext, world: [f64; 3]) -> [f64; 2] {
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
