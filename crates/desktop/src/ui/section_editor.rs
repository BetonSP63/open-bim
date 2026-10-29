//! «Редактор сечений»: user sections drawn as a contour whose corners are
//! formulas over named variables, and sections welded from plates. Saved
//! definitions live in the project and show in the profile picker; «Создать
//! профиль» adds a profile of the chosen size.
use eframe::egui;
use open_bim_commands::Command;
use open_bim_model::sections::{PlateFormula, SectionKind, WeldFormula, templates};
use open_bim_model::{ObjectId, ProfileShape, Project, SectionDefinition, Variable, WeldProcess};
use std::collections::BTreeMap;

/// Context flag the profile picker sets to open the editor.
pub const OPEN_REQUEST: &str = "open section editor";

#[derive(Default)]
pub struct SectionEditor {
    pub open: bool,
    /// Definition being edited and the stored name it replaces.
    draft: Option<(SectionDefinition, Option<String>)>,
    /// Sizes of the input variables for «Создать профиль».
    inputs: BTreeMap<String, f64>,
}

/// Commands storing `draft` in the project, replacing `replaces`.
pub fn save(
    project: &Project,
    draft: &SectionDefinition,
    replaces: Option<&str>,
) -> Result<Vec<Command>, String> {
    let mut sections = project.sections.clone();
    let mut d = draft.clone();
    d.name = d.name.trim().into();
    let at = replaces
        .and_then(|n| sections.definitions.iter().position(|s| s.name == n))
        .or_else(|| sections.definitions.iter().position(|s| s.name == d.name));
    match at {
        Some(i) => sections.definitions[i] = d,
        None => sections.definitions.push(d),
    }
    sections.validate().map_err(|e| e.to_string())?;
    Ok(vec![Command::SetSections { sections }])
}

/// Commands removing the definition `name` (profiles made from it stay).
pub fn remove(project: &Project, name: &str) -> Vec<Command> {
    let mut sections = project.sections.clone();
    sections.definitions.retain(|s| s.name != name);
    vec![Command::SetSections { sections }]
}

/// A profile of `draft` with `inputs`: the project's one of the same name,
/// else a new profile (and the definition saved when the project lacks it).
pub fn create_profile(
    project: &Project,
    draft: &SectionDefinition,
    inputs: &BTreeMap<String, f64>,
) -> Result<(Vec<Command>, ObjectId), String> {
    let name = draft.profile_name(inputs).map_err(|e| e.to_string())?;
    if let Some(p) = project.profiles.values().find(|p| p.name == name) {
        return Ok((Vec::new(), p.id));
    }
    let id = ObjectId(project.next_id);
    let profile = draft.profile(id, inputs).map_err(|e| e.to_string())?;
    let mut commands = Vec::new();
    let stored = project.sections.get(&draft.name);
    let template = templates().into_iter().any(|t| t == *draft);
    if stored != Some(draft) && !template {
        commands.extend(save(project, draft, None)?);
    }
    commands.push(Command::CreateProfile { profile });
    Ok((commands, id))
}

/// A new blank definition of `kind` with a unique name.
pub fn blank(project: &Project, kind: SectionKind) -> SectionDefinition {
    let base = match kind {
        SectionKind::Contour => "Моё сечение",
        SectionKind::Welded => "Моё сварное сечение",
    };
    let mut name = base.to_string();
    let mut n = 2;
    while project.sections.get(&name).is_some() {
        name = format!("{base} {n}");
        n += 1;
    }
    let var = |n: &str, l: &str, f: &str| Variable {
        name: n.into(),
        label: l.into(),
        formula: f.into(),
    };
    match kind {
        SectionKind::Contour => SectionDefinition {
            name,
            variables: vec![
                var("h", "Высота", "200"),
                var("b", "Ширина", "100"),
                var("t", "Толщина", "10"),
            ],
            parameters: 3,
            // Angle with the corner at the origin.
            outer: [
                ["0", "0"],
                ["b", "0"],
                ["b", "t"],
                ["t", "t"],
                ["t", "h"],
                ["0", "h"],
            ]
            .map(|[x, y]| [x.into(), y.into()])
            .to_vec(),
            voids: vec![],
            plates: vec![],
            welds: vec![],
        },
        SectionKind::Welded => SectionDefinition {
            name,
            variables: vec![
                var("h", "Высота", "400"),
                var("b", "Ширина полок", "200"),
                var("tw", "Толщина стенки", "8"),
                var("tf", "Толщина полок", "12"),
                var("k", "Катет швов", "6"),
            ],
            parameters: 4,
            outer: vec![],
            voids: vec![],
            plates: vec![
                PlateFormula {
                    name: "Полка верхняя".into(),
                    x: "0".into(),
                    y: "h/2 - tf/2".into(),
                    width: "b".into(),
                    height: "tf".into(),
                },
                PlateFormula {
                    name: "Стенка".into(),
                    x: "0".into(),
                    y: "0".into(),
                    width: "tw".into(),
                    height: "h - 2*tf".into(),
                },
                PlateFormula {
                    name: "Полка нижняя".into(),
                    x: "0".into(),
                    y: "-h/2 + tf/2".into(),
                    width: "b".into(),
                    height: "tf".into(),
                },
            ],
            welds: vec![
                WeldFormula {
                    plates: [1, 2],
                    leg: "k".into(),
                    sides: 2,
                    process: WeldProcess::Submerged,
                },
                WeldFormula {
                    plates: [2, 3],
                    leg: "k".into(),
                    sides: 2,
                    process: WeldProcess::Submerged,
                },
            ],
        },
    }
}

/// Draws a section scaled into `size`: the outline, voids and, for a welded
/// section, each plate's edges.
pub fn paint_section(ui: &mut egui::Ui, shape: &ProfileShape, size: egui::Vec2) {
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, ui.visuals().extreme_bg_color);
    let Ok((outer, voids)) = shape.outline() else {
        return;
    };
    let Ok([x0, x1, y0, y1]) = shape.bounds() else {
        return;
    };
    let scale = ((rect.width() as f64 - 24.) / (x1 - x0))
        .min((rect.height() as f64 - 24.) / (y1 - y0)) as f32;
    let centre = egui::pos2(((x0 + x1) / 2.) as f32, ((y0 + y1) / 2.) as f32);
    let to = |p: [f64; 2]| {
        rect.center()
            + egui::vec2(
                (p[0] as f32 - centre.x) * scale,
                -(p[1] as f32 - centre.y) * scale,
            )
    };
    let fill = egui::Color32::from_rgb(120, 150, 190);
    let line = egui::Stroke::new(1.5, ui.visuals().strong_text_color());
    let ring = |r: &[[f64; 2]]| -> Vec<egui::Pos2> { r.iter().map(|p| to(*p)).collect() };
    // Fill as triangles from the mesh of the exact outline would need a
    // tessellator; plates and convex parts are filled, the outline is drawn.
    if let ProfileShape::Welded { plates, .. } = shape {
        for p in plates {
            let a = to([p.center[0] - p.width / 2., p.center[1] - p.height / 2.]);
            let b = to([p.center[0] + p.width / 2., p.center[1] + p.height / 2.]);
            painter.rect(
                egui::Rect::from_two_pos(a, b),
                0.0,
                fill,
                egui::Stroke::new(0.8, ui.visuals().weak_text_color()),
                egui::StrokeKind::Inside,
            );
        }
    }
    painter.add(egui::Shape::closed_line(ring(&outer), line));
    for v in &voids {
        painter.add(egui::Shape::closed_line(ring(v), line));
    }
    // Member axis.
    let axis = to([0., 0.]);
    let cross = egui::Stroke::new(1.0, egui::Color32::from_rgb(200, 60, 60));
    painter.line_segment(
        [axis - egui::vec2(6., 0.), axis + egui::vec2(6., 0.)],
        cross,
    );
    painter.line_segment(
        [axis - egui::vec2(0., 6.), axis + egui::vec2(0., 6.)],
        cross,
    );
}

fn formula_field(ui: &mut egui::Ui, text: &mut String, width: f32) -> bool {
    ui.add(egui::TextEdit::singleline(text).desired_width(width))
        .changed()
}

impl SectionEditor {
    /// Opens the editor on `name` (a project definition or template), or on
    /// the first one.
    pub fn open_on(&mut self, project: &Project, name: Option<&str>) {
        self.open = true;
        let all = project.sections.all();
        let pick = name
            .and_then(|n| all.iter().find(|d| d.name == n))
            .or(all.first())
            .cloned();
        if let Some(d) = pick {
            self.load(project, d);
        }
    }

    fn load(&mut self, project: &Project, d: SectionDefinition) {
        let stored = project.sections.get(&d.name).map(|s| s.name.clone());
        self.inputs = d.defaults().unwrap_or_default();
        self.draft = Some((d, stored));
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        project: &Project,
    ) -> Option<(String, Vec<Command>)> {
        let requested = ctx.data_mut(|d| d.remove_temp::<bool>(egui::Id::new(OPEN_REQUEST)));
        if requested == Some(true) && !self.open {
            self.open_on(project, None);
        }
        if !self.open {
            return None;
        }
        if self.draft.is_none() {
            self.open_on(project, None);
        }
        let mut action = None;
        let mut open = true;
        let mut load = None;
        egui::Window::new("Редактор сечений")
            .id(egui::Id::new("section-editor"))
            .open(&mut open)
            .collapsible(false)
            .default_width(860.)
            .show(ctx, |ui| {
                let Some((draft, stored)) = self.draft.as_mut() else {
                    return;
                };
                ui.horizontal(|ui| {
                    ui.label("Сечение:");
                    egui::ComboBox::from_id_salt("section-editor-pick")
                        .selected_text(draft.name.clone())
                        .width(260.)
                        .show_ui(ui, |ui| {
                            for d in project.sections.all() {
                                let tag = if project.sections.get(&d.name).is_some() {
                                    ""
                                } else {
                                    "  · шаблон"
                                };
                                let label = format!("{}{tag}  ({})", d.name, d.kind().label());
                                if ui.selectable_label(d.name == draft.name, label).clicked() {
                                    load = Some(d);
                                }
                            }
                        });
                    if ui.button("Новое по контуру").clicked() {
                        load = Some(blank(project, SectionKind::Contour));
                    }
                    if ui.button("Новое сварное").clicked() {
                        load = Some(blank(project, SectionKind::Welded));
                    }
                });
                ui.separator();
                ui.columns(2, |cols| {
                    egui::ScrollArea::vertical()
                        .id_salt("section-editor-form")
                        .max_height(520.)
                        .show(&mut cols[0], |ui| definition_form(ui, draft));
                    let ui = &mut cols[1];
                    ui.label(egui::RichText::new("Размеры профиля").strong());
                    let inputs: Vec<Variable> =
                        draft.variables[..draft.parameters.min(draft.variables.len())].to_vec();
                    egui::Grid::new("section-editor-sizes")
                        .num_columns(2)
                        .show(ui, |ui| {
                            for v in &inputs {
                                ui.label(if v.label.is_empty() {
                                    v.name.clone()
                                } else {
                                    format!("{} ({})", v.label, v.name)
                                });
                                let fallback =
                                    open_bim_model::formula::eval(&v.formula, &BTreeMap::new())
                                        .unwrap_or(0.);
                                let value = self.inputs.entry(v.name.clone()).or_insert(fallback);
                                ui.add(egui::DragValue::new(value).speed(1.0).suffix(" мм"));
                                ui.end_row();
                            }
                        });
                    self.inputs
                        .retain(|k, _| inputs.iter().any(|v| v.name == *k));
                    ui.add_space(6.);
                    match draft.shape(&self.inputs) {
                        Ok(shape) => {
                            paint_section(ui, &shape, egui::vec2(ui.available_width(), 300.));
                            let area = shape.area_mm2().unwrap_or(0.);
                            ui.label(
                                format!(
                                    "{}: A = {:.1} см², масса {:.2} кг/м",
                                    draft.profile_name(&self.inputs).unwrap_or_default(),
                                    area / 100.,
                                    area * 7850. / 1e6
                                )
                                .replace('.', ","),
                            );
                            if let ProfileShape::Welded { plates, welds } = &shape {
                                ui.weak(format!(
                                    "В ведомостях: {} листа(ов), {} шва(ов) по длине элемента",
                                    plates.len(),
                                    welds.len()
                                ));
                            }
                        }
                        Err(e) => {
                            ui.colored_label(ui.visuals().error_fg_color, e.to_string());
                        }
                    }
                    ui.add_space(6.);
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Сохранить в проект").clicked() {
                            action = Some(match save(project, draft, stored.as_deref()) {
                                Ok(c) => {
                                    *stored = Some(draft.name.trim().into());
                                    ("Сохранить сечение".to_string(), c)
                                }
                                Err(e) => ("".into(), vec![]).tap_error(ui, e),
                            });
                        }
                        if ui.button("Создать профиль").clicked() {
                            action = Some(match create_profile(project, draft, &self.inputs) {
                                Ok((c, _)) if c.is_empty() => ("".into(), vec![])
                                    .tap_error(ui, "Такой профиль уже есть в проекте".into()),
                                Ok((c, _)) => ("Создать профиль".to_string(), c),
                                Err(e) => ("".into(), vec![]).tap_error(ui, e),
                            });
                        }
                        if let Some(name) = stored.clone()
                            && ui.button("Удалить из проекта").clicked()
                        {
                            *stored = None;
                            action = Some(("Удалить сечение".to_string(), remove(project, &name)));
                        }
                    });
                    let id = egui::Id::new("section-editor-error");
                    if let Some(e) = ui.data(|d| d.get_temp::<String>(id)) {
                        ui.colored_label(ui.visuals().error_fg_color, e);
                    }
                });
            });
        if let Some(d) = load {
            ctx.data_mut(|m| m.remove_temp::<String>(egui::Id::new("section-editor-error")));
            self.load(project, d);
        }
        if !open {
            self.open = false;
        }
        action.filter(|(label, c)| !label.is_empty() && !c.is_empty())
    }
}

trait TapError {
    fn tap_error(self, ui: &egui::Ui, e: String) -> Self;
}

impl TapError for (String, Vec<Command>) {
    /// Shows `e` under the buttons and keeps the empty action.
    fn tap_error(self, ui: &egui::Ui, e: String) -> Self {
        ui.data_mut(|d| d.insert_temp(egui::Id::new("section-editor-error"), e));
        self
    }
}

/// Name, variables and either the contour or the plates and seams.
fn definition_form(ui: &mut egui::Ui, d: &mut SectionDefinition) {
    ui.horizontal(|ui| {
        ui.label("Имя:");
        ui.text_edit_singleline(&mut d.name);
    });
    ui.add_space(4.);
    ui.label(egui::RichText::new("Переменные").strong());
    ui.weak("Формула может использовать переменные выше. Первые — размеры, задаваемые при выборе профиля.");
    let mut drop = None;
    egui::Grid::new("section-vars")
        .num_columns(4)
        .show(ui, |ui| {
            ui.weak("Имя");
            ui.weak("Описание");
            ui.weak("Формула");
            ui.end_row();
            for (i, v) in d.variables.iter_mut().enumerate() {
                formula_field(ui, &mut v.name, 50.);
                formula_field(ui, &mut v.label, 150.);
                formula_field(ui, &mut v.formula, 150.);
                if ui.small_button("✕").clicked() {
                    drop = Some(i);
                }
                ui.end_row();
            }
        });
    if let Some(i) = drop {
        d.variables.remove(i);
    }
    ui.horizontal(|ui| {
        if ui.small_button("+ переменная").clicked() {
            d.variables.push(Variable {
                name: format!("p{}", d.variables.len() + 1),
                label: String::new(),
                formula: "0".into(),
            });
        }
        ui.label("Размеров при выборе:");
        let max = d.variables.len();
        ui.add(egui::DragValue::new(&mut d.parameters).range(0..=max));
    });
    d.parameters = d.parameters.min(d.variables.len());
    ui.add_space(6.);
    match d.kind() {
        SectionKind::Contour => contour_form(ui, d),
        SectionKind::Welded => welded_form(ui, d),
    }
}

fn points_grid(ui: &mut egui::Ui, salt: &str, points: &mut Vec<[String; 2]>) {
    let mut drop = None;
    egui::Grid::new(salt).num_columns(4).show(ui, |ui| {
        ui.weak("№");
        ui.weak("x");
        ui.weak("y");
        ui.end_row();
        for (i, [x, y]) in points.iter_mut().enumerate() {
            ui.label((i + 1).to_string());
            formula_field(ui, x, 140.);
            formula_field(ui, y, 140.);
            if ui.small_button("✕").clicked() {
                drop = Some(i);
            }
            ui.end_row();
        }
    });
    if let Some(i) = drop {
        points.remove(i);
    }
    if ui.small_button("+ точка").clicked() {
        let last = points.last().cloned().unwrap_or(["0".into(), "0".into()]);
        points.push(last);
    }
}

fn contour_form(ui: &mut egui::Ui, d: &mut SectionDefinition) {
    ui.label(egui::RichText::new("Контур").strong());
    ui.weak("Углы по порядку обхода; ось элемента — точка (0, 0).");
    points_grid(ui, "section-outer", &mut d.outer);
    let mut drop = None;
    for (i, v) in d.voids.iter_mut().enumerate() {
        ui.add_space(4.);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("Вырез {}", i + 1)).strong());
            if ui.small_button("Удалить вырез").clicked() {
                drop = Some(i);
            }
        });
        points_grid(ui, &format!("section-void-{i}"), v);
    }
    if let Some(i) = drop {
        d.voids.remove(i);
    }
    if ui.button("+ вырез").clicked() {
        d.voids.push(
            [["-10", "-10"], ["10", "-10"], ["10", "10"], ["-10", "10"]]
                .map(|[x, y]| [x.into(), y.into()])
                .to_vec(),
        );
    }
}

fn welded_form(ui: &mut egui::Ui, d: &mut SectionDefinition) {
    ui.label(egui::RichText::new("Листы").strong());
    ui.weak(
        "Прямоугольники: центр (x, y), ширина по x, высота по y. Меньшая сторона — толщина листа.",
    );
    let mut drop = None;
    egui::Grid::new("section-plates")
        .num_columns(7)
        .show(ui, |ui| {
            for h in ["№", "Название", "x", "y", "ширина", "высота"] {
                ui.weak(h);
            }
            ui.end_row();
            for (i, p) in d.plates.iter_mut().enumerate() {
                ui.label((i + 1).to_string());
                formula_field(ui, &mut p.name, 110.);
                formula_field(ui, &mut p.x, 70.);
                formula_field(ui, &mut p.y, 70.);
                formula_field(ui, &mut p.width, 70.);
                formula_field(ui, &mut p.height, 70.);
                if ui.small_button("✕").clicked() {
                    drop = Some(i);
                }
                ui.end_row();
            }
        });
    if let Some(i) = drop {
        d.plates.remove(i);
        // Seams keep pointing at the same plates.
        d.welds.retain(|w| !w.plates.contains(&(i + 1)));
        for w in &mut d.welds {
            for p in &mut w.plates {
                if *p > i + 1 {
                    *p -= 1;
                }
            }
        }
    }
    if ui.small_button("+ лист").clicked() {
        d.plates.push(PlateFormula {
            name: format!("Лист {}", d.plates.len() + 1),
            x: "0".into(),
            y: "0".into(),
            width: "100".into(),
            height: "10".into(),
        });
    }
    ui.add_space(6.);
    ui.label(egui::RichText::new("Швы по длине элемента").strong());
    let n = d.plates.len().max(1);
    let mut drop = None;
    egui::Grid::new("section-welds")
        .num_columns(6)
        .show(ui, |ui| {
            for h in ["Лист", "с листом", "катет", "сторон", "способ"] {
                ui.weak(h);
            }
            ui.end_row();
            for (i, w) in d.welds.iter_mut().enumerate() {
                ui.add(egui::DragValue::new(&mut w.plates[0]).range(1..=n));
                ui.add(egui::DragValue::new(&mut w.plates[1]).range(1..=n));
                formula_field(ui, &mut w.leg, 60.);
                ui.add(egui::DragValue::new(&mut w.sides).range(1..=2));
                egui::ComboBox::from_id_salt(("section-weld-process", i))
                    .selected_text(w.process.standard())
                    .show_ui(ui, |ui| {
                        for p in [
                            WeldProcess::Manual,
                            WeldProcess::GasShielded,
                            WeldProcess::Submerged,
                        ] {
                            ui.selectable_value(&mut w.process, p, p.standard());
                        }
                    });
                if ui.small_button("✕").clicked() {
                    drop = Some(i);
                }
                ui.end_row();
            }
        });
    if let Some(i) = drop {
        d.welds.remove(i);
    }
    if ui.small_button("+ шов").clicked() {
        d.welds.push(WeldFormula {
            plates: [1, 2.min(n)],
            leg: "6".into(),
            sides: 2,
            process: WeldProcess::Submerged,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_profile_needs_no_saved_definition() {
        let project = Project::default();
        let t = templates().remove(0);
        let (commands, id) = create_profile(&project, &t, &t.defaults().unwrap()).unwrap();
        assert_eq!(commands.len(), 1);
        assert_eq!(id, ObjectId(project.next_id));
    }

    #[test]
    fn edited_definition_is_saved_with_the_profile() {
        let mut project = Project::default();
        let mut d = blank(&project, SectionKind::Welded);
        d.name = "Балка подкрановая".into();
        let (commands, _) = create_profile(&project, &d, &BTreeMap::new()).unwrap();
        assert!(matches!(commands[0], Command::SetSections { .. }));
        for c in &commands {
            if let Command::SetSections { sections } = c {
                project.sections = sections.clone();
            }
        }
        assert_eq!(project.sections.definitions.len(), 1);
        let again = blank(&project, SectionKind::Welded);
        assert_ne!(again.name, "Балка подкрановая");
        assert!(remove(&project, "Балка подкрановая").len() == 1);
        // A broken definition is refused with the reason.
        d.plates[1].height = "h".into();
        assert!(
            save(&project, &d, None)
                .unwrap_err()
                .contains("перекрываются")
        );
    }
}
