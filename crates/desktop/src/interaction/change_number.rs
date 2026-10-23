//! «Изменить номер»: give the selected parts (or their assemblies) a chosen
//! number in their series. Equal objects share it; the marks of the other
//! objects are fixed as they are, so the next numbering keeps them all.
use eframe::egui;
use open_bim_commands::Command;
use open_bim_model::{ObjectId, Project};
use open_bim_numbering::{assembly_series, number, part_series, stored_marks};
use std::collections::{BTreeMap, BTreeSet};

/// Commands storing the new marks, and the (old mark, new mark) pairs.
pub type Change = (Vec<Command>, Vec<(String, String)>);

/// Commands storing the new marks; the result lists (old mark, new mark).
pub fn change_number(
    project: &Project,
    selected: &BTreeSet<ObjectId>,
    assemblies: bool,
    new_number: u64,
) -> Result<Change, String> {
    if new_number == 0 {
        return Err("Номер — целое число от 1".into());
    }
    let numbering = number(project).map_err(|e| e.to_string())?;
    let current = if assemblies {
        &numbering.assemblies
    } else {
        &numbering.parts
    };
    // The targets: selected parts, or the assemblies holding them.
    let targets: BTreeSet<ObjectId> = if assemblies {
        project
            .assemblies
            .values()
            .filter(|a| selected.contains(&a.id) || a.members.iter().any(|m| selected.contains(m)))
            .map(|a| a.id)
            .collect()
    } else {
        selected
            .iter()
            .copied()
            .filter(|id| project.elements.contains_key(id))
            .collect()
    };
    if targets.is_empty() {
        return Err(if assemblies {
            "Выберите детали сборок".into()
        } else {
            "Выберите детали".into()
        });
    }
    let series = |id: ObjectId| {
        if assemblies {
            assembly_series(project, id)
        } else {
            part_series(project, id)
        }
    };
    let mut marks = stored_marks(&numbering);
    let mut changes = BTreeMap::new();
    for id in &targets {
        let Some(old) = current.get(id) else {
            continue;
        };
        let new = format!("{}{new_number}", series(*id).prefix);
        changes.insert(old.clone(), (series(*id), new));
    }
    if changes.len() > 1 {
        let mut news: Vec<&String> = changes.values().map(|(_, n)| n).collect();
        news.sort();
        news.dedup();
        if news.len() < changes.len() {
            return Err(format!(
                "Выбраны разные марки ({}): одному номеру — одна марка",
                changes.keys().cloned().collect::<Vec<_>>().join(", ")
            ));
        }
    }
    for (old, (series_of_old, new)) in &changes {
        // The new mark must be free in the series (or already this group's).
        let taken = current
            .iter()
            .any(|(id, mark)| mark == new && mark != old && series(*id) == *series_of_old);
        if taken {
            return Err(format!("Марка {new} уже занята"));
        }
    }
    for (id, mark) in current {
        if let Some((_, new)) = changes.get(mark) {
            marks.insert(*id, new.clone());
        }
    }
    let list = changes
        .into_iter()
        .filter(|(old, (_, new))| old != new)
        .map(|(old, (_, new))| (old, new))
        .collect::<Vec<_>>();
    if list.is_empty() {
        return Ok((Vec::new(), list));
    }
    Ok((vec![Command::SetNumbering { marks }], list))
}

/// Window state of «Изменить номер».
#[derive(Default)]
pub struct ChangeNumber {
    pub open: bool,
    /// Assemblies of the selected parts instead of the parts.
    pub assemblies: bool,
    pub number: String,
    pub message: Option<String>,
}

impl ChangeNumber {
    /// The window; returns a labelled transaction to commit.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        project: &Project,
        selected: &BTreeSet<ObjectId>,
    ) -> Option<(String, Vec<Command>)> {
        if !self.open {
            return None;
        }
        let mut open = true;
        let mut out = None;
        egui::Window::new("Изменить номер")
            .id(egui::Id::new("change number"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.assemblies, false, "Детали");
                    ui.radio_value(&mut self.assemblies, true, "Сборки");
                });
                if let Ok(numbering) = number(project) {
                    let marks = if self.assemblies {
                        &numbering.assemblies
                    } else {
                        &numbering.parts
                    };
                    let mut shown: Vec<&str> = marks
                        .iter()
                        .filter(|(id, _)| {
                            selected.contains(id)
                                || project
                                    .assemblies
                                    .get(id)
                                    .is_some_and(|a| a.members.iter().any(|m| selected.contains(m)))
                        })
                        .map(|(_, m)| m.as_str())
                        .collect();
                    shown.sort();
                    shown.dedup();
                    ui.label(if shown.is_empty() {
                        "Марки выбранных: нет".to_string()
                    } else {
                        format!("Марки выбранных: {}", shown.join(", "))
                    });
                }
                ui.horizontal(|ui| {
                    let l = ui.label("Новый номер");
                    ui.text_edit_singleline(&mut self.number).labelled_by(l.id);
                });
                if ui.button("Изменить").clicked() {
                    let result = self
                        .number
                        .trim()
                        .parse::<u64>()
                        .map_err(|_| "Номер — целое число от 1".to_string())
                        .and_then(|n| change_number(project, selected, self.assemblies, n));
                    self.message = Some(match result {
                        Ok((_, list)) if list.is_empty() => "Номер не изменился".into(),
                        Ok((commands, list)) => {
                            let text = list
                                .iter()
                                .map(|(o, n)| format!("{o} → {n}"))
                                .collect::<Vec<_>>()
                                .join(", ");
                            out = Some(("Изменить номер".to_string(), commands));
                            format!("Изменено: {text}")
                        }
                        Err(e) => e,
                    });
                }
                if let Some(m) = &self.message {
                    ui.label(m);
                }
            });
        self.open = open;
        out
    }
}
