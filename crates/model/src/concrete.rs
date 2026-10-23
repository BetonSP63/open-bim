//! Concrete data that is not a part: pour breaks, the calculated pour
//! units («Бетонирование») and reinforcing bar sets («Армирование»).
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

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

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Concrete {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub breaks: Vec<PourBreak>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub units: Vec<PourUnit>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rebars: Vec<RebarSet>,
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
        if bars > 1_000_000 {
            return bad("Слишком много стержней");
        }
        if self.breaks.len() > 10_000 || self.units.len() > 100_000 {
            return bad("Слишком много швов или единиц бетонирования");
        }
        Ok(())
    }
}
