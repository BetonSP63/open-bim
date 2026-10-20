//! Property pane like Tekla: the common properties of the selected parts,
//! «— разные —» where they differ, and «Изменить» that applies the edited
//! fields to every selected part in one transaction.
use crate::interaction::member_edit::{LengthMode, length_text, resize};
use eframe::egui;
use open_bim_commands::Command;
use open_bim_model::{
    AtDepth, ElementShape, ObjectId, OnPlane, PartPosition, Project, PropertyValue, Rotation,
};
use std::collections::BTreeSet;

pub const VARIES: &str = "— разные —";

/// A property shared by the selection: one value, or differing values.
#[derive(Clone, Debug, PartialEq)]
pub enum Shared<T> {
    Same(T),
    Varies,
}

impl<T: PartialEq + Clone> Shared<T> {
    fn of(values: impl IntoIterator<Item = T>) -> Option<Self> {
        let mut it = values.into_iter();
        let first = it.next()?;
        Some(if it.all(|v| v == first) {
            Shared::Same(first)
        } else {
            Shared::Varies
        })
    }
    pub fn value(&self) -> Option<&T> {
        match self {
            Shared::Same(v) => Some(v),
            Shared::Varies => None,
        }
    }
}

/// What the pane shows for the current selection.
#[derive(Clone, Debug, PartialEq)]
pub struct Common {
    pub parts: Vec<ObjectId>,
    pub kind: Shared<String>,
    pub name: Shared<String>,
    /// Members only; `None` when no member is selected.
    pub profile: Option<Shared<ObjectId>>,
    pub material: Shared<ObjectId>,
    pub class: Shared<String>,
    /// Read-only: member length (mm) or plate thickness.
    pub size: Shared<String>,
    /// Members only: length in mm as shown («6000»).
    pub length: Option<Shared<String>>,
    /// Members only: Tekla «Положение».
    pub position: Option<PositionShared>,
}

/// Position fields of the selected members, each shared or «— разные —».
#[derive(Clone, Debug, PartialEq)]
pub struct PositionShared {
    pub plane: Shared<OnPlane>,
    pub plane_offset: Shared<String>,
    pub rotation: Shared<Rotation>,
    pub rotation_angle: Shared<String>,
    pub depth: Shared<AtDepth>,
    pub depth_offset: Shared<String>,
    pub start_dx: Shared<String>,
    pub end_dx: Shared<String>,
}

/// A position value as shown: «0», «12.5».
pub fn number_text(v: f64) -> String {
    let rounded = (v * 1000.).round() / 1000.;
    if rounded == 0. {
        "0".into()
    } else {
        format!("{rounded}")
    }
}

fn position_shared(positions: &[PartPosition]) -> Option<PositionShared> {
    let text =
        |f: fn(&PartPosition) -> f64| Shared::of(positions.iter().map(|p| number_text(f(p))));
    Some(PositionShared {
        plane: Shared::of(positions.iter().map(|p| p.plane))?,
        plane_offset: text(|p| p.plane_offset)?,
        rotation: Shared::of(positions.iter().map(|p| p.rotation))?,
        rotation_angle: text(|p| p.rotation_angle)?,
        depth: Shared::of(positions.iter().map(|p| p.depth))?,
        depth_offset: text(|p| p.depth_offset)?,
        start_dx: text(|p| p.start_dx)?,
        end_dx: text(|p| p.end_dx)?,
    })
}

fn class_of(e: &open_bim_model::Element) -> String {
    match e.properties.get("class") {
        Some(PropertyValue::Text(t)) => t.clone(),
        Some(PropertyValue::Number(n)) => format!("{n:.0}"),
        _ => String::new(),
    }
}

pub fn common(project: &Project, selected: &BTreeSet<ObjectId>) -> Option<Common> {
    let parts: Vec<&open_bim_model::Element> = selected
        .iter()
        .filter_map(|id| project.elements.get(id))
        .collect();
    let members: Vec<ObjectId> = parts
        .iter()
        .filter_map(|e| match &e.shape {
            ElementShape::Member { profile, .. } => Some(*profile),
            _ => None,
        })
        .collect();
    Some(Common {
        parts: parts.iter().map(|e| e.id).collect(),
        kind: Shared::of(parts.iter().map(|e| {
            match &e.shape {
                ElementShape::Member { member_kind, .. } => match member_kind {
                    open_bim_model::MemberKind::Beam => "Балка",
                    open_bim_model::MemberKind::Column => "Колонна",
                },
                ElementShape::Plate { .. } => match e.properties.get("fabrication_role") {
                    Some(PropertyValue::Text(r)) if r == "gusset" => "Косынка",
                    Some(PropertyValue::Text(r)) if r == "stiffener" => "Ребро жёсткости",
                    _ => "Пластина",
                },
            }
            .to_string()
        }))?,
        name: Shared::of(parts.iter().map(|e| e.name.clone()))?,
        profile: Shared::of(members),
        material: Shared::of(parts.iter().map(|e| e.material))?,
        class: Shared::of(parts.iter().map(|e| class_of(e)))?,
        length: Shared::of(parts.iter().filter_map(|e| match &e.shape {
            ElementShape::Member { length, .. } => Some(length_text(length.value())),
            ElementShape::Plate { .. } => None,
        })),
        position: position_shared(
            &parts
                .iter()
                .filter_map(|e| match &e.shape {
                    ElementShape::Member { position, .. } => Some(*position),
                    ElementShape::Plate { .. } => None,
                })
                .collect::<Vec<_>>(),
        ),
        size: Shared::of(parts.iter().map(|e| match &e.shape {
            ElementShape::Member { length, .. } => format!("L = {:.0} мм", length.value()),
            ElementShape::Plate { thickness, .. } => format!("t = {:.0} мм", thickness.value()),
        }))?,
    })
}

/// Edited fields: `None` keeps each part's own value (Tekla: only checked /
/// changed fields are applied by «Изменить»).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Changes {
    pub name: Option<String>,
    pub profile: Option<ObjectId>,
    pub material: Option<ObjectId>,
    pub class: Option<String>,
    /// New member length (mm) and which end moves.
    pub length: Option<(f64, LengthMode)>,
    pub position: PositionChanges,
}

/// Edited position fields; `None` keeps each member's own value.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PositionChanges {
    pub plane: Option<OnPlane>,
    pub plane_offset: Option<f64>,
    pub rotation: Option<Rotation>,
    pub rotation_angle: Option<f64>,
    pub depth: Option<AtDepth>,
    pub depth_offset: Option<f64>,
    pub start_dx: Option<f64>,
    pub end_dx: Option<f64>,
}

impl PositionChanges {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    pub fn apply(&self, p: &PartPosition) -> PartPosition {
        PartPosition {
            plane: self.plane.unwrap_or(p.plane),
            plane_offset: self.plane_offset.unwrap_or(p.plane_offset),
            rotation: self.rotation.unwrap_or(p.rotation),
            rotation_angle: self.rotation_angle.unwrap_or(p.rotation_angle),
            depth: self.depth.unwrap_or(p.depth),
            depth_offset: self.depth_offset.unwrap_or(p.depth_offset),
            start_dx: self.start_dx.unwrap_or(p.start_dx),
            end_dx: self.end_dx.unwrap_or(p.end_dx),
        }
    }
}

/// One transaction changing every selected part.
pub fn modify(
    project: &Project,
    parts: &[ObjectId],
    changes: &Changes,
) -> Result<Vec<Command>, String> {
    if let Some(p) = changes.profile
        && !project.profiles.contains_key(&p)
    {
        return Err("Нет такого профиля".into());
    }
    if let Some(m) = changes.material
        && !project.materials.contains_key(&m)
    {
        return Err("Нет такого материала".into());
    }
    if let Some(name) = &changes.name
        && (name.trim().is_empty() || name.chars().count() > 61)
    {
        return Err("Имя: 1–61 символ".into());
    }
    let mut commands = Vec::new();
    for id in parts {
        let mut e = project.elements.get(id).ok_or("Объект удалён")?.clone();
        if let Some(name) = &changes.name {
            e.name = name.trim().to_string();
        }
        if let Some(m) = changes.material {
            e.material = m;
        }
        // Profile and position are placed again on the same reference line,
        // so a beam «Сзади» keeps its top flange on the line (Tekla).
        if let ElementShape::Member { position, .. } = &e.shape
            && (changes.profile.is_some() || !changes.position.is_empty())
        {
            let position = changes.position.apply(position);
            e.shape = project
                .replace_member(&e.shape, None, changes.profile, Some(position))
                .map_err(|err| format!("{}: {err}", e.name))?;
        }
        if let Some(class) = &changes.class {
            if class.trim().is_empty() {
                e.properties.remove("class");
            } else {
                e.properties
                    .insert("class".into(), PropertyValue::Text(class.trim().into()));
            }
        }
        if let Some((length, mode)) = changes.length
            && matches!(e.shape, ElementShape::Member { .. })
        {
            e.shape = resize(&e.shape, length, mode)?;
        }
        if &e != project.elements.get(id).unwrap() {
            commands.push(Command::ReplaceElement { element: e });
        }
    }
    if commands.is_empty() {
        return Err("Нет изменений".into());
    }
    Ok(commands)
}

/// Cache key of `common`: project revision plus a fingerprint of the
/// selection (count, sum and xor of IDs), cheap compared with rebuilding the
/// common properties of a large selection every frame.
pub type CacheKey = (u64, usize, u64, u64);

pub fn cache_key(project: &Project, selected: &BTreeSet<ObjectId>) -> CacheKey {
    let (sum, xor) = selected.iter().fold((0u64, 0u64), |(s, x), id| {
        (
            s.wrapping_add(id.0),
            x ^ id.0.wrapping_mul(0x9e37_79b9_7f4a_7c15),
        )
    });
    (project.revision, selected.len(), sum, xor)
}

/// Pane state: the edit buffer for the current selection.
#[derive(Clone, Debug, Default)]
pub struct PropertyPane {
    /// Last computed common properties (None = no parts selected).
    pub cache: Option<(CacheKey, Option<std::sync::Arc<Common>>)>,
    /// Selection the buffer was filled from.
    pub source: Vec<ObjectId>,
    pub name: String,
    pub profile: Option<ObjectId>,
    pub material: Option<ObjectId>,
    pub class: String,
    pub length: String,
    /// Which end the length field moves; kept between selections.
    pub length_mode: LengthMode,
    pub position: PositionBuffer,
}

/// Edit buffer of the position fields; `None` = «— разные —».
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PositionBuffer {
    pub plane: Option<OnPlane>,
    pub plane_offset: String,
    pub rotation: Option<Rotation>,
    pub rotation_angle: String,
    pub depth: Option<AtDepth>,
    pub depth_offset: String,
    pub start_dx: String,
    pub end_dx: String,
}

fn shown(s: &Shared<String>) -> String {
    s.value().cloned().unwrap_or_else(|| VARIES.into())
}

impl PositionBuffer {
    pub fn fill(&mut self, p: &PositionShared) {
        *self = Self {
            plane: p.plane.value().copied(),
            plane_offset: shown(&p.plane_offset),
            rotation: p.rotation.value().copied(),
            rotation_angle: shown(&p.rotation_angle),
            depth: p.depth.value().copied(),
            depth_offset: shown(&p.depth_offset),
            start_dx: shown(&p.start_dx),
            end_dx: shown(&p.end_dx),
        };
    }

    /// Fields edited relative to what the selection shows; an unparsable
    /// number is an error rather than silently ignored.
    pub fn changes(&self, shown: &PositionShared) -> Result<PositionChanges, String> {
        fn pick<T: Copy + PartialEq>(buffer: Option<T>, shown: &Shared<T>) -> Option<T> {
            buffer.filter(|b| shown.value() != Some(b))
        }
        let num =
            |buffer: &str, shown: &Shared<String>, label: &str| -> Result<Option<f64>, String> {
                let was = shown.value().map(String::as_str).unwrap_or(VARIES);
                if buffer == was || buffer == VARIES {
                    return Ok(None);
                }
                crate::tools::number(&buffer.replace(',', "."))
                    .map(Some)
                    .map_err(|_| format!("{label}: введите число"))
            };
        Ok(PositionChanges {
            plane: pick(self.plane, &shown.plane),
            plane_offset: num(
                &self.plane_offset,
                &shown.plane_offset,
                "Смещение в плоскости",
            )?,
            rotation: pick(self.rotation, &shown.rotation),
            rotation_angle: num(&self.rotation_angle, &shown.rotation_angle, "Угол поворота")?,
            depth: pick(self.depth, &shown.depth),
            depth_offset: num(
                &self.depth_offset,
                &shown.depth_offset,
                "Смещение по глубине",
            )?,
            start_dx: num(&self.start_dx, &shown.start_dx, "Dx начала")?,
            end_dx: num(&self.end_dx, &shown.end_dx, "Dx конца")?,
        })
    }
}

impl PropertyPane {
    /// Refill the buffer when the selection changes.
    pub fn sync(&mut self, c: &Common) {
        if self.source == c.parts {
            return;
        }
        self.source = c.parts.clone();
        self.name = c.name.value().cloned().unwrap_or_else(|| VARIES.into());
        self.profile = c.profile.as_ref().and_then(|p| p.value().copied());
        self.material = c.material.value().copied();
        self.class = c.class.value().cloned().unwrap_or_else(|| VARIES.into());
        self.length = c
            .length
            .as_ref()
            .map(|l| l.value().cloned().unwrap_or_else(|| VARIES.into()))
            .unwrap_or_default();
        match &c.position {
            Some(p) => self.position.fill(p),
            None => self.position = PositionBuffer::default(),
        }
    }

    /// Fields edited relative to what the selection shows.
    pub fn changes(&self, c: &Common) -> Result<Changes, String> {
        let edited = |buffer: &str, shown: &Shared<String>| {
            let shown = shown.value().map(String::as_str).unwrap_or(VARIES);
            (buffer != shown && buffer != VARIES).then(|| buffer.to_string())
        };
        Ok(Changes {
            position: match &c.position {
                Some(shown) => self.position.changes(shown)?,
                None => PositionChanges::default(),
            },
            name: edited(&self.name, &c.name),
            class: edited(&self.class, &c.class),
            profile: self
                .profile
                .filter(|p| c.profile.as_ref().and_then(|s| s.value()) != Some(p)),
            material: self.material.filter(|m| c.material.value() != Some(m)),
            length: c.length.as_ref().and_then(|shown| {
                let shown = shown.value().map(String::as_str).unwrap_or(VARIES);
                (self.length != shown && self.length != VARIES)
                    .then(|| crate::tools::number(&self.length).ok())
                    .flatten()
                    .map(|l| (l, self.length_mode))
            }),
        })
    }
}

impl PropertyPane {
    /// Fill the edit buffer from a beam or column property file; «Изменить»
    /// then applies it to the selection. Returns what could not be applied.
    pub fn apply_file(
        &mut self,
        project: &Project,
        values: &crate::tools::property_files::Values,
    ) -> Vec<String> {
        use crate::tools::property_files::{Values, find_material, find_profile};
        let (Values::Beam(v) | Values::Column(v)) = values else {
            return vec!["файл не для балки или колонны".into()];
        };
        let mut warnings = Vec::new();
        if let Some(name) = &v.name {
            self.name = name.clone();
        }
        if let Some(name) = &v.profile {
            match find_profile(project, name) {
                Some(id) => self.profile = Some(id),
                None => warnings.push(format!("нет профиля «{name}»")),
            }
        }
        if let Some(name) = &v.material {
            match find_material(project, name) {
                Some(id) => self.material = Some(id),
                None => warnings.push(format!("нет материала «{name}»")),
            }
        }
        if let Some(p) = &v.position {
            self.position
                .fill(&position_shared(std::slice::from_ref(p)).expect("one position"));
        }
        if v.bottom.is_some() || v.top.is_some() {
            warnings.push("низ и верх применяются только при создании колонны".into());
        }
        warnings
    }
}

/// Tekla «Положение» fields: three lists with an offset each, and Dx at
/// both ends. `None` in a list shows «— разные —».
pub fn position_grid(ui: &mut egui::Ui, salt: &str, b: &mut PositionBuffer) {
    fn list<T: Copy + PartialEq>(
        ui: &mut egui::Ui,
        id: String,
        label: &str,
        value: &mut Option<T>,
        all: &[T],
        name: fn(T) -> &'static str,
    ) {
        let l = ui.label(label);
        egui::ComboBox::new(id, "")
            .selected_text(value.map_or(VARIES, name))
            .show_ui(ui, |ui| {
                for v in all {
                    ui.selectable_value(value, Some(*v), name(*v));
                }
            })
            .response
            .labelled_by(l.id);
    }
    fn field(ui: &mut egui::Ui, id: String, label: &str, text: &mut String) {
        let l = ui.label(label);
        ui.add(
            egui::TextEdit::singleline(text)
                .id(egui::Id::new(id))
                .desired_width(60.),
        )
        .labelled_by(l.id);
    }
    egui::Grid::new(format!("{salt} position"))
        .num_columns(4)
        .show(ui, |ui| {
            list(
                ui,
                format!("{salt} plane"),
                "В плоскости",
                &mut b.plane,
                &OnPlane::ALL,
                OnPlane::label,
            );
            field(
                ui,
                format!("{salt} plane offset"),
                "мм",
                &mut b.plane_offset,
            );
            ui.end_row();
            list(
                ui,
                format!("{salt} rotation"),
                "Поворот",
                &mut b.rotation,
                &Rotation::ALL,
                Rotation::label,
            );
            field(
                ui,
                format!("{salt} rotation angle"),
                "°",
                &mut b.rotation_angle,
            );
            ui.end_row();
            list(
                ui,
                format!("{salt} depth"),
                "По глубине",
                &mut b.depth,
                &AtDepth::ALL,
                AtDepth::label,
            );
            field(
                ui,
                format!("{salt} depth offset"),
                "мм",
                &mut b.depth_offset,
            );
            ui.end_row();
            field(
                ui,
                format!("{salt} start dx"),
                "Dx начала, мм",
                &mut b.start_dx,
            );
            field(ui, format!("{salt} end dx"), "Dx конца, мм", &mut b.end_dx);
            ui.end_row();
        });
}

/// The same fields over one position (the create dialog): edits apply
/// at once and a bad number keeps the previous value.
pub fn position_editor(ui: &mut egui::Ui, salt: &str, position: &mut PartPosition) {
    let shown = position_shared(std::slice::from_ref(position)).expect("one position");
    let id = egui::Id::new((salt, "position buffer"));
    let mut b: PositionBuffer = ui.data_mut(|d| d.get_temp(id)).unwrap_or_else(|| {
        let mut b = PositionBuffer::default();
        b.fill(&shown);
        b
    });
    position_grid(ui, salt, &mut b);
    if let Ok(changes) = b.changes(&shown) {
        *position = changes.apply(position);
    }
    ui.data_mut(|d| d.insert_temp(id, b));
}
