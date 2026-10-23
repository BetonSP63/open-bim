//! «Каталог чертежей-прототипов»: drawings saved as prototypes (sheet,
//! scale, view placement and notes) in a folder next to the model, shared
//! between models; new part or assembly drawings of the selected parts are
//! cloned from a prototype in one undo step.
use crate::tools::Editor;
use crate::tools::property_files::check_name;
use eframe::egui;
use open_bim_commands::Command;
use open_bim_drawings::wizard::{PartRule, Settings, build, empty_reason, plan};
use open_bim_model::{Drawing, DrawingKind, ObjectId};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub const FOLDER: &str = "Прототипы чертежей";
const EXTENSION: &str = "json";

/// Prototype folder for a model file (the working folder when unsaved).
pub fn folder(project_path: &str) -> PathBuf {
    match Path::new(project_path).parent() {
        Some(p) if !p.as_os_str().is_empty() => p.join(FOLDER),
        _ => PathBuf::from(FOLDER),
    }
}

/// Prototype names in the folder, sorted.
pub fn list(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            (path.extension()? == EXTENSION)
                .then(|| path.file_stem()?.to_str().map(String::from))?
        })
        .collect();
    names.sort();
    names
}

/// Save a part or assembly drawing as prototype `name`.
pub fn save(dir: &Path, name: &str, drawing: &Drawing) -> Result<PathBuf, String> {
    let name = check_name(name)?;
    if !matches!(
        drawing.kind,
        DrawingKind::Part { .. } | DrawingKind::Assembly { .. }
    ) {
        return Err("Прототипом может быть чертёж детали или сборки".into());
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{name}.{EXTENSION}"));
    let json = serde_json::to_string_pretty(drawing).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(path)
}

pub fn load(dir: &Path, name: &str) -> Result<Drawing, String> {
    let name = check_name(name)?;
    let text = std::fs::read_to_string(dir.join(format!("{name}.{EXTENSION}")))
        .map_err(|e| format!("Прототип «{name}»: {e}"))?;
    let drawing: Drawing =
        serde_json::from_str(&text).map_err(|e| format!("Прототип «{name}»: {e}"))?;
    drawing
        .validate()
        .map_err(|e| format!("Прототип «{name}»: {e}"))?;
    Ok(drawing)
}

pub fn remove(dir: &Path, name: &str) -> Result<(), String> {
    let name = check_name(name)?;
    std::fs::remove_file(dir.join(format!("{name}.{EXTENSION}"))).map_err(|e| e.to_string())
}

/// Drawings of the selected parts (part prototype) or of their assemblies
/// (assembly prototype), one per mark, laid out like the prototype.
pub fn create_from(
    editor: &mut Editor,
    selected: &BTreeSet<ObjectId>,
    prototype: &Drawing,
) -> Result<usize, String> {
    let assembly = matches!(prototype.kind, DrawingKind::Assembly { .. });
    let settings = Settings {
        selection_only: true,
        assemblies: assembly,
        parts: !assembly,
        part_rule: PartRule::All,
        general_data: false,
        assembly_template: None,
        part_template: None,
    };
    if selected.is_empty() {
        return Err("Выберите детали в модели".into());
    }
    let project = editor.document.project();
    let plan = plan(project, selected, &settings)?;
    let drawings = build(
        project,
        &plan,
        assembly.then_some(prototype),
        (!assembly).then_some(prototype),
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
    editor.commit_data(&format!("Чертежи по прототипу: {count}"), &commands)?;
    Ok(count)
}

#[derive(Default)]
pub struct Catalog {
    pub open: bool,
    pub selected: Option<String>,
    pub name: String,
    pub message: Option<String>,
}

impl Catalog {
    /// The window; `current` is the drawing open in the drawing editor.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        editor: &mut Editor,
        project_path: &str,
        current: Option<ObjectId>,
    ) {
        if !self.open {
            return;
        }
        let dir = folder(project_path);
        let names = list(&dir);
        let (mut save_now, mut create, mut delete) = (false, false, false);
        let mut open = true;
        egui::Window::new("Каталог чертежей-прототипов")
            .id(egui::Id::new("drawing prototypes"))
            .open(&mut open)
            .collapsible(false)
            .default_width(420.)
            .show(ctx, |ui| {
                ui.small(format!("Папка: {}", dir.display()));
                egui::ScrollArea::vertical()
                    .max_height(200.)
                    .show(ui, |ui| {
                        if names.is_empty() {
                            ui.weak("Прототипов нет: сохраните открытый чертёж как прототип");
                        }
                        for name in &names {
                            let on = self.selected.as_deref() == Some(name);
                            if ui.selectable_label(on, name).clicked() {
                                self.selected = Some(name.clone());
                            }
                        }
                    });
                ui.horizontal(|ui| {
                    let chosen = self.selected.as_ref().is_some_and(|n| names.contains(n));
                    create = ui
                        .add_enabled(chosen, egui::Button::new("Создать для выбранных"))
                        .clicked();
                    delete = ui
                        .add_enabled(chosen, egui::Button::new("Удалить прототип"))
                        .clicked();
                });
                ui.separator();
                ui.horizontal(|ui| {
                    let l = ui.label("Имя прототипа");
                    ui.text_edit_singleline(&mut self.name).labelled_by(l.id);
                });
                save_now = ui
                    .add_enabled(
                        current.is_some(),
                        egui::Button::new("Сохранить открытый чертёж как прототип"),
                    )
                    .clicked();
                if let Some(m) = &self.message {
                    ui.label(m);
                }
            });
        self.open = open;
        if save_now {
            let drawing = current.and_then(|id| editor.document.project().drawings.get(&id));
            self.message = Some(match drawing {
                Some(d) => match save(&dir, &self.name, d) {
                    Ok(_) => {
                        self.selected = Some(self.name.trim().to_string());
                        format!("Прототип «{}» сохранён", self.name.trim())
                    }
                    Err(e) => e,
                },
                None => "Откройте чертёж".into(),
            });
        }
        if let Some(name) = self.selected.clone() {
            if create {
                let selected = editor.selected.clone();
                self.message = Some(
                    match load(&dir, &name).and_then(|p| create_from(editor, &selected, &p)) {
                        Ok(n) => format!("Создано чертежей по прототипу «{name}»: {n}"),
                        Err(e) => e,
                    },
                );
            }
            if delete {
                self.message = Some(match remove(&dir, &name) {
                    Ok(()) => {
                        self.selected = None;
                        format!("Прототип «{name}» удалён")
                    }
                    Err(e) => e,
                });
            }
        }
    }
}
