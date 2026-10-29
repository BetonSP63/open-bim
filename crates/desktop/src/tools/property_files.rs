//! Property files («Сохранить как» / «Загрузить» in the beam,
//! column, plate and bolt dialogs): named sets of dialog values kept as JSON
//! in the `attributes` folder next to the project file. The file named
//! «standard» is loaded when the command starts.
//!
//! Profiles, materials and levels are stored by name, so a file works in any
//! project; a name the project does not have keeps the current value and is
//! reported.
use super::Draft;
use super::column_levels::Height;
use super::detailing_ui::DetailingUi;
use crate::interaction::polygon::PlatePosition;
use open_bim_commands::Command;
use open_bim_model::{ObjectId, PartPosition, Project};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const STANDARD: &str = "standard";
const FOLDER: &str = "attributes";

/// Dialog a property file belongs to; also its file extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FileKind {
    Beam,
    Column,
    Plate,
    Bolt,
}

impl FileKind {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Beam => "beam.json",
            Self::Column => "column.json",
            Self::Plate => "plate.json",
            Self::Bolt => "bolt.json",
        }
    }
    pub fn of_tool(kind: super::ToolKind) -> Option<Self> {
        use super::ToolKind;
        match kind {
            ToolKind::Beam => Some(Self::Beam),
            ToolKind::Column => Some(Self::Column),
            ToolKind::Plate | ToolKind::Gusset | ToolKind::Stiffener => Some(Self::Plate),
            _ => None,
        }
    }
}

/// Column end in a file: offset from a named level, or absolute Z.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LevelValue {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    pub value: f64,
}

/// Beam or column dialog values; a missing field keeps the dialog's value.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MemberValues {
    pub name: Option<String>,
    pub profile: Option<String>,
    pub material: Option<String>,
    pub position: Option<PartPosition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottom: Option<LevelValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<LevelValue>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlateValues {
    pub name: Option<String>,
    pub material: Option<String>,
    pub thickness: Option<String>,
    /// «Над плоскостью», «По центру», «Под плоскостью».
    pub position: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BoltValues {
    pub standard: Option<String>,
    pub grade: Option<String>,
    pub diameter: Option<String>,
    pub clearance: Option<String>,
    pub steps_x: Option<String>,
    pub steps_y: Option<String>,
    pub shift_x: Option<String>,
    pub shift_y: Option<String>,
    pub rotate: Option<bool>,
    /// «Массив», «Окружность» or «Список».
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub circle_count: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub circle_diameter: Option<String>,
}

/// Contents of a property file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Values {
    Beam(MemberValues),
    Column(MemberValues),
    Plate(PlateValues),
    Bolt(BoltValues),
}

impl Values {
    pub fn kind(&self) -> FileKind {
        match self {
            Self::Beam(_) => FileKind::Beam,
            Self::Column(_) => FileKind::Column,
            Self::Plate(_) => FileKind::Plate,
            Self::Bolt(_) => FileKind::Bolt,
        }
    }
}

/// `attributes` folder next to the project file.
pub fn folder(project_path: &str) -> PathBuf {
    match Path::new(project_path).parent() {
        Some(p) if !p.as_os_str().is_empty() => p.join(FOLDER),
        _ => PathBuf::from(FOLDER),
    }
}

/// A file name the user typed: letters, digits, spaces, «-», «_», «.».
pub fn check_name(name: &str) -> Result<&str, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Введите имя файла свойств".into());
    }
    if name.starts_with('.')
        || name.chars().count() > 60
        || name
            .chars()
            .any(|c| !(c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.' | '+' | '(' | ')')))
    {
        return Err(format!(
            "Имя «{name}»: буквы, цифры, пробел, - _ . + ( ), до 60 символов"
        ));
    }
    Ok(name)
}

fn path(dir: &Path, kind: FileKind, name: &str) -> PathBuf {
    dir.join(format!("{name}.{}", kind.extension()))
}

/// Saved file names of a kind, sorted, «standard» first.
pub fn list(dir: &Path, kind: FileKind) -> Vec<String> {
    let suffix = format!(".{}", kind.extension());
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            e.file_name()
                .to_str()?
                .strip_suffix(&suffix)
                .map(String::from)
        })
        .collect();
    names.sort_by_key(|n| (n != STANDARD, n.to_lowercase()));
    names
}

pub fn save(dir: &Path, name: &str, values: &Values) -> Result<PathBuf, String> {
    let name = check_name(name)?;
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let file = path(dir, values.kind(), name);
    let text = serde_json::to_string_pretty(values).map_err(|e| e.to_string())?;
    std::fs::write(&file, text).map_err(|e| format!("{}: {e}", file.display()))?;
    Ok(file)
}

pub fn load(dir: &Path, kind: FileKind, name: &str) -> Result<Values, String> {
    let name = check_name(name)?;
    let file = path(dir, kind, name);
    let text = std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
    let values: Values =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", file.display()))?;
    if values.kind() != kind {
        return Err(format!("{}: файл другого типа", file.display()));
    }
    Ok(values)
}

/// The «standard» file of a kind, if saved (loaded when the command
/// starts); a broken file is an error, a missing one is `None`.
pub fn standard(dir: &Path, kind: FileKind) -> Option<Result<Values, String>> {
    path(dir, kind, STANDARD)
        .exists()
        .then(|| load(dir, kind, STANDARD))
}

/// Commands copying into the project the profile and steel a file names
/// when the project lacks them but the ГОСТ catalogue has them.
pub fn catalogue_commands(project: &Project, values: &Values) -> Vec<Command> {
    use crate::ui::catalog_picker::{Pick, material_by_grade, profile_by_name};
    let (profile, material) = match values {
        Values::Beam(v) | Values::Column(v) => (v.profile.as_deref(), v.material.as_deref()),
        Values::Plate(v) => (None, v.material.as_deref()),
        Values::Bolt(_) => (None, None),
    };
    let mut next = project.next_id;
    let mut commands = Vec::new();
    if let Some(Pick::New(mut p)) = profile.and_then(|n| profile_by_name(project, n)) {
        p.id = ObjectId(next);
        next += 1;
        commands.push(Command::CreateProfile { profile: p });
    }
    if let Some(name) = material
        && find_material(project, name).is_none()
        && let Some(Pick::New(mut m)) =
            material_by_grade(project, name.split(" · ").next().unwrap_or_default())
        && m.name == name
    {
        m.id = ObjectId(next);
        commands.push(Command::CreateMaterial { material: m });
    }
    commands
}

/// Copy the catalogue profile and steel a file needs into the model (one
/// undo step), so loading it can then find them by name.
pub fn import_catalogue(editor: &mut super::Editor, values: &Values) -> Result<(), String> {
    let commands = catalogue_commands(editor.document.project(), values);
    if commands.is_empty() {
        return Ok(());
    }
    editor.commit_data("Профиль и сталь из каталога", &commands)
}

fn profile_name(project: &Project, id: ObjectId) -> Option<String> {
    project.profiles.get(&id).map(|p| p.name.clone())
}
fn material_name(project: &Project, id: ObjectId) -> Option<String> {
    project.materials.get(&id).map(|m| m.name.clone())
}
fn level_value(project: &Project, level: Option<ObjectId>, value: &str) -> Option<LevelValue> {
    Some(LevelValue {
        level: match level {
            Some(id) => Some(project.levels.get(&id)?.name.clone()),
            None => None,
        },
        value: super::number(&value.replace(',', ".")).ok()?,
    })
}

/// Values of the create dialog for `kind`.
pub fn from_draft(project: &Project, draft: &Draft, kind: FileKind) -> Values {
    let member = || MemberValues {
        name: Some(draft.name.clone()),
        profile: profile_name(project, draft.profile),
        material: material_name(project, draft.material),
        position: Some(draft.position),
        bottom: None,
        top: None,
    };
    match kind {
        FileKind::Beam => Values::Beam(member()),
        FileKind::Column => Values::Column(MemberValues {
            bottom: level_value(project, draft.bottom_level, &draft.bottom),
            top: level_value(project, draft.top_level, &draft.top),
            ..member()
        }),
        FileKind::Plate => Values::Plate(PlateValues {
            name: Some(draft.name.clone()),
            material: material_name(project, draft.material),
            thickness: Some(draft.thickness.clone()),
            position: Some(draft.plate_position.label().into()),
        }),
        FileKind::Bolt => Values::Bolt(BoltValues::default()),
    }
}

/// Values of one beam or column of the model.
pub fn from_element(project: &Project, element: &open_bim_model::Element) -> Option<Values> {
    let open_bim_model::ElementShape::Member {
        member_kind,
        profile,
        position,
        ..
    } = &element.shape
    else {
        return None;
    };
    let mut values = MemberValues {
        name: Some(element.name.clone()),
        profile: profile_name(project, *profile),
        material: material_name(project, element.material),
        position: Some(*position),
        bottom: None,
        top: None,
    };
    Some(match member_kind {
        open_bim_model::MemberKind::Beam => Values::Beam(values),
        open_bim_model::MemberKind::Column => {
            use super::column_levels::*;
            let end = |level: &str, offset: &str| {
                let (id, offset) = bound(&element.properties, level, offset)?;
                Some(LevelValue {
                    level: Some(project.levels.get(&id)?.name.clone()),
                    value: offset,
                })
            };
            values.bottom = end(BOTTOM_LEVEL, BOTTOM_OFFSET);
            values.top = end(TOP_LEVEL, TOP_OFFSET);
            Values::Column(values)
        }
    })
}

fn find<'a, T: 'a>(
    items: impl IntoIterator<Item = (&'a ObjectId, &'a T)>,
    name: &str,
    of: fn(&T) -> &str,
) -> Option<ObjectId> {
    items
        .into_iter()
        .find(|(_, t)| of(t) == name)
        .map(|(id, _)| *id)
}
pub fn find_profile(project: &Project, name: &str) -> Option<ObjectId> {
    find(&project.profiles, name, |p| &p.name)
}
pub fn find_material(project: &Project, name: &str) -> Option<ObjectId> {
    find(&project.materials, name, |m| &m.name)
}
fn find_level(project: &Project, name: &str) -> Option<ObjectId> {
    find(&project.levels, name, |l| &l.name)
}

/// Resolve a column end against the project levels.
fn height(project: &Project, v: &LevelValue, warnings: &mut Vec<String>) -> Option<Height> {
    match &v.level {
        None => Some(Height {
            level: None,
            value: v.value,
        }),
        Some(name) => match find_level(project, name) {
            Some(id) => Some(Height {
                level: Some(id),
                value: v.value,
            }),
            None => {
                warnings.push(format!("нет уровня «{name}»"));
                None
            }
        },
    }
}

/// Fill the create dialog from a file; returns what could not be applied.
pub fn apply_to_draft(project: &Project, draft: &mut Draft, values: &Values) -> Vec<String> {
    let mut warnings = Vec::new();
    let material = |draft: &mut Draft, name: &Option<String>, warnings: &mut Vec<String>| {
        if let Some(name) = name {
            match find_material(project, name) {
                Some(id) => draft.material = id,
                None => warnings.push(format!("нет материала «{name}»")),
            }
        }
    };
    match values {
        Values::Beam(v) | Values::Column(v) => {
            if let Some(name) = &v.name {
                draft.name = name.clone();
            }
            if let Some(name) = &v.profile {
                match find_profile(project, name) {
                    Some(id) => draft.profile = id,
                    None => warnings.push(format!("нет профиля «{name}»")),
                }
            }
            material(draft, &v.material, &mut warnings);
            if let Some(p) = v.position {
                draft.position = p;
            }
            for (value, level, text) in [
                (&v.bottom, &mut draft.bottom_level, &mut draft.bottom),
                (&v.top, &mut draft.top_level, &mut draft.top),
            ] {
                if let Some(h) = value
                    .as_ref()
                    .and_then(|v| height(project, v, &mut warnings))
                {
                    *level = h.level;
                    *text = super::ui::mm_text(h.value);
                }
            }
        }
        Values::Plate(v) => {
            if let Some(name) = &v.name {
                draft.name = name.clone();
            }
            material(draft, &v.material, &mut warnings);
            if let Some(t) = &v.thickness {
                draft.thickness = t.clone();
            }
            if let Some(label) = &v.position {
                match PlatePosition::ALL.into_iter().find(|p| p.label() == label) {
                    Some(p) => draft.plate_position = p,
                    None => warnings.push(format!("неизвестное положение «{label}»")),
                }
            }
        }
        Values::Bolt(_) => {}
    }
    warnings
}

pub fn from_bolts(d: &DetailingUi) -> Values {
    Values::Bolt(BoltValues {
        standard: Some(d.standard.clone()),
        grade: Some(d.grade.clone()),
        diameter: Some(d.diameter.clone()),
        clearance: Some(d.clearance.clone()),
        steps_x: Some(d.steps_x.clone()),
        steps_y: Some(d.steps_y.clone()),
        shift_x: Some(d.shift_x.clone()),
        shift_y: Some(d.shift_y.clone()),
        rotate: Some(d.rotate),
        shape: Some(d.bolt_shape.label().into()),
        circle_count: Some(d.circle_count.clone()),
        circle_diameter: Some(d.circle_diameter.clone()),
    })
}

pub fn apply_to_bolts(d: &mut DetailingUi, values: &Values) {
    let Values::Bolt(v) = values else { return };
    for (value, field) in [
        (&v.standard, &mut d.standard),
        (&v.grade, &mut d.grade),
        (&v.diameter, &mut d.diameter),
        (&v.clearance, &mut d.clearance),
        (&v.steps_x, &mut d.steps_x),
        (&v.steps_y, &mut d.steps_y),
        (&v.shift_x, &mut d.shift_x),
        (&v.shift_y, &mut d.shift_y),
        (&v.circle_count, &mut d.circle_count),
        (&v.circle_diameter, &mut d.circle_diameter),
    ] {
        if let Some(value) = value {
            field.clone_from(value);
        }
    }
    if let Some(r) = v.rotate {
        d.rotate = r;
    }
    if let Some(shape) = v
        .shape
        .as_deref()
        .and_then(crate::tools::detailing_ui::BoltShape::from_label)
    {
        d.bolt_shape = shape;
    }
}

/// Message after loading: «Загружено «X»» plus what was skipped.
pub fn loaded_message(name: &str, warnings: &[String]) -> String {
    if warnings.is_empty() {
        format!("Загружены свойства «{name}»")
    } else {
        format!(
            "Загружены свойства «{name}», пропущено: {}",
            warnings.join(", ")
        )
    }
}

/// UI state of the property file row: chosen file and the «Сохранить как» name.
#[derive(Clone, Debug, Default)]
pub struct FilesUi {
    pub chosen: std::collections::BTreeMap<FileKind, String>,
    pub save_as: String,
}

/// What the property file row asks for.
#[derive(Clone, Debug, PartialEq)]
pub enum FileRequest {
    Load(String),
    Save(String),
}

impl FilesUi {
    /// File row: list of saved files (choosing loads), «Загрузить» and
    /// «Сохранить как» with the name in its popup.
    pub fn row(
        &mut self,
        ui: &mut eframe::egui::Ui,
        dir: &Path,
        kind: FileKind,
    ) -> Option<FileRequest> {
        use eframe::egui;
        let mut request = None;
        let names = list(dir, kind);
        let chosen = self.chosen.entry(kind).or_default();
        if !names.contains(chosen) {
            *chosen = names.first().cloned().unwrap_or_default();
        }
        // One row as in the pane header: the file list (choosing a file loads
        // it), «Загрузить» again, and the save button with the name to save as.
        ui.horizontal(|ui| {
            let before = chosen.clone();
            let width = (ui.available_width() - 84.).max(100.);
            egui::ComboBox::new(("property file", kind), "")
                .selected_text(if chosen.is_empty() {
                    "— нет —"
                } else {
                    chosen.as_str()
                })
                .width(width)
                .show_ui(ui, |ui| {
                    for n in &names {
                        ui.selectable_value(chosen, n.clone(), n);
                    }
                })
                .response
                .on_hover_text("Файл свойств: выбор файла загружает его в поля")
                .widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, "Файл свойств")
                });
            if *chosen != before && !chosen.is_empty() {
                request = Some(FileRequest::Load(chosen.clone()));
            }
            let load = crate::ui::icons::button(
                ui,
                crate::ui::icons::Icon::Undo,
                16.,
                None,
                "Загрузить",
                "Загрузить выбранный файл свойств ещё раз",
                !chosen.is_empty(),
                false,
            );
            if load.clicked() {
                request = Some(FileRequest::Load(chosen.clone()));
            }
            let save = crate::ui::icons::button(
                ui,
                crate::ui::icons::Icon::Save,
                16.,
                None,
                "Сохранить как",
                &format!(
                    "Сохранить свойства под именем — в папку {} рядом с файлом проекта; «standard» загружается при запуске команды",
                    dir.display()
                ),
                true,
                false,
            );
            egui::Popup::menu(&save)
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                .show(|ui| {
                    let l = ui.label("Имя");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.save_as)
                            .id(egui::Id::new(("property file name", kind)))
                            .hint_text(STANDARD)
                            .desired_width(160.),
                    )
                    .labelled_by(l.id);
                    if ui.button("Сохранить").clicked() {
                        let name = if self.save_as.trim().is_empty() {
                            STANDARD.to_string()
                        } else {
                            self.save_as.trim().to_string()
                        };
                        request = Some(FileRequest::Save(name));
                        ui.close();
                    }
                });
        });
        if let Some(FileRequest::Save(name)) = &request {
            self.chosen.insert(kind, name.clone());
        }
        request
    }
}
