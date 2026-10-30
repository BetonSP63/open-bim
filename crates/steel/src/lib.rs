//! Steel detailing authoring and fabrication data, independent of desktop UI.
use open_bim_commands::Command;
use open_bim_model::{Detail, DetailKind, Frame, ModelError, ObjectId, Point3, Project, WeldType};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug)]
pub struct BoltPattern {
    pub parts: BTreeSet<ObjectId>,
    pub frame: Frame,
    pub diameter: f64,
    pub clearance: f64,
    pub hole_depth: f64,
    pub steps_x: Vec<f64>,
    pub steps_y: Vec<f64>,
    pub standard: String,
    pub grade: String,
    /// Circle or list bolt positions in the group frame (empty: the array of steps).
    pub points: Vec<[f64; 2]>,
    /// Slotted holes in every part: extra length along the group X, mm (0 — round).
    pub slot: f64,
}

/// Build one atomic transaction: all associated holes plus the bolt group.
/// ID allocation happens on a candidate project; failures never mutate input.
pub fn bolt_commands(project: &Project, pattern: &BoltPattern) -> Result<Vec<Command>, ModelError> {
    project.validate()?;
    pattern.frame.validate()?;
    if pattern.steps_x.len() > 1000
        || pattern.steps_y.len() > 1000
        || (pattern.steps_x.len() + 1) * (pattern.steps_y.len() + 1) > 10000
        || pattern.points.len() > 1000
        || pattern.parts.len() > 100
    {
        return Err(ModelError::InvalidValue("Bolt pattern too large".into()));
    }
    let positions =
        open_bim_model::bolt_positions(&pattern.steps_x, &pattern.steps_y, &pattern.points);
    let mut candidate = project.clone();
    let mut commands = Vec::new();
    let mut holes = BTreeSet::new();
    for part in &pattern.parts {
        let parent = *candidate
            .elements
            .get(part)
            .ok_or(ModelError::MissingReference(*part))?
            .shape
            .frame();
        for [x, y] in &positions {
            {
                let world = pattern.frame.world(Point3([*x, *y, 0.0]));
                let delta = std::array::from_fn::<_, 3, _>(|i| world.0[i] - parent.origin.0[i]);
                let dot = |a: [f64; 3], b: [f64; 3]| {
                    a.into_iter().zip(b).map(|(x, y)| x * y).sum::<f64>()
                };
                let frame = Frame {
                    origin: Point3(std::array::from_fn(|i| dot(parent.axes[i], delta))),
                    axes: std::array::from_fn(|i| {
                        std::array::from_fn(|j| dot(parent.axes[j], pattern.frame.axes[i]))
                    }),
                };
                let id = candidate.allocate_id()?;
                let detail = Detail {
                    id,
                    kind: DetailKind::Hole {
                        part: *part,
                        frame,
                        diameter: pattern.diameter + pattern.clearance,
                        depth: pattern.hole_depth,
                        slot: pattern.slot,
                    },
                };
                candidate.details.insert(id, detail.clone());
                holes.insert(id);
                commands.push(Command::CreateDetail { detail });
            }
        }
    }
    let id = candidate.allocate_id()?;
    let detail = Detail {
        id,
        kind: DetailKind::BoltGroup {
            parts: pattern.parts.clone(),
            holes,
            frame: pattern.frame,
            diameter: pattern.diameter,
            clearance: pattern.clearance,
            steps_x: pattern.steps_x.clone(),
            steps_y: pattern.steps_y.clone(),
            standard: pattern.standard.clone(),
            grade: pattern.grade.clone(),
            points: pattern.points.clone(),
        },
    };
    candidate.details.insert(id, detail.clone());
    candidate.validate()?;
    commands.push(Command::CreateDetail { detail });
    Ok(commands)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FabricationRow {
    Bolts {
        id: ObjectId,
        standard: String,
        grade: String,
        diameter_mm: f64,
        count: usize,
        parts: BTreeSet<ObjectId>,
    },
    Weld {
        id: ObjectId,
        weld_type: WeldType,
        size_mm: f64,
        length_mm: f64,
        shop: bool,
        parts: BTreeSet<ObjectId>,
    },
}

pub fn fabrication_rows(project: &Project) -> Result<Vec<FabricationRow>, ModelError> {
    project.validate()?;
    Ok(project
        .details
        .values()
        .filter_map(|detail| match &detail.kind {
            DetailKind::BoltGroup {
                parts,
                diameter,
                steps_x,
                steps_y,
                standard,
                grade,
                points,
                ..
            } => Some(FabricationRow::Bolts {
                id: detail.id,
                standard: standard.clone(),
                grade: grade.clone(),
                diameter_mm: *diameter,
                count: open_bim_model::bolt_positions(steps_x, steps_y, points).len(),
                parts: parts.clone(),
            }),
            DetailKind::Weld {
                parts,
                weld_type,
                size,
                path,
                shop,
                ..
            } => Some(FabricationRow::Weld {
                id: detail.id,
                weld_type: *weld_type,
                size_mm: *size,
                length_mm: path.windows(2).map(|p| p[0].distance(p[1])).sum(),
                shop: *shop,
                parts: parts.clone(),
            }),
            _ => None,
        })
        .collect())
}
