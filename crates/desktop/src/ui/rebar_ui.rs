//! «Армирование» tab: commands, the bar settings and schedule window.
//! Bar sets are made by `interaction::rebar`.
use super::commands::Cmd;
use super::{Action, Mode, Request, Workbench};
use crate::interaction::rebar::{self as rb, RebarSettings};
use crate::interaction::trim::{TrimPicker, TrimTool};
use eframe::egui;
use open_bim_model::ObjectId;

#[derive(Default)]
pub struct RebarUi {
    pub settings: RebarSettings,
    /// Bars drawn over the model («Видимость»).
    pub show: bool,
    pub open: bool,
}

/// Generator of a bar set in a host part.
type Generator = fn(
    &open_bim_model::Project,
    ObjectId,
    &RebarSettings,
) -> Result<open_bim_model::RebarSet, String>;

fn generator(tool: TrimTool) -> Option<Generator> {
    Some(match tool {
        TrimTool::RebarLongitudinal => rb::longitudinal,
        TrimTool::RebarStirrup => rb::stirrups,
        TrimTool::RebarMesh => rb::mesh,
        _ => return None,
    })
}

impl Workbench {
    pub(super) fn rebar_command(&mut self, cmd: Cmd) -> Option<Request> {
        let tool = match cmd {
            Cmd::RebarVisibility => {
                self.rebar.show = !self.rebar.show;
                self.message(if self.rebar.show {
                    "Арматура показана"
                } else {
                    "Арматура скрыта"
                });
                return None;
            }
            Cmd::RebarMore => {
                self.rebar.open = !self.rebar.open;
                self.message("Свойства арматуры и ведомость");
                return None;
            }
            Cmd::RebarLongitudinal => TrimTool::RebarLongitudinal,
            Cmd::RebarStirrup => TrimTool::RebarStirrup,
            Cmd::RebarFace => TrimTool::RebarMesh,
            Cmd::RebarGuide => TrimTool::RebarGuide,
            Cmd::RebarGroup => TrimTool::RebarGroup,
            _ => TrimTool::RebarSingle,
        };
        self.rebar.show = true;
        // Bars in the selected concrete parts at once.
        if let Some(make) = generator(tool) {
            let project = self.editor.document.project();
            let hosts: Vec<ObjectId> = self
                .editor
                .selected
                .iter()
                .copied()
                .filter(|id| {
                    project
                        .elements
                        .get(id)
                        .is_some_and(|e| crate::interaction::concrete::part_type(e).is_some())
                })
                .collect();
            if !hosts.is_empty() {
                let mut staged = project.clone();
                let mut all = Vec::new();
                for host in hosts {
                    let r = make(&staged, host, &self.rebar.settings)
                        .and_then(|set| rb::add(&staged, set));
                    match r {
                        Ok(commands) => {
                            if let Err(e) = open_bim_commands::apply_staged(&mut staged, &commands)
                            {
                                self.message(e.to_string());
                                return None;
                            }
                            all.extend(commands);
                        }
                        Err(e) => {
                            self.message(e);
                            return None;
                        }
                    }
                }
                // One SetConcrete with the final state is enough.
                let last = all.pop()?;
                return Some(Request::Action(Action::Commit(
                    super::commands::caption(cmd),
                    vec![last],
                )));
            }
        }
        self.engine.cancel();
        self.detail.cancel();
        self.mode = Mode::Model;
        self.trim = TrimPicker::start(tool);
        self.message(self.prompt());
        None
    }

    /// A finished pick sequence of a rebar tool.
    pub(super) fn finish_rebar(
        &self,
        tool: TrimTool,
        parts: &[ObjectId],
        points: &[open_bim_renderer::DVec3],
    ) -> Option<Result<Vec<open_bim_commands::Command>, String>> {
        let project = self.editor.document.project();
        let s = &self.rebar.settings;
        let set = match (tool, parts, points) {
            (t, [host], []) if generator(t).is_some() => generator(t)?(project, *host, s),
            (TrimTool::RebarSingle, [], [a, b]) => rb::single(*a, *b, s),
            (TrimTool::RebarGroup, [], [a, b, c]) => rb::group(*a, *b, *c, s),
            (TrimTool::RebarGuide, [], points) => rb::guide(points, s),
            _ => return None,
        };
        Some(set.and_then(|set| rb::add(project, set)))
    }

    pub(super) fn rebar_window(&mut self, ctx: &egui::Context, requests: &mut Vec<Request>) {
        if !self.rebar.open {
            return;
        }
        let mut open = true;
        let project = self.editor.document.project();
        let s = &mut self.rebar.settings;
        egui::Window::new("Армирование")
            .id(egui::Id::new("rebar"))
            .open(&mut open)
            .default_width(480.)
            .show(ctx, |ui| {
                ui.strong("Свойства следующих стержней");
                egui::Grid::new("rebar settings")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label("Диаметр, мм");
                        egui::ComboBox::from_id_salt("rebar d")
                            .selected_text(open_bim_reports::fmt(s.diameter))
                            .show_ui(ui, |ui| {
                                for d in rb::DIAMETERS {
                                    ui.selectable_value(
                                        &mut s.diameter,
                                        d,
                                        open_bim_reports::fmt(d),
                                    );
                                }
                            });
                        ui.end_row();
                        ui.label("Класс");
                        egui::ComboBox::from_id_salt("rebar grade")
                            .selected_text(s.grade.as_str())
                            .show_ui(ui, |ui| {
                                for g in rb::GRADES {
                                    ui.selectable_value(&mut s.grade, g.into(), g);
                                }
                            });
                        ui.end_row();
                        ui.label("Защитный слой, мм");
                        ui.add(egui::DragValue::new(&mut s.cover).range(0.0..=500.));
                        ui.end_row();
                        ui.label("Шаг, мм");
                        ui.add(egui::DragValue::new(&mut s.spacing).range(10.0..=10_000.));
                        ui.end_row();
                        ui.label("Продольных на грани");
                        ui.add(egui::DragValue::new(&mut s.per_face).range(2..=50));
                        ui.end_row();
                    });
                ui.separator();
                ui.strong(format!("Наборы: {}", project.concrete.rebars.len()));
                let mut remove = None;
                egui::ScrollArea::vertical()
                    .id_salt("rebar sets")
                    .max_height(200.)
                    .show(ui, |ui| {
                        egui::Grid::new("rebar sets grid")
                            .num_columns(6)
                            .striped(true)
                            .show(ui, |ui| {
                                for h in ["№", "Вид", "Деталь", "Ø, класс", "Стержней", ""]
                                {
                                    ui.strong(h);
                                }
                                ui.end_row();
                                for r in &project.concrete.rebars {
                                    ui.label(r.number.to_string());
                                    ui.label(r.kind.label());
                                    ui.label(r.host.map_or("—".into(), |h| {
                                        project
                                            .elements
                                            .get(&h)
                                            .map_or(format!("#{} (удалена)", h.0), |e| {
                                                format!("{} #{}", e.name, h.0)
                                            })
                                    }));
                                    ui.label(format!(
                                        "Ø{} {}",
                                        open_bim_reports::fmt(r.diameter),
                                        r.grade
                                    ));
                                    ui.label(r.bars.len().to_string());
                                    if ui.small_button("Удалить").clicked() {
                                        remove = Some(r.number);
                                    }
                                    ui.end_row();
                                }
                            });
                    });
                if let Some(n) = remove
                    && let Ok(commands) = rb::remove(project, n)
                {
                    requests.push(Request::Action(Action::Commit(
                        "Удалить арматуру".into(),
                        commands,
                    )));
                }
                ui.separator();
                ui.strong("Ведомость расхода стали");
                let rows = rb::schedule(project);
                egui::Grid::new("rebar schedule")
                    .num_columns(5)
                    .striped(true)
                    .show(ui, |ui| {
                        for h in ["Класс", "Ø, мм", "Стержней", "Длина, м", "Масса, кг"]
                        {
                            ui.strong(h);
                        }
                        ui.end_row();
                        for r in &rows {
                            ui.label(&r.grade);
                            ui.label(open_bim_reports::fmt(r.diameter));
                            ui.label(r.bars.to_string());
                            ui.label(format!("{:.2}", r.length_m));
                            ui.label(format!("{:.1}", r.mass_kg));
                            ui.end_row();
                        }
                        let total: f64 = rows.iter().map(|r| r.mass_kg).sum();
                        ui.strong("Итого");
                        ui.label("");
                        ui.label("");
                        ui.label("");
                        ui.strong(format!("{total:.1}"));
                        ui.end_row();
                    });
            });
        self.rebar.open = open;
    }
}
