//! Screen-space snapping to model geometry («Привязки»). Candidates come
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
    /// Centres of circles, bolts and holes.
    Center,
    MidPoint,
    Intersection,
    Perpendicular,
    /// Parallel to a line the pointer passed over, through the previous pick.
    Parallel,
    /// A point on the extension of a line beyond its ends.
    Extension,
    /// Any point on a reference line (grid, part axis, construction line).
    Line,
    Nearest,
    Free,
}

impl SnapKind {
    /// Snap priority class (lower wins; ties go to the nearer point).
    /// Reference points and grid/reference-line intersections («Points»)
    /// share the highest class, so the closer of the two is taken.
    pub fn priority(self) -> u8 {
        match self {
            Self::Reference | Self::Intersection => 0,
            Self::EndPoint | Self::Center => 1,
            Self::MidPoint => 2,
            Self::Perpendicular => 3,
            Self::Parallel | Self::Extension => 4,
            Self::Line | Self::Nearest => 5,
            Self::Free => 6,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Reference => "Опорная точка",
            Self::EndPoint => "Конечная точка",
            Self::Center => "Центр",
            Self::MidPoint => "Середина",
            Self::Intersection => "Пересечение",
            Self::Perpendicular => "Перпендикуляр",
            Self::Parallel => "Параллельно",
            Self::Extension => "Продолжение линии",
            Self::Line => "Линия",
            Self::Nearest => "Ближайшая точка",
            Self::Free => "Свободная точка",
        }
    }

    /// Every kind, in the order of the snap toolbar and the override menu.
    pub const ALL: [SnapKind; 11] = [
        Self::Reference,
        Self::EndPoint,
        Self::Center,
        Self::MidPoint,
        Self::Intersection,
        Self::Perpendicular,
        Self::Parallel,
        Self::Extension,
        Self::Line,
        Self::Nearest,
        Self::Free,
    ];
}

/// Snap switches (bottom toolbar). Defaults: reference and
/// geometry points, end points, centres, mid points and intersections on;
/// perpendicular, parallel, line extensions, lines and nearest off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Switches {
    /// Main switch «Reference lines and points».
    pub reference: bool,
    /// Main switch «Geometry lines and points»: solid corners and edges.
    pub geometry: bool,
    /// Reference points themselves (member ends, plate corners, grid ends);
    /// only the snap override turns them off.
    pub points: bool,
    pub end: bool,
    pub center: bool,
    pub mid: bool,
    pub intersection: bool,
    pub perpendicular: bool,
    pub parallel: bool,
    pub extension: bool,
    pub line: bool,
    pub nearest: bool,
    pub ortho: bool,
    /// Snap depth «Плоскость»: snapped points are carried along the line of
    /// sight onto the snap plane; «Авто» (false) keeps them where they are.
    pub depth_plane: bool,
    /// Snap plane «Плоскость вида» (the screen plane through the work plane
    /// origin) instead of «Рабочая плоскость».
    pub view_plane: bool,
}

impl Default for Switches {
    fn default() -> Self {
        Self {
            reference: true,
            geometry: true,
            points: true,
            end: true,
            center: true,
            mid: true,
            intersection: true,
            perpendicular: false,
            parallel: false,
            extension: false,
            line: false,
            nearest: false,
            ortho: false,
            depth_plane: false,
            view_plane: false,
        }
    }
}

impl Switches {
    pub fn enabled(&self, kind: SnapKind) -> bool {
        match kind {
            SnapKind::Reference => self.reference && self.points,
            SnapKind::EndPoint => self.end,
            SnapKind::Center => self.center,
            SnapKind::MidPoint => self.mid,
            SnapKind::Intersection => self.intersection,
            SnapKind::Perpendicular => self.perpendicular,
            SnapKind::Parallel => self.parallel,
            SnapKind::Extension => self.extension,
            SnapKind::Line => self.line,
            SnapKind::Nearest => self.nearest,
            SnapKind::Free => true,
        }
    }

    pub fn flag(&mut self, kind: SnapKind) -> Option<&mut bool> {
        Some(match kind {
            SnapKind::Reference => &mut self.points,
            SnapKind::EndPoint => &mut self.end,
            SnapKind::Center => &mut self.center,
            SnapKind::MidPoint => &mut self.mid,
            SnapKind::Intersection => &mut self.intersection,
            SnapKind::Perpendicular => &mut self.perpendicular,
            SnapKind::Parallel => &mut self.parallel,
            SnapKind::Extension => &mut self.extension,
            SnapKind::Line => &mut self.line,
            SnapKind::Nearest => &mut self.nearest,
            SnapKind::Free => return None,
        })
    }

    /// Every point switch off (the toolbar «Привязки» button off); ortho kept.
    pub fn none(self) -> Self {
        Self {
            reference: false,
            geometry: false,
            points: false,
            end: false,
            center: false,
            mid: false,
            intersection: false,
            perpendicular: false,
            parallel: false,
            extension: false,
            line: false,
            nearest: false,
            ..self
        }
    }

    /// Right-click «Переопределение привязки»: only `kind` for the next
    /// pick, whatever the toolbar says. `Free` means any position.
    pub fn only(self, kind: SnapKind) -> Self {
        let mut out = Self {
            reference: true,
            geometry: true,
            ..self.none()
        };
        match out.flag(kind) {
            Some(flag) => *flag = true,
            None => out = self.none(),
        }
        out
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
    /// Guide line drawn with the hit (extension, parallel direction).
    pub guide: Option<(DVec3, DVec3)>,
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

/// Lines the pointer passed over, kept for parallel and extension snaps.
pub const MAX_ACQUIRED: usize = 4;

/// Feature edges per display mesh, cached across queries.
#[derive(Default)]
pub struct Snapper {
    edges: HashMap<usize, Arc<Vec<(DVec3, DVec3)>>>,
    scene: Option<usize>,
    /// Lines the pointer recently passed over, newest first (kept
    /// for «Параллельно» and «Продолжение линии»).
    pub acquired: Vec<(DVec3, DVec3, Option<ObjectId>)>,
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

/// Parameter along `a + (b - a) t` of the point nearest to the ray (unclamped).
fn line_parameter_to_ray(a: DVec3, b: DVec3, ray: Ray) -> f64 {
    let d = b - a;
    let r = ray.direction;
    let w = a - ray.origin;
    let (aa, bb, cc, dd, ee) = (d.dot(d), d.dot(r), r.dot(r), d.dot(w), r.dot(w));
    let denominator = aa * cc - bb * bb;
    if denominator.abs() < 1e-12 {
        0.
    } else {
        (bb * ee - cc * dd) / denominator
    }
}

fn closest_on_segment_to_ray(a: DVec3, b: DVec3, ray: Ray) -> DVec3 {
    a + (b - a) * line_parameter_to_ray(a, b, ray).clamp(0., 1.)
}

/// Intersection of an infinite line with a segment (closest points closer than `eps`).
fn line_segment_intersection(line: (DVec3, DVec3), b: (DVec3, DVec3), eps: f64) -> Option<DVec3> {
    let (d1, d2, r) = (line.1 - line.0, b.1 - b.0, line.0 - b.0);
    let (aa, ee, f) = (d1.dot(d1), d2.dot(d2), d2.dot(r));
    let (c, bb) = (d1.dot(r), d1.dot(d2));
    let denominator = aa * ee - bb * bb;
    if denominator.abs() < 1e-9 * aa.max(1.) * ee.max(1.) {
        return None;
    }
    let s = (bb * f - c * ee) / denominator;
    let t = (aa * f - bb * c) / denominator;
    if !(-1e-9..=1. + 1e-9).contains(&t) {
        return None;
    }
    let (p, q) = (line.0 + d1 * s, b.0 + d2 * t);
    (p.distance(q) <= eps).then_some((p + q) * 0.5)
}

/// Reference line of a member (picked points, where handles sit). With a
/// part position it differs from the body axis the frame stores; the body
/// axis is the fallback when the profile is unknown.
pub fn member_line(project: &Project, shape: &ElementShape) -> Option<(DVec3, DVec3)> {
    let ElementShape::Member { length, frame, .. } = shape else {
        return None;
    };
    Some(match project.member_reference_line(shape) {
        Some((a, b, _)) => (DVec3::from_array(a.0), DVec3::from_array(b.0)),
        None => (
            DVec3::from_array(frame.origin.0),
            DVec3::from_array(frame.world(Point3([0., 0., length.value()])).0),
        ),
    })
}

/// Bolt centres («Привязка к центрам»); bolt frames are world coordinates.
fn detail_centres(project: &Project) -> Vec<(ObjectId, DVec3)> {
    use open_bim_model::DetailKind;
    let mut out = Vec::new();
    for detail in project.details.values() {
        let at = |frame: &open_bim_model::Frame, x: f64, y: f64| {
            DVec3::from_array(frame.world(Point3([x, y, 0.])).0)
        };
        if let DetailKind::BoltGroup { frame, .. } = &detail.kind {
            for [x, y] in detail.kind.bolt_positions() {
                out.push((detail.id, at(frame, x, y)));
            }
        }
    }
    out
}

/// Segments of construction objects (circles as 48-sided polygons).
pub fn construction_segments(project: &Project) -> Vec<(ObjectId, (DVec3, DVec3))> {
    project
        .construction
        .values()
        .flat_map(|c| {
            c.segments()
                .into_iter()
                .map(move |(a, b)| (c.id, (DVec3::from_array(a), DVec3::from_array(b))))
        })
        .collect()
}

/// Snap points of construction objects: line ends and midpoints, circle
/// centres and quadrant points.
fn construction_marks(project: &Project) -> Vec<(ObjectId, DVec3, SnapKind)> {
    use open_bim_model::ConstructionKind;
    let mut out = Vec::new();
    for c in project.construction.values() {
        match &c.kind {
            ConstructionKind::Line { a, b } => {
                let (a, b) = (DVec3::from_array(a.0), DVec3::from_array(b.0));
                out.push((c.id, a, SnapKind::Reference));
                out.push((c.id, b, SnapKind::Reference));
                out.push((c.id, (a + b) * 0.5, SnapKind::MidPoint));
            }
            ConstructionKind::Circle { center, .. } => {
                out.push((c.id, DVec3::from_array(center.0), SnapKind::Center));
                let segments = c.segments();
                for k in [0, 12, 24, 36] {
                    out.push((c.id, DVec3::from_array(segments[k].0), SnapKind::Reference));
                }
            }
        }
    }
    out
}

/// Every snap point construction objects add: their own points and their
/// crossings with grid lines (at every level), member axes and each other.
pub fn construction_points(project: &Project) -> Vec<DVec3> {
    let own = construction_segments(project);
    let mut others: Vec<(DVec3, DVec3)> = own.iter().map(|(_, s)| *s).collect();
    for axis in project.axes.values() {
        let (a, b) = (
            DVec3::from_array(axis.start.0),
            DVec3::from_array(axis.end.0),
        );
        let mut shifts = vec![0.];
        shifts.extend(project.levels.values().map(|l| l.elevation_mm - a.z));
        for dz in shifts {
            others.push((a + DVec3::Z * dz, b + DVec3::Z * dz));
        }
    }
    for e in project.elements.values() {
        if let Some(line) = member_line(project, &e.shape) {
            others.push(line);
        }
    }
    let mut out: Vec<DVec3> = construction_marks(project)
        .into_iter()
        .map(|m| m.1)
        .collect();
    for (_, x) in &own {
        for y in &others {
            if (x.0 - y.0).length() < 1e-9 && (x.1 - y.1).length() < 1e-9 {
                continue;
            }
            let scale = (x.1 - x.0).length().max((y.1 - y.0).length()).max(1.);
            if let Some(p) = intersection(*x, *y, 1e-9 * scale) {
                out.push(p);
            }
        }
    }
    out
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

/// The plane free picks land on: the work plane, or with «Плоскость вида»
/// the screen plane through the work plane origin.
pub fn snap_plane(view: &View, workplane: &Workplane, view_plane: bool) -> Workplane {
    if !view_plane {
        return *workplane;
    }
    view.ray([view.size[0] / 2., view.size[1] / 2.])
        .and_then(|r| Workplane::facing(workplane.origin, -r.direction))
        .unwrap_or(*workplane)
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

    /// All distinct snap points under the cursor, best first (Tab /
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
        let mut offer = |kind: SnapKind,
                         point: DVec3,
                         object: Option<ObjectId>,
                         reference: bool,
                         guide: Option<(DVec3, DVec3)>| {
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
                guide,
            });
        };
        // Reference lines (member axes, grid lines) and geometry edges.
        let mut lines: Vec<Segment> = Vec::new();
        // Long straight reference lines whose extensions snap anywhere in view.
        let mut extendable: Vec<Segment> = Vec::new();
        // Grid lines exist at every level (grids are drawn per elevation).
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
                offer(SnapKind::Reference, a, Some(axis.id), true, None);
                offer(SnapKind::Reference, b, Some(axis.id), true, None);
                lines.push((a, b, Some(axis.id), true));
                extendable.push((a, b, Some(axis.id), true));
            }
        }
        // Construction lines and circles: reference geometry like grid lines.
        for c in project.construction.values() {
            if let open_bim_model::ConstructionKind::Line { a, b } = &c.kind {
                let (a, b) = (DVec3::from_array(a.0), DVec3::from_array(b.0));
                extendable.push((a, b, Some(c.id), true));
            }
        }
        for (id, (a, b)) in construction_segments(project) {
            lines.push((a, b, Some(id), true));
        }
        for (id, point, kind) in construction_marks(project) {
            offer(kind, point, Some(id), true, None);
        }
        if switches.center {
            for (id, point) in detail_centres(project) {
                offer(SnapKind::Center, point, Some(id), true, None);
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
                        let (a, b) = member_line(project, &element.shape).unwrap_or((
                            world(DVec3::ZERO),
                            world(DVec3::new(0., 0., length.value())),
                        ));
                        offer(SnapKind::Reference, a, Some(id), true, None);
                        offer(SnapKind::Reference, b, Some(id), true, None);
                        offer(SnapKind::MidPoint, (a + b) * 0.5, Some(id), true, None);
                        lines.push((a, b, Some(id), true));
                    }
                    ElementShape::Plate { contour, .. } => {
                        let points: Vec<DVec3> = contour
                            .iter()
                            .map(|p| world(DVec3::new(p[0], p[1], 0.)))
                            .collect();
                        for (k, p) in points.iter().enumerate() {
                            offer(SnapKind::Reference, *p, Some(id), true, None);
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
                offer(SnapKind::EndPoint, a, Some(id), false, None);
                offer(SnapKind::EndPoint, b, Some(id), false, None);
                offer(SnapKind::MidPoint, (a + b) * 0.5, Some(id), false, None);
                lines.push((a, b, Some(id), false));
            }
        }
        // Keep lines that pass within tolerance of the cursor.
        lines.retain(|(a, b, _, _)| {
            pixels(closest_on_segment_to_ray(*a, *b, ray)).is_some_and(|px| px <= tolerance)
        });
        lines.truncate(256);
        for (a, b, id, reference) in &lines {
            let on = closest_on_segment_to_ray(*a, *b, ray);
            offer(SnapKind::Nearest, on, *id, *reference, None);
            if *reference {
                offer(SnapKind::Line, on, *id, true, Some((*a, *b)));
            }
            if let Some(p) = previous {
                let d = *b - *a;
                let t = (p - *a).dot(d) / d.dot(d).max(1e-12);
                if (0. ..=1.).contains(&t) {
                    offer(SnapKind::Perpendicular, *a + d * t, *id, *reference, None);
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
                        offer(SnapKind::Intersection, p, x.2, true, None);
                    }
                }
            }
        }
        // Track the lines the pointer passes over: the nearest one is
        // remembered for parallel and extension snapping.
        if let Some(line) = lines
            .iter()
            .filter(|l| (l.1 - l.0).length() > 1e-6)
            .min_by(|x, y| {
                let d = |l: &Segment| {
                    pixels(closest_on_segment_to_ray(l.0, l.1, ray)).unwrap_or(f64::MAX)
                };
                d(x).partial_cmp(&d(y)).unwrap_or(std::cmp::Ordering::Equal)
            })
        {
            let same = |l: &(DVec3, DVec3, Option<ObjectId>)| {
                (l.0.distance(line.0) < 1e-6 && l.1.distance(line.1) < 1e-6)
                    || (l.0.distance(line.1) < 1e-6 && l.1.distance(line.0) < 1e-6)
            };
            self.acquired.retain(|l| !same(l));
            self.acquired.insert(0, (line.0, line.1, line.2));
            self.acquired.truncate(MAX_ACQUIRED);
        }
        if switches.extension {
            extendable.extend(self.acquired.iter().map(|l| (l.0, l.1, l.2, true)));
            for (a, b, id, reference) in &extendable {
                let t = line_parameter_to_ray(*a, *b, ray);
                // Beyond either end only: inside the segment it is a line snap.
                if (-1e-9..=1. + 1e-9).contains(&t) {
                    continue;
                }
                let point = *a + (*b - *a) * t;
                let end = if t < 0. { *a } else { *b };
                offer(
                    SnapKind::Extension,
                    point,
                    *id,
                    *reference,
                    Some((end, point)),
                );
                // Where the extension meets another line near the cursor.
                if switches.intersection {
                    let scale = (*b - *a).length().max(1.);
                    for other in &lines {
                        if other.2 == *id && other.2.is_some() {
                            continue;
                        }
                        let eps = 1e-9 * scale.max((other.1 - other.0).length());
                        if let Some(p) =
                            line_segment_intersection((*a, *b), (other.0, other.1), eps)
                        {
                            let t = (p - *a).dot(*b - *a) / (*b - *a).length_squared();
                            if !(0. ..=1.).contains(&t) {
                                let end = if t < 0. { *a } else { *b };
                                offer(SnapKind::Intersection, p, *id, true, Some((end, p)));
                            }
                        }
                    }
                }
            }
        }
        if switches.parallel
            && let Some(from) = previous
        {
            for (a, b, id) in &self.acquired {
                let Some(d) = (*b - *a).try_normalize() else {
                    continue;
                };
                // Through the previous pick, along the remembered line.
                let t = line_parameter_to_ray(from, from + d, ray);
                if t.abs() < 1e-6 {
                    continue;
                }
                let point = from + d * t;
                offer(SnapKind::Parallel, point, *id, true, Some((from, point)));
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
        let plane = snap_plane(&view, workplane, switches.view_plane);
        if switches.depth_plane {
            // Same place on screen, depth of the snap plane.
            for hit in &mut distinct {
                if let Some(p) = view
                    .screen(hit.point)
                    .and_then(|s| view.ray(s))
                    .and_then(|r| plane.intersect(r.origin, r.direction))
                {
                    hit.point = p;
                }
            }
        }
        if !distinct.is_empty() {
            return distinct;
        }
        let Some(free) = plane.intersect(ray.origin, ray.direction) else {
            return Vec::new();
        };
        let point = match (switches.ortho, previous) {
            (true, Some(p)) => ortho(p, free, &plane),
            _ => free,
        };
        vec![Hit {
            kind: SnapKind::Free,
            reference: false,
            point,
            object: None,
            pixels: 0.,
            guide: None,
        }]
    }
}

/// Pick locks: Shift holds the direction from the previous pick, and the
/// X, Y, Z keys hold a work plane coordinate at the value it had when the key
/// was pressed.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Locks {
    /// World direction the next point is held on, through `previous`.
    pub direction: Option<DVec3>,
    /// Work plane coordinates x, y, z held fixed.
    pub coordinates: [Option<f64>; 3],
}

impl Locks {
    pub fn any(&self) -> bool {
        self.direction.is_some() || self.coordinates.iter().any(Option::is_some)
    }

    /// Status bar text, e.g. «Фиксация: X, направление».
    pub fn describe(&self) -> Option<String> {
        let mut parts: Vec<&str> = ["X", "Y", "Z"]
            .iter()
            .zip(self.coordinates)
            .filter_map(|(n, c)| c.map(|_| *n))
            .collect();
        if self.direction.is_some() {
            parts.push("направление");
        }
        (!parts.is_empty()).then(|| format!("Фиксация: {}", parts.join(", ")))
    }

    /// X/Y/Z key: lock axis `k` at the coordinate of `at`, or release it.
    pub fn toggle(&mut self, k: usize, at: DVec3, workplane: &Workplane) -> bool {
        let slot = &mut self.coordinates[k];
        *slot = match slot {
            Some(_) => None,
            None => Some(workplane.to_local(at)[k]),
        };
        slot.is_some()
    }

    /// The point a pick at `point` becomes under the locks. A locked direction
    /// projects the point onto the line through `previous`; locked coordinates
    /// replace the work plane coordinates of the result.
    pub fn apply(&self, point: DVec3, previous: Option<DVec3>, workplane: &Workplane) -> DVec3 {
        let mut p = point;
        if let (Some(d), Some(from)) = (self.direction.and_then(|d| d.try_normalize()), previous) {
            p = from + d * (p - from).dot(d);
        }
        if self.coordinates.iter().any(Option::is_some) {
            let mut local = workplane.to_local(p);
            for k in 0..3 {
                if let Some(v) = self.coordinates[k] {
                    local[k] = v;
                }
            }
            p = workplane.to_world(local);
        }
        p
    }
}
