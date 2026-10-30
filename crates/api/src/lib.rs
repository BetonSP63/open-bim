//! Object API over a project for scripts, batch runs and future extensions.
//!
//! The layers stay separate, the way a person works in the program:
//! * an object value ([`Member`], [`Plate`]) is plain data; changing its
//!   fields changes nothing in the model;
//! * `insert` / `modify` / `delete` check the object and stage a change in
//!   the session; later queries already see it;
//! * `commit` turns all staged changes into one undoable step (connections
//!   are rebuilt and the whole model is validated there); `rollback` drops
//!   them;
//! * `save` writes the committed model to disk and refuses while changes
//!   are staged, as do the exports.
use open_bim_commands::{ChangeEvent, Command, CommandError, DeletePolicy, Document};
use open_bim_model::{
    Element, ElementShape, Frame, LengthMm, MemberKind, ModelError, ObjectId, PartPosition, Point3,
    Project, PropertyValue, library,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub mod batch;
pub mod drawings;
pub mod plugins;

pub use drawings::{DrawingEdit, DrawingInfo, DrawingRules, Note};

/// Distance below which two points are the same point, mm.
pub const POINT_TOLERANCE_MM: f64 = 0.001;

#[derive(Debug, Clone, PartialEq)]
pub enum ApiError {
    /// The object values cannot be inserted or modified as given.
    Invalid(String),
    NotFound(ObjectId),
    /// `modify` / `delete` of an object that has no ID yet.
    NotInserted,
    /// Save or export with staged changes: commit or roll back first.
    Uncommitted(usize),
    Command(CommandError),
    Io(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(message) | Self::Io(message) => f.write_str(message),
            Self::NotFound(id) => write!(f, "Объект {id} не найден"),
            Self::NotInserted => f.write_str("Объект ещё не вставлен в модель: нет ID"),
            Self::Uncommitted(n) => write!(
                f,
                "Есть незафиксированные изменения ({n}): выполните фиксацию или откат"
            ),
            Self::Command(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for ApiError {}

impl From<CommandError> for ApiError {
    fn from(value: CommandError) -> Self {
        Self::Command(value)
    }
}

impl From<ModelError> for ApiError {
    fn from(value: ModelError) -> Self {
        Self::Command(CommandError::Model(value))
    }
}

pub type Result<T> = std::result::Result<T, ApiError>;

fn invalid<T>(message: impl Into<String>) -> Result<T> {
    Err(ApiError::Invalid(message.into()))
}

/// A beam or column on its reference line.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Member {
    /// Set by `insert`; required by `modify`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    #[serde(default = "beam")]
    pub kind: MemberKind,
    #[serde(default)]
    pub name: String,
    /// Profile name as in the model or the built-in catalogue («30Б1 (Р 57837)»).
    pub profile: String,
    /// Steel grade («С255») or a material name of the model.
    pub material: String,
    pub start: Point3,
    pub end: Point3,
    #[serde(default, skip_serializing_if = "PartPosition::is_default")]
    pub position: PartPosition,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<String, PropertyValue>,
}

fn beam() -> MemberKind {
    MemberKind::Beam
}

/// A plate given by its outline in world coordinates; the plate lies on the
/// side of the outline's normal (right-hand rule).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plate {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    #[serde(default)]
    pub name: String,
    pub material: String,
    pub thickness: f64,
    pub outline: Vec<Point3>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<String, PropertyValue>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectKind {
    Beam,
    Column,
    Plate,
}

/// Axis-aligned box in world coordinates, mm.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Box3 {
    pub min: Point3,
    pub max: Point3,
}

/// Part selection: every given condition must hold; an empty query selects
/// all parts.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Query {
    /// Any of these kinds (empty = any kind).
    pub kinds: Vec<ObjectKind>,
    pub name: Option<String>,
    pub profile: Option<String>,
    pub material: Option<String>,
    /// The property has exactly this value.
    pub property: Option<(String, PropertyValue)>,
    /// The part box lies completely inside.
    pub inside: Option<Box3>,
    /// The part box touches or overlaps.
    pub touching: Option<Box3>,
}

/// A working session on one model.
pub struct Session {
    document: Document,
    /// Committed model with the staged commands applied.
    working: Project,
    staged: Vec<Command>,
    path: Option<PathBuf>,
    open_drawing: Option<drawings::OpenDrawing>,
    /// Plugins known to the session: placed plugins are run again by name.
    plugins: Vec<plugins::Plugin>,
    /// Problems of the last dependent plugin runs at commit.
    plugin_problems: Vec<String>,
    /// IDs of the previous outputs a plugin run gives again to the objects
    /// it creates, in order.
    reuse: std::collections::VecDeque<ObjectId>,
}

impl Session {
    pub fn new(project: Project) -> Result<Self> {
        let document = Document::new(project)?;
        Ok(Self {
            working: document.project().clone(),
            document,
            staged: Vec::new(),
            path: None,
            open_drawing: None,
            plugins: Vec::new(),
            plugin_problems: Vec::new(),
            reuse: Default::default(),
        })
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let project = open_bim_persistence::load(path.as_ref())
            .map_err(|e| ApiError::Io(format!("{}: {e}", path.as_ref().display())))?;
        let mut session = Self::new(project)?;
        session.path = Some(path.as_ref().to_path_buf());
        Ok(session)
    }

    /// The model as queries see it: committed state plus staged changes.
    pub fn project(&self) -> &Project {
        &self.working
    }

    /// The last committed model.
    pub fn committed(&self) -> &Project {
        self.document.project()
    }

    pub fn staged(&self) -> usize {
        self.staged.len()
    }

    /// The staged commands, for a host that applies them in its own
    /// document as one undo step (the desktop runs plugins this way).
    pub fn into_staged(self) -> Vec<Command> {
        self.staged
    }

    fn stage(&mut self, commands: Vec<Command>) -> Result<()> {
        let mut next = self.working.clone();
        open_bim_commands::apply_staged(&mut next, &commands)?;
        self.working = next;
        self.staged.extend(commands);
        Ok(())
    }

    /// Plugins the session runs again when their inputs change.
    pub fn set_plugins(&mut self, plugins: Vec<plugins::Plugin>) {
        self.plugins = plugins;
    }

    /// Placed plugins the last commit could not run again.
    pub fn plugin_problems(&self) -> &[String] {
        &self.plugin_problems
    }

    /// One undoable step from all staged changes. Nothing staged → `None`.
    /// Placed plugins whose input parts changed run again in the same step.
    pub fn commit(&mut self, label: &str) -> Result<Option<ChangeEvent>> {
        if self.staged.is_empty() {
            return Ok(None);
        }
        let label = if label.trim().is_empty() {
            "Изменения через API"
        } else {
            label
        };
        let mut event = self.document.transact(label, &self.staged)?;
        self.staged.clear();
        self.working = self.document.project().clone();
        self.plugin_problems.clear();
        if !Self::dependent_plugins(&self.working, &event.touched).is_empty() {
            self.plugin_problems = self.rerun_dependent(&event.touched);
            if !self.staged.is_empty() {
                match self.document.amend(&self.staged) {
                    Ok(amended) => event = amended,
                    Err(e) => self.plugin_problems.push(e.to_string()),
                }
                self.staged.clear();
                self.working = self.document.project().clone();
            }
        }
        Ok(Some(event))
    }

    /// ID for a created part: the next previous output of a plugin being
    /// run again, else a new one. `true` when the part already exists.
    fn element_id(&mut self, next: &mut u64) -> (ObjectId, bool) {
        while let Some(id) = self.reuse.pop_front() {
            if self.working.elements.contains_key(&id) {
                return (id, true);
            }
        }
        (take_id(next), false)
    }

    /// Drop all staged changes and close the open drawing without saving.
    pub fn rollback(&mut self) -> usize {
        self.open_drawing = None;
        let n = self.staged.len();
        self.staged.clear();
        self.working = self.document.project().clone();
        n
    }

    pub fn undo(&mut self) -> Result<ChangeEvent> {
        self.require_committed()?;
        self.refuse_any_open("отменить изменения")?;
        let event = self.document.undo()?;
        self.working = self.document.project().clone();
        Ok(event)
    }

    fn require_committed(&self) -> Result<()> {
        match self.staged.len() {
            0 => Ok(()),
            n => Err(ApiError::Uncommitted(n)),
        }
    }

    /// Write the committed model; `None` writes back to the opened file.
    pub fn save(&mut self, path: Option<&Path>) -> Result<PathBuf> {
        self.require_committed()?;
        let path = path
            .map(Path::to_path_buf)
            .or_else(|| self.path.clone())
            .ok_or_else(|| ApiError::Io("Не задан файл для сохранения".into()))?;
        open_bim_persistence::save_atomic(&path, self.document.project())
            .map_err(|e| ApiError::Io(format!("{}: {e}", path.display())))?;
        self.path = Some(path.clone());
        Ok(path)
    }

    /// The committed model for exports and reports.
    pub fn for_export(&self) -> Result<&Project> {
        self.require_committed()?;
        Ok(self.document.project())
    }

    fn profile_id(
        &self,
        name: &str,
        commands: &mut Vec<Command>,
        next: &mut u64,
    ) -> Result<ObjectId> {
        let name = name.trim();
        if name.is_empty() {
            return invalid("Не задан профиль");
        }
        if let Some(p) = self.working.profiles.values().find(|p| p.name == name) {
            return Ok(p.id);
        }
        if let Some(profile) = commands.iter().find_map(|c| match c {
            Command::CreateProfile { profile } if profile.name == name => Some(profile.id),
            _ => None,
        }) {
            return Ok(profile);
        }
        let entry = library::section(name)
            .ok_or_else(|| ApiError::Invalid(format!("Профиль «{name}» не найден в каталоге")))?;
        let id = take_id(next);
        commands.push(Command::CreateProfile {
            profile: entry.profile(id),
        });
        Ok(id)
    }

    fn material_id(
        &self,
        name: &str,
        commands: &mut Vec<Command>,
        next: &mut u64,
    ) -> Result<ObjectId> {
        let name = name.trim();
        if name.is_empty() {
            return invalid("Не задан материал");
        }
        let matches = |m: &str| m == name || m.split(" · ").next() == Some(name);
        if let Some(m) = self.working.materials.values().find(|m| matches(&m.name)) {
            return Ok(m.id);
        }
        if let Some(id) = commands.iter().find_map(|c| match c {
            Command::CreateMaterial { material } if matches(&material.name) => Some(material.id),
            _ => None,
        }) {
            return Ok(id);
        }
        let id = take_id(next);
        let material = library::steel(id, name)
            .ok_or_else(|| ApiError::Invalid(format!("Материал «{name}» не найден в каталоге")))?;
        commands.push(Command::CreateMaterial { material });
        Ok(id)
    }

    fn member_shape(
        &self,
        member: &Member,
        commands: &mut Vec<Command>,
        next: &mut u64,
    ) -> Result<(ElementShape, ObjectId)> {
        if member.start.distance(member.end) <= POINT_TOLERANCE_MM {
            return invalid("Начальная и конечная точки совпадают");
        }
        let profile = self.profile_id(&member.profile, commands, next)?;
        let material = self.material_id(&member.material, commands, next)?;
        // Catalogue profiles staged in this call are not in `working` yet.
        let mut scratch = self.working.clone();
        open_bim_commands::apply_staged(&mut scratch, commands)?;
        let shape = scratch.new_member(
            member.kind,
            profile,
            member.start,
            member.end,
            member.position,
        )?;
        Ok((shape, material))
    }

    /// Check the member, stage it and return its new ID (also written to
    /// `member.id`).
    pub fn insert_member(&mut self, member: &mut Member) -> Result<ObjectId> {
        if member.id.is_some() {
            return invalid("Объект уже вставлен; используйте изменение");
        }
        let mut next = self.working.next_id;
        let mut commands = Vec::new();
        let (shape, material) = self.member_shape(member, &mut commands, &mut next)?;
        let (id, existing) = self.element_id(&mut next);
        commands.push(place(
            Element {
                id,
                name: member.name.clone(),
                material,
                shape,
                properties: member.properties.clone(),
            },
            existing,
        ));
        self.stage(commands)?;
        member.id = Some(id);
        Ok(id)
    }

    /// Replace the stored member by these values; the ID is kept.
    pub fn modify_member(&mut self, member: &Member) -> Result<()> {
        let id = member.id.ok_or(ApiError::NotInserted)?;
        match self.working.elements.get(&id).map(|e| &e.shape) {
            Some(ElementShape::Member { .. }) => {}
            Some(_) => return invalid(format!("Объект {id} — не балка и не колонна")),
            None => return Err(ApiError::NotFound(id)),
        }
        let mut next = self.working.next_id;
        let mut commands = Vec::new();
        let (shape, material) = self.member_shape(member, &mut commands, &mut next)?;
        commands.push(Command::ReplaceElement {
            element: Element {
                id,
                name: member.name.clone(),
                material,
                shape,
                properties: member.properties.clone(),
            },
        });
        self.stage(commands)
    }

    /// The stored member as an object value.
    pub fn select_member(&self, id: ObjectId) -> Result<Member> {
        let p = &self.working;
        if p.swept_parts.contains_key(&id) {
            return invalid("Чтение гнутой балки как прямой не поддерживается");
        }
        let element = p.elements.get(&id).ok_or(ApiError::NotFound(id))?;
        let ElementShape::Member {
            member_kind,
            profile,
            position,
            ..
        } = &element.shape
        else {
            return invalid(format!("Объект {id} — не балка и не колонна"));
        };
        let (start, end, _) = p
            .member_reference_line(&element.shape)
            .ok_or(ApiError::NotFound(*profile))?;
        Ok(Member {
            id: Some(id),
            kind: *member_kind,
            name: element.name.clone(),
            profile: p.profiles[profile].name.clone(),
            material: material_name(p, element.material),
            start,
            end,
            position: *position,
            properties: element.properties.clone(),
        })
    }

    fn plate_shape(
        &self,
        plate: &Plate,
        commands: &mut Vec<Command>,
        next: &mut u64,
    ) -> Result<(ElementShape, ObjectId)> {
        let thickness = LengthMm::new(plate.thickness)
            .map_err(|_| ApiError::Invalid("Толщина пластины должна быть положительной".into()))?;
        let (frame, contour) = plane_outline(&plate.outline)?;
        let material = self.material_id(&plate.material, commands, next)?;
        Ok((
            ElementShape::Plate {
                contour,
                thickness,
                frame,
            },
            material,
        ))
    }

    pub fn insert_plate(&mut self, plate: &mut Plate) -> Result<ObjectId> {
        if plate.id.is_some() {
            return invalid("Объект уже вставлен; используйте изменение");
        }
        let mut next = self.working.next_id;
        let mut commands = Vec::new();
        let (shape, material) = self.plate_shape(plate, &mut commands, &mut next)?;
        let (id, existing) = self.element_id(&mut next);
        commands.push(place(
            Element {
                id,
                name: plate.name.clone(),
                material,
                shape,
                properties: plate.properties.clone(),
            },
            existing,
        ));
        self.stage(commands)?;
        plate.id = Some(id);
        Ok(id)
    }

    pub fn modify_plate(&mut self, plate: &Plate) -> Result<()> {
        let id = plate.id.ok_or(ApiError::NotInserted)?;
        match self.working.elements.get(&id).map(|e| &e.shape) {
            Some(ElementShape::Plate { .. }) => {}
            Some(_) => return invalid(format!("Объект {id} — не пластина")),
            None => return Err(ApiError::NotFound(id)),
        }
        let mut next = self.working.next_id;
        let mut commands = Vec::new();
        let (shape, material) = self.plate_shape(plate, &mut commands, &mut next)?;
        commands.push(Command::ReplaceElement {
            element: Element {
                id,
                name: plate.name.clone(),
                material,
                shape,
                properties: plate.properties.clone(),
            },
        });
        self.stage(commands)
    }

    pub fn select_plate(&self, id: ObjectId) -> Result<Plate> {
        let p = &self.working;
        if p.swept_parts.contains_key(&id) {
            return invalid("Чтение гнутого листа как плоского не поддерживается");
        }
        let element = p.elements.get(&id).ok_or(ApiError::NotFound(id))?;
        let ElementShape::Plate {
            contour,
            thickness,
            frame,
        } = &element.shape
        else {
            return invalid(format!("Объект {id} — не пластина"));
        };
        Ok(Plate {
            id: Some(id),
            name: element.name.clone(),
            material: material_name(p, element.material),
            thickness: thickness.value(),
            outline: contour
                .iter()
                .map(|[x, y]| frame.world(Point3([*x, *y, 0.])))
                .collect(),
            properties: element.properties.clone(),
        })
    }

    /// Stage removal of a part; `cascade` also removes what depends on it
    /// (otherwise a part with dependants is refused).
    pub fn delete(&mut self, id: ObjectId, cascade: bool) -> Result<()> {
        if !self.working.contains_id(id) {
            return Err(ApiError::NotFound(id));
        }
        self.stage(vec![Command::Delete {
            id,
            policy: if cascade {
                DeletePolicy::Cascade
            } else {
                DeletePolicy::RejectDependencies
            },
        }])
    }

    /// Stage a property change; `None` removes the property.
    pub fn set_property(
        &mut self,
        id: ObjectId,
        key: &str,
        value: Option<PropertyValue>,
    ) -> Result<()> {
        if key.trim().is_empty() {
            return invalid("Не задано имя свойства");
        }
        self.stage(vec![Command::ChangeProperty {
            id,
            key: key.into(),
            value,
        }])
    }

    /// IDs of the parts matching the query, ascending.
    pub fn query(&self, query: &Query) -> Result<Vec<ObjectId>> {
        let p = &self.working;
        let mut out = Vec::new();
        for (id, element) in &p.elements {
            let kind = match &element.shape {
                ElementShape::Member {
                    member_kind: MemberKind::Beam,
                    ..
                } => ObjectKind::Beam,
                ElementShape::Member { .. } => ObjectKind::Column,
                ElementShape::Plate { .. } => ObjectKind::Plate,
            };
            if !query.kinds.is_empty() && !query.kinds.contains(&kind) {
                continue;
            }
            if query.name.as_ref().is_some_and(|n| *n != element.name) {
                continue;
            }
            if let Some(name) = &query.profile {
                let ElementShape::Member { profile, .. } = &element.shape else {
                    continue;
                };
                if p.profiles.get(profile).is_none_or(|pr| pr.name != *name) {
                    continue;
                }
            }
            if let Some(name) = &query.material {
                let m = material_name(p, element.material);
                if m != *name && m.split(" · ").next() != Some(name) {
                    continue;
                }
            }
            if let Some((key, value)) = &query.property
                && element.properties.get(key) != Some(value)
            {
                continue;
            }
            if query.inside.is_some() || query.touching.is_some() {
                let [lo, hi] = p.element_bounds(*id)?;
                let t = POINT_TOLERANCE_MM;
                if let Some(b) = query.inside
                    && !(0..3).all(|i| lo[i] >= b.min.0[i] - t && hi[i] <= b.max.0[i] + t)
                {
                    continue;
                }
                if let Some(b) = query.touching
                    && !(0..3).all(|i| lo[i] <= b.max.0[i] + t && hi[i] >= b.min.0[i] - t)
                {
                    continue;
                }
            }
            out.push(*id);
        }
        Ok(out)
    }

    fn parts(&self, ids: &[ObjectId]) -> Result<Vec<ObjectId>> {
        let unique: BTreeSet<_> = ids.iter().copied().collect();
        if unique.is_empty() {
            return invalid("Не выбраны детали");
        }
        for id in &unique {
            if !self.working.elements.contains_key(id) {
                return Err(ApiError::NotFound(*id));
            }
            if self
                .working
                .connections
                .values()
                .any(|c| c.children.values().any(|child| child == id))
            {
                return invalid(format!(
                    "Деталь {id} создана узлом: перемещайте или копируйте основные детали"
                ));
            }
        }
        Ok(unique.into_iter().collect())
    }

    /// Move parts so that `from` lands on `to` (a translation when both
    /// frames have the same axes).
    pub fn move_between(&mut self, ids: &[ObjectId], from: &Frame, to: &Frame) -> Result<()> {
        let ids = self.parts(ids)?;
        let (rotation, pivot) = mapping(from, to)?;
        self.stage(vec![Command::Transform {
            ids,
            rotation,
            pivot,
        }])
    }

    pub fn move_by(&mut self, ids: &[ObjectId], vector: [f64; 3]) -> Result<()> {
        self.move_between(ids, &Frame::default(), &shifted(vector)?)
    }

    /// Copies of the parts mapped from `from` to `to`; returns the new IDs
    /// in the order of the sorted source IDs.
    pub fn copy_between(
        &mut self,
        ids: &[ObjectId],
        from: &Frame,
        to: &Frame,
    ) -> Result<Vec<ObjectId>> {
        let ids = self.parts(ids)?;
        let (rotation, pivot) = mapping(from, to)?;
        let mut next = self.working.next_id;
        let mut commands = Vec::new();
        let mut copies = Vec::new();
        for id in &ids {
            let mut element = self.working.elements[id].clone();
            let existing;
            (element.id, existing) = self.element_id(&mut next);
            copies.push(element.id);
            if let Some(swept) = self.working.swept_parts.get(id) {
                if existing {
                    return invalid("Повторное использование ID гнутой детали не поддерживается");
                }
                commands.push(Command::CreateSwept {
                    element,
                    swept: swept.clone(),
                });
            } else {
                commands.push(place(element, existing));
            }
        }
        commands.push(Command::Transform {
            ids: copies.clone(),
            rotation,
            pivot,
        });
        self.stage(commands)?;
        Ok(copies)
    }

    pub fn copy_by(&mut self, ids: &[ObjectId], vector: [f64; 3]) -> Result<Vec<ObjectId>> {
        self.copy_between(ids, &Frame::default(), &shifted(vector)?)
    }
}

fn place(element: Element, existing: bool) -> Command {
    if existing {
        Command::ReplaceElement { element }
    } else {
        Command::Create { element }
    }
}

fn take_id(next: &mut u64) -> ObjectId {
    let id = ObjectId(*next);
    *next += 1;
    id
}

fn material_name(p: &Project, id: ObjectId) -> String {
    p.materials
        .get(&id)
        .map(|m| m.name.clone())
        .unwrap_or_default()
}

fn shifted(vector: [f64; 3]) -> Result<Frame> {
    if !vector.iter().all(|v| v.is_finite() && v.abs() <= 1e9) {
        return invalid("Недопустимый вектор");
    }
    Ok(Frame {
        origin: Point3(vector),
        ..Frame::default()
    })
}

/// `Transform` arguments mapping frame `from` onto frame `to`:
/// p ↦ R·(p − from.origin) + to.origin with R = to·fromᵀ.
fn mapping(from: &Frame, to: &Frame) -> Result<(Frame, Point3)> {
    from.validate()?;
    to.validate()?;
    let mut axes = [[0.; 3]; 3];
    for (j, column) in axes.iter_mut().enumerate() {
        for (k, to_axis) in to.axes.iter().enumerate() {
            let weight = from.axes[k][j];
            for i in 0..3 {
                column[i] += weight * to_axis[i];
            }
        }
    }
    // Re-orthonormalise against rounding so the frame check passes.
    let norm = |v: [f64; 3]| {
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        v.map(|c| c / l)
    };
    let x = norm(axes[0]);
    let z = norm(cross(x, axes[1]));
    let y = cross(z, x);
    let rotation = Frame {
        origin: Point3(std::array::from_fn(|i| to.origin.0[i] - from.origin.0[i])),
        axes: [x, y, z],
    };
    rotation.validate()?;
    Ok((rotation, from.origin))
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Plane frame of a planar outline (origin at the first point, x towards
/// the second) and the outline in it.
fn plane_outline(outline: &[Point3]) -> Result<(Frame, Vec<[f64; 2]>)> {
    if outline.len() < 3 {
        return invalid("Контур пластины: не меньше трёх точек");
    }
    let o = outline[0].0;
    let sub = |p: Point3| std::array::from_fn::<f64, 3, _>(|i| p.0[i] - o[i]);
    let len = |v: [f64; 3]| dot(v, v).sqrt();
    let first = sub(outline[1]);
    if len(first) <= POINT_TOLERANCE_MM {
        return invalid("Контур пластины: первые две точки совпадают");
    }
    let x = first.map(|c| c / len(first));
    // Normal from the signed area (Newell), so the outline's direction
    // decides the side and a concave first corner does not flip it.
    let mut n = [0.; 3];
    for (i, a) in outline.iter().enumerate() {
        let b = outline[(i + 1) % outline.len()];
        n = std::array::from_fn(|k| n[k] + cross(sub(*a), sub(b))[k] / 2.);
    }
    let n = std::array::from_fn::<f64, 3, _>(|k| n[k] - x[k] * dot(n, x));
    if len(n) <= POINT_TOLERANCE_MM {
        return invalid("Контур пластины вырожден: точки на одной прямой");
    }
    let z = n.map(|c| c / len(n));
    let y = cross(z, x);
    let mut contour = Vec::with_capacity(outline.len());
    for p in outline {
        let v = sub(*p);
        if dot(v, z).abs() > POINT_TOLERANCE_MM {
            return invalid("Точки контура пластины не лежат в одной плоскости");
        }
        contour.push([dot(v, x), dot(v, y)]);
    }
    let frame = Frame {
        origin: outline[0],
        axes: [x, y, z],
    };
    frame.validate()?;
    Ok((frame, contour))
}
