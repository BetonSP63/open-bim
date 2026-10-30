//! Orthographic projection, feature edges, ray-cast hidden line removal and
//! planar sections of exact-body tessellations.
use open_bim_model::{ObjectId, Project, ViewDirection};
use parry3d_f64::{
    math::Vector,
    query::{Ray, RayCast},
    shape::TriMesh,
};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};

pub type V3 = [f64; 3];
pub(crate) fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub(crate) fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub(crate) fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
pub(crate) fn mul(a: V3, s: f64) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
pub(crate) fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn neg(a: V3) -> V3 {
    [-a[0], -a[1], -a[2]]
}

/// Right-handed drawing frame in world coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Basis {
    pub origin: V3,
    pub x: V3,
    pub y: V3,
    pub z: V3,
}

/// View axes: `right`, `up` on paper; `toward` points at the viewer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewBasis {
    pub origin: V3,
    pub right: V3,
    pub up: V3,
    pub toward: V3,
}

impl ViewBasis {
    /// First-angle projection directions (ГОСТ 2.305) in the drawing frame.
    pub fn new(frame: &Basis, direction: ViewDirection) -> Self {
        let (right, up, toward) = match direction {
            ViewDirection::Front => (frame.x, frame.y, frame.z),
            ViewDirection::Top => (frame.x, neg(frame.z), frame.y),
            ViewDirection::Left => (frame.z, frame.y, neg(frame.x)),
            ViewDirection::Right => (neg(frame.z), frame.y, frame.x),
            ViewDirection::Bottom => (frame.x, frame.z, neg(frame.y)),
            ViewDirection::Back => (neg(frame.x), frame.y, neg(frame.z)),
        };
        Self {
            origin: frame.origin,
            right,
            up,
            toward,
        }
    }
    /// (u, v, depth) in model millimetres; larger depth is nearer the viewer.
    pub fn project(&self, p: V3) -> V3 {
        let d = sub(p, self.origin);
        [dot(d, self.right), dot(d, self.up), dot(d, self.toward)]
    }
}

/// World-placed tessellation of one part with a ray-cast acceleration tree.
#[derive(Clone)]
pub struct PartMesh {
    pub id: ObjectId,
    pub vertices: Vec<V3>,
    pub triangles: Vec<[u32; 3]>,
    trimesh: TriMesh,
}

impl PartMesh {
    pub fn new(project: &Project, id: ObjectId) -> Result<Self, String> {
        let element = project
            .elements
            .get(&id)
            .ok_or_else(|| format!("Деталь #{} не найдена", id.0))?;
        let body = project.element_body(id).map_err(|e| e.to_string())?;
        let (local, triangles) = body.tessellation_f64().map_err(|e| e.to_string())?;
        let frame = element.shape.frame();
        // Merge coincident tessellation vertices so shared edges are detected.
        let mut index = HashMap::new();
        let mut vertices = Vec::new();
        let remap: Vec<u32> = local
            .iter()
            .map(|p| {
                let w = frame.world(open_bim_model::Point3(*p)).0;
                let key = w.map(|v| (v * 1e6).round() as i64);
                *index.entry(key).or_insert_with(|| {
                    vertices.push(w);
                    (vertices.len() - 1) as u32
                })
            })
            .collect();
        let triangles: Vec<[u32; 3]> = triangles
            .iter()
            .map(|t| t.map(|i| remap[i as usize]))
            .filter(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2])
            .collect();
        let trimesh = TriMesh::new(
            vertices
                .iter()
                .map(|p| Vector::new(p[0], p[1], p[2]))
                .collect(),
            triangles.clone(),
        )
        .map_err(|e| format!("Сетка детали #{}: {e}", id.0))?;
        Ok(Self {
            id,
            vertices,
            triangles,
            trimesh,
        })
    }

    fn normal(&self, t: [u32; 3]) -> Option<V3> {
        let [a, b, c] = t.map(|i| self.vertices[i as usize]);
        let n = cross(sub(b, a), sub(c, a));
        let l = dot(n, n).sqrt();
        (l > 1e-12).then(|| mul(n, 1. / l))
    }

    /// Crease, boundary and view-dependent silhouette edges.
    pub fn feature_edges(&self, toward: V3) -> Vec<(V3, V3)> {
        let mut faces: BTreeMap<(u32, u32), Vec<V3>> = BTreeMap::new();
        for t in &self.triangles {
            let Some(n) = self.normal(*t) else { continue };
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                faces.entry((a.min(b), a.max(b))).or_default().push(n);
            }
        }
        let crease = 15f64.to_radians().cos();
        faces
            .into_iter()
            .filter(|(_, normals)| match normals.as_slice() {
                // Faces seen edge-on (|n·v| ≈ 0) are not silhouettes; their
                // outline is already a crease or boundary edge.
                [a, b] => {
                    let (da, db) = (dot(*a, toward), dot(*b, toward));
                    dot(*a, *b) < crease || (da * db < 0. && da.abs().min(db.abs()) > 1e-9)
                }
                _ => true,
            })
            .map(|((a, b), _)| (self.vertices[a as usize], self.vertices[b as usize]))
            .collect()
    }

    /// Distance along `direction` to the first surface hit, if any.
    pub fn ray(&self, origin: V3, direction: V3, max: f64) -> Option<f64> {
        let ray = Ray::new(
            Vector::new(origin[0], origin[1], origin[2]),
            Vector::new(direction[0], direction[1], direction[2]),
        );
        self.trimesh.cast_local_ray(&ray, max, false)
    }

    /// Segments of the mesh cut by the plane `dot(p - origin, normal) = 0`.
    pub fn section(&self, origin: V3, normal: V3) -> Vec<(V3, V3)> {
        let side = |p: V3| dot(sub(p, origin), normal);
        let mut result = Vec::new();
        for t in &self.triangles {
            let p = t.map(|i| self.vertices[i as usize]);
            let d = p.map(side);
            let mut points = Vec::new();
            for k in 0..3 {
                let (a, b) = (k, (k + 1) % 3);
                if (d[a] > 0.) != (d[b] > 0.) {
                    let s = d[a] / (d[a] - d[b]);
                    points.push(add(p[a], mul(sub(p[b], p[a]), s)));
                }
            }
            if let [a, b] = points.as_slice() {
                result.push((*a, *b));
            }
        }
        result
    }
}

pub const RAY_EPSILON_MM: f64 = 1e-3;

/// True when nothing lies between the point and the viewer (within `limit`).
pub fn unobstructed(point: V3, toward: V3, meshes: &[PartMesh], limit: f64) -> bool {
    let origin = add(point, mul(toward, RAY_EPSILON_MM));
    meshes.iter().all(|m| {
        m.ray(origin, toward, limit)
            .is_none_or(|t| t <= RAY_EPSILON_MM)
    })
}

/// Visible and hidden runs of a world segment as parameter intervals.
pub fn visibility_runs(
    a: V3,
    b: V3,
    toward: V3,
    meshes: &[PartMesh],
    limit_at: impl Fn(V3) -> f64,
    samples: usize,
) -> Vec<(f64, f64, bool)> {
    let at = |t: f64| add(a, mul(sub(b, a), t));
    let visible = |t: f64| {
        let p = at(t);
        unobstructed(p, toward, meshes, limit_at(p))
    };
    let n = samples.max(2);
    let flags: Vec<bool> = (0..=n).map(|i| visible(i as f64 / n as f64)).collect();
    let mut runs: Vec<(f64, f64, bool)> = Vec::new();
    let mut start = 0.;
    for i in 0..n {
        let (t0, t1) = (i as f64 / n as f64, (i + 1) as f64 / n as f64);
        if flags[i] != flags[i + 1] {
            let (mut lo, mut hi) = (t0, t1);
            for _ in 0..12 {
                let mid = (lo + hi) / 2.;
                if visible(mid) == flags[i] {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            runs.push((start, lo, flags[i]));
            start = lo;
        }
    }
    runs.push((start, 1., flags[n]));
    runs.retain(|r| r.1 - r.0 > 1e-9);
    runs
}

pub type Segment = ([f64; 2], [f64; 2]);
/// Line direction, normal and offset with the merged parameter intervals.
type LineGroup = (([f64; 2], [f64; 2], f64), Vec<(f64, f64)>);

/// Group collinear segments by supporting line; returns merged intervals.
fn line_groups(segments: &[Segment]) -> HashMap<[i64; 3], LineGroup> {
    let mut groups: HashMap<[i64; 3], LineGroup> = HashMap::new();
    for (a, b) in segments {
        let d = [b[0] - a[0], b[1] - a[1]];
        let len = (d[0] * d[0] + d[1] * d[1]).sqrt();
        if len < 1e-9 {
            continue;
        }
        let mut u = [d[0] / len, d[1] / len];
        if u[0] < -1e-12 || (u[0].abs() <= 1e-12 && u[1] < 0.) {
            u = [-u[0], -u[1]];
        }
        let normal = [-u[1], u[0]];
        let offset = a[0] * normal[0] + a[1] * normal[1];
        let key = [
            (u[0] * 1e6).round() as i64,
            (u[1] * 1e6).round() as i64,
            (offset * 1e4).round() as i64,
        ];
        let t = |p: [f64; 2]| p[0] * u[0] + p[1] * u[1];
        let (t0, t1) = (t(*a).min(t(*b)), t(*a).max(t(*b)));
        groups
            .entry(key)
            .or_insert_with(|| ((u, normal, offset), Vec::new()))
            .1
            .push((t0, t1));
    }
    for (_, intervals) in groups.values_mut() {
        intervals.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut merged: Vec<(f64, f64)> = Vec::new();
        for (a, b) in intervals.drain(..) {
            match merged.last_mut() {
                Some(last) if a <= last.1 + 1e-6 => last.1 = last.1.max(b),
                _ => merged.push((a, b)),
            }
        }
        *intervals = merged;
    }
    groups
}

fn rebuild((u, normal, offset): ([f64; 2], [f64; 2], f64), (t0, t1): (f64, f64)) -> Segment {
    let at = |t: f64| [u[0] * t + normal[0] * offset, u[1] * t + normal[1] * offset];
    (at(t0), at(t1))
}

/// Merge collinear overlaps and drop hidden parts covered by visible lines
/// (visible outlines take precedence, ГОСТ 2.303).
pub fn clean(visible: &[Segment], hidden: &[Segment]) -> (Vec<Segment>, Vec<Segment>) {
    let visible_groups = line_groups(visible);
    let mut shown = Vec::new();
    for (axes, intervals) in visible_groups.values() {
        for interval in intervals {
            shown.push(rebuild(*axes, *interval));
        }
    }
    let mut concealed = Vec::new();
    for (key, (axes, intervals)) in line_groups(hidden) {
        let cover = visible_groups
            .get(&key)
            .map(|g| g.1.as_slice())
            .unwrap_or(&[]);
        for (mut a, b) in intervals {
            for (c, d) in cover {
                if *d <= a || *c >= b {
                    continue;
                }
                if *c > a {
                    concealed.push(rebuild(axes, (a, *c)));
                }
                a = a.max(*d);
                if a >= b {
                    break;
                }
            }
            if b - a > 1e-6 {
                concealed.push(rebuild(axes, (a, b)));
            }
        }
    }
    let order = |v: &mut Vec<Segment>| {
        v.sort_by(|x, y| {
            (x.0[0], x.0[1], x.1[0], x.1[1])
                .partial_cmp(&(y.0[0], y.0[1], y.1[0], y.1[1]))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    };
    order(&mut shown);
    order(&mut concealed);
    (shown, concealed)
}

/// Join 2D segments sharing endpoints into polylines, then simplify.
pub fn polylines(segments: &[([f64; 2], [f64; 2])], tolerance: f64) -> Vec<Vec<[f64; 2]>> {
    let key = |p: [f64; 2]| p.map(|v| (v * 1e4).round() as i64);
    let mut at: HashMap<[i64; 2], Vec<usize>> = HashMap::new();
    for (i, (a, b)) in segments.iter().enumerate() {
        at.entry(key(*a)).or_default().push(i);
        at.entry(key(*b)).or_default().push(i);
    }
    let mut used = vec![false; segments.len()];
    let mut result = Vec::new();
    for start in 0..segments.len() {
        if used[start] {
            continue;
        }
        used[start] = true;
        let mut line = vec![segments[start].0, segments[start].1];
        for forward in [true, false] {
            loop {
                let (end, previous) = if forward {
                    (line[line.len() - 1], line[line.len() - 2])
                } else {
                    (line[0], line[1])
                };
                let heading = [end[0] - previous[0], end[1] - previous[1]];
                // Prefer the straightest continuation; stop on reversals.
                let next = at.get(&key(end)).and_then(|list| {
                    list.iter()
                        .copied()
                        .filter(|i| !used[*i])
                        .map(|i| {
                            let (a, b) = segments[i];
                            let other = if key(a) == key(end) { b } else { a };
                            let d = [other[0] - end[0], other[1] - end[1]];
                            let norm =
                                (heading[0].hypot(heading[1]) * d[0].hypot(d[1])).max(1e-300);
                            (i, (heading[0] * d[0] + heading[1] * d[1]) / norm)
                        })
                        .filter(|(_, cos)| *cos > -0.5)
                        .max_by(|x, y| x.1.total_cmp(&y.1))
                        .map(|(i, _)| i)
                });
                let Some(i) = next else { break };
                used[i] = true;
                let (a, b) = segments[i];
                let other = if key(a) == key(end) { b } else { a };
                if forward {
                    line.push(other);
                } else {
                    line.insert(0, other);
                }
            }
        }
        result.push(simplify(&line, tolerance));
    }
    result
}

fn simplify(points: &[[f64; 2]], tolerance: f64) -> Vec<[f64; 2]> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    let mut stack = vec![(0, points.len() - 1)];
    while let Some((i, j)) = stack.pop() {
        let (a, b) = (points[i], points[j]);
        let d = [b[0] - a[0], b[1] - a[1]];
        let len = (d[0] * d[0] + d[1] * d[1]).sqrt();
        let mut best = (0., i);
        for (k, p) in points.iter().enumerate().take(j).skip(i + 1) {
            let e = [p[0] - a[0], p[1] - a[1]];
            let dist = if len < 1e-12 {
                (e[0] * e[0] + e[1] * e[1]).sqrt()
            } else {
                (d[0] * e[1] - d[1] * e[0]).abs() / len
            };
            if dist > best.0 {
                best = (dist, k);
            }
        }
        if best.0 > tolerance {
            keep[best.1] = true;
            stack.push((i, best.1));
            stack.push((best.1, j));
        }
    }
    points
        .iter()
        .zip(keep)
        .filter_map(|(p, k)| k.then_some(*p))
        .collect()
}

/// Closed loops from section segments (endpoint matching at 1e-6 mm).
pub fn loops(segments: &[([f64; 2], [f64; 2])]) -> Vec<Vec<[f64; 2]>> {
    polylines(segments, 0.)
        .into_iter()
        .filter(|l| l.len() >= 3)
        .collect()
}

/// 45° hatch lines clipped to loops by the even-odd rule.
pub fn hatch(loops: &[Vec<[f64; 2]>], spacing: f64) -> Vec<([f64; 2], [f64; 2])> {
    // Rotate by -45° so hatch lines become horizontal scanlines.
    let (c, s) = (
        std::f64::consts::FRAC_1_SQRT_2,
        std::f64::consts::FRAC_1_SQRT_2,
    );
    let rot = |p: [f64; 2]| [c * p[0] + s * p[1], -s * p[0] + c * p[1]];
    let back = |p: [f64; 2]| [c * p[0] - s * p[1], s * p[0] + c * p[1]];
    let rotated: Vec<Vec<[f64; 2]>> = loops
        .iter()
        .map(|l| l.iter().map(|p| rot(*p)).collect())
        .collect();
    let (lo, hi) = rotated
        .iter()
        .flatten()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
            (lo.min(p[1]), hi.max(p[1]))
        });
    let mut result = Vec::new();
    if !lo.is_finite() || spacing <= 0. {
        return result;
    }
    let mut y = (lo / spacing).ceil() * spacing;
    while y < hi {
        let mut xs = Vec::new();
        for l in &rotated {
            for k in 0..l.len() {
                let (a, b) = (l[k], l[(k + 1) % l.len()]);
                if (a[1] > y) != (b[1] > y) {
                    xs.push(a[0] + (y - a[1]) / (b[1] - a[1]) * (b[0] - a[0]));
                }
            }
        }
        xs.sort_by(f64::total_cmp);
        for [a, b] in xs.as_chunks::<2>().0 {
            result.push((back([*a, y]), back([*b, y])));
        }
        y += spacing;
    }
    result
}
