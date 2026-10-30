//! Reinforcing bars: classes of reinforcing steel (ГОСТ 34028-2016,
//! design resistances of СП 63.13330.2018 tables 6.13–6.14), nominal
//! masses, bend diameters and hooks, anchorage and lap lengths
//! (СП 63.13330.2018, 10.3.24–10.3.31), and the generators that place a
//! bar set in its host part and build it again when the host changes.
use serde::{Deserialize, Serialize};

use crate::{
    ElementShape, Frame, ModelError, ObjectId, Point3, ProfileShape, Project, RebarKind, RebarSet,
};

/// Modulus of elasticity of reinforcing steel, MPa (СП 63, 6.2.12).
pub const REBAR_ES: f64 = 200_000.;

/// Bar surface: sets the bond factor η1 and the bend diameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarSurface {
    /// Plain bar (А240).
    Smooth,
    /// Hot-rolled or thermomechanically strengthened ribbed bar.
    Ribbed,
    /// Cold-worked ribbed bar (В500С).
    ColdRibbed,
}

/// A class of reinforcing steel with its resistances, MPa.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RebarClass {
    /// «А500С» (Cyrillic letters).
    pub name: &'static str,
    pub standard: &'static str,
    /// Normative tensile resistance Rs,n.
    pub rs_n: f64,
    /// Design tensile resistance Rs.
    pub rs: f64,
    /// Design compressive resistance Rsc (long-term load).
    pub rsc: f64,
    /// Design resistance of transverse bars Rsw; 0 when not used as such.
    pub rsw: f64,
    pub surface: BarSurface,
    /// Diameter range of the standard, mm.
    pub d_min: f64,
    pub d_max: f64,
}

/// Classes in the order shown in the tools.
pub const REBAR_CLASSES: [RebarClass; 5] = [
    RebarClass {
        name: "А240",
        standard: "ГОСТ 34028-2016",
        rs_n: 240.,
        rs: 210.,
        rsc: 210.,
        rsw: 170.,
        surface: BarSurface::Smooth,
        d_min: 6.,
        d_max: 40.,
    },
    RebarClass {
        name: "А400",
        standard: "ГОСТ 34028-2016",
        rs_n: 400.,
        rs: 350.,
        rsc: 350.,
        rsw: 280.,
        surface: BarSurface::Ribbed,
        d_min: 6.,
        d_max: 40.,
    },
    RebarClass {
        name: "А500С",
        standard: "ГОСТ 34028-2016",
        rs_n: 500.,
        rs: 435.,
        rsc: 435.,
        rsw: 300.,
        surface: BarSurface::Ribbed,
        d_min: 6.,
        d_max: 40.,
    },
    RebarClass {
        name: "В500С",
        standard: "ГОСТ 34028-2016",
        rs_n: 500.,
        rs: 435.,
        rsc: 415.,
        rsw: 300.,
        surface: BarSurface::ColdRibbed,
        d_min: 4.,
        d_max: 16.,
    },
    RebarClass {
        name: "А600",
        standard: "ГОСТ 34028-2016",
        rs_n: 600.,
        rs: 520.,
        rsc: 470.,
        rsw: 0.,
        surface: BarSurface::Ribbed,
        d_min: 10.,
        d_max: 40.,
    },
];

/// Nominal diameters of the bar schedule, mm.
pub const REBAR_DIAMETERS: [f64; 16] = [
    4., 5., 6., 8., 10., 12., 14., 16., 18., 20., 22., 25., 28., 32., 36., 40.,
];

/// A class by name; Latin letters are accepted («A500C», «B500C»).
pub fn rebar_class(name: &str) -> Option<&'static RebarClass> {
    let name: String = name
        .trim()
        .chars()
        .map(|c| match c {
            'A' | 'a' | 'а' => 'А',
            'B' | 'b' | 'в' => 'В',
            'C' | 'c' | 'с' => 'С',
            c => c,
        })
        .collect();
    REBAR_CLASSES.iter().find(|c| c.name == name)
}

/// Nominal cross-section area of a bar, mm².
pub fn bar_area(d: f64) -> f64 {
    std::f64::consts::PI * d * d / 4.
}

/// Nominal mass of one metre of bar, kg (ГОСТ 34028: the nominal area at
/// 7850 kg/m³, to three decimals: Ø16 → 1.578).
pub fn mass_per_m(d: f64) -> f64 {
    (bar_area(d) * 1e-6 * crate::concrete::REBAR_DENSITY * 1000.).round() / 1000.
}

/// Smallest mandrel diameter of a bend (СП 63, section 10.3, «диаметр
/// оправки»):
/// plain bars 2.5·ds below Ø20 and 4·ds from Ø20, ribbed bars 5·ds and 8·ds.
pub fn min_bend_diameter(class: Option<&RebarClass>, d: f64) -> f64 {
    let smooth = class.is_some_and(|c| c.surface == BarSurface::Smooth);
    let k = match (smooth, d < 20.) {
        (true, true) => 2.5,
        (true, false) => 4.,
        (false, true) => 5.,
        (false, false) => 8.,
    };
    k * d
}

/// End of a bar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookKind {
    #[default]
    None,
    /// «Отгиб 90°».
    Deg90,
    /// «Крюк 135°» (stirrups).
    Deg135,
    /// «Крюк 180°» (plain bars in tension).
    Deg180,
}

impl HookKind {
    pub const ALL: [Self; 4] = [Self::None, Self::Deg90, Self::Deg135, Self::Deg180];

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "Прямой конец",
            Self::Deg90 => "Отгиб 90°",
            Self::Deg135 => "Крюк 135°",
            Self::Deg180 => "Крюк 180°",
        }
    }

    /// Straight part after the bend when none is given: 10·ds after 90°,
    /// 6·ds after 135°, 3·ds after 180°.
    pub fn default_extension(self, d: f64) -> f64 {
        match self {
            Self::None => 0.,
            Self::Deg90 => 10. * d,
            Self::Deg135 => 6. * d,
            Self::Deg180 => 3. * d,
        }
    }
}

/// A hook or bend at one end of the bars of a set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hook {
    pub kind: HookKind,
    /// Straight part after the bend, mm; the default of the kind when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<f64>,
}

impl Hook {
    pub fn new(kind: HookKind) -> Self {
        Self { kind, length: None }
    }

    pub fn is_none(&self) -> bool {
        self.kind == HookKind::None
    }

    pub fn extension(&self, d: f64) -> f64 {
        self.length.unwrap_or(self.kind.default_extension(d))
    }
}

/// How a host-generated set is laid out; kept with the set so that it can
/// be built again when the host part changes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RebarRule {
    /// Concrete cover to the bar surface, mm.
    pub cover: f64,
    /// Spacing of stirrups and mesh bars, mm.
    pub spacing: f64,
    /// Longitudinal bars on each face, corners included.
    pub per_face: u32,
    /// Longest bar delivered; longer bars are split with laps (СП 63,
    /// 10.3.30), mm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stock_length: Option<f64>,
}

/// Bar in tension or compression.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stress {
    Tension,
    Compression,
}

/// Anchorage and lap lengths of one bar, mm.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchorage {
    /// Basic anchorage length l0,an (10.3.24).
    pub basic: f64,
    /// Required anchorage length lan (10.3.25).
    pub anchorage: f64,
    /// Lap length ll (10.3.30).
    pub lap: f64,
}

/// Anchorage and lap of a bar of class `class`, Ø`d` in concrete with the
/// design tensile resistance `rbt` (MPa); `ratio` = As,cal / As,ef ≤ 1.
///
/// Rbond = η1·η2·Rbt, η1 = 1.5 plain, 2.0 cold-worked ribbed, 2.5 hot-rolled
/// ribbed; η2 = 1.0 up to Ø32 and 0.9 for Ø36, Ø40. l0,an = Rs·As/(Rbond·us)
/// = Rs·ds/(4·Rbond). lan = α·l0,an·ratio with α = 1.0 in tension (ribbed
/// bars with straight ends, plain bars with hooks) and 0.75 in compression,
/// at least 0.3·l0,an, 15·ds and 200 mm. ll = α·l0,an·ratio with α = 1.2 in
/// tension and 0.9 in compression (no more than half of the bars spliced in
/// one section), at least 0.4·α·l0,an, 20·ds and 250 mm.
pub fn anchorage(class: &RebarClass, d: f64, rbt: f64, ratio: f64, stress: Stress) -> Anchorage {
    let eta1 = match class.surface {
        BarSurface::Smooth => 1.5,
        BarSurface::ColdRibbed => 2.0,
        BarSurface::Ribbed => 2.5,
    };
    let eta2 = if d <= 32. { 1.0 } else { 0.9 };
    let rbond = eta1 * eta2 * rbt;
    let rs = match stress {
        Stress::Tension => class.rs,
        Stress::Compression => class.rsc,
    };
    let basic = rs * d / (4. * rbond);
    let ratio = ratio.clamp(0., 1.);
    let (a_an, a_lap) = match stress {
        Stress::Tension => (1.0, 1.2),
        Stress::Compression => (0.75, 0.9),
    };
    let anchorage = (a_an * basic * ratio)
        .max(0.3 * basic)
        .max(15. * d)
        .max(200.);
    let lap = (a_lap * basic * ratio)
        .max(0.4 * a_lap * basic)
        .max(20. * d)
        .max(250.);
    Anchorage {
        basic,
        anchorage,
        lap,
    }
}

/// Rbt of the concrete of a part from its material name («Бетон В25»);
/// В25 when the class is not known.
pub fn host_rbt(project: &Project, host: ObjectId) -> f64 {
    project
        .elements
        .get(&host)
        .and_then(|e| project.materials.get(&e.material))
        .and_then(|m| crate::library::concrete_of(&m.name))
        .or_else(|| crate::library::concrete_class("В25"))
        .map_or(1.05, |c| c.rbt)
}

/// Lap length of the bars of a set in tension in their host part, mm.
pub fn set_lap(project: &Project, set: &RebarSet) -> f64 {
    let rbt = set.host.map_or(1.05, |h| host_rbt(project, h));
    let class = rebar_class(&set.grade).unwrap_or(&REBAR_CLASSES[2]);
    anchorage(class, set.diameter, rbt, 1., Stress::Tension).lap
}

type V = [f64; 3];

fn add(a: V, b: V) -> V {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn mul(a: V, k: f64) -> V {
    [a[0] * k, a[1] * k, a[2] * k]
}
fn dot(a: V, b: V) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn norm(a: V) -> Option<V> {
    let l = dot(a, a).sqrt();
    (l > 1e-9).then(|| mul(a, 1. / l))
}

/// Points of a hook at the bar end `end`, the bar arriving along `out`
/// (unit, pointing past the end) and bending towards `side` (unit, normal
/// to `out`); `r` is the bend radius of the centre line. The points are
/// the corners of the centre line (tangent intersections), the end first.
pub fn hook_points(end: V, out: V, side: V, hook: Hook, d: f64, r: f64) -> Vec<V> {
    let ext = hook.extension(d);
    match hook.kind {
        HookKind::None => vec![end],
        HookKind::Deg90 => vec![end, add(end, mul(side, r + ext))],
        HookKind::Deg135 => {
            let (c, s) = (
                std::f64::consts::FRAC_1_SQRT_2,
                std::f64::consts::FRAC_1_SQRT_2,
            );
            let dir = add(mul(out, -c), mul(side, s));
            let tangent = r * (67.5f64).to_radians().tan();
            vec![end, add(end, mul(dir, tangent + ext))]
        }
        HookKind::Deg180 => {
            let c2 = add(end, mul(side, 2. * r));
            vec![end, c2, add(c2, mul(out, -(r + ext)))]
        }
    }
}

/// A straight bar `a`→`b` with the hooks of the set bending towards `side`;
/// the bar end is moved in by half a diameter so that the bend keeps the
/// cover.
fn hooked(a: V, b: V, side: V, hooks: [Hook; 2], d: f64, r: f64) -> Vec<V> {
    let Some(dir) = norm(sub(b, a)) else {
        return vec![a, b];
    };
    let inset = |h: &Hook| if h.is_none() { 0. } else { d / 2. };
    let a = add(a, mul(dir, inset(&hooks[0])));
    let b = sub(b, mul(dir, inset(&hooks[1])));
    let mut start = hook_points(a, mul(dir, -1.), side, hooks[0], d, r);
    start.reverse();
    start.extend(hook_points(b, dir, side, hooks[1], d, r));
    start
}

/// Pieces of a straight bar `a`→`b` no longer than `stock`, overlapping by
/// `lap`; the whole bar when it fits.
fn split(a: V, b: V, stock: Option<f64>, lap: f64) -> Vec<(V, V, bool, bool)> {
    let l = dot(sub(b, a), sub(b, a)).sqrt();
    let Some(stock) = stock.filter(|s| l > *s && *s > lap + 1.) else {
        return vec![(a, b, true, true)];
    };
    let n = ((l - lap) / (stock - lap)).ceil() as usize;
    let piece = (l + (n - 1) as f64 * lap) / n as f64;
    let dir = mul(sub(b, a), 1. / l);
    (0..n)
        .map(|i| {
            let s = i as f64 * (piece - lap);
            (
                add(a, mul(dir, s)),
                add(a, mul(dir, s + piece)),
                i == 0,
                i == n - 1,
            )
        })
        .collect()
}

/// Rectangular concrete member: frame, width, height, length.
pub fn rect_member(project: &Project, host: ObjectId) -> Result<(Frame, f64, f64, f64), String> {
    let e = project.elements.get(&host).ok_or("Деталь не найдена")?;
    let ElementShape::Member {
        profile,
        length,
        frame,
        ..
    } = &e.shape
    else {
        return Err("Выберите балку или колонну".into());
    };
    match project.profiles.get(profile).map(|p| &p.shape) {
        Some(ProfileShape::Rectangle { height, width }) => {
            Ok((*frame, *width, *height, length.value()))
        }
        _ => Err("Нужна деталь прямоугольного сечения".into()),
    }
}

/// Positions from `from` to `to` at `spacing`, both ends included.
pub fn spread(from: f64, to: f64, spacing: f64) -> Vec<f64> {
    let n = ((to - from) / spacing).floor().max(0.) as usize;
    let mut v: Vec<f64> = (0..=n).map(|i| from + i as f64 * spacing).collect();
    if to - v[v.len() - 1] > spacing * 0.25 {
        v.push(to);
    }
    v
}

/// Segments of the line `coord[axis] = c` inside the polygon, shrunk by
/// `inset` at both ends.
fn scan(contour: &[[f64; 2]], axis: usize, c: f64, inset: f64) -> Vec<(f64, f64)> {
    let other = 1 - axis;
    let mut xs: Vec<f64> = Vec::new();
    for i in 0..contour.len() {
        let (a, b) = (contour[i], contour[(i + 1) % contour.len()]);
        if (a[axis] <= c) != (b[axis] <= c) {
            let t = (c - a[axis]) / (b[axis] - a[axis]);
            xs.push(a[other] + (b[other] - a[other]) * t);
        }
    }
    xs.sort_by(f64::total_cmp);
    xs.chunks(2)
        .filter(|p| p.len() == 2 && p[1] - p[0] > 2. * inset + 1.)
        .map(|p| (p[0] + inset, p[1] - inset))
        .collect()
}

fn world(frame: &Frame, bar: Vec<V>) -> Vec<Point3> {
    bar.into_iter().map(|p| frame.world(Point3(p))).collect()
}

/// The centre lines of a set laid out by its rule in its host part.
pub fn generate(project: &Project, set: &RebarSet) -> Result<Vec<Vec<Point3>>, String> {
    let rule = set.rule.as_ref().ok_or("У набора нет правила раскладки")?;
    let host = set.host.ok_or("У набора нет детали-основы")?;
    let d = set.diameter;
    let r = set.bend_radius();
    let lap = set_lap(project, set);
    match set.kind {
        RebarKind::Longitudinal => {
            let (frame, w, h, l) = rect_member(project, host)?;
            let (x, y) = (w / 2. - rule.cover - d / 2., h / 2. - rule.cover - d / 2.);
            if x <= 0. || y <= 0. || l <= 2. * rule.cover {
                return Err("Сечение меньше защитного слоя".into());
            }
            let n = rule.per_face.max(2) as usize - 1;
            let mut spots: Vec<[f64; 2]> = Vec::new();
            for i in 0..=n {
                let t = i as f64 / n as f64;
                for p in [
                    [-x + 2. * x * t, -y],
                    [x, -y + 2. * y * t],
                    [x - 2. * x * t, y],
                    [-x, y - 2. * y * t],
                ] {
                    if !spots
                        .iter()
                        .any(|q| (q[0] - p[0]).abs() < 1e-6 && (q[1] - p[1]).abs() < 1e-6)
                    {
                        spots.push(p);
                    }
                }
            }
            let mut bars = Vec::new();
            for [px, py] in spots {
                // Hooks turn into the section: up from the bottom face,
                // down from the top, across from the side faces.
                let side = if py.abs() >= y - 1e-6 {
                    [0., -py.signum(), 0.]
                } else {
                    [-px.signum(), 0., 0.]
                };
                let (a, b) = ([px, py, rule.cover], [px, py, l - rule.cover]);
                for (a, b, first, last) in split(a, b, rule.stock_length, lap) {
                    let none = Hook::default();
                    let hooks = [
                        if first { set.hooks[0] } else { none },
                        if last { set.hooks[1] } else { none },
                    ];
                    bars.push(world(&frame, hooked(a, b, side, hooks, d, r)));
                }
            }
            Ok(bars)
        }
        RebarKind::Stirrup => {
            let (frame, w, h, l) = rect_member(project, host)?;
            let (x, y) = (w / 2. - rule.cover - d / 2., h / 2. - rule.cover - d / 2.);
            if x <= 0. || y <= 0. || l <= 2. * rule.cover {
                return Err("Сечение меньше защитного слоя".into());
            }
            Ok(spread(rule.cover, l - rule.cover, rule.spacing)
                .into_iter()
                .map(|z| {
                    let c = |px: f64, py: f64| [px, py, z];
                    let loop_ = [c(-x, -y), c(x, -y), c(x, y), c(-x, y), c(-x, -y)];
                    let mut bar: Vec<V> = Vec::new();
                    // The closing hooks both bend into the core at the
                    // first corner.
                    let mut start =
                        hook_points(loop_[0], [-1., 0., 0.], [0., 1., 0.], set.hooks[0], d, r);
                    start.reverse();
                    bar.extend(start);
                    bar.extend(&loop_[1..4]);
                    bar.extend(hook_points(
                        loop_[4],
                        [0., -1., 0.],
                        [1., 0., 0.],
                        set.hooks[1],
                        d,
                        r,
                    ));
                    world(&frame, bar)
                })
                .collect())
        }
        RebarKind::Mesh => {
            let e = project.elements.get(&host).ok_or("Деталь не найдена")?;
            // Contour in a plane, thickness along the plane normal, and the frame.
            let (contour, t, frame) = match &e.shape {
                ElementShape::Plate {
                    contour,
                    thickness,
                    frame,
                } => (contour.clone(), thickness.value(), *frame),
                ElementShape::Member { .. } => {
                    let (f, w, h, l) = rect_member(project, host)?;
                    // The panel face plane: local y (height) and z (length);
                    // the thinner side is the thickness.
                    let (thin, wide) = if w <= h { (w, h) } else { (h, w) };
                    let across = if w <= h { 0 } else { 1 };
                    let along = 1 - across;
                    let o = f.world(Point3({
                        let mut p = [0.; 3];
                        p[across] = -thin / 2.;
                        p[along] = -wide / 2.;
                        p
                    }));
                    let frame = Frame {
                        origin: o,
                        axes: [f.axes[2], f.axes[along], f.axes[across]],
                    };
                    (vec![[0., 0.], [l, 0.], [l, wide], [0., wide]], thin, frame)
                }
            };
            let layer = rule.cover + d / 2.;
            if t <= 2. * (layer + d) {
                return Err("Толщина меньше защитного слоя двух сеток".into());
            }
            let (lo, hi) = contour
                .iter()
                .fold(([f64::MAX; 2], [f64::MIN; 2]), |(lo, hi), p| {
                    (
                        [lo[0].min(p[0]), lo[1].min(p[1])],
                        [hi[0].max(p[0]), hi[1].max(p[1])],
                    )
                });
            let mut bars = Vec::new();
            for (z_x, z_y, toward) in [(layer, layer + d, 1.), (t - layer, t - layer - d, -1.)] {
                // Bars along x at y steps, then along y at x steps; hooks
                // turn towards the other face.
                for (axis, z) in [(1usize, z_x), (0usize, z_y)] {
                    for c in spread(lo[axis] + layer, hi[axis] - layer, rule.spacing) {
                        for (a, b) in scan(&contour, axis, c, rule.cover) {
                            let p = |v: f64| {
                                let mut q = [0., 0., z];
                                q[axis] = c;
                                q[1 - axis] = v;
                                q
                            };
                            for (a, b, first, last) in split(p(a), p(b), rule.stock_length, lap) {
                                let none = Hook::default();
                                let hooks = [
                                    if first { set.hooks[0] } else { none },
                                    if last { set.hooks[1] } else { none },
                                ];
                                bars.push(world(
                                    &frame,
                                    hooked(a, b, [0., 0., toward], hooks, d, r),
                                ));
                            }
                        }
                    }
                }
            }
            if bars.is_empty() {
                return Err("Контур меньше защитного слоя".into());
            }
            Ok(bars)
        }
        _ => Err("Набор ставится по точкам, правила раскладки нет".into()),
    }
}

/// Hooks of a bar placed by picks: for a straight bar they bend down (or
/// along +X when the bar is vertical); otherwise in the plane of the end
/// legs, on the side the bar turns to.
pub fn hook_picked(bar: &[Point3], hooks: [Hook; 2], d: f64, r: f64) -> Vec<Point3> {
    let pts: Vec<V> = bar.iter().map(|p| p.0).collect();
    if hooks.iter().all(Hook::is_none) || pts.len() < 2 {
        return bar.to_vec();
    }
    let n = pts.len();
    let side_of = |end: V, next: V, far: Option<V>| -> V {
        let out = norm(sub(end, next)).unwrap_or([1., 0., 0.]);
        let turn = far
            .map(|f| sub(f, next))
            .map(|v| sub(v, mul(out, dot(v, out))))
            .and_then(norm);
        turn.unwrap_or_else(|| {
            let down = [0., 0., -1.];
            let v = sub(down, mul(out, dot(down, out)));
            norm(v).unwrap_or([1., 0., 0.])
        })
    };
    let far0 = (n > 2).then(|| pts[2]);
    let far1 = (n > 2).then(|| pts[n - 3]);
    let s0 = side_of(pts[0], pts[1], far0);
    let s1 = side_of(pts[n - 1], pts[n - 2], far1);
    let o0 = norm(sub(pts[0], pts[1])).unwrap_or([1., 0., 0.]);
    let o1 = norm(sub(pts[n - 1], pts[n - 2])).unwrap_or([1., 0., 0.]);
    let mut start = hook_points(pts[0], o0, s0, hooks[0], d, r);
    start.reverse();
    start.extend(&pts[1..n - 1]);
    start.extend(hook_points(pts[n - 1], o1, s1, hooks[1], d, r));
    start.into_iter().map(Point3).collect()
}

/// Whether the host of a set differs between the two states.
fn host_changed(project: &Project, previous: &Project, host: ObjectId) -> bool {
    let (Some(a), Some(b)) = (project.elements.get(&host), previous.elements.get(&host)) else {
        return true;
    };
    let profile = |p: &Project, e: &crate::Element| match &e.shape {
        ElementShape::Member { profile, .. } => p.profiles.get(profile).cloned(),
        _ => None,
    };
    a != b
        || profile(project, a) != profile(previous, b)
        || project.materials.get(&a.material) != previous.materials.get(&b.material)
}

/// Build again the bars of every set laid out by a rule whose host part,
/// its section or concrete, or whose own rule changed since `previous`;
/// drop the sets of deleted parts.
pub fn rebuild_rebars(project: &mut Project, previous: &Project) -> Result<(), ModelError> {
    if project.concrete.rebars.is_empty() {
        return Ok(());
    }
    let mut sets = std::mem::take(&mut project.concrete.rebars);
    sets.retain(|s| s.host.is_none_or(|h| project.elements.contains_key(&h)));
    for set in &mut sets {
        let (Some(host), Some(_)) = (set.host, set.rule.as_ref()) else {
            continue;
        };
        let before = previous
            .concrete
            .rebars
            .iter()
            .find(|s| s.number == set.number);
        let own = before.is_none_or(|b| {
            b.rule != set.rule
                || b.hooks != set.hooks
                || b.diameter != set.diameter
                || b.grade != set.grade
                || b.bend_diameter != set.bend_diameter
        });
        if before.is_some() && !own && !host_changed(project, previous, host) {
            continue;
        }
        match generate(project, set) {
            Ok(bars) => set.bars = bars,
            Err(e) => {
                return Err(ModelError::InvalidValue(format!(
                    "Арматура №{}: {e}",
                    set.number
                )));
            }
        }
    }
    project.concrete.rebars = sets;
    Ok(())
}
