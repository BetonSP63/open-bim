//! Drawing conventions of ГОСТ Р 21.101-2026 (5.3–5.5) and ГОСТ 2.307:
//! 45° ticks on dimension lines of КМ schemes (arrows stay on КМД product
//! drawings), level marks, coordination axes with bubbles, marks on leader
//! shelves, view titles and section lines with arrows.
use crate::{
    Align, Drawing, DrawingKind, DrawingView, Label, LineStyle, Polyline, ViewContext, ViewGraphic,
};
use open_bim_model::numbering_rules::{level_mark, plan_title};
use open_bim_model::{Project, ViewDirection};

/// Capital-letter height of the drawing font as a share of the label height
/// (the em size labels are drawn at).
pub const CAP_RATIO: f64 = 0.7;

/// Label height giving ГОСТ 2.304 font size `size` (capital-letter height).
pub const fn em(size: f64) -> f64 {
    size / CAP_RATIO
}

/// Dimension digits: font 2.5, the smallest ГОСТ 2.304 size.
pub const DIM_TEXT: f64 = em(2.5);
/// Axis labels, marks and view titles: font 5, twice the dimension digits (5.4.7).
pub const AXIS_TEXT: f64 = em(5.);
pub const MARK_TEXT: f64 = em(5.);
pub const TITLE_TEXT: f64 = em(5.);
/// Level values beside the level sign: font 3.5.
pub const LEVEL_TEXT: f64 = em(3.5);
/// Axis bubble diameter, 6–12 mm (5.3.2).
pub const BUBBLE: f64 = 10.;
/// Tick length, 2–4 mm, at 45° (5.4.2).
pub const TICK: f64 = 3.;
/// Dimension line beyond the outer extension lines, 0–3 mm (5.4.2).
pub const DIM_OVERSHOOT: f64 = 2.;
/// Level sign strokes, 2–4 mm at 45° (5.4.3).
pub const LEVEL_STROKE: f64 = 3.;

/// КМ arrangement drawings use ticks; КМД product drawings keep ГОСТ 2.307 arrows.
pub fn uses_ticks(drawing: &Drawing) -> bool {
    matches!(drawing.kind, DrawingKind::General { .. })
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

/// 45° tick through `at`, inclined to the right of the dimension line.
pub fn tick(at: [f64; 2], direction: [f64; 2]) -> Polyline {
    let n = [-direction[1], direction[0]];
    let s = std::f64::consts::FRAC_1_SQRT_2 * TICK / 2.;
    let t = [(direction[0] + n[0]) * s, (direction[1] + n[1]) * s];
    line(
        vec![[at[0] - t[0], at[1] - t[1]], [at[0] + t[0], at[1] + t[1]]],
        LineStyle::Visible,
    )
}

pub fn circle(c: [f64; 2], r: f64, style: LineStyle) -> Polyline {
    line(
        (0..=48)
            .map(|k| {
                let t = k as f64 / 48. * std::f64::consts::TAU;
                [c[0] + r * t.cos(), c[1] + r * t.sin()]
            })
            .collect(),
        style,
    )
}

fn bubble(graphic: &mut ViewGraphic, centre: [f64; 2], name: &str) {
    graphic
        .lines
        .push(circle(centre, BUBBLE / 2., LineStyle::Thin));
    graphic.labels.push(label(
        [centre[0], centre[1] - 0.35 * AXIS_TEXT],
        name.into(),
        AXIS_TEXT,
        Align::Middle,
    ));
}

/// Coordination axes on a КМ view: thin dash-dot lines with bubbles on the
/// left and bottom (5.3.2–5.3.4). Lines across the view in plans; in
/// elevations the axes seen end-on become vertical lines.
pub(crate) fn axes(
    project: &Project,
    view: &DrawingView,
    ctx: &ViewContext,
    graphic: &mut ViewGraphic,
    reach: [f64; 2],
) {
    let plan = view.direction == ViewDirection::Top;
    let [lo, hi] = graphic.bounds;
    let below = (lo[1] - 12.).min(reach[1] - 4.);
    for axis in project.axes.values() {
        let (a, b) = (
            ctx.basis.project(axis.start.0),
            ctx.basis.project(axis.end.0),
        );
        let (pa, pb) = (ctx.paper([a[0], a[1]]), ctx.paper([b[0], b[1]]));
        let length = (pb[0] - pa[0]).hypot(pb[1] - pa[1]);
        if plan && length > 1. {
            // In the view plane, clipped to the view with a 10 mm margin;
            // the bubble beyond its left or lower end.
            let margin = [
                [
                    (lo[0] - 10.).min(reach[0] - 4.),
                    (lo[1] - 10.).min(reach[1] - 4.),
                ],
                [hi[0] + 10., hi[1] + 10.],
            ];
            let Some((pa, pb)) = clip(pa, pb, margin) else {
                continue;
            };
            let horizontal = (pb[0] - pa[0]).abs() > (pb[1] - pa[1]).abs();
            let (end, other) = match (horizontal, pa[0] < pb[0], pa[1] < pb[1]) {
                (true, true, _) | (false, _, true) => (pa, pb),
                _ => (pb, pa),
            };
            let d = [(end[0] - other[0]) / length, (end[1] - other[1]) / length];
            graphic
                .lines
                .push(line(vec![other, end], LineStyle::DashDot));
            bubble(
                graphic,
                [end[0] + d[0] * BUBBLE / 2., end[1] + d[1] * BUBBLE / 2.],
                &axis.name,
            );
        } else if !plan && length <= 1. && (a[2] - b[2]).abs() > 1. {
            // Seen end-on: a vertical axis line through the view, bubble below.
            let x = pa[0];
            if x < lo[0] - 1. || x > hi[0] + 1. {
                continue;
            }
            let bottom = below;
            graphic
                .lines
                .push(line(vec![[x, hi[1] + 5.], [x, bottom]], LineStyle::DashDot));
            bubble(graphic, [x, bottom - BUBBLE / 2.], &axis.name);
        }
    }
}

/// Segment a–b clipped to a rectangle (Liang–Barsky).
fn clip(a: [f64; 2], b: [f64; 2], r: [[f64; 2]; 2]) -> Option<([f64; 2], [f64; 2])> {
    let d = [b[0] - a[0], b[1] - a[1]];
    let (mut t0, mut t1) = (0f64, 1f64);
    for (p, q) in [
        (-d[0], a[0] - r[0][0]),
        (d[0], r[1][0] - a[0]),
        (-d[1], a[1] - r[0][1]),
        (d[1], r[1][1] - a[1]),
    ] {
        if p.abs() < 1e-12 {
            if q < 0. {
                return None;
            }
        } else if p < 0. {
            t0 = t0.max(q / p);
        } else {
            t1 = t1.min(q / p);
        }
    }
    (t0 < t1).then(|| {
        (
            [a[0] + d[0] * t0, a[1] + d[1] * t0],
            [a[0] + d[0] * t1, a[1] + d[1] * t1],
        )
    })
}

/// Plan level shown on GA plans: the highest level.
pub fn plan_level(project: &Project) -> Option<f64> {
    project
        .levels
        .values()
        .map(|l| l.elevation_mm)
        .fold(None, |m: Option<f64>, z| Some(m.map_or(z, |m| m.max(z))))
}

/// Level marks (5.4.3): on elevations a 45° sign on an extension line right
/// of the view with the value on a shelf; on plans the value in a rectangle.
pub(crate) fn levels(
    project: &Project,
    view: &DrawingView,
    ctx: &ViewContext,
    graphic: &mut ViewGraphic,
) {
    let [lo, hi] = graphic.bounds;
    match view.direction {
        ViewDirection::Top => {
            if let Some(z) = plan_level(project) {
                let text = level_mark(z);
                let w = text.chars().count() as f64 * LEVEL_TEXT * 0.62 + 2.;
                // Above the upper-left corner, clear of the axis lines.
                let (x0, y0) = (lo[0], hi[1] + 12.);
                let (x1, y1) = (x0 + w, y0 + LEVEL_TEXT + 2.);
                graphic.lines.push(line(
                    vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1], [x0, y0]],
                    LineStyle::Thin,
                ));
                graphic
                    .labels
                    .push(label([x0 + 1., y0 + 1.], text, LEVEL_TEXT, Align::Start));
            }
        }
        _ => {
            let mut zs: Vec<f64> = project.levels.values().map(|l| l.elevation_mm).collect();
            zs.sort_by(f64::total_cmp);
            for (k, z) in zs.iter().enumerate() {
                let q = ctx.basis.project([0., 0., *z]);
                let y = ctx.paper([q[0], q[1]])[1];
                // Stagger the signs so close levels do not overlap.
                let x = hi[0] + 10. + 4. * (k % 2) as f64;
                graphic
                    .lines
                    .push(line(vec![[hi[0] + 1., y], [x + 4., y]], LineStyle::Thin));
                let s = LEVEL_STROKE * std::f64::consts::FRAC_1_SQRT_2;
                graphic.lines.push(line(
                    vec![[x - s, y + s], [x, y], [x + s, y + s]],
                    LineStyle::Thin,
                ));
                let shelf = y + 6.;
                let text = level_mark(*z);
                let w = text.chars().count() as f64 * LEVEL_TEXT * 0.62 + 1.;
                graphic.lines.push(line(
                    vec![[x, y], [x, shelf], [x + w, shelf]],
                    LineStyle::Thin,
                ));
                graphic.labels.push(label(
                    [x + 0.5, shelf + 0.8],
                    text,
                    LEVEL_TEXT,
                    Align::Start,
                ));
            }
        }
    }
}

/// Names of the outer axes an elevation shows, left to right.
fn outer_axes(project: &Project, ctx: &ViewContext) -> Option<(String, String)> {
    let mut seen: Vec<(f64, String)> = project
        .axes
        .values()
        .filter_map(|axis| {
            let (a, b) = (
                ctx.basis.project(axis.start.0),
                ctx.basis.project(axis.end.0),
            );
            ((a[0] - b[0]).hypot(a[1] - b[1]) < 1.).then(|| (a[0], axis.name.clone()))
        })
        .collect();
    seen.sort_by(|a, b| a.0.total_cmp(&b.0));
    Some((seen.first()?.1.clone(), seen.last()?.1.clone()))
}

/// Title of a КМ view (5.5.12–5.5.16): «План на отм. +3,600», «Фасад 1-5»,
/// «Разрез 1-1»; no scale on КМ drawings (5.1.6), never underlined.
pub(crate) fn ga_title(
    project: &Project,
    drawing: &Drawing,
    view: &DrawingView,
    ctx: &ViewContext,
) -> String {
    if view.section.is_some() {
        let n = section_number(drawing, view.id);
        return format!("Разрез {n}-{n}");
    }
    match view.direction {
        ViewDirection::Top => plan_level(project).map_or_else(|| "План".into(), plan_title),
        _ => match outer_axes(project, ctx) {
            Some((a, b)) => format!("Фасад {a}-{b}"),
            None => view.name.clone(),
        },
    }
}

/// Sections are numbered 1, 2, … in view order (5.5.2, 5.5.6).
pub fn section_number(drawing: &Drawing, view: u32) -> usize {
    drawing
        .views
        .iter()
        .filter(|v| v.section.is_some())
        .position(|v| v.id == view)
        .map_or(0, |i| i + 1)
}

/// Trace of each section view on the main view: thick strokes beyond the
/// view with arrows (viewing direction) 2.5 mm from their outer ends and the
/// section number (ГОСТ 2.305, ГОСТ Р 21.101 5.5.2).
pub(crate) fn section_traces(
    drawing: &Drawing,
    main: &ViewContext,
    sections: &[(&DrawingView, &ViewContext)],
    graphic: &mut ViewGraphic,
) {
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let [lo, hi] = graphic.bounds;
    for (view, ctx) in sections {
        let Some(depth) = view.section else { continue };
        let t = ctx.basis.toward;
        if dot(t, main.basis.toward).abs() > 1e-6 {
            continue;
        }
        let at = [
            ctx.basis.origin[0] + t[0] * depth,
            ctx.basis.origin[1] + t[1] * depth,
            ctx.basis.origin[2] + t[2] * depth,
        ];
        let q = main.basis.project(at);
        let on_plane = main.paper([q[0], q[1]]);
        let look = [-dot(t, main.basis.right), -dot(t, main.basis.up)];
        let along = [-look[1], look[0]];
        // The point of the trace line opposite the view centre.
        let mid = [(lo[0] + hi[0]) / 2., (lo[1] + hi[1]) / 2.];
        let shift = (mid[0] - on_plane[0]) * along[0] + (mid[1] - on_plane[1]) * along[1];
        let centre = [
            on_plane[0] + along[0] * shift,
            on_plane[1] + along[1] * shift,
        ];
        // Reach of the trace: beyond the view by 4 mm on both sides.
        let reach = [
            [lo[0], lo[1]],
            [hi[0], lo[1]],
            [hi[0], hi[1]],
            [lo[0], hi[1]],
        ]
        .iter()
        .map(|c| ((c[0] - centre[0]) * along[0] + (c[1] - centre[1]) * along[1]).abs())
        .fold(0., f64::max)
            + 4.;
        let n = section_number(drawing, view.id).to_string();
        for side in [-1., 1.] {
            let outer = [
                centre[0] + along[0] * side * (reach + 10.),
                centre[1] + along[1] * side * (reach + 10.),
            ];
            let inner = [
                centre[0] + along[0] * side * reach,
                centre[1] + along[1] * side * reach,
            ];
            graphic
                .lines
                .push(line(vec![inner, outer], LineStyle::Section));
            let base = [
                outer[0] - along[0] * side * 2.5,
                outer[1] - along[1] * side * 2.5,
            ];
            let tip = [base[0] + look[0] * 6., base[1] + look[1] * 6.];
            graphic.lines.push(line(vec![base, tip], LineStyle::Thin));
            graphic.lines.push(crate::arrow(tip, look));
            graphic.labels.push(label(
                [
                    tip[0] + look[0] * 1.5 + along[0] * side * 1.5,
                    tip[1] + look[1] * 1.5 + along[1] * side * 1.5 - 0.35 * MARK_TEXT,
                ],
                n.clone(),
                MARK_TEXT,
                Align::Middle,
            ));
        }
    }
}

/// Keep the lines of a node view inside its region (`graphic.bounds`),
/// clipping segments, and the texts placed inside it; the title stays.
pub(crate) fn keep_inside(graphic: &mut ViewGraphic) {
    let [lo, hi] = graphic.bounds;
    let mut lines = Vec::new();
    for l in &graphic.lines {
        let mut run: Vec<[f64; 2]> = Vec::new();
        for w in l.points.windows(2) {
            match clip(w[0], w[1], [lo, hi]) {
                Some((a, b)) => {
                    if run
                        .last()
                        .is_none_or(|p| (p[0] - a[0]).hypot(p[1] - a[1]) > 1e-9)
                    {
                        if run.len() >= 2 {
                            lines.push(Polyline {
                                points: std::mem::take(&mut run),
                                ..l.clone()
                            });
                        }
                        run = vec![a];
                    }
                    run.push(b);
                }
                None => {
                    if run.len() >= 2 {
                        lines.push(Polyline {
                            points: std::mem::take(&mut run),
                            ..l.clone()
                        });
                    }
                    run.clear();
                }
            }
        }
        if run.len() >= 2 {
            lines.push(Polyline {
                points: run,
                ..l.clone()
            });
        }
    }
    graphic.lines = lines;
    let inside = |p: [f64; 2]| p[0] >= lo[0] && p[0] <= hi[0] && p[1] >= lo[1] && p[1] <= hi[1];
    let title = graphic.labels.first().cloned();
    graphic.labels.retain(|l| inside(l.position));
    if let Some(t) = title
        && graphic.labels.first() != Some(&t)
    {
        graphic.labels.insert(0, t);
    }
}

/// Place of a node on the main view: a thin circle and the number above a
/// leader shelf (5.5.4).
pub(crate) fn node_mark(graphic: &mut ViewGraphic, centre: [f64; 2], radius: f64, number: usize) {
    graphic.lines.push(circle(centre, radius, LineStyle::Thin));
    let d = std::f64::consts::FRAC_1_SQRT_2;
    let p0 = [centre[0] + radius * d, centre[1] + radius * d];
    let p1 = [p0[0] + 6., p0[1] + 6.];
    let shelf = 1.2 * MARK_TEXT * crate::conventions::CAP_RATIO;
    graphic
        .lines
        .push(line(vec![p0, p1, [p1[0] + shelf, p1[1]]], LineStyle::Thin));
    graphic.labels.push(label(
        [p1[0] + shelf / 2., p1[1] + 0.8],
        number.to_string(),
        MARK_TEXT,
        Align::Middle,
    ));
}
