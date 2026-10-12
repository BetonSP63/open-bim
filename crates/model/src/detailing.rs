use crate::{Frame, ModelError, ObjectId, Point3, Project};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeldType {
    Fillet,
    Butt,
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
    Hole {
        part: ObjectId,
        frame: Frame,
        diameter: f64,
        depth: f64,
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
    },
    Weld {
        parts: BTreeSet<ObjectId>,
        weld_type: WeldType,
        size: f64,
        path: Vec<Point3>,
        shop: bool,
    },
}

impl DetailKind {
    pub fn part(&self) -> Option<ObjectId> {
        match self {
            Self::PlaneCut { part, .. } | Self::BoxCut { part, .. } | Self::Hole { part, .. } => {
                Some(*part)
            }
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
            Self::Hole {
                part: id,
                frame,
                diameter,
                depth,
            } => {
                part(*id)?;
                frame.validate()?;
                if !positive(*diameter) || !positive(*depth) {
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
            } => {
                frame.validate()?;
                if parts.is_empty()
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
                let count = (steps_x.len() + 1) * (steps_y.len() + 1);
                if holes.len() != count * parts.len() {
                    return Err(invalid());
                }
                let mut per_part = std::collections::BTreeMap::<ObjectId, usize>::new();
                let coordinates = |steps: &[f64]| {
                    let mut values = vec![0.0];
                    for step in steps {
                        values.push(values.last().unwrap() + step);
                    }
                    values
                };
                let xs = coordinates(steps_x);
                let ys = coordinates(steps_y);
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
                    let ix = xs
                        .iter()
                        .position(|x| (*x - dot(delta, frame.axes[0])).abs() < 1e-6)
                        .ok_or_else(invalid)?;
                    let iy = ys
                        .iter()
                        .position(|y| (*y - dot(delta, frame.axes[1])).abs() < 1e-6)
                        .ok_or_else(invalid)?;
                    if !occupied.insert((*part, ix, iy)) {
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
                if parts.len() < 2 || !positive(*size) || path.len() < 2 || path.len() > 10000 {
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
