//! Drawing engine: regenerates sheets from model references. Nothing drawn is
//! stored in the model; a drawing keeps view definitions, associative
//! dimension anchors and object fingerprints used for up-to-date checks.
pub mod font;
pub mod projection;
pub mod qr;
pub mod sample;
pub mod svg;
pub mod title_block;

use open_bim_model::{
    Anchor, AnchorPoint, DetailKind, DimensionAxis, DimensionDef, Drawing, DrawingKind,
    DrawingView, ElementShape, ObjectId, Point3, Project, SheetSize, ViewDirection,
};
use projection::{Basis, PartMesh, V3, ViewBasis, add, cross, mul, sub};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// Standard scale denominators (ГОСТ 2.302, reduction scales).
pub const SCALES: [f64; 15] = [
    1., 2., 2.5, 4., 5., 10., 15., 20., 25., 40., 50., 75., 100., 200., 400.,
];
/// Left binding margin and other margins of the inner frame (ГОСТ 2.301/2.104).
pub const MARGIN_LEFT: f64 = 20.;
pub const MARGIN: f64 = 5.;
pub const TITLE_BLOCK: [f64; 2] = [185., 55.];
pub const TEXT_HEIGHT: f64 = 3.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LineStyle {
    Visible,
    Hidden,
    Section,
    Hatch,
    Thin,
    Frame,
    Lost,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Polyline {
    pub points: Vec<[f64; 2]>,
    pub style: LineStyle,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object: Option<ObjectId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    Start,
    Middle,
    End,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Label {
    pub position: [f64; 2],
    pub text: String,
    pub height: f64,
    pub align: Align,
    /// Counter-clockwise rotation in degrees.
    pub rotation: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object: Option<ObjectId>,
    pub lost: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DimensionGraphic {
    pub id: u32,
    pub view: u32,
    /// Measured model values of each chain segment, millimetres.
    pub values_mm: Vec<f64>,
    pub objects: Vec<ObjectId>,
    pub lost: bool,
    pub lines: Vec<Polyline>,
    pub labels: Vec<Label>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ViewGraphic {
    pub id: u32,
    pub name: String,
    pub scale: f64,
    /// Paper rectangle [min, max] occupied by the view geometry.
    pub bounds: [[f64; 2]; 2],
    pub lines: Vec<Polyline>,
    pub labels: Vec<Label>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Status {
    UpToDate,
    Outdated { changed: Vec<ObjectId> },
    LostReferences { missing: Vec<ObjectId> },
}
impl Status {
    pub fn label(&self) -> String {
        match self {
            Self::UpToDate => "Актуален".into(),
            Self::Outdated { changed } => format!("УСТАРЕЛ: изменено объектов {}", changed.len()),
            Self::LostReferences { missing } => {
                format!("ПОТЕРЯНЫ ПРИВЯЗКИ: {}", ids(missing))
            }
        }
    }
}

fn ids(list: &[ObjectId]) -> String {
    list.iter()
        .map(|id| format!("#{}", id.0))
        .collect::<Vec<_>>()
        .join(", ")
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Sheet {
    pub drawing: ObjectId,
    pub size: [f64; 2],
    pub status: Status,
    pub views: Vec<ViewGraphic>,
    pub dimensions: Vec<DimensionGraphic>,
    pub frame: Vec<Polyline>,
    pub labels: Vec<Label>,
    /// Filled black rectangles [x0, y0, x1, y1] (QR code modules).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fills: Vec<[f64; 4]>,
    /// Area kept free of drawing content: inscription, graph 27, QR code.
    pub title_area: [f64; 4],
}

pub type Error = String;

/// Drawing frame: member length along x, plate in its own plane, world for GA.
pub fn drawing_basis(project: &Project, kind: &DrawingKind) -> Result<Basis, Error> {
    let world = Basis {
        origin: project.origin.0,
        x: [1., 0., 0.],
        y: [0., 0., 1.],
        z: [0., -1., 0.],
    };
    let part = match kind {
        DrawingKind::General { .. } => return Ok(world),
        DrawingKind::Part { part } => *part,
        DrawingKind::Assembly { assembly } => {
            project
                .assemblies
                .get(assembly)
                .ok_or_else(|| format!("Сборка #{} не найдена", assembly.0))?
                .main_part
        }
    };
    let element = project
        .elements
        .get(&part)
        .ok_or_else(|| format!("Деталь #{} не найдена", part.0))?;
    let f = element.shape.frame();
    Ok(match element.shape {
        ElementShape::Member { .. } => Basis {
            origin: f.origin.0,
            x: f.axes[2],
            y: f.axes[1],
            z: cross(f.axes[2], f.axes[1]),
        },
        ElementShape::Plate { .. } => Basis {
            origin: f.origin.0,
            x: f.axes[0],
            y: f.axes[1],
            z: f.axes[2],
        },
    })
}

/// Parts shown by a drawing (existing ones only).
pub fn drawing_parts(project: &Project, kind: &DrawingKind) -> Vec<ObjectId> {
    let mut parts = BTreeSet::new();
    match kind {
        DrawingKind::General { objects } if objects.is_empty() => {
            parts.extend(project.elements.keys())
        }
        DrawingKind::General { objects } => parts.extend(objects),
        DrawingKind::Part { part } => {
            parts.insert(*part);
        }
        DrawingKind::Assembly { assembly } => {
            let mut stack = vec![*assembly];
            let mut seen = BTreeSet::new();
            while let Some(id) = stack.pop() {
                if !seen.insert(id) {
                    continue;
                }
                if let Some(a) = project.assemblies.get(&id) {
                    parts.extend(&a.members);
                    stack.extend(&a.children);
                }
            }
        }
    }
    parts.retain(|id| project.elements.contains_key(id));
    parts.into_iter().collect()
}

/// Objects whose state a drawing depends on, including missing references.
pub fn referenced_objects(project: &Project, drawing: &Drawing) -> BTreeSet<ObjectId> {
    let mut refs: BTreeSet<ObjectId> = match &drawing.kind {
        DrawingKind::General { objects } if objects.is_empty() => {
            project.elements.keys().copied().collect()
        }
        DrawingKind::General { objects } => objects.iter().copied().collect(),
        DrawingKind::Part { part } => BTreeSet::from([*part]),
        DrawingKind::Assembly { assembly } => {
            let mut set: BTreeSet<_> = drawing_parts(project, &drawing.kind).into_iter().collect();
            set.insert(*assembly);
            set
        }
    };
    for dim in &drawing.dimensions {
        for anchor in &dim.anchors {
            refs.insert(anchor.object);
            if let AnchorPoint::Hole { hole } = anchor.point {
                refs.insert(hole);
            }
        }
    }
    refs
}

/// Hash of everything that changes an object's drawn geometry or marks.
pub fn fingerprint(project: &Project, id: ObjectId) -> Option<String> {
    let value = if let Some(element) = project.elements.get(&id) {
        let details: Vec<_> = project
            .details
            .values()
            .filter(|d| d.kind.part() == Some(id) || d.kind.references().contains(&id))
            .collect();
        let profile = match &element.shape {
            ElementShape::Member { profile, .. } => project.profiles.get(profile),
            ElementShape::Plate { .. } => None,
        };
        serde_json::json!({
            "element": element,
            "details": details,
            "profile": profile,
            "material": project.materials.get(&element.material),
        })
    } else if let Some(assembly) = project.assemblies.get(&id) {
        serde_json::to_value(assembly).ok()?
    } else {
        serde_json::to_value(project.details.get(&id)?).ok()?
    };
    let bytes = serde_json::to_vec(&value).ok()?;
    Some(
        Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    )
}

pub fn status(project: &Project, drawing: &Drawing) -> Status {
    let refs = referenced_objects(project, drawing);
    let missing: Vec<ObjectId> = refs
        .iter()
        .copied()
        .filter(|id| fingerprint(project, *id).is_none())
        .chain(
            drawing
                .fingerprints
                .keys()
                .copied()
                .filter(|id| !project.contains_id(*id)),
        )
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if !missing.is_empty() {
        return Status::LostReferences { missing };
    }
    let changed: Vec<ObjectId> = refs
        .iter()
        .chain(drawing.fingerprints.keys())
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|id| drawing.fingerprints.get(id) != fingerprint(project, *id).as_ref())
        .collect();
    if changed.is_empty() {
        Status::UpToDate
    } else {
        Status::Outdated { changed }
    }
}

/// Regenerated drawing record with fresh fingerprints (associations kept).
pub fn updated(project: &Project, drawing: &Drawing) -> Drawing {
    let mut next = drawing.clone();
    next.fingerprints = referenced_objects(project, drawing)
        .into_iter()
        .filter_map(|id| Some((id, fingerprint(project, id)?)))
        .collect();
    next.generated_revision = project.revision;
    next.issued_revision = None;
    next
}

/// Only an up-to-date drawing without lost bindings can be issued.
pub fn issue(project: &Project, drawing: &Drawing) -> Result<Drawing, Error> {
    match status(project, drawing) {
        Status::UpToDate => {
            let mut next = drawing.clone();
            next.issued_revision = Some(project.revision);
            Ok(next)
        }
        other => Err(format!(
            "Чертёж {} нельзя выдать: {}",
            drawing.number,
            other.label()
        )),
    }
}

/// World point of an anchor, or `None` when the binding is lost.
pub fn anchor_point(project: &Project, anchor: &Anchor) -> Option<V3> {
    let element = project.elements.get(&anchor.object)?;
    let frame = element.shape.frame();
    match (&element.shape, anchor.point) {
        (_, AnchorPoint::Start) => Some(frame.origin.0),
        (ElementShape::Member { length, .. }, AnchorPoint::End) => {
            Some(frame.world(Point3([0., 0., length.value()])).0)
        }
        (ElementShape::Plate { contour, .. }, AnchorPoint::Vertex { index }) => {
            let p = contour.get(index)?;
            Some(frame.world(Point3([p[0], p[1], 0.])).0)
        }
        (_, AnchorPoint::Hole { hole }) => match &project.details.get(&hole)?.kind {
            DetailKind::Hole {
                part, frame: local, ..
            } if *part == anchor.object => Some(frame.world(local.origin).0),
            _ => None,
        },
        _ => None,
    }
}

fn view_scale(drawing: &Drawing, view: &DrawingView) -> f64 {
    view.scale.unwrap_or(drawing.scale)
}

struct ViewContext {
    basis: ViewBasis,
    /// Model-space centre of the projected geometry.
    centre: [f64; 2],
    scale: f64,
    position: [f64; 2],
    /// Model-space extents of the view geometry.
    bounds: [[f64; 2]; 2],
}
impl ViewContext {
    fn paper(&self, uv: [f64; 2]) -> [f64; 2] {
        [
            self.position[0] + (uv[0] - self.centre[0]) / self.scale,
            self.position[1] + (uv[1] - self.centre[1]) / self.scale,
        ]
    }
}

fn format_mm(value: f64) -> String {
    let rounded = (value * 10.).round() / 10.;
    if (rounded - rounded.round()).abs() < 1e-9 {
        format!("{}", rounded.round() as i64)
    } else {
        format!("{rounded:.1}").replace('.', ",")
    }
}

fn arrow(tip: [f64; 2], direction: [f64; 2]) -> Polyline {
    // ГОСТ 2.307 arrow: length 2.5 mm, opening about 20°.
    let (l, w) = (2.5, 0.45);
    let n = [-direction[1], direction[0]];
    let base = [tip[0] - direction[0] * l, tip[1] - direction[1] * l];
    Polyline {
        points: vec![
            [base[0] + n[0] * w, base[1] + n[1] * w],
            tip,
            [base[0] - n[0] * w, base[1] - n[1] * w],
        ],
        style: LineStyle::Thin,
        object: None,
    }
}

fn dimension(project: &Project, def: &DimensionDef, view: &ViewContext) -> DimensionGraphic {
    let objects: Vec<ObjectId> = def.anchors.iter().map(|a| a.object).collect();
    let points: Vec<Option<[f64; 2]>> = def
        .anchors
        .iter()
        .map(|a| {
            anchor_point(project, a).map(|p| {
                let q = view.basis.project(p);
                [q[0], q[1]]
            })
        })
        .collect();
    let mut graphic = DimensionGraphic {
        id: def.id,
        view: def.view,
        values_mm: Vec::new(),
        objects,
        lost: points.iter().any(Option::is_none),
        lines: Vec::new(),
        labels: Vec::new(),
    };
    let known: Vec<[f64; 2]> = points.iter().flatten().copied().collect();
    if graphic.lost {
        let at = known
            .first()
            .map(|p| view.paper(*p))
            .unwrap_or(view.position);
        graphic.labels.push(Label {
            position: [at[0], at[1] + def.offset],
            text: "? потеряна привязка".into(),
            height: TEXT_HEIGHT,
            align: Align::Start,
            rotation: 0.,
            object: None,
            lost: true,
        });
        return graphic;
    }
    let first = known[0];
    let last = known[known.len() - 1];
    let direction = match def.axis {
        DimensionAxis::Horizontal => [1., 0.],
        DimensionAxis::Vertical => [0., 1.],
        DimensionAxis::Aligned => {
            let d = [last[0] - first[0], last[1] - first[1]];
            let l = (d[0] * d[0] + d[1] * d[1]).sqrt();
            if l < 1e-9 {
                [1., 0.]
            } else {
                [d[0] / l, d[1] / l]
            }
        }
    };
    let normal = [-direction[1], direction[0]];
    let along = |p: [f64; 2]| p[0] * direction[0] + p[1] * direction[1];
    let across = |p: [f64; 2]| p[0] * normal[0] + p[1] * normal[1];
    let mut sorted = known.clone();
    sorted.sort_by(|a, b| along(*a).total_cmp(&along(*b)));
    // Points at the same station collapse into one extension line.
    sorted.dedup_by(|b, a| (along(*a) - along(*b)).abs() < 1e-6);
    // Dimension line placed beyond the measured points and the view outline.
    let [lo, hi] = view.bounds;
    let corners = [
        [lo[0], lo[1]],
        [hi[0], lo[1]],
        [hi[0], hi[1]],
        [lo[0], hi[1]],
    ];
    let reach = sorted.iter().chain(&corners).map(|p| across(*p));
    let extreme = if def.offset >= 0. {
        reach.fold(f64::NEG_INFINITY, f64::max)
    } else {
        reach.fold(f64::INFINITY, f64::min)
    };
    let line_at = |p: [f64; 2]| {
        let t = along(p);
        let model = [
            direction[0] * t + normal[0] * extreme,
            direction[1] * t + normal[1] * extreme,
        ];
        let paper = view.paper(model);
        [
            paper[0] + normal[0] * def.offset,
            paper[1] + normal[1] * def.offset,
        ]
    };
    let overshoot = 2. * def.offset.signum();
    for p in &sorted {
        let from = view.paper(*p);
        let to = line_at(*p);
        let gap = 1.0 * def.offset.signum();
        graphic.lines.push(Polyline {
            points: vec![
                [from[0] + normal[0] * gap, from[1] + normal[1] * gap],
                [to[0] + normal[0] * overshoot, to[1] + normal[1] * overshoot],
            ],
            style: LineStyle::Thin,
            object: None,
        });
    }
    for pair in sorted.windows(2) {
        let value = (along(pair[1]) - along(pair[0])).abs();
        graphic.values_mm.push(value);
        let (a, b) = (line_at(pair[0]), line_at(pair[1]));
        graphic.lines.push(Polyline {
            points: vec![a, b],
            style: LineStyle::Thin,
            object: None,
        });
        if value > 1e-9 {
            graphic.lines.push(arrow(a, [-direction[0], -direction[1]]));
            graphic.lines.push(arrow(b, direction));
        }
        let mid = [(a[0] + b[0]) / 2., (a[1] + b[1]) / 2.];
        graphic.labels.push(Label {
            position: [mid[0] + normal[0] * 1., mid[1] + normal[1] * 1.],
            text: format_mm(value),
            height: TEXT_HEIGHT,
            align: Align::Middle,
            rotation: direction[1].atan2(direction[0]).to_degrees(),
            object: None,
            lost: false,
        });
    }
    graphic
}

fn view_graphic(
    drawing: &Drawing,
    view: &DrawingView,
    basis: &Basis,
    meshes: &[PartMesh],
    marks: &BTreeMap<ObjectId, String>,
) -> (ViewGraphic, ViewContext) {
    let vb = ViewBasis::new(basis, view.direction);
    let scale = view_scale(drawing, view);
    let cut = view.section;
    let kept = |depth: f64| cut.is_none_or(|s| depth <= s + 1e-9);
    // Projected extents of the kept geometry define the view centre.
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for m in meshes {
        for p in &m.vertices {
            let q = vb.project(*p);
            if kept(q[2]) {
                for i in 0..2 {
                    lo[i] = lo[i].min(q[i]);
                    hi[i] = hi[i].max(q[i]);
                }
            }
        }
    }
    if !lo[0].is_finite() {
        (lo, hi) = ([0.; 2], [0.; 2]);
    }
    let context = ViewContext {
        basis: vb,
        centre: [(lo[0] + hi[0]) / 2., (lo[1] + hi[1]) / 2.],
        scale,
        position: view.position,
        bounds: [lo, hi],
    };
    let mut lines = Vec::new();
    let tolerance = 0.02 * scale;
    let limit_at = |p: V3| match cut {
        Some(s) => (s - vb.project(p)[2]).max(0.),
        None => f64::MAX,
    };
    for m in meshes {
        let mut runs: BTreeMap<bool, Vec<projection::Segment>> = BTreeMap::new();
        for (a, b) in m.feature_edges(vb.toward) {
            let (mut a, mut b) = (a, b);
            if let Some(s) = cut {
                let (da, db) = (vb.project(a)[2], vb.project(b)[2]);
                if da > s && db > s {
                    continue;
                }
                if da > s || db > s {
                    let t = (s - da) / (db - da);
                    let p = add(a, mul(sub(b, a), t));
                    if da > s { a = p } else { b = p }
                }
            }
            let (pa, pb) = (vb.project(a), vb.project(b));
            let length = ((pb[0] - pa[0]).powi(2) + (pb[1] - pa[1]).powi(2)).sqrt();
            if length / scale < 1e-4 {
                continue;
            }
            let samples = ((length / scale) / 0.5).ceil().clamp(2., 64.) as usize;
            for (t0, t1, visible) in
                projection::visibility_runs(a, b, vb.toward, meshes, limit_at, samples)
            {
                let at = |t: f64| {
                    let p = vb.project(add(a, mul(sub(b, a), t)));
                    [p[0], p[1]]
                };
                runs.entry(visible).or_default().push((at(t0), at(t1)));
            }
        }
        let empty = Vec::new();
        let (shown, concealed) = projection::clean(
            runs.get(&true).unwrap_or(&empty),
            if cut.is_some() {
                &empty
            } else {
                runs.get(&false).unwrap_or(&empty)
            },
        );
        for (segments, style) in [(concealed, LineStyle::Hidden), (shown, LineStyle::Visible)] {
            for points in projection::polylines(&segments, tolerance) {
                let length: f64 = points
                    .windows(2)
                    .map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]))
                    .sum();
                // Remnants shorter than 0.1 mm on paper are not drawable lines.
                if length / scale < 0.1 {
                    continue;
                }
                lines.push(Polyline {
                    points: points.iter().map(|p| context.paper(*p)).collect(),
                    style,
                    object: Some(m.id),
                });
            }
        }
        if let Some(s) = cut {
            let origin = add(vb.origin, mul(vb.toward, s));
            let segments: Vec<([f64; 2], [f64; 2])> = m
                .section(origin, vb.toward)
                .into_iter()
                .map(|(a, b)| {
                    let (a, b) = (vb.project(a), vb.project(b));
                    ([a[0], a[1]], [b[0], b[1]])
                })
                .collect();
            let loops = projection::loops(&segments);
            for (a, b) in projection::hatch(&loops, 2. * scale) {
                lines.push(Polyline {
                    points: vec![context.paper(a), context.paper(b)],
                    style: LineStyle::Hatch,
                    object: Some(m.id),
                });
            }
            for l in loops {
                lines.push(Polyline {
                    points: l.iter().map(|p| context.paper(*p)).collect(),
                    style: LineStyle::Section,
                    object: Some(m.id),
                });
            }
        }
    }
    let paper_lo = context.paper(lo);
    let paper_hi = context.paper(hi);
    let mut labels = vec![Label {
        // Title under the view keeps the space above free for dimensions.
        position: [view.position[0], paper_lo[1] - 12.],
        text: match cut {
            Some(_) => format!("{} (1:{})", view.name, format_mm(scale)),
            None => format!("{} 1:{}", view.name, format_mm(scale)),
        },
        height: 5.,
        align: Align::Middle,
        rotation: 0.,
        object: None,
        lost: false,
    }];
    if view.id == drawing.views[0].id {
        place_marks(&mut lines, &mut labels, meshes, &context, marks);
    }
    (
        ViewGraphic {
            id: view.id,
            name: view.name.clone(),
            scale,
            bounds: [paper_lo, paper_hi],
            lines,
            labels,
        },
        context,
    )
}

/// Marks next to the part centre, moved around it until the label does not
/// overlap another mark or the view title.
fn place_marks(
    lines: &mut Vec<Polyline>,
    labels: &mut Vec<Label>,
    meshes: &[PartMesh],
    context: &ViewContext,
    marks: &BTreeMap<ObjectId, String>,
) {
    let Ok(metrics) = font::Font::load() else {
        return;
    };
    let rect = |label: &Label| {
        let width = metrics.width(&label.text, label.height);
        let x0 = match label.align {
            Align::Start => label.position[0],
            Align::Middle => label.position[0] - width / 2.,
            Align::End => label.position[0] - width,
        };
        [
            [x0 - 0.5, label.position[1] - 0.3 * label.height],
            [x0 + width + 0.5, label.position[1] + label.height],
        ]
    };
    let mut placed: Vec<[[f64; 2]; 2]> = labels.iter().map(rect).collect();
    let free = |r: [[f64; 2]; 2], placed: &[[[f64; 2]; 2]]| {
        placed.iter().all(|q| {
            r[1][0] <= q[0][0] || q[1][0] <= r[0][0] || r[1][1] <= q[0][1] || q[1][1] <= r[0][1]
        })
    };
    for m in meshes {
        let Some(mark) = marks.get(&m.id) else {
            continue;
        };
        let centre = m.vertices.iter().fold([0.; 2], |acc, p| {
            let q = context.basis.project(*p);
            [acc[0] + q[0], acc[1] + q[1]]
        });
        let n = m.vertices.len().max(1) as f64;
        let at = context.paper([centre[0] / n, centre[1] / n]);
        let mut label = Label {
            position: [at[0] + 4., at[1] + 4.],
            text: mark.clone(),
            height: TEXT_HEIGHT,
            align: Align::Start,
            rotation: 0.,
            object: Some(m.id),
            lost: false,
        };
        'search: for radius in [4., 9., 14., 20., 27., 35.] {
            for (dx, dy) in [
                (1., 1.),
                (-1., 1.),
                (1., -1.),
                (-1., -1.),
                (0., 1.),
                (0., -1.),
            ] {
                label.align = if dx < 0. { Align::End } else { Align::Start };
                label.position = [at[0] + dx * radius, at[1] + dy * radius];
                if free(rect(&label), &placed) {
                    break 'search;
                }
            }
        }
        placed.push(rect(&label));
        lines.push(Polyline {
            points: vec![at, label.position],
            style: LineStyle::Thin,
            object: Some(m.id),
        });
        labels.push(label);
    }
}

/// Mark of the set a drawing belongs to (ГОСТ Р 21.101-2026, appendix В):
/// arrangement drawings — КМ, part and assembly drawings — КМД.
pub fn set_mark(kind: &DrawingKind) -> &'static str {
    match kind {
        DrawingKind::General { .. } => "КМ",
        DrawingKind::Part { .. } | DrawingKind::Assembly { .. } => "КМД",
    }
}

/// Designation, sheet number and sheet count of a drawing within its set:
/// sheets of one mark are numbered in drawing ID order.
pub fn sheet_number(project: &Project, drawing: &Drawing) -> (String, usize, usize) {
    let mark = set_mark(&drawing.kind);
    let mut ids: Vec<ObjectId> = project
        .drawings
        .values()
        .filter(|d| set_mark(&d.kind) == mark)
        .map(|d| d.id)
        .collect();
    if !ids.contains(&drawing.id) {
        ids.push(drawing.id);
    }
    ids.sort();
    let no = ids.iter().position(|id| *id == drawing.id).unwrap_or(0) + 1;
    (project.document.designation(mark), no, ids.len())
}

fn title_block(
    project: &Project,
    drawing: &Drawing,
    status: &Status,
) -> Result<title_block::TitleBlock, Error> {
    let (designation, sheet_no, sheets) = sheet_number(project, drawing);
    let mut tb = title_block::build(&title_block::SheetInfo {
        info: &project.document,
        size: drawing.sheet_mm(),
        form: title_block::form_for(title_block::DocumentClass::WorkingSet, sheet_no),
        designation,
        title: drawing.title.clone(),
        sheet_no,
        sheets,
        format: drawing.format_label(),
        material: None,
        mass_kg: None,
        scale: None,
        version: 1,
    })?;
    // Model link state is a program mark, outside the inscription.
    if *status != Status::UpToDate {
        tb.labels.push(Label {
            position: [tb.block[0], tb.reserved[3] + 2.],
            text: format!(
                "{} · модель рев. {} · чертёж рев. {}",
                status.label(),
                project.revision,
                drawing.generated_revision
            ),
            height: 3.5,
            align: Align::Start,
            rotation: 0.,
            object: None,
            lost: true,
        });
    }
    Ok(tb)
}

/// Regenerate the full sheet from the model.
pub fn generate(project: &Project, drawing: &Drawing) -> Result<Sheet, Error> {
    drawing.validate().map_err(|e| e.to_string())?;
    let status = status(project, drawing);
    let parts = drawing_parts(project, &drawing.kind);
    let basis = drawing_basis(project, &drawing.kind).unwrap_or(Basis {
        origin: project.origin.0,
        x: [1., 0., 0.],
        y: [0., 0., 1.],
        z: [0., -1., 0.],
    });
    let meshes = parts
        .iter()
        .map(|id| PartMesh::new(project, *id))
        .collect::<Result<Vec<_>, _>>()?;
    let marks = open_bim_numbering::number(project)
        .map(|n| n.parts)
        .unwrap_or_default();
    let mut views: Vec<ViewGraphic> = Vec::new();
    let mut contexts = BTreeMap::new();
    for view in &drawing.views {
        let (graphic, context) = view_graphic(drawing, view, &basis, &meshes, &marks);
        views.push(graphic);
        contexts.insert(view.id, context);
    }
    // Weld signs in the main view (ГОСТ 2.312): leader from the weld to a
    // shelf, the designation above the shelf.
    if let Some(main) = drawing.views.first()
        && let Some(context) = contexts.get(&main.id)
        && let Some(graphic) = views.iter_mut().find(|v| v.id == main.id)
    {
        let shown: BTreeSet<ObjectId> = parts.iter().copied().collect();
        for (k, detail) in project.details.values().enumerate() {
            let DetailKind::Weld {
                parts: welded,
                path,
                weld_type,
                size,
                shop,
                sides,
                around,
            } = &detail.kind
            else {
                continue;
            };
            if shown.len() < 2 || !welded.is_subset(&shown) || path.len() < 2 {
                continue;
            }
            let (a, b) = (path[0].0, path[path.len() - 1].0);
            let mid = [(a[0] + b[0]) / 2., (a[1] + b[1]) / 2., (a[2] + b[2]) / 2.];
            let q = context.basis.project(mid);
            let at = context.paper([q[0], q[1]]);
            let text = open_bim_model::weld_mark(*weld_type, *sides, *size, *around, *shop);
            let width = text.chars().count() as f64 * TEXT_HEIGHT * 0.62;
            let lift = 10. + 6. * (k % 3) as f64;
            let shelf = [at[0] + lift, at[1] + lift];
            let line = |points: Vec<[f64; 2]>| Polyline {
                points,
                style: LineStyle::Thin,
                object: Some(detail.id),
            };
            // One-sided arrow at the weld, leader, shelf.
            let d = [shelf[0] - at[0], shelf[1] - at[1]];
            let len = (d[0] * d[0] + d[1] * d[1]).sqrt();
            let (ux, uy) = (d[0] / len, d[1] / len);
            graphic.lines.push(line(vec![
                at,
                [at[0] + ux * 3. - uy * 1., at[1] + uy * 3. + ux * 1.],
            ]));
            graphic
                .lines
                .push(line(vec![at, shelf, [shelf[0] + width, shelf[1]]]));
            graphic.labels.push(Label {
                position: [shelf[0] + 0.5, shelf[1] + 1.],
                text,
                height: TEXT_HEIGHT,
                align: Align::Start,
                rotation: 0.,
                object: Some(detail.id),
                lost: false,
            });
        }
    }
    let dimensions: Vec<DimensionGraphic> = drawing
        .dimensions
        .iter()
        .map(|d| dimension(project, d, &contexts[&d.view]))
        .collect();
    // View titles go under the lowest dimension of their view.
    for view in &mut views {
        let lowest = dimensions
            .iter()
            .filter(|d| d.view == view.id)
            .flat_map(|d| d.lines.iter())
            .flat_map(|l| l.points.iter().map(|p| p[1]))
            .chain(
                dimensions
                    .iter()
                    .filter(|d| d.view == view.id)
                    .flat_map(|d| d.labels.iter().map(|l| l.position[1] - l.height)),
            )
            .fold(view.bounds[0][1], f64::min);
        if let Some(title) = view.labels.first_mut() {
            title.position[1] = lowest - 10.;
        }
    }
    let block = title_block(project, drawing, &status)?;
    let title_area = block.reserved;
    let (frame, mut labels, fills) = (block.lines, block.labels, block.fills);
    for note in &drawing.texts {
        labels.push(Label {
            position: note.position,
            text: note.text.clone(),
            height: note.height,
            align: Align::Start,
            rotation: 0.,
            object: None,
            lost: false,
        });
    }
    Ok(Sheet {
        drawing: drawing.id,
        size: drawing.sheet_mm(),
        status,
        views,
        dimensions,
        frame,
        labels,
        fills,
        title_area,
    })
}

/// Model-space extents of the parts in a view direction (u and v spans).
fn extents(meshes: &[PartMesh], basis: &Basis, direction: ViewDirection) -> [f64; 2] {
    let vb = ViewBasis::new(basis, direction);
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for p in meshes.iter().flat_map(|m| &m.vertices) {
        let q = vb.project(*p);
        for i in 0..2 {
            lo[i] = lo[i].min(q[i]);
            hi[i] = hi[i].max(q[i]);
        }
    }
    if lo[0].is_finite() {
        [hi[0] - lo[0], hi[1] - lo[1]]
    } else {
        [0.; 2]
    }
}

/// Smallest standard scale fitting the views into the drawing field, plus
/// first-angle positions: main view top-left, plan below, side view right.
pub fn layout(
    project: &Project,
    kind: &DrawingKind,
    sheet: SheetSize,
    directions: &[ViewDirection],
) -> Result<(f64, Vec<[f64; 2]>), Error> {
    let basis = drawing_basis(project, kind)?;
    let meshes = drawing_parts(project, kind)
        .iter()
        .map(|id| PartMesh::new(project, *id))
        .collect::<Result<Vec<_>, _>>()?;
    let sizes: Vec<[f64; 2]> = directions
        .iter()
        .map(|d| extents(&meshes, &basis, *d))
        .collect();
    let [w, h] = sheet.landscape_mm();
    // Field above the title block; gaps leave room for dimensions and titles.
    let field = [
        w - MARGIN_LEFT - MARGIN - 20.,
        h - 2. * MARGIN - TITLE_BLOCK[1] - 25.,
    ];
    let gap = 30.;
    let column = |i: usize| sizes.get(i).copied().unwrap_or([0.; 2]);
    let main = column(0);
    let below = directions.iter().position(|d| *d == ViewDirection::Top);
    let side = directions
        .iter()
        .position(|d| matches!(d, ViewDirection::Left | ViewDirection::Right));
    let fits = |s: f64| {
        let width = main[0] / s + side.map_or(0., |i| gap + column(i)[0] / s);
        let height = main[1] / s + below.map_or(0., |i| gap + column(i)[1] / s);
        width + gap <= field[0] && height + gap <= field[1]
    };
    let scale = SCALES
        .iter()
        .copied()
        .find(|s| fits(*s))
        .ok_or("Виды не помещаются на листе даже в масштабе 1:400")?;
    let left = MARGIN_LEFT + 15.;
    let top = h - MARGIN - 35.;
    let main_centre = [left + main[0] / scale / 2., top - main[1] / scale / 2.];
    let positions = directions
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let size = column(i);
            match d {
                ViewDirection::Top | ViewDirection::Bottom if i != 0 => [
                    main_centre[0],
                    main_centre[1] - main[1] / scale / 2. - gap - size[1] / scale / 2.,
                ],
                ViewDirection::Left | ViewDirection::Right if i != 0 => [
                    main_centre[0] + main[0] / scale / 2. + gap + size[0] / scale / 2.,
                    main_centre[1],
                ],
                _ => main_centre,
            }
        })
        .collect();
    Ok((scale, positions))
}

fn views(directions: &[(ViewDirection, &str)], positions: &[[f64; 2]]) -> Vec<DrawingView> {
    directions
        .iter()
        .zip(positions)
        .enumerate()
        .map(|(i, ((direction, name), position))| DrawingView {
            id: i as u32 + 1,
            name: (*name).into(),
            direction: *direction,
            section: None,
            position: *position,
            scale: None,
        })
        .collect()
}

/// Default part drawing: front/top/left views, overall length and hole chain.
pub fn part_drawing(
    project: &Project,
    id: ObjectId,
    part: ObjectId,
    number: &str,
) -> Result<Drawing, Error> {
    let element = project
        .elements
        .get(&part)
        .ok_or_else(|| format!("Деталь #{} не найдена", part.0))?;
    let kind = DrawingKind::Part { part };
    let (directions, sheet): (Vec<(ViewDirection, &str)>, SheetSize) = match element.shape {
        ElementShape::Member { .. } => (
            vec![
                (ViewDirection::Front, "Вид спереди"),
                (ViewDirection::Top, "Вид сверху"),
                (ViewDirection::Left, "Вид слева"),
            ],
            SheetSize::A3,
        ),
        ElementShape::Plate { .. } => (vec![(ViewDirection::Front, "Вид на плиту")], SheetSize::A3),
    };
    let dirs: Vec<ViewDirection> = directions.iter().map(|d| d.0).collect();
    let (scale, positions) = layout(project, &kind, sheet, &dirs)?;
    let holes: Vec<Anchor> = project
        .details
        .values()
        .filter_map(|d| match d.kind {
            DetailKind::Hole { part: p, .. } if p == part => Some(Anchor {
                object: part,
                point: AnchorPoint::Hole { hole: d.id },
            }),
            _ => None,
        })
        .collect();
    let anchor = |point| Anchor {
        object: part,
        point,
    };
    let mut dimensions = Vec::new();
    match &element.shape {
        ElementShape::Member { .. } => {
            dimensions.push(DimensionDef {
                id: 1,
                view: 1,
                anchors: vec![anchor(AnchorPoint::Start), anchor(AnchorPoint::End)],
                axis: DimensionAxis::Horizontal,
                offset: 18.,
            });
            if !holes.is_empty() {
                let mut chain = vec![anchor(AnchorPoint::Start)];
                chain.extend(holes.iter().copied());
                chain.push(anchor(AnchorPoint::End));
                dimensions.push(DimensionDef {
                    id: 2,
                    view: 1,
                    anchors: chain,
                    axis: DimensionAxis::Horizontal,
                    offset: 8.,
                });
            }
        }
        ElementShape::Plate { contour, .. } => {
            let vertices: Vec<Anchor> = (0..contour.len())
                .map(|index| anchor(AnchorPoint::Vertex { index }))
                .collect();
            let mut horizontal = vertices.clone();
            horizontal.extend(holes.iter().copied());
            dimensions.push(DimensionDef {
                id: 1,
                view: 1,
                anchors: horizontal.clone(),
                axis: DimensionAxis::Horizontal,
                offset: -12.,
            });
            dimensions.push(DimensionDef {
                id: 2,
                view: 1,
                anchors: horizontal,
                axis: DimensionAxis::Vertical,
                offset: -12.,
            });
        }
    }
    Ok(Drawing {
        id,
        kind,
        number: number.into(),
        title: element.name.clone(),
        sheet,
        scale,
        views: views(&directions, &positions),
        dimensions,
        texts: Vec::new(),
        fingerprints: BTreeMap::new(),
        generated_revision: 0,
        issued_revision: None,
        multiple: 1,
        portrait: false,
    })
    .map(|d| updated(project, &d))
}

/// Default assembly drawing: front and top views, main part overall length.
pub fn assembly_drawing(
    project: &Project,
    id: ObjectId,
    assembly: ObjectId,
    number: &str,
) -> Result<Drawing, Error> {
    let a = project
        .assemblies
        .get(&assembly)
        .ok_or_else(|| format!("Сборка #{} не найдена", assembly.0))?;
    let kind = DrawingKind::Assembly { assembly };
    let directions = [
        (ViewDirection::Front, "Вид спереди"),
        (ViewDirection::Top, "Вид сверху"),
    ];
    let dirs: Vec<ViewDirection> = directions.iter().map(|d| d.0).collect();
    let (scale, positions) = layout(project, &kind, SheetSize::A3, &dirs)?;
    let mut dimensions = Vec::new();
    if matches!(
        project.elements.get(&a.main_part).map(|e| &e.shape),
        Some(ElementShape::Member { .. })
    ) {
        dimensions.push(DimensionDef {
            id: 1,
            view: 1,
            anchors: vec![
                Anchor {
                    object: a.main_part,
                    point: AnchorPoint::Start,
                },
                Anchor {
                    object: a.main_part,
                    point: AnchorPoint::End,
                },
            ],
            axis: DimensionAxis::Horizontal,
            offset: 18.,
        });
    }
    Ok(updated(
        project,
        &Drawing {
            id,
            kind,
            number: number.into(),
            title: a.name.clone(),
            sheet: SheetSize::A3,
            scale,
            views: views(&directions, &positions),
            dimensions,
            texts: Vec::new(),
            fingerprints: BTreeMap::new(),
            generated_revision: 0,
            issued_revision: None,
            multiple: 1,
            portrait: false,
        },
    ))
}

/// Default general arrangement: elevation and plan of the listed parts.
pub fn general_drawing(
    project: &Project,
    id: ObjectId,
    objects: Vec<ObjectId>,
    number: &str,
    sheet: SheetSize,
) -> Result<Drawing, Error> {
    let kind = DrawingKind::General { objects };
    let directions = [
        (ViewDirection::Front, "Фасад"),
        (ViewDirection::Top, "План"),
    ];
    let dirs: Vec<ViewDirection> = directions.iter().map(|d| d.0).collect();
    let (scale, positions) = layout(project, &kind, sheet, &dirs)?;
    Ok(updated(
        project,
        &Drawing {
            id,
            kind,
            number: number.into(),
            title: format!("Общий вид · {}", project.name),
            sheet,
            scale,
            views: views(&directions, &positions),
            dimensions: Vec::new(),
            texts: Vec::new(),
            fingerprints: BTreeMap::new(),
            generated_revision: 0,
            issued_revision: None,
            multiple: 1,
            portrait: false,
        },
    ))
}
