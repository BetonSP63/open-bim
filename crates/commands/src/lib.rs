//! Transactional model editing with delta history and dependency-aware events.
mod patch;

use open_bim_model::{
    Element, Frame, GridAxis, ModelError, ObjectId, Point3, Project, PropertyValue,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeletePolicy {
    RejectDependencies,
    Cascade,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Command {
    CreateAssembly {
        assembly: open_bim_model::Assembly,
    },
    ReplaceAssembly {
        assembly: open_bim_model::Assembly,
    },
    CreateDrawing {
        drawing: open_bim_model::Drawing,
    },
    ReplaceDrawing {
        drawing: open_bim_model::Drawing,
    },
    CreateConnection {
        connection: open_bim_model::ConnectionInstance,
    },
    ReplaceConnection {
        connection: open_bim_model::ConnectionInstance,
    },
    CreateDetail {
        detail: open_bim_model::Detail,
    },
    ReplaceDetail {
        detail: open_bim_model::Detail,
    },
    Create {
        element: Element,
    },
    CreateAxis {
        axis: GridAxis,
    },
    ReplaceLevel {
        level: open_bim_model::Level,
    },
    ReplaceAxis {
        axis: GridAxis,
    },
    CreateLevel {
        level: open_bim_model::Level,
    },
    Delete {
        id: ObjectId,
        policy: DeletePolicy,
    },
    Transform {
        ids: Vec<ObjectId>,
        rotation: Frame,
        pivot: Point3,
    },
    ChangeProperty {
        id: ObjectId,
        key: String,
        value: Option<PropertyValue>,
    },
    ReplaceElement {
        element: Element,
    },
    /// Main inscription attributes of the project (ГОСТ Р 21.101, прил. Е).
    SetDocumentInfo {
        info: Box<open_bim_model::DocumentInfo>,
    },
    RenameProject {
        name: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandError {
    Model(ModelError),
    Dependencies(ObjectId),
    NoHistory,
    InvalidTransaction(String),
}

impl From<ModelError> for CommandError {
    fn from(value: ModelError) -> Self {
        Self::Model(value)
    }
}
impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Model(error) => error.fmt(f),
            Self::Dependencies(id) => write!(
                f,
                "Объект {id} имеет зависимости; выберите каскадное удаление"
            ),
            Self::NoHistory => f.write_str("Нет операций для отмены или повтора"),
            Self::InvalidTransaction(message) => f.write_str(message),
        }
    }
}
impl std::error::Error for CommandError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangeEvent {
    pub revision: u64,
    pub serial: u64,
    pub touched: BTreeSet<ObjectId>,
    pub label: String,
}

#[derive(Clone, Debug)]
pub struct Document {
    project: Project,
    undo: Vec<patch::Patch>,
    redo: Vec<patch::Patch>,
    serial: u64,
    history_limit: usize,
}

impl Document {
    pub fn new(project: Project) -> Result<Self, CommandError> {
        project.validate()?;
        Ok(Self {
            project,
            undo: Vec::new(),
            redo: Vec::new(),
            serial: 0,
            history_limit: 256,
        })
    }
    pub fn project(&self) -> &Project {
        &self.project
    }
    pub fn history_depth(&self) -> (usize, usize) {
        (self.undo.len(), self.redo.len())
    }

    pub fn transact(
        &mut self,
        label: impl Into<String>,
        commands: &[Command],
    ) -> Result<ChangeEvent, CommandError> {
        let label = label.into();
        if commands.is_empty() || commands.len() > 100000 || label.len() > 16384 {
            return Err(CommandError::InvalidTransaction(
                "Недопустимая транзакция".into(),
            ));
        }
        let mut staged = self.project.clone();
        for command in commands {
            apply_command(&mut staged, command)?;
        }
        open_bim_connections::reject_generated_geometry_edits(&staged, &self.project)?;
        open_bim_connections::rebuild_base_plates(&mut staged, &self.project)?;
        open_bim_connections::rebuild_end_plates(&mut staged, &self.project)?;
        open_bim_connections::rebuild_clip_angles(&mut staged, &self.project)?;
        staged.validate()?;
        staged.revision = staged
            .revision
            .checked_add(1)
            .ok_or_else(|| CommandError::InvalidTransaction("Исчерпан диапазон ревизий".into()))?;
        let serial = self
            .serial
            .checked_add(1)
            .ok_or_else(|| CommandError::InvalidTransaction("Исчерпан диапазон событий".into()))?;
        let patch = patch::Patch::between(&self.project, &staged, label);
        let event = ChangeEvent {
            revision: staged.revision,
            serial,
            touched: patch.touched.clone(),
            label: patch.label.clone(),
        };
        self.project = staged;
        self.serial = serial;
        self.undo.push(patch);
        self.redo.clear();
        if self.undo.len() > self.history_limit {
            self.undo.remove(0);
        }
        Ok(event)
    }

    pub fn undo(&mut self) -> Result<ChangeEvent, CommandError> {
        self.move_history(false)
    }
    pub fn redo(&mut self) -> Result<ChangeEvent, CommandError> {
        self.move_history(true)
    }

    fn move_history(&mut self, forward: bool) -> Result<ChangeEvent, CommandError> {
        let serial = self
            .serial
            .checked_add(1)
            .ok_or_else(|| CommandError::InvalidTransaction("Исчерпан диапазон событий".into()))?;
        let source = if forward {
            &mut self.redo
        } else {
            &mut self.undo
        };
        let patch = source.last().ok_or(CommandError::NoHistory)?;
        let mut staged = self.project.clone();
        patch.apply(&mut staged, forward);
        staged.validate()?;
        let event = ChangeEvent {
            revision: staged.revision,
            serial,
            touched: patch.touched.clone(),
            label: patch.label.clone(),
        };
        let patch = source.pop().ok_or(CommandError::NoHistory)?;
        self.project = staged;
        self.serial = serial;
        if forward {
            self.undo.push(patch);
        } else {
            self.redo.push(patch);
        }
        Ok(event)
    }
}

pub fn project_hash(project: &Project) -> Result<String, ModelError> {
    let canonical = project.to_json()?;
    Ok(Sha256::digest(canonical.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

/// Cache entries declare source IDs; model events invalidate entries by dependency.
#[derive(Clone, Debug)]
pub struct DependencyCache<T> {
    entries: BTreeMap<String, (BTreeSet<ObjectId>, T)>,
}

impl<T> Default for DependencyCache<T> {
    fn default() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }
}

impl<T> DependencyCache<T> {
    pub fn insert(&mut self, key: String, dependencies: BTreeSet<ObjectId>, value: T) {
        self.entries.insert(key, (dependencies, value));
    }
    pub fn get(&self, key: &str) -> Option<&T> {
        self.entries.get(key).map(|(_, value)| value)
    }
    pub fn invalidate(&mut self, event: &ChangeEvent) -> usize {
        let previous = self.entries.len();
        self.entries
            .retain(|_, (dependencies, _)| dependencies.is_disjoint(&event.touched));
        previous - self.entries.len()
    }
}

fn apply_command(project: &mut Project, command: &Command) -> Result<(), CommandError> {
    match command {
        Command::CreateConnection { connection } => {
            if project.contains_id(connection.id) || connection.id.0 == 0 {
                return Err(ModelError::DuplicateId(connection.id).into());
            }
            if !connection.children.is_empty() {
                return Err(CommandError::InvalidTransaction(
                    "Новый узел не должен содержать внешние ID деталей".into(),
                ));
            }
            project.next_id =
                project
                    .next_id
                    .max(connection.id.0.checked_add(1).ok_or_else(|| {
                        CommandError::InvalidTransaction("Недопустимый ID узла".into())
                    })?);
            project
                .connections
                .insert(connection.id, connection.clone());
        }
        Command::ReplaceConnection { connection } => {
            let old = project
                .connections
                .get(&connection.id)
                .ok_or(ModelError::MissingReference(connection.id))?;
            if old.children != connection.children {
                return Err(CommandError::InvalidTransaction(
                    "ID деталей узла управляются генератором".into(),
                ));
            }
            project
                .connections
                .insert(connection.id, connection.clone());
        }
        Command::CreateDetail { detail } => {
            if project.contains_id(detail.id) || detail.id.0 == 0 {
                return Err(ModelError::DuplicateId(detail.id).into());
            }
            project.next_id =
                project.next_id.max(
                    detail.id.0.checked_add(1).ok_or_else(|| {
                        CommandError::InvalidTransaction("Invalid detail ID".into())
                    })?,
                );
            project.details.insert(detail.id, detail.clone());
        }
        Command::ReplaceDetail { detail } => {
            if !project.details.contains_key(&detail.id) {
                return Err(ModelError::MissingReference(detail.id).into());
            }
            project.details.insert(detail.id, detail.clone());
        }
        Command::CreateAssembly { assembly } => {
            if project.contains_id(assembly.id) || assembly.id.0 == 0 {
                return Err(ModelError::DuplicateId(assembly.id).into());
            }
            project.next_id =
                project
                    .next_id
                    .max(assembly.id.0.checked_add(1).ok_or_else(|| {
                        CommandError::InvalidTransaction("Недопустимый ID сборки".into())
                    })?);
            project.assemblies.insert(assembly.id, assembly.clone());
        }
        Command::ReplaceAssembly { assembly } => {
            if !project.assemblies.contains_key(&assembly.id) {
                return Err(ModelError::MissingReference(assembly.id).into());
            }
            project.assemblies.insert(assembly.id, assembly.clone());
        }
        Command::CreateDrawing { drawing } => {
            if project.contains_id(drawing.id) || drawing.id.0 == 0 {
                return Err(ModelError::DuplicateId(drawing.id).into());
            }
            project.next_id =
                project
                    .next_id
                    .max(drawing.id.0.checked_add(1).ok_or_else(|| {
                        CommandError::InvalidTransaction("Недопустимый ID чертежа".into())
                    })?);
            project.drawings.insert(drawing.id, drawing.clone());
        }
        Command::ReplaceDrawing { drawing } => {
            if !project.drawings.contains_key(&drawing.id) {
                return Err(ModelError::MissingReference(drawing.id).into());
            }
            project.drawings.insert(drawing.id, drawing.clone());
        }
        Command::Create { element } => {
            if project.contains_id(element.id) || element.id.0 == 0 {
                return Err(ModelError::DuplicateId(element.id).into());
            }
            project.next_id =
                project.next_id.max(
                    element.id.0.checked_add(1).ok_or_else(|| {
                        CommandError::InvalidTransaction("Недопустимый ID".into())
                    })?,
                );
            project.elements.insert(element.id, element.clone());
        }
        Command::CreateAxis { axis } => {
            if project.contains_id(axis.id) || axis.id.0 == 0 {
                return Err(ModelError::DuplicateId(axis.id).into());
            }
            project.next_id = project.next_id.max(
                axis.id
                    .0
                    .checked_add(1)
                    .ok_or_else(|| CommandError::InvalidTransaction("Недопустимый ID".into()))?,
            );
            project.axes.insert(axis.id, axis.clone());
        }
        Command::CreateLevel { level } => {
            if project.contains_id(level.id) || level.id.0 == 0 {
                return Err(ModelError::DuplicateId(level.id).into());
            }
            project.next_id =
                project.next_id.max(
                    level.id.0.checked_add(1).ok_or_else(|| {
                        CommandError::InvalidTransaction("Недопустимый ID".into())
                    })?,
                );
            project.levels.insert(level.id, level.clone());
        }
        Command::ReplaceLevel { level } => {
            if !project.levels.contains_key(&level.id) {
                return Err(ModelError::MissingReference(level.id).into());
            }
            if !level.elevation_mm.is_finite() || level.elevation_mm.abs() > 1e8 {
                return Err(ModelError::InvalidValue("Отметка уровня вне диапазона".into()).into());
            }
            project.levels.insert(level.id, level.clone());
        }
        Command::ReplaceAxis { axis } => {
            if !project.axes.contains_key(&axis.id) {
                return Err(ModelError::MissingReference(axis.id).into());
            }
            project.axes.insert(axis.id, axis.clone());
        }
        Command::ReplaceElement { element } => {
            if !project.elements.contains_key(&element.id) {
                return Err(ModelError::MissingReference(element.id).into());
            }
            project.elements.insert(element.id, element.clone());
        }
        Command::Delete { id, policy } => delete(project, *id, *policy)?,
        Command::Transform {
            ids,
            rotation,
            pivot,
        } => {
            rotation.validate()?;
            if !pivot.0.iter().all(|v| v.is_finite() && v.abs() <= 1e9)
                || ids.is_empty()
                || ids.iter().copied().collect::<BTreeSet<_>>().len() != ids.len()
            {
                return Err(CommandError::InvalidTransaction(
                    "Недопустимое преобразование".into(),
                ));
            }
            for id in ids {
                let element = project
                    .elements
                    .get_mut(id)
                    .ok_or(ModelError::MissingReference(*id))?;
                let frame = element.shape.frame_mut();
                let relative = Point3(std::array::from_fn(|i| frame.origin.0[i] - pivot.0[i]));
                let transformed = rotation.world(relative);
                frame.origin = Point3(std::array::from_fn(|i| transformed.0[i] + pivot.0[i]));
                for axis in &mut frame.axes {
                    *axis = std::array::from_fn(|i| {
                        (0..3).map(|j| rotation.axes[j][i] * axis[j]).sum::<f64>()
                    });
                }
            }
        }
        Command::ChangeProperty { id, key, value } => {
            let properties = if let Some(element) = project.elements.get_mut(id) {
                &mut element.properties
            } else if let Some(assembly) = project.assemblies.get_mut(id) {
                &mut assembly.properties
            } else {
                return Err(ModelError::MissingReference(*id).into());
            };
            match value {
                Some(value) => {
                    properties.insert(key.clone(), value.clone());
                }
                None => {
                    properties.remove(key);
                }
            }
        }
        Command::RenameProject { name } => project.name = name.clone(),
        Command::SetDocumentInfo { info } => {
            info.validate()?;
            project.document = info.as_ref().clone();
        }
    }
    Ok(())
}

fn delete(project: &mut Project, id: ObjectId, policy: DeletePolicy) -> Result<(), CommandError> {
    if !project.contains_id(id) {
        return Err(ModelError::MissingReference(id).into());
    }
    let mut removed = BTreeSet::from([id]);
    loop {
        let previous = removed.len();
        for connection in project.connections.values() {
            if removed.contains(&connection.primary)
                || connection.secondary.is_some_and(|id| removed.contains(&id))
                || connection.children.values().any(|id| removed.contains(id))
            {
                removed.insert(connection.id);
            }
            if removed.contains(&connection.id) {
                removed.extend(connection.children.values());
            }
        }
        for element in project.elements.values() {
            let uses_profile = match &element.shape {
                open_bim_model::ElementShape::Member { profile, .. } => removed.contains(profile),
                _ => false,
            };
            if removed.contains(&element.material) || uses_profile {
                removed.insert(element.id);
            }
        }
        for assembly in project.assemblies.values() {
            if removed.contains(&assembly.main_part) {
                removed.insert(assembly.id);
            }
            if removed.contains(&assembly.id) {
                removed.extend(&assembly.members);
                removed.extend(&assembly.children);
            }
        }
        for detail in project.details.values() {
            if !detail.kind.references().is_disjoint(&removed) {
                removed.insert(detail.id);
            }
            if removed.contains(&detail.id)
                && let open_bim_model::DetailKind::BoltGroup { holes, .. } = &detail.kind
            {
                removed.extend(holes);
            }
        }
        if previous == removed.len() {
            break;
        }
    }
    let linked = removed.len() > 1
        || project
            .assemblies
            .values()
            .any(|a| a.members.contains(&id) || a.children.contains(&id));
    if policy == DeletePolicy::RejectDependencies && linked {
        return Err(CommandError::Dependencies(id));
    }
    for id in &removed {
        project.elements.remove(id);
        project.assemblies.remove(id);
        project.materials.remove(id);
        project.profiles.remove(id);
        project.axes.remove(id);
        project.levels.remove(id);
        project.details.remove(id);
        project.connections.remove(id);
        project.drawings.remove(id);
    }
    for assembly in project.assemblies.values_mut() {
        assembly.members.retain(|id| !removed.contains(id));
        assembly.children.retain(|id| !removed.contains(id));
    }
    Ok(())
}
