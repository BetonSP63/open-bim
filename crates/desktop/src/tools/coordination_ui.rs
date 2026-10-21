//! Numbering and clash report panel. Rows select the referenced parts.
use super::Editor;
use eframe::egui;
use open_bim_commands::Command;
use open_bim_coordination::{Conflict, ConflictKind, Report};
use open_bim_model::{DetailKind, ObjectId, Project, PropertyValue};
use open_bim_numbering::Numbering;
use std::collections::BTreeSet;

pub struct CoordinationUi {
    pub numbering: Option<Numbering>,
    pub report: Option<Report>,
    pub selected_row: Option<usize>,
    pub show_informative: bool,
    pub message: Option<String>,
    /// Set by the panel button; the workbench starts the background check.
    pub requested: bool,
    /// The next run ignores the stored marks («Перенумеровать всё»).
    pub renumber_all: bool,
    /// Series fields: part prefix and start, assembly prefix and start.
    pub series: [String; 4],
    /// (revision, objects whose mark changed since the last run).
    outdated: Option<(u64, usize)>,
}

pub fn kind_label(kind: ConflictKind) -> &'static str {
    match kind {
        ConflictKind::Intersection => "Пересечение",
        ConflictKind::Touching => "Касание",
        ConflictKind::AllowedBoltHole => "Болт в отверстии",
    }
}

/// Elements highlighted by a row: both parts, or for a bolt row the bolted part
/// plus every part of the bolt group.
pub fn row_objects(project: &Project, conflict: &Conflict) -> BTreeSet<ObjectId> {
    let mut ids = BTreeSet::from([conflict.second]);
    match project.details.get(&conflict.first).map(|d| &d.kind) {
        Some(DetailKind::BoltGroup { parts, .. }) => ids.extend(parts),
        _ => {
            ids.insert(conflict.first);
        }
    }
    ids.retain(|id| project.elements.contains_key(id));
    ids
}

impl Default for CoordinationUi {
    fn default() -> Self {
        Self {
            numbering: None,
            report: None,
            selected_row: None,
            show_informative: false,
            message: None,
            requested: false,
            renumber_all: false,
            series: [String::new(), "1".into(), String::new(), "1".into()],
            outdated: None,
        }
    }
}

/// Property commands that put the parts and the assemblies of `ids` into a
/// series; an empty prefix with start 1 removes the series (default one).
pub fn series_commands(
    project: &Project,
    ids: &BTreeSet<ObjectId>,
    part: Option<(&str, f64)>,
    assembly: Option<(&str, f64)>,
) -> Vec<Command> {
    use open_bim_numbering::{SERIES_PREFIX, SERIES_START};
    let set = |id: ObjectId, (prefix, start): (&str, f64)| {
        let default = prefix.trim().is_empty() && start == 1.;
        [
            Command::ChangeProperty {
                id,
                key: SERIES_PREFIX.into(),
                value: (!default).then(|| PropertyValue::Text(prefix.trim().into())),
            },
            Command::ChangeProperty {
                id,
                key: SERIES_START.into(),
                value: (!default).then_some(PropertyValue::Number(start)),
            },
        ]
    };
    let mut commands = Vec::new();
    if let Some(series) = part {
        for id in ids.iter().filter(|id| project.elements.contains_key(id)) {
            commands.extend(set(*id, series));
        }
    }
    if let Some(series) = assembly {
        for a in project
            .assemblies
            .values()
            .filter(|a| ids.contains(&a.id) || a.members.iter().any(|m| ids.contains(m)))
        {
            commands.extend(set(a.id, series));
        }
    }
    commands
}

impl CoordinationUi {
    pub fn run(&mut self, project: &Project) -> Result<(), String> {
        let (numbering, report) = Self::compute(project, false)?;
        self.accept(numbering, report);
        Ok(())
    }

    /// Pure computation, safe to run on a worker thread. `fresh` numbers
    /// from scratch; otherwise stored marks are kept where possible.
    pub fn compute(project: &Project, fresh: bool) -> Result<(Numbering, Report), String> {
        let numbering = if fresh {
            open_bim_numbering::renumber_all(project)
        } else {
            open_bim_numbering::number(project)
        }
        .map_err(|e| e.to_string())?;
        let report =
            open_bim_coordination::check(project, &numbering).map_err(|e| e.to_string())?;
        Ok((numbering, report))
    }

    pub fn accept(&mut self, numbering: Numbering, report: Report) {
        self.message = Some(format!(
            "Позиций деталей: {}, сборок: {}; кандидатов {}, пересечений {}",
            numbering.parts.values().collect::<BTreeSet<_>>().len(),
            numbering.assemblies.values().collect::<BTreeSet<_>>().len(),
            report.candidates,
            report.blocking().count()
        ));
        self.numbering = Some(numbering);
        self.report = Some(report);
        self.selected_row = None;
    }

    /// Fix the marks of the accepted run in the model («Нумеровать»), so
    /// later runs keep them. No transaction when nothing changed.
    pub fn store(&mut self, editor: &mut super::Editor) -> Result<(), String> {
        let Some(numbering) = &self.numbering else {
            return Ok(());
        };
        let marks = open_bim_numbering::stored_marks(numbering);
        if marks == editor.document.project().numbering {
            return Ok(());
        }
        let fresh_report = self
            .report
            .as_ref()
            .is_some_and(|r| r.revision == editor.document.project().revision);
        editor.commit_data("Нумерация", &[Command::SetNumbering { marks }])?;
        // Only the stored marks changed: the clash report stays current.
        if fresh_report && let Some(report) = &mut self.report {
            report.revision = editor.document.project().revision;
        }
        self.outdated = None;
        Ok(())
    }

    /// Objects whose mark would change at the next run (cached per revision).
    pub fn outdated_count(&mut self, project: &Project) -> usize {
        if let Some((revision, count)) = self.outdated
            && revision == project.revision
        {
            return count;
        }
        let count = open_bim_numbering::number(project)
            .map(|n| open_bim_numbering::outdated(project, &n).len())
            .unwrap_or(0);
        self.outdated = Some((project.revision, count));
        count
    }

    pub fn stale(&self, project: &Project) -> bool {
        self.report
            .as_ref()
            .is_some_and(|r| r.revision != project.revision)
    }

    pub fn visible_rows(&self) -> Vec<(usize, &Conflict)> {
        self.report
            .iter()
            .flat_map(|r| r.conflicts.iter().enumerate())
            .filter(|(_, c)| self.show_informative || c.kind == ConflictKind::Intersection)
            .collect()
    }

    pub fn select_row(&mut self, row: usize, editor: &mut Editor) -> Result<(), String> {
        let conflict = self
            .report
            .as_ref()
            .and_then(|r| r.conflicts.get(row))
            .ok_or("Строка отчёта не найдена")?
            .clone();
        let ids = row_objects(editor.document.project(), &conflict);
        if ids.is_empty() {
            return Err("Объекты строки удалены; повторите проверку".into());
        }
        editor.selected = ids;
        self.selected_row = Some(row);
        editor.record(&format!(
            "Отчёт коллизий: строка {} ({} #{} / #{})",
            row + 1,
            kind_label(conflict.kind),
            conflict.first.0,
            conflict.second.0
        ));
        Ok(())
    }

    /// «Серия нумерации» of the selected parts and their assemblies.
    fn series_section(&mut self, ui: &mut egui::Ui, editor: &mut Editor) {
        ui.collapsing("Серии нумерации", |ui| {
            ui.small(
                "Детали и сборки сравниваются и нумеруются внутри своей серии: \
                 префикс и начальный номер (П с 101 → П101, П102…).",
            );
            egui::Grid::new("numbering series")
                .num_columns(3)
                .show(ui, |ui| {
                    ui.label("");
                    ui.label("Префикс");
                    ui.label("Начало");
                    ui.end_row();
                    for (row, name) in ["Детали", "Сборки"].into_iter().enumerate() {
                        ui.label(name);
                        let [prefix, start] = &mut self.series[row * 2..row * 2 + 2] else {
                            unreachable!()
                        };
                        ui.add(egui::TextEdit::singleline(prefix).desired_width(60.))
                            .widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::WidgetType::TextEdit,
                                    true,
                                    format!("{name}: префикс серии"),
                                )
                            });
                        ui.add(egui::TextEdit::singleline(start).desired_width(50.))
                            .widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::WidgetType::TextEdit,
                                    true,
                                    format!("{name}: начальный номер"),
                                )
                            });
                        ui.end_row();
                    }
                });
            let parse = |text: &str| -> Result<f64, String> {
                let n = super::number(&text.replace(',', "."))?;
                if n < 1. || n.fract() != 0. {
                    return Err("Начальный номер: целое число от 1".into());
                }
                Ok(n)
            };
            let mut apply = None;
            ui.horizontal(|ui| {
                if ui
                    .button("Назначить выбранным")
                    .on_hover_text("Выбранным деталям и их сборкам")
                    .clicked()
                {
                    apply = Some(false);
                }
                if ui.button("Сбросить серию").clicked() {
                    apply = Some(true);
                }
            });
            let Some(reset) = apply else { return };
            let result = (|| {
                let (part, assembly) = if reset {
                    (("", 1.), ("", 1.))
                } else {
                    (
                        (self.series[0].as_str(), parse(&self.series[1])?),
                        (self.series[2].as_str(), parse(&self.series[3])?),
                    )
                };
                let project = editor.document.project();
                if editor.selected.is_empty() {
                    return Err("Выберите детали".to_string());
                }
                // An empty assembly prefix keeps the К/Б/М marks.
                let assembly = (reset || !assembly.0.trim().is_empty() || assembly.1 != 1.)
                    .then_some(assembly);
                let commands = series_commands(project, &editor.selected, Some(part), assembly);
                if commands.is_empty() {
                    return Err("Среди выбранных нет деталей".to_string());
                }
                editor.commit_data("Серия нумерации", &commands)?;
                Ok(())
            })();
            self.message = Some(match result {
                Ok(()) if reset => "Серия сброшена; нумеруйте заново (⌘K)".into(),
                Ok(()) => "Серия назначена; нумеруйте заново (⌘K)".into(),
                Err(e) => e,
            });
        });
    }

    pub fn show(&mut self, ui: &mut egui::Ui, editor: &mut Editor) {
        egui::CollapsingHeader::new("Нумерация и коллизии")
            .default_open(true)
            .show(ui, |ui| {
                if ui
                    .button("Нумеровать и проверить · ⌘K")
                    .on_hover_text("Пересчитать позиции и проверить пересечения в фоне")
                    .clicked()
                {
                    // Runs as a cancellable background job (see ui::Workbench).
                    self.requested = true;
                }
                if ui
                    .button("Перенумеровать всё")
                    .on_hover_text("Номера заново по порядку, без сохранения прежних")
                    .clicked()
                {
                    self.renumber_all = true;
                    self.requested = true;
                }
                let project = editor.document.project();
                if !project.numbering.is_empty() {
                    let outdated = self.outdated_count(project);
                    if outdated > 0 {
                        ui.colored_label(
                            ui.visuals().warn_fg_color,
                            format!("Нумерация устарела: {outdated} объектов получат новые марки"),
                        );
                    } else {
                        ui.label("Нумерация актуальна");
                    }
                }
                self.series_section(ui, editor);
                ui.checkbox(&mut self.show_informative, "Показывать касания и болты");
                if self.stale(editor.document.project()) {
                    ui.colored_label(ui.visuals().warn_fg_color, "Отчёт устарел: модель изменена");
                }
                if let Some(message) = &self.message {
                    ui.label(message);
                }
                let rows: Vec<(usize, Conflict)> = self
                    .visible_rows()
                    .into_iter()
                    .map(|(i, c)| (i, c.clone()))
                    .collect();
                if self.report.is_some() && rows.is_empty() {
                    ui.label("Нет строк для выбранного фильтра");
                }
                let mut clicked = None;
                egui::ScrollArea::vertical()
                    .id_salt("clash rows")
                    .max_height(220.)
                    .show(ui, |ui| {
                        egui::Grid::new("clash grid")
                            .striped(true)
                            .num_columns(5)
                            .show(ui, |ui| {
                                for title in ["№", "Тип", "ID", "Позиции", "Объём, мм³"]
                                {
                                    ui.strong(title);
                                }
                                ui.end_row();
                                for (index, c) in &rows {
                                    let label = format!("{}", index + 1);
                                    if ui
                                        .selectable_label(self.selected_row == Some(*index), label)
                                        .clicked()
                                    {
                                        clicked = Some(*index);
                                    }
                                    ui.label(kind_label(c.kind));
                                    ui.label(match c.bolt {
                                        Some(bolt) => {
                                            format!(
                                                "болт #{}.{} / #{}",
                                                c.first.0,
                                                bolt + 1,
                                                c.second.0
                                            )
                                        }
                                        None => format!("#{} / #{}", c.first.0, c.second.0),
                                    });
                                    ui.label(format!(
                                        "{} / {}",
                                        c.first_position.as_deref().unwrap_or("—"),
                                        c.second_position.as_deref().unwrap_or("—")
                                    ));
                                    ui.label(format!("{:.1}", c.overlap_mm3)).on_hover_text(
                                        format!(
                                            "X {:.1} Y {:.1} Z {:.1} мм",
                                            c.location[0], c.location[1], c.location[2]
                                        ),
                                    );
                                    ui.end_row();
                                }
                            });
                    });
                if let Some(row) = clicked
                    && let Err(error) = self.select_row(row, editor)
                {
                    self.message = Some(error);
                }
                if let Some(numbering) = &self.numbering {
                    ui.collapsing("Позиции деталей", |ui| {
                        for (id, position) in &numbering.parts {
                            ui.label(format!("#{} → {position}", id.0));
                        }
                        for (id, position) in &numbering.assemblies {
                            ui.label(format!("сборка #{} → {position}", id.0));
                        }
                    });
                }
            });
    }
}
