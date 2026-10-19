//! Deterministic manufacturing equivalence, independent of model IDs and placement.
//! Coordinates use 1e-6 mm resolution; direction cosines use 1e-9 resolution.
use open_bim_model::{DetailKind, ElementShape, Frame, ModelError, ObjectId, Point3, Project};
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

fn positions(keys: &BTreeMap<ObjectId, String>, prefix: &str) -> BTreeMap<ObjectId, String> {
    let unique: BTreeSet<&String> = keys.values().collect();
    let by_key: BTreeMap<&String, String> = unique
        .into_iter()
        .enumerate()
        .map(|(i, k)| (k, format!("{prefix}{:04}", i + 1)))
        .collect();
    keys.iter()
        .map(|(id, k)| (*id, by_key[k].clone()))
        .collect()
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
        parts: positions(&part_keys, "P"),
        assemblies: positions(&assembly_keys, "A"),
        part_keys,
        assembly_keys,
    })
}
