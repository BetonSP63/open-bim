//! Property pane like Tekla: the common properties of the selected parts,
//! «— разные —» where they differ, and «Изменить» that applies the edited
//! fields to every selected part in one transaction.
use open_bim_commands::Command;
use open_bim_model::{ElementShape, ObjectId, Project, PropertyValue};
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
                ElementShape::Plate { .. } => "Пластина",
            }
            .to_string()
        }))?,
        name: Shared::of(parts.iter().map(|e| e.name.clone()))?,
        profile: Shared::of(members),
        material: Shared::of(parts.iter().map(|e| e.material))?,
        class: Shared::of(parts.iter().map(|e| class_of(e)))?,
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
        if let (Some(p), ElementShape::Member { profile, .. }) = (changes.profile, &mut e.shape) {
            *profile = p;
        }
        if let Some(class) = &changes.class {
            if class.trim().is_empty() {
                e.properties.remove("class");
            } else {
                e.properties
                    .insert("class".into(), PropertyValue::Text(class.trim().into()));
            }
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

/// Pane state: the edit buffer for the current selection.
#[derive(Clone, Debug, Default)]
pub struct PropertyPane {
    /// Selection the buffer was filled from.
    pub source: Vec<ObjectId>,
    pub name: String,
    pub profile: Option<ObjectId>,
    pub material: Option<ObjectId>,
    pub class: String,
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
    }

    /// Fields edited relative to what the selection shows.
    pub fn changes(&self, c: &Common) -> Changes {
        let edited = |buffer: &str, shown: &Shared<String>| {
            let shown = shown.value().map(String::as_str).unwrap_or(VARIES);
            (buffer != shown && buffer != VARIES).then(|| buffer.to_string())
        };
        Changes {
            name: edited(&self.name, &c.name),
            class: edited(&self.class, &c.class),
            profile: self
                .profile
                .filter(|p| c.profile.as_ref().and_then(|s| s.value()) != Some(p)),
            material: self.material.filter(|m| c.material.value() != Some(m)),
        }
    }
}
