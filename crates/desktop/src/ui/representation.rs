//! «Представление объектов» window: object groups with their rules and the
//! colour and visibility of each group in the model view.
use super::Workbench;
use crate::interaction::object_groups::{
    Condition, Field, Group, Join, RepresentationRow, Rule, Visibility, apply,
};
use eframe::egui;
use open_bim_model::ObjectId;
use open_bim_renderer::Appearance;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// Representation applied to the current model, shared with the view.
#[derive(Clone, Debug, Default)]
pub struct Shared {
    pub appearance: Arc<BTreeMap<ObjectId, Appearance>>,
    pub hidden: Arc<BTreeSet<ObjectId>>,
}

impl Workbench {
    /// Colours, opacity and hidden parts of the representation for the
    /// model; recomputed only when `scene` (a key of the built scene) or the
    /// groups and rows changed.
    pub fn applied_representation(&mut self, scene: usize) -> Shared {
        let key = (scene, self.representation_version);
        if let Some((k, shared)) = &self.applied
            && *k == key
        {
            return shared.clone();
        }
        let applied = apply(
            self.editor.document.project(),
            &self.groups,
            &self.representation,
        );
        let mut appearance = applied.appearance;
        // «Статус проекта» colours win over the representation rows.
        if let Some(status) = self
            .management
            .status_colours(self.editor.document.project())
        {
            appearance.extend(status);
        }
        let shared = Shared {
            appearance: Arc::new(appearance),
            hidden: Arc::new(applied.hidden),
        };
        self.applied = Some((key, shared.clone()));
        shared
    }

    pub(super) fn representation_window(&mut self, ctx: &egui::Context) {
        if !self.representation_open {
            return;
        }
        let mut open = true;
        let before = (self.groups.clone(), self.representation.clone());
        egui::Window::new("Представление объектов")
            .open(&mut open)
            .collapsible(false)
            .default_width(560.)
            .show(ctx, |ui| {
                self.representation_rows(ui);
                ui.separator();
                self.group_editor(ui);
            });
        self.representation_open = open;
        if (self.groups.clone(), self.representation.clone()) != before {
            self.representation_version += 1;
        }
    }

    fn representation_rows(&mut self, ui: &mut egui::Ui) {
        ui.strong("Строки представления — первая подходящая задаёт вид детали");
        let names: Vec<String> = self.groups.iter().map(|g| g.name.clone()).collect();
        let mut remove = None;
        egui::Grid::new("representation rows")
            .num_columns(4)
            .striped(true)
            .show(ui, |ui| {
                ui.label("Группа");
                ui.label("Цвет");
                ui.label("Видимость");
                ui.label("");
                ui.end_row();
                for (i, row) in self.representation.iter_mut().enumerate() {
                    egui::ComboBox::new(("rep group", i), "")
                        .selected_text(names.get(row.group).cloned().unwrap_or_default())
                        .show_ui(ui, |ui| {
                            for (g, name) in names.iter().enumerate() {
                                ui.selectable_value(&mut row.group, g, name);
                            }
                        });
                    ui.horizontal(|ui| {
                        let mut own = row.color.is_none();
                        if ui.checkbox(&mut own, "свой").changed() {
                            row.color = if own { None } else { Some([220, 60, 60]) };
                        }
                        if let Some(c) = &mut row.color {
                            ui.color_edit_button_srgb(c);
                        }
                    });
                    egui::ComboBox::new(("rep visibility", i), "")
                        .selected_text(row.visibility.label())
                        .show_ui(ui, |ui| {
                            for v in Visibility::CHOICES {
                                ui.selectable_value(&mut row.visibility, v, v.label());
                            }
                        });
                    if ui.button("Удалить").clicked() {
                        remove = Some(i);
                    }
                    ui.end_row();
                }
            });
        if let Some(i) = remove {
            self.representation.remove(i);
        }
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !self.groups.is_empty(),
                    egui::Button::new("Добавить строку"),
                )
                .clicked()
            {
                self.representation.push(RepresentationRow {
                    group: self.group_edited.min(self.groups.len().saturating_sub(1)),
                    color: Some([220, 60, 60]),
                    visibility: Visibility::Visible,
                });
            }
            if ui.button("Очистить").clicked() {
                self.representation.clear();
            }
        });
    }

    fn group_editor(&mut self, ui: &mut egui::Ui) {
        ui.strong("Группы объектов");
        ui.horizontal(|ui| {
            let selected = self
                .groups
                .get(self.group_edited)
                .map_or(String::new(), |g| g.name.clone());
            egui::ComboBox::new("group edited", "Группа")
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    for (i, g) in self.groups.iter().enumerate() {
                        ui.selectable_value(&mut self.group_edited, i, &g.name);
                    }
                });
            if ui.button("Новая").clicked() {
                self.groups.push(Group {
                    name: format!("Группа {}", self.groups.len() + 1),
                    rules: Vec::new(),
                });
                self.group_edited = self.groups.len() - 1;
            }
            if ui
                .add_enabled(!self.groups.is_empty(), egui::Button::new("Удалить группу"))
                .clicked()
            {
                let i = self.group_edited;
                self.groups.remove(i);
                // Rows of the removed group go, later rows shift down.
                self.representation.retain(|r| r.group != i);
                for r in &mut self.representation {
                    if r.group > i {
                        r.group -= 1;
                    }
                }
                self.group_edited = i.saturating_sub(1);
            }
        });
        let Some(group) = self.groups.get_mut(self.group_edited) else {
            return;
        };
        ui.horizontal(|ui| {
            let l = ui.label("Имя группы");
            ui.text_edit_singleline(&mut group.name).labelled_by(l.id);
        });
        let mut remove = None;
        egui::Grid::new("group rules")
            .num_columns(5)
            .striped(true)
            .show(ui, |ui| {
                for (i, rule) in group.rules.iter_mut().enumerate() {
                    if i == 0 {
                        ui.label("");
                    } else {
                        egui::ComboBox::new(("rule join", i), "")
                            .selected_text(rule.join.label())
                            .width(50.)
                            .show_ui(ui, |ui| {
                                for j in [Join::And, Join::Or] {
                                    ui.selectable_value(&mut rule.join, j, j.label());
                                }
                            });
                    }
                    ui.horizontal(|ui| {
                        let is_property = matches!(rule.field, Field::Property(_));
                        egui::ComboBox::new(("rule field", i), "")
                            .selected_text(if is_property {
                                "Свойство".to_string()
                            } else {
                                rule.field.label()
                            })
                            .show_ui(ui, |ui| {
                                for f in Field::FIXED {
                                    let label = f.label();
                                    ui.selectable_value(&mut rule.field, f, label);
                                }
                                if ui.selectable_label(is_property, "Свойство").clicked()
                                    && !is_property
                                {
                                    rule.field = Field::Property("surface_treatment".into());
                                }
                            });
                        if let Field::Property(key) = &mut rule.field {
                            ui.add(egui::TextEdit::singleline(key).desired_width(110.))
                                .on_hover_text("Имя свойства детали");
                        }
                    });
                    egui::ComboBox::new(("rule condition", i), "")
                        .selected_text(rule.condition.label())
                        .show_ui(ui, |ui| {
                            for c in Condition::ALL {
                                ui.selectable_value(&mut rule.condition, c, c.label());
                            }
                        });
                    ui.add(egui::TextEdit::singleline(&mut rule.value).desired_width(110.))
                        .on_hover_text("* и ? — шаблоны; числа для «больше» и «меньше»");
                    if ui.button("Удалить").clicked() {
                        remove = Some(i);
                    }
                    ui.end_row();
                }
            });
        if let Some(i) = remove {
            group.rules.remove(i);
        }
        if ui.button("Добавить условие").clicked() {
            group.rules.push(Rule {
                join: Join::And,
                field: Field::Profile,
                condition: Condition::Equals,
                value: String::new(),
            });
        }
        let project = self.editor.document.project();
        let count = project
            .elements
            .keys()
            .filter(|id| group.matches(project, **id))
            .count();
        ui.label(format!(
            "Деталей в группе: {count}. «И» связывает сильнее «ИЛИ»: a И b ИЛИ c = (a И b) ИЛИ c."
        ));
    }
}
