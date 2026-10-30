//! Three-way merge of whole projects, one unit at a time. A unit is one entry
//! of a keyed map (an element, a drawing, a family…) or one other top-level
//! field taken whole. Units changed on one side only are taken from that side;
//! units changed on both sides differently are conflicts.
use open_bim_model::Project;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

/// Keyed maps: entries merge and lock one by one (by object ID, families by name).
/// `swept_parts` is keyed by its element's ID, so it shares that lock.
pub const KEYED_MAPS: [&str; 15] = [
    "materials",
    "profiles",
    "elements",
    "assemblies",
    "axes",
    "levels",
    "details",
    "connections",
    "drawings",
    "swept_parts",
    "construction",
    "dimensions",
    "plugins",
    "family_instances",
    "families",
];
/// Counters handled by the merge itself, never units.
const COUNTERS: [&str; 2] = ["revision", "next_id"];

/// One merge and lock unit: `key` is the map key for keyed maps.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Unit {
    pub field: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

impl Unit {
    /// Lock name: objects by ID («id-12»), families by name, fields by name.
    /// The name is safe as a file name.
    pub fn lock_name(&self) -> String {
        match (&self.key, self.field.as_str()) {
            (Some(name), "families") => format!("family-{}", hex(name)),
            (Some(id), _) => format!("id-{id}"),
            (None, field) => format!("field-{field}"),
        }
    }

    /// Readable name for messages: «объект 12», «семейство Балка», «поле grid».
    pub fn label(&self) -> String {
        match (&self.key, self.field.as_str()) {
            (Some(name), "families") => format!("семейство {name}"),
            (Some(id), _) => format!("объект {id}"),
            (None, field) => format!("поле {field}"),
        }
    }
}

fn hex(text: &str) -> String {
    text.bytes().map(|b| format!("{b:02x}")).collect()
}

/// Which side wins a conflicting unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Resolution {
    /// Keep the local version.
    Mine,
    /// Take the shared version.
    Theirs,
}

/// A unit changed on both sides differently; `None` means deleted.
#[derive(Clone, Debug, PartialEq)]
pub struct Conflict {
    pub unit: Unit,
    pub mine: Option<Value>,
    pub theirs: Option<Value>,
}

pub(crate) fn units(project: &Project) -> Result<BTreeMap<Unit, Value>, String> {
    let Value::Object(fields) = serde_json::to_value(project).map_err(|e| e.to_string())? else {
        return Err("Проект не является объектом JSON".into());
    };
    let mut units = BTreeMap::new();
    for (field, value) in fields {
        if COUNTERS.contains(&field.as_str()) {
            continue;
        }
        match value {
            Value::Object(entries) if KEYED_MAPS.contains(&field.as_str()) => {
                for (key, value) in entries {
                    units.insert(
                        Unit {
                            field: field.clone(),
                            key: Some(key),
                        },
                        value,
                    );
                }
            }
            value => {
                units.insert(Unit { field, key: None }, value);
            }
        }
    }
    Ok(units)
}

/// Units that differ between two versions of a project.
pub fn changed_units(before: &Project, after: &Project) -> Result<BTreeSet<Unit>, String> {
    let (before, after) = (units(before)?, units(after)?);
    Ok(before
        .keys()
        .chain(after.keys())
        .filter(|unit| before.get(*unit) != after.get(*unit))
        .cloned()
        .collect())
}

pub(crate) struct Merged {
    pub project: Project,
    /// Units the result takes from the local side (what a publish sends).
    pub from_mine: BTreeSet<Unit>,
}

/// Merges `mine` and `theirs`, both edited from `base`. Unresolved
/// conflicts are returned all at once; the result is validated.
pub(crate) fn merge(
    base: &Project,
    mine: &Project,
    theirs: &Project,
    resolutions: &BTreeMap<Unit, Resolution>,
) -> Result<Result<Merged, Vec<Conflict>>, String> {
    let (b, m, t) = (units(base)?, units(mine)?, units(theirs)?);
    let all: BTreeSet<&Unit> = b.keys().chain(m.keys()).chain(t.keys()).collect();
    let mut result = BTreeMap::new();
    let mut from_mine = BTreeSet::new();
    let mut conflicts = Vec::new();
    for unit in all {
        let (bv, mv, tv) = (b.get(unit), m.get(unit), t.get(unit));
        let take_mine = if mv == tv || mv == bv {
            false
        } else if tv == bv {
            true
        } else {
            match resolutions.get(unit) {
                Some(Resolution::Mine) => true,
                Some(Resolution::Theirs) => false,
                None => {
                    conflicts.push(Conflict {
                        unit: unit.clone(),
                        mine: mv.cloned(),
                        theirs: tv.cloned(),
                    });
                    continue;
                }
            }
        };
        let chosen = if take_mine {
            if mv != tv {
                from_mine.insert(unit.clone());
            }
            mv
        } else {
            tv
        };
        if let Some(value) = chosen {
            result.insert(unit.clone(), value.clone());
        }
    }
    if !conflicts.is_empty() {
        return Ok(Err(conflicts));
    }
    let mut fields = Map::new();
    for map in KEYED_MAPS {
        fields.insert(map.into(), Value::Object(Map::new()));
    }
    for (unit, value) in result {
        match unit.key {
            Some(key) => {
                if let Some(Value::Object(entries)) = fields.get_mut(&unit.field) {
                    entries.insert(key, value);
                }
            }
            None => {
                fields.insert(unit.field, value);
            }
        }
    }
    fields.insert(
        "revision".into(),
        (mine.revision.max(theirs.revision) + 1).into(),
    );
    // The local counter stays in this engineer's own ID range.
    fields.insert("next_id".into(), mine.next_id.into());
    let project: Project = serde_json::from_value(Value::Object(fields))
        .map_err(|e| format!("Объединённая модель не читается: {e}"))?;
    project
        .validate()
        .map_err(|e| format!("Объединённая модель не проходит проверку: {e}"))?;
    Ok(Ok(Merged { project, from_mine }))
}
