//! Property pane: the common properties of the selected parts,
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

/// Share of the pane width taken by the caption column: captions on the
/// left, fields from one common edge to the right side of the pane.
pub const LABEL_SHARE: f32 = 0.42;

/// A pane section: bold caption with a triangle, its rows below.
pub fn section(title: &str) -> egui::CollapsingHeader {
    egui::CollapsingHeader::new(egui::RichText::new(title).strong()).id_salt(title)
}

/// Two-column grid of a section whose captions all start the field column
/// at the same edge; fields stretch to the pane's right side.
pub fn rows(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    body: impl FnOnce(&mut egui::Ui),
) {
    let caption = (ui.available_width() * LABEL_SHARE).clamp(90., 220.);
    egui::Grid::new(id)
        .num_columns(2)
        .min_col_width(caption)
        .spacing([6., 4.])
        .show(ui, body);
}

/// Width left for a field in the current grid cell, less the field's own
/// frame and margins so the pane never grows frame after frame.
pub fn field_width(ui: &egui::Ui) -> f32 {
    (ui.available_width() - 16.).max(60.)
}

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
    /// Members only: «Положение».
    pub position: Option<PositionShared>,
    /// Series, deformation and IFC fields by key.
    pub fields: std::collections::BTreeMap<&'static str, Shared<String>>,
    /// Columns only: bottom and top bound to a level.
    pub levels: Option<EndsShared>,
    /// Beams or columns of one kind: the member type, if the selection agrees.
    pub member_type: Option<TypeShared>,
}

/// Level and offset of a column end, shared across the selection.
#[derive(Clone, Debug, PartialEq)]
pub struct EndShared {
    pub level: Shared<Option<ObjectId>>,
    pub offset: Shared<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EndsShared {
    pub bottom: EndShared,
    pub top: EndShared,
}

/// What the header shows for the member type.
#[derive(Clone, Debug, PartialEq)]
pub struct TypeShared {
    pub kind: open_bim_model::MemberKind,
    pub type_id: Shared<Option<ObjectId>>,
    pub overrides: BTreeSet<open_bim_model::TypeField>,
    /// True when every part overrides the same set (the marks are meaningful).
    pub overrides_agree: bool,
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
    let ids: Vec<ObjectId> = parts.iter().map(|e| e.id).collect();
    Some(Common {
        fields: super::property_fields::common(project, &ids),
        parts: ids.clone(),
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
        levels: column_levels(project, &parts),
        member_type: type_shared(project, &parts),
    })
}

fn end_shared(
    project: &Project,
    parts: &[&open_bim_model::Element],
    end: open_bim_model::MemberEnd,
) -> Option<EndShared> {
    let level = Shared::of(
        parts
            .iter()
            .map(|e| project.level_ends.get(e.id, end).map(|b| b.level)),
    )?;
    let offset = Shared::of(parts.iter().map(|e| {
        match project.level_ends.get(e.id, end) {
            Some(b) => number_text(b.offset_mm),
            None => open_bim_model::level_ends::end_z(project, &e.shape, end)
                .map(number_text)
                .unwrap_or_else(|| "0".into()),
        }
    }))?;
    Some(EndShared { level, offset })
}

fn column_levels(project: &Project, parts: &[&open_bim_model::Element]) -> Option<EndsShared> {
    if parts.is_empty()
        || parts.iter().any(|e| {
            !matches!(
                e.shape,
                ElementShape::Member {
                    member_kind: open_bim_model::MemberKind::Column,
                    ..
                }
            )
        })
    {
        return None;
    }
    Some(EndsShared {
        bottom: end_shared(project, parts, open_bim_model::MemberEnd::Bottom)?,
        top: end_shared(project, parts, open_bim_model::MemberEnd::Top)?,
    })
}

fn type_shared(project: &Project, parts: &[&open_bim_model::Element]) -> Option<TypeShared> {
    let kinds: Vec<_> = parts
        .iter()
        .filter_map(|e| match &e.shape {
            ElementShape::Member { member_kind, .. } => Some(*member_kind),
            ElementShape::Plate { .. } => None,
        })
        .collect();
    if kinds.len() != parts.len() {
        return None;
    }
    let kind = *kinds.first()?;
    if kinds.iter().any(|k| *k != kind) {
        return None;
    }
    let type_id = Shared::of(
        parts
            .iter()
            .map(|e| project.member_types.use_of(e.id).map(|u| u.type_id)),
    )?;
    let sets: Vec<_> = parts
        .iter()
        .map(|e| {
            project
                .member_types
                .use_of(e.id)
                .map(|u| u.overrides.clone())
                .unwrap_or_default()
        })
        .collect();
    let overrides_agree = sets.windows(2).all(|w| w[0] == w[1]);
    let overrides = if overrides_agree {
        sets.first().cloned().unwrap_or_default()
    } else {
        BTreeSet::new()
    };
    Some(TypeShared {
        kind,
        type_id,
        overrides,
        overrides_agree,
    })
}

/// Edited fields: `None` keeps each part's own value (only checked /
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
    /// Catalogue profile / steel picked for this change, stored in the same
    /// transaction (`profile` / `material` hold their ids).
    pub new_profile: Option<open_bim_model::Profile>,
    pub new_material: Option<open_bim_model::Material>,
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
    // Catalogue records are added first, in the same transaction.
    let mut added = Vec::new();
    let mut changes = changes.clone();
    let mut staged = None;
    if changes.new_profile.is_some() || changes.new_material.is_some() {
        let mut copy = project.clone();
        let mut next = copy.next_id;
        if let Some(mut p) = changes.new_profile.take() {
            let picked = changes.profile == Some(p.id);
            p.id = ObjectId(next);
            next += 1;
            if picked {
                changes.profile = Some(p.id);
            }
            copy.profiles.insert(p.id, p.clone());
            added.push(Command::CreateProfile { profile: p });
        }
        if let Some(mut m) = changes.new_material.take() {
            let picked = changes.material == Some(m.id);
            m.id = ObjectId(next);
            next += 1;
            if picked {
                changes.material = Some(m.id);
            }
            copy.materials.insert(m.id, m.clone());
            added.push(Command::CreateMaterial { material: m });
        }
        copy.next_id = next;
        staged = Some(copy);
    }
    let project = staged.as_ref().unwrap_or(project);
    let changes = &changes;
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
        // so a beam «Сзади» keeps its top flange on the line.
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
        if let Some(usage) = project.member_types.use_of(*id).cloned()
            && let Some(ty) = project.member_types.types.get(&usage.type_id).cloned()
        {
            let overrides = open_bim_model::member_type::differing(project, &e, &ty);
            if overrides != usage.overrides {
                commands.push(Command::SetMemberUse {
                    part: *id,
                    type_id: Some(ty.id),
                    overrides,
                    apply: false,
                });
            }
        }
        if &e != project.elements.get(id).unwrap() {
            commands.push(Command::ReplaceElement { element: e });
        }
    }
    if commands.is_empty() {
        return Err("Нет изменений".into());
    }
    added.extend(commands);
    Ok(added)
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
    pub fields: super::property_fields::FieldBuffer,
    /// Catalogue picks not yet in the project.
    pub pending_profile: Option<open_bim_model::Profile>,
    pub pending_material: Option<open_bim_model::Material>,
    pub bottom_level: LevelChoice,
    pub bottom_offset: String,
    pub top_level: LevelChoice,
    pub top_offset: String,
}

/// Level picked for a column end. `Varies` is the untouched «— разные —».
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LevelChoice {
    #[default]
    Absolute,
    Level(ObjectId),
    Varies,
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
        self.pending_profile = None;
        self.pending_material = None;
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
        self.fields.fill(&c.fields);
        if let Some(levels) = &c.levels {
            self.bottom_level = LevelChoice::from_shared(&levels.bottom.level);
            self.bottom_offset = shown(&levels.bottom.offset);
            self.top_level = LevelChoice::from_shared(&levels.top.level);
            self.top_offset = shown(&levels.top.offset);
        }
    }

    /// Fields edited relative to what the selection shows.
    pub fn changes(&self, c: &Common) -> Result<Changes, String> {
        let edited = |buffer: &str, shown: &Shared<String>| {
            let shown = shown.value().map(String::as_str).unwrap_or(VARIES);
            (buffer != shown && buffer != VARIES).then(|| buffer.to_string())
        };
        Ok(Changes {
            new_profile: self
                .pending_profile
                .clone()
                .filter(|p| self.profile == Some(p.id)),
            new_material: self
                .pending_material
                .clone()
                .filter(|m| self.material == Some(m.id)),
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
        let apply_end = |choice: &mut LevelChoice,
                         offset: &mut String,
                         value: &Option<crate::tools::property_files::LevelValue>,
                         warnings: &mut Vec<String>| {
            let Some(value) = value else {
                return;
            };
            match &value.level {
                None => {
                    *choice = LevelChoice::Absolute;
                    *offset = number_text(value.value);
                }
                Some(name) => match project.levels.values().find(|l| &l.name == name) {
                    Some(level) => {
                        *choice = LevelChoice::Level(level.id);
                        *offset = number_text(value.value);
                    }
                    None => warnings.push(format!("нет уровня «{name}»")),
                },
            }
        };
        apply_end(
            &mut self.bottom_level,
            &mut self.bottom_offset,
            &v.bottom,
            &mut warnings,
        );
        apply_end(
            &mut self.top_level,
            &mut self.top_offset,
            &v.top,
            &mut warnings,
        );
        warnings
    }
}

impl LevelChoice {
    fn from_shared(level: &Shared<Option<ObjectId>>) -> Self {
        match level {
            Shared::Varies => Self::Varies,
            Shared::Same(Some(id)) => Self::Level(*id),
            Shared::Same(None) => Self::Absolute,
        }
    }

    fn label(self, project: &Project) -> String {
        use open_bim_model::numbering_rules::level_mark;
        match self {
            Self::Varies => VARIES.into(),
            Self::Absolute => "Абс. отметка".into(),
            Self::Level(id) => match project.levels.get(&id) {
                Some(l) if l.name == level_mark(l.elevation_mm) => l.name.clone(),
                Some(l) => format!("{} ({})", l.name, level_mark(l.elevation_mm)),
                None => "Абс. отметка".into(),
            },
        }
    }
}

/// Commands for edited column ends. A level moves the end to elevation +
/// offset; «Абс. отметка» clears the binding and sets that Z.
pub fn level_commands(
    project: &Project,
    parts: &[ObjectId],
    shown: &EndsShared,
    bottom_level: LevelChoice,
    bottom_offset: &str,
    top_level: LevelChoice,
    top_offset: &str,
) -> Result<Vec<Command>, String> {
    let parse = |text: &str, label: &str| -> Result<f64, String> {
        crate::tools::number(&text.replace(',', ".")).map_err(|_| format!("{label}: введите число"))
    };
    let mut commands = Vec::new();
    for id in parts {
        let element = project.elements.get(id).ok_or("Объект удалён")?;
        let mut bottom_z = None;
        let mut top_z = None;
        let mut sets = Vec::new();
        for (end, choice, text, was_level, was_offset, label) in [
            (
                open_bim_model::MemberEnd::Bottom,
                bottom_level,
                bottom_offset,
                &shown.bottom.level,
                &shown.bottom.offset,
                "Низ",
            ),
            (
                open_bim_model::MemberEnd::Top,
                top_level,
                top_offset,
                &shown.top.level,
                &shown.top.offset,
                "Верх",
            ),
        ] {
            let same_level = LevelChoice::from_shared(was_level) == choice;
            let was = was_offset.value().map(String::as_str).unwrap_or(VARIES);
            let same_offset = text == was || text == VARIES;
            if same_level && same_offset || matches!(choice, LevelChoice::Varies) && same_offset {
                continue;
            }
            if text == VARIES {
                return Err(format!("{label}: введите смещение"));
            }
            let value = parse(text, label)?;
            match choice {
                LevelChoice::Varies => {}
                LevelChoice::Absolute => {
                    sets.push(Command::SetLevelEnd {
                        part: *id,
                        end,
                        binding: None,
                    });
                    match end {
                        open_bim_model::MemberEnd::Bottom => bottom_z = Some(value),
                        open_bim_model::MemberEnd::Top => top_z = Some(value),
                    }
                }
                LevelChoice::Level(level) => {
                    if !project.levels.contains_key(&level) {
                        return Err(format!("{label}: нет такого уровня"));
                    }
                    sets.push(Command::SetLevelEnd {
                        part: *id,
                        end,
                        binding: Some(open_bim_model::LevelBinding {
                            level,
                            offset_mm: value,
                        }),
                    });
                }
            }
        }
        if bottom_z.is_some() || top_z.is_some() {
            let element =
                open_bim_model::level_ends::with_end_zs(project, element, bottom_z, top_z)
                    .map_err(|e| e.to_string())?;
            commands.push(Command::ReplaceElement { element });
        }
        commands.extend(sets);
    }
    Ok(commands)
}

/// Mark next to a field that a type drives. Empty when the selection has
/// no shared type or the overrides disagree.
pub fn type_mark(shared: Option<&TypeShared>, field: open_bim_model::TypeField) -> &'static str {
    match shared {
        Some(t) if t.overrides_agree && matches!(t.type_id, Shared::Same(Some(_))) => {
            if t.overrides.contains(&field) {
                " · своё"
            } else {
                " · из типа"
            }
        }
        _ => "",
    }
}

/// What the type header asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeAction {
    /// Assign this type, or clear it.
    Assign(Option<ObjectId>),
    /// Make a type from the first selected part and assign it.
    Create,
    SelectAll,
    Similar,
}

/// Header row: type combo, «Выбрать все этого типа», «Создать подобный».
pub fn type_header(
    ui: &mut egui::Ui,
    project: &Project,
    shared: &TypeShared,
) -> Option<TypeAction> {
    let mut action = None;
    ui.horizontal(|ui| {
        let current = match &shared.type_id {
            Shared::Same(Some(id)) => project
                .member_types
                .types
                .get(id)
                .map(|t| t.name.clone())
                .unwrap_or_else(|| "Без типа".into()),
            Shared::Same(None) => "Без типа".into(),
            Shared::Varies => VARIES.into(),
        };
        let l = ui.label("Типоразмер");
        egui::ComboBox::new("member type", "")
            .selected_text(current)
            .show_ui(ui, |ui| {
                if ui.selectable_label(false, "Без типа").clicked() {
                    action = Some(TypeAction::Assign(None));
                }
                let mut types: Vec<_> = project
                    .member_types
                    .types
                    .values()
                    .filter(|t| t.kind == shared.kind)
                    .collect();
                types.sort_by(|a, b| a.name.cmp(&b.name));
                for ty in types {
                    if ui.selectable_label(false, &ty.name).clicked() {
                        action = Some(TypeAction::Assign(Some(ty.id)));
                    }
                }
            })
            .response
            .labelled_by(l.id);
        if ui.button("Создать тип").clicked() {
            action = Some(TypeAction::Create);
        }
    });
    ui.horizontal(|ui| {
        let same = matches!(shared.type_id, Shared::Same(Some(_)));
        if ui
            .add_enabled(same, egui::Button::new("Выбрать все этого типа"))
            .clicked()
        {
            action = Some(TypeAction::SelectAll);
        }
        if ui.button("Создать подобный").clicked() {
            action = Some(TypeAction::Similar);
        }
    });
    action
}

/// «Низ» / «Верх»: level and offset, edited until «Изменить».
pub fn column_level_grid(ui: &mut egui::Ui, project: &Project, pane: &mut PropertyPane) {
    let mut levels: Vec<&open_bim_model::Level> = project.levels.values().collect();
    levels.sort_by(|a, b| a.elevation_mm.total_cmp(&b.elevation_mm));
    egui::Grid::new("selection levels")
        .num_columns(3)
        .show(ui, |ui| {
            for (label, choice, offset) in [
                ("Низ", &mut pane.bottom_level, &mut pane.bottom_offset),
                ("Верх", &mut pane.top_level, &mut pane.top_offset),
            ] {
                let l = ui.label(format!("{label}: уровень"));
                let selected = choice.label(project);
                egui::ComboBox::new(("pane level", label), "")
                    .selected_text(selected)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(choice, LevelChoice::Absolute, "Абс. отметка");
                        for level in &levels {
                            ui.selectable_value(
                                choice,
                                LevelChoice::Level(level.id),
                                choice_name(project, level.id),
                            );
                        }
                    })
                    .response
                    .labelled_by(l.id);
                let unit = if matches!(choice, LevelChoice::Level(_)) {
                    "смещение, мм"
                } else {
                    "Z, мм"
                };
                ui.add(
                    egui::TextEdit::singleline(offset)
                        .id(egui::Id::new(("pane level offset", label)))
                        .desired_width(70.)
                        .hint_text(unit),
                )
                .on_hover_text(unit)
                .labelled_by(l.id)
                .widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::TextEdit,
                        true,
                        format!("{label}, {unit}"),
                    )
                });
                ui.end_row();
            }
        });
}

fn choice_name(project: &Project, id: ObjectId) -> String {
    LevelChoice::Level(id).label(project)
}

/// «Положение» fields: three lists with an offset each, and Dx at
/// both ends. `None` in a list shows «— разные —».
pub fn position_grid(ui: &mut egui::Ui, salt: &str, b: &mut PositionBuffer) {
    /// A list and its offset field in one value cell.
    #[allow(clippy::too_many_arguments)]
    fn list<T: Copy + PartialEq>(
        ui: &mut egui::Ui,
        id: String,
        label: &str,
        value: &mut Option<T>,
        all: &[T],
        name: fn(T) -> &'static str,
        unit: &str,
        offset: &mut String,
    ) {
        let l = ui.label(label);
        ui.horizontal(|ui| {
            let w = field_width(ui);
            egui::ComboBox::new(&id, "")
                .selected_text(value.map_or(VARIES, name))
                .width((w * 0.6).max(110.))
                .show_ui(ui, |ui| {
                    for v in all {
                        ui.selectable_value(value, Some(*v), name(*v));
                    }
                })
                .response
                .labelled_by(l.id);
            ui.add(
                egui::TextEdit::singleline(offset)
                    .id(egui::Id::new((id, "offset")))
                    .desired_width((field_width(ui) - 30.).max(40.)),
            )
            .on_hover_text(unit)
            .widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::TextEdit,
                    true,
                    format!("{label}, {unit}"),
                )
            });
            ui.label(unit);
        });
        ui.end_row();
    }
    fn field(ui: &mut egui::Ui, id: String, label: &str, text: &mut String) {
        let l = ui.label(label);
        ui.add(
            egui::TextEdit::singleline(text)
                .id(egui::Id::new(id))
                .desired_width(field_width(ui)),
        )
        .labelled_by(l.id);
        ui.end_row();
    }
    rows(ui, format!("{salt} position"), |ui| {
        list(
            ui,
            format!("{salt} plane"),
            "В плоскости",
            &mut b.plane,
            &OnPlane::ALL,
            OnPlane::label,
            "мм",
            &mut b.plane_offset,
        );
        list(
            ui,
            format!("{salt} rotation"),
            "Поворот",
            &mut b.rotation,
            &Rotation::ALL,
            Rotation::label,
            "°",
            &mut b.rotation_angle,
        );
        list(
            ui,
            format!("{salt} depth"),
            "По глубине",
            &mut b.depth,
            &AtDepth::ALL,
            AtDepth::label,
            "мм",
            &mut b.depth_offset,
        );
        field(
            ui,
            format!("{salt} start dx"),
            "Dx начала, мм",
            &mut b.start_dx,
        );
        field(ui, format!("{salt} end dx"), "Dx конца, мм", &mut b.end_dx);
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
