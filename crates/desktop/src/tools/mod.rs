//! Model tools commit only validated documents and renderable geometry.
pub mod column_levels;
pub mod connection_ui;
pub mod coordination_ui;
pub mod detailing_ui;
pub mod property_files;
mod snap;
pub mod ui;
use open_bim_commands::{Command, Document, project_hash};
use open_bim_model::{
    DetailKind, Element, ElementShape, Frame, GridAxis, LengthMm, MemberKind, ObjectId, Point3,
    Project, PropertyValue,
};
use open_bim_renderer::{DVec3, Scene};
use serde::Serialize;
pub use snap::{SnapHit, SnapIndex, SnapMode};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolKind {
    Beam,
    Column,
    Plate,
    Gusset,
    Stiffener,
    Axis,
}
impl ToolKind {
    pub fn is_plate(self) -> bool {
        matches!(self, Self::Plate | Self::Gusset | Self::Stiffener)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Beam => "Балка",
            Self::Column => "Колонна",
            Self::Plate => "Контурная плита",
            Self::Gusset => "Косынка",
            Self::Stiffener => "Ребро жёсткости",
            Self::Axis => "Ось",
        }
    }
}
#[derive(Clone, Debug)]
pub struct Draft {
    pub kind: ToolKind,
    pub name: String,
    pub start: [String; 3],
    pub end: [String; 3],
    pub profile: ObjectId,
    pub material: ObjectId,
    pub thickness: String,
    /// Plate contour picking mode and position across the thickness.
    pub plate_mode: crate::interaction::polygon::PlateMode,
    pub plate_position: crate::interaction::polygon::PlatePosition,
    /// Beam/column position on the reference line («Положение»).
    pub position: open_bim_model::PartPosition,
    pub contour: String,
    /// Column bottom and top, mm («Низ»/«Верх»): global Z, or an
    /// offset from the chosen level.
    pub bottom: String,
    pub top: String,
    pub bottom_level: Option<ObjectId>,
    pub top_level: Option<ObjectId>,
}
impl Default for Draft {
    fn default() -> Self {
        Self {
            kind: ToolKind::Beam,
            name: "Балка".into(),
            start: ["0".into(), "0".into(), "0".into()],
            end: ["6000".into(), "0".into(), "0".into()],
            profile: ObjectId(2),
            material: ObjectId(1),
            thickness: "20".into(),
            contour: "0,0; 400,0; 400,300; 0,300".into(),
            plate_mode: Default::default(),
            plate_position: Default::default(),
            position: Default::default(),
            bottom: "0".into(),
            top: "3600".into(),
            bottom_level: None,
            top_level: None,
        }
    }
}
pub fn number(text: &str) -> Result<f64, String> {
    let value = text
        .trim()
        .parse::<f64>()
        .map_err(|_| format!("Некорректное число: {text}"))?;
    if !value.is_finite() {
        return Err("Число должно быть конечным".into());
    }
    Ok(value)
}
pub fn point(text: &[String; 3]) -> Result<Point3, String> {
    Ok(Point3([
        number(&text[0])?,
        number(&text[1])?,
        number(&text[2])?,
    ]))
}
pub fn parse_contour(text: &str) -> Result<Vec<[f64; 2]>, String> {
    let contour: Result<Vec<_>, String> = text
        .split(';')
        .map(|pair| {
            let p: Vec<_> = pair.trim().split(',').collect();
            if p.len() != 2 {
                return Err("Контур: X,Y; X,Y; …".into());
            }
            Ok([number(p[0])?, number(p[1])?])
        })
        .collect();
    contour
}
impl Draft {
    /// Column bottom and top as entered: a level and an offset, or global Z.
    pub fn column_ends(&self) -> Result<(column_levels::Height, column_levels::Height), String> {
        let h = |level, text: &str| -> Result<column_levels::Height, String> {
            Ok(column_levels::Height {
                level,
                value: number(&text.replace(',', "."))?,
            })
        };
        Ok((
            h(self.bottom_level, &self.bottom)?,
            h(self.top_level, &self.top)?,
        ))
    }

    pub fn commands(&self, project: &Project) -> Result<Vec<Command>, String> {
        let id = ObjectId(project.next_id);
        let start = point(&self.start)?;
        if self.kind == ToolKind::Axis {
            let end = point(&self.end)?;
            return Ok(vec![Command::CreateAxis {
                axis: GridAxis {
                    id,
                    name: self.name.clone(),
                    start,
                    end,
                },
            }]);
        }
        let shape = if self.kind.is_plate() {
            let mut contour = parse_contour(&self.contour)?;
            open_bim_model::polygon_area(&contour).map_err(|e| e.to_string())?;
            let signed: f64 = (0..contour.len())
                .map(|i| {
                    let a = contour[i];
                    let b = contour[(i + 1) % contour.len()];
                    a[0] * b[1] - b[0] * a[1]
                })
                .sum();
            if signed < 0.0 {
                contour.reverse();
            }
            ElementShape::Plate {
                contour,
                thickness: LengthMm::new(number(&self.thickness)?).map_err(|e| e.to_string())?,
                frame: Frame {
                    origin: start,
                    ..Default::default()
                },
            }
        } else {
            let member_kind = if self.kind == ToolKind::Beam {
                MemberKind::Beam
            } else {
                MemberKind::Column
            };
            project
                .new_member(
                    member_kind,
                    self.profile,
                    start,
                    point(&self.end)?,
                    self.position,
                )
                .map_err(|e| e.to_string())?
        };
        Ok(vec![Command::Create {
            element: Element {
                id,
                name: self.name.clone(),
                material: self.material,
                shape,
                properties: if self.kind.is_plate() {
                    let role = match self.kind {
                        ToolKind::Gusset => "gusset",
                        ToolKind::Stiffener => "stiffener",
                        _ => "plate",
                    };
                    std::collections::BTreeMap::from([(
                        "fabrication_role".into(),
                        PropertyValue::Text(role.into()),
                    )])
                } else {
                    Default::default()
                },
            },
        }])
    }
}
#[derive(Clone, Debug)]
pub enum Edit {
    PlateShape {
        contour: Vec<[f64; 2]>,
        thickness: f64,
    },
    Move(Point3),
    Copy(Point3),
    Rotate {
        axis: Point3,
        degrees: f64,
        pivot: Point3,
    },
    Mirror {
        normal: Point3,
        offset: f64,
    },
    /// Rigid motion `world = frame.world(p)` (copy special: rotate with dZ,
    /// to another plane). The axes must be orthonormal and right-handed.
    Transform(Frame),
    Property {
        key: String,
        value: PropertyValue,
    },
}
fn transform_point(p: Point3, rotation: Frame, pivot: Point3) -> Point3 {
    let r = rotation.world(Point3(std::array::from_fn(|i| p.0[i] - pivot.0[i])));
    Point3(std::array::from_fn(|i| r.0[i] + pivot.0[i]))
}
pub(crate) fn rotation(axis: Point3, degrees: f64) -> Result<Frame, String> {
    let axis = DVec3::from_array(axis.0)
        .try_normalize()
        .ok_or("Нулевая ось поворота")?;
    if !degrees.is_finite() || !axis.is_finite() {
        return Err("Некорректный поворот".into());
    }
    let (s, c) = degrees.to_radians().sin_cos();
    let turn = |v: DVec3| v * c + axis.cross(v) * s + axis * axis.dot(v) * (1.0 - c);
    Ok(Frame {
        axes: [
            turn(DVec3::X).to_array(),
            turn(DVec3::Y).to_array(),
            turn(DVec3::Z).to_array(),
        ],
        ..Default::default()
    })
}
/// A rigid motion: finite, orthonormal, right-handed axes.
fn rigid(frame: Frame) -> Result<Frame, String> {
    let [x, y, z] = frame.axes.map(DVec3::from_array);
    let ok = DVec3::from_array(frame.origin.0).is_finite()
        && [x, y, z].iter().all(|a| (a.length() - 1.0).abs() < 1e-6)
        && x.dot(y).abs() < 1e-6
        && (x.cross(y) - z).length() < 1e-6;
    if ok {
        Ok(frame)
    } else {
        Err("Некорректное преобразование".into())
    }
}
pub fn edit_commands(
    project: &Project,
    selected: &BTreeSet<ObjectId>,
    edit: &Edit,
) -> Result<Vec<Command>, String> {
    project.validate().map_err(|e| e.to_string())?;
    if selected.is_empty() {
        return Err("Выберите объекты".into());
    }
    let mut next = project.next_id;
    let mut commands = Vec::new();
    let mut copies = std::collections::BTreeMap::new();
    let generated: BTreeSet<_> = project
        .connections
        .values()
        .flat_map(|c| c.children.values().copied())
        .collect();
    for id in selected {
        if let Edit::PlateShape { contour, thickness } = edit {
            let mut element = project.elements.get(id).ok_or("Выберите пластину")?.clone();
            let frame = *element.shape.frame();
            if !matches!(element.shape, ElementShape::Plate { .. }) {
                return Err("Контур и толщина применимы к пластинам".into());
            }
            element.shape = ElementShape::Plate {
                contour: contour.clone(),
                thickness: LengthMm::new(*thickness).map_err(|e| e.to_string())?,
                frame,
            };
            commands.push(Command::ReplaceElement { element });
            continue;
        }
        if let Edit::Property { key, value } = edit {
            commands.push(Command::ChangeProperty {
                id: *id,
                key: key.clone(),
                value: Some(value.clone()),
            });
            continue;
        }
        let (r, pivot) = match edit {
            Edit::Move(delta) | Edit::Copy(delta) => (
                Frame {
                    origin: *delta,
                    ..Default::default()
                },
                Point3([0.0; 3]),
            ),
            Edit::Rotate {
                axis,
                degrees,
                pivot,
            } => (rotation(*axis, *degrees)?, *pivot),
            Edit::Transform(frame) => (rigid(*frame)?, Point3([0.0; 3])),
            _ => (Frame::default(), Point3([0.0; 3])),
        };
        let mirror = if let Edit::Mirror { normal, offset } = edit {
            let n = DVec3::from_array(normal.0)
                .try_normalize()
                .ok_or("Нулевая нормаль зеркала")?;
            if !n.is_finite() || !offset.is_finite() {
                return Err("Некорректное зеркало".into());
            }
            Some((n, *offset))
        } else {
            None
        };
        let reflection = |p: Point3| {
            let v = DVec3::from_array(p.0);
            let (n, offset) = mirror.expect("mirror branch");
            Point3((v - 2.0 * (n.dot(v) - offset) * n).to_array())
        };
        let copy = matches!(edit, Edit::Copy(_));
        let target_id = if copy {
            let value = ObjectId(next);
            next = next.checked_add(1).ok_or("Исчерпаны ID")?;
            value
        } else {
            *id
        };
        if let Some(source) = project.elements.get(id) {
            if copy {
                copies.insert(*id, target_id);
            }
            let mut element = source.clone();
            element.id = target_id;
            let extrusion = match &element.shape {
                ElementShape::Member { length, .. } => length.value(),
                ElementShape::Plate { thickness, .. } => thickness.value(),
            };
            let frame = element.shape.frame_mut();
            if let Some((n, _)) = mirror {
                // Reverse extrusion, rather than flip an asymmetric profile or store a left-handed frame.
                frame.origin = reflection(frame.world(Point3([0.0, 0.0, extrusion])));
                for (index, axis) in frame.axes.iter_mut().enumerate() {
                    let v = DVec3::from_array(*axis);
                    let reflected = v - 2.0 * n.dot(v) * n;
                    *axis = (if index == 2 { -reflected } else { reflected }).to_array();
                }
            } else {
                frame.origin = transform_point(frame.origin, r, pivot);
                for axis in &mut frame.axes {
                    *axis = std::array::from_fn(|i| (0..3).map(|j| r.axes[j][i] * axis[j]).sum());
                }
            }
            commands.push(if copy {
                Command::Create { element }
            } else {
                Command::ReplaceElement { element }
            });
        } else if let Some(source) = project.axes.get(id) {
            let mut axis = source.clone();
            axis.id = target_id;
            axis.start = if mirror.is_some() {
                reflection(axis.start)
            } else {
                transform_point(axis.start, r, pivot)
            };
            axis.end = if mirror.is_some() {
                reflection(axis.end)
            } else {
                transform_point(axis.end, r, pivot)
            };
            commands.push(if copy {
                Command::CreateAxis { axis }
            } else {
                Command::ReplaceAxis { axis }
            });
        } else {
            return Err(format!("Объект {id} недоступен для преобразования"));
        }
    }
    if let Edit::Copy(delta) = edit {
        let mut feature_copies = std::collections::BTreeMap::new();
        for source in project.details.values() {
            let Some(parent) = source.kind.part() else {
                continue;
            };
            let Some(target) = copies.get(&parent) else {
                continue;
            };
            let mut detail = source.clone();
            detail.id = ObjectId(next);
            next = next.checked_add(1).ok_or("Исчерпаны ID")?;
            feature_copies.insert(source.id, detail.id);
            match &mut detail.kind {
                DetailKind::PlaneCut { part, .. }
                | DetailKind::BoxCut { part, .. }
                | DetailKind::Hole { part, .. } => *part = *target,
                _ => unreachable!(),
            }
            commands.push(Command::CreateDetail { detail });
        }
        for source in project.details.values() {
            let mut detail = source.clone();
            let parts = match &detail.kind {
                DetailKind::BoltGroup { parts, .. } | DetailKind::Weld { parts, .. } => parts,
                _ => continue,
            };
            if !parts.iter().all(|p| copies.contains_key(p)) {
                continue;
            }
            detail.id = ObjectId(next);
            next = next.checked_add(1).ok_or("Исчерпаны ID")?;
            match &mut detail.kind {
                DetailKind::BoltGroup {
                    parts,
                    holes,
                    frame,
                    ..
                } => {
                    *parts = parts.iter().map(|p| copies[p]).collect();
                    *holes = holes.iter().map(|h| feature_copies[h]).collect();
                    frame.origin = Point3(std::array::from_fn(|i| frame.origin.0[i] + delta.0[i]));
                }
                DetailKind::Weld { parts, path, .. } => {
                    *parts = parts.iter().map(|p| copies[p]).collect();
                    for point in path {
                        point.0 = std::array::from_fn(|i| point.0[i] + delta.0[i]);
                    }
                }
                _ => unreachable!(),
            }
            commands.push(Command::CreateDetail { detail });
        }
    }
    if let Edit::Mirror { normal, offset } = edit {
        let n = DVec3::from_array(normal.0)
            .try_normalize()
            .ok_or("Нулевая нормаль зеркала")?;
        let reflect_vector = |p: [f64; 3]| {
            let v = DVec3::from_array(p);
            (v - 2.0 * n.dot(v) * n).to_array()
        };
        let reflect_point = |p: Point3| {
            let v = DVec3::from_array(p.0);
            Point3((v - 2.0 * (n.dot(v) - offset) * n).to_array())
        };
        for source in project.details.values() {
            if generated.contains(&source.id) {
                continue;
            }
            let mut detail = source.clone();
            if let Some(parent) = detail.kind.part() {
                if !selected.contains(&parent) {
                    continue;
                }
                let length = match &project.elements[&parent].shape {
                    ElementShape::Member { length, .. } => length.value(),
                    ElementShape::Plate { thickness, .. } => thickness.value(),
                };
                match &mut detail.kind {
                    DetailKind::PlaneCut { normal, offset, .. } => {
                        *offset -= normal[2] * length;
                        normal[2] = -normal[2];
                    }
                    DetailKind::BoxCut { origin, size, .. } => {
                        origin.0[2] = length - origin.0[2] - size[2]
                    }
                    DetailKind::Hole { frame, .. } => {
                        frame.origin.0[2] = length - frame.origin.0[2];
                        for (i, axis) in frame.axes.iter_mut().enumerate() {
                            axis[2] = -axis[2];
                            if i == 1 {
                                *axis = axis.map(|v| -v);
                            }
                        }
                    }
                    _ => unreachable!(),
                }
            } else {
                let parts = match &detail.kind {
                    DetailKind::BoltGroup { parts, .. } | DetailKind::Weld { parts, .. } => parts,
                    _ => unreachable!(),
                };
                if parts.is_disjoint(selected) {
                    continue;
                }
                if !parts.is_subset(selected) {
                    return Err("Для зеркала выберите все связанные детали".into());
                }
                match &mut detail.kind {
                    DetailKind::BoltGroup { frame, .. } => {
                        frame.origin = reflect_point(frame.origin);
                        for (i, axis) in frame.axes.iter_mut().enumerate() {
                            *axis = reflect_vector(*axis);
                            if i == 2 {
                                *axis = axis.map(|v| -v);
                            }
                        }
                    }
                    DetailKind::Weld { path, .. } => {
                        for point in path {
                            *point = reflect_point(*point);
                        }
                    }
                    _ => unreachable!(),
                }
            }
            commands.push(Command::ReplaceDetail { detail });
        }
    }
    if let Edit::Move(_) | Edit::Rotate { .. } | Edit::Transform(_) = edit {
        let (r, pivot) = match edit {
            Edit::Move(delta) => (
                Frame {
                    origin: *delta,
                    ..Frame::default()
                },
                Point3([0.0; 3]),
            ),
            Edit::Rotate {
                axis,
                degrees,
                pivot,
            } => (rotation(*axis, *degrees)?, *pivot),
            Edit::Transform(frame) => (rigid(*frame)?, Point3([0.0; 3])),
            _ => unreachable!(),
        };
        for source in project.details.values() {
            let mut detail = source.clone();
            let parts = match &detail.kind {
                DetailKind::BoltGroup { parts, .. } | DetailKind::Weld { parts, .. } => parts,
                _ => continue,
            };
            if generated.contains(&source.id) {
                continue;
            }
            if parts.is_disjoint(selected) {
                continue;
            }
            if !parts.is_subset(selected) {
                return Err("Для преобразования соединения выберите все связанные детали".into());
            }
            match &mut detail.kind {
                DetailKind::BoltGroup { frame, .. } => {
                    frame.origin = transform_point(frame.origin, r, pivot);
                    for axis in &mut frame.axes {
                        *axis =
                            std::array::from_fn(|i| (0..3).map(|j| r.axes[j][i] * axis[j]).sum());
                    }
                }
                DetailKind::Weld { path, .. } => {
                    for point in path {
                        *point = transform_point(*point, r, pivot);
                    }
                }
                _ => unreachable!(),
            }
            commands.push(Command::ReplaceDetail { detail });
        }
    }
    Ok(commands)
}
#[derive(Clone, Debug, Serialize)]
pub struct FlowEvent {
    pub action: String,
    pub revision: u64,
    pub objects: usize,
    pub axes: usize,
    pub sha256: String,
}
#[derive(Serialize)]
struct Recording<'a> {
    events: &'a [FlowEvent],
    project: &'a Project,
}
pub struct Editor {
    pub document: Document,
    pub selected: BTreeSet<ObjectId>,
    pub draft: Draft,
    pub active: bool,
    pub picked: Vec<Point3>,
    pub snap: SnapIndex,
    pub events: Vec<FlowEvent>,
    pub record_path: Option<PathBuf>,
    pub record_warning: Option<String>,
    /// Each tool keeps its own properties, like separate beam and column
    /// dialogs: the drafts of the other tools used in this session.
    pub saved_drafts: Vec<Draft>,
    /// Tools started at least once in this session.
    pub used_tools: Vec<ToolKind>,
}
impl Editor {
    pub fn new(project: Project) -> Result<Self, String> {
        let snap = SnapIndex::new(&project);
        Ok(Self {
            document: Document::new(project).map_err(|e| e.to_string())?,
            selected: Default::default(),
            draft: Draft::default(),
            active: false,
            picked: Vec::new(),
            snap,
            events: Vec::new(),
            record_path: None,
            record_warning: None,
            saved_drafts: Vec::new(),
            used_tools: Vec::new(),
        })
    }
    /// Start a tool with the properties it was last used with; returns
    /// true on its first use in the session (defaults, then «standard»).
    pub fn begin(&mut self, kind: ToolKind) -> bool {
        self.active = true;
        self.picked.clear();
        let first = !self.used_tools.contains(&kind);
        if !first && self.draft.kind == kind {
            return false;
        }
        let old = self.draft.kind;
        if self.used_tools.contains(&old) {
            self.saved_drafts.retain(|d| d.kind != old);
            self.saved_drafts.push(self.draft.clone());
        }
        if let Some(i) = self.saved_drafts.iter().position(|d| d.kind == kind) {
            self.draft = self.saved_drafts.remove(i);
            return false;
        }
        self.used_tools.push(kind);
        // First use: start from the current draft with the tool's defaults.
        self.draft.kind = kind;
        self.draft.name = kind.label().into();
        if kind == ToolKind::Gusset {
            self.draft.contour = "0,0; 300,0; 0,300".into();
            self.draft.thickness = "12".into();
        } else if kind == ToolKind::Stiffener {
            self.draft.contour = "0,0; 150,0; 150,250; 0,250".into();
            self.draft.thickness = "10".into();
        } else if kind == ToolKind::Column {
            self.draft.end = ["0".into(), "0".into(), "3000".into()];
        }
        true
    }
    pub fn cancel(&mut self) {
        self.active = false;
        self.picked.clear();
    }
    pub fn accept_point(&mut self, p: Point3) {
        if !self.active {
            return;
        }
        let string = |v: f64| format!("{v:.6}");
        self.picked.push(p);
        if self.picked.len() == 1 {
            self.draft.start = p.0.map(string);
        } else if self.draft.kind.is_plate() {
            let first = self.picked[0];
            self.draft.contour = self
                .picked
                .iter()
                .map(|q| format!("{},{}", q.0[0] - first.0[0], q.0[1] - first.0[1]))
                .collect::<Vec<_>>()
                .join("; ");
        } else {
            self.draft.end = p.0.map(string);
        }
    }
    pub fn commit(&mut self, label: &str, commands: &[Command]) -> Result<Arc<Scene>, String> {
        let mut candidate = self.document.clone();
        candidate
            .transact(label, commands)
            .map_err(|e| e.to_string())?;
        let scene = Arc::new(Scene::from_project(candidate.project()).map_err(|e| e.to_string())?);
        self.document = candidate;
        self.snap = SnapIndex::new(self.document.project());
        self.prune_selection();
        self.record(label);
        Ok(scene)
    }
    /// Commit commands that do not change model geometry (drawings, reports):
    /// the 3D scene stays valid, so it is not rebuilt.
    pub fn commit_data(&mut self, label: &str, commands: &[Command]) -> Result<(), String> {
        let mut candidate = self.document.clone();
        candidate
            .transact(label, commands)
            .map_err(|e| e.to_string())?;
        self.document = candidate;
        self.record(label);
        Ok(())
    }
    pub fn create(&mut self) -> Result<Arc<Scene>, String> {
        if !self.active {
            return Err("Инструмент не активен".into());
        }
        let commands = self.draft.commands(self.document.project())?;
        let scene = self.commit("Создать", &commands)?;
        self.selected = BTreeSet::from([ObjectId(self.document.project().next_id - 1)]);
        self.cancel();
        Ok(scene)
    }
    pub fn edit(&mut self, edit: &Edit) -> Result<Arc<Scene>, String> {
        let commands = edit_commands(self.document.project(), &self.selected, edit)?;
        self.commit(
            match edit {
                Edit::Move(_) => "Переместить",
                Edit::Copy(_) => "Копировать",
                Edit::Rotate { .. } => "Повернуть",
                Edit::Mirror { .. } => "Зеркало",
                Edit::Transform(_) => "Переместить",
                Edit::Property { .. } => "Свойства",
                Edit::PlateShape { .. } => "Изменить пластину",
            },
            &commands,
        )
    }
    pub fn history(&mut self, redo: bool) -> Result<Arc<Scene>, String> {
        let mut candidate = self.document.clone();
        if redo {
            candidate.redo()
        } else {
            candidate.undo()
        }
        .map_err(|e| e.to_string())?;
        let scene = Arc::new(Scene::from_project(candidate.project()).map_err(|e| e.to_string())?);
        self.document = candidate;
        self.snap = SnapIndex::new(self.document.project());
        self.prune_selection();
        self.record(if redo {
            "Повторить"
        } else {
            "Отменить"
        });
        Ok(scene)
    }
    fn prune_selection(&mut self) {
        self.selected
            .retain(|id| self.document.project().contains_id(*id));
    }
    pub fn save(&mut self, path: &Path) -> Result<(), String> {
        open_bim_persistence::save_atomic(path, self.document.project())
            .map_err(|e| e.to_string())?;
        self.record("Сохранить");
        Ok(())
    }
    pub fn open(&mut self, path: &Path) -> Result<Arc<Scene>, String> {
        let project = open_bim_persistence::load(path).map_err(|e| e.to_string())?;
        let scene = Arc::new(Scene::from_project(&project).map_err(|e| e.to_string())?);
        self.document = Document::new(project).map_err(|e| e.to_string())?;
        self.selected.clear();
        self.cancel();
        self.snap = SnapIndex::new(self.document.project());
        self.record("Открыть");
        Ok(scene)
    }
    /// IFC4 export; returns a short report of skipped model data.
    pub fn export_ifc(&mut self, path: &Path) -> Result<String, String> {
        let (text, report) = open_bim_ifc::export(self.document.project())?;
        std::fs::write(path, text).map_err(|e| e.to_string())?;
        self.record(&format!("Экспорт IFC {}", path.display()));
        let skipped: Vec<String> = report
            .skipped
            .iter()
            .map(|(k, v)| format!("{k}: {}", v.len()))
            .collect();
        Ok(format!(
            "IFC4: балок {}, колонн {}, пластин {}, проёмов {}; вне подмножества — {}",
            report.beams,
            report.columns,
            report.plates,
            report.openings,
            if skipped.is_empty() {
                "нет".into()
            } else {
                skipped.join(", ")
            }
        ))
    }
    /// IFC4 import replaces the project; the report lists unsupported content.
    pub fn import_ifc(&mut self, path: &Path) -> Result<(Arc<Scene>, String), String> {
        let bytes = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
        if bytes > open_bim_ifc::step::MAX_FILE_BYTES as u64 {
            return Err("IFC: файл слишком большой".into());
        }
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let (project, report) = open_bim_ifc::import(&text)?;
        let scene = self.replace(project, &format!("Импорт IFC {}", path.display()))?;
        let unsupported: usize = report.unsupported_entities.values().sum();
        Ok((
            scene,
            format!(
                "Импорт IFC: {:?}; неподдерживаемых сущностей {unsupported}; замечаний {}{}",
                report.imported,
                report.issues.len(),
                report
                    .issues
                    .first()
                    .map(|i| format!(" — {i}"))
                    .unwrap_or_default()
            ),
        ))
    }
    /// Take a project whose scene was already built elsewhere (background open).
    pub fn adopt(&mut self, project: Project, label: &str) -> Result<(), String> {
        self.document = Document::new(project).map_err(|e| e.to_string())?;
        self.selected.clear();
        self.cancel();
        self.snap = SnapIndex::new(self.document.project());
        self.record(label);
        Ok(())
    }
    pub fn replace(&mut self, project: Project, label: &str) -> Result<Arc<Scene>, String> {
        let scene = Arc::new(Scene::from_project(&project).map_err(|e| e.to_string())?);
        self.document = Document::new(project).map_err(|e| e.to_string())?;
        self.selected.clear();
        self.cancel();
        self.snap = SnapIndex::new(self.document.project());
        self.record(label);
        Ok(scene)
    }
    pub fn record(&mut self, action: &str) {
        let p = self.document.project();
        let hash = match project_hash(p) {
            Ok(hash) => hash,
            Err(error) => {
                self.record_warning = Some(error.to_string());
                return;
            }
        };
        self.events.push(FlowEvent {
            action: action.into(),
            revision: p.revision,
            objects: p.elements.len(),
            axes: p.axes.len(),
            sha256: hash,
        });
        if let Some(path) = &self.record_path {
            let result = serde_json::to_vec_pretty(&Recording {
                events: &self.events,
                project: p,
            })
            .map_err(|e| e.to_string())
            .and_then(|bytes| std::fs::write(path, bytes).map_err(|e| e.to_string()));
            if let Err(error) = result {
                self.record_warning = Some(format!("Журнал сценария: {error}"));
            }
        }
    }
}
