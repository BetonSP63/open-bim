//! Construction objects of the model («Вспомогательные линии и
//! окружности») and model dimensions. They help to place parts, are snap
//! targets and are never shown on drawings.
use crate::{ElementShape, ModelError, ObjectId, Point3, Project};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ConstructionKind {
    Line {
        a: Point3,
        b: Point3,
    },
    Circle {
        center: Point3,
        normal: [f64; 3],
        radius: f64,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Construction {
    pub id: ObjectId,
    pub kind: ConstructionKind,
}

/// One end of a model dimension: a fixed point, or the start/end of a member
/// so the dimension follows the part when it changes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "at", rename_all = "snake_case", deny_unknown_fields)]
pub enum DimPoint {
    Fixed { point: Point3 },
    PartEnd { part: ObjectId, end: bool },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelDimension {
    pub id: ObjectId,
    pub a: DimPoint,
    pub b: DimPoint,
}

fn finite(p: &Point3) -> bool {
    p.0.iter().all(|v| v.is_finite() && v.abs() <= 1e9)
}

impl Construction {
    /// Line segments that draw the object (a circle as 48 chords).
    pub fn segments(&self) -> Vec<([f64; 3], [f64; 3])> {
        match &self.kind {
            ConstructionKind::Line { a, b } => vec![(a.0, b.0)],
            ConstructionKind::Circle {
                center,
                normal,
                radius,
            } => {
                let n = *normal;
                // Any unit vector across the normal, then the second one.
                let helper = if n[0].abs() < 0.9 {
                    [1., 0., 0.]
                } else {
                    [0., 1., 0.]
                };
                let cross = |a: [f64; 3], b: [f64; 3]| {
                    [
                        a[1] * b[2] - a[2] * b[1],
                        a[2] * b[0] - a[0] * b[2],
                        a[0] * b[1] - a[1] * b[0],
                    ]
                };
                let norm = |a: [f64; 3]| {
                    let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt().max(1e-12);
                    a.map(|v| v / l)
                };
                let u = norm(cross(n, helper));
                let w = cross(n, u);
                let at = |k: usize| {
                    let t = k as f64 / 48. * std::f64::consts::TAU;
                    std::array::from_fn(|i| {
                        center.0[i] + radius * (u[i] * t.cos() + w[i] * t.sin())
                    })
                };
                (0..48).map(|k| (at(k), at(k + 1))).collect()
            }
        }
    }

    pub fn validate(&self) -> Result<(), ModelError> {
        let invalid = |m: &str| {
            Err(ModelError::InvalidValue(format!(
                "Вспомогательный объект: {m}"
            )))
        };
        match &self.kind {
            ConstructionKind::Line { a, b } => {
                if !finite(a) || !finite(b) {
                    return invalid("точка вне диапазона");
                }
                let d: f64 = (0..3)
                    .map(|i| (a.0[i] - b.0[i]).powi(2))
                    .sum::<f64>()
                    .sqrt();
                if d < 1.0 {
                    return invalid("линия нулевой длины");
                }
            }
            ConstructionKind::Circle {
                center,
                normal,
                radius,
            } => {
                let n: f64 = normal.iter().map(|v| v * v).sum::<f64>().sqrt();
                if !finite(center) || (n - 1.0).abs() > 1e-6 {
                    return invalid("центр или нормаль");
                }
                if !(radius.is_finite() && (1.0..=1e7).contains(radius)) {
                    return invalid("радиус 1…10 000 000 мм");
                }
            }
        }
        Ok(())
    }
}

impl Project {
    /// World position of a dimension end, `None` when its part is gone.
    pub fn dim_point(&self, p: &DimPoint) -> Option<Point3> {
        match p {
            DimPoint::Fixed { point } => Some(*point),
            DimPoint::PartEnd { part, end } => match &self.elements.get(part)?.shape {
                ElementShape::Member { length, frame, .. } => {
                    Some(frame.world(Point3([0., 0., if *end { length.value() } else { 0. }])))
                }
                ElementShape::Plate { .. } => None,
            },
        }
    }

    /// Measured length of a model dimension (mm).
    pub fn dimension_value(&self, d: &ModelDimension) -> Option<f64> {
        let (a, b) = (self.dim_point(&d.a)?, self.dim_point(&d.b)?);
        Some(
            (0..3)
                .map(|i| (a.0[i] - b.0[i]).powi(2))
                .sum::<f64>()
                .sqrt(),
        )
    }
}

impl ModelDimension {
    pub fn validate(&self, project: &Project) -> Result<(), ModelError> {
        for p in [&self.a, &self.b] {
            match p {
                DimPoint::Fixed { point } if !finite(point) => {
                    return Err(ModelError::InvalidValue(
                        "Размер: точка вне диапазона".into(),
                    ));
                }
                DimPoint::PartEnd { part, .. } if !project.elements.contains_key(part) => {
                    return Err(ModelError::MissingReference(*part));
                }
                _ => {}
            }
        }
        Ok(())
    }
}
