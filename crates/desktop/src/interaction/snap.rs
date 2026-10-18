//! Screen-space snapping to model geometry (Tekla «Привязки»). Candidates come
//! from the scene BVH near the cursor ray; hits are ranked by snap priority,
//! then by distance in pixels.
use super::workplane::Workplane;
use open_bim_model::{ElementShape, ObjectId, Point3, Project};
use open_bim_renderer::{Camera, DVec3, Ray, Scene};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SnapKind {
    /// Reference points: member ends, plate corners, grid line ends.
    Reference,
    /// Geometry corners of the displayed solid.
    EndPoint,
    MidPoint,
    Intersection,
    Perpendicular,
    Nearest,
    Free,
}

impl SnapKind {
    /// Snap priority class (lower wins; ties go to the nearer point). Like
    /// Tekla, reference points and grid/reference-line intersections («Points»)
    /// share the highest class, so the closer of the two is taken.
    pub fn priority(self) -> u8 {
        match self {
            Self::Reference | Self::Intersection => 0,
            Self::EndPoint => 1,
            Self::MidPoint => 2,
            Self::Perpendicular => 3,
            Self::Nearest => 4,
            Self::Free => 5,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Reference => "Опорная точка",
            Self::EndPoint => "Конечная точка",
            Self::MidPoint => "Середина",
            Self::Intersection => "Пересечение",
            Self::Perpendicular => "Перпендикуляр",
            Self::Nearest => "Ближайшая точка",
            Self::Free => "Свободная точка",
        }
    }
}

/// Snap switches (bottom toolbar). Defaults follow Tekla: reference and
/// geometry points, mid points and intersections on; nearest and
/// perpendicular off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Switches {
    /// Tekla main switch «Reference lines and points».
    pub reference: bool,
    /// Tekla main switch «Geometry lines and points»: solid corners and edges.
    pub geometry: bool,
    pub end: bool,
    pub mid: bool,
    pub intersection: bool,
    pub perpendicular: bool,
    pub nearest: bool,
    pub ortho: bool,
}

impl Default for Switches {
    fn default() -> Self {
        Self {
            reference: true,
            geometry: true,
            end: true,
            mid: true,
            intersection: true,
            perpendicular: false,
            nearest: false,
            ortho: false,
        }
    }
}

impl Switches {
    pub fn enabled(&self, kind: SnapKind) -> bool {
        match kind {
            SnapKind::Reference => self.reference,
            SnapKind::EndPoint => self.end,
            SnapKind::MidPoint => self.mid,
            SnapKind::Intersection => self.intersection,
            SnapKind::Perpendicular => self.perpendicular,
            SnapKind::Nearest => self.nearest,
            SnapKind::Free => true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub kind: SnapKind,
    /// From reference geometry (part axes, contours, grid) rather than the solid.
    pub reference: bool,
    pub point: DVec3,
    pub object: Option<ObjectId>,
    pub pixels: f64,
}

/// Viewport mapping: camera, scene origin and size in pixels.
#[derive(Clone, Copy)]
pub struct View<'a> {
    pub camera: &'a Camera,
    pub origin: DVec3,
    pub size: [f64; 2],
}

impl View<'_> {
    fn aspect(&self) -> f64 {
        self.size[0] / self.size[1].max(1.)
    }
    pub fn screen(&self, p: DVec3) -> Option<[f64; 2]> {
        self.camera
            .project(self.origin, p, self.aspect())
            .map(|uv| [uv[0] * self.size[0], uv[1] * self.size[1]])
    }
    pub fn ray(&self, px: [f64; 2]) -> Option<Ray> {
        self.camera.ray(
            self.origin,
            [px[0] / self.size[0], px[1] / self.size[1]],
            self.aspect(),
        )
    }
    /// World millimetres per pixel at a distance from the eye.
    fn mm_per_pixel(&self, distance: f64) -> f64 {
        if self.camera.orthographic {
            self.camera.ortho_height / self.size[1].max(1.)
        } else {
            2. * distance * (22.5_f64.to_radians()).tan() / self.size[1].max(1.)
        }
    }
}

type Segment = (DVec3, DVec3, Option<ObjectId>, bool);

/// Most snap candidates kept for Tab cycling.
pub const MAX_CANDIDATES: usize = 32;

/// Feature edges per display mesh, cached across queries.
#[derive(Default)]
pub struct Snapper {
    edges: HashMap<usize, Arc<Vec<(DVec3, DVec3)>>>,
    scene: Option<usize>,
}

fn feature_edges(mesh: &open_bim_renderer::DisplayMesh) -> Vec<(DVec3, DVec3)> {
    let mut faces: HashMap<(u32, u32), Vec<DVec3>> = HashMap::new();
    for t in &mesh.triangles {
        let [a, b, c] = t.map(|i| mesh.positions[i as usize]);
        let Some(n) = (b - a).cross(c - a).try_normalize() else {
            continue;
        };
        for k in 0..3 {
            let (x, y) = (t[k], t[(k + 1) % 3]);
            faces.entry((x.min(y), x.max(y))).or_default().push(n);
        }
    }
    let crease = 15f64.to_radians().cos();
    let mut edges: Vec<(DVec3, DVec3)> = faces
        .into_iter()
        .filter(|(_, n)| n.len() != 2 || n[0].dot(n[1]) < crease)
        .map(|((a, b), _)| (mesh.positions[a as usize], mesh.positions[b as usize]))
        .collect();
    edges.sort_by(|a, b| {
        a.0.to_array()
            .partial_cmp(&b.0.to_array())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    edges
}

fn closest_on_segment_to_ray(a: DVec3, b: DVec3, ray: Ray) -> DVec3 {
    let d = b - a;
    let r = ray.direction;
    let w = a - ray.origin;
    let (aa, bb, cc, dd, ee) = (d.dot(d), d.dot(r), r.dot(r), d.dot(w), r.dot(w));
    let denominator = aa * cc - bb * bb;
    let t = if denominator.abs() < 1e-12 {
        0.
    } else {
        (bb * ee - cc * dd) / denominator
    };
    a + d * t.clamp(0., 1.)
}

/// Intersection of two segments in 3D (closest points closer than `eps`).
fn intersection(a: (DVec3, DVec3), b: (DVec3, DVec3), eps: f64) -> Option<DVec3> {
    let (d1, d2, r) = (a.1 - a.0, b.1 - b.0, a.0 - b.0);
    let (aa, ee, f) = (d1.dot(d1), d2.dot(d2), d2.dot(r));
    let (c, bb) = (d1.dot(r), d1.dot(d2));
    let denominator = aa * ee - bb * bb;
    if denominator.abs() < 1e-9 * aa.max(1.) * ee.max(1.) {
        return None;
    }
    let s = (bb * f - c * ee) / denominator;
    let t = (aa * f - bb * c) / denominator;
    if !(-1e-9..=1. + 1e-9).contains(&s) || !(-1e-9..=1. + 1e-9).contains(&t) {
        return None;
    }
    let (p, q) = (a.0 + d1 * s, b.0 + d2 * t);
    (p.distance(q) <= eps).then_some((p + q) * 0.5)
}

/// Ortho constraint: direction from `previous` rounded to 45° in the work plane.
pub fn ortho(previous: DVec3, point: DVec3, workplane: &Workplane) -> DVec3 {
    let local = workplane.to_local(point) - workplane.to_local(previous);
    let length = (local.x * local.x + local.y * local.y).sqrt();
    if length < 1e-9 {
        return point;
    }
    let step = std::f64::consts::FRAC_PI_4;
    let angle = (local.y.atan2(local.x) / step).round() * step;
    let rounded = DVec3::new(angle.cos() * length, angle.sin() * length, local.z);
    workplane.to_world(workplane.to_local(previous) + rounded)
}

impl Snapper {
    /// The best snap under the cursor.
    #[allow(clippy::too_many_arguments)]
    pub fn snap(
        &mut self,
        project: &Project,
        scene: &Arc<Scene>,
        view: View,
        cursor: [f64; 2],
        switches: Switches,
        tolerance: f64,
        previous: Option<DVec3>,
        workplane: &Workplane,
    ) -> Option<Hit> {
        self.candidates(
            project, scene, view, cursor, switches, tolerance, previous, workplane,
        )
        .into_iter()
        .next()
    }

    /// All distinct snap points under the cursor, best first (Tekla Tab /
    /// Shift+Tab cycles through them); a free work plane point when none.
    #[allow(clippy::too_many_arguments)]
    pub fn candidates(
        &mut self,
        project: &Project,
        scene: &Arc<Scene>,
        view: View,
        cursor: [f64; 2],
        switches: Switches,
        tolerance: f64,
        previous: Option<DVec3>,
        workplane: &Workplane,
    ) -> Vec<Hit> {
        let Some(ray) = view.ray(cursor) else {
            return Vec::new();
        };
        let key = Arc::as_ptr(scene) as usize;
        if self.scene != Some(key) {
            self.edges.clear();
            self.scene = Some(key);
        }
        let near = scene.near_ray(ray, |d| tolerance * view.mm_per_pixel(d), 128);
        let pixels = |p: DVec3| {
            view.screen(p)
                .map(|s| ((s[0] - cursor[0]).powi(2) + (s[1] - cursor[1]).powi(2)).sqrt())
        };
        let mut found: Vec<Hit> = Vec::new();
        let mut offer =
            |kind: SnapKind, point: DVec3, object: Option<ObjectId>, reference: bool| {
                if !switches.enabled(kind) || !point.is_finite() {
                    return;
                }
                // Main switches: geometry points need «Геометрия», reference ones
                // (member axes, grid lines, plate corners) need «Опорные».
                if (reference && !switches.reference) || (!reference && !switches.geometry) {
                    return;
                }
                let Some(px) = pixels(point) else { return };
                if px > tolerance {
                    return;
                }
                found.push(Hit {
                    kind,
                    reference,
                    point,
                    object,
                    pixels: px,
                });
            };
        // Reference lines (member axes, grid lines) and geometry edges.
        let mut lines: Vec<Segment> = Vec::new();
        // Grid lines exist at every level (Tekla grids are drawn per elevation).
        for axis in project.axes.values() {
            let (a, b) = (
                DVec3::from_array(axis.start.0),
                DVec3::from_array(axis.end.0),
            );
            let mut shifts = vec![0.];
            shifts.extend(
                project
                    .levels
                    .values()
                    .map(|l| l.elevation_mm - a.z)
                    .filter(|dz| dz.abs() > 1e-6),
            );
            for dz in shifts {
                let (a, b) = (a + DVec3::Z * dz, b + DVec3::Z * dz);
                offer(SnapKind::Reference, a, Some(axis.id), true);
                offer(SnapKind::Reference, b, Some(axis.id), true);
                lines.push((a, b, Some(axis.id), true));
            }
        }
        for index in &near {
            let instance = &scene.instances[*index];
            let id = instance.id;
            let frame = instance.frame;
            let world = |p: DVec3| DVec3::from_array(frame.world(Point3(p.to_array())).0);
            if let Some(element) = project.elements.get(&id) {
                match &element.shape {
                    ElementShape::Member { length, .. } => {
                        let (a, b) = (
                            world(DVec3::ZERO),
                            world(DVec3::new(0., 0., length.value())),
                        );
                        offer(SnapKind::Reference, a, Some(id), true);
                        offer(SnapKind::Reference, b, Some(id), true);
                        offer(SnapKind::MidPoint, (a + b) * 0.5, Some(id), true);
                        lines.push((a, b, Some(id), true));
                    }
                    ElementShape::Plate { contour, .. } => {
                        let points: Vec<DVec3> = contour
                            .iter()
                            .map(|p| world(DVec3::new(p[0], p[1], 0.)))
                            .collect();
                        for (k, p) in points.iter().enumerate() {
                            offer(SnapKind::Reference, *p, Some(id), true);
                            lines.push((*p, points[(k + 1) % points.len()], Some(id), true));
                        }
                    }
                }
            }
            let mesh = instance.mesh;
            let edges = self
                .edges
                .entry(mesh)
                .or_insert_with(|| Arc::new(feature_edges(&scene.meshes[mesh])))
                .clone();
            for (a, b) in edges.iter() {
                let (a, b) = (world(*a), world(*b));
                // Only edges passing near the cursor matter.
                let near_point = closest_on_segment_to_ray(a, b, ray);
                if pixels(near_point).is_none_or(|px| px > tolerance) {
                    continue;
                }
                offer(SnapKind::EndPoint, a, Some(id), false);
                offer(SnapKind::EndPoint, b, Some(id), false);
                offer(SnapKind::MidPoint, (a + b) * 0.5, Some(id), false);
                lines.push((a, b, Some(id), false));
            }
        }
        // Keep lines that pass within tolerance of the cursor.
        lines.retain(|(a, b, _, _)| {
            pixels(closest_on_segment_to_ray(*a, *b, ray)).is_some_and(|px| px <= tolerance)
        });
        lines.truncate(256);
        for (a, b, id, reference) in &lines {
            offer(
                SnapKind::Nearest,
                closest_on_segment_to_ray(*a, *b, ray),
                *id,
                *reference,
            );
            if let Some(p) = previous {
                let d = *b - *a;
                let t = (p - *a).dot(d) / d.dot(d).max(1e-12);
                if (0. ..=1.).contains(&t) {
                    offer(SnapKind::Perpendicular, *a + d * t, *id, *reference);
                }
            }
        }
        if switches.intersection {
            // An intersection near the cursor needs both lines near the cursor.
            let references: Vec<&Segment> = lines
                .iter()
                .filter(|l| {
                    l.3 && pixels(closest_on_segment_to_ray(l.0, l.1, ray))
                        .is_some_and(|px| px <= tolerance)
                })
                .take(128)
                .collect();
            for (i, x) in references.iter().enumerate() {
                for y in references.iter().skip(i + 1) {
                    let scale = (x.1 - x.0).length().max((y.1 - y.0).length()).max(1.);
                    if let Some(p) = intersection((x.0, x.1), (y.0, y.1), 1e-9 * scale) {
                        offer(SnapKind::Intersection, p, x.2, true);
                    }
                }
            }
        }
        // Ranking: priority class, then reference before solid geometry, then pixels.
        found.sort_by(|a, b| {
            (a.kind.priority(), !a.reference, a.pixels)
                .partial_cmp(&(b.kind.priority(), !b.reference, b.pixels))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        // Distinct points (1 µm grid), best first; Tab cycles through at most
        // `MAX_CANDIDATES`. Dense areas offer thousands of points, so this must
        // stay linear.
        let mut seen = std::collections::HashSet::new();
        let mut distinct: Vec<Hit> = Vec::new();
        for hit in found {
            let key = (hit.point * 1e3).round().to_array().map(|v| v as i64);
            if seen.insert(key) {
                distinct.push(hit);
                if distinct.len() == MAX_CANDIDATES {
                    break;
                }
            }
        }
        if !distinct.is_empty() {
            return distinct;
        }
        let Some(free) = workplane.intersect(ray.origin, ray.direction) else {
            return Vec::new();
        };
        let point = match (switches.ortho, previous) {
            (true, Some(p)) => ortho(p, free, workplane),
            _ => free,
        };
        vec![Hit {
            kind: SnapKind::Free,
            reference: false,
            point,
            object: None,
            pixels: 0.,
        }]
    }
}
