//! «Опалубка»: formwork panels tiled over the formed faces of concrete
//! parts, props under slabs and beams, and the formwork schedule.
use super::concrete::part_type;
use open_bim_commands::Command;
use open_bim_model::{
    ElementShape, FormPanel, Frame, ObjectId, Point3, ProfileShape, Project, Prop,
};
use open_bim_renderer::DVec3;
use open_bim_renderer::overlay::{Mark, Projector};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq)]
pub struct FormworkSettings {
    /// Largest panel, mm.
    pub panel_width: f64,
    pub panel_height: f64,
    /// Prop grid step, mm.
    pub prop_spacing: f64,
}

impl Default for FormworkSettings {
    fn default() -> Self {
        Self {
            panel_width: 1200.,
            panel_height: 3000.,
            prop_spacing: 1200.,
        }
    }
}

impl FormworkSettings {
    fn check(&self) -> Result<(), String> {
        let ok = |v: f64| (100.0..=20_000.).contains(&v);
        if ok(self.panel_width) && ok(self.panel_height) && ok(self.prop_spacing) {
            Ok(())
        } else {
            Err("Размеры щитов и шаг опор от 100 до 20 000 мм".into())
        }
    }
}

/// A rectangular formed face: origin, unit directions and sizes.
struct Face {
    origin: DVec3,
    u: DVec3,
    v: DVec3,
    width: f64,
    height: f64,
    /// Panels whose centre is outside this outline (face coordinates) are
    /// left out: the soffit of a non-rectangular slab.
    clip: Option<Vec<[f64; 2]>>,
}

fn v3(p: Point3) -> DVec3 {
    DVec3::from_array(p.0)
}

fn world_dir(f: &Frame, k: usize) -> DVec3 {
    DVec3::from_array(f.axes[k])
}

/// The faces of a part that need formwork: sides of columns and footings;
/// sides and soffit of beams; both faces and the ends of panels; the soffit
/// and edges of slabs. Tops are open.
fn faces(project: &Project, host: ObjectId) -> Result<Vec<Face>, String> {
    let e = project.elements.get(&host).ok_or("Деталь не найдена")?;
    let kind = part_type(e).ok_or("Выберите железобетонную деталь")?;
    let mut out = Vec::new();
    match &e.shape {
        ElementShape::Member {
            profile,
            length,
            frame,
            ..
        } => {
            let Some(ProfileShape::Rectangle { height, width }) =
                project.profiles.get(profile).map(|p| &p.shape)
            else {
                return Err("Нужна деталь прямоугольного сечения".into());
            };
            let (w, h, l) = (*width, *height, length.value());
            let o = v3(frame.origin);
            let (x, y, z) = (
                world_dir(frame, 0),
                world_dir(frame, 1),
                world_dir(frame, 2),
            );
            // Long faces: normal ±x (spanning y and z), ±y (spanning x and z).
            for (n, across, size) in [(x, y, h), (y, x, w)] {
                let half_n = if n == x { w / 2. } else { h / 2. };
                for s in [-1., 1.] {
                    let normal = n * s;
                    // Tops are open; only beams need a soffit.
                    if normal.z > 0.5 || normal.z < -0.5 && kind != "beam" {
                        continue;
                    }
                    out.push(Face {
                        origin: o + normal * half_n - across * (size / 2.),
                        u: z,
                        v: across,
                        width: l,
                        height: size,
                        clip: None,
                    });
                }
            }
            if kind == "panel" {
                for (at, s) in [(0., -1.), (l, 1.)] {
                    let normal = z * s;
                    if normal.z.abs() > 0.5 {
                        continue;
                    }
                    out.push(Face {
                        origin: o + z * at - x * (w / 2.) - y * (h / 2.),
                        u: x,
                        v: y,
                        width: w,
                        height: h,
                        clip: None,
                    });
                }
            }
        }
        ElementShape::Plate {
            contour,
            thickness,
            frame,
        } => {
            let t = thickness.value();
            let (x, y, z) = (
                world_dir(frame, 0),
                world_dir(frame, 1),
                world_dir(frame, 2),
            );
            let o = v3(frame.origin);
            let at = |p: [f64; 2], h: f64| o + x * p[0] + y * p[1] + z * h;
            // Soffit: the contour's bounding rectangle on the lower face.
            let (lo, hi) = contour
                .iter()
                .fold(([f64::MAX; 2], [f64::MIN; 2]), |(lo, hi), p| {
                    (
                        [lo[0].min(p[0]), lo[1].min(p[1])],
                        [hi[0].max(p[0]), hi[1].max(p[1])],
                    )
                });
            let bottom = if z.z >= 0. { 0. } else { t };
            out.push(Face {
                origin: at(lo, bottom),
                u: x,
                v: y,
                width: hi[0] - lo[0],
                height: hi[1] - lo[1],
                clip: Some(
                    contour
                        .iter()
                        .map(|p| [p[0] - lo[0], p[1] - lo[1]])
                        .collect(),
                ),
            });
            for i in 0..contour.len() {
                let (a, b) = (contour[i], contour[(i + 1) % contour.len()]);
                let (pa, pb) = (at(a, 0.), at(b, 0.));
                let len = pa.distance(pb);
                if len < 1. {
                    continue;
                }
                out.push(Face {
                    origin: pa,
                    u: (pb - pa) / len,
                    v: z,
                    width: len,
                    height: t,
                    clip: None,
                });
            }
        }
    }
    Ok(out)
}

/// Panels for one part, tiled from the face origin.
pub fn panels(
    project: &Project,
    host: ObjectId,
    s: &FormworkSettings,
) -> Result<Vec<FormPanel>, String> {
    s.check()?;
    let mut out = Vec::new();
    for f in faces(project, host)? {
        let mut a = 0.;
        while a < f.width - 1e-6 {
            let w = (f.width - a).min(s.panel_width);
            let mut b = 0.;
            while b < f.height - 1e-6 {
                let h = (f.height - b).min(s.panel_height);
                let centre = [a + w / 2., b + h / 2.];
                if f.clip.as_ref().is_some_and(|c| !inside(c, centre)) {
                    b += h;
                    continue;
                }
                out.push(FormPanel {
                    host,
                    origin: Point3((f.origin + f.u * a + f.v * b).to_array()),
                    u: f.u.to_array(),
                    v: f.v.to_array(),
                    width: w,
                    height: h,
                });
                b += h;
            }
            a += w;
        }
    }
    Ok(out)
}

fn inside(contour: &[[f64; 2]], p: [f64; 2]) -> bool {
    let mut odd = false;
    for i in 0..contour.len() {
        let (a, b) = (contour[i], contour[(i + 1) % contour.len()]);
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < a[0] + (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1])
        {
            odd = !odd;
        }
    }
    odd
}

/// Top of what a prop at `(x, y)` below `z` stands on: the highest concrete
/// part under it, else ground level 0.
fn support(project: &Project, host: ObjectId, x: f64, y: f64, z: f64) -> f64 {
    project
        .elements
        .values()
        .filter(|e| e.id != host && part_type(e).is_some())
        .filter_map(|e| project.element_bounds(e.id).ok())
        .filter(|b| {
            b[0][0] <= x && x <= b[1][0] && b[0][1] <= y && y <= b[1][1] && b[1][2] <= z + 1e-6
        })
        .map(|b| b[1][2])
        .fold(0., f64::max)
}

/// Props under a slab (a grid) or a beam (a row along it).
pub fn props(project: &Project, host: ObjectId, s: &FormworkSettings) -> Result<Vec<Prop>, String> {
    s.check()?;
    let e = project.elements.get(&host).ok_or("Деталь не найдена")?;
    let kind = part_type(e).ok_or("Выберите железобетонную деталь")?;
    let bounds = project.element_bounds(host).map_err(|e| e.to_string())?;
    let z = bounds[0][2];
    let mut spots: Vec<[f64; 2]> = Vec::new();
    match (&e.shape, kind) {
        (ElementShape::Plate { contour, frame, .. }, _) => {
            if frame.axes[2][2].abs() < 0.99 {
                return Err("Опоры ставятся под горизонтальные перекрытия".into());
            }
            let world: Vec<[f64; 2]> = contour
                .iter()
                .map(|p| {
                    let w = frame.world(Point3([p[0], p[1], 0.]));
                    [w.0[0], w.0[1]]
                })
                .collect();
            let st = s.prop_spacing;
            let mut x = bounds[0][0] + st / 2.;
            while x < bounds[1][0] {
                let mut y = bounds[0][1] + st / 2.;
                while y < bounds[1][1] {
                    if inside(&world, [x, y]) {
                        spots.push([x, y]);
                    }
                    y += st;
                }
                x += st;
            }
        }
        (ElementShape::Member { frame, length, .. }, "beam") => {
            let (o, d) = (v3(frame.origin), world_dir(frame, 2));
            if d.z.abs() > 0.1 {
                return Err("Опоры ставятся под горизонтальные балки".into());
            }
            let l = length.value();
            let n = (l / s.prop_spacing).floor().max(1.) as usize;
            for i in 0..n {
                let p = o + d * (l * (i as f64 + 0.5) / n as f64);
                spots.push([p.x, p.y]);
            }
        }
        _ => return Err("Опоры ставятся под перекрытия и балки".into()),
    }
    let out: Vec<Prop> = spots
        .into_iter()
        .filter_map(|[x, y]| {
            let base = support(project, host, x, y, z);
            (z - base >= 1.).then_some(Prop {
                host,
                base: Point3([x, y, base]),
                top: Point3([x, y, z]),
            })
        })
        .collect();
    if out.is_empty() {
        return Err("Под деталью нет места для опор".into());
    }
    Ok(out)
}

/// Commands replacing the panels (or props) of the given parts.
pub fn generate(
    project: &Project,
    hosts: &BTreeSet<ObjectId>,
    s: &FormworkSettings,
    with_props: bool,
) -> Result<(Vec<Command>, usize), String> {
    let mut concrete = project.concrete.clone();
    let mut made = 0;
    if with_props {
        concrete.props.retain(|p| !hosts.contains(&p.host));
        for h in hosts {
            let new = props(project, *h, s)?;
            made += new.len();
            concrete.props.extend(new);
        }
    } else {
        concrete.forms.retain(|p| !hosts.contains(&p.host));
        for h in hosts {
            let new = panels(project, *h, s)?;
            made += new.len();
            concrete.forms.extend(new);
        }
    }
    concrete.validate().map_err(|e| e.to_string())?;
    Ok((vec![Command::SetConcrete { concrete }], made))
}

/// Removes the formwork and props of the given parts.
pub fn clear(project: &Project, hosts: &BTreeSet<ObjectId>) -> Result<Vec<Command>, String> {
    let mut concrete = project.concrete.clone();
    let before = concrete.forms.len() + concrete.props.len();
    concrete.forms.retain(|p| !hosts.contains(&p.host));
    concrete.props.retain(|p| !hosts.contains(&p.host));
    if concrete.forms.len() + concrete.props.len() == before {
        return Err("У выбранных деталей нет опалубки".into());
    }
    Ok(vec![Command::SetConcrete { concrete }])
}

/// Schedule row: panels of one size, or props of one length.
#[derive(Clone, Debug, PartialEq)]
pub struct ScheduleRow {
    pub item: String,
    pub count: usize,
    pub area_m2: f64,
}

/// «Ведомость опалубки»: panels grouped by size, then props by length.
pub fn schedule(project: &Project) -> Vec<ScheduleRow> {
    let mut rows: Vec<ScheduleRow> = Vec::new();
    let mut add = |item: String, area: f64| match rows.iter_mut().find(|r| r.item == item) {
        Some(r) => {
            r.count += 1;
            r.area_m2 += area;
        }
        None => rows.push(ScheduleRow {
            item,
            count: 1,
            area_m2: area,
        }),
    };
    let fmt = |v: f64| open_bim_reports::fmt((v * 10.).round() / 10.);
    for f in &project.concrete.forms {
        let (a, b) = if f.width >= f.height {
            (f.width, f.height)
        } else {
            (f.height, f.width)
        };
        add(format!("Щит {}x{}", fmt(a), fmt(b)), f.area_m2());
    }
    for p in &project.concrete.props {
        add(format!("Опора L={}", fmt(p.length())), 0.);
    }
    rows.sort_by(|a, b| a.item.cmp(&b.item));
    rows
}

const FORM: [u8; 4] = [190, 140, 40, 255];
const PROP: [u8; 4] = [120, 80, 30, 255];

/// Panel outlines and props over the model.
pub fn marks(project: &Project, p: &Projector) -> Vec<Mark> {
    let mut out = Vec::new();
    let mut line = |a: Point3, b: Point3, color, width| {
        if let (Some(a), Some(b)) = (p.screen(v3(a)), p.screen(v3(b))) {
            out.push(Mark::Line {
                a,
                b,
                color,
                width,
                dash: None,
            });
        }
    };
    for f in &project.concrete.forms {
        let c = f.corners();
        for i in 0..4 {
            line(c[i], c[(i + 1) % 4], FORM, 1.);
        }
    }
    for pr in &project.concrete.props {
        line(pr.base, pr.top, PROP, 2.);
    }
    out
}
