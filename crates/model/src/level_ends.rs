//! Column ends bound to storeys. World Z of an end is the level elevation
//! plus an offset. This is not a constraint solver: only these two ends of
//! a column are driven, and only when that binding or that level changes.
//! A handle drag keeps the binding and writes a new offset.
use serde::{Deserialize, Serialize};

use crate::{
    Element, ElementShape, MemberKind, ModelError, ObjectId, Point3, Project, PropertyValue,
};

/// Property keys of the previous binding (written into the part). Loaded
/// files are copied into [`LevelEnds`] and the keys are dropped.
pub const LEGACY_BOTTOM_LEVEL: &str = "level_bottom";
pub const LEGACY_BOTTOM_OFFSET: &str = "level_bottom_offset";
pub const LEGACY_TOP_LEVEL: &str = "level_top";
pub const LEGACY_TOP_OFFSET: &str = "level_top_offset";

/// An end still within this of `elevation + offset` is on its binding.
pub const ON_LEVEL_MM: f64 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberEnd {
    Bottom,
    Top,
}

impl MemberEnd {
    pub fn label(self) -> &'static str {
        match self {
            Self::Bottom => "Низ",
            Self::Top => "Верх",
        }
    }
    pub const ALL: [Self; 2] = [Self::Bottom, Self::Top];
}

/// Level and offset of one end. Z = elevation + offset, millimetres.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LevelBinding {
    pub level: ObjectId,
    pub offset_mm: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LevelEnd {
    pub part: ObjectId,
    pub end: MemberEnd,
    pub level: ObjectId,
    pub offset_mm: f64,
}

impl LevelEnd {
    pub fn binding(&self) -> LevelBinding {
        LevelBinding {
            level: self.level,
            offset_mm: self.offset_mm,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct LevelEnds {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ends: Vec<LevelEnd>,
}

impl LevelEnds {
    pub fn is_default(&self) -> bool {
        self.ends.is_empty()
    }

    pub fn get(&self, part: ObjectId, end: MemberEnd) -> Option<&LevelEnd> {
        self.ends.iter().find(|e| e.part == part && e.end == end)
    }

    /// `None` clears the binding of that end.
    pub fn set(&mut self, part: ObjectId, end: MemberEnd, binding: Option<LevelBinding>) {
        self.ends.retain(|e| !(e.part == part && e.end == end));
        if let Some(binding) = binding {
            self.ends.push(LevelEnd {
                part,
                end,
                level: binding.level,
                offset_mm: binding.offset_mm,
            });
        }
    }

    pub fn validate(&self, project: &Project) -> Result<(), ModelError> {
        if self.ends.len() > 100_000 {
            return Err(ModelError::InvalidValue(
                "Слишком много привязок к уровням".into(),
            ));
        }
        let mut seen = std::collections::BTreeSet::new();
        for end in &self.ends {
            if !seen.insert((end.part, end.end)) {
                return Err(ModelError::InvalidValue(format!(
                    "Колонна {}: {} привязан дважды",
                    end.part,
                    end.end.label()
                )));
            }
            let element = project
                .elements
                .get(&end.part)
                .ok_or(ModelError::MissingReference(end.part))?;
            if !is_column(element) {
                return Err(ModelError::InvalidValue(format!(
                    "К уровню привязывается конец колонны, не «{}»",
                    element.name
                )));
            }
            if !project.levels.contains_key(&end.level) {
                return Err(ModelError::InvalidValue(format!(
                    "Колонна «{}»: {} ссылается на удалённый уровень",
                    element.name,
                    end.end.label()
                )));
            }
            if !end.offset_mm.is_finite() || end.offset_mm.abs() > 1e8 {
                return Err(ModelError::InvalidValue(format!(
                    "Колонна «{}»: смещение {} вне диапазона",
                    element.name,
                    end.end.label()
                )));
            }
        }
        Ok(())
    }
}

pub fn is_column(element: &Element) -> bool {
    matches!(
        element.shape,
        ElementShape::Member {
            member_kind: MemberKind::Column,
            ..
        }
    )
}

/// Lower and upper ends of the reference line, and whether the stored
/// start is the upper end.
pub fn column_ends(project: &Project, shape: &ElementShape) -> Option<(Point3, Point3, bool)> {
    let (start, end, _) = project.member_reference_line(shape)?;
    if start.0[2] <= end.0[2] {
        Some((start, end, false))
    } else {
        Some((end, start, true))
    }
}

pub fn end_z(project: &Project, shape: &ElementShape, end: MemberEnd) -> Option<f64> {
    let (bottom, top, _) = column_ends(project, shape)?;
    Some(match end {
        MemberEnd::Bottom => bottom.0[2],
        MemberEnd::Top => top.0[2],
    })
}

/// The column with one or both reference-line ends moved in Z.
pub fn with_end_zs(
    project: &Project,
    element: &Element,
    bottom_z: Option<f64>,
    top_z: Option<f64>,
) -> Result<Element, ModelError> {
    let (mut bottom, mut top, flipped) = column_ends(project, &element.shape).ok_or(
        ModelError::InvalidValue(format!("«{}»: нет опорной линии", element.name)),
    )?;
    if let Some(z) = bottom_z {
        bottom.0[2] = z;
    }
    if let Some(z) = top_z {
        top.0[2] = z;
    }
    if !(bottom.0[2].is_finite() && top.0[2].is_finite()) || top.0[2] <= bottom.0[2] + 0.001 {
        return Err(ModelError::InvalidValue(format!(
            "«{}»: верх колонны должен быть выше низа",
            element.name
        )));
    }
    let line = if flipped {
        (top, bottom)
    } else {
        (bottom, top)
    };
    let mut element = element.clone();
    element.shape = project.replace_member(&element.shape, Some(line), None, None)?;
    Ok(element)
}

/// After the commands of one transaction: a changed binding or a moved
/// level pulls the end to `elevation + offset`; a handle drag that left
/// the end elsewhere keeps the binding and stores the new offset.
pub fn settle(before: &Project, staged: &mut Project) -> Result<(), ModelError> {
    let mut drive: Vec<(ObjectId, MemberEnd)> = Vec::new();
    for end in &staged.level_ends.ends {
        let old = before.level_ends.get(end.part, end.end);
        let level_moved = before.levels.get(&end.level).map(|l| l.elevation_mm)
            != staged.levels.get(&end.level).map(|l| l.elevation_mm);
        let binding_changed = match old {
            None => true,
            Some(old) => old.level != end.level || (old.offset_mm - end.offset_mm).abs() > 1e-6,
        };
        if binding_changed || level_moved {
            drive.push((end.part, end.end));
        }
    }
    let parts: Vec<ObjectId> = {
        let mut ids: Vec<_> = drive.iter().map(|(id, _)| *id).collect();
        ids.sort();
        ids.dedup();
        ids
    };
    for part in parts {
        let Some(element) = staged.elements.get(&part).cloned() else {
            continue;
        };
        if !is_column(&element) {
            continue;
        }
        let mut bottom_z = None;
        let mut top_z = None;
        for end in MemberEnd::ALL {
            if !drive.contains(&(part, end)) {
                continue;
            }
            let Some(binding) = staged.level_ends.get(part, end) else {
                continue;
            };
            let Some(level) = staged.levels.get(&binding.level) else {
                continue;
            };
            let z = level.elevation_mm + binding.offset_mm;
            match end {
                MemberEnd::Bottom => bottom_z = Some(z),
                MemberEnd::Top => top_z = Some(z),
            }
        }
        if bottom_z.is_none() && top_z.is_none() {
            continue;
        }
        let current_b = end_z(staged, &element.shape, MemberEnd::Bottom);
        let current_t = end_z(staged, &element.shape, MemberEnd::Top);
        let same = |target: Option<f64>, current: Option<f64>| match (target, current) {
            (Some(t), Some(c)) => (t - c).abs() <= 1e-6,
            _ => target.is_none(),
        };
        if same(bottom_z, current_b) && same(top_z, current_t) {
            continue;
        }
        let updated = with_end_zs(staged, &element, bottom_z, top_z)?;
        staged.elements.insert(part, updated);
    }
    // Geometry that moved without a new binding: keep the binding, write
    // the offset. A missing level is reported by validation, not dropped.
    let ends = staged.level_ends.ends.clone();
    for end in ends {
        if drive.contains(&(end.part, end.end)) {
            continue;
        }
        let Some(level) = staged.levels.get(&end.level) else {
            continue;
        };
        let Some(element) = staged.elements.get(&end.part) else {
            continue;
        };
        let Some(z) = end_z(staged, &element.shape, end.end) else {
            continue;
        };
        let target = level.elevation_mm + end.offset_mm;
        if (z - target).abs() > ON_LEVEL_MM {
            let offset = z - level.elevation_mm;
            if let Some(slot) = staged
                .level_ends
                .ends
                .iter_mut()
                .find(|e| e.part == end.part && e.end == end.end)
            {
                slot.offset_mm = offset;
            }
        }
    }
    Ok(())
}

fn legacy_pair(
    properties: &std::collections::BTreeMap<String, PropertyValue>,
    level_key: &str,
    offset_key: &str,
) -> Option<(ObjectId, f64)> {
    let PropertyValue::Number(id) = properties.get(level_key)? else {
        return None;
    };
    if !id.is_finite() || *id < 1.0 || id.fract() != 0.0 {
        return None;
    }
    let offset = match properties.get(offset_key) {
        Some(PropertyValue::Number(o)) => *o,
        _ => 0.0,
    };
    Some((ObjectId(*id as u64), offset))
}

/// Copy the old property binding into [`Project::level_ends`] and drop the
/// keys. A file that already has the model binding is left as it is.
pub fn adopt_legacy(project: &mut Project) {
    let found: Vec<_> = project
        .elements
        .iter()
        .filter(|(_, e)| is_column(e))
        .map(|(id, e)| (*id, e.properties.clone()))
        .collect();
    for (id, properties) in found {
        for (end, level_key, offset_key) in [
            (MemberEnd::Bottom, LEGACY_BOTTOM_LEVEL, LEGACY_BOTTOM_OFFSET),
            (MemberEnd::Top, LEGACY_TOP_LEVEL, LEGACY_TOP_OFFSET),
        ] {
            let Some((level, offset)) = legacy_pair(&properties, level_key, offset_key) else {
                continue;
            };
            if project.level_ends.get(id, end).is_none() && project.levels.contains_key(&level) {
                project.level_ends.set(
                    id,
                    end,
                    Some(LevelBinding {
                        level,
                        offset_mm: offset,
                    }),
                );
            }
            if let Some(element) = project.elements.get_mut(&id) {
                element.properties.remove(level_key);
                element.properties.remove(offset_key);
            }
        }
    }
}

/// Ends whose Z is not `elevation + offset` (the binding survived, the
/// geometry did not). A missing level is included.
pub fn broken(project: &Project) -> Vec<LevelEnd> {
    project
        .level_ends
        .ends
        .iter()
        .filter(|end| match project.levels.get(&end.level) {
            None => true,
            Some(level) => project
                .elements
                .get(&end.part)
                .and_then(|e| end_z(project, &e.shape, end.end))
                .is_none_or(|z| (z - (level.elevation_mm + end.offset_mm)).abs() > ON_LEVEL_MM),
        })
        .cloned()
        .collect()
}
