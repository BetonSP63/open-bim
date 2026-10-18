//! Numbering and clash report panel. Rows select the referenced parts.
use super::Editor;
use eframe::egui;
use open_bim_coordination::{Conflict, ConflictKind, Report};
use open_bim_model::{DetailKind, ObjectId, Project};
use open_bim_numbering::Numbering;
use std::collections::BTreeSet;

#[derive(Default)]
pub struct CoordinationUi {
    pub numbering: Option<Numbering>,
    pub report: Option<Report>,
    pub selected_row: Option<usize>,
    pub show_informative: bool,
    pub message: Option<String>,
    /// Set by the panel button; the workbench starts the background check.
    pub requested: bool,
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

impl CoordinationUi {
    pub fn run(&mut self, project: &Project) -> Result<(), String> {
        let (numbering, report) = Self::compute(project)?;
        self.accept(numbering, report);
        Ok(())
    }

    /// Pure computation, safe to run on a worker thread.
    pub fn compute(project: &Project) -> Result<(Numbering, Report), String> {
        let numbering = open_bim_numbering::number(project).map_err(|e| e.to_string())?;
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
