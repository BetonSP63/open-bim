//! A member type and its instances. The type holds the profile, material,
//! class, numbering prefix and default position. An instance may override
//! any of those; editing the type rewrites every instance except the
//! fields it overrode. A part without a type is unchanged.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    Element, ElementShape, MemberKind, ModelError, ObjectId, PartPosition, Project, PropertyValue,
};

/// The numbering series prefix. The same key as `numbering::SERIES_PREFIX`.
pub const NUMBERING_PREFIX: &str = "numbering_prefix";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeField {
    Profile,
    Material,
    Class,
    Prefix,
    Position,
}

impl TypeField {
    pub fn label(self) -> &'static str {
        match self {
            Self::Profile => "Профиль",
            Self::Material => "Материал",
            Self::Class => "Класс",
            Self::Prefix => "Префикс",
            Self::Position => "Положение",
        }
    }
    pub const ALL: [Self; 5] = [
        Self::Profile,
        Self::Material,
        Self::Class,
        Self::Prefix,
        Self::Position,
    ];
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemberType {
    pub id: ObjectId,
    pub name: String,
    pub kind: MemberKind,
    pub profile: ObjectId,
    pub material: ObjectId,
    pub class: String,
    /// Empty means the instance keeps its own prefix when the field is not overridden.
    pub prefix: String,
    pub position: PartPosition,
}

/// One part using a type, and the fields it keeps for itself.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypeUse {
    pub part: ObjectId,
    pub type_id: ObjectId,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub overrides: BTreeSet<TypeField>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct MemberTypes {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub types: BTreeMap<ObjectId, MemberType>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub uses: Vec<TypeUse>,
}

impl MemberTypes {
    pub fn is_default(&self) -> bool {
        self.types.is_empty() && self.uses.is_empty()
    }

    pub fn use_of(&self, part: ObjectId) -> Option<&TypeUse> {
        self.uses.iter().find(|u| u.part == part)
    }

    pub fn set_use(&mut self, usage: Option<TypeUse>) {
        let Some(usage) = usage else {
            return;
        };
        self.uses.retain(|u| u.part != usage.part);
        self.uses.push(usage);
    }

    pub fn clear_part(&mut self, part: ObjectId) {
        self.uses.retain(|u| u.part != part);
    }

    pub fn validate(&self, project: &Project) -> Result<(), ModelError> {
        if self.types.len() > 10_000 || self.uses.len() > 100_000 {
            return Err(ModelError::InvalidValue(
                "Слишком много типоразмеров".into(),
            ));
        }
        let mut names = BTreeSet::new();
        for (key, ty) in &self.types {
            if *key != ty.id || ty.id.0 == 0 {
                return Err(ModelError::DuplicateId(ty.id));
            }
            let name = ty.name.trim();
            if name.is_empty() || name.chars().count() > 61 || !names.insert(name.to_string()) {
                return Err(ModelError::InvalidValue(format!(
                    "Типоразмер «{}»: имя пустое, длинное или повторяется",
                    ty.name
                )));
            }
            if !project.profiles.contains_key(&ty.profile) {
                return Err(ModelError::MissingReference(ty.profile));
            }
            if !project.materials.contains_key(&ty.material) {
                return Err(ModelError::MissingReference(ty.material));
            }
            if ty.class.chars().count() > 64 || ty.prefix.chars().count() > 16 {
                return Err(ModelError::InvalidValue(format!(
                    "Типоразмер «{}»: класс или префикс слишком длинный",
                    ty.name
                )));
            }
            if ty.class.contains('\0') || ty.prefix.contains('\0') {
                return Err(ModelError::InvalidValue(
                    "Класс или префикс типоразмера".into(),
                ));
            }
        }
        let mut seen = BTreeSet::new();
        for usage in &self.uses {
            if !seen.insert(usage.part) {
                return Err(ModelError::InvalidValue(format!(
                    "Деталь {} принадлежит двум типоразмерам",
                    usage.part
                )));
            }
            let element = project
                .elements
                .get(&usage.part)
                .ok_or(ModelError::MissingReference(usage.part))?;
            let ty = self
                .types
                .get(&usage.type_id)
                .ok_or(ModelError::MissingReference(usage.type_id))?;
            let kind = match &element.shape {
                ElementShape::Member { member_kind, .. } => *member_kind,
                ElementShape::Plate { .. } => {
                    return Err(ModelError::InvalidValue(format!(
                        "«{}»: типоразмер задаётся балке или колонне",
                        element.name
                    )));
                }
            };
            if kind != ty.kind {
                return Err(ModelError::InvalidValue(format!(
                    "«{}»: типоразмер «{}» другого вида",
                    element.name, ty.name
                )));
            }
        }
        Ok(())
    }
}

fn class_of(element: &Element) -> String {
    match element.properties.get("class") {
        Some(PropertyValue::Text(t)) => t.clone(),
        Some(PropertyValue::Number(n)) => format!("{n:.0}"),
        _ => String::new(),
    }
}

fn prefix_of(element: &Element) -> String {
    match element.properties.get(NUMBERING_PREFIX) {
        Some(PropertyValue::Text(t)) => t.clone(),
        _ => String::new(),
    }
}

/// Write the type onto `element`, skipping overridden fields.
pub fn apply_type(
    project: &Project,
    element: &Element,
    ty: &MemberType,
    overrides: &BTreeSet<TypeField>,
) -> Result<Element, ModelError> {
    let mut element = element.clone();
    if !overrides.contains(&TypeField::Material) {
        element.material = ty.material;
    }
    if !overrides.contains(&TypeField::Class) {
        if ty.class.is_empty() {
            element.properties.remove("class");
        } else {
            element
                .properties
                .insert("class".into(), PropertyValue::Text(ty.class.clone()));
        }
    }
    if !overrides.contains(&TypeField::Prefix) {
        if ty.prefix.is_empty() {
            element.properties.remove(NUMBERING_PREFIX);
        } else {
            element.properties.insert(
                NUMBERING_PREFIX.into(),
                PropertyValue::Text(ty.prefix.clone()),
            );
        }
    }
    let profile = (!overrides.contains(&TypeField::Profile)).then_some(ty.profile);
    let position = (!overrides.contains(&TypeField::Position)).then_some(ty.position);
    if profile.is_some() || position.is_some() {
        if !matches!(element.shape, ElementShape::Member { .. }) {
            return Err(ModelError::InvalidValue(
                "Типоразмер задаётся балке или колонне".into(),
            ));
        }
        element.shape = project.replace_member(&element.shape, None, profile, position)?;
    }
    Ok(element)
}

/// Fields of `element` that differ from the type. Used to mark overrides
/// after the user edits an instance.
pub fn differing(project: &Project, element: &Element, ty: &MemberType) -> BTreeSet<TypeField> {
    let mut out = BTreeSet::new();
    if let ElementShape::Member {
        profile, position, ..
    } = &element.shape
    {
        if *profile != ty.profile {
            out.insert(TypeField::Profile);
        }
        if *position != ty.position {
            out.insert(TypeField::Position);
        }
    }
    if element.material != ty.material {
        out.insert(TypeField::Material);
    }
    if class_of(element) != ty.class {
        out.insert(TypeField::Class);
    }
    if prefix_of(element) != ty.prefix {
        out.insert(TypeField::Prefix);
    }
    let _ = project;
    out
}

/// Apply `ty` to every instance that does not override the written fields.
pub fn apply_to_instances(project: &mut Project, type_id: ObjectId) -> Result<(), ModelError> {
    let Some(ty) = project.member_types.types.get(&type_id).cloned() else {
        return Err(ModelError::MissingReference(type_id));
    };
    let uses: Vec<_> = project
        .member_types
        .uses
        .iter()
        .filter(|u| u.type_id == type_id)
        .cloned()
        .collect();
    for usage in uses {
        let Some(element) = project.elements.get(&usage.part).cloned() else {
            continue;
        };
        let updated = apply_type(project, &element, &ty, &usage.overrides)?;
        if updated != element {
            project.elements.insert(usage.part, updated);
        }
    }
    Ok(())
}
