//! «Армирование»: bar sets generated in concrete parts (longitudinal bars
//! and stirrups of beams and columns, face meshes of slabs and panels) or
//! placed by picks (one bar, a group, a bar along guide points), a 3D
//! overlay and the bar schedule.
use open_bim_commands::Command;
use open_bim_model::rebar::{self as model, Hook, HookKind, RebarRule, Stress};
use open_bim_model::{ObjectId, Point3, Project, RebarKind, RebarSet};
use open_bim_renderer::DVec3;
use open_bim_renderer::overlay::{Mark, Projector};

pub const GRADES: [&str; 5] = ["А240", "А400", "А500С", "В500С", "А600"];
pub const DIAMETERS: [f64; 16] = model::REBAR_DIAMETERS;

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
    /// Ends of longitudinal bars, mesh bars and bars by picks.
    pub hook: HookKind,
    /// Ends of stirrups.
    pub stirrup_hook: HookKind,
    /// Mandrel diameter, mm; the smallest of the class when unset.
    pub bend_diameter: Option<f64>,
    /// Longest delivered bar; longer bars are lapped, mm.
    pub stock_length: Option<f64>,
}

impl Default for RebarSettings {
    fn default() -> Self {
        Self {
            diameter: 16.,
            grade: "А500С".into(),
            cover: 30.,
            spacing: 200.,
            per_face: 2,
            hook: HookKind::None,
            stirrup_hook: HookKind::Deg135,
            bend_diameter: None,
            stock_length: Some(11_700.),
        }
    }
}

impl RebarSettings {
    fn check(&self) -> Result<(), String> {
        if !(3.0..=60.).contains(&self.diameter) {
            return Err("Диаметр арматуры от 3 до 60 мм".into());
        }
        if let Some(c) = model::rebar_class(&self.grade)
            && !(c.d_min..=c.d_max).contains(&self.diameter)
        {
            return Err(format!(
                "Класс {} выпускается диаметром {}–{} мм",
                c.name, c.d_min, c.d_max
            ));
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
        if self
            .bend_diameter
            .is_some_and(|d| !(self.diameter..=10_000.).contains(&d))
        {
            return Err("Диаметр оправки не меньше диаметра стержня".into());
        }
        if self
            .stock_length
            .is_some_and(|l| !(500.0..=100_000.).contains(&l))
        {
            return Err("Длина стержня от 500 до 100 000 мм".into());
        }
        Ok(())
    }

    fn set(&self, kind: RebarKind, host: Option<ObjectId>, bars: Vec<Vec<Point3>>) -> RebarSet {
        let hook = if kind == RebarKind::Stirrup {
            self.stirrup_hook
        } else {
            self.hook
        };
        RebarSet {
            number: 0,
            kind,
            host,
            diameter: self.diameter,
            grade: self.grade.clone(),
            bars,
            hooks: [Hook::new(hook); 2],
            bend_diameter: self.bend_diameter,
            rule: host.map(|_| RebarRule {
                cover: self.cover,
                spacing: self.spacing,
                per_face: self.per_face,
                stock_length: self.stock_length,
            }),
        }
    }

    /// Smallest mandrel diameter of the current class and diameter, mm.
    pub fn min_bend(&self) -> f64 {
        model::min_bend_diameter(model::rebar_class(&self.grade), self.diameter)
    }

    /// Anchorage and lap of the next bars in concrete with `rbt`, MPa.
    pub fn anchorage(&self, rbt: f64, stress: Stress) -> Option<model::Anchorage> {
        model::rebar_class(&self.grade).map(|c| model::anchorage(c, self.diameter, rbt, 1., stress))
    }

    /// A set laid out by the rule in the host part.
    fn generated(
        &self,
        project: &Project,
        kind: RebarKind,
        host: ObjectId,
    ) -> Result<RebarSet, String> {
        self.check()?;
        let mut set = self.set(kind, Some(host), Vec::new());
        set.bars = model::generate(project, &set)?;
        Ok(set)
    }

    /// Bars placed by picks with the hooks of the settings.
    fn picked(&self, kind: RebarKind, bars: Vec<Vec<Point3>>) -> RebarSet {
        let mut set = self.set(kind, None, Vec::new());
        let r = set.bend_radius();
        set.bars = bars
            .iter()
            .map(|b| model::hook_picked(b, set.hooks, self.diameter, r))
            .collect();
        set
    }
}

/// «Продольные»: bars along the member at the perimeter inside the cover,
/// lapped when longer than the stock length.
pub fn longitudinal(
    project: &Project,
    host: ObjectId,
    s: &RebarSettings,
) -> Result<RebarSet, String> {
    s.generated(project, RebarKind::Longitudinal, host)
}

/// «Поперечные»: closed stirrups along the member.
pub fn stirrups(project: &Project, host: ObjectId, s: &RebarSettings) -> Result<RebarSet, String> {
    s.generated(project, RebarKind::Stirrup, host)
}

/// «По грани»: two-way meshes at both faces of a slab, or of a panel
/// (a rectangular member: meshes at its two wide faces).
pub fn mesh(project: &Project, host: ObjectId, s: &RebarSettings) -> Result<RebarSet, String> {
    s.generated(project, RebarKind::Mesh, host)
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
    Ok(s.picked(RebarKind::Single, vec![vec![p3(a), p3(b)]]))
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
    let bars = model::spread(0., span, s.spacing)
        .into_iter()
        .map(|t| vec![p3(a + dir * t), p3(b + dir * t)])
        .collect();
    Ok(s.picked(RebarKind::Group, bars))
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
    Ok(s.picked(RebarKind::Guide, vec![bar]))
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
