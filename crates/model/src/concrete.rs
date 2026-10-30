//! Concrete data that is not a part: pour breaks, the calculated pour
//! units («Бетонирование»), reinforcing bar sets («Армирование») and the
//! formwork panels and props («Опалубка»).
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

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
        }
    }
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
    pub bars: Vec<Vec<Point3>>,
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

    pub fn total_length_mm(&self) -> f64 {
        self.bars.iter().map(|b| Self::bar_length(b)).sum()
    }

    pub fn mass_kg(&self) -> f64 {
        let area = std::f64::consts::PI * self.diameter * self.diameter / 4.;
        area * self.total_length_mm() * 1e-9 * REBAR_DENSITY
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
        if self.breaks.len() > 10_000 || self.units.len() > 100_000 {
            return bad("Слишком много швов или единиц бетонирования");
        }
        Ok(())
    }
}
