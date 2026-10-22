//! «Мастер чертежей»: part and assembly drawings by rules, one drawing per
//! mark (equal parts share a drawing), optionally cloned from a template
//! drawing (sheet, scale, view placement and notes). The wizard first shows
//! the plan, then creates every drawing in one transaction.
use crate::tools::Editor;
use open_bim_commands::Command;
pub use open_bim_drawings::wizard::{
    PartRule, Plan, Planned, Settings, build, clone_layout, empty_reason, plan,
};
use open_bim_model::{DrawingKind, ObjectId};
use std::collections::BTreeSet;

/// Create the planned drawings in one transaction; returns how many.
pub fn run(
    editor: &mut Editor,
    selected: &BTreeSet<ObjectId>,
    settings: &Settings,
) -> Result<usize, String> {
    let project = editor.document.project();
    let plan = plan(project, selected, settings)?;
    let template = |id: Option<ObjectId>| id.and_then(|id| project.drawings.get(&id));
    let drawings = build(
        project,
        &plan,
        template(settings.assembly_template),
        template(settings.part_template),
        project.next_id,
    )?;
    let count = drawings.len();
    if count == 0 {
        return Err(empty_reason(&plan));
    }
    let commands: Vec<Command> = drawings
        .into_iter()
        .map(|drawing| Command::CreateDrawing { drawing })
        .collect();
    editor.commit_data(&format!("Мастер чертежей: {count}"), &commands)?;
    Ok(count)
}

/// Window state of the wizard.
#[derive(Default)]
pub struct Wizard {
    pub open: bool,
    pub settings: Settings,
    pub message: Option<String>,
}

impl Wizard {
    pub fn show(&mut self, ctx: &eframe::egui::Context, editor: &mut Editor) {
        use eframe::egui;
        if !self.open {
            return;
        }
        let mut open = self.open;
        egui::Window::new("Мастер чертежей")
            .id(egui::Id::new("drawing wizard"))
            .open(&mut open)
            .default_width(460.)
            .show(ctx, |ui| {
                let s = &mut self.settings;
                ui.horizontal(|ui| {
                    ui.radio_value(&mut s.selection_only, false, "Вся модель");
                    ui.radio_value(&mut s.selection_only, true, "Выбранные детали");
                });
                ui.checkbox(&mut s.assemblies, "Сборочные чертежи (по одному на марку)");
                ui.horizontal(|ui| {
                    ui.checkbox(&mut s.parts, "Чертежи деталей:");
                    egui::ComboBox::from_id_salt("wizard part rule")
                        .selected_text(s.part_rule.label())
                        .show_ui(ui, |ui| {
                            for rule in PartRule::ALL {
                                ui.selectable_value(&mut s.part_rule, rule, rule.label());
                            }
                        });
                });
                ui.checkbox(&mut s.general_data, "Общие данные КМД, если их нет");
                let project = editor.document.project();
                let template_combo = |ui: &mut egui::Ui,
                                      label: &str,
                                      value: &mut Option<ObjectId>,
                                      assembly: bool| {
                    let candidates: Vec<(ObjectId, String)> = project
                        .drawings
                        .values()
                        .filter(|d| {
                            if assembly {
                                matches!(d.kind, DrawingKind::Assembly { .. })
                            } else {
                                matches!(d.kind, DrawingKind::Part { .. })
                            }
                        })
                        .map(|d| (d.id, d.number.clone()))
                        .collect();
                    let current = value
                        .and_then(|id| candidates.iter().find(|c| c.0 == id))
                        .map_or("нет (по умолчанию)".to_string(), |c| {
                            c.1.clone()
                        });
                    ui.horizontal(|ui| {
                        ui.label(label);
                        egui::ComboBox::from_id_salt(label)
                            .selected_text(current)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(value, None, "нет (по умолчанию)");
                                for (id, number) in &candidates {
                                    ui.selectable_value(value, Some(*id), number);
                                }
                            });
                    });
                };
                ui.separator();
                ui.label("Клонировать раскладку (лист, масштаб, виды, примечания) с чертежа:");
                template_combo(ui, "Образец сборки", &mut s.assembly_template, true);
                template_combo(ui, "Образец детали", &mut s.part_template, false);
                ui.separator();
                match plan(project, &editor.selected, &self.settings) {
                    Ok(p) => {
                        let marks: Vec<&str> = p.drawings.iter().map(|d| d.mark.as_str()).collect();
                        ui.label(format!(
                            "Будет создано чертежей: {}{}",
                            p.drawings.len() + usize::from(p.general_data),
                            if p.general_data {
                                " (с общими данными)"
                            } else {
                                ""
                            }
                        ));
                        if !marks.is_empty() {
                            ui.small(format!("Марки: {}", marks.join(", ")));
                        }
                        if !p.existing.is_empty() {
                            ui.small(format!("Уже есть чертежи: {}", p.existing.join(", ")));
                        }
                    }
                    Err(e) => {
                        ui.colored_label(ui.visuals().error_fg_color, e);
                    }
                }
                if ui.button("Создать чертежи").clicked() {
                    let selected = editor.selected.clone();
                    self.message = Some(match run(editor, &selected, &self.settings) {
                        Ok(n) => format!("Создано чертежей: {n} (⌘Z — отменить)"),
                        Err(e) => e,
                    });
                }
                if let Some(m) = &self.message {
                    ui.label(m);
                }
            });
        self.open = open;
    }
}
