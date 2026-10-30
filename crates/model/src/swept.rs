use crate::{ElementShape, ModelError, ObjectId, Project};
use serde::{Deserialize, Serialize};

/// Additional geometry of a part. Coordinates are in the part's local XY plane.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SweptPart {
    CurvedBeam {
        center: [f64; 2],
        radius: f64,
        start_angle: f64,
        sweep_angle: f64,
    },
    PolyBeam {
        points: Vec<[f64; 2]>,
        bend_radius: f64,
    },
    /// Straight lengths end at the tangent points; radius is the inside radius.
    BentPlate {
        width: f64,
        first_length: f64,
        second_length: f64,
        radius: f64,
        angle: f64,
    },
}

fn invalid() -> ModelError {
    ModelError::InvalidValue("Недопустимые параметры гнутой детали".into())
}
fn positive(x: f64) -> bool {
    x.is_finite() && x > 0.001 && x <= 1e7
}
fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    (b[0] - a[0]).hypot(b[1] - a[1])
}

impl SweptPart {
    pub(crate) fn validate(&self, shape: &ElementShape) -> Result<(), ModelError> {
        match (self, shape) {
            (
                Self::CurvedBeam {
                    center,
                    radius,
                    start_angle,
                    sweep_angle,
                },
                ElementShape::Member { .. },
            ) if center.iter().all(|x| x.is_finite() && x.abs() <= 1e7)
                && positive(*radius)
                && start_angle.is_finite()
                && sweep_angle.is_finite()
                && sweep_angle.abs() > 1e-6
                && sweep_angle.abs() < std::f64::consts::TAU =>
            {
                Ok(())
            }
            (
                Self::PolyBeam {
                    points,
                    bend_radius,
                },
                ElementShape::Member { .. },
            ) => rounded_path(points, *bend_radius).map(|_| ()),
            (
                Self::BentPlate {
                    width,
                    first_length,
                    second_length,
                    radius,
                    angle,
                },
                ElementShape::Plate { thickness, .. },
            ) if positive(*width)
                && positive(*first_length)
                && positive(*second_length)
                && positive(*radius)
                && angle.is_finite()
                && angle.abs() > 1e-6
                && angle.abs() < std::f64::consts::PI =>
            {
                bent_plate_contour(
                    *first_length,
                    *second_length,
                    *radius,
                    *angle,
                    thickness.value(),
                )
                .map(|_| ())
            }
            _ => Err(invalid()),
        }
    }
}

/// Rounded corners are represented by straight chords. The sagitta of each chord
/// is at most 0.01 mm; developed length is computed from the true arcs.
pub(crate) fn rounded_path(points: &[[f64; 2]], radius: f64) -> Result<Vec<[f64; 2]>, ModelError> {
    if points.len() < 2
        || points.len() > 1000
        || !radius.is_finite()
        || !(0.0..=1e7).contains(&radius)
        || !points
            .iter()
            .flatten()
            .all(|v| v.is_finite() && v.abs() <= 1e7)
    {
        return Err(invalid());
    }
    let mut tangencies = vec![0.; points.len()];
    let mut turns = vec![0.; points.len()];
    for i in 1..points.len() - 1 {
        let a = points[i - 1];
        let b = points[i];
        let c = points[i + 1];
        let l1 = distance(a, b);
        let l2 = distance(b, c);
        if l1 <= 0.001 || l2 <= 0.001 {
            return Err(invalid());
        }
        let u = [(b[0] - a[0]) / l1, (b[1] - a[1]) / l1];
        let v = [(c[0] - b[0]) / l2, (c[1] - b[1]) / l2];
        let turn = (u[0] * v[1] - u[1] * v[0]).atan2(u[0] * v[0] + u[1] * v[1]);
        if turn.abs() >= std::f64::consts::PI - 1e-6 {
            return Err(invalid());
        }
        turns[i] = turn;
        tangencies[i] = radius * (turn.abs() / 2.).tan();
    }
    for i in 0..points.len() - 1 {
        if distance(points[i], points[i + 1]) <= tangencies[i] + tangencies[i + 1] + 0.001 {
            return Err(invalid());
        }
    }
    let mut result = vec![points[0]];
    for i in 1..points.len() - 1 {
        let turn = turns[i];
        let t = tangencies[i];
        if t <= 0.001 {
            result.push(points[i]);
            continue;
        }
        let l = distance(points[i - 1], points[i]);
        let u = [
            (points[i][0] - points[i - 1][0]) / l,
            (points[i][1] - points[i - 1][1]) / l,
        ];
        let start = [points[i][0] - u[0] * t, points[i][1] - u[1] * t];
        let sign = turn.signum();
        let center = [
            start[0] - u[1] * radius * sign,
            start[1] + u[0] * radius * sign,
        ];
        let a0 = (start[1] - center[1]).atan2(start[0] - center[0]);
        let step = (2. * (1. - (0.01 / radius).min(1.)).acos()).min(std::f64::consts::PI / 18.);
        let count = (turn.abs() / step).ceil().max(1.) as usize;
        if result.len() + count + 2 > 1000 {
            return Err(invalid());
        }
        result.push(start);
        for j in 1..=count {
            let a = a0 + turn * j as f64 / count as f64;
            result.push([center[0] + radius * a.cos(), center[1] + radius * a.sin()]);
        }
    }
    result.push(*points.last().ok_or_else(invalid)?);
    Ok(result)
}

impl Project {
    /// Developed length along the reference line of a beam or the neutral
    /// surface of a bent plate (the neutral axis is at half the thickness).
    pub fn part_developed_length(&self, id: ObjectId) -> Result<f64, ModelError> {
        let part = self
            .elements
            .get(&id)
            .ok_or(ModelError::MissingReference(id))?;
        let swept = self.swept_parts.get(&id).ok_or_else(invalid)?;
        swept.validate(&part.shape)?;
        match swept {
            SweptPart::CurvedBeam {
                radius,
                sweep_angle,
                ..
            } => Ok(radius * sweep_angle.abs()),
            SweptPart::PolyBeam {
                points,
                bend_radius,
            } => {
                rounded_path(points, *bend_radius)?;
                let mut length: f64 = points.windows(2).map(|p| distance(p[0], p[1])).sum();
                for p in points.windows(3) {
                    let a = [p[1][0] - p[0][0], p[1][1] - p[0][1]];
                    let b = [p[2][0] - p[1][0], p[2][1] - p[1][1]];
                    let turn = (a[0] * b[1] - a[1] * b[0])
                        .atan2(a[0] * b[0] + a[1] * b[1])
                        .abs();
                    length += bend_radius * (turn - 2. * (turn / 2.).tan());
                }
                Ok(length)
            }
            SweptPart::BentPlate {
                first_length,
                second_length,
                radius,
                angle,
                ..
            } => {
                let ElementShape::Plate { thickness, .. } = &part.shape else {
                    return Err(invalid());
                };
                Ok(first_length + second_length + (radius + thickness.value() / 2.) * angle.abs())
            }
        }
    }
}

/// Cross-section of the bent sheet, approximating the two circular edges with
/// at most 0.01 mm chord sagitta. The extrusion axis is the bend line.
pub(crate) fn bent_plate_contour(
    first: f64,
    second: f64,
    radius: f64,
    angle: f64,
    thickness: f64,
) -> Result<Vec<[f64; 2]>, ModelError> {
    if !positive(first)
        || !positive(second)
        || !positive(radius)
        || !positive(thickness)
        || !angle.is_finite()
        || angle.abs() <= 1e-6
        || angle.abs() >= std::f64::consts::PI
    {
        return Err(invalid());
    }
    let a = angle.abs();
    let outer = radius + thickness;
    let step = (2. * (1. - (0.01 / outer).min(1.)).acos()).min(std::f64::consts::PI / 36.);
    let count = (a / step).ceil().max(2.) as usize;
    if count > 4000 {
        return Err(invalid());
    }
    let arc = |r: f64, t: f64| [r * t.sin(), radius - r * t.cos()];
    let tangent = [a.cos(), a.sin()];
    let mut contour = vec![[-first, 0.]];
    for i in 0..=count {
        contour.push(arc(radius, a * i as f64 / count as f64));
    }
    let end = arc(radius, a);
    contour.push([end[0] + second * tangent[0], end[1] + second * tangent[1]]);
    let end = arc(outer, a);
    contour.push([end[0] + second * tangent[0], end[1] + second * tangent[1]]);
    for i in (0..=count).rev() {
        contour.push(arc(outer, a * i as f64 / count as f64));
    }
    contour.push([-first, -thickness]);
    if angle < 0. {
        for point in &mut contour {
            point[1] = -point[1];
        }
    }
    crate::polygon_area(&contour)?;
    Ok(contour)
}

/// A catalogue area without a catalogue centroid cannot determine mass on a curved axis.
pub(crate) fn validate_swept_profile(profile: &crate::Profile) -> Result<(), ModelError> {
    if profile.area_mm2.is_some()
        && matches!(
            profile.shape,
            crate::ProfileShape::Angle { .. } | crate::ProfileShape::Channel { .. }
        )
    {
        return Err(ModelError::InvalidValue("Для гнутого несимметричного профиля с площадью сортамента требуется положение центра тяжести".into()));
    }
    Ok(())
}
