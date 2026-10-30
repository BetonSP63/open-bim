use crate::{Frame, ModelError, ObjectId, Point3, Project};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::weld::{WeldGost, weld_mark, weld_notes, welded_length};
pub use crate::weld::{WeldProcess, WeldSides, WeldType};

/// Nut and washer standards of a bolt set: high-strength bolts of ГОСТ Р
/// 52644 come with ГОСТ Р 52645 nuts and ГОСТ Р 52646 washers, other bolts
/// with ГОСТ 5915-70 nuts and ГОСТ 11371-78 washers.
pub fn bolt_set(standard: &str) -> (&'static str, &'static str) {
    if standard.contains("52644") {
        ("ГОСТ Р 52645-2006", "ГОСТ Р 52646-2006")
    } else {
        ("ГОСТ 5915-70", "ГОСТ 11371-78")
    }
}

fn is_manual(p: &WeldProcess) -> bool {
    *p == WeldProcess::Manual
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

fn is_one_side(s: &WeldSides) -> bool {
    *s == WeldSides::One
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Detail {
    pub id: ObjectId,
    pub kind: DetailKind,
}

/// Feature coordinates are local to the part; bolt frames and weld paths are
/// world coordinates. Hole references are stable IDs, never vector offsets.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DetailKind {
    PlaneCut {
        part: ObjectId,
        normal: [f64; 3],
        offset: f64,
    },
    BoxCut {
        part: ObjectId,
        origin: Point3,
        size: [f64; 3],
    },
    /// Prism cut in the part's local frame: `contour` in the XY plane of
    /// `frame`, extruded `length` along its Z (cut by polygon, cut by part).
    PrismCut {
        part: ObjectId,
        frame: crate::Frame,
        contour: Vec<[f64; 2]>,
        length: f64,
    },
    Hole {
        part: ObjectId,
        frame: Frame,
        diameter: f64,
        depth: f64,
        /// Slotted hole: extra length along the frame X, centred on the
        /// origin (centre distance of the two half circles), mm; 0 — round.
        #[serde(default, skip_serializing_if = "is_zero")]
        slot: f64,
    },
    BoltGroup {
        parts: BTreeSet<ObjectId>,
        holes: BTreeSet<ObjectId>,
        frame: Frame,
        diameter: f64,
        clearance: f64,
        steps_x: Vec<f64>,
        steps_y: Vec<f64>,
        standard: String,
        grade: String,
        /// Bolt positions in the group frame for the «Окружность» and
        /// «Список» shapes; empty for «Массив» (`steps_x`/`steps_y`).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        points: Vec<[f64; 2]>,
    },
    /// `parts` has one part for a weld on a part («шов на детали»).
    Weld {
        parts: BTreeSet<ObjectId>,
        weld_type: WeldType,
        size: f64,
        path: Vec<Point3>,
        shop: bool,
        #[serde(default, skip_serializing_if = "is_one_side")]
        sides: WeldSides,
        /// Welded all around the joint («по контуру»).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        around: bool,
        #[serde(default, skip_serializing_if = "is_manual")]
        process: WeldProcess,
        /// Joint, method, intermittent weld and side by ГОСТ 2.312.
        #[serde(default, skip_serializing_if = "WeldGost::is_default")]
        gost: WeldGost,
    },
}

/// Bolt positions of a group in its frame: the array of `steps_x` ×
/// `steps_y` from the origin, or the explicit `points` when given.
pub fn bolt_positions(steps_x: &[f64], steps_y: &[f64], points: &[[f64; 2]]) -> Vec<[f64; 2]> {
    if !points.is_empty() {
        return points.to_vec();
    }
    let prefix = |steps: &[f64]| {
        let mut out = vec![0.0];
        for step in steps {
            out.push(out.last().unwrap() + step);
        }
        out
    };
    let ys = prefix(steps_y);
    prefix(steps_x)
        .into_iter()
        .flat_map(|x| ys.iter().map(move |y| [x, *y]))
        .collect()
}

/// Bolts on a circle of `diameter` around the group origin, the first on its X axis.
pub fn circle_points(count: usize, diameter: f64) -> Vec<[f64; 2]> {
    (0..count)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / count as f64;
            let p = [diameter / 2.0 * a.cos(), diameter / 2.0 * a.sin()];
            // Exact zeros keep saved files and hashes clean.
            p.map(|v| if v.abs() < 1e-9 { 0.0 } else { v })
        })
        .collect()
}

impl DetailKind {
    /// Shelf designation of a weld by ГОСТ 2.312 (`weld_mark`); `None` for
    /// other details.
    pub fn weld_mark(&self) -> Option<String> {
        match self {
            Self::Weld {
                weld_type,
                size,
                sides,
                process,
                gost,
                ..
            } => Some(weld_mark(*process, *weld_type, *sides, *size, gost)),
            _ => None,
        }
    }
    /// Weld designation for lists: the shelf text with the words of the
    /// leader signs («по контуру», «(монт.)»).
    pub fn weld_designation(&self) -> Option<String> {
        match self {
            Self::Weld { around, shop, .. } => {
                Some(self.weld_mark()? + &weld_notes(*around, *shop))
            }
            _ => None,
        }
    }
    /// Path length and welded length (intermittent welds) of a weld, mm.
    pub fn weld_lengths(&self) -> Option<(f64, f64)> {
        match self {
            Self::Weld { path, gost, .. } => {
                let length: f64 = path.windows(2).map(|w| w[0].distance(w[1])).sum();
                Some((length, welded_length(length, gost)))
            }
            _ => None,
        }
    }
    /// Bolt positions of a bolt group in its frame; empty for other details.
    pub fn bolt_positions(&self) -> Vec<[f64; 2]> {
        match self {
            Self::BoltGroup {
                steps_x,
                steps_y,
                points,
                ..
            } => bolt_positions(steps_x, steps_y, points),
            _ => Vec::new(),
        }
    }
    pub fn part(&self) -> Option<ObjectId> {
        match self {
            Self::PlaneCut { part, .. }
            | Self::BoxCut { part, .. }
            | Self::PrismCut { part, .. }
            | Self::Hole { part, .. } => Some(*part),
            _ => None,
        }
    }
    pub fn references(&self) -> BTreeSet<ObjectId> {
        match self {
            Self::BoltGroup { parts, holes, .. } => parts.union(holes).copied().collect(),
            Self::Weld { parts, .. } => parts.clone(),
            _ => self.part().into_iter().collect(),
        }
    }
    pub fn validate(&self, project: &Project) -> Result<(), ModelError> {
        let invalid = || ModelError::InvalidValue("Invalid steel detailing parameters".into());
        let positive = |v: f64| v.is_finite() && v > 0.001 && v <= 1e7;
        let part = |id: ObjectId| {
            project
                .elements
                .contains_key(&id)
                .then_some(())
                .ok_or(ModelError::MissingReference(id))
        };
        match self {
            Self::PlaneCut {
                part: id,
                normal,
                offset,
            } => {
                part(*id)?;
                if !normal.iter().all(|v| v.is_finite())
                    || normal.iter().map(|v| v * v).sum::<f64>() < 1e-16
                    || !offset.is_finite()
                {
                    return Err(invalid());
                }
            }
            Self::BoxCut {
                part: id,
                origin,
                size,
            } => {
                part(*id)?;
                crate::validation::valid_point(*origin)?;
                if !size.iter().all(|v| positive(*v)) {
                    return Err(invalid());
                }
            }
            Self::PrismCut {
                part: id,
                frame,
                contour,
                length,
            } => {
                part(*id)?;
                frame.validate()?;
                if contour.len() < 3
                    || contour.len() > 10_000
                    || !contour
                        .iter()
                        .flatten()
                        .all(|v| v.is_finite() && v.abs() <= 1e7)
                    || !positive(*length)
                    || crate::polygon_area(contour).is_err()
                {
                    return Err(invalid());
                }
            }
            Self::Hole {
                part: id,
                frame,
                diameter,
                depth,
                slot,
            } => {
                part(*id)?;
                frame.validate()?;
                if !positive(*diameter)
                    || !positive(*depth)
                    || !slot.is_finite()
                    || !(0.0..=1e5).contains(slot)
                {
                    return Err(invalid());
                }
            }
            Self::BoltGroup {
                parts,
                holes,
                frame,
                diameter,
                clearance,
                steps_x,
                steps_y,
                standard,
                grade,
                points,
            } => {
                frame.validate()?;
                let pitch = *diameter + clearance;
                let points_ok = points.is_empty()
                    || (steps_x.is_empty()
                        && steps_y.is_empty()
                        && points.len() <= 1000
                        && points
                            .iter()
                            .all(|p| p.iter().all(|v| v.is_finite() && v.abs() <= 1e7))
                        && points.iter().enumerate().all(|(i, a)| {
                            points[i + 1..]
                                .iter()
                                .all(|b| (a[0] - b[0]).hypot(a[1] - b[1]) > pitch)
                        }));
                if !points_ok
                    || parts.is_empty()
                    || !positive(*diameter)
                    || !clearance.is_finite()
                    || *clearance < 0.0
                    || !positive(*diameter + clearance)
                    || standard.trim().is_empty()
                    || grade.trim().is_empty()
                    || standard.len() > 1024
                    || grade.len() > 1024
                    || standard.contains('\0')
                    || grade.contains('\0')
                    || steps_x.len() > 1000
                    || steps_y.len() > 1000
                    || !steps_x
                        .iter()
                        .chain(steps_y)
                        .all(|v| positive(*v) && *v > *diameter + clearance)
                    || (steps_x.len() + 1) * (steps_y.len() + 1) > 10000
                {
                    return Err(invalid());
                }
                for id in parts {
                    part(*id)?;
                }
                let positions = bolt_positions(steps_x, steps_y, points);
                let count = positions.len();
                if holes.len() != count * parts.len() {
                    return Err(invalid());
                }
                let mut per_part = std::collections::BTreeMap::<ObjectId, usize>::new();
                let mut occupied = std::collections::BTreeSet::new();
                for id in holes {
                    let detail = project
                        .details
                        .get(id)
                        .ok_or(ModelError::MissingReference(*id))?;
                    let DetailKind::Hole {
                        part,
                        diameter: hole_diameter,
                        frame: hole_frame,
                        ..
                    } = &detail.kind
                    else {
                        return Err(invalid());
                    };
                    if !parts.contains(part) || (*hole_diameter - diameter - clearance).abs() > 1e-8
                    {
                        return Err(invalid());
                    }
                    let parent = project.elements[part].shape.frame();
                    let world = parent.world(hole_frame.origin);
                    let delta: [f64; 3] = std::array::from_fn(|i| world.0[i] - frame.origin.0[i]);
                    let dot = |a: [f64; 3], b: [f64; 3]| (0..3).map(|i| a[i] * b[i]).sum::<f64>();
                    let world_axis: [f64; 3] = std::array::from_fn(|i| {
                        (0..3)
                            .map(|j| parent.axes[j][i] * hole_frame.axes[2][j])
                            .sum()
                    });
                    if dot(world_axis, frame.axes[2]).abs() < 1.0 - 1e-9 {
                        return Err(invalid());
                    }
                    let (x, y) = (dot(delta, frame.axes[0]), dot(delta, frame.axes[1]));
                    let index = positions
                        .iter()
                        .position(|p| (p[0] - x).abs() < 1e-6 && (p[1] - y).abs() < 1e-6)
                        .ok_or_else(invalid)?;
                    if !occupied.insert((*part, index)) {
                        return Err(invalid());
                    }
                    *per_part.entry(*part).or_default() += 1;
                }
                if parts.iter().any(|p| per_part.get(p) != Some(&count)) {
                    return Err(invalid());
                }
            }
            Self::Weld {
                parts, size, path, ..
            } => {
                if parts.is_empty() || !positive(*size) || path.len() < 2 || path.len() > 10000 {
                    return Err(invalid());
                }
                for id in parts {
                    part(*id)?;
                }
                for point in path {
                    crate::validation::valid_point(*point)?;
                }
                if path.windows(2).any(|p| p[0].distance(p[1]) <= 0.001) {
                    return Err(invalid());
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_welds_keep_their_saved_form() {
        let json = r#"{"kind":"weld","parts":[1,2],"weld_type":"fillet","size":6.0,"path":[[0.0,0.0,0.0],[1.0,0.0,0.0]],"shop":true}"#;
        let kind: DetailKind = serde_json::from_str(json).unwrap();
        assert!(matches!(
            kind,
            DetailKind::Weld {
                process: WeldProcess::Manual,
                ..
            }
        ));
        assert_eq!(serde_json::to_string(&kind).unwrap(), json);
    }
}
