//! Concrete data that is not a part: pour breaks, the calculated pour
//! units («Бетонирование»), reinforcing bar sets («Армирование») and the
//! formwork panels and props («Опалубка»).
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::rebar::{Hook, RebarRule};
use crate::{ModelError, ObjectId, Point3};

/// «Шов бетонирования»: a vertical plane through the line `start`–`end`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PourBreak {
    pub start: Point3,
    pub end: Point3,
}

impl PourBreak {
    /// Signed side of a point: >0 left of start→end seen from above.
    pub fn side(&self, p: [f64; 3]) -> f64 {
        let (a, b) = (self.start.0, self.end.0);
        (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
    }
}

/// A calculated pour unit: cast-in-place parts of one grade placed at once.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PourUnit {
    pub number: u32,
    pub grade: String,
    pub parts: BTreeSet<ObjectId>,
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
}

fn no_hooks(h: &[Hook; 2]) -> bool {
    h.iter().all(Hook::is_none)
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
            if (e[0] - s[0]).hypot(e[1] - s[1]) < 1. {
                return bad("Шов бетонирования: точки совпадают в плане");
            }
        }
        let mut seen = BTreeSet::new();
        for u in &self.units {
            if u.parts.iter().any(|p| !seen.insert(*p)) {
                return bad("Деталь входит в две единицы бетонирования");
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
        if self.breaks.len() > 10_000 || self.units.len() > 100_000 {
            return bad("Слишком много швов или единиц бетонирования");
        }
        Ok(())
    }
}
