//! «Бетонирование»: pour zones are the connected cast-in-place concrete
//! parts of one class between construction joints (a joint cutting a part
//! splits its volume); «Рассчитать единицы бетонирования» stores them as
//! numbered pour units so the numbers survive later edits.
use eframe::egui;
use open_bim_commands::Command;
use open_bim_model::{ObjectId, Point3, PourBreak, PourZone, Project};
use open_bim_renderer::DVec3;
use std::collections::BTreeSet;

/// Pour zones of the model, numbered by the stored pour units.
pub fn objects(project: &Project) -> Vec<PourZone> {
    project.pour_zones()
}

/// Concrete parts cast in place (not in a precast cast unit).
pub fn cast_in_place(project: &Project) -> Vec<ObjectId> {
    project.cast_in_place()
}

/// «Рассчитать единицы бетонирования».
pub fn calculate(project: &Project) -> Result<(Vec<Command>, usize), String> {
    let units = project.pour_units();
    if units.is_empty() {
        return Err("В модели нет монолитных железобетонных деталей".into());
    }
    let mut concrete = project.concrete.clone();
    let n = units.len();
    concrete.units = units;
    Ok((vec![Command::SetConcrete { concrete }], n))
}

/// «Шов бетонирования» through two picked points; a horizontal joint lies
/// at the level of the first point over the plan rectangle of the two.
pub fn add_break(
    project: &Project,
    a: DVec3,
    b: DVec3,
    horizontal: bool,
) -> Result<Vec<Command>, String> {
    let mut concrete = project.concrete.clone();
    let mut end = b.to_array();
    if horizontal {
        end[2] = a.z;
    }
    concrete.breaks.push(PourBreak {
        start: Point3(a.to_array()),
        end: Point3(end),
        horizontal,
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

/// Saves «Ведомость объёмов бетона по захваткам» as CSV.
pub fn save_schedule(project: &Project, path: &std::path::Path) -> Result<(), String> {
    std::fs::write(path, open_bim_reports::csv::pours(project)).map_err(|e| e.to_string())
}

/// What the «Захватки бетонирования» window asks for.
pub enum PourAction {
    Commit(String, Vec<Command>),
    Select(BTreeSet<ObjectId>),
    Message(String),
}

#[derive(Default)]
pub struct PoursWindow {
    pub open: bool,
    cache: Option<(u64, Vec<PourZone>, bool)>,
}

impl PoursWindow {
    pub fn show(&mut self, ctx: &egui::Context, project: &Project) -> Option<PourAction> {
        if !self.open {
            self.cache = None;
            return None;
        }
        if self.cache.as_ref().is_none_or(|c| c.0 != project.revision) {
            self.cache = Some((
                project.revision,
                objects(project),
                project.pour_units_outdated(),
            ));
        }
        let (_, objects, outdated) = self.cache.as_ref()?;
        let outdated = *outdated;
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
                                for h in ["Захватка", "Класс", "Деталей", "Объём, м³", ""]
                                {
                                    ui.strong(h);
                                }
                                ui.end_row();
                                for o in objects {
                                    ui.label(o.number.to_string());
                                    ui.label(&o.grade);
                                    let split = o.pieces.iter().filter(|p| p.split).count();
                                    ui.label(if split > 0 {
                                        format!("{} (из них частей {split})", o.pieces.len())
                                    } else {
                                        o.pieces.len().to_string()
                                    });
                                    ui.label(format!("{:.2}", o.volume_m3));
                                    if ui.small_button("Выделить").clicked() {
                                        action = Some(PourAction::Select(o.parts()));
                                    }
                                    ui.end_row();
                                }
                            });
                    });
                for (grade, v) in open_bim_model::pours::volumes_by_grade(objects) {
                    ui.label(format!("Бетон {grade}: {v:.2} м³"));
                }
                if ui.button("Ведомость объёмов в CSV…").clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("CSV", &["csv"])
                        .set_file_name("объёмы-бетона.csv")
                        .save_file()
                {
                    action = Some(match save_schedule(project, &path) {
                        Ok(()) => PourAction::Message(format!("Ведомость: {}", path.display())),
                        Err(e) => PourAction::Message(e),
                    });
                }
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
                        let kind = if b.horizontal {
                            format!(
                                "Горизонтальный, отм. {}",
                                open_bim_reports::fmt(b.start.0[2])
                            )
                        } else {
                            "Вертикальный".into()
                        };
                        ui.label(format!("{kind}: {} — {}", f(b.start), f(b.end)));
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
                    if outdated {
                        ui.colored_label(
                            ui.visuals().warn_fg_color,
                            "Модель изменилась: пересчитайте единицы, номера захваток сохранятся",
                        );
                    }
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
