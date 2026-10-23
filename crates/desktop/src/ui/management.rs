//! «Управление»: phases («Стадии»), lots («Партии»), sequences
//! («Последовательности») and locks («Блокировки»). The lists live in the
//! project; each part keeps its phase, lot, sequence numbers and lock as
//! properties, so selection filters and object groups can use them.
use eframe::egui;
use open_bim_commands::Command;
use open_bim_model::management::{LOCK_PROPERTY, LOT_PROPERTY, PHASE_PROPERTY, sequence_key};
use open_bim_model::{ListEntry, Management, ObjectId, Project, PropertyValue};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Window {
    Phases,
    Lots,
    Sequences,
    Locks,
}

impl Window {
    pub fn title(self) -> &'static str {
        match self {
            Self::Phases => "Стадии",
            Self::Lots => "Партии",
            Self::Sequences => "Последовательности",
            Self::Locks => "Блокировки",
        }
    }
}

/// What a window asks the workbench to do.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    Commit(String, Vec<Command>),
    Select(BTreeSet<ObjectId>),
    /// Start picking parts in order for the sequence being numbered.
    PickSequence,
}

#[derive(Default)]
pub struct ManagementUi {
    pub open: Option<Window>,
    /// Row chosen in the phase or lot list (its number).
    pub chosen: Option<u32>,
    /// Name of a new phase, lot or sequence.
    pub name: String,
    /// Sequence being numbered by picks: its name and the next number.
    pub picking: Option<(String, u32)>,
    pub message: Option<String>,
}

/// Phase of a part; a part without one is in phase 1.
pub fn phase_of(project: &Project, id: ObjectId) -> Option<u32> {
    let e = project.elements.get(&id)?;
    Some(match e.properties.get(PHASE_PROPERTY) {
        Some(PropertyValue::Number(n)) => *n as u32,
        _ => 1,
    })
}

fn number_of(project: &Project, id: ObjectId, key: &str) -> Option<u32> {
    match project.elements.get(&id)?.properties.get(key) {
        Some(PropertyValue::Number(n)) => Some(*n as u32),
        _ => None,
    }
}

pub fn is_locked(project: &Project, id: ObjectId) -> bool {
    project
        .elements
        .get(&id)
        .is_some_and(|e| e.properties.get(LOCK_PROPERTY) == Some(&PropertyValue::Boolean(true)))
}

/// Parts in phase (`lots` false) or lot (`lots` true) `number`.
pub fn members(project: &Project, lots: bool, number: u32) -> BTreeSet<ObjectId> {
    project
        .elements
        .keys()
        .copied()
        .filter(|id| {
            if lots {
                number_of(project, *id, LOT_PROPERTY) == Some(number)
            } else {
                phase_of(project, *id) == Some(number)
            }
        })
        .collect()
}

/// Set (or with `None` remove) a property on the selected parts; parts that
/// already have the value are left out. Locked parts are refused by the
/// document, so they are left out here too, except for the lock itself.
pub fn assign(
    project: &Project,
    ids: &BTreeSet<ObjectId>,
    key: &str,
    value: Option<PropertyValue>,
) -> Vec<Command> {
    ids.iter()
        .filter_map(|id| project.elements.get(id))
        .filter(|e| key == LOCK_PROPERTY || !is_locked(project, e.id))
        .filter(|e| e.properties.get(key) != value.as_ref())
        .map(|e| Command::ChangeProperty {
            id: e.id,
            key: key.into(),
            value: value.clone(),
        })
        .collect()
}

/// Parts of a sequence in order of their numbers.
pub fn sequence(project: &Project, name: &str) -> Vec<(u32, ObjectId)> {
    let key = sequence_key(name);
    let mut out: Vec<(u32, ObjectId)> = project
        .elements
        .keys()
        .filter_map(|id| Some((number_of(project, *id, &key)?, *id)))
        .collect();
    out.sort();
    out
}

/// Delete a sequence: its name and the numbers on the parts, one step.
pub fn remove_sequence(project: &Project, name: &str) -> Vec<Command> {
    let key = sequence_key(name);
    let mut m = project.management.clone();
    m.sequences.retain(|s| s != name);
    let ids: BTreeSet<ObjectId> = sequence(project, name).into_iter().map(|p| p.1).collect();
    let mut out = assign(project, &ids, &key, None);
    out.push(Command::SetManagement { management: m });
    out
}

fn next_number(list: &[ListEntry]) -> u32 {
    list.iter().map(|s| s.number).max().unwrap_or(0) + 1
}

/// Add a phase or lot named `name` with the next free number.
pub fn add_entry(m: &Management, lots: bool, name: &str) -> Result<Management, String> {
    let mut m = m.clone();
    let list = if lots { &mut m.lots } else { &mut m.phases };
    let name = name.trim();
    let what = if lots { "Партия" } else { "Стадия" };
    let number = next_number(list);
    list.push(ListEntry {
        number,
        name: if name.is_empty() {
            format!("{what} {number}")
        } else {
            name.into()
        },
        comment: String::new(),
    });
    m.validate().map_err(|e| e.to_string())?;
    Ok(m)
}

/// Remove a phase or lot no part uses (the current phase stays).
pub fn remove_entry(project: &Project, lots: bool, number: u32) -> Result<Vec<Command>, String> {
    let used = members(project, lots, number).len();
    if used > 0 {
        return Err(format!(
            "Используется деталями: {used}. Сначала назначьте им другую"
        ));
    }
    let mut m = project.management.clone();
    if !lots && m.current_phase == number {
        return Err("Текущую стадию удалить нельзя".into());
    }
    if lots {
        m.lots.retain(|s| s.number != number);
    } else {
        m.phases.retain(|s| s.number != number);
    }
    Ok(vec![Command::SetManagement { management: m }])
}

impl ManagementUi {
    pub fn toggle(&mut self, window: Window) {
        self.open = if self.open == Some(window) {
            None
        } else {
            Some(window)
        };
        self.chosen = None;
        self.message = None;
    }

    pub fn picks_objects(&self) -> bool {
        self.picking.is_some()
    }

    pub fn prompt(&self) -> Option<String> {
        let (name, next) = self.picking.as_ref()?;
        Some(format!(
            "Последовательность «{name}»: укажите деталь № {next} (Esc — закончить)"
        ))
    }

    pub fn cancel(&mut self) -> bool {
        self.picking.take().is_some()
    }

    /// A picked part gets the next number of the sequence.
    pub fn pick(&mut self, project: &Project, id: ObjectId) -> Option<Outcome> {
        let (name, next) = self.picking.as_mut()?;
        let key = sequence_key(name);
        let ids = BTreeSet::from([id]);
        let commands = assign(
            project,
            &ids,
            &key,
            Some(PropertyValue::Number(*next as f64)),
        );
        if commands.is_empty() {
            self.message = Some("Деталь уже имеет этот номер или заблокирована".into());
            return None;
        }
        let label = format!("Последовательность «{name}»: {next}");
        *next += 1;
        Some(Outcome::Commit(label, commands))
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        project: &Project,
        selected: &BTreeSet<ObjectId>,
    ) -> Option<Outcome> {
        let window = self.open?;
        let mut open = true;
        let mut out = None;
        egui::Window::new(window.title())
            .id(egui::Id::new(("management window", window.title())))
            .open(&mut open)
            .collapsible(false)
            .default_width(380.)
            .show(ctx, |ui| {
                out = match window {
                    Window::Phases => self.list(ui, project, selected, false),
                    Window::Lots => self.list(ui, project, selected, true),
                    Window::Sequences => self.sequences(ui, project),
                    Window::Locks => self.locks(ui, project, selected),
                };
                if let Some(m) = &self.message {
                    ui.separator();
                    ui.label(m);
                }
            });
        if !open {
            self.open = None;
            self.picking = None;
        }
        out
    }

    fn list(
        &mut self,
        ui: &mut egui::Ui,
        project: &Project,
        selected: &BTreeSet<ObjectId>,
        lots: bool,
    ) -> Option<Outcome> {
        let m = &project.management;
        let entries = if lots { &m.lots } else { &m.phases };
        let mut out = None;
        egui::Grid::new(("management list", lots))
            .striped(true)
            .num_columns(4)
            .show(ui, |ui| {
                ui.strong("Номер");
                ui.strong("Имя");
                ui.strong("Деталей");
                ui.strong(if lots { "" } else { "Текущая" });
                ui.end_row();
                for s in entries {
                    let chosen = self.chosen == Some(s.number);
                    if ui.selectable_label(chosen, s.number.to_string()).clicked() {
                        self.chosen = Some(s.number);
                    }
                    if ui.selectable_label(chosen, &s.name).clicked() {
                        self.chosen = Some(s.number);
                    }
                    ui.label(members(project, lots, s.number).len().to_string());
                    if !lots {
                        let current = m.current_phase == s.number;
                        if ui.radio(current, "").clicked() && !current {
                            let mut next = m.clone();
                            next.current_phase = s.number;
                            out = Some(Outcome::Commit(
                                format!("Текущая стадия: {}", s.name),
                                vec![Command::SetManagement { management: next }],
                            ));
                        }
                    } else {
                        ui.label("");
                    }
                    ui.end_row();
                }
            });
        ui.horizontal(|ui| {
            let l = ui.label("Имя");
            ui.add(egui::TextEdit::singleline(&mut self.name).desired_width(160.))
                .labelled_by(l.id);
            if ui.button("Добавить").clicked() {
                match add_entry(m, lots, &self.name) {
                    Ok(next) => {
                        self.name.clear();
                        out = Some(Outcome::Commit(
                            if lots {
                                "Добавить партию"
                            } else {
                                "Добавить стадию"
                            }
                            .into(),
                            vec![Command::SetManagement { management: next }],
                        ));
                    }
                    Err(e) => self.message = Some(e),
                }
            }
        });
        let chosen = self
            .chosen
            .filter(|n| entries.iter().any(|s| s.number == *n));
        let key = if lots { LOT_PROPERTY } else { PHASE_PROPERTY };
        ui.horizontal_wrapped(|ui| {
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
                    out = Some(Outcome::Commit(
                        if lots {
                            "Назначить партию"
                        } else {
                            "Назначить стадию"
                        }
                        .into(),
                        commands,
                    ));
                }
            }
            if ui
                .add_enabled(chosen.is_some(), egui::Button::new("Выбрать объекты"))
                .clicked()
                && let Some(n) = chosen
            {
                out = Some(Outcome::Select(members(project, lots, n)));
            }
            if ui
                .add_enabled(chosen.is_some(), egui::Button::new("Удалить"))
                .clicked()
                && let Some(n) = chosen
            {
                match remove_entry(project, lots, n) {
                    Ok(commands) => {
                        self.chosen = None;
                        out = Some(Outcome::Commit(
                            if lots {
                                "Удалить партию"
                            } else {
                                "Удалить стадию"
                            }
                            .into(),
                            commands,
                        ));
                    }
                    Err(e) => self.message = Some(e),
                }
            }
        });
        out
    }

    fn sequences(&mut self, ui: &mut egui::Ui, project: &Project) -> Option<Outcome> {
        let mut out = None;
        let names = &project.management.sequences;
        for name in names {
            ui.horizontal(|ui| {
                let parts = sequence(project, name);
                ui.label(format!("{name}: деталей {}", parts.len()));
                if ui.button("Продолжить").clicked() {
                    let next = parts.last().map_or(1, |p| p.0 + 1);
                    self.picking = Some((name.clone(), next));
                    out = Some(Outcome::PickSequence);
                }
                if ui.button("Выбрать").clicked() {
                    out = Some(Outcome::Select(parts.iter().map(|p| p.1).collect()));
                }
                if ui.button("Удалить").clicked() {
                    out = Some(Outcome::Commit(
                        format!("Удалить последовательность «{name}»"),
                        remove_sequence(project, name),
                    ));
                }
            });
        }
        if names.is_empty() {
            ui.weak("Последовательностей нет");
        }
        ui.separator();
        ui.horizontal(|ui| {
            let l = ui.label("Имя");
            ui.add(egui::TextEdit::singleline(&mut self.name).desired_width(160.))
                .labelled_by(l.id);
            if ui.button("Создать и указать детали").clicked() {
                let name = self.name.trim().to_string();
                let mut m = project.management.clone();
                m.sequences.push(name.clone());
                match m.validate() {
                    Ok(()) => {
                        self.name.clear();
                        self.picking = Some((name.clone(), 1));
                        out = Some(Outcome::Commit(
                            format!("Создать последовательность «{name}»"),
                            vec![Command::SetManagement { management: m }],
                        ));
                    }
                    Err(e) => self.message = Some(e.to_string()),
                }
            }
        });
        ui.small("Детали получают номера 1, 2, 3… в порядке указания");
        out
    }

    fn locks(
        &mut self,
        ui: &mut egui::Ui,
        project: &Project,
        selected: &BTreeSet<ObjectId>,
    ) -> Option<Outcome> {
        let mut out = None;
        let locked: BTreeSet<ObjectId> = project
            .elements
            .keys()
            .copied()
            .filter(|id| is_locked(project, *id))
            .collect();
        ui.label(format!("Заблокировано деталей: {}", locked.len()));
        ui.small("Заблокированные детали нельзя изменить или удалить");
        ui.horizontal_wrapped(|ui| {
            for (label, lock) in [
                ("Заблокировать выбранные", true),
                ("Разблокировать выбранные", false),
            ] {
                if ui
                    .add_enabled(!selected.is_empty(), egui::Button::new(label))
                    .clicked()
                {
                    let commands = assign(
                        project,
                        selected,
                        LOCK_PROPERTY,
                        lock.then_some(PropertyValue::Boolean(true)),
                    );
                    if commands.is_empty() {
                        self.message = Some("Нечего менять".into());
                    } else {
                        self.message = Some(format!("Деталей: {}", commands.len()));
                        out = Some(Outcome::Commit(label.into(), commands));
                    }
                }
            }
            if ui
                .add_enabled(
                    !locked.is_empty(),
                    egui::Button::new("Выбрать заблокированные"),
                )
                .clicked()
            {
                out = Some(Outcome::Select(locked.clone()));
            }
        });
        out
    }
}
