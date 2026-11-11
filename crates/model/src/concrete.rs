//! Concrete data that is not a part: pour breaks, the calculated pour
//! units («Бетонирование»), reinforcing bar sets («Армирование») and the
//! formwork panels and props («Опалубка»).
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::rebar::{Hook, RebarRule};
use crate::{ModelError, ObjectId, Point3};

/// «Шов бетонирования». A vertical joint is the plane through the line
/// `start`–`end` seen from above and reaches as far as that line. A
/// horizontal joint is the plane at the level of `start` over the plan
/// rectangle with the corners `start` and `end`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PourBreak {
    pub start: Point3,
    pub end: Point3,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub horizontal: bool,
}

impl PourBreak {
    /// Unit normal and offset of the joint plane, normal·p = offset. A
    /// vertical joint's normal points left of start→end seen from above.
    pub fn plane(&self) -> ([f64; 3], f64) {
        let (a, b) = (self.start.0, self.end.0);
        if self.horizontal {
            return ([0., 0., 1.], a[2]);
        }
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let l = dx.hypot(dy).max(1e-12);
        let n = [-dy / l, dx / l, 0.];
        (n, n[0] * a[0] + n[1] * a[1])
    }

    /// Signed distance of a point from the joint plane, mm.
    pub fn side(&self, p: [f64; 3]) -> f64 {
        let (n, d) = self.plane();
        n[0] * p[0] + n[1] * p[1] + n[2] * p[2] - d
    }

    /// Whether a point of the joint plane lies within the joint's reach.
    pub fn covers(&self, x: [f64; 3]) -> bool {
        let (a, b) = (self.start.0, self.end.0);
        if self.horizontal {
            return (0..2).all(|k| x[k] >= a[k].min(b[k]) - 1. && x[k] <= a[k].max(b[k]) + 1.);
        }
        let d = [b[0] - a[0], b[1] - a[1]];
        let u = ((x[0] - a[0]) * d[0] + (x[1] - a[1]) * d[1]) / (d[0] * d[0] + d[1] * d[1]);
        (-1e-9..=1. + 1e-9).contains(&u)
    }

    /// Whether the joint passes between two points: they lie on opposite
    /// sides and the segment between them meets the plane within reach.
    pub fn separates(&self, p: [f64; 3], q: [f64; 3]) -> bool {
        let (sp, sq) = (self.side(p), self.side(q));
        if sp * sq >= 0. {
            return false;
        }
        let t = sp / (sp - sq);
        self.covers(std::array::from_fn(|k| p[k] + (q[k] - p[k]) * t))
    }

    /// Whether the joint cuts a box [min, max]: the plane passes through it
    /// and the joint reaches it in plan.
    pub fn cuts(&self, b: &[[f64; 3]; 2]) -> bool {
        const EPS: f64 = 1.;
        let sides: Vec<f64> = (0..8)
            .map(|i| self.side(std::array::from_fn(|k| b[(i >> k) & 1][k])))
            .collect();
        let (lo, hi) = sides
            .iter()
            .fold((f64::MAX, f64::MIN), |(l, h), s| (l.min(*s), h.max(*s)));
        if lo > -EPS || hi < EPS {
            return false;
        }
        let (a, e) = (self.start.0, self.end.0);
        if self.horizontal {
            return (0..2).all(|k| a[k].min(e[k]) < b[1][k] && a[k].max(e[k]) > b[0][k]);
        }
        // The joint line clipped to the box in plan (Liang–Barsky).
        let (mut t0, mut t1) = (0f64, 1f64);
        for k in 0..2 {
            let d = e[k] - a[k];
            if d.abs() < 1e-12 {
                if a[k] < b[0][k] || a[k] > b[1][k] {
                    return false;
                }
                continue;
            }
            let (u, v) = ((b[0][k] - a[k]) / d, (b[1][k] - a[k]) / d);
            t0 = t0.max(u.min(v));
            t1 = t1.min(u.max(v));
        }
        t0 < t1
    }
}

/// A calculated pour unit: cast-in-place parts of one grade placed at once.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PourUnit {
    pub number: u32,
    pub grade: String,
    pub parts: BTreeSet<ObjectId>,
    /// Centre of the zone when it was stored: tells apart the zones of a
    /// part split by a joint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub centre: Option<Point3>,
}

/// How a bar set was made.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RebarKind {
    /// «Продольные»: bars along a beam or column.
    Longitudinal,
    /// «Поперечные»: closed stirrups along a beam or column.
    Stirrup,
    /// «По грани»: meshes at the faces of a slab or panel.
    Mesh,
    /// «По направляющим»: a bent bar along picked points.
    Guide,
    /// «Группа стержней»: parallel bars distributed between picks.
    Group,
    /// «Один стержень».
    Single,
    /// «П-образные»: U-bars along a slab edge or an opening.
    Edge,
    /// «Лягушки»: support bars between the meshes, as a line array.
    Frog,
    /// «По типу формы»: a bar instantiated from a shape type.
    Shape,
}

impl RebarKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Longitudinal => "Продольные",
            Self::Stirrup => "Поперечные",
            Self::Mesh => "По грани",
            Self::Guide => "По направляющим",
            Self::Group => "Группа стержней",
            Self::Single => "Один стержень",
            Self::Edge => "П-образные",
            Self::Frog => "Лягушки",
            Self::Shape => "По типу формы",
        }
    }
}

/// Property of a concrete part: concrete cover to the bar surface, mm.
/// Area reinforcement reads the cover from the host, not from the slab type.
pub const REBAR_COVER: &str = "rebar_cover";

/// Role of a set inside an area-reinforcement zone. Kept so a rebuild can
/// reuse the position number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZoneRole {
    BottomMain,
    BottomCross,
    TopMain,
    TopCross,
    Edge,
    Opening,
    Frog,
}

/// Where the first bar of a mesh is measured from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BarAlign {
    /// First bar at the cover from the edge; the last interval is the
    /// remainder step («доборный шаг») when the span is not a whole number
    /// of spacings.
    Edge,
    /// Bars measured both ways from the axis of the zone (the middle of the
    /// span); remainder steps fall toward the edges.
    Axis,
}

/// One straight portion of a bar shape, between bend tangents.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShapeSegment {
    /// Schedule letter: «A», «B», «C»… The same letter on several segments
    /// is one parameter (both risers of a frog).
    pub letter: String,
    /// Length between the bend tangents, mm.
    pub length: f64,
    /// Turn at the end of the segment, degrees, positive to the left in the
    /// shape plane. 0 — the bar ends.
    pub bend_deg: f64,
}

/// A bar shape type («тип формы»): centreline, segment parameters, hooks and
/// mandrel. Sets store the id and follow the type. Built-in forms live in
/// `rebar::builtin_shapes`; a project entry with the same id replaces one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BarShapeType {
    pub id: String,
    pub name: String,
    /// Short mark kept for a future detail schedule and BVBS. Not a GOST number.
    pub code: String,
    pub segments: Vec<ShapeSegment>,
    pub hooks: [Hook; 2],
    /// Mandrel diameter, mm. None — the minimum of СП 63 for the bar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mandrel: Option<f64>,
}

/// Where a shape instance sits. With `local_to_host` the vectors are in the
/// host part's frame and the bar follows that part.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShapePlace {
    pub origin: Point3,
    pub x: [f64; 3],
    pub y: [f64; 3],
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub local_to_host: bool,
}

/// Frogs of one set, generated along lines rather than one by one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrogArray {
    /// Start and end of each line in the host plane, mm.
    pub lines: Vec<([f64; 2], [f64; 2])>,
    pub spacing: f64,
}

/// One mesh of a slab type: bars of one diameter, a spacing and a direction,
/// plus the perpendicular bars when `cross_spacing` is set.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeshLayer {
    pub diameter: f64,
    pub grade: String,
    /// Spacing of the primary bars, mm.
    pub spacing: f64,
    /// Direction of the primary bars, degrees from the host X axis.
    pub direction_deg: f64,
    /// Perpendicular bars. None — the mesh has one direction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cross_spacing: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cross_diameter: Option<f64>,
}

impl MeshLayer {
    pub fn new(diameter: f64, spacing: f64, direction_deg: f64) -> Self {
        Self {
            diameter,
            grade: "А500С".into(),
            spacing,
            direction_deg,
            cross_spacing: Some(spacing),
            cross_diameter: None,
        }
    }

    pub fn cross_diameter(&self) -> f64 {
        self.cross_diameter.unwrap_or(self.diameter)
    }
}

/// Reinforcement stored on a slab type and applied to slabs of that type.
/// The cover is not here: it lives on the concrete host (`REBAR_COVER`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlabRebarType {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottom: Option<MeshLayer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<MeshLayer>,
    /// Cut mesh bars at openings. When false the bars run through.
    pub cut_openings: bool,
    pub align: BarAlign,
    pub edge_shape: String,
    pub edge_diameter: f64,
    pub edge_grade: String,
    pub edge_spacing: f64,
    pub frog_shape: String,
    pub frog_diameter: f64,
    pub frog_grade: String,
    pub frog_spacing: f64,
    /// Place frogs when both meshes exist.
    pub frogs: bool,
    /// Place U-bars when both meshes exist.
    pub edge_bars: bool,
    /// Applied to a slab or a raft as it is created.
    pub default_for_new: bool,
}

impl SlabRebarType {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            bottom: None,
            top: None,
            cut_openings: false,
            align: BarAlign::Edge,
            edge_shape: "gost.u".into(),
            edge_diameter: 8.,
            edge_grade: "А500С".into(),
            edge_spacing: 200.,
            frog_shape: "gost.frog".into(),
            frog_diameter: 8.,
            frog_grade: "А500С".into(),
            frog_spacing: 600.,
            frogs: true,
            edge_bars: true,
            default_for_new: false,
        }
    }
}

/// A stretch the layout refused to reinforce, kept for the model check.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RebarCheck {
    pub message: String,
    /// Opening index among the through openings of the host; none — the
    /// outer contour, or a note that is not tied to a side.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opening: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge: Option<u32>,
    /// Distances along the edge, mm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<f64>,
}

/// Area reinforcement of one slab. The mark sits at the zone centre and
/// lists the bar positions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RebarZone {
    pub number: u32,
    pub host: ObjectId,
    pub type_id: String,
    /// Outer edges with U-bars turned off, by contour index.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edge_off: Vec<u32>,
    /// Opening index and edge index with U-bars turned off.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub opening_off: Vec<[u32; 2]>,
    /// «1, 2, 4».
    pub mark: String,
    pub mark_at: Point3,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<RebarCheck>,
}

/// Steel density of reinforcing bars, kg/m³.
pub const REBAR_DENSITY: f64 = 7850.;

/// A set of reinforcing bars of one diameter and grade; each bar is the
/// polyline of its centre line, mm.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RebarSet {
    pub number: u32,
    pub kind: RebarKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<ObjectId>,
    pub diameter: f64,
    pub grade: String,
    /// Centre lines through the bend corners (tangent intersections), hooks
    /// included.
    pub bars: Vec<Vec<Point3>>,
    /// Ends of the bars: start, end.
    #[serde(default, skip_serializing_if = "no_hooks")]
    pub hooks: [Hook; 2],
    /// Mandrel diameter of the bends, mm; the smallest of the class when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bend_diameter: Option<f64>,
    /// Layout in the host part, kept to build the bars again when the host
    /// changes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<RebarRule>,
    /// Shape type. Segment lengths in `shape_params` override the type;
    /// everything else follows the type and is rebuilt when it changes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape_id: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub shape_params: BTreeMap<String, f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place: Option<ShapePlace>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zone: Option<u32>,
    /// Position shown on the zone mark. Stable across a rebuild.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<ZoneRole>,
    /// Remainder steps of a mesh, mm.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remainders: Vec<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub array: Option<FrogArray>,
}

fn no_hooks(h: &[Hook; 2]) -> bool {
    h.iter().all(Hook::is_none)
}

impl RebarSet {
    pub fn basic(kind: RebarKind, diameter: f64, grade: impl Into<String>) -> Self {
        Self {
            number: 0,
            kind,
            host: None,
            diameter,
            grade: grade.into(),
            bars: Vec::new(),
            hooks: [Hook::default(); 2],
            bend_diameter: None,
            rule: None,
            shape_id: None,
            shape_params: BTreeMap::new(),
            place: None,
            zone: None,
            position: None,
            role: None,
            remainders: Vec::new(),
            array: None,
        }
    }
}

impl RebarSet {
    pub fn bar_length(bar: &[Point3]) -> f64 {
        bar.windows(2)
            .map(|w| {
                (0..3)
                    .map(|k| (w[1].0[k] - w[0].0[k]).powi(2))
                    .sum::<f64>()
                    .sqrt()
            })
            .sum()
    }

    /// Mandrel diameter of the bends, mm.
    pub fn mandrel(&self) -> f64 {
        self.bend_diameter.unwrap_or_else(|| {
            crate::rebar::min_bend_diameter(crate::rebar::rebar_class(&self.grade), self.diameter)
        })
    }

    /// Bend radius of the centre line, mm.
    pub fn bend_radius(&self) -> f64 {
        (self.mandrel() + self.diameter) / 2.
    }

    /// Cut length of a bar: the corner polyline less, at each bend through
    /// θ, the two tangents R·tan(θ/2) plus the arc R·θ.
    pub fn developed_length(&self, bar: &[Point3]) -> f64 {
        let r = self.bend_radius();
        let dir = |a: &Point3, b: &Point3| {
            let v: [f64; 3] = std::array::from_fn(|k| b.0[k] - a.0[k]);
            let l = v.iter().map(|x| x * x).sum::<f64>().sqrt();
            (l > 1e-9).then(|| v.map(|x| x / l))
        };
        let mut length = Self::bar_length(bar);
        for w in bar.windows(3) {
            let (Some(u), Some(v)) = (dir(&w[0], &w[1]), dir(&w[1], &w[2])) else {
                continue;
            };
            let cos = (0..3).map(|k| u[k] * v[k]).sum::<f64>().clamp(-1., 1.);
            let theta = cos.acos().min(179f64.to_radians());
            if theta > 1e-6 {
                length -= 2. * r * (theta / 2.).tan() - r * theta;
            }
        }
        length
    }

    pub fn total_length_mm(&self) -> f64 {
        self.bars.iter().map(|b| self.developed_length(b)).sum()
    }

    /// Mass by the nominal mass per metre of the diameter.
    pub fn mass_kg(&self) -> f64 {
        crate::rebar::mass_per_m(self.diameter) * self.total_length_mm() / 1000.
    }
}

/// A formwork panel: the rectangle `origin + u·[0, width] + v·[0, height]`
/// on a face of its concrete part, mm.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormPanel {
    pub host: ObjectId,
    pub origin: Point3,
    pub u: [f64; 3],
    pub v: [f64; 3],
    pub width: f64,
    pub height: f64,
}

impl FormPanel {
    pub fn area_m2(&self) -> f64 {
        self.width * self.height / 1e6
    }

    pub fn corners(&self) -> [Point3; 4] {
        let at = |a: f64, b: f64| {
            Point3(std::array::from_fn(|k| {
                self.origin.0[k] + self.u[k] * a + self.v[k] * b
            }))
        };
        [
            at(0., 0.),
            at(self.width, 0.),
            at(self.width, self.height),
            at(0., self.height),
        ]
    }
}

/// A vertical formwork prop from `base` up to `top`, mm.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Prop {
    pub host: ObjectId,
    pub base: Point3,
    pub top: Point3,
}

impl Prop {
    pub fn length(&self) -> f64 {
        self.top.0[2] - self.base.0[2]
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Concrete {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub breaks: Vec<PourBreak>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub units: Vec<PourUnit>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rebars: Vec<RebarSet>,
    /// User bar shapes. An entry whose id matches a built-in form replaces it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shape_types: Vec<BarShapeType>,
    /// Slab types that carry the default meshes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub slab_types: Vec<SlabRebarType>,
    /// Area-reinforcement zones.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub zones: Vec<RebarZone>,
    /// Cover of a new slab when the host has no `REBAR_COVER` yet, mm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_cover: Option<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub forms: Vec<FormPanel>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub props: Vec<Prop>,
}

impl Concrete {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn validate(&self) -> Result<(), ModelError> {
        let bad = |m: &str| Err(ModelError::InvalidValue(m.into()));
        for b in &self.breaks {
            let (s, e) = (b.start.0, b.end.0);
            if !s.iter().chain(&e).all(|v| v.is_finite() && v.abs() <= 1e9) {
                return bad("Шов бетонирования: неверные координаты");
            }
            if b.horizontal && ((e[0] - s[0]).abs() < 1. || (e[1] - s[1]).abs() < 1.) {
                return bad("Горизонтальный шов: укажите два угла области в плане");
            }
            if (e[0] - s[0]).hypot(e[1] - s[1]) < 1. {
                return bad("Шов бетонирования: точки совпадают в плане");
            }
        }
        // A part split by a joint is poured in two units, so only the
        // numbers must differ.
        let mut seen = BTreeSet::new();
        for u in &self.units {
            if !seen.insert(u.number) {
                return bad("Номер захватки повторяется");
            }
        }
        let mut numbers = BTreeSet::new();
        let mut bars = 0;
        for r in &self.rebars {
            if !numbers.insert(r.number) {
                return bad("Номер арматурного набора повторяется");
            }
            if !(3.0..=60.).contains(&r.diameter) {
                return bad("Диаметр арматуры от 3 до 60 мм");
            }
            if r.grade.trim().is_empty() || r.grade.len() > 32 {
                return bad("Класс арматуры не задан");
            }
            if r.bars.is_empty() {
                return bad("Арматурный набор без стержней");
            }
            if r.hooks
                .iter()
                .any(|h| h.length.is_some_and(|l| !(0.0..=10_000.).contains(&l)))
            {
                return bad("Прямой участок крюка от 0 до 10 000 мм");
            }
            if r.bend_diameter
                .is_some_and(|d| !(r.diameter..=10_000.).contains(&d))
            {
                return bad("Диаметр оправки не меньше диаметра стержня");
            }
            if let Some(rule) = &r.rule
                && (!(0.0..=500.).contains(&rule.cover)
                    || !(r.diameter..=10_000.).contains(&rule.spacing)
                    || !(2..=50).contains(&rule.per_face)
                    || rule
                        .stock_length
                        .is_some_and(|s| !(500.0..=100_000.).contains(&s)))
            {
                return bad("Правило раскладки арматуры: неверные значения");
            }
            for b in &r.bars {
                if b.len() < 2
                    || !b
                        .iter()
                        .all(|p| p.0.iter().all(|v| v.is_finite() && v.abs() <= 1e9))
                {
                    return bad("Стержень: нужно не меньше двух точек");
                }
                if RebarSet::bar_length(b) < 1. {
                    return bad("Стержень нулевой длины");
                }
            }
            bars += r.bars.len();
        }
        let finite = |v: &[f64]| v.iter().all(|x| x.is_finite() && x.abs() <= 1e9);
        for f in &self.forms {
            let unit = |a: [f64; 3]| (a.iter().map(|x| x * x).sum::<f64>() - 1.).abs() < 1e-6;
            if !finite(&f.origin.0) || !unit(f.u) || !unit(f.v) {
                return bad("Щит опалубки: неверное положение");
            }
            if !(f.width > 0. && f.height > 0. && f.width <= 1e6 && f.height <= 1e6) {
                return bad("Щит опалубки: размеры должны быть положительными");
            }
        }
        for p in &self.props {
            if !finite(&p.base.0) || !finite(&p.top.0) || p.length() < 1. {
                return bad("Опора: верх должен быть выше низа");
            }
        }
        if self.forms.len() > 1_000_000 || self.props.len() > 1_000_000 {
            return bad("Слишком много щитов или опор");
        }
        if bars > 1_000_000 {
            return bad("Слишком много стержней");
        }
        self.validate_shapes()?;
        self.validate_slab_types()?;
        self.validate_zones()?;
        let mut positions = BTreeSet::new();
        for r in &self.rebars {
            if let Some(n) = r.position
                && (n == 0 || !positions.insert(n))
            {
                return bad("Номер позиции арматуры повторяется");
            }
            if r.shape_params
                .values()
                .any(|v| !(1.0..=100_000.).contains(v))
            {
                return bad("Длина участка формы от 1 до 100 000 мм");
            }
            if r.remainders.iter().any(|v| !(0.0..=100_000.).contains(v)) {
                return bad("Доборный шаг задан неверно");
            }
            if let Some(place) = &r.place {
                let unit = |a: [f64; 3]| {
                    let l = a.iter().map(|x| x * x).sum::<f64>().sqrt();
                    (l - 1.).abs() < 1e-3 && a.iter().all(|x| x.is_finite())
                };
                if !finite(&place.origin.0) || !unit(place.x) || !unit(place.y) {
                    return bad("Положение формы задано неверно");
                }
            }
            if let Some(array) = &r.array {
                if array.lines.is_empty() || !(r.diameter..=100_000.).contains(&array.spacing) {
                    return bad("Линейный массив лягушек задан неверно");
                }
                for (a, b) in &array.lines {
                    if !finite(a) || !finite(b) || (a[0] - b[0]).hypot(a[1] - b[1]) < 1. {
                        return bad("Линия массива лягушек слишком короткая");
                    }
                }
            }
        }
        if self.breaks.len() > 10_000 || self.units.len() > 100_000 {
            return bad("Слишком много швов или единиц бетонирования");
        }
        let zone_numbers: BTreeSet<u32> = self.zones.iter().map(|z| z.number).collect();
        for r in &self.rebars {
            if let Some(z) = r.zone
                && !zone_numbers.contains(&z)
            {
                return bad("Набор ссылается на отсутствующую зону армирования");
            }
            if let Some(id) = &r.shape_id
                && !crate::rebar::shape_known(self, id)
            {
                return bad("Набор ссылается на неизвестный тип формы");
            }
        }
        Ok(())
    }

    fn validate_shapes(&self) -> Result<(), ModelError> {
        let bad = |m: &str| Err(ModelError::InvalidValue(m.into()));
        let mut ids = BTreeSet::new();
        if self.shape_types.len() > 10_000 {
            return bad("Слишком много типов форм");
        }
        for s in &self.shape_types {
            if s.id.is_empty() || s.id.len() > 64 || !ids.insert(s.id.clone()) {
                return bad("Идентификатор типа формы пустой или повторяется");
            }
            if s.name.trim().is_empty() || s.name.len() > 128 || s.code.len() > 32 {
                return bad("Имя типа формы не задано");
            }
            check_shape(s)?;
        }
        Ok(())
    }

    fn validate_slab_types(&self) -> Result<(), ModelError> {
        let bad = |m: &str| Err(ModelError::InvalidValue(m.into()));
        let mut ids = BTreeSet::new();
        let mut defaults = 0;
        if self.slab_types.len() > 10_000 {
            return bad("Слишком много типов плит");
        }
        for t in &self.slab_types {
            if t.id.is_empty() || t.id.len() > 64 || !ids.insert(t.id.clone()) {
                return bad("Идентификатор типа плиты пустой или повторяется");
            }
            if t.name.trim().is_empty() || t.name.len() > 128 {
                return bad("Имя типа плиты не задано");
            }
            if t.default_for_new {
                defaults += 1;
            }
            for layer in t.bottom.iter().chain(&t.top) {
                check_layer(layer)?;
            }
            for (d, g, step) in [
                (t.edge_diameter, &t.edge_grade, t.edge_spacing),
                (t.frog_diameter, &t.frog_grade, t.frog_spacing),
            ] {
                if !(3.0..=60.).contains(&d) || !(d..=100_000.).contains(&step) {
                    return bad("Диаметр или шаг обрамления задан неверно");
                }
                if g.trim().is_empty() || g.len() > 32 {
                    return bad("Класс арматуры обрамления не задан");
                }
            }
            if !crate::rebar::shape_known(self, &t.edge_shape)
                || !crate::rebar::shape_known(self, &t.frog_shape)
            {
                return bad("Тип плиты ссылается на неизвестный тип формы");
            }
        }
        if defaults > 1 {
            return bad("Тип плиты по умолчанию может быть только один");
        }
        if self
            .default_cover
            .is_some_and(|c| !(0.0..=500.).contains(&c))
        {
            return bad("Защитный слой новых плит от 0 до 500 мм");
        }
        Ok(())
    }

    fn validate_zones(&self) -> Result<(), ModelError> {
        let bad = |m: &str| Err(ModelError::InvalidValue(m.into()));
        let mut numbers = BTreeSet::new();
        if self.zones.len() > 100_000 {
            return bad("Слишком много зон армирования");
        }
        for z in &self.zones {
            if !numbers.insert(z.number) {
                return bad("Номер зоны армирования повторяется");
            }
            if self.slab_types.iter().all(|t| t.id != z.type_id) {
                return bad("Зона ссылается на неизвестный тип плиты");
            }
            if !z.mark_at.0.iter().all(|v| v.is_finite() && v.abs() <= 1e9) {
                return bad("Марка зоны: неверные координаты");
            }
            if z.mark.len() > 512 {
                return bad("Марка зоны слишком длинная");
            }
            for c in &z.checks {
                if c.message.is_empty() || c.message.len() > 512 {
                    return bad("Замечание проверки армирования пустое");
                }
                if c.from.is_some_and(|v| !(0.0..=1e9).contains(&v))
                    || c.to.is_some_and(|v| !(0.0..=1e9).contains(&v))
                {
                    return bad("Участок без П-стержней задан неверно");
                }
            }
        }
        Ok(())
    }
}

fn check_shape(s: &BarShapeType) -> Result<(), ModelError> {
    let bad = |m: &str| Err(ModelError::InvalidValue(m.into()));
    if s.segments.is_empty() || s.segments.len() > 26 {
        return bad("У типа формы от 1 до 26 участков");
    }
    for seg in &s.segments {
        if seg.letter.len() != 1 || !seg.letter.chars().all(|c| c.is_ascii_uppercase()) {
            return bad("Параметр участка — буква A…Z");
        }
        if !(1.0..=100_000.).contains(&seg.length) {
            return bad("Длина участка формы от 1 до 100 000 мм");
        }
        if !(-180.0..=180.0).contains(&seg.bend_deg) {
            return bad("Угол загиба от −180 до 180°");
        }
    }
    if s.mandrel.is_some_and(|d| !(1.0..=10_000.).contains(&d)) {
        return bad("Диаметр оправки типа формы от 1 до 10 000 мм");
    }
    if s.hooks
        .iter()
        .any(|h| h.length.is_some_and(|l| !(0.0..=10_000.).contains(&l)))
    {
        return bad("Прямой участок крюка от 0 до 10 000 мм");
    }
    Ok(())
}

fn check_layer(layer: &MeshLayer) -> Result<(), ModelError> {
    let bad = |m: &str| Err(ModelError::InvalidValue(m.into()));
    if !(3.0..=60.).contains(&layer.diameter) {
        return bad("Диаметр сетки от 3 до 60 мм");
    }
    if layer.grade.trim().is_empty() || layer.grade.len() > 32 {
        return bad("Класс арматуры сетки не задан");
    }
    if !(layer.diameter..=100_000.).contains(&layer.spacing) {
        return bad("Шаг сетки не меньше диаметра и не больше 100 000 мм");
    }
    if !layer.direction_deg.is_finite() || !(-360.0..=360.0).contains(&layer.direction_deg) {
        return bad("Направление стержней задано неверно");
    }
    if let Some(step) = layer.cross_spacing
        && !(layer.cross_diameter()..=100_000.).contains(&step)
    {
        return bad("Шаг поперечных стержней задан неверно");
    }
    if layer
        .cross_diameter
        .is_some_and(|d| !(3.0..=60.).contains(&d))
    {
        return bad("Диаметр поперечных стержней от 3 до 60 мм");
    }
    Ok(())
}
