//! Broad phase over conservative world bounds, then closed geometric mesh intersection.
//! Curves retain the geometry kernel tessellation tolerance; errors remain explicit.
mod csg;
use open_bim_geometry::Body;
use open_bim_model::{
    DetailKind, ElementShape, Frame, ModelError, ObjectId, Point3, ProfileShape, Project,
};
use open_bim_numbering::Numbering;
use parry3d_f64::{
    math::{Pose, Vector},
    query::{self, PointQuery},
    shape::{TriMesh, TriMeshFlags},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const CONTACT_TOLERANCE_MM: f64 = 0.0001;
/// Bolt shafts are 32-sided prisms inscribed in the nominal diameter and reduced
/// by this radial margin, so a zero-clearance hole is still classified as a fit.
pub const BOLT_SHAFT_MARGIN_MM: f64 = 2. * CONTACT_TOLERANCE_MM;
const BOLT_SHAFT_SEGMENTS: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictKind {
    Touching,
    Intersection,
    AllowedBoltHole,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Conflict {
    /// Part ID, or the bolt group ID for bolt shaft rows.
    pub first: ObjectId,
    pub second: ObjectId,
    pub kind: ConflictKind,
    /// Zero-based bolt index in the group's row-major grid for bolt shaft rows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bolt: Option<usize>,
    /// World centre of the overlap bounds (or shaft centre), millimetres.
    pub location: [f64; 3],
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
impl Report {
    /// Rows that require a model change; touching and fitted bolts are informative.
    pub fn blocking(&self) -> impl Iterator<Item = &Conflict> {
        self.conflicts
            .iter()
            .filter(|c| c.kind == ConflictKind::Intersection)
    }
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
    fn centre_of_overlap(self, other: Self) -> [f64; 3] {
        std::array::from_fn(|i| {
            (self.min[i].max(other.min[i]) + self.max[i].min(other.max[i])) / 2.
        })
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

fn closed_volume(mesh: &TriMesh) -> f64 {
    mesh.triangles()
        .map(|t| t.a.dot(t.b.cross(t.c)) / 6.)
        .sum::<f64>()
}

/// True when an axis exists along which the vertex intervals overlap by at
/// most the contact tolerance: then the interiors are disjoint (zero volume).
fn separated(a: &TriMesh, b: &TriMesh, axes: &[[f64; 3]]) -> bool {
    axes.iter().any(|axis| {
        let axis = Vector::new(axis[0], axis[1], axis[2]);
        let range = |mesh: &TriMesh| {
            mesh.vertices()
                .iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
                    let t = v.dot(axis);
                    (lo.min(t), hi.max(t))
                })
        };
        let (a, b) = (range(a), range(b));
        a.1.min(b.1) - a.0.max(b.0) <= CONTACT_TOLERANCE_MM
    })
}

/// Overlap volume. Surfaces that do not cross are either disjoint or nested,
/// decided by an oriented point-in-mesh test; only crossing surfaces need BSP.
fn overlap_volume(a: &TriMesh, b: &TriMesh, axes: &[[f64; 3]]) -> Result<f64, ModelError> {
    let pose = Pose::IDENTITY;
    if separated(a, b, axes) {
        return Ok(0.);
    }
    if !query::intersection_test(&pose, a, &pose, b)
        .map_err(invalid)?
        .intersecting
    {
        let inside =
            |point: Vector, mesh: &TriMesh| mesh.project_local_point(point, false).is_inside;
        return Ok(if a.vertices().first().is_some_and(|v| inside(*v, b)) {
            closed_volume(a)
        } else if b.vertices().first().is_some_and(|v| inside(*v, a)) {
            closed_volume(b)
        } else {
            0.
        });
    }
    csg::intersection_volume(a, b).map_err(invalid)
}

/// Narrow phase returns positive-volume intersection, contact, or separation.
fn classify(
    a: &TriMesh,
    b: &TriMesh,
    axes: &[[f64; 3]],
) -> Result<Option<(ConflictKind, f64)>, ModelError> {
    let pose = Pose::IDENTITY;
    let volume = overlap_volume(a, b, axes)?;
    if volume > CONTACT_TOLERANCE_MM.powi(3) {
        return Ok(Some((ConflictKind::Intersection, volume)));
    }
    let separation = query::distance(&pose, a, &pose, b)
        .map_err(invalid)?
        .distance;
    Ok((separation <= CONTACT_TOLERANCE_MM).then_some((ConflictKind::Touching, 0.)))
}

struct Shaft {
    group: ObjectId,
    index: usize,
    parts: std::collections::BTreeSet<ObjectId>,
    mesh: TriMesh,
    centre: [f64; 3],
    axes: [[f64; 3]; 3],
}

fn world_axis(parent: &Frame, local: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| (0..3).map(|j| parent.axes[j][i] * local[j]).sum())
}

/// One shaft per bolt position spanning the union of the group's hole spans.
fn shafts(project: &Project, reference: Point3) -> Result<Vec<(Bounds, Shaft)>, ModelError> {
    let mut result = Vec::new();
    for detail in project.details.values() {
        let DetailKind::BoltGroup {
            parts,
            holes,
            frame,
            diameter,
            ..
        } = &detail.kind
        else {
            continue;
        };
        let dot = |a: [f64; 3], b: [f64; 3]| (0..3).map(|i| a[i] * b[i]).sum::<f64>();
        let radius = diameter / 2. - BOLT_SHAFT_MARGIN_MM;
        if radius <= 0. {
            return Err(invalid("диаметр болта меньше допуска"));
        }
        let mut index = 0;
        for [x, y] in detail.kind.bolt_positions() {
            let centre = frame.world(Point3([x, y, 0.]));
            let (mut low, mut high) = (f64::INFINITY, f64::NEG_INFINITY);
            for hole in holes {
                let Some(DetailKind::Hole {
                    part,
                    frame: local,
                    depth,
                    ..
                }) = project.details.get(hole).map(|d| &d.kind)
                else {
                    continue;
                };
                let parent = project.elements[part].shape.frame();
                let start = parent.world(local.origin);
                let delta: [f64; 3] = std::array::from_fn(|i| start.0[i] - centre.0[i]);
                let along = dot(delta, frame.axes[2]);
                let lateral = (dot(delta, delta) - along * along).max(0.).sqrt();
                if lateral > 1e-6 {
                    continue;
                }
                let end = along + depth * dot(world_axis(parent, local.axes[2]), frame.axes[2]);
                low = low.min(along.min(end));
                high = high.max(along.max(end));
            }
            if !(low.is_finite() && high - low > CONTACT_TOLERANCE_MM) {
                return Err(invalid(format!(
                    "болтовая группа #{} без отверстий в позиции {index}",
                    detail.id.0
                )));
            }
            let mut local = Vec::with_capacity(BOLT_SHAFT_SEGMENTS * 2);
            for z in [low, high] {
                for k in 0..BOLT_SHAFT_SEGMENTS {
                    let angle = std::f64::consts::TAU * k as f64 / BOLT_SHAFT_SEGMENTS as f64;
                    local.push(Point3([
                        x + radius * angle.cos(),
                        y + radius * angle.sin(),
                        z,
                    ]));
                }
            }
            let world: Vec<Point3> = local.iter().map(|p| frame.world(*p)).collect();
            let n = BOLT_SHAFT_SEGMENTS as u32;
            let mut indices = Vec::new();
            for k in 1..n - 1 {
                indices.push([0, k + 1, k]);
                indices.push([n, n + k, n + k + 1]);
            }
            for k in 0..n {
                let next = (k + 1) % n;
                indices.push([k, next, n + next]);
                indices.push([k, n + next, n + k]);
            }
            let mesh = TriMesh::with_flags(
                world
                    .iter()
                    .map(|p| {
                        Vector::new(
                            p.0[0] - reference.0[0],
                            p.0[1] - reference.0[1],
                            p.0[2] - reference.0[2],
                        )
                    })
                    .collect(),
                indices,
                TriMeshFlags::ORIENTED | TriMeshFlags::HALF_EDGE_TOPOLOGY,
            )
            .map_err(invalid)?;
            let mid = frame.world(Point3([x, y, (low + high) / 2.]));
            result.push((
                Bounds::points(world),
                Shaft {
                    group: detail.id,
                    index,
                    parts: parts.clone(),
                    mesh,
                    centre: mid.0,
                    axes: frame.axes,
                },
            ));
            index += 1;
        }
    }
    Ok(result)
}

#[derive(Clone, Copy)]
enum Item {
    Part(ObjectId),
    Shaft(usize),
}

/// Reports are deterministic by object ID and do not alter model/history.
/// Bolt shafts are checked against every part they reach: a part of the bolt
/// group whose net body leaves the shaft free is an allowed bolt hole; any
/// material inside a shaft (missing, short or undersized hole, foreign part)
/// is an intersection.
pub fn check(project: &Project, numbering: &Numbering) -> Result<Report, ModelError> {
    project.validate()?;
    let reference = project.origin;
    let shafts = shafts(project, reference)?;
    let mut entries = project
        .elements
        .keys()
        .map(|id| Ok((Item::Part(*id), bounds(project, *id)?)))
        .collect::<Result<Vec<_>, ModelError>>()?;
    entries.extend(
        shafts
            .iter()
            .enumerate()
            .map(|(i, (b, _))| (Item::Shaft(i), *b)),
    );
    let key = |item: Item| match item {
        Item::Part(id) => (id, usize::MAX),
        Item::Shaft(i) => (shafts[i].1.group, shafts[i].1.index),
    };
    entries.sort_by(|a, b| {
        a.1.min[0]
            .total_cmp(&b.1.min[0])
            .then(key(a.0).cmp(&key(b.0)))
    });
    let mut cache = BTreeMap::new();
    let mut active: Vec<(Item, Bounds)> = Vec::new();
    let mut report = Report {
        revision: project.revision,
        candidates: 0,
        conflicts: Vec::new(),
    };
    for (item, bound) in entries {
        active.retain(|(_, b)| b.max[0] + CONTACT_TOLERANCE_MM >= bound.min[0]);
        for (other, other_bound) in &active {
            if !other_bound.intersects(bound) {
                continue;
            }
            let (part, shaft) = match (item, *other) {
                (Item::Shaft(_), Item::Shaft(_)) => continue,
                (Item::Part(a), Item::Part(b)) => {
                    report.candidates += 1;
                    for part in [a, b] {
                        if let std::collections::btree_map::Entry::Vacant(entry) = cache.entry(part)
                        {
                            entry.insert(mesh(
                                project.element_body(part)?,
                                project.elements[&part].shape.frame(),
                                reference,
                            )?);
                        }
                    }
                    let axes: Vec<[f64; 3]> = [a, b]
                        .iter()
                        .flat_map(|id| project.elements[id].shape.frame().axes)
                        .collect();
                    if let Some((kind, overlap_mm3)) = classify(&cache[&a], &cache[&b], &axes)? {
                        let (first, second) = if a < b { (a, b) } else { (b, a) };
                        report.conflicts.push(Conflict {
                            first,
                            second,
                            kind,
                            bolt: None,
                            location: bound.centre_of_overlap(*other_bound),
                            first_position: numbering.parts.get(&first).cloned(),
                            second_position: numbering.parts.get(&second).cloned(),
                            overlap_mm3,
                        });
                    }
                    continue;
                }
                (Item::Part(p), Item::Shaft(s)) | (Item::Shaft(s), Item::Part(p)) => (p, s),
            };
            report.candidates += 1;
            if let std::collections::btree_map::Entry::Vacant(entry) = cache.entry(part) {
                entry.insert(mesh(
                    project.element_body(part)?,
                    project.elements[&part].shape.frame(),
                    reference,
                )?);
            }
            let shaft = &shafts[shaft].1;
            let mut axes = project.elements[&part].shape.frame().axes.to_vec();
            axes.extend(shaft.axes);
            let volume = overlap_volume(&shaft.mesh, &cache[&part], &axes)?;
            let kind = if volume > CONTACT_TOLERANCE_MM.powi(3) {
                ConflictKind::Intersection
            } else if shaft.parts.contains(&part) {
                ConflictKind::AllowedBoltHole
            } else {
                continue;
            };
            report.conflicts.push(Conflict {
                first: shaft.group,
                second: part,
                kind,
                bolt: Some(shaft.index),
                location: shaft.centre,
                first_position: None,
                second_position: numbering.parts.get(&part).cloned(),
                overlap_mm3: if kind == ConflictKind::Intersection {
                    volume
                } else {
                    0.
                },
            });
        }
        active.push((item, bound));
    }
    report
        .conflicts
        .sort_by_key(|r| (r.first, r.second, r.bolt));
    Ok(report)
}
