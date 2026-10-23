//! «Бетонирование»: pour objects are the connected cast-in-place concrete
//! parts of one grade, split by pour breaks; «Рассчитать единицы
//! бетонирования» stores them numbered as pour units.
use super::concrete::{CAST_UNIT, part_type};
use eframe::egui;
use open_bim_commands::Command;
use open_bim_model::{ObjectId, Point3, PourBreak, PourUnit, Project, PropertyValue};
use open_bim_renderer::DVec3;
use std::collections::BTreeSet;

/// Parts closer than this touch, mm.
const TOUCH: f64 = 1.0;

#[derive(Clone, Debug, PartialEq)]
pub struct PourObject {
    pub grade: String,
    pub parts: BTreeSet<ObjectId>,
    pub volume_m3: f64,
}

/// Concrete parts cast in place (not in a precast cast unit).
pub fn cast_in_place(project: &Project) -> Vec<ObjectId> {
    let precast: BTreeSet<ObjectId> = project
        .assemblies
        .values()
        .filter(|a| a.properties.get(CAST_UNIT) == Some(&PropertyValue::Text("сборный".into())))
        .flat_map(|a| a.members.iter().copied())
        .collect();
    project
        .elements
        .values()
        .filter(|e| part_type(e).is_some() && !precast.contains(&e.id))
        .map(|e| e.id)
        .collect()
}

fn grade(project: &Project, part: ObjectId) -> String {
    let m = &project.elements[&part].material;
    project.materials.get(m).map_or(String::new(), |m| {
        m.name.trim_start_matches("Бетон ").to_string()
    })
}

/// Whether a break lies between two points (opposite sides, crossing within
/// the break's length).
fn separates(b: &PourBreak, p: [f64; 3], q: [f64; 3]) -> bool {
    let (sp, sq) = (b.side(p), b.side(q));
    if sp * sq >= 0. {
        return false;
    }
    let t = sp / (sp - sq);
    let x = [p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t];
    let (a, e) = (b.start.0, b.end.0);
    let d = [e[0] - a[0], e[1] - a[1]];
    let u = ((x[0] - a[0]) * d[0] + (x[1] - a[1]) * d[1]) / (d[0] * d[0] + d[1] * d[1]);
    (0. ..=1.).contains(&u)
}

fn find(parent: &mut [usize], i: usize) -> usize {
    let mut r = i;
    while parent[r] != r {
        r = parent[r];
    }
    parent[i] = r;
    r
}

/// Pour objects, ordered bottom-up then by X and Y.
pub fn objects(project: &Project) -> Vec<PourObject> {
    let parts: Vec<(ObjectId, String, [[f64; 3]; 2])> = cast_in_place(project)
        .into_iter()
        .filter_map(|id| Some((id, grade(project, id), project.element_bounds(id).ok()?)))
        .collect();
    let centre = |b: &[[f64; 3]; 2]| std::array::from_fn(|k| (b[0][k] + b[1][k]) / 2.);
    let mut parent: Vec<usize> = (0..parts.len()).collect();
    for i in 0..parts.len() {
        for j in i + 1..parts.len() {
            let (a, b) = (&parts[i], &parts[j]);
            let touch =
                (0..3).all(|k| a.2[0][k] <= b.2[1][k] + TOUCH && b.2[0][k] <= a.2[1][k] + TOUCH);
            if a.1 != b.1 || !touch {
                continue;
            }
            let (ca, cb) = (centre(&a.2), centre(&b.2));
            if project
                .concrete
                .breaks
                .iter()
                .any(|br| separates(br, ca, cb))
            {
                continue;
            }
            let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
            parent[ri] = rj;
        }
    }
    let mut groups: std::collections::BTreeMap<usize, Vec<usize>> = Default::default();
    for i in 0..parts.len() {
        let r = find(&mut parent, i);
        groups.entry(r).or_default().push(i);
    }
    let mut out: Vec<([f64; 3], PourObject)> = groups
        .into_values()
        .map(|members| {
            let low = members.iter().fold([f64::MAX; 3], |m, &i| {
                std::array::from_fn(|k| m[k].min(parts[i].2[0][k]))
            });
            let ids: BTreeSet<ObjectId> = members.iter().map(|&i| parts[i].0).collect();
            let volume_m3 = ids
                .iter()
                .filter_map(|id| project.element_volume_mm3(*id).ok())
                .sum::<f64>()
                / 1e9;
            (
                low,
                PourObject {
                    grade: parts[members[0]].1.clone(),
                    parts: ids,
                    volume_m3,
                },
            )
        })
        .collect();
    out.sort_by(|a, b| {
        [a.0[2], a.0[0], a.0[1]]
            .partial_cmp(&[b.0[2], b.0[0], b.0[1]])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out.into_iter().map(|(_, o)| o).collect()
}

/// «Рассчитать единицы бетонирования».
pub fn calculate(project: &Project) -> Result<(Vec<Command>, usize), String> {
    let objects = objects(project);
    if objects.is_empty() {
        return Err("В модели нет монолитных железобетонных деталей".into());
    }
    let mut concrete = project.concrete.clone();
    concrete.units = objects
        .iter()
        .zip(1..)
        .map(|(o, number)| PourUnit {
            number,
            grade: o.grade.clone(),
            parts: o.parts.clone(),
        })
        .collect();
    let n = concrete.units.len();
    Ok((vec![Command::SetConcrete { concrete }], n))
}

/// «Шов бетонирования» through two picked points.
pub fn add_break(project: &Project, a: DVec3, b: DVec3) -> Result<Vec<Command>, String> {
    let mut concrete = project.concrete.clone();
    concrete.breaks.push(PourBreak {
        start: Point3(a.to_array()),
        end: Point3(b.to_array()),
    });
    concrete.validate().map_err(|e| e.to_string())?;
    Ok(vec![Command::SetConcrete { concrete }])
}

pub fn remove_break(project: &Project, index: usize) -> Result<Vec<Command>, String> {
    let mut concrete = project.concrete.clone();
    if index >= concrete.breaks.len() {
        return Err("Нет такого шва".into());
    }
    concrete.breaks.remove(index);
    Ok(vec![Command::SetConcrete { concrete }])
}

/// What the «Захватки бетонирования» window asks for.
pub enum PourAction {
    Commit(String, Vec<Command>),
    Select(BTreeSet<ObjectId>),
}

#[derive(Default)]
pub struct PoursWindow {
    pub open: bool,
    cache: Option<(u64, Vec<PourObject>)>,
}

impl PoursWindow {
    pub fn show(&mut self, ctx: &egui::Context, project: &Project) -> Option<PourAction> {
        if !self.open {
            self.cache = None;
            return None;
        }
        if self.cache.as_ref().is_none_or(|c| c.0 != project.revision) {
            self.cache = Some((project.revision, objects(project)));
        }
        let objects = &self.cache.as_ref()?.1;
        let mut action = None;
        let mut open = true;
        egui::Window::new("Захватки бетонирования")
            .id(egui::Id::new("pours"))
            .open(&mut open)
            .default_width(460.)
            .show(ctx, |ui| {
                let total: f64 = objects.iter().map(|o| o.volume_m3).sum();
                ui.label(format!(
                    "Захваток: {}, бетон {} м³",
                    objects.len(),
                    open_bim_reports::fmt((total * 100.).round() / 100.)
                ));
                egui::ScrollArea::vertical()
                    .max_height(260.)
                    .show(ui, |ui| {
                        egui::Grid::new("pour objects")
                            .num_columns(5)
                            .striped(true)
                            .show(ui, |ui| {
                                for h in ["№", "Класс", "Деталей", "Объём, м³", ""]
                                {
                                    ui.strong(h);
                                }
                                ui.end_row();
                                for (i, o) in objects.iter().enumerate() {
                                    ui.label((i + 1).to_string());
                                    ui.label(&o.grade);
                                    ui.label(o.parts.len().to_string());
                                    ui.label(format!("{:.2}", o.volume_m3));
                                    if ui.small_button("Выделить").clicked() {
                                        action = Some(PourAction::Select(o.parts.clone()));
                                    }
                                    ui.end_row();
                                }
                            });
                    });
                ui.separator();
                ui.strong(format!(
                    "Швы бетонирования: {}",
                    project.concrete.breaks.len()
                ));
                for (i, b) in project.concrete.breaks.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let f = |p: Point3| {
                            format!(
                                "({}, {})",
                                open_bim_reports::fmt(p.0[0]),
                                open_bim_reports::fmt(p.0[1])
                            )
                        };
                        ui.label(format!("{} — {}", f(b.start), f(b.end)));
                        if ui.small_button("Удалить").clicked() {
                            action = remove_break(project, i)
                                .ok()
                                .map(|c| PourAction::Commit("Удалить шов бетонирования".into(), c));
                        }
                    });
                }
                ui.separator();
                if !project.concrete.units.is_empty() {
                    ui.label(format!(
                        "Рассчитано единиц бетонирования: {}",
                        project.concrete.units.len()
                    ));
                }
                if ui.button("Рассчитать единицы бетонирования").clicked()
                {
                    action = calculate(project).ok().map(|(c, _)| {
                        PourAction::Commit("Рассчитать единицы бетонирования".into(), c)
                    });
                }
            });
        self.open = open;
        action
    }
}
