//! Drawing engine: regenerates sheets from model references. Nothing drawn is
//! stored in the model; a drawing keeps view definitions, associative
//! dimension anchors and object fingerprints used for up-to-date checks.
pub mod conventions;
pub mod font;
pub mod general_data;
mod pour_marks;
pub mod projection;
pub mod qr;
pub mod sample;
pub mod sheet_objects;
pub mod svg;
mod swept_drawing;
pub mod tables;
pub mod title_block;
pub mod wizard;

pub use general_data::general_data_drawing;

use open_bim_model::{
    Anchor, AnchorPoint, DetailKind, DimensionAxis, DimensionDef, Drawing, DrawingKind,
    DrawingView, ElementShape, MemberKind, ObjectId, Point3, Project, SheetSize, ViewDirection,
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
/// Advance of the drawn ГОСТ 2.312 leg sign, in character heights.
const LEG_SIGN_WIDTH: f64 = 0.85;
/// Notes and weld signs: font 2.5.
pub const TEXT_HEIGHT: f64 = conventions::em(2.5);

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LineStyle {
    Visible,
    Hidden,
    Section,
    Hatch,
    Thin,
    Frame,
    Lost,
    /// Thin dash-dot (axes, user lines).
    DashDot,
    /// Thick dashed / dash-dot user lines.
    ThickDashed,
    ThickDashDot,
    Custom(open_bim_model::LinePen),
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
    /// User sheet objects: their lines and, for masking, the hidden area.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub objects: Vec<ObjectGraphic>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ObjectGraphic {
    pub id: u32,
    pub lines: Vec<Polyline>,
    /// Masking polygon drawn opaque white over the views.
    pub mask: Option<Vec<[f64; 2]>>,
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
        DrawingKind::General { .. }
        | DrawingKind::GeneralData { .. }
        | DrawingKind::Multi { .. } => return Ok(world),
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
    if project.swept_parts.contains_key(&part) {
        return Ok(Basis {
            origin: f.origin.0,
            x: f.axes[0],
            y: f.axes[1],
            z: f.axes[2],
        });
    }
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

/// What one object of a «Комплексный чертёж» is drawn as: an assembly
/// drawing for an assembly id, else a part drawing.
pub fn source_kind(project: &Project, object: ObjectId) -> DrawingKind {
    if project.assemblies.contains_key(&object) {
        DrawingKind::Assembly { assembly: object }
    } else {
        DrawingKind::Part { part: object }
    }
}

/// Parts shown by a drawing (existing ones only).
pub fn drawing_parts(project: &Project, kind: &DrawingKind) -> Vec<ObjectId> {
    let mut parts = BTreeSet::new();
    match kind {
        DrawingKind::General { objects } if objects.is_empty() => {
            parts.extend(project.elements.keys())
        }
        DrawingKind::General { objects } => parts.extend(objects),
        DrawingKind::GeneralData { .. } => {}
        DrawingKind::Multi { objects } => {
            for o in objects {
                parts.extend(drawing_parts(project, &source_kind(project, *o)));
            }
        }
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
        // The general notes state the model mass.
        DrawingKind::GeneralData { .. } => project.elements.keys().copied().collect(),
        DrawingKind::Part { part } => BTreeSet::from([*part]),
        DrawingKind::Assembly { assembly } => {
            let mut set: BTreeSet<_> = drawing_parts(project, &drawing.kind).into_iter().collect();
            set.insert(*assembly);
            set
        }
        DrawingKind::Multi { objects } => {
            let mut set: BTreeSet<_> = drawing_parts(project, &drawing.kind).into_iter().collect();
            set.extend(objects);
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
    let mut value = value;
    if let (Some(swept), Some(object)) = (project.swept_parts.get(&id), value.as_object_mut()) {
        object.insert("swept_part".into(), serde_json::json!(swept));
    }
    // Joints and stored zone numbers are drawn on concrete parts; parts
    // without them keep their fingerprints.
    let concrete = &project.concrete;
    if project.concrete_type(id).is_some()
        && (!concrete.breaks.is_empty() || !concrete.units.is_empty())
        && let Some(object) = value.as_object_mut()
    {
        object.insert(
            "pours".into(),
            serde_json::json!({"breaks": concrete.breaks, "units": concrete.units}),
        );
    }
    // A fixed mark (numbering run) is drawn too; absent marks keep the
    // fingerprints of never-numbered models unchanged.
    if let (Some(mark), Some(object)) = (project.numbering.get(&id), value.as_object_mut()) {
        object.insert("mark".into(), serde_json::json!(mark));
    }
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
    if let Some(swept) = project.swept_parts.get(&anchor.object) {
        use open_bim_model::SweptPart;
        let local = match (swept, anchor.point) {
            (
                SweptPart::CurvedBeam {
                    center,
                    radius,
                    start_angle,
                    sweep_angle,
                },
                AnchorPoint::Start | AnchorPoint::End,
            ) => {
                let a = start_angle
                    + if anchor.point == AnchorPoint::End {
                        *sweep_angle
                    } else {
                        0.
                    };
                [
                    center[0] + radius * a.cos(),
                    center[1] + radius * a.sin(),
                    0.,
                ]
            }
            (SweptPart::PolyBeam { points, .. }, AnchorPoint::Start | AnchorPoint::End) => {
                let p = if anchor.point == AnchorPoint::End {
                    points.last()?
                } else {
                    points.first()?
                };
                [p[0], p[1], 0.]
            }
            (
                SweptPart::BentPlate {
                    width,
                    first_length,
                    second_length,
                    radius,
                    angle,
                },
                AnchorPoint::Vertex { index },
            ) if index < 4 => {
                let a = angle.abs();
                let end = [
                    radius * a.sin() + second_length * a.cos(),
                    (radius * (1. - a.cos()) + second_length * a.sin()) * angle.signum(),
                ];
                let p = if index == 1 || index == 2 {
                    end
                } else {
                    [-first_length, 0.]
                };
                [p[0], p[1], if index >= 2 { *width } else { 0. }]
            }
            (SweptPart::BentPlate { first_length, .. }, AnchorPoint::Start) => {
                [-first_length, 0., 0.]
            }
            _ => return None,
        };
        return Some(frame.world(Point3(local)).0);
    }
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
    /// Sheet point back to the view plane (u, v), model mm.
    fn plane(&self, paper: [f64; 2]) -> [f64; 2] {
        [
            self.centre[0] + (paper[0] - self.position[0]) * self.scale,
            self.centre[1] + (paper[1] - self.position[1]) * self.scale,
        ]
    }

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

fn dimension(
    project: &Project,
    def: &DimensionDef,
    view: &ViewContext,
    ticks: bool,
) -> DimensionGraphic {
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
    let count = sorted.len();
    for (k, pair) in sorted.windows(2).enumerate() {
        let value = (along(pair[1]) - along(pair[0])).abs();
        graphic.values_mm.push(value);
        let (a, b) = (line_at(pair[0]), line_at(pair[1]));
        // With ticks the line runs 0–3 mm past the outer extension lines.
        let (mut sa, mut sb) = (a, b);
        if ticks {
            if k == 0 {
                sa = [
                    a[0] - direction[0] * conventions::DIM_OVERSHOOT,
                    a[1] - direction[1] * conventions::DIM_OVERSHOOT,
                ];
            }
            if k + 2 == count {
                sb = [
                    b[0] + direction[0] * conventions::DIM_OVERSHOOT,
                    b[1] + direction[1] * conventions::DIM_OVERSHOOT,
                ];
            }
        }
        graphic.lines.push(Polyline {
            points: vec![sa, sb],
            style: LineStyle::Thin,
            object: None,
        });
        if value > 1e-9 {
            if ticks {
                if k == 0 {
                    graphic.lines.push(conventions::tick(a, direction));
                }
                graphic.lines.push(conventions::tick(b, direction));
            } else {
                graphic.lines.push(arrow(a, [-direction[0], -direction[1]]));
                graphic.lines.push(arrow(b, direction));
            }
        }
        let mid = [(a[0] + b[0]) / 2., (a[1] + b[1]) / 2.];
        graphic.labels.push(Label {
            position: [mid[0] + normal[0] * 1., mid[1] + normal[1] * 1.],
            text: format_mm(value),
            height: conventions::DIM_TEXT,
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
    // Hidden objects neither draw nor occlude other parts. Keep extents stable
    // so hiding a part does not shift the remaining geometry or dimensions.
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
    // A node shows its region only, centred on it.
    if let Some(n) = view.node {
        lo = [n.centre[0] - n.radius, n.centre[1] - n.radius];
        hi = [n.centre[0] + n.radius, n.centre[1] + n.radius];
    }
    let context = ViewContext {
        basis: vb,
        centre: [(lo[0] + hi[0]) / 2., (lo[1] + hi[1]) / 2.],
        scale,
        position: view.position,
        bounds: [lo, hi],
    };
    let filtered;
    let meshes = if view.appearance.hidden_objects.is_empty() {
        meshes
    } else {
        filtered = meshes
            .iter()
            .filter(|m| !view.appearance.hidden_objects.contains(&m.id))
            .cloned()
            .collect::<Vec<_>>();
        &filtered
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
    let default = open_bim_model::ViewAppearance::default();
    lines.retain(|line| view.appearance.show_hidden || line.style != LineStyle::Hidden);
    for line in &mut lines {
        let pens = match line.style {
            LineStyle::Visible => Some((view.appearance.visible, default.visible)),
            LineStyle::Hidden => Some((view.appearance.hidden, default.hidden)),
            LineStyle::Section => Some((view.appearance.section, default.section)),
            _ => None,
        };
        if let Some((pen, original)) = pens {
            let pen = line
                .object
                .and_then(|id| view.appearance.objects.get(&id))
                .copied()
                .unwrap_or(pen);
            if pen != original {
                line.style = LineStyle::Custom(pen);
            }
        }
    }
    let paper_lo = context.paper(lo);
    let paper_hi = context.paper(hi);
    let mut labels = vec![Label {
        // Title under the view keeps the space above free for dimensions.
        position: [view.position[0], paper_lo[1] - 12.],
        // Sections are titled by their number, as on the trace (5.5.2).
        text: match cut {
            Some(_) => {
                let n = conventions::section_number(drawing, view.id);
                format!("{n}-{n} (1:{})", format_mm(scale))
            }
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
    // Parts behind one another with the same mark are marked once.
    let mut marked: Vec<(&str, [f64; 2])> = Vec::new();
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
        if marked
            .iter()
            .any(|(t, p)| *t == mark.as_str() && (p[0] - at[0]).hypot(p[1] - at[1]) < 1.)
        {
            continue;
        }
        marked.push((mark, at));
        let mut label = Label {
            position: [at[0] + 4., at[1] + 4.],
            text: mark.clone(),
            height: conventions::MARK_TEXT,
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
        // Leader from a dot on the part to a shelf under the mark (5.4.5).
        let width = metrics.width(&label.text, label.height);
        let shelf_y = label.position[1] - 0.6;
        let (s0, s1) = match label.align {
            Align::End => (label.position[0] - width - 0.5, label.position[0] + 0.5),
            _ => (label.position[0] - 0.5, label.position[0] + width + 0.5),
        };
        let near = if (at[0] - s0).abs() < (at[0] - s1).abs() {
            s0
        } else {
            s1
        };
        lines.push(Polyline {
            points: vec![at, [near, shelf_y]],
            style: LineStyle::Thin,
            object: Some(m.id),
        });
        lines.push(Polyline {
            points: vec![[s0, shelf_y], [s1, shelf_y]],
            style: LineStyle::Thin,
            object: Some(m.id),
        });
        lines.push(conventions::circle(at, 0.5, LineStyle::Visible));
        labels.push(label);
    }
}

/// Mark of the set a drawing belongs to (ГОСТ Р 21.101-2026, appendix В):
/// arrangement drawings — КМ, part and assembly drawings — КМД.
pub fn set_mark(kind: &DrawingKind) -> &'static str {
    match kind {
        DrawingKind::General { .. } => "КМ",
        DrawingKind::Part { .. } | DrawingKind::Assembly { .. } | DrawingKind::Multi { .. } => {
            "КМД"
        }
        DrawingKind::GeneralData { mark } if mark == "КМД" => "КМД",
        DrawingKind::GeneralData { .. } => "КМ",
    }
}

/// Designation, sheet number and sheet count of a drawing within its set:
/// «Общие данные» first (4.3.1), then the sheets of one mark in drawing ID
/// order.
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
    let first = |id: &ObjectId| {
        let kind = if *id == drawing.id {
            Some(&drawing.kind)
        } else {
            project.drawings.get(id).map(|d| &d.kind)
        };
        !matches!(kind, Some(DrawingKind::GeneralData { .. }))
    };
    ids.sort_by_key(|id| (first(id), *id));
    // Numbers of annulled sheets stay taken (7.3.9): later sheets keep theirs.
    let annulled = annulled_sheets(project, mark);
    let mut no = 0;
    let mut k = 0;
    for id in &ids {
        k += 1;
        while annulled.iter().any(|(n, _, _)| *n == k) {
            k += 1;
        }
        if *id == drawing.id {
            no = k;
        }
    }
    (project.document.designation(mark), no, ids.len())
}

/// Annulled sheets of a set: (sheet number, title, change number).
pub fn annulled_sheets(project: &Project, mark: &str) -> Vec<(usize, String, u32)> {
    project
        .changes
        .iter()
        .filter(|c| c.mark == mark)
        .flat_map(|c| {
            c.sheets.iter().filter_map(move |s| match &s.kind {
                open_bim_model::SheetChangeKind::Annulled { sheet, title } => {
                    Some((*sheet, title.clone(), c.number))
                }
                _ => None,
            })
        })
        .collect()
}

/// Changes touching a drawing, oldest first.
pub fn drawing_changes(
    project: &Project,
    drawing: ObjectId,
) -> Vec<(&open_bim_model::Change, &open_bim_model::SheetChange)> {
    project
        .changes
        .iter()
        .flat_map(|c| {
            c.sheets
                .iter()
                .filter(move |s| s.drawing == drawing)
                .map(move |s| (c, s))
        })
        .collect()
}

/// Version of a set's documents: 1 for the original, +1 per change (7.3.2).
pub fn set_version(project: &Project, mark: &str) -> u32 {
    1 + project.changes.iter().filter(|c| c.mark == mark).count() as u32
}

/// Area of the main inscription with graph 27 and the QR code, kept free
/// of other content.
pub fn title_block_area(project: &Project, drawing: &Drawing) -> Result<[f64; 4], Error> {
    Ok(title_block(project, drawing, &Status::UpToDate)?.reserved)
}

/// Specification placed on the sheet above the main inscription (6.1,
/// 6.2): ГОСТ 21.502 rolled steel on КМ schemes, form 7 on assembly drawings.
pub fn specification(project: &Project, kind: &DrawingKind) -> Option<tables::Table> {
    match kind {
        DrawingKind::General { .. } => {
            let (concrete, parts): (Vec<ObjectId>, Vec<ObjectId>) = drawing_parts(project, kind)
                .into_iter()
                .partition(|id| project.concrete_type(*id).is_some());
            if parts.is_empty() {
                // A concrete arrangement lists the volumes of its pour zones.
                let zones = drawing_zones(project, &concrete);
                return (!zones.is_empty()).then(|| tables::pour_schedule_table(project, &zones));
            }
            tables::rolled_steel(project, &parts)
                .ok()
                .map(|rs| tables::rolled_steel_table(&rs))
        }
        DrawingKind::Assembly { assembly } => tables::assembly_specification(project, *assembly)
            .ok()
            .map(|rows| tables::form7(Some("Спецификация"), rows)),
        _ => None,
    }
}

/// Pour zones holding any of the given parts.
pub fn drawing_zones(project: &Project, parts: &[ObjectId]) -> Vec<open_bim_model::PourZone> {
    if parts.is_empty() {
        return Vec::new();
    }
    let parts: BTreeSet<ObjectId> = parts.iter().copied().collect();
    project
        .pour_zones()
        .into_iter()
        .filter(|z| z.pieces.iter().any(|p| parts.contains(&p.part)))
        .collect()
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
        version: set_version(project, set_mark(&drawing.kind)),
        changes: drawing_changes(project, drawing.id)
            .into_iter()
            .map(|(c, s)| title_block::ChangeRow {
                number: c.number.to_string(),
                areas: s.areas_text(),
                sheet: s.sheet_text().into(),
                document: c.permission.clone(),
                date: c.date.clone(),
            })
            .collect(),
    })?;
    // Model link state is a program mark, outside the inscription.
    if *status != Status::UpToDate {
        tb.labels.push(Label {
            position: [tb.block[0], tb.reserved[3] + 2.],
            text: format!(
                "{} · ревизия модели {}, чертежа {}",
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

/// Frames and meshes a drawing's views are drawn from: the drawing's own,
/// or on a «Комплексный чертёж» the frame of each view's object.
struct Frames {
    main: (Basis, Vec<ObjectId>, Vec<PartMesh>),
    sources: BTreeMap<ObjectId, (Basis, Vec<ObjectId>, Vec<PartMesh>)>,
}

impl Frames {
    fn new(project: &Project, drawing: &Drawing) -> Result<Self, Error> {
        let load = |kind: &DrawingKind| -> Result<(Basis, Vec<ObjectId>, Vec<PartMesh>), Error> {
            let parts = drawing_parts(project, kind);
            let meshes = parts
                .iter()
                .map(|id| PartMesh::new(project, *id))
                .collect::<Result<Vec<_>, _>>()?;
            let basis = drawing_basis(project, kind).unwrap_or(Basis {
                origin: project.origin.0,
                x: [1., 0., 0.],
                y: [0., 0., 1.],
                z: [0., -1., 0.],
            });
            Ok((basis, parts, meshes))
        };
        let mut sources = BTreeMap::new();
        let main = if let DrawingKind::Multi { objects } = &drawing.kind {
            for o in objects {
                sources.insert(*o, load(&source_kind(project, *o))?);
            }
            let world = drawing_basis(project, &drawing.kind)?;
            (world, Vec::new(), Vec::new())
        } else {
            load(&drawing.kind)?
        };
        Ok(Self { main, sources })
    }

    /// Basis, parts and meshes of one view.
    fn of(&self, view: &DrawingView) -> (&Basis, &[ObjectId], &[PartMesh]) {
        let (b, p, m) = view
            .source
            .and_then(|s| self.sources.get(&s))
            .unwrap_or(&self.main);
        (b, p, m)
    }
}

/// The view (not a node or section) under a sheet point and its context.
fn host_view<'a>(
    drawing: &'a Drawing,
    frames: &Frames,
    paper: [f64; 2],
) -> Option<(&'a DrawingView, ViewContext)> {
    let marks = BTreeMap::new();
    drawing
        .views
        .iter()
        .filter(|v| v.node.is_none() && v.section.is_none() && !v.unfolded)
        .find_map(|view| {
            let (basis, _, meshes) = frames.of(view);
            let (g, ctx) = view_graphic(drawing, view, basis, meshes, &marks);
            let [lo, hi] = g.bounds;
            (paper[0] >= lo[0] - 5.
                && paper[0] <= hi[0] + 5.
                && paper[1] >= lo[1] - 5.
                && paper[1] <= hi[1] + 5.)
                .then_some((view, ctx))
        })
}

/// The view (not a node or section) under a sheet point and the point in
/// that view's plane, model mm.
pub fn view_at(
    project: &Project,
    drawing: &Drawing,
    paper: [f64; 2],
) -> Result<Option<(u32, [f64; 2], f64)>, Error> {
    let frames = Frames::new(project, drawing)?;
    Ok(
        host_view(drawing, &frames, paper)
            .map(|(view, ctx)| (view.id, ctx.plane(paper), ctx.scale)),
    )
}

/// A section picked on the sheet: the cutting plane through a line drawn on
/// a view, looked at from the side opposite the place of the new view
/// (first-angle projection, the arrows point toward the placed view).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SectionCut {
    /// View the line was drawn on.
    pub host: u32,
    pub direction: ViewDirection,
    /// Plane depth along the new view direction, model mm.
    pub depth: f64,
    /// Object of the host view on a «Комплексный чертёж».
    pub source: Option<ObjectId>,
    /// The line was not along a view axis and was turned to the nearest one.
    pub snapped: bool,
}

/// Section through the line `a`–`b` (sheet mm) on the view under `a`; the
/// view goes to `place`. The line is taken along the nearer view axis,
/// since views look along the drawing frame axes only.
pub fn section_cut(
    project: &Project,
    drawing: &Drawing,
    a: [f64; 2],
    b: [f64; 2],
    place: [f64; 2],
) -> Result<SectionCut, Error> {
    let frames = Frames::new(project, drawing)?;
    let (host, ctx) = host_view(drawing, &frames, a)
        .ok_or("Начните линию разреза на виде (не узле и не разрезе)")?;
    let d = [b[0] - a[0], b[1] - a[1]];
    if d[0].hypot(d[1]) < 1. {
        return Err("Укажите две разные точки линии разреза".into());
    }
    let mid = [(a[0] + b[0]) / 2., (a[1] + b[1]) / 2.];
    let horizontal = d[0].abs() >= d[1].abs();
    let (normal, side) = if horizontal {
        (ctx.basis.up, place[1] - mid[1])
    } else {
        (ctx.basis.right, place[0] - mid[0])
    };
    // The viewer stands opposite the placed view.
    let toward = if side > 0. { mul(normal, -1.) } else { normal };
    let (basis, _, _) = frames.of(host);
    let direction = ViewDirection::ALL
        .into_iter()
        .find(|v| {
            let t = ViewBasis::new(basis, *v).toward;
            t[0] * toward[0] + t[1] * toward[1] + t[2] * toward[2] > 0.999
        })
        .ok_or("Разрез по этой линии не строится")?;
    let uv = ctx.plane(mid);
    let at = add(
        add(ctx.basis.origin, mul(ctx.basis.right, uv[0])),
        mul(ctx.basis.up, uv[1]),
    );
    let o = sub(at, ctx.basis.origin);
    let depth = o[0] * toward[0] + o[1] * toward[1] + o[2] * toward[2];
    let angle = d[1].atan2(d[0]).to_degrees().rem_euclid(90.);
    Ok(SectionCut {
        host: host.id,
        direction,
        depth,
        source: host.source,
        snapped: angle.min(90. - angle) > 1.,
    })
}

/// Points a sheet dimension can be tied to: member ends, plate vertices and
/// hole centres of the drawing's parts, as (view, anchor, sheet position).
pub fn anchor_candidates(
    project: &Project,
    drawing: &Drawing,
) -> Result<Vec<(u32, Anchor, [f64; 2])>, Error> {
    let parts = drawing_parts(project, &drawing.kind);
    let frames = Frames::new(project, drawing)?;
    let marks = BTreeMap::new();
    let mut anchors = Vec::new();
    for id in &parts {
        let Some(e) = project.elements.get(id) else {
            continue;
        };
        match &e.shape {
            open_bim_model::ElementShape::Member { .. } => {
                anchors.push(Anchor {
                    object: *id,
                    point: AnchorPoint::Start,
                });
                anchors.push(Anchor {
                    object: *id,
                    point: AnchorPoint::End,
                });
            }
            open_bim_model::ElementShape::Plate { contour, .. } => {
                anchors.extend((0..contour.len()).map(|index| Anchor {
                    object: *id,
                    point: AnchorPoint::Vertex { index },
                }));
            }
        }
        for d in project.details.values() {
            if let DetailKind::Hole { part, .. } = d.kind
                && part == *id
            {
                anchors.push(Anchor {
                    object: *id,
                    point: AnchorPoint::Hole { hole: d.id },
                });
            }
        }
    }
    let mut out = Vec::new();
    for view in &drawing.views {
        let (basis, shown, meshes) = frames.of(view);
        let stock = swept_drawing::flat_project(project, drawing, view)?;
        let flat;
        let source = stock.as_ref().unwrap_or(project);
        let meshes = if view.unfolded {
            let part = view
                .source
                .or(match drawing.kind {
                    DrawingKind::Part { part } => Some(part),
                    _ => None,
                })
                .ok_or("Нет детали")?;
            flat = vec![PartMesh::new(source, part)?];
            flat.as_slice()
        } else {
            meshes
        };
        let (_, context) = view_graphic(drawing, view, basis, meshes, &marks);
        let multi = matches!(drawing.kind, DrawingKind::Multi { .. });
        for a in anchors
            .iter()
            .filter(|a| !multi || shown.contains(&a.object))
        {
            if view.appearance.hidden_objects.contains(&a.object) {
                continue;
            }
            if let Some(w) = anchor_point(source, a) {
                let q = context.basis.project(w);
                out.push((view.id, *a, context.paper([q[0], q[1]])));
            }
        }
    }
    Ok(out)
}

/// Regenerate the full sheet from the model.
pub fn generate(project: &Project, drawing: &Drawing) -> Result<Sheet, Error> {
    drawing.validate().map_err(|e| e.to_string())?;
    let status = status(project, drawing);
    let parts = drawing_parts(project, &drawing.kind);
    let frames = Frames::new(project, drawing)?;
    let marks = open_bim_numbering::number(project)
        .map(|n| n.parts)
        .unwrap_or_default();
    let mut views: Vec<ViewGraphic> = Vec::new();
    let mut contexts = BTreeMap::new();
    let mut flat_projects = BTreeMap::new();
    for view in &drawing.views {
        let (basis, _, meshes) = frames.of(view);
        let stock = swept_drawing::flat_project(project, drawing, view)?;
        let flat;
        let meshes = if let Some(stock) = &stock {
            let part = view
                .source
                .or(match drawing.kind {
                    DrawingKind::Part { part } => Some(part),
                    _ => None,
                })
                .ok_or("Нет детали")?;
            flat = vec![PartMesh::new(stock, part)?];
            flat.as_slice()
        } else {
            meshes
        };
        let (mut graphic, context) = view_graphic(drawing, view, basis, meshes, &marks);
        swept_drawing::annotate(project, drawing, view, &context, &mut graphic)?;
        views.push(graphic);
        contexts.insert(view.id, context);
        if let Some(stock) = stock {
            flat_projects.insert(view.id, stock);
        }
    }
    let dimensions: Vec<DimensionGraphic> = drawing
        .dimensions
        .iter()
        .map(|d| {
            dimension(
                flat_projects.get(&d.view).unwrap_or(project),
                d,
                &contexts[&d.view],
                conventions::uses_ticks(drawing),
            )
        })
        .collect();
    // КМ schemes: titles, coordination axes and level marks (5.3–5.5).
    if matches!(drawing.kind, DrawingKind::General { .. }) {
        for view in &drawing.views {
            let ctx = &contexts[&view.id];
            if let Some(g) = views.iter_mut().find(|g| g.id == view.id) {
                if let Some(t) = g.labels.first_mut() {
                    t.text = conventions::ga_title(project, drawing, view, ctx);
                    t.height = conventions::TITLE_TEXT;
                }
                // Bubbles go beyond the dimension chains left of and below the view.
                let reach = dimensions
                    .iter()
                    .filter(|d| d.view == view.id)
                    .flat_map(|d| d.lines.iter().flat_map(|l| l.points.iter()))
                    .fold(g.bounds[0], |r, p| [r[0].min(p[0]), r[1].min(p[1])]);
                conventions::axes(project, view, ctx, g, reach);
                conventions::levels(project, view, ctx, g);
            }
        }
        // Construction joints and pour zones of the concrete shown.
        let concrete: Vec<ObjectId> = parts
            .iter()
            .copied()
            .filter(|id| project.concrete_type(*id).is_some())
            .collect();
        if !concrete.is_empty() {
            let zones = drawing_zones(project, &concrete);
            let shown: BTreeSet<ObjectId> = concrete.into_iter().collect();
            for view in drawing.views.iter().filter(|v| v.node.is_none()) {
                if let Some(g) = views.iter_mut().find(|g| g.id == view.id) {
                    pour_marks::annotate(
                        project,
                        &zones,
                        &shown,
                        &contexts[&view.id],
                        matches!(view.direction, ViewDirection::Top | ViewDirection::Bottom),
                        g,
                    );
                }
            }
        }
    }
    // Nodes (5.5.3, 5.5.4): the node view keeps what lies in its region;
    // the main view of the same direction marks the place with a thin
    // circle and the node number on a leader shelf.
    let mut node_number = 0;
    for view in &drawing.views {
        let Some(node) = view.node else { continue };
        node_number += 1;
        if let Some(g) = views.iter_mut().find(|g| g.id == view.id) {
            conventions::keep_inside(g);
            if let Some(t) = g.labels.first_mut() {
                t.text = if matches!(drawing.kind, DrawingKind::General { .. }) {
                    format!("Узел {node_number}")
                } else {
                    format!("Узел {node_number} 1:{}", format_mm(g.scale))
                };
            }
        }
        let host = drawing
            .views
            .iter()
            .find(|v| v.direction == view.direction && v.node.is_none() && v.section.is_none());
        if let Some(host) = host
            && let Some(g) = views.iter_mut().find(|g| g.id == host.id)
        {
            let ctx = &contexts[&host.id];
            conventions::node_mark(
                g,
                ctx.paper(node.centre),
                node.radius / ctx.scale,
                node_number,
            );
        }
    }
    // Section lines with arrows on the first view the cutting plane crosses
    // edge-on (a vertical section shows on the plan).
    for section in drawing.views.iter().filter(|v| v.section.is_some()) {
        let t = contexts[&section.id].basis.toward;
        let host = drawing.views.iter().find(|v| {
            if let Some(id) = section.section_host {
                return v.id == id;
            }
            if v.source != section.source || v.node.is_some() || v.section.is_some() {
                return false;
            }
            let h = contexts[&v.id].basis.toward;
            v.id != section.id && (t[0] * h[0] + t[1] * h[1] + t[2] * h[2]).abs() < 1e-6
        });
        if let Some(host) = host
            && let Some(g) = views.iter_mut().find(|g| g.id == host.id)
        {
            conventions::section_traces(
                drawing,
                &contexts[&host.id],
                &[(section, &contexts[&section.id])],
                g,
            );
        }
    }
    // Weld signs in the main view (ГОСТ 2.312): leader from the weld to a
    // shelf, the designation above the shelf.
    if let Some(main) = drawing.views.first()
        && let Some(context) = contexts.get(&main.id)
        && let Some(graphic) = views.iter_mut().find(|v| v.id == main.id)
    {
        let shown: BTreeSet<ObjectId> = parts.iter().copied().collect();
        let metrics = font::Font::load();
        for (k, detail) in project.details.values().enumerate() {
            let DetailKind::Weld {
                parts: welded,
                path,
                shop,
                around,
                gost,
                ..
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
            let text = detail.kind.weld_mark().unwrap_or_default();
            // Signs on the leader break (ГОСТ 2.312): a circle for a weld
            // all around, a flag for a site weld; the text starts after them.
            let lead = if *around || !*shop { 2. } else { 0. };
            // Reverse side: the designation goes under the shelf.
            let under = gost.back;
            // The leg sign is drawn as lines: the sheet font has no glyph.
            let (head, tail) = text
                .split_once(open_bim_model::LEG_SIGN)
                .map_or((text.as_str(), None), |(h, t)| (h, Some(t)));
            let text_width = |s: &str| {
                metrics.as_ref().map_or_else(
                    |_| s.chars().count() as f64 * TEXT_HEIGHT * 0.62,
                    |m| m.width(s, TEXT_HEIGHT),
                )
            };
            let head_width = text_width(head);
            let width = lead
                + head_width
                + tail.map_or(0., |t| LEG_SIGN_WIDTH * TEXT_HEIGHT + text_width(t))
                + 1.;
            // Shelf up-right of the weld, moved further out and to the left
            // until the sign clears the marks and other signs.
            let boxes: Vec<[[f64; 2]; 2]> = graphic
                .labels
                .iter()
                .map(|l| {
                    let w = metrics.as_ref().map_or_else(
                        |_| l.text.chars().count() as f64 * l.height * 0.62,
                        |m| m.width(&l.text, l.height),
                    );
                    let x0 = match l.align {
                        Align::Start => l.position[0],
                        Align::Middle => l.position[0] - w / 2.,
                        Align::End => l.position[0] - w,
                    };
                    [
                        [x0, l.position[1] - 0.3 * l.height],
                        [x0 + w, l.position[1] + l.height],
                    ]
                })
                .collect();
            let sign_box = |shelf: [f64; 2], left: bool| {
                let x0 = if left { shelf[0] - width } else { shelf[0] };
                let (y0, y1) = if under {
                    (shelf[1] - TEXT_HEIGHT - 1.5, shelf[1])
                } else {
                    (shelf[1], shelf[1] + TEXT_HEIGHT + 1.)
                };
                [[x0 - 0.5, y0], [x0 + width + 0.5, y1]]
            };
            let paper = drawing.sheet_mm();
            let clear = |r: [[f64; 2]; 2]| {
                r[0][0] >= MARGIN_LEFT + 1.
                    && r[1][0] <= paper[0] - MARGIN - 1.
                    && r[0][1] >= MARGIN + 1.
                    && r[1][1] <= paper[1] - MARGIN - 1.
                    && boxes.iter().all(|q| {
                        r[1][0] <= q[0][0]
                            || q[1][0] <= r[0][0]
                            || r[1][1] <= q[0][1]
                            || q[1][1] <= r[0][1]
                    })
            };
            let first = 10. + 6. * (k % 3) as f64;
            let (shelf, left) = [0., 6., 12., 18., 24., 30.]
                .iter()
                .flat_map(|extra| [(first + extra, false), (first + extra, true)])
                .map(|(lift, left)| {
                    let dx = if left { -lift } else { lift };
                    ([at[0] + dx, at[1] + lift], left)
                })
                .find(|(shelf, left)| clear(sign_box(*shelf, *left)))
                .unwrap_or(([at[0] + first, at[1] + first], false));
            let shelf_end = if left {
                shelf[0] - width
            } else {
                shelf[0] + width
            };
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
                .push(line(vec![at, shelf, [shelf_end, shelf[1]]]));
            if *around {
                graphic.lines.push(line(
                    (0..=24)
                        .map(|i| {
                            let a = std::f64::consts::TAU * i as f64 / 24.;
                            [shelf[0] + 1.5 * a.cos(), shelf[1] + 1.5 * a.sin()]
                        })
                        .collect(),
                ));
            }
            if !*shop {
                // «┐»: up from the break, the flag away from the shelf.
                let back = if left { 2.5 } else { -2.5 };
                let top = shelf[1] + 4.;
                graphic
                    .lines
                    .push(line(vec![shelf, [shelf[0], top], [shelf[0] + back, top]]));
            }
            let start = if left {
                shelf_end + 0.5
            } else {
                shelf[0] + lead + 0.5
            };
            let base = if under {
                [start, shelf[1] - 1. - 0.75 * TEXT_HEIGHT]
            } else {
                [start, shelf[1] + 1.]
            };
            let label = |position: [f64; 2], text: &str| Label {
                position,
                text: text.into(),
                height: TEXT_HEIGHT,
                align: Align::Start,
                rotation: 0.,
                object: Some(detail.id),
                lost: false,
            };
            graphic.labels.push(label(base, head));
            if let Some(tail) = tail {
                // ⊿: right angle at the lower right, cap height tall.
                let x0 = base[0] + head_width + 0.15 * TEXT_HEIGHT;
                let x1 = base[0] + head_width + (LEG_SIGN_WIDTH - 0.15) * TEXT_HEIGHT;
                let top = base[1] + 0.7 * TEXT_HEIGHT;
                graphic.lines.push(line(vec![
                    [x0, base[1]],
                    [x1, base[1]],
                    [x1, top],
                    [x0, base[1]],
                ]));
                let after = [base[0] + head_width + LEG_SIGN_WIDTH * TEXT_HEIGHT, base[1]];
                graphic.labels.push(label(after, tail));
            }
        }
    }
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
        // Axis bubbles and other view annotations below the outline too.
        let lowest = view
            .lines
            .iter()
            .flat_map(|l| l.points.iter().map(|p| p[1]))
            .fold(lowest, f64::min);
        if let Some(title) = view.labels.first_mut() {
            title.position[1] = lowest - 10.;
        }
    }
    let block = title_block(project, drawing, &status)?;
    let title_area = block.reserved;
    let (mut frame, mut labels, fills) = (block.lines, block.labels, block.fills);
    let size = drawing.sheet_mm();
    if let Some(table) = specification(project, &drawing.kind) {
        let at = [
            size[0] - MARGIN - table.width(),
            title_area[3] + 3. + table.height(),
        ];
        let drawn = table.draw(at);
        frame.extend(drawn.lines);
        labels.extend(drawn.labels);
    }
    if matches!(drawing.kind, DrawingKind::GeneralData { .. }) {
        let (lines, texts) = general_data::content(project, drawing, size, title_area)?;
        frame.extend(lines);
        labels.extend(texts);
    }
    // Changed areas (appendix Л): a thin closed contour, the designation
    // «change.area» in a parallelogram outside it with a thin leader.
    let metrics = font::Font::load().ok();
    for (change, sheet_change) in drawing_changes(project, drawing.id) {
        let open_bim_model::SheetChangeKind::Changed { areas } = &sheet_change.kind else {
            continue;
        };
        for (k, a) in areas.iter().enumerate() {
            let thin = |points: Vec<[f64; 2]>| Polyline {
                points,
                style: LineStyle::Thin,
                object: None,
            };
            frame.push(thin(vec![
                [a[0], a[1]],
                [a[2], a[1]],
                [a[2], a[3]],
                [a[0], a[3]],
                [a[0], a[1]],
            ]));
            let text = format!("{}.{}", change.number, k + 1);
            let w = metrics
                .as_ref()
                .map_or(text.chars().count() as f64 * 2., |m| {
                    m.width(&text, TEXT_HEIGHT)
                })
                + 4.;
            let (x, y) = (a[0] - w - 4., a[3] + 4.);
            let (x, y) = if x < MARGIN_LEFT + 1. {
                (a[0], y)
            } else {
                (x, y)
            };
            let (h, slant) = (5., 1.5);
            frame.push(thin(vec![
                [x, y],
                [x + w, y],
                [x + w + slant, y + h],
                [x + slant, y + h],
                [x, y],
            ]));
            frame.push(thin(vec![[x + w / 2., y], [a[0], a[3]]]));
            labels.push(Label {
                position: [x + slant / 2. + w / 2., y + 1.5],
                text,
                height: TEXT_HEIGHT,
                align: Align::Middle,
                rotation: 0.,
                object: None,
                lost: false,
            });
        }
    }
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
        objects: drawing.objects.iter().map(sheet_objects::graphic).collect(),
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
    // КМ schemes need room for axis bubbles and view titles between views.
    let gap = if matches!(kind, DrawingKind::General { .. }) {
        60.
    } else {
        30.
    };
    let column = |i: usize| sizes.get(i).copied().unwrap_or([0.; 2]);
    let main = column(0);
    let below = directions.iter().position(|d| *d == ViewDirection::Top);
    let side = directions
        .iter()
        .position(|d| matches!(d, ViewDirection::Left | ViewDirection::Right));
    // The specification above the inscription takes either the strip above
    // it or the column at the right, whichever leaves the larger scale.
    let spec = specification(project, kind).map(|t| [t.width() + 10., t.height() + 30.]);
    let fields = match spec {
        Some([sw, sh]) => vec![[field[0], field[1] - sh], [field[0] - sw, field[1]]],
        None => vec![field],
    };
    let fits = |s: f64| {
        let width = main[0] / s + side.map_or(0., |i| gap + column(i)[0] / s);
        let height = main[1] / s + below.map_or(0., |i| gap + column(i)[1] / s);
        fields
            .iter()
            .any(|f| width + gap <= f[0] && height + gap <= f[1])
    };
    let scale = SCALES
        .iter()
        .copied()
        .find(|s| fits(*s))
        .ok_or("Виды не помещаются на листе даже в масштабе 1:400")?;
    // КМ schemes leave room for axis bubbles left of the views.
    let left = MARGIN_LEFT
        + if matches!(kind, DrawingKind::General { .. }) {
            35.
        } else {
            15.
        };
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
            node: None,
            source: None,
            section_host: None,
            appearance: Default::default(),
            unfolded: false,
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
    if project.swept_parts.contains_key(&part) {
        return swept_drawing::part_drawing(project, id, part, number);
    }
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
        objects: Vec::new(),
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
            objects: Vec::new(),
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
    // Chain over the columns of the front row on the elevation, with ticks.
    let mut front: Vec<(f64, f64, ObjectId)> = drawing_parts(project, &kind)
        .iter()
        .filter_map(|id| match &project.elements.get(id)?.shape {
            ElementShape::Member {
                member_kind: MemberKind::Column,
                frame,
                ..
            } => Some((frame.origin.0[1], frame.origin.0[0], *id)),
            _ => None,
        })
        .collect();
    let nearest = front.iter().map(|c| c.0).fold(f64::INFINITY, f64::min);
    front.retain(|c| c.0 - nearest < 1.);
    front.sort_by(|a, b| a.1.total_cmp(&b.1));
    front.dedup_by(|a, b| (a.1 - b.1).abs() < 1.);
    let dimensions = if front.len() >= 2 {
        vec![DimensionDef {
            id: 1,
            view: 1,
            anchors: front
                .iter()
                .map(|c| Anchor {
                    object: c.2,
                    point: AnchorPoint::Start,
                })
                .collect(),
            axis: DimensionAxis::Horizontal,
            offset: -20.,
        }]
    } else {
        Vec::new()
    };
    Ok(updated(
        project,
        &Drawing {
            id,
            kind,
            number: number.into(),
            // КМ drawings are element arrangement schemes (5.5.17).
            title: format!("Схема расположения элементов · {}", project.name),
            sheet,
            scale,
            views: views(&directions, &positions),
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

/// Default «Комплексный чертёж»: for each part or assembly of `objects` a
/// front view (and a view from above for members and assemblies) with the
/// overall length, packed in rows on the smallest sheet from A3 to A1 at
/// the largest standard scale that fits.
pub fn multi_drawing(
    project: &Project,
    id: ObjectId,
    objects: Vec<ObjectId>,
    number: &str,
) -> Result<Drawing, Error> {
    if objects.is_empty() {
        return Err("Выберите детали или сборки".into());
    }
    let numbering = open_bim_numbering::number(project).ok();
    let mark = |o: ObjectId| {
        numbering
            .as_ref()
            .and_then(|n| n.parts.get(&o).or_else(|| n.assemblies.get(&o)))
            .cloned()
            .unwrap_or_else(|| format!("#{}", o.0))
    };
    // Per object: views (direction, size in model mm) and the length anchor.
    struct Block {
        object: ObjectId,
        views: Vec<(ViewDirection, [f64; 2])>,
        length: Option<ObjectId>,
    }
    let mut blocks = Vec::new();
    for object in &objects {
        let kind = source_kind(project, *object);
        let main = match kind {
            DrawingKind::Assembly { assembly } => project.assemblies[&assembly].main_part,
            _ => *object,
        };
        let member = matches!(
            project
                .elements
                .get(&main)
                .ok_or_else(|| format!("Деталь #{} не найдена", main.0))?
                .shape,
            ElementShape::Member { .. }
        );
        let basis = drawing_basis(project, &kind)?;
        let meshes = drawing_parts(project, &kind)
            .iter()
            .map(|id| PartMesh::new(project, *id))
            .collect::<Result<Vec<_>, _>>()?;
        let mut directions = vec![ViewDirection::Front];
        if member || matches!(kind, DrawingKind::Assembly { .. }) {
            directions.push(ViewDirection::Top);
        }
        blocks.push(Block {
            object: *object,
            views: directions
                .into_iter()
                .map(|d| (d, extents(&meshes, &basis, d)))
                .collect(),
            length: member.then_some(main),
        });
    }
    let gap = 30.;
    // Block size on paper at scale 1:s, the views stacked.
    let size = |b: &Block, s: f64| {
        let w = b.views.iter().map(|v| v.1[0] / s).fold(0., f64::max);
        let h = b.views.iter().map(|v| v.1[1] / s).sum::<f64>() + gap * (b.views.len() - 1) as f64;
        [w, h]
    };
    // Rows of blocks inside the field above the title block, or None.
    let pack = |sheet: SheetSize, s: f64| -> Option<Vec<[f64; 2]>> {
        let [w, h] = sheet.landscape_mm();
        let (left, right, top) = (MARGIN_LEFT + 15., w - MARGIN - 10., h - MARGIN - 35.);
        let bottom = MARGIN + TITLE_BLOCK[1] + 10.;
        let mut corners = Vec::new();
        let (mut x, mut y, mut row) = (left, top, 0.0_f64);
        for b in &blocks {
            let [bw, bh] = size(b, s);
            if x > left && x + bw > right {
                x = left;
                y -= row + gap;
                row = 0.;
            }
            if x + bw > right || y - bh < bottom {
                return None;
            }
            corners.push([x, y]);
            x += bw + gap;
            row = row.max(bh);
        }
        Some(corners)
    };
    let (sheet, scale, corners) = [SheetSize::A3, SheetSize::A2, SheetSize::A1]
        .into_iter()
        .find_map(|sheet| {
            SCALES
                .iter()
                .find_map(|s| pack(sheet, *s).map(|c| (sheet, *s, c)))
        })
        .ok_or("Объекты не помещаются на листе A1 даже в масштабе 1:400")?;
    let mut views = Vec::new();
    let mut dimensions = Vec::new();
    for (b, corner) in blocks.iter().zip(corners) {
        let width = size(b, scale)[0];
        let mut top = corner[1];
        for (k, (direction, extent)) in b.views.iter().enumerate() {
            let id = views.len() as u32 + 1;
            let h = extent[1] / scale;
            views.push(DrawingView {
                id,
                name: match direction {
                    ViewDirection::Top => format!("{} — вид сверху", mark(b.object)),
                    _ => mark(b.object),
                },
                direction: *direction,
                section: None,
                position: [corner[0] + width / 2., top - h / 2.],
                scale: None,
                node: None,
                source: Some(b.object),
                section_host: None,
                appearance: Default::default(),
                unfolded: false,
            });
            top -= h + gap;
            if k == 0
                && let Some(part) = b.length
            {
                dimensions.push(DimensionDef {
                    id: dimensions.len() as u32 + 1,
                    view: id,
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
                    axis: DimensionAxis::Horizontal,
                    offset: 12.,
                });
            }
        }
    }
    Ok(updated(
        project,
        &Drawing {
            id,
            kind: DrawingKind::Multi { objects },
            number: number.into(),
            title: "Комплексный чертёж".into(),
            sheet,
            scale,
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
