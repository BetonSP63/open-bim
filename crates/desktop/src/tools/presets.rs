//! Property files of the create dialogs («Сохранить как» / «Загрузить»):
//! named sets of beam, column and plate properties kept in the user's
//! settings folder, so they work in every project. Profiles and steels are
//! stored by name and taken from the project or the ГОСТ catalogue when the
//! set is loaded.
use super::{Draft, ToolKind};
use crate::ui::catalog_picker::{Pick, material_by_grade, profile_by_name};
use open_bim_commands::Command;
use open_bim_model::{ObjectId, PartPosition, Project};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preset {
    pub name: String,
    /// «beam», «column», «plate», «gusset», «stiffener».
    pub tool: String,
    pub part_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// Material name as shown («С345 · ГОСТ 27772-2015»).
    pub material: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thickness: Option<String>,
    #[serde(default, skip_serializing_if = "PartPosition::is_default")]
    pub position: PartPosition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottom: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<String>,
}

pub fn tool_key(kind: ToolKind) -> &'static str {
    match kind {
        ToolKind::Beam => "beam",
        ToolKind::Column => "column",
        ToolKind::Plate => "plate",
        ToolKind::Gusset => "gusset",
        ToolKind::Stiffener => "stiffener",
        ToolKind::Axis => "axis",
    }
}

impl Preset {
    /// The draft's properties under `name` (picked points are not saved).
    pub fn from_draft(name: &str, draft: &Draft, project: &Project) -> Result<Self, String> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 60 {
            return Err("Имя набора свойств: 1–60 символов".into());
        }
        let member = matches!(draft.kind, ToolKind::Beam | ToolKind::Column);
        let column = draft.kind == ToolKind::Column;
        Ok(Self {
            name: name.into(),
            tool: tool_key(draft.kind).into(),
            part_name: draft.name.clone(),
            profile: member
                .then(|| project.profiles.get(&draft.profile).map(|p| p.name.clone()))
                .flatten(),
            material: project
                .materials
                .get(&draft.material)
                .map(|m| m.name.clone())
                .unwrap_or_default(),
            thickness: draft.kind.is_plate().then(|| draft.thickness.clone()),
            position: if member {
                draft.position
            } else {
                PartPosition::default()
            },
            bottom: column.then(|| draft.bottom.clone()),
            top: column.then(|| draft.top.clone()),
        })
    }

    /// Fill the draft from the set. Returns the commands that copy a
    /// catalogue profile or steel into the project (empty when the project
    /// already has them); an unknown profile or steel is an error and the
    /// draft is left unchanged.
    pub fn apply(&self, draft: &mut Draft, project: &Project) -> Result<Vec<Command>, String> {
        if self.tool != tool_key(draft.kind) {
            return Err("Набор свойств относится к другому инструменту".into());
        }
        let mut commands = Vec::new();
        let mut next = project.next_id;
        let profile = match &self.profile {
            None => None,
            Some(name) => Some(match profile_by_name(project, name) {
                Some(Pick::Project(id)) => id,
                Some(Pick::New(mut p)) => {
                    p.id = ObjectId(next);
                    next += 1;
                    let id = p.id;
                    commands.push(Command::CreateProfile { profile: p });
                    id
                }
                None => return Err(format!("Нет профиля «{name}» ни в проекте, ни в каталоге")),
            }),
        };
        let material = match project.materials.values().find(|m| m.name == self.material) {
            Some(m) => m.id,
            None => {
                let grade = self.material.split(" · ").next().unwrap_or_default();
                match material_by_grade(project, grade) {
                    Some(Pick::New(mut m)) => {
                        m.id = ObjectId(next);
                        let id = m.id;
                        commands.push(Command::CreateMaterial { material: m });
                        id
                    }
                    Some(Pick::Project(id)) => id,
                    None => {
                        return Err(format!(
                            "Нет материала «{}» ни в проекте, ни в каталоге",
                            self.material
                        ));
                    }
                }
            }
        };
        draft.name = self.part_name.clone();
        if let Some(p) = profile {
            draft.profile = p;
        }
        draft.material = material;
        if let Some(t) = &self.thickness {
            draft.thickness = t.clone();
        }
        draft.position = self.position;
        if let Some(b) = &self.bottom {
            draft.bottom = b.clone();
        }
        if let Some(t) = &self.top {
            draft.top = t.clone();
        }
        Ok(commands)
    }
}

/// The property sets file: `OPEN_BIM_PRESETS`, else
/// `~/Library/Application Support/Open BIM/properties.json` on macOS and
/// `~/.config/open-bim/properties.json` elsewhere.
pub fn store_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("OPEN_BIM_PRESETS") {
        return Some(PathBuf::from(p));
    }
    let home = PathBuf::from(std::env::var_os("HOME")?);
    Some(if cfg!(target_os = "macos") {
        home.join("Library/Application Support/Open BIM/properties.json")
    } else {
        home.join(".config/open-bim/properties.json")
    })
}

/// Saved sets, sorted by tool and name.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PresetStore {
    pub presets: Vec<Preset>,
}

impl PresetStore {
    /// A missing file is an empty store; a broken one is an error.
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|e| format!("Файл наборов свойств повреждён: {e}")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(format!("Не удалось прочитать наборы свойств: {e}")),
        }
    }

    pub fn save(&self, path: &std::path::Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        // Write a temporary file first so a crash never leaves half a file.
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }

    /// Add or replace the set with the same tool and name.
    pub fn put(&mut self, preset: Preset) {
        self.presets
            .retain(|p| !(p.tool == preset.tool && p.name == preset.name));
        self.presets.push(preset);
        self.presets
            .sort_by(|a, b| (&a.tool, &a.name).cmp(&(&b.tool, &b.name)));
    }

    pub fn for_tool(&self, kind: ToolKind) -> impl Iterator<Item = &Preset> {
        let key = tool_key(kind);
        self.presets.iter().filter(move |p| p.tool == key)
    }
}
