//! Columns by levels («Низ»/«Верх» from a level): each end is a level
//! plus an offset. The column keeps the levels in its properties and follows
//! when a level's elevation changes, unless it was moved off them since.
use open_bim_commands::Command;
use open_bim_model::{ElementShape, MemberKind, ObjectId, Point3, Project, PropertyValue};
use std::collections::BTreeMap;

pub const BOTTOM_LEVEL: &str = "level_bottom";
pub const BOTTOM_OFFSET: &str = "level_bottom_offset";
pub const TOP_LEVEL: &str = "level_top";
pub const TOP_OFFSET: &str = "level_top_offset";

/// A column end within 0.5 mm of its level + offset is still on the level.
const ON_LEVEL_MM: f64 = 0.5;

/// One end of a column: an offset from a level, or an absolute Z (mm).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Height {
    pub level: Option<ObjectId>,
    pub value: f64,
}

impl Height {
    pub fn z(&self, project: &Project) -> Result<f64, String> {
        match self.level {
            None => Ok(self.value),
            Some(id) => project
                .levels
                .get(&id)
                .map(|l| l.elevation_mm + self.value)
                .ok_or_else(|| "Уровень колонны удалён, выберите другой".into()),
        }
    }
}

/// Bottom and top Z of a new column; the top must be above the bottom.
pub fn heights(project: &Project, bottom: Height, top: Height) -> Result<(f64, f64), String> {
    let (b, t) = (bottom.z(project)?, top.z(project)?);
    if t <= b {
        return Err("Верх колонны должен быть выше низа".into());
    }
    Ok((b, t))
}

/// Properties binding a new column to its levels (empty for absolute Z).
pub fn binding(bottom: Height, top: Height) -> BTreeMap<String, PropertyValue> {
    let mut properties = BTreeMap::new();
    for (h, level, offset) in [
        (bottom, BOTTOM_LEVEL, BOTTOM_OFFSET),
        (top, TOP_LEVEL, TOP_OFFSET),
    ] {
        if let Some(id) = h.level {
            properties.insert(level.into(), PropertyValue::Number(id.0 as f64));
            properties.insert(offset.into(), PropertyValue::Number(h.value));
        }
    }
    properties
}

/// The level and offset a column end is bound to.
pub fn bound(
    properties: &BTreeMap<String, PropertyValue>,
    level: &str,
    offset: &str,
) -> Option<(ObjectId, f64)> {
    let Some(PropertyValue::Number(id)) = properties.get(level) else {
        return None;
    };
    let offset = match properties.get(offset) {
        Some(PropertyValue::Number(o)) => *o,
        _ => 0.,
    };
    Some((ObjectId(*id as u64), offset))
}

/// Commands that move bound columns to new level elevations (`elevations`:
/// level id → new Z). Elevations are read from `project` before the change;
/// a column end that is no longer on its old level + offset stays put.
pub fn follow(
    project: &Project,
    elevations: &BTreeMap<ObjectId, f64>,
) -> Result<Vec<Command>, String> {
    let mut commands = Vec::new();
    for e in project.elements.values() {
        let ElementShape::Member {
            member_kind: MemberKind::Column,
            ..
        } = &e.shape
        else {
            continue;
        };
        let ends = [
            bound(&e.properties, BOTTOM_LEVEL, BOTTOM_OFFSET),
            bound(&e.properties, TOP_LEVEL, TOP_OFFSET),
        ];
        if !ends
            .iter()
            .flatten()
            .any(|(id, _)| elevations.contains_key(id))
        {
            continue;
        }
        let Some((start, end, _)) = project.member_reference_line(&e.shape) else {
            continue;
        };
        // Bottom is the lower end of the reference line.
        let (mut bottom, mut top, flipped) = if start.0[2] <= end.0[2] {
            (start, end, false)
        } else {
            (end, start, true)
        };
        let mut moved = false;
        for (point, end) in [(&mut bottom, ends[0]), (&mut top, ends[1])] {
            let Some((id, offset)) = end else { continue };
            let (Some(new), Some(old)) = (elevations.get(&id), project.levels.get(&id)) else {
                continue;
            };
            if (point.0[2] - (old.elevation_mm + offset)).abs() <= ON_LEVEL_MM
                && (point.0[2] - (new + offset)).abs() > f64::EPSILON
            {
                point.0[2] = new + offset;
                moved = true;
            }
        }
        if !moved {
            continue;
        }
        if top.0[2] <= bottom.0[2] {
            return Err(format!("{}: верх колонны окажется ниже низа", e.name));
        }
        let line: (Point3, Point3) = if flipped {
            (top, bottom)
        } else {
            (bottom, top)
        };
        let mut element = e.clone();
        element.shape = project
            .replace_member(&e.shape, Some(line), None, None)
            .map_err(|err| format!("{}: {err}", e.name))?;
        commands.push(Command::ReplaceElement { element });
    }
    Ok(commands)
}
