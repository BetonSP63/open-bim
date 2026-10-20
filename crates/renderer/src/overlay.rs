//! Tekla-like viewport cues computed as screen-space marks: background
//! gradient colours, model origin symbol, axis gizmo, work area box, dashed
//! grid with bubble labels and level labels, snap symbol, rubber band with a
//! live length label, and selection handles. Pure geometry: the desktop app
//! paints the marks with egui, tests check them against the camera.
use crate::{Bounds, Camera, DVec3};
use open_bim_model::{ElementShape, ObjectId, Point3, Project};
use std::collections::BTreeSet;

pub type Rgba = [u8; 4];

pub const WORK_AREA: Rgba = [0, 150, 60, 255];
pub const GRID: Rgba = [70, 100, 140, 255];
pub const LABEL: Rgba = [20, 40, 70, 255];
pub const SNAP: Rgba = [230, 120, 0, 255];
pub const RUBBER: Rgba = [200, 30, 160, 255];
pub const START_HANDLE: Rgba = [240, 200, 0, 255];
pub const END_HANDLE: Rgba = [210, 0, 200, 255];
/// Direct modification handles (Tekla: blue).
pub const DIRECT_HANDLE: Rgba = [30, 110, 230, 255];
/// Selected grid line.
pub const SELECTED_AXIS: Rgba = [255, 107, 20, 255];
/// Line the snapped point belongs to (Tekla: turquoise).
pub const SNAP_LINE: Rgba = [0, 185, 195, 255];
pub const AXIS_COLORS: [Rgba; 3] = [[210, 40, 40, 255], [20, 150, 50, 255], [30, 80, 220, 255]];
/// Light theme background: grey-blue at the top fading to near white.
pub const GRADIENT_LIGHT: [Rgba; 2] = [[150, 168, 196, 255], [236, 240, 245, 255]];
pub const GRADIENT_DARK: [Rgba; 2] = [[20, 28, 40, 255], [6, 9, 13, 255]];
/// At most this many selected objects get handles (bounded overlay cost).
pub const MAX_HANDLES: usize = 500;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyph {
    Reference,
    End,
    Mid,
    Intersection,
    Perpendicular,
    Nearest,
    Free,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Mark {
    Line {
        a: [f64; 2],
        b: [f64; 2],
        color: Rgba,
        width: f32,
        /// Dash and gap length in pixels; `None` = solid.
        dash: Option<[f32; 2]>,
    },
    Circle {
        center: [f64; 2],
        radius: f32,
        color: Rgba,
        fill: Option<Rgba>,
    },
    Square {
        center: [f64; 2],
        half: f32,
        color: Rgba,
        fill: bool,
    },
    /// Filled simple polygon (screen points), e.g. the plate preview.
    Fill { points: Vec<[f64; 2]>, color: Rgba },
    Text {
        at: [f64; 2],
        text: String,
        color: Rgba,
        /// Horizontal and vertical alignment: -1 left/top, 0 centre, 1 right/bottom.
        align: [i8; 2],
        size: f32,
    },
}

/// Line that represents a level in the views: the first grid axis (by ID)
/// raised to the level's elevation.
pub fn level_line(project: &Project, level: &open_bim_model::Level) -> Option<(DVec3, DVec3)> {
    let axis = project.axes.values().next()?;
    let (a, b) = (v(axis.start), v(axis.end));
    let dz = DVec3::Z * (level.elevation_mm - a.z);
    Some((a + dz, b + dz))
}

/// World → absolute screen pixels for a viewport rectangle.
#[derive(Clone, Copy)]
pub struct Projector<'a> {
    pub camera: &'a Camera,
    pub origin: DVec3,
    pub min: [f64; 2],
    pub size: [f64; 2],
}

impl Projector<'_> {
    pub fn screen(&self, p: DVec3) -> Option<[f64; 2]> {
        let uv = self
            .camera
            .project(self.origin, p, self.size[0] / self.size[1])?;
        Some([
            self.min[0] + uv[0] * self.size[0],
            self.min[1] + uv[1] * self.size[1],
        ])
    }
    fn inside(&self, p: [f64; 2], pad: f64) -> bool {
        p[0] >= self.min[0] - pad
            && p[1] >= self.min[1] - pad
            && p[0] <= self.min[0] + self.size[0] + pad
            && p[1] <= self.min[1] + self.size[1] + pad
    }
    /// Unit screen direction (y down) of a world direction at `at`, or `None`
    /// when it points (almost) at the viewer.
    fn direction(&self, at: DVec3, dir: DVec3) -> Option<[f64; 2]> {
        let a = self.screen(at)?;
        let step = self.camera.distance.max(1.0) * 1e-3;
        let b = self.screen(at + dir * step)?;
        let d = [b[0] - a[0], b[1] - a[1]];
        let len = (d[0] * d[0] + d[1] * d[1]).sqrt();
        (len > 1e-9).then(|| [d[0] / len, d[1] / len])
    }
}

/// Screen-plane components (x right, y down) of the world X, Y and Z axes in
/// the camera's view: unit length when an axis lies in the screen plane,
/// shorter as it turns towards the viewer.
pub fn gizmo_directions(camera: &Camera) -> [[f64; 2]; 3] {
    let forward = (camera.target - camera.eye()).normalize();
    let up_hint = if camera.pitch.cos().abs() < 1e-5 {
        DVec3::Y
    } else {
        DVec3::Z
    };
    let right = forward.cross(up_hint).normalize();
    let up = right.cross(forward);
    [DVec3::X, DVec3::Y, DVec3::Z].map(|a| [a.dot(right), -a.dot(up)])
}

/// Model extent like Tekla's work area: parts plus grid lines and levels.
/// `None` for an empty model without a grid.
pub fn model_bounds(scene: &crate::Scene, project: &Project) -> Option<Bounds> {
    let mut points: Vec<DVec3> = project
        .axes
        .values()
        .flat_map(|a| [v(a.start), v(a.end)])
        .collect();
    if !scene.instances.is_empty() {
        points.extend([scene.bounds.min, scene.bounds.max]);
    }
    let first = *points.first()?;
    let (mut min, mut max) = (first, first);
    for p in &points {
        min = min.min(*p);
        max = max.max(*p);
    }
    for level in project.levels.values() {
        min.z = min.z.min(level.elevation_mm);
        max.z = max.z.max(level.elevation_mm);
    }
    Some(Bounds { min, max })
}

/// Work area: model bounds expanded by `margin` mm on every side.
pub fn work_area(bounds: Bounds, margin: f64) -> Bounds {
    Bounds {
        min: bounds.min - DVec3::splat(margin),
        max: bounds.max + DVec3::splat(margin),
    }
}

pub fn box_edges(b: Bounds) -> [(DVec3, DVec3); 12] {
    let c = |i: usize| {
        DVec3::new(
            if i & 1 == 0 { b.min.x } else { b.max.x },
            if i & 2 == 0 { b.min.y } else { b.max.y },
            if i & 4 == 0 { b.min.z } else { b.max.z },
        )
    };
    [
        (0, 1),
        (2, 3),
        (4, 5),
        (6, 7),
        (0, 2),
        (1, 3),
        (4, 6),
        (5, 7),
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7),
    ]
    .map(|(i, j)| (c(i), c(j)))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandleKind {
    Start,
    End,
    Vertex(usize),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Handle {
    pub object: ObjectId,
    pub kind: HandleKind,
    pub world: DVec3,
    pub screen: [f64; 2],
}

fn v(p: Point3) -> DVec3 {
    DVec3::from_array(p.0)
}

/// Start/end handles of selected members and vertex handles of selected
/// plates, in screen space (bounded by [`MAX_HANDLES`] objects).
pub fn handles(project: &Project, selected: &BTreeSet<ObjectId>, p: &Projector) -> Vec<Handle> {
    let mut out = Vec::new();
    for id in selected.iter().take(MAX_HANDLES) {
        let Some(e) = project.elements.get(id) else {
            // Grid lines have start/end handles too.
            if let Some(axis) = project.axes.get(id) {
                for (kind, world) in [
                    (HandleKind::Start, v(axis.start)),
                    (HandleKind::End, v(axis.end)),
                ] {
                    if let Some(screen) = p.screen(world) {
                        out.push(Handle {
                            object: *id,
                            kind,
                            world,
                            screen,
                        });
                    }
                }
            }
            continue;
        };
        let frame = e.shape.frame();
        let points: Vec<(HandleKind, DVec3)> = match &e.shape {
            ElementShape::Member { length, .. } => match project.member_reference_line(&e.shape) {
                // Handles sit on the reference line, like Tekla.
                Some((start, end, _)) => {
                    vec![(HandleKind::Start, v(start)), (HandleKind::End, v(end))]
                }
                None => vec![
                    (HandleKind::Start, v(frame.origin)),
                    (
                        HandleKind::End,
                        v(frame.world(Point3([0., 0., length.value()]))),
                    ),
                ],
            },
            ElementShape::Plate { contour, .. } => contour
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    (
                        HandleKind::Vertex(i),
                        v(frame.world(Point3([c[0], c[1], 0.]))),
                    )
                })
                .collect(),
        };
        for (kind, world) in points {
            if let Some(screen) = p.screen(world) {
                out.push(Handle {
                    object: *id,
                    kind,
                    world,
                    screen,
                });
            }
        }
    }
    out
}

/// What the overlay shows this frame.
pub struct Input<'a> {
    pub project: &'a Project,
    pub hidden: &'a BTreeSet<ObjectId>,
    pub selected: &'a BTreeSet<ObjectId>,
    /// Work plane origin and axes (local x, y, z in world coordinates).
    pub workplane: (DVec3, [DVec3; 3]),
    pub work_area: Option<Bounds>,
    /// Points picked so far by the active command and whether it closes a polygon.
    pub picks: &'a [DVec3],
    pub polygon: bool,
    pub cursor: Option<(DVec3, Glyph)>,
    /// Snap kind name shown next to the cursor (Tekla snap tooltip).
    pub snap_label: Option<String>,
    /// Reference line or edge of the snapped point, highlighted.
    pub highlight: Option<(DVec3, DVec3)>,
    /// Temporary reference point (Ctrl+pick), drawn as a black cross.
    pub reference: Option<DVec3>,
    /// Direct modification on: handles are blue and draggable.
    pub direct: bool,
    /// Plate being picked: contour points (with the cursor) and whether the
    /// next point is accepted (red when it would cross the contour).
    pub plate_preview: Option<(Vec<DVec3>, bool)>,
    pub light: bool,
}

fn line(a: [f64; 2], b: [f64; 2], color: Rgba, width: f32, dash: Option<[f32; 2]>) -> Mark {
    Mark::Line {
        a,
        b,
        color,
        width,
        dash,
    }
}

fn text(at: [f64; 2], text: impl Into<String>, color: Rgba, align: [i8; 2], size: f32) -> Mark {
    Mark::Text {
        at,
        text: text.into(),
        color,
        align,
        size,
    }
}

fn add(a: [f64; 2], d: [f64; 2], k: f64) -> [f64; 2] {
    [a[0] + d[0] * k, a[1] + d[1] * k]
}

/// Snap symbol: one glyph per snap kind, drawn about `c`.
pub fn glyph(c: [f64; 2], g: Glyph) -> Vec<Mark> {
    let r = 7.0;
    let at = |dx: f64, dy: f64| [c[0] + dx, c[1] + dy];
    let s = |a, b| line(a, b, SNAP, 2., None);
    match g {
        Glyph::End => vec![Mark::Square {
            center: c,
            half: r as f32,
            color: SNAP,
            fill: false,
        }],
        Glyph::Mid => vec![
            s(at(-r, r * 0.7), at(r, r * 0.7)),
            s(at(r, r * 0.7), at(0., -r)),
            s(at(0., -r), at(-r, r * 0.7)),
        ],
        Glyph::Intersection => vec![s(at(-r, -r), at(r, r)), s(at(-r, r), at(r, -r))],
        Glyph::Perpendicular => vec![s(at(-r, r), at(r, r)), s(at(0., r), at(0., -r))],
        Glyph::Nearest => vec![
            s(at(-r, -r), at(r, -r)),
            s(at(r, -r), at(-r, r)),
            s(at(-r, r), at(r, r)),
            s(at(r, r), at(-r, -r)),
        ],
        Glyph::Reference => vec![
            Mark::Circle {
                center: c,
                radius: r as f32,
                color: SNAP,
                fill: None,
            },
            s(at(-r, 0.), at(r, 0.)),
            s(at(0., -r), at(0., r)),
        ],
        Glyph::Free => vec![s(at(-4., 0.), at(4., 0.)), s(at(0., -4.), at(0., 4.))],
    }
}

/// All marks for one frame, culled to the viewport.
pub fn build(p: &Projector, input: &Input) -> Vec<Mark> {
    let mut marks = Vec::new();
    let label = if input.light {
        LABEL
    } else {
        [190, 210, 235, 255]
    };
    let grid = if input.light {
        GRID
    } else {
        [95, 135, 150, 255]
    };
    // Work area box (green dashed).
    if let Some(area) = input.work_area {
        for (a, b) in box_edges(area) {
            if let (Some(a), Some(b)) = (p.screen(a), p.screen(b)) {
                marks.push(line(a, b, WORK_AREA, 1.2, Some([6., 4.])));
            }
        }
    }
    // Grid: dotted lines, bubbles with names at both ends.
    let mut first_axis = None;
    for axis in input.project.axes.values() {
        if input.hidden.contains(&axis.id) {
            continue;
        }
        let (wa, wb) = (v(axis.start), v(axis.end));
        let (Some(a), Some(b)) = (p.screen(wa), p.screen(wb)) else {
            continue;
        };
        if !(p.inside(a, 2000.) || p.inside(b, 2000.)) {
            continue;
        }
        first_axis.get_or_insert((wa, wb));
        if input.selected.contains(&axis.id) {
            marks.push(line(a, b, SELECTED_AXIS, 3., None));
        }
        marks.push(line(a, b, grid, 1., Some([2., 4.])));
        // The same grid line at the other levels, fainter.
        let faint = [grid[0], grid[1], grid[2], 110];
        for level in input.project.levels.values() {
            let dz = DVec3::Z * (level.elevation_mm - wa.z);
            if dz.length() > 1e-6
                && let (Some(la), Some(lb)) = (p.screen(wa + dz), p.screen(wb + dz))
            {
                marks.push(line(la, lb, faint, 1., Some([2., 6.])));
            }
        }
        let d = [b[0] - a[0], b[1] - a[1]];
        let len = (d[0] * d[0] + d[1] * d[1]).sqrt().max(1e-9);
        let d = [d[0] / len, d[1] / len];
        for (end, sign) in [(a, -1.), (b, 1.)] {
            let c = add(end, d, sign * 14.);
            marks.push(line(end, add(end, d, sign * 4.), grid, 1., None));
            marks.push(Mark::Circle {
                center: c,
                radius: 10.,
                color: grid,
                fill: Some(if input.light {
                    [255, 255, 255, 255]
                } else {
                    [15, 22, 32, 255]
                }),
            });
            marks.push(text(c, axis.name.clone(), label, [0, 0], 12.));
        }
    }
    // Construction lines and circles: dashed magenta (Tekla style).
    let construction: Rgba = if input.light {
        [190, 30, 150, 255]
    } else {
        [240, 110, 220, 255]
    };
    for c in input.project.construction.values() {
        let selected = input.selected.contains(&c.id);
        for (a, b) in c.segments() {
            if let (Some(sa), Some(sb)) = (
                p.screen(DVec3::from_array(a)),
                p.screen(DVec3::from_array(b)),
            ) {
                if selected {
                    marks.push(line(sa, sb, SELECTED_AXIS, 3., None));
                }
                marks.push(line(sa, sb, construction, 1.2, Some([9., 4.])));
            }
        }
    }
    // Model dimensions: line, 45° ticks and the value in millimetres.
    for d in input.project.dimensions.values() {
        let (Some(a), Some(b)) = (input.project.dim_point(&d.a), input.project.dim_point(&d.b))
        else {
            continue;
        };
        let (wa, wb) = (v(a), v(b));
        let (Some(sa), Some(sb)) = (p.screen(wa), p.screen(wb)) else {
            continue;
        };
        let colour = if input.selected.contains(&d.id) {
            SELECTED_AXIS
        } else {
            label
        };
        marks.push(line(sa, sb, colour, 1.2, None));
        let dir = [sb[0] - sa[0], sb[1] - sa[1]];
        let len = (dir[0] * dir[0] + dir[1] * dir[1]).sqrt().max(1e-9);
        let (ux, uy) = (dir[0] / len, dir[1] / len);
        // Tick at 45° to the dimension line, 10 px long.
        let tick = [(ux - uy) * 5. / 2f64.sqrt(), (uy + ux) * 5. / 2f64.sqrt()];
        for c in [sa, sb] {
            marks.push(line(
                [c[0] - tick[0], c[1] - tick[1]],
                [c[0] + tick[0], c[1] + tick[1]],
                colour,
                1.6,
                None,
            ));
        }
        let mid = [
            (sa[0] + sb[0]) / 2. + uy * 10.,
            (sa[1] + sb[1]) / 2. - ux * 10.,
        ];
        marks.push(text(
            mid,
            format!("{:.0}", wa.distance(wb)),
            colour,
            [0, 0],
            13.,
        ));
    }
    // Welds: a thick red line along the path with the ГОСТ 2.312-style mark;
    // bolts: a circle at each bolt of the group.
    for detail in input.project.details.values() {
        let selected = input.selected.contains(&detail.id);
        match &detail.kind {
            open_bim_model::DetailKind::Weld {
                path,
                weld_type,
                size,
                shop,
                sides,
                around,
                ..
            } => {
                let points: Vec<[f64; 2]> = path.iter().filter_map(|q| p.screen(v(*q))).collect();
                let colour = if selected {
                    SELECTED_AXIS
                } else {
                    [210, 40, 40, 255]
                };
                for w in points.windows(2) {
                    marks.push(line(w[0], w[1], colour, 4., None));
                }
                if let (Some(a), Some(b)) = (points.first(), points.last()) {
                    let mid = [(a[0] + b[0]) / 2., (a[1] + b[1]) / 2. - 12.];
                    let mark = open_bim_model::weld_mark(*weld_type, *sides, *size, *around, *shop);
                    // The joint and leg only: «Т3-Δ6».
                    let short = mark.trim_start_matches("ГОСТ 5264-80-").to_string();
                    marks.push(text(mid, short, colour, [0, 1], 12.));
                }
            }
            open_bim_model::DetailKind::BoltGroup {
                frame,
                steps_x,
                steps_y,
                ..
            } => {
                let prefix = |s: &[f64]| {
                    let mut out = vec![0.];
                    for x in s {
                        out.push(out.last().copied().unwrap_or(0.) + x);
                    }
                    out
                };
                let colour = if selected {
                    SELECTED_AXIS
                } else {
                    [20, 90, 30, 255]
                };
                for x in prefix(steps_x) {
                    for y in prefix(steps_y) {
                        if let Some(c) = p.screen(v(frame.world(Point3([x, y, 0.])))) {
                            marks.push(Mark::Circle {
                                center: c,
                                radius: 4.,
                                color: colour,
                                fill: None,
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }
    // Level lines along the first grid axis, labelled at its start.
    if first_axis.is_some() {
        for level in input.project.levels.values() {
            let Some((la, lb)) = level_line(input.project, level) else {
                continue;
            };
            if let (Some(a), Some(b)) = (p.screen(la), p.screen(lb)) {
                if input.selected.contains(&level.id) {
                    marks.push(line(a, b, SELECTED_AXIS, 3., None));
                }
                marks.push(line(a, b, grid, 1., Some([8., 6.])));
                marks.push(text(
                    add(a, [-1., 0.], 30.),
                    if level.name.chars().any(|c| c.is_ascii_digit()) {
                        level.name.clone()
                    } else {
                        format!("{} {:+.0}", level.name, level.elevation_mm)
                    },
                    label,
                    [1, 0],
                    11.,
                ));
            }
        }
    }
    // Model/work plane origin symbol: X/Y/Z arrows 40 px long.
    let (origin, axes) = input.workplane;
    if let Some(o) = p.screen(origin) {
        for (i, (axis, name)) in axes.iter().zip(["X", "Y", "Z"]).enumerate() {
            if let Some(d) = p.direction(origin, *axis) {
                let tip = add(o, d, 40.);
                marks.push(line(o, tip, AXIS_COLORS[i], 2., None));
                let n = [-d[1], d[0]];
                marks.push(line(
                    tip,
                    add(add(tip, d, -7.), n, 4.),
                    AXIS_COLORS[i],
                    2.,
                    None,
                ));
                marks.push(line(
                    tip,
                    add(add(tip, d, -7.), n, -4.),
                    AXIS_COLORS[i],
                    2.,
                    None,
                ));
                marks.push(text(add(tip, d, 9.), name, AXIS_COLORS[i], [0, 0], 12.));
            }
        }
        marks.push(Mark::Square {
            center: o,
            half: 3.,
            color: label,
            fill: true,
        });
    }
    // Axis gizmo, bottom-right corner.
    let centre = [p.min[0] + p.size[0] - 48., p.min[1] + p.size[1] - 48.];
    marks.push(Mark::Circle {
        center: centre,
        radius: 36.,
        color: [140, 150, 165, 160],
        fill: None,
    });
    for (i, (d, name)) in gizmo_directions(p.camera)
        .iter()
        .zip(["X", "Y", "Z"])
        .enumerate()
    {
        let tip = add(centre, *d, 28.);
        marks.push(line(centre, tip, AXIS_COLORS[i], 2.5, None));
        marks.push(text(
            add(centre, *d, 36.),
            name,
            AXIS_COLORS[i],
            [0, 0],
            12.,
        ));
    }
    // Selection handles.
    for h in handles(input.project, input.selected, p) {
        if !p.inside(h.screen, 10.) {
            continue;
        }
        let color = if input.direct {
            DIRECT_HANDLE
        } else {
            match h.kind {
                // Tekla: the start handle is yellow, all others magenta.
                HandleKind::Start | HandleKind::Vertex(0) => START_HANDLE,
                HandleKind::End | HandleKind::Vertex(_) => END_HANDLE,
            }
        };
        marks.push(Mark::Square {
            center: h.screen,
            half: 4.,
            color,
            fill: true,
        });
    }
    if let Some((a, b)) = input.highlight
        && let (Some(a), Some(b)) = (p.screen(a), p.screen(b))
    {
        marks.push(line(a, b, SNAP_LINE, 2.5, None));
    }
    if let Some(c) = input.reference.and_then(|r| p.screen(r)) {
        let k = 6.;
        let black = [0, 0, 0, 255];
        marks.push(line(
            [c[0] - k, c[1] - k],
            [c[0] + k, c[1] + k],
            black,
            2.,
            None,
        ));
        marks.push(line(
            [c[0] - k, c[1] + k],
            [c[0] + k, c[1] - k],
            black,
            2.,
            None,
        ));
    }
    if let Some((points, valid)) = &input.plate_preview {
        let screen: Vec<[f64; 2]> = points.iter().filter_map(|w| p.screen(*w)).collect();
        if screen.len() == points.len() && screen.len() >= 2 {
            let (fill, edge) = if *valid {
                ([30, 110, 230, 60], [30, 110, 230, 255])
            } else {
                ([220, 40, 40, 70], [220, 40, 40, 255])
            };
            if screen.len() >= 3 {
                marks.push(Mark::Fill {
                    points: screen.clone(),
                    color: fill,
                });
            }
            let n = screen.len();
            for i in 0..n {
                let j = (i + 1) % n;
                if n == 2 && j == 0 {
                    break;
                }
                let closing = j == 0;
                marks.push(line(
                    screen[i],
                    screen[j],
                    edge,
                    2.,
                    closing.then_some([5., 4.]),
                ));
                let mid = [
                    (screen[i][0] + screen[j][0]) / 2.,
                    (screen[i][1] + screen[j][1]) / 2.,
                ];
                marks.push(text(
                    mid,
                    format!("{:.0}", points[i].distance(points[j])),
                    edge,
                    [0, 1],
                    11.,
                ));
            }
            for (k, s) in screen.iter().enumerate() {
                marks.push(Mark::Circle {
                    center: *s,
                    radius: 8.,
                    color: edge,
                    fill: Some([255, 255, 255, 230]),
                });
                marks.push(text(*s, format!("{}", k + 1), edge, [0, 0], 10.));
            }
        }
    }
    // Rubber band with live length, then the snap symbol on top.
    let picked: Vec<[f64; 2]> = input.picks.iter().filter_map(|w| p.screen(*w)).collect();
    for w in picked.windows(2) {
        marks.push(line(w[0], w[1], RUBBER, 2., None));
    }
    if let Some((world, g)) = input.cursor
        && let Some(c) = p.screen(world)
    {
        if let (Some(last), Some(&last_world)) = (picked.last(), input.picks.last()) {
            marks.push(line(*last, c, RUBBER, 1.5, Some([7., 4.])));
            let mid = [(last[0] + c[0]) / 2., (last[1] + c[1]) / 2.];
            marks.push(text(
                add(mid, [0., -1.], 10.),
                format!("{:.0} мм", last_world.distance(world)),
                RUBBER,
                [0, 1],
                13.,
            ));
            if input.polygon && picked.len() >= 2 {
                marks.push(line(c, picked[0], RUBBER, 1., Some([3., 4.])));
            }
        }
        marks.extend(glyph(c, g));
        if let Some(name) = &input.snap_label {
            marks.push(text(
                add(c, [1., 1.], 14.),
                name.clone(),
                SNAP,
                [-1, -1],
                12.,
            ));
        }
    }
    marks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NamedView;

    fn close(a: [f64; 2], b: [f64; 2], eps: f64) -> bool {
        (a[0] - b[0]).abs() < eps && (a[1] - b[1]).abs() < eps
    }

    #[test]
    fn gizmo_matches_the_camera_matrix_in_named_views() {
        let expected = [
            (NamedView::Top, [Some([1., 0.]), Some([0., -1.]), None]),
            (NamedView::Front, [Some([1., 0.]), None, Some([0., -1.])]),
            (NamedView::Right, [None, Some([1., 0.]), Some([0., -1.])]),
        ];
        for (view, axes) in expected {
            let mut camera = Camera::default();
            camera.set_view(view);
            let g = gizmo_directions(&camera);
            for (i, e) in axes.iter().enumerate() {
                match e {
                    Some(e) => assert!(close(g[i], *e, 1e-6), "{view:?} axis {i}: {:?}", g[i]),
                    None => assert!(
                        g[i][0].hypot(g[i][1]) < 1e-6,
                        "{view:?} axis {i} faces viewer"
                    ),
                }
            }
        }
        // Every named view including isometric: same directions as projecting
        // world axis steps through the camera matrix (orthographic).
        for view in [
            NamedView::Isometric,
            NamedView::Front,
            NamedView::Top,
            NamedView::Right,
        ] {
            let mut camera = Camera::default();
            camera.set_view(view);
            camera.orthographic = true;
            camera.target = DVec3::new(1000., 2000., 500.);
            let p = Projector {
                camera: &camera,
                origin: DVec3::ZERO,
                min: [0., 0.],
                size: [800., 800.],
            };
            let g = gizmo_directions(&camera);
            let o = p.screen(camera.target).unwrap();
            let px_per_mm = 800. / camera.ortho_height;
            for (i, axis) in [DVec3::X, DVec3::Y, DVec3::Z].iter().enumerate() {
                let s = p.screen(camera.target + *axis * 100.).unwrap();
                let d = [
                    (s[0] - o[0]) / (100. * px_per_mm),
                    (s[1] - o[1]) / (100. * px_per_mm),
                ];
                assert!(
                    close(d, g[i], 1e-6),
                    "{view:?} axis {i}: {d:?} vs {:?}",
                    g[i]
                );
            }
        }
    }

    #[test]
    fn work_area_is_bounds_plus_margin() {
        let b = Bounds {
            min: DVec3::new(-100., 0., 50.),
            max: DVec3::new(6000., 4000., 3000.),
        };
        let w = work_area(b, 500.);
        assert_eq!(w.min, DVec3::new(-600., -500., -450.));
        assert_eq!(w.max, DVec3::new(6500., 4500., 3500.));
        let edges = box_edges(w);
        for (a, b) in edges {
            // Each edge runs along exactly one axis.
            let d = b - a;
            assert_eq!(
                [d.x != 0., d.y != 0., d.z != 0.]
                    .iter()
                    .filter(|x| **x)
                    .count(),
                1
            );
        }
    }

    #[test]
    fn handles_sit_on_member_ends_and_plate_vertices() {
        let project = open_bim_model::example_steel_frame().unwrap();
        let mut camera = Camera::default();
        camera.fit(crate::Scene::from_project(&project).unwrap().bounds);
        let p = Projector {
            camera: &camera,
            origin: DVec3::new(3000., 2000., 1500.),
            min: [100., 50.],
            size: [1280., 720.],
        };
        let selected: BTreeSet<ObjectId> = project.elements.keys().copied().collect();
        let hs = handles(&project, &selected, &p);
        let mut members = 0;
        let mut vertices = 0;
        for e in project.elements.values() {
            let f = e.shape.frame();
            let o = DVec3::from_array(f.origin.0);
            let axis = |j: usize| DVec3::from_array(f.axes[j]);
            let expected: Vec<(HandleKind, DVec3)> = match &e.shape {
                ElementShape::Member { length, .. } => {
                    members += 1;
                    vec![
                        (HandleKind::Start, o),
                        (HandleKind::End, o + axis(2) * length.value()),
                    ]
                }
                ElementShape::Plate { contour, .. } => contour
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        vertices += 1;
                        (HandleKind::Vertex(i), o + axis(0) * c[0] + axis(1) * c[1])
                    })
                    .collect(),
            };
            for (kind, world) in expected {
                let h = hs
                    .iter()
                    .find(|h| h.object == e.id && h.kind == kind)
                    .expect("handle");
                // Independent projection: full camera matrix by hand.
                let m = camera.matrix(p.origin, 1280. / 720.);
                let c = m * (world - p.origin).extend(1.);
                let ndc = c.truncate() / c.w;
                let s = [
                    100. + (ndc.x + 1.) * 0.5 * 1280.,
                    50. + (1. - ndc.y) * 0.5 * 720.,
                ];
                assert!(close(h.screen, s, 0.5), "{kind:?}: {:?} vs {s:?}", h.screen);
            }
        }
        assert!(members > 0);
        assert!(vertices > 0, "example has a plate");
        assert_eq!(handles(&project, &BTreeSet::new(), &p).len(), 0);
    }

    #[test]
    fn rubber_band_reports_live_length_and_snap_glyph() {
        let project = open_bim_model::example_steel_frame().unwrap();
        let camera = Camera::default();
        let p = Projector {
            camera: &camera,
            origin: DVec3::ZERO,
            min: [0., 0.],
            size: [1000., 700.],
        };
        let none = BTreeSet::new();
        let picks = [DVec3::ZERO];
        let input = Input {
            project: &project,
            hidden: &none,
            selected: &none,
            workplane: (DVec3::ZERO, [DVec3::X, DVec3::Y, DVec3::Z]),
            work_area: None,
            picks: &picks,
            polygon: false,
            cursor: Some((DVec3::new(3000., 4000., 0.), Glyph::End)),
            snap_label: Some("Конечная точка".into()),
            highlight: None,
            reference: None,
            direct: false,
            plate_preview: None,
            light: true,
        };
        let marks = build(&p, &input);
        assert!(
            marks
                .iter()
                .any(|m| matches!(m, Mark::Text { text, .. } if text == "5000 мм"))
        );
        assert!(
            marks
                .iter()
                .any(|m| matches!(m, Mark::Square { color, fill: false, .. } if *color == SNAP))
        );
        // Grid bubbles: two per axis with the axis name.
        for axis in project.axes.values() {
            let n = marks
                .iter()
                .filter(|m| matches!(m, Mark::Text { text, .. } if *text == axis.name))
                .count();
            assert!(n >= 2, "{}: {n}", axis.name);
        }
        // Distinct glyphs per snap kind.
        let kinds = [
            Glyph::Reference,
            Glyph::End,
            Glyph::Mid,
            Glyph::Intersection,
            Glyph::Perpendicular,
            Glyph::Nearest,
            Glyph::Free,
        ];
        let shapes: BTreeSet<String> = kinds
            .iter()
            .map(|g| format!("{:?}", glyph([0., 0.], *g)))
            .collect();
        assert_eq!(shapes.len(), kinds.len());
    }
}
