//! «Мастер чертежей»: part and assembly drawings by rules, one drawing per
//! mark (equal parts share a drawing), optionally cloned from a template
//! drawing (sheet, scale, view placement and notes). The wizard first shows
//! the plan, then creates every drawing in one transaction.
use crate::tools::Editor;
use open_bim_commands::Command;
use open_bim_model::{Drawing, DrawingKind, ElementShape, ObjectId, Project};
use std::collections::{BTreeMap, BTreeSet};

/// Which parts get a part drawing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PartRule {
    #[default]
    All,
    /// Plates only (gussets, stiffeners, end plates…).
    Plates,
    /// Every part except the main parts of assemblies.
    Secondary,
}

impl PartRule {
    pub const ALL: [Self; 3] = [Self::All, Self::Plates, Self::Secondary];
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "все детали",
            Self::Plates => "только пластины",
            Self::Secondary => "кроме главных деталей сборок",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// Only the selection (parts and assemblies of selected parts), or the
    /// whole model.
    pub selection_only: bool,
    pub assemblies: bool,
    pub parts: bool,
    pub part_rule: PartRule,
    /// Add «Общие данные» of the КМД set when it is missing.
    pub general_data: bool,
    /// Template drawings to clone the layout from, per kind.
    pub assembly_template: Option<ObjectId>,
    pub part_template: Option<ObjectId>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            selection_only: false,
            assemblies: true,
            parts: true,
            part_rule: PartRule::All,
            general_data: true,
            assembly_template: None,
            part_template: None,
        }
    }
}

/// One drawing the wizard will create.
#[derive(Clone, Debug, PartialEq)]
pub struct Planned {
    pub kind: DrawingKind,
    pub mark: String,
    pub number: String,
}

/// The plan and the marks skipped because they already have a drawing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    pub drawings: Vec<Planned>,
    pub existing: Vec<String>,
    pub general_data: bool,
}

fn is_plate(project: &Project, id: ObjectId) -> bool {
    matches!(
        project.elements.get(&id).map(|e| &e.shape),
        Some(ElementShape::Plate { .. })
    )
}

/// Drawings to create: one per mark of the scope, skipping marks that
/// already have a drawing of that kind.
pub fn plan(
    project: &Project,
    selected: &BTreeSet<ObjectId>,
    settings: &Settings,
) -> Result<Plan, String> {
    let numbering = open_bim_numbering::number(project).map_err(|e| e.to_string())?;
    let in_scope_part = |id: &ObjectId| !settings.selection_only || selected.contains(id);
    let in_scope_assembly = |id: &ObjectId| {
        !settings.selection_only
            || selected.contains(id)
            || project.assemblies[id]
                .members
                .iter()
                .any(|m| selected.contains(m))
    };
    let main_parts: BTreeSet<ObjectId> = project.assemblies.values().map(|a| a.main_part).collect();
    let mut covered: BTreeSet<(bool, String)> = BTreeSet::new();
    for d in project.drawings.values() {
        match d.kind {
            DrawingKind::Part { part } => {
                if let Some(m) = numbering.parts.get(&part) {
                    covered.insert((false, m.clone()));
                }
            }
            DrawingKind::Assembly { assembly } => {
                if let Some(m) = numbering.assemblies.get(&assembly) {
                    covered.insert((true, m.clone()));
                }
            }
            _ => {}
        }
    }
    let mut out = Plan::default();
    let mut seen: BTreeSet<(bool, String)> = BTreeSet::new();
    let mut add = |assembly: bool, id: ObjectId, mark: &String, out: &mut Plan| {
        let key = (assembly, mark.clone());
        if !seen.insert(key.clone()) {
            return;
        }
        if covered.contains(&key) {
            out.existing.push(mark.clone());
            return;
        }
        out.drawings.push(Planned {
            kind: if assembly {
                DrawingKind::Assembly { assembly: id }
            } else {
                DrawingKind::Part { part: id }
            },
            mark: mark.clone(),
            number: format!("КМД-{mark}"),
        });
    };
    if settings.assemblies {
        for (id, mark) in numbering
            .assemblies
            .iter()
            .filter(|(id, _)| in_scope_assembly(id))
        {
            add(true, *id, mark, &mut out);
        }
    }
    if settings.parts {
        for (id, mark) in numbering.parts.iter().filter(|(id, _)| in_scope_part(id)) {
            let accepted = match settings.part_rule {
                PartRule::All => true,
                PartRule::Plates => is_plate(project, *id),
                PartRule::Secondary => !main_parts.contains(id),
            };
            if accepted {
                add(false, *id, mark, &mut out);
            }
        }
    }
    out.general_data = settings.general_data
        && !project
            .drawings
            .values()
            .any(|d| matches!(&d.kind, DrawingKind::GeneralData { mark } if mark == "КМД"))
        && (!out.drawings.is_empty()
            || project.drawings.values().any(|d| {
                matches!(
                    d.kind,
                    DrawingKind::Part { .. } | DrawingKind::Assembly { .. }
                )
            }));
    Ok(out)
}

/// Clone the layout of `template` onto a generated drawing: sheet, format,
/// scale, view placement and scales (matched by direction) and notes.
/// Dimensions stay the generated ones: they belong to the template's objects.
pub fn clone_layout(drawing: &mut Drawing, template: &Drawing) {
    drawing.sheet = template.sheet;
    drawing.multiple = template.multiple;
    drawing.portrait = template.portrait;
    drawing.scale = template.scale;
    for view in &mut drawing.views {
        if let Some(t) = template
            .views
            .iter()
            .find(|t| t.direction == view.direction)
        {
            view.position = t.position;
            view.scale = t.scale;
        }
    }
    drawing.texts = template.texts.clone();
    drawing.objects = template.objects.clone();
}

/// Create the planned drawings in one transaction; returns how many.
pub fn run(
    editor: &mut Editor,
    selected: &BTreeSet<ObjectId>,
    settings: &Settings,
) -> Result<usize, String> {
    let project = editor.document.project();
    let plan = plan(project, selected, settings)?;
    let templates: BTreeMap<bool, &Drawing> = [
        (true, settings.assembly_template),
        (false, settings.part_template),
    ]
    .into_iter()
    .filter_map(|(k, id)| Some((k, project.drawings.get(&id?)?)))
    .collect();
    let mut next = project.next_id;
    let mut commands = Vec::new();
    for planned in &plan.drawings {
        let id = ObjectId(next);
        next += 1;
        let (mut drawing, assembly) = match planned.kind {
            DrawingKind::Assembly { assembly } => (
                open_bim_drawings::assembly_drawing(project, id, assembly, &planned.number)?,
                true,
            ),
            DrawingKind::Part { part } => (
                open_bim_drawings::part_drawing(project, id, part, &planned.number)?,
                false,
            ),
            _ => continue,
        };
        if let Some(template) = templates.get(&assembly) {
            clone_layout(&mut drawing, template);
        }
        commands.push(Command::CreateDrawing { drawing });
    }
    if plan.general_data {
        // Its sheet lists are generated from the set, so it can come last.
        commands.push(Command::CreateDrawing {
            drawing: open_bim_drawings::general_data_drawing(project, ObjectId(next), "КМД")?,
        });
    }
    let count = commands.len();
    if count == 0 {
        return Err(if plan.existing.is_empty() {
            "Нет деталей и сборок для чертежей по выбранным правилам".into()
        } else {
            format!(
                "У всех марок уже есть чертежи ({})",
                plan.existing.join(", ")
            )
        });
    }
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
