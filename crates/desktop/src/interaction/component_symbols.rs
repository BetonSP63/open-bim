//! Component symbols in the model: one mark per component at the joint,
//! coloured by its status — created (green), created with remarks by ГОСТ and
//! СП (yellow), not created after its parts changed (red).
use open_bim_model::{ConnectionInstance, ElementShape, ObjectId, Project};
use open_bim_renderer::DVec3;
use open_bim_renderer::overlay::Projector;
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Created,
    /// Created, but the template breaks the norms (texts of the remarks).
    Remarks(Vec<String>),
    /// The parts no longer allow the component; it has no objects.
    Failed,
}

impl Status {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Created => "создан",
            Self::Remarks(_) => "создан, есть замечания",
            Self::Failed => "не создан",
        }
    }
}

pub fn status(c: &ConnectionInstance) -> Status {
    if c.children.is_empty() {
        return Status::Failed;
    }
    let remarks: Vec<_> = open_bim_connections::norms::advice(&c.template)
        .into_iter()
        .map(|a| a.text)
        .collect();
    if remarks.is_empty() {
        Status::Created
    } else {
        Status::Remarks(remarks)
    }
}

fn line(project: &Project, id: ObjectId) -> Option<(DVec3, DVec3)> {
    crate::interaction::snap::member_line(project, &project.elements.get(&id)?.shape)
}

/// Closest points of two segments, midway between them.
fn joint(a: (DVec3, DVec3), b: (DVec3, DVec3)) -> DVec3 {
    let (d1, d2, r) = (a.1 - a.0, b.1 - b.0, a.0 - b.0);
    let (aa, ee, f) = (d1.dot(d1), d2.dot(d2), d2.dot(r));
    let (s, t) = if aa < 1e-9 && ee < 1e-9 {
        (0., 0.)
    } else if aa < 1e-9 {
        (0., (f / ee).clamp(0., 1.))
    } else {
        let c = d1.dot(r);
        if ee < 1e-9 {
            ((-c / aa).clamp(0., 1.), 0.)
        } else {
            let bb = d1.dot(d2);
            let denom = aa * ee - bb * bb;
            let mut s = if denom > 1e-9 {
                ((bb * f - c * ee) / denom).clamp(0., 1.)
            } else {
                // Parallel (a splice): the end of the first nearest the second.
                let mid = (b.0 + b.1) / 2.;
                if a.0.distance(mid) < a.1.distance(mid) {
                    0.
                } else {
                    1.
                }
            };
            let mut t = (bb * s + f) / ee;
            if t < 0. {
                t = 0.;
                s = (-c / aa).clamp(0., 1.);
            } else if t > 1. {
                t = 1.;
                s = ((bb - c) / aa).clamp(0., 1.);
            }
            (s, t)
        }
    };
    (a.0 + d1 * s + b.0 + d2 * t) / 2.
}

/// Where the symbol stands: at the joint of the two parts, or at the end of
/// the main part for a single-part component (base plate).
pub fn anchor(project: &Project, c: &ConnectionInstance) -> Option<DVec3> {
    let main = line(project, c.primary);
    match (main, c.secondary.and_then(|s| line(project, s))) {
        (Some(a), Some(b)) => Some(joint(a, b)),
        (Some((start, end)), None) => Some(if c.at_end { end } else { start }),
        _ => {
            // A plate: its origin.
            let e = project.elements.get(&c.primary)?;
            match &e.shape {
                ElementShape::Plate { frame, .. } | ElementShape::Member { frame, .. } => {
                    Some(DVec3::from_array(frame.origin.0))
                }
            }
        }
    }
}

/// One symbol on the screen.
#[derive(Clone, Debug, PartialEq)]
pub struct Symbol {
    pub id: ObjectId,
    pub at: [f64; 2],
    pub status: Status,
    /// Connection code, or the component name when no code is set.
    pub label: String,
    pub selected: bool,
}

/// Symbols of the components whose main part is visible.
pub fn symbols(
    project: &Project,
    p: &Projector,
    hidden: &BTreeSet<ObjectId>,
    selected: &BTreeSet<ObjectId>,
) -> Vec<Symbol> {
    project
        .connections
        .values()
        .filter(|c| !hidden.contains(&c.primary))
        .filter_map(|c| {
            let at = p.screen(anchor(project, c)?)?;
            Some(Symbol {
                id: c.id,
                at,
                status: status(c),
                label: if c.general.code.is_empty() {
                    c.template.name.clone()
                } else {
                    c.general.code.clone()
                },
                selected: selected.contains(&c.id),
            })
        })
        .collect()
}

/// The component whose symbol is within `tolerance` pixels of the cursor.
pub fn pick(
    project: &Project,
    p: &Projector,
    hidden: &BTreeSet<ObjectId>,
    cursor: [f64; 2],
    tolerance: f64,
) -> Option<ObjectId> {
    symbols(project, p, hidden, &BTreeSet::new())
        .into_iter()
        .map(|s| {
            let d = ((s.at[0] - cursor[0]).powi(2) + (s.at[1] - cursor[1]).powi(2)).sqrt();
            (s.id, d)
        })
        .filter(|(_, d)| *d <= tolerance)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
}

/// The components among `ids`, and the components that made any of them.
pub fn components_of(project: &Project, ids: &BTreeSet<ObjectId>) -> BTreeSet<ObjectId> {
    project
        .connections
        .values()
        .filter(|c| ids.contains(&c.id) || c.children.values().any(|id| ids.contains(id)))
        .map(|c| c.id)
        .collect()
}
