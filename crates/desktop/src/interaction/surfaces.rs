//! «Поверхности»: surface treatment of parts (paint system, galvanizing,
//! fire protection) with the treated area per treatment; a row selects its
//! parts in the model.
use crate::tools::Editor;
use eframe::egui;
use open_bim_commands::Command;
use open_bim_model::{ObjectId, Project, PropertyValue};
use open_bim_reports::{SURFACE_TREATMENT, coatings};
use std::collections::BTreeSet;

/// Commands setting `treatment` on the selected parts (`None` removes it).
pub fn assign(
    project: &Project,
    selected: &BTreeSet<ObjectId>,
    treatment: Option<&str>,
) -> Result<Vec<Command>, String> {
    let value = treatment
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(|t| PropertyValue::Text(t.to_string()));
    let parts: Vec<ObjectId> = selected
        .iter()
        .copied()
        .filter(|id| project.elements.contains_key(id))
        .collect();
    if parts.is_empty() {
        return Err("Выберите детали".into());
    }
    Ok(parts
        .into_iter()
        .filter(|id| project.elements[id].properties.get(SURFACE_TREATMENT) != value.as_ref())
        .map(|id| Command::ChangeProperty {
            id,
            key: SURFACE_TREATMENT.into(),
            value: value.clone(),
        })
        .collect())
}

/// Outer area of the selected parts, mm².
pub fn selected_area_mm2(project: &Project, selected: &BTreeSet<ObjectId>) -> f64 {
    selected
        .iter()
        .filter(|id| project.elements.contains_key(id))
        .filter_map(|id| project.element_surface_mm2(*id).ok())
        .sum()
}

#[derive(Default)]
pub struct Surfaces {
    pub open: bool,
    pub treatment: String,
    pub message: Option<String>,
}

impl Surfaces {
    pub fn show(&mut self, ctx: &egui::Context, editor: &mut Editor) {
        if !self.open {
            return;
        }
        if self.treatment.is_empty() {
            self.treatment = crate::ui::TREATMENTS[1].into();
        }
        let project = editor.document.project();
        let rows = coatings(project).unwrap_or_default();
        let area = selected_area_mm2(project, &editor.selected);
        let (mut set, mut clear, mut select) = (false, false, None);
        let mut open = true;
        egui::Window::new("Поверхности")
            .id(egui::Id::new("surfaces"))
            .open(&mut open)
            .collapsible(false)
            .default_width(460.)
            .show(ctx, |ui| {
                egui::ComboBox::new("surfaces treatment", "Покрытие")
                    .selected_text(self.treatment.clone())
                    .width(260.)
                    .show_ui(ui, |ui| {
                        for t in &crate::ui::TREATMENTS[1..] {
                            ui.selectable_value(&mut self.treatment, t.to_string(), *t);
                        }
                    });
                ui.horizontal(|ui| {
                    let l = ui.label("Своё покрытие");
                    ui.text_edit_singleline(&mut self.treatment)
                        .labelled_by(l.id);
                });
                ui.horizontal(|ui| {
                    set = ui.button("Назначить выбранным").clicked();
                    clear = ui.button("Снять с выбранных").clicked();
                });
                ui.label(format!("Площадь выбранных деталей: {:.3} м²", area / 1e6));
                ui.separator();
                if rows.is_empty() {
                    ui.weak("Покрытия не назначены");
                }
                egui::Grid::new("surfaces grid")
                    .num_columns(3)
                    .striped(true)
                    .show(ui, |ui| {
                        if !rows.is_empty() {
                            ui.strong("Покрытие");
                            ui.strong("Деталей");
                            ui.strong("Площадь, м²");
                            ui.end_row();
                        }
                        for (i, r) in rows.iter().enumerate() {
                            if ui
                                .selectable_label(false, &r.treatment)
                                .on_hover_text("Выбрать детали с этим покрытием")
                                .clicked()
                            {
                                select = Some(i);
                            }
                            ui.label(r.quantity.to_string());
                            ui.label(format!("{:.3}", r.area_m2));
                            ui.end_row();
                        }
                    });
                if let Some(m) = &self.message {
                    ui.label(m);
                }
            });
        self.open = open;
        if let Some(i) = select {
            let r = &rows[i];
            editor.selected = r.ids.iter().copied().collect();
            self.message = Some(format!("Выбрано деталей: {}", r.ids.len()));
        }
        if set || clear {
            let treatment = set.then_some(self.treatment.as_str());
            let result = assign(editor.document.project(), &editor.selected, treatment).and_then(
                |commands| {
                    let n = commands.len();
                    if n > 0 {
                        editor.commit_data("Обработка поверхности", &commands)?;
                    }
                    Ok(n)
                },
            );
            self.message = Some(match result {
                Ok(0) => "Покрытие не изменилось".into(),
                Ok(n) if set => format!("Покрытие назначено деталям: {n}"),
                Ok(n) => format!("Покрытие снято с деталей: {n}"),
                Err(e) => e,
            });
        }
    }
}
