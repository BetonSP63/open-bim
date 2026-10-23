//! Concrete data that is not a part: pour breaks and the calculated pour
//! units («Бетонирование»).
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

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Concrete {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub breaks: Vec<PourBreak>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub units: Vec<PourUnit>,
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
        if self.breaks.len() > 10_000 || self.units.len() > 100_000 {
            return bad("Слишком много швов или единиц бетонирования");
        }
        Ok(())
    }
}
