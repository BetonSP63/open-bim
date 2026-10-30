//! Faces of a piece as flat regions. Every wall of the section (web,
//! flanges, tube walls, the plate itself) is a slab with plane coordinates
//! `u` along the piece and `w` across it, and `c` through its thickness.
//! Plane cuts bound the region by straight lines, through cuts (boxes and
//! prisms) take polygons out of it; the boundary of what is left gives the
//! outer contour (`AK`, counter-clockwise) and the inner ones (`IK`,
//! clockwise) of the face.
use crate::Face;
use open_bim_model::DetailKind;

pub(crate) type P = [f64; 2];

/// A wall of the piece in part-local coordinates: `p = origin + u·U + w·W + c·C`.
#[derive(Clone, Debug)]
pub(crate) struct Wall {
    pub face: Face,
    pub origin: [f64; 3],
    pub u: [f64; 3],
    pub w: [f64; 3],
    pub c: [f64; 3],
    /// Thickness range along `C`.
    pub through: (f64, f64),
    /// Region of the uncut wall in (u, w).
    pub base: Vec<P>,
}

/// What a detail does to one wall.
pub(crate) enum Effect {
    None,
    /// Material kept where `a·(u, w) ≤ k`.
    Half([f64; 2], f64),
    /// Material taken out inside the polygons.
    Remove(Vec<Vec<P>>),
    /// The whole wall is gone.
    Gone,
    /// Cannot be described on this face.
    Unsupported(&'static str),
}

const EPS: f64 = 1e-6;

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn add(a: [f64; 3], b: [f64; 3], k: f64) -> [f64; 3] {
    [a[0] + b[0] * k, a[1] + b[1] * k, a[2] + b[2] * k]
}

impl Wall {
    /// Wall coordinates (u, w, c) of a part-local point.
    pub fn project(&self, p: [f64; 3]) -> [f64; 3] {
        let d = add(p, self.origin, -1.0);
        [dot(d, self.u), dot(d, self.w), dot(d, self.c)]
    }

    fn mid(&self) -> f64 {
        (self.through.0 + self.through.1) / 2.0
    }

    /// Effect of a cut detail on this wall.
    pub fn effect(&self, kind: &DetailKind) -> Effect {
        match kind {
            DetailKind::PlaneCut { normal, offset, .. } => {
                let (a_u, a_w, a_c) = (
                    dot(*normal, self.u),
                    dot(*normal, self.w),
                    dot(*normal, self.c),
                );
                let k = offset - dot(*normal, self.origin);
                if a_u.abs() < EPS && a_w.abs() < EPS {
                    // Parallel to the wall: all of it is kept, gone or cut in thickness.
                    let lo = a_c * self.through.0;
                    let hi = a_c * self.through.1;
                    if lo.max(hi) <= k + 1e-6 {
                        Effect::None
                    } else if lo.min(hi) >= k - 1e-6 {
                        Effect::Gone
                    } else {
                        Effect::Unsupported("срез по толщине стенки не передан")
                    }
                } else {
                    Effect::Half([a_u, a_w], k - a_c * self.mid())
                }
            }
            DetailKind::BoxCut { origin, size, .. } => {
                let mut lo = [f64::MAX; 3];
                let mut hi = [f64::MIN; 3];
                for i in 0..8 {
                    let p = std::array::from_fn(|k| {
                        origin.0[k] + if i >> k & 1 == 1 { size[k] } else { 0.0 }
                    });
                    let q = self.project(p);
                    for k in 0..3 {
                        lo[k] = lo[k].min(q[k]);
                        hi[k] = hi[k].max(q[k]);
                    }
                }
                match self.coverage(lo[2], hi[2]) {
                    Some(true) => Effect::Remove(vec![vec![
                        [lo[0], lo[1]],
                        [hi[0], lo[1]],
                        [hi[0], hi[1]],
                        [lo[0], hi[1]],
                    ]]),
                    Some(false) => Effect::None,
                    None => Effect::Unsupported("вырез на часть толщины стенки не передан"),
                }
            }
            DetailKind::PrismCut {
                frame,
                contour,
                length,
                ..
            } => {
                let [x, y, z] = frame.axes;
                let o = frame.origin.0;
                let along = dot(z, self.c);
                if (along.abs() - 1.0).abs() < 1e-6 {
                    let c0 = self.project(o)[2];
                    let c1 = c0 + along * length;
                    match self.coverage(c0.min(c1), c0.max(c1)) {
                        Some(true) => Effect::Remove(vec![
                            contour
                                .iter()
                                .map(|p| {
                                    let q = self.project(add(add(o, x, p[0]), y, p[1]));
                                    [q[0], q[1]]
                                })
                                .collect(),
                        ]),
                        Some(false) => Effect::None,
                        None => Effect::Unsupported("вырез на часть толщины стенки не передан"),
                    }
                } else if along.abs() < 1e-6 {
                    // Axis in the wall plane: the contour crosses the wall in
                    // intervals that the extrusion sweeps into rectangles.
                    let at = |c: f64| self.prism_slice(frame, contour, *length, c);
                    let (from, to) = self.through;
                    let span = (to - from) * 1e-3;
                    let (a, m, b) = (at(from + span), at(self.mid()), at(to - span));
                    let same = |p: &Vec<Vec<P>>, q: &Vec<Vec<P>>| {
                        p.len() == q.len()
                            && p.iter().flatten().zip(q.iter().flatten()).all(|(s, t)| {
                                (s[0] - t[0]).abs() < 0.01 && (s[1] - t[1]).abs() < 0.01
                            })
                    };
                    if m.is_empty() && a.is_empty() && b.is_empty() {
                        Effect::None
                    } else if same(&a, &m) && same(&m, &b) {
                        Effect::Remove(m)
                    } else {
                        Effect::Unsupported("вырез под углом к стенке не передан")
                    }
                } else {
                    Effect::Unsupported("вырез под углом к стенке не передан")
                }
            }
            _ => Effect::None,
        }
    }

    /// `Some(true)` when `[lo, hi]` covers the thickness, `Some(false)` when it
    /// misses it, `None` when it goes part way.
    fn coverage(&self, lo: f64, hi: f64) -> Option<bool> {
        let (from, to) = self.through;
        if hi <= from + 1e-6 || lo >= to - 1e-6 {
            Some(false)
        } else if lo <= from + 1e-6 && hi >= to - 1e-6 {
            Some(true)
        } else {
            None
        }
    }

    /// Rectangles in (u, w) where a prism with its axis in the wall plane
    /// crosses the level `c`.
    fn prism_slice(
        &self,
        frame: &open_bim_model::Frame,
        contour: &[P],
        length: f64,
        c: f64,
    ) -> Vec<Vec<P>> {
        let [x, y, z] = frame.axes;
        let o = frame.origin.0;
        // Level line in contour coordinates: (cx, cy)·(px, py) = level.
        let (cx, cy) = (dot(x, self.c), dot(y, self.c));
        let norm = (cx * cx + cy * cy).sqrt();
        if norm < 1e-9 {
            return Vec::new();
        }
        let level = (c - self.project(o)[2]) / norm;
        let (nx, ny) = (cx / norm, cy / norm);
        let (dx, dy) = (-ny, nx);
        let base = [nx * level, ny * level];
        let mut ts = Vec::new();
        for i in 0..contour.len() {
            let (a, b) = (contour[i], contour[(i + 1) % contour.len()]);
            let (sa, sb) = (a[0] * nx + a[1] * ny - level, b[0] * nx + b[1] * ny - level);
            if (sa < 0.0) != (sb < 0.0) {
                let t = sa / (sa - sb);
                let p = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
                ts.push((p[0] - base[0]) * dx + (p[1] - base[1]) * dy);
            }
        }
        ts.sort_by(f64::total_cmp);
        let point = |t: f64, s: f64| {
            let q = add(add(add(o, x, base[0] + dx * t), y, base[1] + dy * t), z, s);
            let q = self.project(q);
            [q[0], q[1]]
        };
        ts.as_chunks::<2>()
            .0
            .iter()
            .filter(|pair| pair[1] - pair[0] > 1e-6)
            .map(|pair| {
                vec![
                    point(pair[0], 0.0),
                    point(pair[1], 0.0),
                    point(pair[1], length),
                    point(pair[0], length),
                ]
            })
            .collect()
    }
}

/// Region of a wall after the effects: closed loops in (u, w), outer ones
/// counter-clockwise and inner ones clockwise, without the repeated point.
pub(crate) fn region(base: &[P], halves: &[([f64; 2], f64)], removed: &[Vec<P>]) -> Vec<Vec<P>> {
    if halves.is_empty() && removed.is_empty() {
        return vec![base.to_vec()];
    }
    let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
    for p in base {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let span = (hi[0] - lo[0]).max(hi[1] - lo[1]) + 1.0;
    let inside = |p: P| {
        contains(base, p)
            && halves.iter().all(|(a, k)| a[0] * p[0] + a[1] * p[1] <= *k)
            && !removed.iter().any(|poly| contains(poly, p))
    };
    // Every boundary candidate as a segment.
    let mut segments: Vec<(P, P)> = Vec::new();
    let mut ring = |poly: &[P]| {
        for i in 0..poly.len() {
            segments.push((poly[i], poly[(i + 1) % poly.len()]));
        }
    };
    ring(base);
    for poly in removed {
        ring(poly);
    }
    for (a, k) in halves {
        let n = (a[0] * a[0] + a[1] * a[1]).sqrt();
        let (nx, ny, d) = (a[0] / n, a[1] / n, k / n);
        let centre = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
        let foot = centre[0] * nx + centre[1] * ny - d;
        let p = [centre[0] - nx * foot, centre[1] - ny * foot];
        let r = 2.0 * span;
        segments.push((
            [p[0] + ny * r, p[1] - nx * r],
            [p[0] - ny * r, p[1] + nx * r],
        ));
    }
    segments.retain(|(a, b)| (a[0] - b[0]).hypot(a[1] - b[1]) > 1e-9);
    // Split every segment at the points where others meet it.
    let mut pieces: Vec<(P, P)> = Vec::new();
    for (i, &(a, b)) in segments.iter().enumerate() {
        let d = [b[0] - a[0], b[1] - a[1]];
        let len2 = d[0] * d[0] + d[1] * d[1];
        let mut ts = vec![0.0, 1.0];
        for (j, &(c, e)) in segments.iter().enumerate() {
            if i == j {
                continue;
            }
            for p in [c, e] {
                let t = ((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / len2;
                let q = [a[0] + d[0] * t, a[1] + d[1] * t];
                if (0.0..=1.0).contains(&t) && (q[0] - p[0]).hypot(q[1] - p[1]) < 1e-7 {
                    ts.push(t);
                }
            }
            let f = [e[0] - c[0], e[1] - c[1]];
            let den = d[0] * f[1] - d[1] * f[0];
            if den.abs() > 1e-12 {
                let t = ((c[0] - a[0]) * f[1] - (c[1] - a[1]) * f[0]) / den;
                let s = ((c[0] - a[0]) * d[1] - (c[1] - a[1]) * d[0]) / den;
                if (-1e-12..=1.0 + 1e-12).contains(&t) && (-1e-12..=1.0 + 1e-12).contains(&s) {
                    ts.push(t.clamp(0.0, 1.0));
                }
            }
        }
        ts.sort_by(f64::total_cmp);
        let at = |t: f64| snap([a[0] + d[0] * t, a[1] + d[1] * t]);
        for pair in ts.windows(2) {
            let (p, q) = (at(pair[0]), at(pair[1]));
            if p != q {
                pieces.push((p, q));
            }
        }
    }
    // Keep the pieces with material on exactly one side, material on the left.
    let mut edges: Vec<(P, P)> = Vec::new();
    for (p, q) in pieces {
        let m = [(p[0] + q[0]) / 2.0, (p[1] + q[1]) / 2.0];
        let len = (q[0] - p[0]).hypot(q[1] - p[1]);
        let n = [-(q[1] - p[1]) / len * 1e-3, (q[0] - p[0]) / len * 1e-3];
        let left = inside([m[0] + n[0], m[1] + n[1]]);
        let right = inside([m[0] - n[0], m[1] - n[1]]);
        let edge = match (left, right) {
            (true, false) => (p, q),
            (false, true) => (q, p),
            _ => continue,
        };
        if !edges.contains(&edge) {
            edges.push(edge);
        }
    }
    // Chain the edges into loops.
    let mut used = vec![false; edges.len()];
    let mut loops = Vec::new();
    for first in 0..edges.len() {
        if used[first] {
            continue;
        }
        used[first] = true;
        let mut points = vec![edges[first].0];
        let mut at = edges[first].1;
        let mut incoming = edges[first];
        while at != points[0] {
            points.push(at);
            // At a shared corner keep turning left, so touching loops stay apart.
            let next = (0..edges.len())
                .filter(|&k| !used[k] && edges[k].0 == at)
                .max_by(|&k, &l| turn(incoming, edges[k]).total_cmp(&turn(incoming, edges[l])));
            let Some(k) = next else { break };
            used[k] = true;
            incoming = edges[k];
            at = edges[k].1;
        }
        let points = simplify(points);
        if points.len() >= 3 {
            loops.push(points);
        }
    }
    loops.sort_by(|a, b| area(b).total_cmp(&area(a)));
    loops
}

/// Signed turn from edge `a` to edge `b`, larger for a sharper left turn.
fn turn(a: (P, P), b: (P, P)) -> f64 {
    let d = [a.1[0] - a.0[0], a.1[1] - a.0[1]];
    let e = [b.1[0] - b.0[0], b.1[1] - b.0[1]];
    (d[0] * e[1] - d[1] * e[0]).atan2(d[0] * e[0] + d[1] * e[1])
}

fn snap(p: P) -> P {
    p.map(|v| {
        let r = (v * 1e5).round() / 1e5;
        if r == 0.0 { 0.0 } else { r }
    })
}

/// Drop points on straight runs.
fn simplify(mut points: Vec<P>) -> Vec<P> {
    let mut changed = true;
    while changed && points.len() > 3 {
        changed = false;
        for i in 0..points.len() {
            let n = points.len();
            let (a, b, c) = (points[(i + n - 1) % n], points[i], points[(i + 1) % n]);
            let cross = (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]);
            let len = (c[0] - a[0]).hypot(c[1] - a[1]);
            if cross.abs() <= 1e-6 * len.max(1.0) {
                points.remove(i);
                changed = true;
                break;
            }
        }
    }
    points
}

/// Signed area, positive counter-clockwise.
pub(crate) fn area(points: &[P]) -> f64 {
    (0..points.len())
        .map(|i| {
            let (a, b) = (points[i], points[(i + 1) % points.len()]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        / 2.0
}

/// Even-odd point in polygon.
pub(crate) fn contains(poly: &[P], p: P) -> bool {
    let mut inside = false;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
            if p[0] < x {
                inside = !inside;
            }
        }
    }
    inside
}

/// Point inside the region given by its loops.
pub(crate) fn in_region(loops: &[Vec<P>], p: P) -> bool {
    loops.iter().filter(|l| contains(l, p)).count() % 2 == 1
}
