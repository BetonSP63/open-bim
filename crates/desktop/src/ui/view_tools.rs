//! View tab tools: «Свойства вида», «Окрасить», «Перечертить», the
//! centre of rotation, «Облёт» and «Снимок экрана».
use super::{Request, Workbench};
use eframe::egui;
use open_bim_model::ObjectId;
use open_bim_renderer::RenderMode;
use std::collections::BTreeMap;

#[derive(Debug)]
pub struct ViewTools {
    pub properties_open: bool,
    /// Edited copy of the active view's name and depth.
    pub name: String,
    pub depth: [String; 2],
    /// The view being edited (fields are reloaded when it changes).
    edited: Option<usize>,
    pub paint_open: bool,
    pub paint_colour: [u8; 3],
    /// Temporary colours of objects («Окрасить»), cleared by «Перечертить».
    pub painted: BTreeMap<ObjectId, [f32; 3]>,
    /// «Центр вращения»: the next picked point becomes the centre.
    pub centre_pick: bool,
    /// «Облёт»: the wheel moves the eye forward and back.
    pub fly: bool,
}

impl Default for ViewTools {
    fn default() -> Self {
        Self {
            properties_open: false,
            name: String::new(),
            depth: [String::new(), String::new()],
            edited: None,
            paint_open: false,
            paint_colour: [230, 120, 30],
            painted: BTreeMap::new(),
            centre_pick: false,
            fly: false,
        }
    }
}

fn mm(v: f64) -> String {
    open_bim_reports::fmt(v)
}

impl Workbench {
    /// Paint the selected parts with the chosen colour (view only).
    pub fn paint_selected(&mut self) {
        let project = self.editor.document.project();
        let [r, g, b] = self.view_tools.paint_colour;
        let colour = [r, g, b].map(|c| c as f32 / 255.);
        let ids: Vec<ObjectId> = self
            .editor
            .selected
            .iter()
            .copied()
            .filter(|id| project.elements.contains_key(id))
            .collect();
        if ids.is_empty() {
            self.message("Окрасить: выберите детали");
            return;
        }
        for id in &ids {
            self.view_tools.painted.insert(*id, colour);
        }
        self.representation_version += 1;
        self.message(format!(
            "Окрашено деталей: {} (до «Перечертить»)",
            ids.len()
        ));
    }

    /// «Перечертить»: drop temporary colours and redraw the view.
    pub fn redraw(&mut self) {
        self.view_tools.painted.clear();
        self.representation_version += 1;
        self.message("Вид перечерчен");
    }

    pub(super) fn paint_window(&mut self, ctx: &egui::Context) {
        if !self.view_tools.paint_open {
            return;
        }
        let mut open = true;
        let (mut paint, mut clear) = (false, false);
        egui::Window::new("Окрасить")
            .id(egui::Id::new("paint objects"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Временный цвет выбранных деталей; «Перечертить» его снимает.");
                ui.horizontal(|ui| {
                    let l = ui.label("Цвет");
                    ui.color_edit_button_srgb(&mut self.view_tools.paint_colour)
                        .labelled_by(l.id);
                });
                ui.horizontal(|ui| {
                    paint = ui.button("Окрасить выбранные").clicked();
                    clear = ui.button("Снять окраску").clicked();
                });
                ui.weak(format!(
                    "Окрашено деталей: {}",
                    self.view_tools.painted.len()
                ));
            });
        self.view_tools.paint_open = open;
        if paint {
            self.paint_selected();
        }
        if clear {
            self.redraw();
        }
    }

    pub(super) fn view_properties_window(
        &mut self,
        ctx: &egui::Context,
        requests: &mut Vec<Request>,
    ) {
        if !self.view_tools.properties_open {
            self.view_tools.edited = None;
            return;
        }
        let index = self.active_view.min(self.views.len().saturating_sub(1));
        let Some(view) = self.views.get(index).cloned() else {
            self.view_tools.properties_open = false;
            return;
        };
        if self.view_tools.edited != Some(index) {
            self.view_tools.edited = Some(index);
            self.view_tools.name = view.name.clone();
            self.view_tools.depth = view
                .depth()
                .map_or([String::new(), String::new()], |d| d.map(mm));
        }
        let mut open = true;
        let (mut apply, mut save_new) = (false, false);
        let mut orthographic = view.orthographic;
        let mut mode = self.display.mode;
        egui::Window::new("Свойства вида")
            .id(egui::Id::new("view properties"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                egui::Grid::new("view properties grid")
                    .num_columns(2)
                    .show(ui, |ui| {
                        let l = ui.label("Имя");
                        ui.text_edit_singleline(&mut self.view_tools.name)
                            .labelled_by(l.id);
                        ui.end_row();
                        let l = ui.label("Проекция");
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut orthographic, true, "Ортогональная");
                            ui.radio_value(&mut orthographic, false, "Перспектива");
                        })
                        .response
                        .labelled_by(l.id);
                        ui.end_row();
                        let l = ui.label("Отображение");
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut mode, RenderMode::Wireframe, "Каркас");
                            ui.radio_value(&mut mode, RenderMode::Transparent, "Прозрачный");
                            ui.radio_value(&mut mode, RenderMode::Shaded, "Тонированный");
                        })
                        .response
                        .labelled_by(l.id);
                        ui.end_row();
                        if view.depth().is_some() {
                            for (caption, k) in [
                                ("Глубина вида вниз (за плоскостью), мм", 0),
                                ("Глубина вида вверх (перед плоскостью), мм", 1),
                            ] {
                                let l = ui.label(caption);
                                ui.text_edit_singleline(&mut self.view_tools.depth[k])
                                    .labelled_by(l.id);
                                ui.end_row();
                            }
                        }
                    });
                ui.horizontal(|ui| {
                    apply = ui.button("Изменить").clicked();
                    save_new = ui.button("Сохранить как новый вид").clicked();
                });
            });
        self.view_tools.properties_open = open;
        self.display.mode = mode;
        if orthographic != view.orthographic {
            self.views[index].orthographic = orthographic;
            requests.push(Request::Projection(orthographic));
        }
        if !(apply || save_new) {
            return;
        }
        match self.edited_view(index) {
            Ok(edited) => {
                let target = if save_new {
                    self.views.push(edited);
                    self.views.len() - 1
                } else {
                    self.views[index] = edited;
                    index
                };
                self.view_tools.edited = None;
                if let Some(request) = self.apply_view(target) {
                    requests.push(request);
                }
            }
            Err(e) => self.message(e),
        }
    }

    /// The active view with the name and depth from the window.
    fn edited_view(&self, index: usize) -> Result<super::views::ModelView, String> {
        let mut view = self.views[index].clone();
        let name = self.view_tools.name.trim();
        if name.is_empty() {
            return Err("Имя вида не может быть пустым".into());
        }
        view.name = name.to_string();
        if view.depth().is_some() {
            let [below, above] = [0, 1].map(|k| crate::tools::number(&self.view_tools.depth[k]));
            view.set_depth(below?, above?)?;
        }
        Ok(view)
    }
}

/// Folder of screenshots next to the project («Снимки»).
pub const SCREENSHOT_FOLDER: &str = "Снимки";

/// A free file name for the next screenshot: «Снимки/снимок-N.png» next to
/// the project file (in the working folder for an unsaved model).
pub fn screenshot_path(project_path: &str) -> std::path::PathBuf {
    let folder = match std::path::Path::new(project_path).parent() {
        Some(p) if !p.as_os_str().is_empty() => p.join(SCREENSHOT_FOLDER),
        _ => std::path::PathBuf::from(SCREENSHOT_FOLDER),
    };
    (1..)
        .map(|n| folder.join(format!("снимок-{n}.png")))
        .find(|p| !p.exists())
        .expect("an unused number")
}
