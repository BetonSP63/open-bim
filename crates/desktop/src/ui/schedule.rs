//! «Задания», «Статус проекта» and «Иерархия здания»: tasks with dates, the
//! model coloured by task state at a date, and the site, building and
//! storeys the parts belong to (exported as the IFC spatial structure).
use super::management::{ManagementUi, Outcome, assign, number_of};
use eframe::egui;
use open_bim_commands::Command;
use open_bim_model::management::{STOREY_PROPERTY, TASK_PROPERTY, day_number};
use open_bim_model::{Management, ObjectId, Project, PropertyValue, Storey, Task};
use open_bim_renderer::Appearance;
use std::collections::{BTreeMap, BTreeSet};

/// State of a part's task at a date.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskState {
    Planned,
    InWork,
    Done,
}

impl TaskState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Planned => "Запланировано",
            Self::InWork => "В работе",
            Self::Done => "Выполнено",
        }
    }
    pub fn appearance(self) -> Appearance {
        match self {
            Self::Planned => Appearance {
                color: Some([0.62, 0.62, 0.66]),
                alpha: 0.3,
            },
            Self::InWork => Appearance {
                color: Some([1.0, 0.78, 0.1]),
                alpha: 1.0,
            },
            Self::Done => Appearance {
                color: Some([0.18, 0.65, 0.25]),
                alpha: 1.0,
            },
        }
    }
}

/// Task state of every part with a task at `date` (`ГГГГ-ММ-ДД`): done
/// after the end day, in work from the start to the end day, else planned.
pub fn status_at(project: &Project, date: &str) -> Option<BTreeMap<ObjectId, TaskState>> {
    let day = day_number(date)?;
    let tasks: BTreeMap<u32, &Task> = project
        .management
        .tasks
        .iter()
        .map(|t| (t.number, t))
        .collect();
    Some(
        project
            .elements
            .keys()
            .filter_map(|id| {
                let t = tasks.get(&number_of(project, *id, TASK_PROPERTY)?)?;
                let (start, end) = (day_number(&t.start)?, day_number(&t.end)?);
                let state = if day > end {
                    TaskState::Done
                } else if day >= start {
                    TaskState::InWork
                } else {
                    TaskState::Planned
                };
                Some((*id, state))
            })
            .collect(),
    )
}

/// Parts of task or storey `number` (by the property `key`).
pub fn with_number(project: &Project, key: &str, number: u32) -> BTreeSet<ObjectId> {
    project
        .elements
        .keys()
        .copied()
        .filter(|id| number_of(project, *id, key) == Some(number))
        .collect()
}

pub fn add_task(m: &Management, name: &str, start: &str, end: &str) -> Result<Management, String> {
    let mut m = m.clone();
    let number = m.tasks.iter().map(|t| t.number).max().unwrap_or(0) + 1;
    let name = name.trim();
    m.tasks.push(Task {
        number,
        name: if name.is_empty() {
            format!("Задание {number}")
        } else {
            name.into()
        },
        start: start.trim().into(),
        end: end.trim().into(),
    });
    m.validate().map_err(|e| e.to_string())?;
    Ok(m)
}

pub fn add_storey(m: &Management, name: &str, elevation: &str) -> Result<Management, String> {
    let mut m = m.clone();
    let elevation_mm = crate::tools::number(elevation)?;
    let number = m
        .hierarchy
        .storeys
        .iter()
        .map(|s| s.number)
        .max()
        .unwrap_or(0)
        + 1;
    let name = name.trim();
    m.hierarchy.storeys.push(Storey {
        number,
        name: if name.is_empty() {
            format!("Этаж {number}")
        } else {
            name.into()
        },
        elevation_mm,
    });
    m.hierarchy
        .storeys
        .sort_by(|a, b| a.elevation_mm.total_cmp(&b.elevation_mm));
    m.validate().map_err(|e| e.to_string())?;
    Ok(m)
}

/// Remove a task or storey and its number from the parts, one step.
fn remove_numbered(project: &Project, key: &str, number: u32, m: Management) -> Vec<Command> {
    let mut out = assign(project, &with_number(project, key, number), key, None);
    out.push(Command::SetManagement { management: m });
    out
}

impl ManagementUi {
    pub(super) fn tasks(
        &mut self,
        ui: &mut egui::Ui,
        project: &Project,
        selected: &BTreeSet<ObjectId>,
    ) -> Option<Outcome> {
        let m = &project.management;
        let mut out = None;
        egui::Grid::new("tasks")
            .striped(true)
            .num_columns(5)
            .show(ui, |ui| {
                for h in ["Номер", "Имя", "Начало", "Окончание", "Деталей"]
                {
                    ui.strong(h);
                }
                ui.end_row();
                for t in &m.tasks {
                    let chosen = self.chosen == Some(t.number);
                    if ui.selectable_label(chosen, t.number.to_string()).clicked()
                        || ui.selectable_label(chosen, &t.name).clicked()
                    {
                        self.chosen = Some(t.number);
                    }
                    ui.label(&t.start);
                    ui.label(&t.end);
                    ui.label(
                        with_number(project, TASK_PROPERTY, t.number)
                            .len()
                            .to_string(),
                    );
                    ui.end_row();
                }
            });
        egui::Grid::new("new task").num_columns(2).show(ui, |ui| {
            for (label, value) in [
                ("Имя", &mut self.name),
                ("Начало (ГГГГ-ММ-ДД)", &mut self.task_start),
                ("Окончание", &mut self.task_end),
            ] {
                let l = ui.label(label);
                ui.add(egui::TextEdit::singleline(value).desired_width(140.))
                    .labelled_by(l.id);
                ui.end_row();
            }
        });
        let chosen = self
            .chosen
            .filter(|n| m.tasks.iter().any(|t| t.number == *n));
        ui.horizontal_wrapped(|ui| {
            if ui.button("Добавить").clicked() {
                match add_task(m, &self.name, &self.task_start, &self.task_end) {
                    Ok(next) => {
                        self.name.clear();
                        out = Some(Outcome::Commit(
                            "Добавить задание".into(),
                            vec![Command::SetManagement { management: next }],
                        ));
                    }
                    Err(e) => self.message = Some(e),
                }
            }
            out = out.take().or(self.numbered_buttons(
                ui,
                project,
                selected,
                chosen,
                TASK_PROPERTY,
                "задание",
                |m, n| m.tasks.retain(|t| t.number != n),
            ));
        });
        out
    }

    /// «Назначить выбранным», «Выбрать объекты», «Удалить» for the chosen row.
    #[allow(clippy::too_many_arguments)]
    fn numbered_buttons(
        &mut self,
        ui: &mut egui::Ui,
        project: &Project,
        selected: &BTreeSet<ObjectId>,
        chosen: Option<u32>,
        key: &str,
        what: &str,
        remove: impl FnOnce(&mut Management, u32),
    ) -> Option<Outcome> {
        let mut out = None;
        if ui
            .add_enabled(
                chosen.is_some() && !selected.is_empty(),
                egui::Button::new("Назначить выбранным"),
            )
            .clicked()
            && let Some(n) = chosen
        {
            let commands = assign(
                project,
                selected,
                key,
                Some(PropertyValue::Number(n as f64)),
            );
            if commands.is_empty() {
                self.message = Some("Нечего менять: детали уже там или заблокированы".into());
            } else {
                self.message = Some(format!("Назначено деталей: {}", commands.len()));
                out = Some(Outcome::Commit(format!("Назначить {what}"), commands));
            }
        }
        if ui
            .add_enabled(chosen.is_some(), egui::Button::new("Выбрать объекты"))
            .clicked()
            && let Some(n) = chosen
        {
            out = Some(Outcome::Select(with_number(project, key, n)));
        }
        if ui
            .add_enabled(chosen.is_some(), egui::Button::new("Удалить"))
            .clicked()
            && let Some(n) = chosen
        {
            let mut m = project.management.clone();
            remove(&mut m, n);
            self.chosen = None;
            out = Some(Outcome::Commit(
                format!("Удалить {what}"),
                remove_numbered(project, key, n, m),
            ));
        }
        out
    }

    pub(super) fn status(&mut self, ui: &mut egui::Ui, project: &Project) -> Option<Outcome> {
        if self.status_date.is_empty() {
            self.status_date = project
                .management
                .tasks
                .iter()
                .map(|t| t.start.clone())
                .min()
                .unwrap_or_default();
        }
        let before = (self.status_date.clone(), self.status_on);
        ui.horizontal(|ui| {
            let l = ui.label("Дата (ГГГГ-ММ-ДД)");
            ui.add(egui::TextEdit::singleline(&mut self.status_date).desired_width(110.))
                .labelled_by(l.id);
            ui.checkbox(&mut self.status_on, "Показать в модели");
        });
        match status_at(project, &self.status_date) {
            None => {
                ui.colored_label(ui.visuals().warn_fg_color, "Введите дату в виде ГГГГ-ММ-ДД");
            }
            Some(states) => {
                for state in [TaskState::Done, TaskState::InWork, TaskState::Planned] {
                    let n = states.values().filter(|s| **s == state).count();
                    ui.horizontal(|ui| {
                        let c = state.appearance().color.unwrap_or([1.; 3]);
                        let (r, _) =
                            ui.allocate_exact_size(egui::vec2(14., 14.), egui::Sense::hover());
                        ui.painter().rect_filled(
                            r,
                            2.,
                            egui::Color32::from_rgb(
                                (c[0] * 255.) as u8,
                                (c[1] * 255.) as u8,
                                (c[2] * 255.) as u8,
                            ),
                        );
                        ui.label(format!("{}: {n}", state.label()));
                    });
                }
                ui.small("Детали без задания показаны как обычно");
            }
        }
        if (self.status_date.clone(), self.status_on) != before {
            self.status_changed = true;
        }
        None
    }

    /// Colours of the project status when it is shown.
    pub fn status_colours(&self, project: &Project) -> Option<BTreeMap<ObjectId, Appearance>> {
        if !self.status_on {
            return None;
        }
        Some(
            status_at(project, &self.status_date)?
                .into_iter()
                .map(|(id, s)| (id, s.appearance()))
                .collect(),
        )
    }

    pub(super) fn hierarchy(
        &mut self,
        ui: &mut egui::Ui,
        project: &Project,
        selected: &BTreeSet<ObjectId>,
    ) -> Option<Outcome> {
        let m = &project.management;
        let mut out = None;
        let names = self
            .spatial_names
            .get_or_insert_with(|| (m.hierarchy.site.clone(), m.hierarchy.building.clone()));
        egui::Grid::new("hierarchy names")
            .num_columns(2)
            .show(ui, |ui| {
                let l = ui.label("Площадка");
                ui.add(egui::TextEdit::singleline(&mut names.0).desired_width(200.))
                    .labelled_by(l.id);
                ui.end_row();
                let l = ui.label("Здание");
                ui.add(egui::TextEdit::singleline(&mut names.1).desired_width(200.))
                    .labelled_by(l.id);
                ui.end_row();
            });
        let changed = *names != (m.hierarchy.site.clone(), m.hierarchy.building.clone());
        if ui
            .add_enabled(changed, egui::Button::new("Применить имена"))
            .clicked()
        {
            let mut next = m.clone();
            (next.hierarchy.site, next.hierarchy.building) = names.clone();
            match next.validate() {
                Ok(()) => {
                    self.spatial_names = None;
                    out = Some(Outcome::Commit(
                        "Иерархия здания".into(),
                        vec![Command::SetManagement { management: next }],
                    ));
                }
                Err(e) => self.message = Some(e.to_string()),
            }
        }
        ui.strong("Этажи");
        egui::Grid::new("storeys")
            .striped(true)
            .num_columns(3)
            .show(ui, |ui| {
                for s in m.hierarchy.storeys.iter().rev() {
                    let chosen = self.chosen == Some(s.number);
                    if ui.selectable_label(chosen, &s.name).clicked() {
                        self.chosen = Some(s.number);
                    }
                    ui.label(format!("{:+.0} мм", s.elevation_mm));
                    ui.label(format!(
                        "деталей {}",
                        with_number(project, STOREY_PROPERTY, s.number).len()
                    ));
                    ui.end_row();
                }
            });
        if m.hierarchy.storeys.is_empty() {
            ui.weak("Этажей нет: все детали в «Уровень 0»");
        }
        ui.horizontal(|ui| {
            let l = ui.label("Имя");
            ui.add(egui::TextEdit::singleline(&mut self.name).desired_width(110.))
                .labelled_by(l.id);
            let l = ui.label("Отметка, мм");
            ui.add(egui::TextEdit::singleline(&mut self.storey_elevation).desired_width(70.))
                .labelled_by(l.id);
            if ui.button("Добавить этаж").clicked() {
                match add_storey(m, &self.name, &self.storey_elevation) {
                    Ok(next) => {
                        self.name.clear();
                        out = Some(Outcome::Commit(
                            "Добавить этаж".into(),
                            vec![Command::SetManagement { management: next }],
                        ));
                    }
                    Err(e) => self.message = Some(e),
                }
            }
        });
        let chosen = self
            .chosen
            .filter(|n| m.hierarchy.storeys.iter().any(|s| s.number == *n));
        ui.horizontal_wrapped(|ui| {
            out = out.take().or(self.numbered_buttons(
                ui,
                project,
                selected,
                chosen,
                STOREY_PROPERTY,
                "этаж",
                |m, n| m.hierarchy.storeys.retain(|s| s.number != n),
            ));
        });
        ui.small("Этажи уходят в IFC как IfcBuildingStorey с деталями");
        out
    }
}
