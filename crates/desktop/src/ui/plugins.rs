//! Plugin window: plugins found in the plugin folders, their inputs
//! (points typed in, parts taken from the selection) and the attribute
//! dialog. The run itself happens in a background job; its steps come
//! back as commands committed as one undo step. A placed plugin (picked
//! by one of its parts) can be run again with new attribute values.

use eframe::egui;
use open_bim_api::plugins::{self, AttributeType, InputType, InputValue, Plugin};
use open_bim_model::{ObjectId, PluginInstance, PluginValue, Project};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// A run the window asked for; the workbench starts it as a job.
pub struct RunRequest {
    pub plugin: Plugin,
    pub inputs: BTreeMap<String, InputValue>,
    pub attributes: BTreeMap<String, Value>,
    /// Run this placed plugin again instead of placing a new one.
    pub placed: Option<ObjectId>,
}

#[derive(Default)]
pub struct PluginsUi {
    pub open: bool,
    /// Extra folder typed by the user, searched with the default ones.
    pub folder: String,
    pub plugins: Vec<Plugin>,
    pub problems: Vec<String>,
    scanned: bool,
    pub selected: usize,
    /// Input name → text: «x y z» for points, IDs for parts.
    pub inputs: BTreeMap<String, String>,
    /// Attribute name → text shown in the dialog.
    pub attributes: BTreeMap<String, String>,
    pub message: Option<String>,
    /// The placed plugin picked by the selection, with its attribute texts.
    pub placed: Option<ObjectId>,
    pub placed_attributes: BTreeMap<String, String>,
}

/// The placed plugin one of the selected objects belongs to.
pub fn placed_in<'a>(
    project: &'a Project,
    selected: &BTreeSet<ObjectId>,
) -> Option<&'a PluginInstance> {
    project
        .plugins
        .values()
        .find(|p| selected.contains(&p.id) || !p.outputs.is_disjoint(selected))
}

fn text_of(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Folders searched for plugins: «plugins» next to the project file,
/// the user folder, the folders of `OPEN_BIM_PLUGINS` (a path list) and
/// the folder typed in the window.
pub fn search_dirs(project_path: &str, extra: &str) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(parent) = Path::new(project_path).parent() {
        dirs.push(parent.join("plugins"));
    }
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(
            PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("Open BIM")
                .join("plugins"),
        );
    }
    if let Some(list) = std::env::var_os("OPEN_BIM_PLUGINS") {
        dirs.extend(std::env::split_paths(&list).filter(|d| !d.as_os_str().is_empty()));
    }
    if !extra.trim().is_empty() {
        dirs.push(PathBuf::from(extra.trim()));
    }
    let mut seen = BTreeSet::new();
    dirs.retain(|d| seen.insert(d.clone()));
    dirs
}

impl PluginsUi {
    pub fn rescan(&mut self, project_path: &str) {
        let dirs = search_dirs(project_path, &self.folder);
        let (found, problems) = plugins::discover(&dirs);
        let current = self.current().map(|p| p.manifest.name.clone());
        self.plugins = found;
        self.problems = problems;
        self.scanned = true;
        self.selected = current
            .and_then(|name| self.plugins.iter().position(|p| p.manifest.name == name))
            .unwrap_or(0);
        self.reset_fields();
        self.message = Some(if self.plugins.is_empty() {
            format!(
                "Плагинов нет. Папки: {}",
                dirs.iter()
                    .map(|d| d.display().to_string())
                    .collect::<Vec<_>>()
                    .join("; ")
            )
        } else {
            format!("Найдено плагинов: {}", self.plugins.len())
        });
    }

    pub fn current(&self) -> Option<&Plugin> {
        self.plugins.get(self.selected)
    }

    pub fn select(&mut self, index: usize) {
        if index < self.plugins.len() && index != self.selected {
            self.selected = index;
            self.reset_fields();
        }
    }

    /// Empty inputs and attribute defaults of the current plugin.
    fn reset_fields(&mut self) {
        self.inputs.clear();
        self.attributes.clear();
        let Some(plugin) = self.plugins.get(self.selected) else {
            return;
        };
        for input in &plugin.manifest.inputs {
            self.inputs.insert(input.name.clone(), String::new());
        }
        for a in &plugin.manifest.attributes {
            self.attributes.insert(a.name.clone(), text_of(&a.default));
        }
    }

    /// Fill every part input from the selection: a single-part input
    /// takes the selection only when one part is selected.
    pub fn take_selection(&mut self, selected: &BTreeSet<ObjectId>) {
        let Some(plugin) = self.plugins.get(self.selected) else {
            return;
        };
        let ids = selected
            .iter()
            .map(|id| id.0.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        for input in &plugin.manifest.inputs {
            let fits = match input.kind {
                InputType::Parts => !selected.is_empty(),
                InputType::Part => selected.len() == 1,
                InputType::Point | InputType::Points => false,
            };
            if fits {
                self.inputs.insert(input.name.clone(), ids.clone());
            }
        }
    }

    /// Check the typed values against the plugin description.
    pub fn request(&self) -> Result<RunRequest, String> {
        let plugin = self.current().ok_or("Плагин не выбран")?.clone();
        let mut inputs = BTreeMap::new();
        for input in &plugin.manifest.inputs {
            let text = self.inputs.get(&input.name).map_or("", |s| s.trim());
            let label = if input.prompt.is_empty() {
                &input.name
            } else {
                &input.prompt
            };
            if text.is_empty() {
                return Err(format!("Не задан вход «{label}»"));
            }
            let json = parse_input(input.kind, text).map_err(|e| format!("«{label}»: {e}"))?;
            let value =
                InputValue::from_json(input.kind, &json).map_err(|e| format!("«{label}»: {e}"))?;
            inputs.insert(input.name.clone(), value);
        }
        let attributes = parse_attributes(&plugin, &self.attributes)?;
        Ok(RunRequest {
            plugin,
            inputs,
            attributes,
            placed: None,
        })
    }

    /// Follow the selection: load the attributes of the placed plugin it
    /// picks (once per placed plugin).
    pub fn follow_selection(&mut self, project: &Project, selected: &BTreeSet<ObjectId>) {
        let placed = placed_in(project, selected);
        if placed.map(|p| p.id) == self.placed {
            return;
        }
        self.placed = placed.map(|p| p.id);
        self.placed_attributes = placed
            .map(|p| {
                p.attributes
                    .iter()
                    .map(|(k, v)| {
                        let text = match v {
                            PluginValue::Number(n) => n.to_string(),
                            PluginValue::Text(t) => t.clone(),
                        };
                        (k.clone(), text)
                    })
                    .collect()
            })
            .unwrap_or_default();
    }

    /// Run the picked placed plugin again with the edited attributes.
    pub fn edit_request(&self, project: &Project) -> Result<RunRequest, String> {
        let id = self.placed.ok_or("Плагин в модели не выбран")?;
        let instance = project
            .plugins
            .get(&id)
            .ok_or("Плагина больше нет в модели")?;
        let plugin = self
            .plugins
            .iter()
            .find(|p| p.manifest.name == instance.plugin)
            .ok_or_else(|| format!("Плагин «{}» не найден в папках плагинов", instance.plugin))?
            .clone();
        let attributes = parse_attributes(&plugin, &self.placed_attributes)?;
        Ok(RunRequest {
            plugin,
            inputs: BTreeMap::new(),
            attributes,
            placed: Some(id),
        })
    }

    /// Draw the window; returns a run request when «Выполнить» is pressed.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        project_path: &str,
        project: &Project,
        selected: &BTreeSet<ObjectId>,
        busy: bool,
    ) -> Option<RunRequest> {
        if !self.open {
            return None;
        }
        if !self.scanned {
            self.rescan(project_path);
        }
        self.follow_selection(project, selected);
        let mut open = true;
        let mut run = None;
        egui::Window::new("Плагины")
            .id(egui::Id::new("plugins window"))
            .open(&mut open)
            .collapsible(false)
            .default_width(420.)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Папка");
                    ui.text_edit_singleline(&mut self.folder)
                        .on_hover_text("Дополнительная папка с плагинами");
                    if ui.button("Обновить").clicked() {
                        self.rescan(project_path);
                    }
                });
                for problem in &self.problems {
                    ui.colored_label(ui.visuals().warn_fg_color, problem);
                }
                if let Some(instance) = self.placed.and_then(|id| project.plugins.get(&id)) {
                    ui.separator();
                    ui.strong(format!(
                        "В модели: «{}» ({}), объектов {}",
                        instance.plugin,
                        instance.id,
                        instance.outputs.len()
                    ));
                    let labels: BTreeMap<String, String> = self
                        .plugins
                        .iter()
                        .find(|p| p.manifest.name == instance.plugin)
                        .map(|p| {
                            p.manifest
                                .attributes
                                .iter()
                                .map(|a| (a.name.clone(), a.label.clone()))
                                .collect()
                        })
                        .unwrap_or_default();
                    egui::Grid::new("placed plugin attributes")
                        .num_columns(2)
                        .show(ui, |ui| {
                            for (name, text) in &mut self.placed_attributes {
                                let label = labels.get(name).filter(|l| !l.is_empty());
                                ui.label(label.unwrap_or(name));
                                ui.text_edit_singleline(text);
                                ui.end_row();
                            }
                        });
                    if ui
                        .add_enabled(!busy, egui::Button::new("Изменить"))
                        .on_hover_text("Запустить плагин снова с этими атрибутами")
                        .clicked()
                    {
                        match self.edit_request(project) {
                            Ok(r) => run = Some(r),
                            Err(e) => self.message = Some(e),
                        }
                    }
                    ui.separator();
                }
                if self.plugins.is_empty() {
                    if let Some(m) = &self.message {
                        ui.label(m);
                    }
                    return;
                }
                let names: Vec<String> = self
                    .plugins
                    .iter()
                    .map(|p| match &p.manifest.code {
                        Some(code) => format!("{} ({code})", p.manifest.name),
                        None => p.manifest.name.clone(),
                    })
                    .collect();
                let mut index = self.selected;
                egui::ComboBox::from_label("Плагин")
                    .selected_text(names[index].clone())
                    .show_ui(ui, |ui| {
                        for (i, name) in names.iter().enumerate() {
                            ui.selectable_value(&mut index, i, name);
                        }
                    });
                self.select(index);
                let Some(plugin) = self.plugins.get(self.selected).cloned() else {
                    return;
                };
                if !plugin.manifest.description.is_empty() {
                    ui.label(&plugin.manifest.description);
                }
                ui.separator();
                ui.strong("Входы");
                egui::Grid::new("plugin inputs")
                    .num_columns(2)
                    .show(ui, |ui| {
                        for input in &plugin.manifest.inputs {
                            let label = if input.prompt.is_empty() {
                                input.name.clone()
                            } else {
                                input.prompt.clone()
                            };
                            ui.label(label);
                            let text = self.inputs.entry(input.name.clone()).or_default();
                            ui.add(egui::TextEdit::singleline(text).hint_text(hint(input.kind)));
                            ui.end_row();
                        }
                    });
                let parts = plugin
                    .manifest
                    .inputs
                    .iter()
                    .any(|i| matches!(i.kind, InputType::Part | InputType::Parts));
                if parts
                    && ui
                        .add_enabled(!selected.is_empty(), egui::Button::new("Детали из выбора"))
                        .clicked()
                {
                    self.take_selection(selected);
                }
                if !plugin.manifest.attributes.is_empty() {
                    ui.separator();
                    ui.strong("Атрибуты");
                    egui::Grid::new("plugin attributes")
                        .num_columns(2)
                        .show(ui, |ui| {
                            for a in &plugin.manifest.attributes {
                                ui.label(if a.label.is_empty() {
                                    &a.name
                                } else {
                                    &a.label
                                });
                                let text = self.attributes.entry(a.name.clone()).or_default();
                                ui.text_edit_singleline(text);
                                ui.end_row();
                            }
                        });
                    if ui.button("По умолчанию").clicked() {
                        let inputs = std::mem::take(&mut self.inputs);
                        self.reset_fields();
                        self.inputs = inputs;
                    }
                }
                ui.separator();
                if ui
                    .add_enabled(!busy, egui::Button::new("Выполнить"))
                    .clicked()
                {
                    match self.request() {
                        Ok(r) => run = Some(r),
                        Err(e) => self.message = Some(e),
                    }
                }
                if let Some(m) = &self.message {
                    ui.label(m);
                }
            });
        if !open {
            self.open = false;
        }
        run
    }
}

fn hint(kind: InputType) -> &'static str {
    match kind {
        InputType::Point => "x y z, мм",
        InputType::Points => "x y z; x y z; …",
        InputType::Part => "ID детали",
        InputType::Parts => "ID через пробел",
    }
}

/// Attribute texts checked against the plugin description; empty texts
/// are left out (the plugin takes its default).
fn parse_attributes(
    plugin: &Plugin,
    texts: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, Value>, String> {
    let mut attributes = BTreeMap::new();
    for a in &plugin.manifest.attributes {
        let text = texts.get(&a.name).map_or("", |s| s.trim());
        if text.is_empty() {
            continue;
        }
        let value = match a.kind {
            AttributeType::Number => {
                let n = parse_number(text)
                    .ok_or_else(|| format!("«{}»: нужно число, а не «{text}»", a.label))?;
                serde_json::Number::from_f64(n)
                    .map(Value::Number)
                    .ok_or_else(|| format!("«{}»: недопустимое число", a.label))?
            }
            AttributeType::Text | AttributeType::Profile | AttributeType::Material => {
                Value::String(text.into())
            }
        };
        attributes.insert(a.name.clone(), value);
    }
    Ok(attributes)
}

fn parse_number(text: &str) -> Option<f64> {
    text.replace(',', ".")
        .parse::<f64>()
        .ok()
        .filter(|n| n.is_finite())
}

fn parse_point(text: &str) -> Result<Value, String> {
    // A decimal comma is allowed only when coordinates are separated by spaces.
    let parts: Vec<&str> = if text.contains(char::is_whitespace) {
        text.split_whitespace().collect()
    } else {
        text.split(',').collect()
    };
    let coords: Vec<f64> = parts
        .iter()
        .map(|p| parse_number(p.trim_matches(',')))
        .collect::<Option<_>>()
        .ok_or_else(|| format!("не точка: «{text}»"))?;
    if coords.len() != 3 {
        return Err(format!("нужно три координаты x y z, а не «{text}»"));
    }
    Ok(Value::from(coords))
}

fn parse_ids(text: &str) -> Result<Vec<u64>, String> {
    text.split(|c: char| c.is_whitespace() || c == ',' || c == ';')
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<u64>().map_err(|_| format!("не ID: «{s}»")))
        .collect()
}

/// Typed text as the JSON value of the declared input type.
pub fn parse_input(kind: InputType, text: &str) -> Result<Value, String> {
    match kind {
        InputType::Point => parse_point(text),
        InputType::Points => text
            .split([';', '\n'])
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(parse_point)
            .collect::<Result<Vec<_>, _>>()
            .map(Value::from),
        InputType::Part => match parse_ids(text)?.as_slice() {
            [id] => Ok(Value::from(*id)),
            _ => Err("нужна одна деталь".into()),
        },
        InputType::Parts => parse_ids(text).map(Value::from),
    }
}
