//! Keyboard cursor: without a mouse, Alt+arrows move a crosshair to the next
//! snap point (pick commands) or the next object (selection) in the arrow's
//! screen direction; Space picks or selects there.
use open_bim_model::{ElementShape, ObjectId, Point3, Project};
use open_bim_renderer::DVec3;
use open_bim_renderer::overlay::Projector;

fn v(p: Point3) -> DVec3 {
    DVec3::from_array(p.0)
}

/// Points a keyboard pick can land on: grid nodes at every level, member
/// ends and midpoints, plate vertices.
pub fn snap_points(project: &Project) -> Vec<DVec3> {
    let mut out = Vec::new();
    let axes: Vec<_> = project.axes.values().collect();
    let mut levels: Vec<f64> = project.levels.values().map(|l| l.elevation_mm).collect();
    if levels.is_empty() {
        levels.push(axes.first().map_or(0., |a| a.start.0[2]));
    }
    for (i, a) in axes.iter().enumerate() {
        for b in &axes[i + 1..] {
            let (p, d1) = (v(a.start), v(a.end) - v(a.start));
            let (q, d2) = (v(b.start), v(b.end) - v(b.start));
            let denom = d1.x * d2.y - d1.y * d2.x;
            if denom.abs() < 1e-9 {
                continue;
            }
            let t = ((q.x - p.x) * d2.y - (q.y - p.y) * d2.x) / denom;
            let u = ((q.x - p.x) * d1.y - (q.y - p.y) * d1.x) / denom;
            if (-1e-9..=1. + 1e-9).contains(&t) && (-1e-9..=1. + 1e-9).contains(&u) {
                let node = p + d1 * t;
                for z in &levels {
                    out.push(DVec3::new(node.x, node.y, *z));
                }
            }
        }
    }
    for e in project.elements.values() {
        let frame = e.shape.frame();
        match &e.shape {
            ElementShape::Member { length, .. } => {
                let (a, b) = (
                    v(frame.origin),
                    v(frame.world(Point3([0., 0., length.value()]))),
                );
                out.extend([a, b, (a + b) * 0.5]);
            }
            ElementShape::Plate { contour, .. } => {
                out.extend(
                    contour
                        .iter()
                        .map(|c| v(frame.world(Point3([c[0], c[1], 0.])))),
                );
            }
        }
    }
    out
}

/// One point per selectable part: its centre.
pub fn object_points(project: &Project) -> Vec<(ObjectId, DVec3)> {
    project
        .elements
        .values()
        .map(|e| {
            let frame = e.shape.frame();
            let centre = match &e.shape {
                ElementShape::Member { length, .. } => {
                    v(frame.world(Point3([0., 0., length.value() / 2.])))
                }
                ElementShape::Plate { contour, .. } => {
                    let n = contour.len().max(1) as f64;
                    let c = contour
                        .iter()
                        .fold([0., 0.], |s, p| [s[0] + p[0], s[1] + p[1]]);
                    v(frame.world(Point3([c[0] / n, c[1] / n, 0.])))
                }
            };
            (e.id, centre)
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arrow {
    Left,
    Right,
    Up,
    Down,
}

impl Arrow {
    fn screen(self) -> [f64; 2] {
        match self {
            Self::Left => [-1., 0.],
            Self::Right => [1., 0.],
            Self::Up => [0., -1.],
            Self::Down => [0., 1.],
        }
    }
}

/// Index of the next point from `from` in the arrow direction: inside a 45°
/// cone around it, preferring the same level, small angular deviation,
/// then distance.
pub fn step(p: &Projector, points: &[DVec3], from: DVec3, arrow: Arrow) -> Option<usize> {
    let origin = p.screen(from)?;
    let dir = arrow.screen();
    points
        .iter()
        .enumerate()
        .filter_map(|(i, w)| {
            let s = p.screen(*w)?;
            let d = [s[0] - origin[0], s[1] - origin[1]];
            let len = d[0].hypot(d[1]);
            if len < 2. {
                return None;
            }
            let along = (d[0] * dir[0] + d[1] * dir[1]) / len;
            // cos 45° cone; score = distance weighted by off-axis deviation,
            // doubled for points off the current level so the cursor stays at
            // one elevation unless only other levels lie that way.
            let level = if (w.z - from.z).abs() > 1. { 2. } else { 1. };
            (along >= std::f64::consts::FRAC_1_SQRT_2).then_some((i, len * (2. - along) * level))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}
