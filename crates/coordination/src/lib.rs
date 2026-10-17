//! Broad phase over conservative world bounds, then closed geometric mesh intersection.
//! Curves retain the geometry kernel tessellation tolerance; errors remain explicit.
mod csg;
use open_bim_geometry::Body;
use open_bim_model::{ElementShape, Frame, ModelError, ObjectId, Point3, ProfileShape, Project};
use open_bim_numbering::Numbering;
use parry3d_f64::{
    math::{Pose, Vector},
    query,
    shape::{TriMesh, TriMeshFlags},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const CONTACT_TOLERANCE_MM: f64 = 0.0001;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictKind {
    Touching,
    Intersection,
    AllowedBoltHole,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Conflict {
    pub first: ObjectId,
    pub second: ObjectId,
    pub kind: ConflictKind,
    pub first_position: Option<String>,
    pub second_position: Option<String>,
    /// Approximate intersection volume of closed tessellated bodies.
    pub overlap_mm3: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub revision: u64,
    pub candidates: usize,
    pub conflicts: Vec<Conflict>,
}

#[derive(Clone, Copy, Debug)]
struct Bounds {
    min: [f64; 3],
    max: [f64; 3],
}
impl Bounds {
    fn points(points: impl IntoIterator<Item = Point3>) -> Self {
        let mut b = Self {
            min: [f64::INFINITY; 3],
            max: [f64::NEG_INFINITY; 3],
        };
        for p in points {
            for i in 0..3 {
                b.min[i] = b.min[i].min(p.0[i]);
                b.max[i] = b.max[i].max(p.0[i]);
            }
        }
        b
    }
    fn intersects(self, other: Self) -> bool {
        (0..3).all(|i| {
            self.min[i] <= other.max[i] + CONTACT_TOLERANCE_MM
                && other.min[i] <= self.max[i] + CONTACT_TOLERANCE_MM
        })
    }
}

fn invalid(message: impl std::fmt::Display) -> ModelError {
    ModelError::InvalidValue(format!("Проверка коллизий: {message}"))
}

fn bounds(project: &Project, id: ObjectId) -> Result<Bounds, ModelError> {
    let part = &project.elements[&id];
    let (points, depth) = match &part.shape {
        ElementShape::Plate {
            contour, thickness, ..
        } => (contour.clone(), thickness.value()),
        ElementShape::Member {
            profile, length, ..
        } => {
            let points = match project.profiles[profile].shape {
                ProfileShape::CircularHollow { diameter, .. } => vec![
                    [-diameter / 2., -diameter / 2.],
                    [diameter / 2., diameter / 2.],
                ],
                ProfileShape::RectangularHollow { width, height, .. } => {
                    vec![[-width / 2., -height / 2.], [width / 2., height / 2.]]
                }
                ref shape => shape.solid_contour()?,
            };
            (points, length.value())
        }
    };
    // Use all corners of the local bounding box for rotated hollow sections too.
    let lo = [
        points.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min),
        points.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min),
    ];
    let hi = [
        points
            .iter()
            .map(|p| p[0])
            .fold(f64::NEG_INFINITY, f64::max),
        points
            .iter()
            .map(|p| p[1])
            .fold(f64::NEG_INFINITY, f64::max),
    ];
    Ok(Bounds::points([lo[0], hi[0]].into_iter().flat_map(|x| {
        [lo[1], hi[1]].into_iter().flat_map(move |y| {
            [0., depth]
                .into_iter()
                .map(move |z| part.shape.frame().world(Point3([x, y, z])))
        })
    })))
}

fn mesh(body: Body, frame: &Frame, reference: Point3) -> Result<TriMesh, ModelError> {
    let (vertices, indices) = body.tessellation_f64().map_err(invalid)?;
    let vertices = vertices
        .into_iter()
        .map(|p| {
            let p = frame.world(Point3(p));
            Vector::new(
                p.0[0] - reference.0[0],
                p.0[1] - reference.0[1],
                p.0[2] - reference.0[2],
            )
        })
        .collect();
    TriMesh::with_flags(
        vertices,
        indices,
        TriMeshFlags::ORIENTED
            | TriMeshFlags::HALF_EDGE_TOPOLOGY
            | TriMeshFlags::MERGE_DUPLICATE_VERTICES,
    )
    .map_err(invalid)
}

/// Narrow phase returns positive-volume intersection, contact, or separation.
fn classify(a: &TriMesh, b: &TriMesh) -> Result<Option<(ConflictKind, f64)>, ModelError> {
    let pose = Pose::IDENTITY;
    let volume = csg::intersection_volume(a, b).map_err(invalid)?;
    if volume > CONTACT_TOLERANCE_MM.powi(3) {
        return Ok(Some((ConflictKind::Intersection, volume)));
    }
    let separation = query::distance(&pose, a, &pose, b)
        .map_err(invalid)?
        .distance;
    Ok((separation <= CONTACT_TOLERANCE_MM).then_some((ConflictKind::Touching, 0.)))
}

/// Reports are deterministic by object ID and do not alter model/history.
pub fn check(project: &Project, numbering: &Numbering) -> Result<Report, ModelError> {
    project.validate()?;
    let mut entries = project
        .elements
        .keys()
        .map(|id| Ok((*id, bounds(project, *id)?)))
        .collect::<Result<Vec<_>, ModelError>>()?;
    entries.sort_by(|a, b| a.1.min[0].total_cmp(&b.1.min[0]).then(a.0.cmp(&b.0)));
    let reference = project.origin;
    let mut cache = BTreeMap::new();
    let mut active: Vec<(ObjectId, Bounds)> = Vec::new();
    let mut report = Report {
        revision: project.revision,
        candidates: 0,
        conflicts: Vec::new(),
    };
    for (id, bound) in entries {
        active.retain(|(_, b)| b.max[0] + CONTACT_TOLERANCE_MM >= bound.min[0]);
        for (other, b) in &active {
            if !b.intersects(bound) {
                continue;
            }
            report.candidates += 1;
            for part in [id, *other] {
                if let std::collections::btree_map::Entry::Vacant(entry) = cache.entry(part) {
                    entry.insert(mesh(
                        project.element_body(part)?,
                        project.elements[&part].shape.frame(),
                        reference,
                    )?);
                }
            }
            if let Some((kind, overlap_mm3)) = classify(&cache[&id], &cache[other])? {
                let (first, second) = if id < *other {
                    (id, *other)
                } else {
                    (*other, id)
                };
                report.conflicts.push(Conflict {
                    first,
                    second,
                    kind,
                    first_position: numbering.parts.get(&first).cloned(),
                    second_position: numbering.parts.get(&second).cloned(),
                    overlap_mm3,
                });
            }
        }
        active.push((id, bound));
    }
    report.conflicts.sort_by_key(|r| (r.first, r.second));
    Ok(report)
}
