//! «Параметрическое моделирование»: model variables and part dimensions
//! bound to formulas over them. Every change of a variable or a binding
//! re-places the bound parts in the same undo step.
use eframe::egui;
use open_bim_commands::Command;
use open_bim_model::{
    Binding, Element, ElementShape, LengthMm, ObjectId, ParamField, Parametric, Point3, Project,
    parametric::Variable,
};
use std::collections::{BTreeMap, BTreeSet};

/// Current value of a part dimension, mm.
pub fn current(project: &Project, part: ObjectId, field: ParamField) -> Option<f64> {
    let e = project.elements.get(&part)?;
    match (&e.shape, field) {
        (ElementShape::Member { .. }, f) if ParamField::MEMBER.contains(&f) => {
            let (start, end, _) = project.member_reference_line(&e.shape)?;
            let k = ParamField::MEMBER.iter().position(|x| *x == f)?;
            Some(if k < 3 { start.0[k] } else { end.0[k - 3] })
        }
        (ElementShape::Plate { frame, .. }, f) if f != ParamField::Thickness => {
            let k = ParamField::PLATE.iter().position(|x| *x == f)?;
            Some(frame.origin.0[k])
        }
        (ElementShape::Plate { thickness, .. }, ParamField::Thickness) => Some(thickness.value()),
        _ => None,
    }
}

/// The part with the bound dimensions set to `values` (field → mm).
fn placed(
    project: &Project,
    e: &Element,
    values: &BTreeMap<ParamField, f64>,
) -> Result<Element, String> {
    let mut e = e.clone();
    match &e.shape {
        ElementShape::Member { .. } => {
            let (start, end, _) = project
                .member_reference_line(&e.shape)
                .ok_or("Нет профиля детали")?;
            let mut s = start.0;
            let mut t = end.0;
            for (k, f) in ParamField::MEMBER.iter().enumerate() {
                if let Some(v) = values.get(f) {
                    if k < 3 {
                        s[k] = *v;
                    } else {
                        t[k - 3] = *v;
                    }
                }
            }
            e.shape = project
                .replace_member(&e.shape, Some((Point3(s), Point3(t))), None, None)
                .map_err(|err| format!("«{}»: {err}", e.name))?;
        }
        ElementShape::Plate { .. } => {
            if let ElementShape::Plate {
                frame, thickness, ..
            } = &mut e.shape
            {
                for (k, f) in ParamField::PLATE[..3].iter().enumerate() {
                    if let Some(v) = values.get(f) {
                        frame.origin.0[k] = *v;
                    }
                }
                if let Some(v) = values.get(&ParamField::Thickness) {
                    *thickness = LengthMm::new(*v).map_err(|err| format!("Толщина: {err}"))?;
                }
            }
        }
    }
    Ok(e)
}

/// Commands storing `parametric` and re-placing every bound part.
pub fn apply(project: &Project, parametric: Parametric) -> Result<Vec<Command>, String> {
    parametric.validate().map_err(|e| e.to_string())?;
    let values = parametric.values()?;
    let mut per_part: BTreeMap<ObjectId, BTreeMap<ParamField, f64>> = BTreeMap::new();
    for b in &parametric.bindings {
        if !project.elements.contains_key(&b.part) {
            continue;
        }
        let v = open_bim_model::formula::eval(&b.formula, &values)
            .map_err(|e| format!("Деталь #{} ({}): {e}", b.part.0, b.field.label()))?;
        per_part.entry(b.part).or_default().insert(b.field, v);
    }
    let mut commands = vec![Command::SetParametric { parametric }];
    for (part, values) in per_part {
        let old = &project.elements[&part];
        let new = placed(project, old, &values)?;
        if new != *old {
            commands.push(Command::ReplaceElement { element: new });
        }
    }
    Ok(commands)
}

/// Bind `field` of the selected parts to `formula` (replacing a binding of
/// the same dimension).
pub fn bind(
    project: &Project,
    selected: &BTreeSet<ObjectId>,
    field: ParamField,
    formula: &str,
) -> Result<Vec<Command>, String> {
    let formula = formula.trim();
    if formula.is_empty() {
        return Err("Введите формулу".into());
    }
    let parts: Vec<ObjectId> = selected
        .iter()
        .copied()
        .filter(|id| current(project, *id, field).is_some())
        .collect();
    if parts.is_empty() {
        return Err(format!(
            "Выберите детали, у которых есть «{}»",
            field.label()
        ));
    }
    let mut p = project.parametric.clone();
    for part in parts {
        p.bindings.retain(|b| !(b.part == part && b.field == field));
        p.bindings.push(Binding {
            part,
            field,
            formula: formula.into(),
        });
    }
    apply(project, p)
}

/// Remove the bindings of the selected parts (all dimensions).
pub fn unbind(project: &Project, selected: &BTreeSet<ObjectId>) -> Result<Vec<Command>, String> {
    let mut p = project.parametric.clone();
    let before = p.bindings.len();
    p.bindings.retain(|b| !selected.contains(&b.part));
    if p.bindings.len() == before {
        return Err("У выбранных деталей нет связей".into());
    }
    Ok(vec![Command::SetParametric { parametric: p }])
}

/// Window state of «Параметрическое моделирование».
pub struct ParametricWindow {
    pub open: bool,
    /// Edited copy of the variables.
    pub variables: Vec<Variable>,
    loaded: Option<Vec<Variable>>,
    pub field: ParamField,
    pub formula: String,
    pub message: Option<String>,
}

impl Default for ParametricWindow {
    fn default() -> Self {
        Self {
            open: false,
            variables: Vec::new(),
            loaded: None,
            field: ParamField::EndZ,
            formula: String::new(),
            message: None,
        }
    }
}

impl ParametricWindow {
    /// The window; returns a labelled transaction to commit.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        project: &Project,
        selected: &BTreeSet<ObjectId>,
    ) -> Option<(String, Vec<Command>)> {
        if !self.open {
            self.loaded = None;
            return None;
        }
        if self.loaded.as_ref() != Some(&project.parametric.variables) {
            self.variables = project.parametric.variables.clone();
            self.loaded = Some(self.variables.clone());
        }
        let edited = Parametric {
            variables: self.variables.clone(),
            bindings: project.parametric.bindings.clone(),
        };
        let values = edited.values();
        let mut open = true;
        let (mut apply_now, mut bind_now, mut unbind_now, mut remove) = (false, false, false, None);
        egui::Window::new("Параметрическое моделирование")
            .id(egui::Id::new("parametric"))
            .open(&mut open)
            .default_width(520.)
            .show(ctx, |ui| {
                ui.strong("Параметры модели");
                egui::Grid::new("parametric vars")
                    .num_columns(4)
                    .striped(true)
                    .show(ui, |ui| {
                        for h in ["Имя", "Формула", "Значение", ""] {
                            ui.label(h);
                        }
                        ui.end_row();
                        for (i, v) in self.variables.iter_mut().enumerate() {
                            ui.add(egui::TextEdit::singleline(&mut v.name).desired_width(90.));
                            ui.add(egui::TextEdit::singleline(&mut v.formula).desired_width(200.));
                            ui.label(
                                values
                                    .as_ref()
                                    .ok()
                                    .and_then(|m| m.get(&v.name))
                                    .map_or("—".into(), |x| open_bim_reports::fmt(*x)),
                            );
                            if ui.small_button("✕").clicked() {
                                remove = Some(i);
                            }
                            ui.end_row();
                        }
                    });
                ui.horizontal(|ui| {
                    if ui.button("+ Параметр").clicked() {
                        let n = self.variables.len() + 1;
                        self.variables.push(Variable {
                            name: format!("П{n}"),
                            formula: "0".into(),
                        });
                    }
                    apply_now = ui.button("Применить к модели").clicked();
                });
                if let Err(e) = &values {
                    ui.colored_label(egui::Color32::RED, e);
                }
                ui.separator();
                ui.strong("Связать размер выбранных деталей");
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("param field")
                        .selected_text(self.field.label())
                        .show_ui(ui, |ui| {
                            for f in ParamField::MEMBER.iter().chain(&ParamField::PLATE) {
                                ui.selectable_value(&mut self.field, *f, f.label());
                            }
                        });
                    ui.label("=");
                    ui.add(egui::TextEdit::singleline(&mut self.formula).hint_text("Высота + 200"));
                });
                ui.horizontal(|ui| {
                    bind_now = ui.button("Связать").clicked();
                    unbind_now = ui.button("Снять связи выбранных").clicked();
                });
                if !project.parametric.bindings.is_empty() {
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .max_height(180.)
                        .show(ui, |ui| {
                            egui::Grid::new("parametric bindings")
                                .num_columns(3)
                                .striped(true)
                                .show(ui, |ui| {
                                    for b in &project.parametric.bindings {
                                        let name = project.elements.get(&b.part).map_or(
                                            format!("#{} (удалена)", b.part.0),
                                            |e| format!("{} #{}", e.name, b.part.0),
                                        );
                                        ui.label(name);
                                        ui.label(b.field.label());
                                        ui.label(&b.formula);
                                        ui.end_row();
                                    }
                                });
                        });
                }
                if let Some(m) = &self.message {
                    ui.label(m);
                }
            });
        self.open = open;
        if let Some(i) = remove {
            self.variables.remove(i);
        }
        let result = if apply_now {
            Some((
                "Параметры модели",
                apply(
                    project,
                    Parametric {
                        variables: self.variables.clone(),
                        bindings: project.parametric.bindings.clone(),
                    },
                ),
            ))
        } else if bind_now {
            Some((
                "Связать размер",
                bind(project, selected, self.field, &self.formula),
            ))
        } else if unbind_now {
            Some(("Снять связи", unbind(project, selected)))
        } else {
            None
        }?;
        match result {
            (label, Ok(commands)) => {
                self.message = Some(format!(
                    "Перемещено деталей: {}",
                    commands.len().saturating_sub(1)
                ));
                Some((label.to_string(), commands))
            }
            (_, Err(e)) => {
                self.message = Some(e);
                None
            }
        }
    }
}
