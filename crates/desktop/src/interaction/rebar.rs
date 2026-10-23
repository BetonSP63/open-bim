//! «Армирование»: bar sets generated in concrete parts (longitudinal bars
//! and stirrups of beams and columns, face meshes of slabs and panels) or
//! placed by picks (one bar, a group, a bar along guide points), a 3D
//! overlay and the bar schedule.
use open_bim_commands::Command;
use open_bim_model::{ElementShape, ObjectId, Point3, ProfileShape, Project, RebarKind, RebarSet};
use open_bim_renderer::DVec3;
use open_bim_renderer::overlay::{Mark, Projector};

pub const GRADES: [&str; 4] = ["А240", "А400", "А500С", "А600"];
pub const DIAMETERS: [f64; 13] = [
    6., 8., 10., 12., 14., 16., 18., 20., 22., 25., 28., 32., 40.,
];

/// Tool settings shared by the «Армирование» commands.
#[derive(Clone, Debug, PartialEq)]
pub struct RebarSettings {
    pub diameter: f64,
    pub grade: String,
    /// Concrete cover to the bar surface, mm.
    pub cover: f64,
    /// Bar spacing of stirrups, meshes and groups, mm.
    pub spacing: f64,
    /// Longitudinal bars on each face, corners included (≥ 2).
    pub per_face: u32,
}

impl Default for RebarSettings {
    fn default() -> Self {
        Self {
            diameter: 16.,
            grade: "А500С".into(),
            cover: 30.,
            spacing: 200.,
            per_face: 2,
        }
    }
}

impl RebarSettings {
    fn check(&self) -> Result<(), String> {
        if !(3.0..=60.).contains(&self.diameter) {
            return Err("Диаметр арматуры от 3 до 60 мм".into());
        }
        if !(0.0..=500.).contains(&self.cover) {
            return Err("Защитный слой от 0 до 500 мм".into());
        }
        if !(self.diameter..=10_000.).contains(&self.spacing) {
            return Err("Шаг не меньше диаметра и не больше 10 000 мм".into());
        }
        if !(2..=50).contains(&self.per_face) {
            return Err("Стержней на грани от 2 до 50".into());
        }
        Ok(())
    }

    fn set(&self, kind: RebarKind, host: Option<ObjectId>, bars: Vec<Vec<Point3>>) -> RebarSet {
        RebarSet {
            number: 0,
            kind,
            host,
            diameter: self.diameter,
            grade: self.grade.clone(),
            bars,
        }
    }
}

/// Rectangular concrete member: frame, width, height, length.
fn member(
    project: &Project,
    host: ObjectId,
) -> Result<(open_bim_model::Frame, f64, f64, f64), String> {
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

/// «Продольные»: bars along the member at the perimeter inside the cover.
pub fn longitudinal(
    project: &Project,
    host: ObjectId,
    s: &RebarSettings,
) -> Result<RebarSet, String> {
    s.check()?;
    let (frame, w, h, l) = member(project, host)?;
    let (x, y) = (
        w / 2. - s.cover - s.diameter / 2.,
        h / 2. - s.cover - s.diameter / 2.,
    );
    if x <= 0. || y <= 0. || l <= 2. * s.cover {
        return Err("Сечение меньше защитного слоя".into());
    }
    let n = s.per_face as usize - 1;
    let mut spots: Vec<[f64; 2]> = Vec::new();
    let mut push = |p: [f64; 2]| {
        if !spots
            .iter()
            .any(|q| (q[0] - p[0]).abs() < 1e-6 && (q[1] - p[1]).abs() < 1e-6)
        {
            spots.push(p);
        }
    };
    for i in 0..=n {
        let t = i as f64 / n as f64;
        for p in [
            [-x + 2. * x * t, -y],
            [x, -y + 2. * y * t],
            [x - 2. * x * t, y],
            [-x, y - 2. * y * t],
        ] {
            push(p);
        }
    }
    let bars = spots
        .into_iter()
        .map(|[px, py]| {
            vec![
                frame.world(Point3([px, py, s.cover])),
                frame.world(Point3([px, py, l - s.cover])),
            ]
        })
        .collect();
    Ok(s.set(RebarKind::Longitudinal, Some(host), bars))
}

/// Positions from `from` to `to` at `spacing`, both ends included.
fn spread(from: f64, to: f64, spacing: f64) -> Vec<f64> {
    let n = ((to - from) / spacing).floor().max(0.) as usize;
    let mut v: Vec<f64> = (0..=n).map(|i| from + i as f64 * spacing).collect();
    if to - v[v.len() - 1] > spacing * 0.25 {
        v.push(to);
    }
    v
}

/// «Поперечные»: closed stirrups along the member.
pub fn stirrups(project: &Project, host: ObjectId, s: &RebarSettings) -> Result<RebarSet, String> {
    s.check()?;
    let (frame, w, h, l) = member(project, host)?;
    let (x, y) = (
        w / 2. - s.cover - s.diameter / 2.,
        h / 2. - s.cover - s.diameter / 2.,
    );
    if x <= 0. || y <= 0. || l <= 2. * s.cover {
        return Err("Сечение меньше защитного слоя".into());
    }
    let bars = spread(s.cover, l - s.cover, s.spacing)
        .into_iter()
        .map(|z| {
            [[-x, -y], [x, -y], [x, y], [-x, y], [-x, -y]]
                .into_iter()
                .map(|[px, py]| frame.world(Point3([px, py, z])))
                .collect()
        })
        .collect();
    Ok(s.set(RebarKind::Stirrup, Some(host), bars))
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

/// «По грани»: two-way meshes at both faces of a slab, or of a panel
/// (a rectangular member: meshes at its two wide faces).
pub fn mesh(project: &Project, host: ObjectId, s: &RebarSettings) -> Result<RebarSet, String> {
    s.check()?;
    let e = project.elements.get(&host).ok_or("Деталь не найдена")?;
    // Contour in a plane, thickness along the plane normal, and the frame.
    let (contour, t, frame) = match &e.shape {
        ElementShape::Plate {
            contour,
            thickness,
            frame,
        } => (contour.clone(), thickness.value(), *frame),
        ElementShape::Member { .. } => {
            let (f, w, h, l) = member(project, host)?;
            // The panel face plane: local y (height) and z (length); the
            // thinner side is the thickness.
            let (thin, wide) = if w <= h { (w, h) } else { (h, w) };
            let across = if w <= h { 0 } else { 1 };
            let along = 1 - across;
            let col = |k: usize| [f.axes[k][0], f.axes[k][1], f.axes[k][2]];
            let o = f.world(Point3({
                let mut p = [0.; 3];
                p[across] = -thin / 2.;
                p[along] = -wide / 2.;
                p
            }));
            let frame = open_bim_model::Frame {
                origin: o,
                axes: [col(2), col(along), col(across)],
            };
            (vec![[0., 0.], [l, 0.], [l, wide], [0., wide]], thin, frame)
        }
    };
    let layer = s.cover + s.diameter / 2.;
    if t <= 2. * (layer + s.diameter) {
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
    for (z_x, z_y) in [
        (layer, layer + s.diameter),
        (t - layer, t - layer - s.diameter),
    ] {
        // Bars along x at y steps, then along y at x steps.
        for (axis, z) in [(1usize, z_x), (0usize, z_y)] {
            for c in spread(lo[axis] + layer, hi[axis] - layer, s.spacing) {
                for (a, b) in scan(&contour, axis, c, s.cover) {
                    let p = |v: f64| {
                        let mut q = [0., 0., z];
                        q[axis] = c;
                        q[1 - axis] = v;
                        frame.world(Point3(q))
                    };
                    bars.push(vec![p(a), p(b)]);
                }
            }
        }
    }
    if bars.is_empty() {
        return Err("Контур меньше защитного слоя".into());
    }
    Ok(s.set(RebarKind::Mesh, Some(host), bars))
}

fn p3(v: DVec3) -> Point3 {
    Point3(v.to_array())
}

/// «Один стержень» between two points.
pub fn single(a: DVec3, b: DVec3, s: &RebarSettings) -> Result<RebarSet, String> {
    s.check()?;
    if a.distance(b) < 1. {
        return Err("Точки стержня совпадают".into());
    }
    Ok(s.set(RebarKind::Single, None, vec![vec![p3(a), p3(b)]]))
}

/// «Группа стержней»: bars parallel to `a`–`b`, spread at the spacing up
/// to the point `c` (measured across the bar).
pub fn group(a: DVec3, b: DVec3, c: DVec3, s: &RebarSettings) -> Result<RebarSet, String> {
    s.check()?;
    let d = (b - a).try_normalize().ok_or("Точки стержня совпадают")?;
    let across = (c - a) - d * (c - a).dot(d);
    let span = across.length();
    if span < 1. {
        return Err("Третья точка лежит на линии стержня".into());
    }
    let dir = across / span;
    let bars = spread(0., span, s.spacing)
        .into_iter()
        .map(|t| vec![p3(a + dir * t), p3(b + dir * t)])
        .collect();
    Ok(s.set(RebarKind::Group, None, bars))
}

/// «По направляющим»: one bar bent along the picked points.
pub fn guide(points: &[DVec3], s: &RebarSettings) -> Result<RebarSet, String> {
    s.check()?;
    let mut bar: Vec<Point3> = Vec::new();
    for p in points {
        if bar
            .last()
            .is_none_or(|q| DVec3::from_array(q.0).distance(*p) >= 1.)
        {
            bar.push(p3(*p));
        }
    }
    if bar.len() < 2 {
        return Err("Нужно не меньше двух разных точек".into());
    }
    Ok(s.set(RebarKind::Guide, None, vec![bar]))
}

/// Adds the set under the next number.
pub fn add(project: &Project, mut set: RebarSet) -> Result<Vec<Command>, String> {
    let mut concrete = project.concrete.clone();
    set.number = concrete.rebars.iter().map(|r| r.number).max().unwrap_or(0) + 1;
    concrete.rebars.push(set);
    concrete.validate().map_err(|e| e.to_string())?;
    Ok(vec![Command::SetConcrete { concrete }])
}

pub fn remove(project: &Project, number: u32) -> Result<Vec<Command>, String> {
    let mut concrete = project.concrete.clone();
    let before = concrete.rebars.len();
    concrete.rebars.retain(|r| r.number != number);
    if concrete.rebars.len() == before {
        return Err("Нет такого набора".into());
    }
    Ok(vec![Command::SetConcrete { concrete }])
}

/// Bar schedule row: diameter and grade with the bar count, length and mass.
#[derive(Clone, Debug, PartialEq)]
pub struct ScheduleRow {
    pub diameter: f64,
    pub grade: String,
    pub bars: usize,
    pub length_m: f64,
    pub mass_kg: f64,
}

/// Rows grouped by grade and diameter.
pub fn schedule(project: &Project) -> Vec<ScheduleRow> {
    let mut rows: Vec<ScheduleRow> = Vec::new();
    for r in &project.concrete.rebars {
        let i = match rows
            .iter()
            .position(|x| x.grade == r.grade && x.diameter == r.diameter)
        {
            Some(i) => i,
            None => {
                rows.push(ScheduleRow {
                    diameter: r.diameter,
                    grade: r.grade.clone(),
                    bars: 0,
                    length_m: 0.,
                    mass_kg: 0.,
                });
                rows.len() - 1
            }
        };
        rows[i].bars += r.bars.len();
        rows[i].length_m += r.total_length_mm() / 1000.;
        rows[i].mass_kg += r.mass_kg();
    }
    rows.sort_by(|a, b| {
        a.grade
            .cmp(&b.grade)
            .then(a.diameter.total_cmp(&b.diameter))
    });
    rows
}

const BAR: [u8; 4] = [200, 60, 40, 255];

/// Bars drawn over the model.
pub fn marks(project: &Project, p: &Projector) -> Vec<Mark> {
    let mut out = Vec::new();
    for r in &project.concrete.rebars {
        let width = (r.diameter / 8.).clamp(1., 3.) as f32;
        for bar in &r.bars {
            for w in bar.windows(2) {
                if let (Some(a), Some(b)) = (
                    p.screen(DVec3::from_array(w[0].0)),
                    p.screen(DVec3::from_array(w[1].0)),
                ) {
                    out.push(Mark::Line {
                        a,
                        b,
                        color: BAR,
                        width,
                        dash: None,
                    });
                }
            }
        }
    }
    out
}
