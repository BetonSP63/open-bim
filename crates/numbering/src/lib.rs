//! Deterministic manufacturing equivalence, independent of model IDs and placement.
//! Coordinates use 1e-6 mm resolution; direction cosines use 1e-9 resolution.
use open_bim_model::{
    DetailKind, ElementShape, Frame, MemberKind, ModelError, ObjectId, Point3, Project,
    PropertyValue,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Numbering {
    pub parts: BTreeMap<ObjectId, String>,
    pub assemblies: BTreeMap<ObjectId, String>,
    pub part_keys: BTreeMap<ObjectId, String>,
    pub assembly_keys: BTreeMap<ObjectId, String>,
}

impl Numbering {
    /// IDs whose position or equivalence changed, including removed objects.
    pub fn changed_since(&self, previous: &Self) -> BTreeSet<ObjectId> {
        let mut changed = BTreeSet::new();
        for (current, old, keys, old_keys) in [
            (
                &self.parts,
                &previous.parts,
                &self.part_keys,
                &previous.part_keys,
            ),
            (
                &self.assemblies,
                &previous.assemblies,
                &self.assembly_keys,
                &previous.assembly_keys,
            ),
        ] {
            for id in current.keys().chain(old.keys()) {
                if current.get(id) != old.get(id) || keys.get(id) != old_keys.get(id) {
                    changed.insert(*id);
                }
            }
        }
        changed
    }
}

fn serialized(value: &Value) -> Result<String, ModelError> {
    let bytes = serde_json::to_vec(value).map_err(|e| ModelError::Serialization(e.to_string()))?;
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn quantize(value: &mut Value, scale: f64) {
    match value {
        Value::Number(n) => {
            if let Some(v) = n.as_f64() {
                *value = json!((v * scale).round() as i64);
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|v| quantize(v, scale)),
        Value::Object(o) => o.values_mut().for_each(|v| quantize(v, scale)),
        _ => {}
    }
}

fn dimensions<T: Serialize>(value: T) -> Value {
    let mut v = json!(value);
    quantize(&mut v, 1e6);
    v
}

fn frame_key(frame: &Frame) -> Value {
    let mut axes = json!(frame.axes);
    quantize(&mut axes, 1e9);
    json!({"origin": dimensions(frame.origin), "axes": axes})
}

fn canonical_contour(contour: &[[f64; 2]]) -> Vec<[i64; 2]> {
    let points: Vec<[i64; 2]> = contour
        .iter()
        .map(|p| p.map(|v| (v * 1e6).round() as i64))
        .collect();
    let mut candidates = Vec::new();
    for reverse in [false, true] {
        let mut sequence = points.clone();
        if reverse {
            sequence.reverse();
        }
        for start in 0..sequence.len() {
            candidates.push(
                sequence[start..]
                    .iter()
                    .chain(&sequence[..start])
                    .copied()
                    .collect::<Vec<_>>(),
            );
        }
    }
    candidates.into_iter().min().unwrap_or_default()
}

fn local_point(frame: &Frame, world: Point3) -> Point3 {
    Point3(std::array::from_fn(|i| {
        (0..3)
            .map(|j| (world.0[j] - frame.origin.0[j]) * frame.axes[i][j])
            .sum()
    }))
}

fn relative_frame(reference: &Frame, other: &Frame) -> Frame {
    Frame {
        origin: local_point(reference, other.origin),
        axes: std::array::from_fn(|i| {
            std::array::from_fn(|j| {
                (0..3)
                    .map(|k| other.axes[i][k] * reference.axes[j][k])
                    .sum()
            })
        }),
    }
}

/// Canonical manufacture key. Model names and catalog IDs are not manufacture data.
pub fn part_key(project: &Project, id: ObjectId) -> Result<String, ModelError> {
    let part = project
        .elements
        .get(&id)
        .ok_or(ModelError::MissingReference(id))?;
    let material = project
        .materials
        .get(&part.material)
        .ok_or(ModelError::MissingReference(part.material))?;
    let shape = match &part.shape {
        ElementShape::Member {
            profile, length, ..
        } => {
            let profile = project
                .profiles
                .get(profile)
                .ok_or(ModelError::MissingReference(*profile))?;
            json!({"kind":"member", "profile": dimensions(&profile.shape), "length": dimensions(length.value())})
        }
        ElementShape::Plate {
            contour, thickness, ..
        } => {
            json!({"kind":"plate", "contour":canonical_contour(contour),"thickness":dimensions(thickness.value())})
        }
    };
    let mut features = Vec::new();
    for detail in project
        .details
        .values()
        .filter(|d| d.kind.part() == Some(id))
    {
        let feature = match &detail.kind {
            DetailKind::Hole {
                frame,
                diameter,
                depth,
                ..
            } => {
                json!({"kind":"hole","frame":frame_key(frame),"diameter":dimensions(diameter),"depth":dimensions(depth)})
            }
            DetailKind::BoxCut { origin, size, .. } => {
                json!({"kind":"box_cut","origin":dimensions(origin),"size":dimensions(size)})
            }
            DetailKind::PlaneCut { normal, offset, .. } => {
                let norm = normal.iter().map(|v| v * v).sum::<f64>().sqrt();
                let mut direction = json!(normal.map(|v| v / norm));
                quantize(&mut direction, 1e9);
                json!({"kind":"plane_cut","normal":direction,"offset":dimensions(offset/norm)})
            }
            _ => unreachable!("part() only returns local manufacture features"),
        };
        features.push(serialized(&feature)?);
    }
    features.sort();
    serialized(
        &json!({"schema":1,"material":{"name":material.name,"density":material.density_kg_m3},"shape":shape,"features":features,"properties":part.properties}),
    )
}

/// Property keys of a numbering series on a part or an assembly: the mark
/// prefix (text) and the first number (number).
pub const SERIES_PREFIX: &str = "numbering_prefix";
pub const SERIES_START: &str = "numbering_start";

/// Numbering series: marks are compared and numbered only within a series.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Series {
    pub prefix: String,
    pub start: u64,
}

impl Series {
    /// Number of `mark` in this series («К12» in К → 12).
    fn number_of(&self, mark: &str) -> Option<u64> {
        let rest = mark.strip_prefix(self.prefix.as_str())?;
        if rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        rest.parse().ok()
    }
}

/// Series set on an object by its properties, or the default one.
fn series(properties: &BTreeMap<String, PropertyValue>, default_prefix: &str) -> Series {
    let prefix = match properties.get(SERIES_PREFIX) {
        Some(PropertyValue::Text(t)) => t.trim().to_string(),
        _ => default_prefix.to_string(),
    };
    let start = match properties.get(SERIES_START) {
        Some(PropertyValue::Number(n)) if n.is_finite() && *n >= 1. => n.round() as u64,
        _ => 1,
    };
    Series { prefix, start }
}

/// Series of a part (default: plain numbers from 1).
pub fn part_series(project: &Project, id: ObjectId) -> Series {
    match project.elements.get(&id) {
        Some(e) => series(&e.properties, ""),
        None => series(&BTreeMap::new(), ""),
    }
}

/// Series of an assembly (default: К, Б or М by the main part, from 1).
pub fn assembly_series(project: &Project, id: ObjectId) -> Series {
    let prefix = assembly_prefix(project, id);
    match project.assemblies.get(&id) {
        Some(a) => series(&a.properties, prefix),
        None => series(&BTreeMap::new(), prefix),
    }
}

/// Group of equal objects in one series.
type Group<'a> = (Series, &'a String);

/// Marks within each series: equal keys share a mark. A group of equal
/// objects keeps the old mark most of its members had at the last run (the
/// lowest number on a tie) unless another group already took it; the other
/// groups get the next numbers after the highest one kept, in key order.
/// Without old marks this numbers start, start+1… in key order.
fn positions(
    keys: &BTreeMap<ObjectId, String>,
    series_of: impl Fn(ObjectId) -> Series,
    old: &BTreeMap<ObjectId, String>,
) -> BTreeMap<ObjectId, String> {
    let mut groups: BTreeMap<Group, Vec<ObjectId>> = BTreeMap::new();
    for (id, key) in keys {
        groups.entry((series_of(*id), key)).or_default().push(*id);
    }
    let mut by_series: BTreeMap<&Series, Vec<&Group>> = BTreeMap::new();
    for group in groups.keys() {
        by_series.entry(&group.0).or_default().push(group);
    }
    let mut marks: BTreeMap<&Group, String> = BTreeMap::new();
    for (series, members) in by_series {
        // (members holding the mark, number, group), most held first.
        let mut candidates = Vec::new();
        for group in &members {
            let mut counts: BTreeMap<u64, usize> = BTreeMap::new();
            for id in &groups[*group] {
                if let Some(n) = old.get(id).and_then(|m| series.number_of(m)) {
                    *counts.entry(n).or_default() += 1;
                }
            }
            for (n, count) in counts {
                candidates.push((std::cmp::Reverse(count), n, *group));
            }
        }
        candidates.sort();
        let mut taken = BTreeSet::new();
        for (_, n, group) in candidates {
            if !marks.contains_key(group) && !taken.contains(&n) {
                taken.insert(n);
                marks.insert(group, format!("{}{n}", series.prefix));
            }
        }
        let mut next = taken
            .last()
            .map_or(series.start, |n| (n + 1).max(series.start));
        for group in members {
            if !marks.contains_key(group) {
                marks.insert(group, format!("{}{next}", series.prefix));
                next += 1;
            }
        }
    }
    groups
        .iter()
        .flat_map(|(group, ids)| ids.iter().map(move |id| (*id, group)))
        .map(|(id, group)| (id, marks[group].clone()))
        .collect()
}

/// Shipping mark prefix of an assembly by its main part, as on КМД
/// drawings: «К» columns, «Б» beams, «М» other assemblies.
pub fn assembly_prefix(project: &Project, assembly: ObjectId) -> &'static str {
    let main = project
        .assemblies
        .get(&assembly)
        .and_then(|a| project.elements.get(&a.main_part));
    match main.map(|e| &e.shape) {
        Some(ElementShape::Member {
            member_kind: MemberKind::Column,
            ..
        }) => "К",
        Some(ElementShape::Member {
            member_kind: MemberKind::Beam,
            ..
        }) => "Б",
        _ => "М",
    }
}

fn assembly_key(
    project: &Project,
    id: ObjectId,
    parts: &BTreeMap<ObjectId, String>,
    cache: &mut BTreeMap<ObjectId, String>,
) -> Result<String, ModelError> {
    if let Some(key) = cache.get(&id) {
        return Ok(key.clone());
    }
    let assembly = project
        .assemblies
        .get(&id)
        .ok_or(ModelError::MissingReference(id))?;
    let main = &project.elements[&assembly.main_part];
    let mut members = Vec::new();
    for member in &assembly.members {
        let part = &project.elements[member];
        members.push(serialized(&json!({"key":parts[member],"frame":frame_key(&relative_frame(main.shape.frame(),part.shape.frame()))}))?);
    }
    members.sort();
    let mut children = Vec::new();
    for child in &assembly.children {
        let child_key = &cache[child];
        let child_main = &project.elements[&project.assemblies[child].main_part];
        children.push(serialized(&json!({"key":child_key,"frame":frame_key(&relative_frame(main.shape.frame(),child_main.shape.frame()))}))?);
    }
    children.sort();
    let mut fasteners = Vec::new();
    for detail in project.details.values() {
        let feature = match &detail.kind {
            DetailKind::BoltGroup {
                parts: joined,
                frame,
                diameter,
                clearance,
                steps_x,
                steps_y,
                standard,
                grade,
                ..
            } if joined.is_subset(&assembly.members) => {
                let mut keys: Vec<_> = joined.iter().map(|p| &parts[p]).collect();
                keys.sort();
                Some(
                    json!({"kind":"bolts","parts":keys,"frame":frame_key(&relative_frame(main.shape.frame(),frame)),"diameter":dimensions(diameter),"clearance":dimensions(clearance),"steps_x":dimensions(steps_x),"steps_y":dimensions(steps_y),"standard":standard,"grade":grade}),
                )
            }
            DetailKind::Weld {
                parts: joined,
                weld_type,
                size,
                path,
                shop,
                sides,
                around,
                process,
            } if joined.is_subset(&assembly.members) => {
                let mut keys: Vec<_> = joined.iter().map(|p| &parts[p]).collect();
                keys.sort();
                let mut weld = json!({"kind":"weld","parts":keys,"type":weld_type,"size":dimensions(size),"path":dimensions(path.iter().map(|p|local_point(main.shape.frame(),*p)).collect::<Vec<_>>()),"shop":shop});
                // Only non-default extras, so existing assembly keys stay the same.
                if *sides != open_bim_model::WeldSides::One {
                    weld["sides"] = json!(sides);
                }
                if *around {
                    weld["around"] = json!(true);
                }
                if *process != open_bim_model::WeldProcess::Manual {
                    weld["process"] = json!(process);
                }
                Some(weld)
            }
            _ => None,
        };
        if let Some(feature) = feature {
            fasteners.push(serialized(&feature)?);
        }
    }
    fasteners.sort();
    let key = serialized(
        &json!({"schema":1,"main":parts[&assembly.main_part],"members":members,"children":children,"fasteners":fasteners,"properties":assembly.properties}),
    )?;
    cache.insert(id, key.clone());
    Ok(key)
}

/// Recomputable report; numbering does not mutate source geometry or user properties.
pub fn number(project: &Project) -> Result<Numbering, ModelError> {
    number_with(project, &project.numbering)
}

/// Numbering from scratch («Перенумеровать всё»): stored marks are ignored.
pub fn renumber_all(project: &Project) -> Result<Numbering, ModelError> {
    number_with(project, &BTreeMap::new())
}

/// Marks of a numbering result, as stored by «Нумеровать».
pub fn stored_marks(numbering: &Numbering) -> BTreeMap<ObjectId, String> {
    numbering
        .parts
        .iter()
        .chain(&numbering.assemblies)
        .map(|(id, mark)| (*id, mark.clone()))
        .collect()
}

/// Objects whose current mark differs from the stored one or that have
/// none: the numbering is out of date for them. Empty when never numbered.
pub fn outdated(project: &Project, numbering: &Numbering) -> BTreeSet<ObjectId> {
    if project.numbering.is_empty() {
        return BTreeSet::new();
    }
    numbering
        .parts
        .iter()
        .chain(&numbering.assemblies)
        .filter(|(id, mark)| project.numbering.get(id) != Some(*mark))
        .map(|(id, _)| *id)
        .collect()
}

fn number_with(
    project: &Project,
    old: &BTreeMap<ObjectId, String>,
) -> Result<Numbering, ModelError> {
    project.validate()?;
    let part_keys: BTreeMap<ObjectId, String> = project
        .elements
        .keys()
        .map(|id| Ok((*id, part_key(project, *id)?)))
        .collect::<Result<_, ModelError>>()?;
    let mut assembly_keys = BTreeMap::new();
    for root in project.assemblies.keys() {
        let mut stack = vec![(*root, false)];
        while let Some((id, exit)) = stack.pop() {
            if assembly_keys.contains_key(&id) {
                continue;
            }
            if exit {
                assembly_key(project, id, &part_keys, &mut assembly_keys)?;
            } else {
                stack.push((id, true));
                stack.extend(
                    project.assemblies[&id]
                        .children
                        .iter()
                        .map(|child| (*child, false)),
                );
            }
        }
    }
    Ok(Numbering {
        // Part positions are plain numbers (поз. 1, 2…), shipping marks
        // carry the kind of the main part (К1, Б1…).
        parts: positions(&part_keys, |id| part_series(project, id), old),
        assemblies: positions(&assembly_keys, |id| assembly_series(project, id), old),
        part_keys,
        assembly_keys,
    })
}
