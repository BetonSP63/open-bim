//! Steel release lists regenerated from the model: parts, assemblies and
//! fasteners. Positions come from `open-bim-numbering`; masses are net masses
//! of the exact bodies (holes and cuts removed).
pub mod csv;
pub mod gost_release;
pub mod gost_tables;
pub mod release;

use open_bim_commands::Command;
use open_bim_model::{Assembly, DetailKind, ElementShape, ObjectId, Project, WeldType};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

pub type Error = String;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PartRow {
    pub position: String,
    pub name: String,
    pub section: String,
    pub material: String,
    pub quantity: usize,
    pub length_mm: f64,
    pub unit_mass_kg: f64,
    pub total_mass_kg: f64,
    pub ids: Vec<ObjectId>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AssemblyRow {
    pub position: String,
    pub name: String,
    pub main_part: String,
    pub quantity: usize,
    pub parts_per_assembly: usize,
    pub unit_mass_kg: f64,
    pub total_mass_kg: f64,
    pub ids: Vec<ObjectId>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FastenerRow {
    pub kind: String,
    pub description: String,
    pub quantity: usize,
    /// Total weld length for welds, millimetres; 0 for bolts.
    pub length_mm: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Reports {
    pub revision: u64,
    pub parts: Vec<PartRow>,
    pub assemblies: Vec<AssemblyRow>,
    pub fasteners: Vec<FastenerRow>,
    pub total_part_mass_kg: f64,
    pub total_assembly_mass_kg: f64,
    pub unassembled: Vec<ObjectId>,
}

fn round(value: f64, digits: i32) -> f64 {
    let f = 10f64.powi(digits);
    (value * f).round() / f
}

/// Section label: catalogue profile name, or «—t×b» for plates (ГОСТ notation).
pub fn section(project: &Project, id: ObjectId) -> Result<(String, f64), Error> {
    let element = project
        .elements
        .get(&id)
        .ok_or_else(|| format!("Деталь #{} не найдена", id.0))?;
    Ok(match &element.shape {
        ElementShape::Member {
            profile, length, ..
        } => (
            project
                .profiles
                .get(profile)
                .map(|p| p.name.clone())
                .ok_or_else(|| format!("Профиль #{} не найден", profile.0))?,
            length.value(),
        ),
        ElementShape::Plate {
            contour, thickness, ..
        } => {
            let span = |i: usize| {
                let (lo, hi) = contour
                    .iter()
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                        (lo.min(p[i]), hi.max(p[i]))
                    });
                hi - lo
            };
            let (a, b) = (span(0), span(1));
            (
                format!("—{}×{}", fmt(thickness.value()), fmt(a.min(b))),
                a.max(b),
            )
        }
    })
}

pub fn fmt(value: f64) -> String {
    let r = round(value, 1);
    if (r - r.round()).abs() < 1e-9 {
        format!("{}", r.round() as i64)
    } else {
        format!("{r:.1}")
    }
}

pub fn generate(project: &Project) -> Result<Reports, Error> {
    project.validate().map_err(|e| e.to_string())?;
    let numbering = open_bim_numbering::number(project).map_err(|e| e.to_string())?;
    let mut by_position: BTreeMap<&String, Vec<ObjectId>> = BTreeMap::new();
    for (id, position) in &numbering.parts {
        by_position.entry(position).or_default().push(*id);
    }
    let mut parts = Vec::new();
    for (position, ids) in by_position {
        let first = ids[0];
        let element = &project.elements[&first];
        let (section, length) = section(project, first)?;
        let unit = project.element_mass_kg(first).map_err(|e| e.to_string())?;
        parts.push(PartRow {
            position: position.clone(),
            name: element.name.clone(),
            section,
            material: project
                .materials
                .get(&element.material)
                .map(|m| m.name.clone())
                .unwrap_or_default(),
            quantity: ids.len(),
            length_mm: round(length, 3),
            unit_mass_kg: round(unit, 3),
            total_mass_kg: round(unit * ids.len() as f64, 3),
            ids,
        });
    }
    let mut by_assembly: BTreeMap<&String, Vec<ObjectId>> = BTreeMap::new();
    for (id, position) in &numbering.assemblies {
        by_assembly.entry(position).or_default().push(*id);
    }
    let mut assemblies = Vec::new();
    for (position, ids) in by_assembly {
        let a = &project.assemblies[&ids[0]];
        let members = assembly_parts(project, a.id);
        let mut unit = 0.;
        for m in &members {
            unit += project.element_mass_kg(*m).map_err(|e| e.to_string())?;
        }
        assemblies.push(AssemblyRow {
            position: position.clone(),
            name: a.name.clone(),
            main_part: numbering
                .parts
                .get(&a.main_part)
                .cloned()
                .unwrap_or_default(),
            quantity: ids.len(),
            parts_per_assembly: members.len(),
            unit_mass_kg: round(unit, 3),
            total_mass_kg: round(unit * ids.len() as f64, 3),
            ids,
        });
    }
    let assembled: BTreeSet<ObjectId> = project
        .assemblies
        .values()
        .flat_map(|a| a.members.iter().copied())
        .collect();
    let unassembled = project
        .elements
        .keys()
        .copied()
        .filter(|id| !assembled.contains(id))
        .collect();
    let mut raw_parts = 0.;
    for id in project.elements.keys() {
        raw_parts += project.element_mass_kg(*id).map_err(|e| e.to_string())?;
    }
    let total_part_mass_kg = round(raw_parts, 3);
    // Top-level assemblies only, so nested children are not counted twice.
    let children: BTreeSet<ObjectId> = project
        .assemblies
        .values()
        .flat_map(|a| a.children.iter().copied())
        .collect();
    let mut raw_assemblies = 0.;
    for a in project
        .assemblies
        .values()
        .filter(|a| !children.contains(&a.id))
    {
        for m in assembly_parts(project, a.id) {
            raw_assemblies += project.element_mass_kg(m).map_err(|e| e.to_string())?;
        }
    }
    let total_assembly_mass_kg = round(raw_assemblies, 3);
    Ok(Reports {
        revision: project.revision,
        parts,
        assemblies,
        fasteners: fasteners(project),
        total_part_mass_kg,
        total_assembly_mass_kg,
        unassembled,
    })
}

/// Parts of an assembly including nested sub-assemblies.
pub fn assembly_parts(project: &Project, id: ObjectId) -> BTreeSet<ObjectId> {
    let mut result = BTreeSet::new();
    let mut stack = vec![id];
    let mut seen = BTreeSet::new();
    while let Some(a) = stack.pop() {
        if !seen.insert(a) {
            continue;
        }
        if let Some(assembly) = project.assemblies.get(&a) {
            result.extend(&assembly.members);
            stack.extend(&assembly.children);
        }
    }
    result
}

fn fasteners(project: &Project) -> Vec<FastenerRow> {
    // (kind, description): bolts with their nuts and washers, one each.
    let mut bolts: BTreeMap<(&str, String), usize> = BTreeMap::new();
    let mut welds: BTreeMap<String, (usize, f64)> = BTreeMap::new();
    for detail in project.details.values() {
        match &detail.kind {
            DetailKind::BoltGroup {
                diameter,
                standard,
                grade,
                steps_x,
                steps_y,
                points,
                ..
            } => {
                let count = open_bim_model::bolt_positions(steps_x, steps_y, points).len();
                let d = fmt(*diameter);
                let (nut, washer) = open_bim_model::bolt_set(standard);
                for key in [
                    ("Болт", format!("М{d} {standard} кл. {grade}")),
                    ("Гайка", format!("М{d} {nut}")),
                    ("Шайба", format!("{d} {washer}")),
                ] {
                    *bolts.entry(key).or_default() += count;
                }
            }
            DetailKind::Weld {
                weld_type,
                size,
                path,
                shop,
                sides,
                process,
                ..
            } => {
                let kind = match weld_type {
                    WeldType::Fillet => "угловой",
                    WeldType::Butt => "стыковой",
                    WeldType::ButtSquare => "стыковой без скоса",
                    WeldType::ButtX => "стыковой X",
                    WeldType::Lap => "нахлёсточный",
                };
                // Joint per the process standard (Т1, Т3, С2, …).
                let kind = format!(
                    "{kind} {} {}",
                    process.standard(),
                    weld_type.joint(*sides, *process)
                );
                let place = if *shop {
                    "заводской"
                } else {
                    "монтажный"
                };
                let entry = welds
                    .entry(format!("Шов {kind} k={} {place}", fmt(*size)))
                    .or_default();
                entry.0 += 1;
                entry.1 += path.windows(2).map(|p| p[0].distance(p[1])).sum::<f64>();
            }
            _ => {}
        }
    }
    bolts
        .into_iter()
        .map(|((kind, description), quantity)| FastenerRow {
            kind: kind.into(),
            description,
            quantity,
            length_mm: 0.,
        })
        .chain(
            welds
                .into_iter()
                .map(|(description, (quantity, length))| FastenerRow {
                    kind: "Сварка".into(),
                    description,
                    quantity,
                    length_mm: round(length, 1),
                }),
        )
        .collect()
}

/// Assemblies for parts not yet assembled: parts joined by shop welds form one
/// assembly whose main part is the heaviest member (or heaviest part).
pub fn auto_assemblies(project: &Project) -> Result<Vec<Command>, Error> {
    let assembled: BTreeSet<ObjectId> = project
        .assemblies
        .values()
        .flat_map(|a| a.members.iter().copied())
        .collect();
    let free: Vec<ObjectId> = project
        .elements
        .keys()
        .copied()
        .filter(|id| !assembled.contains(id))
        .collect();
    let mut parent: BTreeMap<ObjectId, ObjectId> = free.iter().map(|id| (*id, *id)).collect();
    fn root(parent: &mut BTreeMap<ObjectId, ObjectId>, mut id: ObjectId) -> ObjectId {
        while parent[&id] != id {
            let next = parent[&parent[&id]];
            parent.insert(id, next);
            id = next;
        }
        id
    }
    for detail in project.details.values() {
        if let DetailKind::Weld {
            parts, shop: true, ..
        } = &detail.kind
        {
            let joined: Vec<ObjectId> = parts
                .iter()
                .copied()
                .filter(|p| parent.contains_key(p))
                .collect();
            for pair in joined.windows(2) {
                let (a, b) = (root(&mut parent, pair[0]), root(&mut parent, pair[1]));
                if a != b {
                    parent.insert(a.max(b), a.min(b));
                }
            }
        }
    }
    let mut groups: BTreeMap<ObjectId, Vec<ObjectId>> = BTreeMap::new();
    for id in &free {
        let r = root(&mut parent, *id);
        groups.entry(r).or_default().push(*id);
    }
    let mut commands = Vec::new();
    for (next, members) in (project.next_id..).zip(groups.values()) {
        let weight = |id: &ObjectId| {
            let member = matches!(project.elements[id].shape, ElementShape::Member { .. });
            (member, project.element_mass_kg(*id).unwrap_or(0.))
        };
        let main = *members
            .iter()
            .max_by(|a, b| {
                let (ma, wa) = weight(a);
                let (mb, wb) = weight(b);
                ma.cmp(&mb).then(wa.total_cmp(&wb)).then(b.cmp(a))
            })
            .ok_or("Пустая группа")?;
        let element = &project.elements[&main];
        let id = ObjectId(next);
        commands.push(Command::CreateAssembly {
            assembly: Assembly {
                id,
                name: element.name.clone(),
                main_part: main,
                members: members.iter().copied().collect(),
                children: BTreeSet::new(),
                properties: BTreeMap::new(),
            },
        });
    }
    Ok(commands)
}
