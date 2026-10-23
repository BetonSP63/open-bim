//! «Опалубка» tab: panels and props for concrete parts and the formwork
//! schedule window. Made by `interaction::formwork`.
use super::commands::Cmd;
use super::{Action, Mode, Request, Workbench};
use crate::interaction::formwork::{self as fw, FormworkSettings};
use crate::interaction::trim::{TrimPicker, TrimTool};
use eframe::egui;
use open_bim_model::ObjectId;
use std::collections::BTreeSet;

#[derive(Default)]
pub struct FormworkUi {
    pub settings: FormworkSettings,
    /// Panels and props drawn over the model.
    pub show: bool,
    pub open: bool,
}

impl Workbench {
    pub(super) fn formwork_command(&mut self, cmd: Cmd) -> Option<Request> {
        let tool = match cmd {
            Cmd::FormworkSchedule => {
                self.formwork.open = !self.formwork.open;
                self.formwork.show = true;
                self.message("Ведомость опалубки");
                return None;
            }
            Cmd::FormworkProps => TrimTool::FormProps,
            _ => TrimTool::FormPanels,
        };
        self.formwork.show = true;
        let project = self.editor.document.project();
        let hosts: BTreeSet<ObjectId> = self
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
            return match self.formwork_commit(tool, &hosts) {
                Ok(r) => Some(r),
                Err(e) => {
                    self.message(e);
                    None
                }
            };
        }
        self.engine.cancel();
        self.detail.cancel();
        self.mode = Mode::Model;
        self.trim = TrimPicker::start(tool);
        self.message(self.prompt());
        None
    }

    fn formwork_commit(
        &mut self,
        tool: TrimTool,
        hosts: &BTreeSet<ObjectId>,
    ) -> Result<Request, String> {
        let props = tool == TrimTool::FormProps;
        let r = fw::generate(
            self.editor.document.project(),
            hosts,
            &self.formwork.settings,
            props,
        );
        r.map(|(commands, n)| {
            self.message(if props {
                format!("Опор: {n}")
            } else {
                format!("Щитов опалубки: {n}")
            });
            Request::Action(Action::Commit(
                if props {
                    "Опоры"
                } else {
                    "Щиты опалубки"
                }
                .into(),
                commands,
            ))
        })
    }

    /// A finished pick of a formwork tool.
    pub(super) fn finish_formwork(
        &self,
        tool: TrimTool,
        parts: &[ObjectId],
    ) -> Option<Result<Vec<open_bim_commands::Command>, String>> {
        let [host] = parts else {
            return None;
        };
        let props = match tool {
            TrimTool::FormPanels => false,
            TrimTool::FormProps => true,
            _ => return None,
        };
        Some(
            fw::generate(
                self.editor.document.project(),
                &[*host].into(),
                &self.formwork.settings,
                props,
            )
            .map(|(c, _)| c),
        )
    }

    pub(super) fn formwork_window(&mut self, ctx: &egui::Context, requests: &mut Vec<Request>) {
        if !self.formwork.open {
            return;
        }
        let mut open = true;
        let project = self.editor.document.project();
        let selected = &self.editor.selected;
        let s = &mut self.formwork.settings;
        egui::Window::new("Ведомость опалубки")
            .id(egui::Id::new("formwork"))
            .open(&mut open)
            .default_width(420.)
            .show(ctx, |ui| {
                egui::Grid::new("formwork settings")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label("Щит: ширина, мм");
                        ui.add(egui::DragValue::new(&mut s.panel_width).range(100.0..=20_000.));
                        ui.end_row();
                        ui.label("Щит: высота, мм");
                        ui.add(egui::DragValue::new(&mut s.panel_height).range(100.0..=20_000.));
                        ui.end_row();
                        ui.label("Шаг опор, мм");
                        ui.add(egui::DragValue::new(&mut s.prop_spacing).range(100.0..=20_000.));
                        ui.end_row();
                    });
                if ui.button("Удалить опалубку выбранных деталей").clicked()
                    && let Ok(commands) = fw::clear(project, selected)
                {
                    requests.push(Request::Action(Action::Commit(
                        "Удалить опалубку".into(),
                        commands,
                    )));
                }
                ui.separator();
                let rows = fw::schedule(project);
                egui::ScrollArea::vertical()
                    .max_height(300.)
                    .show(ui, |ui| {
                        egui::Grid::new("formwork schedule")
                            .num_columns(3)
                            .striped(true)
                            .show(ui, |ui| {
                                for h in ["Элемент", "Кол-во", "Площадь, м²"] {
                                    ui.strong(h);
                                }
                                ui.end_row();
                                for r in &rows {
                                    ui.label(&r.item);
                                    ui.label(r.count.to_string());
                                    ui.label(if r.area_m2 > 0. {
                                        format!("{:.2}", r.area_m2)
                                    } else {
                                        "—".into()
                                    });
                                    ui.end_row();
                                }
                                let area: f64 = rows.iter().map(|r| r.area_m2).sum();
                                ui.strong("Итого опалубки");
                                ui.label("");
                                ui.strong(format!("{area:.2}"));
                                ui.end_row();
                            });
                    });
            });
        self.formwork.open = open;
    }
}
