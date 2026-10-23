//! «Пакетный редактор»: one value set on all selected parts, or text found
//! and replaced in it, previewed part by part and applied as one undo step.
use eframe::egui;
use open_bim_commands::Command;
use open_bim_model::{ObjectId, Project, PropertyValue};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Field {
    Name,
    Class,
    Prefix,
    Start,
    /// Any part property by its key.
    Property(String),
}

impl Field {
    pub const FIXED: [Self; 4] = [Self::Name, Self::Class, Self::Prefix, Self::Start];

    pub fn label(&self) -> String {
        match self {
            Self::Name => "Имя".into(),
            Self::Class => "Класс".into(),
            Self::Prefix => "Префикс деталей".into(),
            Self::Start => "Начальный номер деталей".into(),
            Self::Property(key) => format!("Свойство «{key}»"),
        }
    }

    fn key(&self) -> Option<&str> {
        match self {
            Self::Name => None,
            Self::Class => Some("class"),
            Self::Prefix => Some(open_bim_numbering::SERIES_PREFIX),
            Self::Start => Some(open_bim_numbering::SERIES_START),
            Self::Property(key) => Some(key),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Operation {
    /// Set the value (empty removes a property).
    #[default]
    Set,
    /// Replace every occurrence of a text.
    Replace,
}

pub struct BatchEditor {
    pub open: bool,
    pub field: Field,
    pub key: String,
    pub operation: Operation,
    pub value: String,
    pub find: String,
    pub message: Option<String>,
}

impl Default for BatchEditor {
    fn default() -> Self {
        Self {
            open: false,
            field: Field::Name,
            key: String::new(),
            operation: Operation::Set,
            value: String::new(),
            find: String::new(),
            message: None,
        }
    }
}

fn shown(v: Option<&PropertyValue>) -> String {
    match v {
        Some(PropertyValue::Text(t)) => t.clone(),
        Some(PropertyValue::Number(n)) => open_bim_reports::fmt(*n),
        Some(PropertyValue::Boolean(b)) => if *b { "да" } else { "нет" }.into(),
        None => String::new(),
    }
}

/// Current text of `field` on part `id`.
pub fn current(project: &Project, id: ObjectId, field: &Field) -> Option<String> {
    let e = project.elements.get(&id)?;
    Some(match field.key() {
        None => e.name.clone(),
        Some(key) => shown(e.properties.get(key)),
    })
}

/// New text of the field, or `None` when the part keeps it.
fn next_text(old: &str, operation: Operation, value: &str, find: &str) -> Option<String> {
    let new = match operation {
        Operation::Set => value.trim().to_string(),
        Operation::Replace if find.is_empty() => return None,
        Operation::Replace => old.replace(find, value),
    };
    (new != old).then_some(new)
}

/// A preview row: part, old text, new text.
pub type Row = (ObjectId, String, String);

/// Preview rows (part, old, new) and the commands of the edit.
pub fn plan(
    project: &Project,
    selected: &BTreeSet<ObjectId>,
    field: &Field,
    operation: Operation,
    value: &str,
    find: &str,
) -> Result<(Vec<Row>, Vec<Command>), String> {
    let mut rows = Vec::new();
    let mut commands = Vec::new();
    for id in selected {
        let Some(e) = project.elements.get(id) else {
            continue;
        };
        let old = current(project, *id, field).unwrap_or_default();
        let Some(new) = next_text(&old, operation, value, find) else {
            continue;
        };
        match field.key() {
            None => {
                if new.trim().is_empty() {
                    return Err("Имя не может быть пустым".into());
                }
                let mut element = e.clone();
                element.name = new.clone();
                commands.push(Command::ReplaceElement { element });
            }
            Some(key) => {
                let value = if new.is_empty() {
                    None
                } else if *field == Field::Start
                    || matches!(e.properties.get(key), Some(PropertyValue::Number(_)))
                {
                    let n = crate::tools::number(&new)
                        .map_err(|_| format!("{}: введите число", field.label()))?;
                    Some(PropertyValue::Number(n))
                } else {
                    Some(PropertyValue::Text(new.clone()))
                };
                commands.push(Command::ChangeProperty {
                    id: *id,
                    key: key.into(),
                    value,
                });
            }
        }
        rows.push((*id, old, new));
    }
    Ok((rows, commands))
}

impl BatchEditor {
    fn field(&self) -> Field {
        match &self.field {
            Field::Property(_) => Field::Property(self.key.trim().to_string()),
            f => f.clone(),
        }
    }

    /// The window; returns a labelled transaction to commit.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        project: &Project,
        selected: &BTreeSet<ObjectId>,
    ) -> Option<(String, Vec<Command>)> {
        if !self.open {
            return None;
        }
        let mut open = true;
        let mut out = None;
        egui::Window::new("Пакетный редактор")
            .id(egui::Id::new("batch editor"))
            .open(&mut open)
            .collapsible(false)
            .default_width(460.)
            .show(ctx, |ui| {
                let parts = selected
                    .iter()
                    .filter(|id| project.elements.contains_key(id))
                    .count();
                ui.label(format!("Выбрано деталей: {parts}"));
                egui::Grid::new("batch fields")
                    .num_columns(2)
                    .show(ui, |ui| {
                        let l = ui.label("Поле");
                        egui::ComboBox::new("batch field", "")
                            .selected_text(match &self.field {
                                Field::Property(_) => "Свойство по ключу".into(),
                                f => f.label(),
                            })
                            .show_ui(ui, |ui| {
                                for f in Field::FIXED {
                                    let label = f.label();
                                    ui.selectable_value(&mut self.field, f, label);
                                }
                                ui.selectable_value(
                                    &mut self.field,
                                    Field::Property(String::new()),
                                    "Свойство по ключу",
                                );
                            })
                            .response
                            .labelled_by(l.id);
                        ui.end_row();
                        if matches!(self.field, Field::Property(_)) {
                            let l = ui.label("Ключ свойства");
                            ui.text_edit_singleline(&mut self.key).labelled_by(l.id);
                            ui.end_row();
                        }
                        let l = ui.label("Действие");
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut self.operation, Operation::Set, "Задать");
                            ui.radio_value(
                                &mut self.operation,
                                Operation::Replace,
                                "Заменить текст",
                            );
                        })
                        .response
                        .labelled_by(l.id);
                        ui.end_row();
                        if self.operation == Operation::Replace {
                            let l = ui.label("Найти");
                            ui.text_edit_singleline(&mut self.find).labelled_by(l.id);
                            ui.end_row();
                        }
                        let l = ui.label(if self.operation == Operation::Replace {
                            "Заменить на"
                        } else {
                            "Значение"
                        });
                        ui.text_edit_singleline(&mut self.value).labelled_by(l.id);
                        ui.end_row();
                    });
                let field = self.field();
                if matches!(&field, Field::Property(k) if k.is_empty()) {
                    ui.weak("Введите ключ свойства");
                    return;
                }
                match plan(
                    project,
                    selected,
                    &field,
                    self.operation,
                    &self.value,
                    &self.find,
                ) {
                    Err(e) => {
                        ui.colored_label(ui.visuals().warn_fg_color, e);
                    }
                    Ok((rows, commands)) => {
                        ui.separator();
                        ui.label(format!("Изменится деталей: {}", rows.len()));
                        egui::ScrollArea::vertical()
                            .max_height(220.)
                            .show(ui, |ui| {
                                egui::Grid::new("batch preview")
                                    .striped(true)
                                    .num_columns(3)
                                    .show(ui, |ui| {
                                        for (id, old, new) in rows.iter().take(200) {
                                            let name = project
                                                .elements
                                                .get(id)
                                                .map_or(String::new(), |e| e.name.clone());
                                            ui.label(name);
                                            ui.label(old);
                                            ui.label(format!("→ {new}"));
                                            ui.end_row();
                                        }
                                    });
                            });
                        if ui
                            .add_enabled(!commands.is_empty(), egui::Button::new("Применить"))
                            .clicked()
                        {
                            self.message = Some(format!("Изменено деталей: {}", rows.len()));
                            out = Some((format!("Пакетно: {}", field.label()), commands));
                        }
                    }
                }
                if let Some(m) = &self.message {
                    ui.label(m);
                }
            });
        self.open = open;
        out
    }
}
