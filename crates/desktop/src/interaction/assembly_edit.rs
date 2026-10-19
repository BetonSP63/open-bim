//! Assembly commands like Tekla «Сборка»: create an assembly from the
//! selected parts, add parts, remove parts, set the main part, explode;
//! a shop weld joins its two parts into one assembly.
use open_bim_commands::Command;
use open_bim_model::{Assembly, ElementShape, ObjectId, Project};
use std::collections::{BTreeMap, BTreeSet};

/// Assembly a part belongs to.
pub fn assembly_of(project: &Project, part: ObjectId) -> Option<&Assembly> {
    project
        .assemblies
        .values()
        .find(|a| a.members.contains(&part))
}

fn parts_only(project: &Project, ids: &BTreeSet<ObjectId>) -> Result<Vec<ObjectId>, String> {
    let parts: Vec<ObjectId> = ids
        .iter()
        .copied()
        .filter(|id| project.elements.contains_key(id))
        .collect();
    if parts.is_empty() {
        return Err("Выберите детали".into());
    }
    Ok(parts)
}

/// The heaviest member (else the heaviest part) is the main part, as in Tekla.
fn heaviest(project: &Project, parts: &[ObjectId]) -> ObjectId {
    *parts
        .iter()
        .max_by(|a, b| {
            let w = |id: &ObjectId| {
                (
                    matches!(project.elements[id].shape, ElementShape::Member { .. }),
                    project.element_mass_kg(*id).unwrap_or(0.),
                )
            };
            let (ma, wa) = w(a);
            let (mb, wb) = w(b);
            ma.cmp(&mb).then(wa.total_cmp(&wb)).then(b.cmp(a))
        })
        .expect("non-empty")
}

/// Commands that take `parts` out of their current assemblies (an assembly
/// left with one part or without its main part is deleted).
fn detach(project: &Project, parts: &BTreeSet<ObjectId>, out: &mut Vec<Command>) {
    for a in project.assemblies.values() {
        if a.members.is_disjoint(parts) {
            continue;
        }
        let mut kept = a.clone();
        kept.members.retain(|m| !parts.contains(m));
        if kept.members.len() < 2 || !kept.members.contains(&kept.main_part) {
            out.push(Command::RemoveAssembly { id: a.id });
        } else {
            out.push(Command::ReplaceAssembly { assembly: kept });
        }
    }
}

/// «Создать сборку»: a new assembly of the selected parts.
pub fn create_from(
    project: &Project,
    selected: &BTreeSet<ObjectId>,
) -> Result<Vec<Command>, String> {
    let parts = parts_only(project, selected)?;
    let set: BTreeSet<ObjectId> = parts.iter().copied().collect();
    let mut out = Vec::new();
    detach(project, &set, &mut out);
    let main = heaviest(project, &parts);
    out.push(Command::CreateAssembly {
        assembly: Assembly {
            id: ObjectId(project.next_id),
            name: project.elements[&main].name.clone(),
            main_part: main,
            members: set,
            children: BTreeSet::new(),
            properties: BTreeMap::new(),
        },
    });
    Ok(out)
}

/// «Добавить в сборку»: the selected parts join the assembly of `target`
/// (a free target part becomes the main part of a new assembly).
pub fn add_to(
    project: &Project,
    selected: &BTreeSet<ObjectId>,
    target: ObjectId,
) -> Result<Vec<Command>, String> {
    let mut parts: BTreeSet<ObjectId> = parts_only(project, selected)?.into_iter().collect();
    parts.remove(&target);
    if parts.is_empty() {
        return Err("Выберите добавляемые детали, затем укажите деталь сборки".into());
    }
    let mut out = Vec::new();
    match assembly_of(project, target) {
        Some(a) => {
            let others: BTreeSet<ObjectId> = parts.difference(&a.members).copied().collect();
            detach(project, &others, &mut out);
            let mut grown = a.clone();
            grown.members.extend(&others);
            out.push(Command::ReplaceAssembly { assembly: grown });
        }
        None => {
            detach(project, &parts, &mut out);
            let mut members = parts.clone();
            members.insert(target);
            out.push(Command::CreateAssembly {
                assembly: Assembly {
                    id: ObjectId(project.next_id),
                    name: project.elements[&target].name.clone(),
                    main_part: target,
                    members,
                    children: BTreeSet::new(),
                    properties: BTreeMap::new(),
                },
            });
        }
    }
    Ok(out)
}

/// «Удалить из сборки».
pub fn remove_from(
    project: &Project,
    selected: &BTreeSet<ObjectId>,
) -> Result<Vec<Command>, String> {
    let parts: BTreeSet<ObjectId> = parts_only(project, selected)?.into_iter().collect();
    let mut out = Vec::new();
    detach(project, &parts, &mut out);
    if out.is_empty() {
        return Err("Выбранные детали не входят в сборки".into());
    }
    Ok(out)
}

/// «Сделать главной деталью».
pub fn set_main(project: &Project, part: ObjectId) -> Result<Vec<Command>, String> {
    let a = assembly_of(project, part).ok_or("Деталь не входит в сборку")?;
    if a.main_part == part {
        return Err("Деталь уже главная".into());
    }
    let mut changed = a.clone();
    changed.main_part = part;
    changed.name = project.elements[&part].name.clone();
    Ok(vec![Command::ReplaceAssembly { assembly: changed }])
}

/// «Разбить сборку»: the assemblies of the selected parts are deleted.
pub fn explode(project: &Project, selected: &BTreeSet<ObjectId>) -> Result<Vec<Command>, String> {
    let ids: BTreeSet<ObjectId> = project
        .assemblies
        .values()
        .filter(|a| !a.members.is_disjoint(selected))
        .map(|a| a.id)
        .collect();
    if ids.is_empty() {
        return Err("Выбранные детали не входят в сборки".into());
    }
    Ok(ids
        .into_iter()
        .map(|id| Command::RemoveAssembly { id })
        .collect())
}

/// A shop weld joins its parts: `secondary` (and its assembly) goes into the
/// assembly of `main`, or both start a new one with `main` as main part.
pub fn join(project: &Project, main: ObjectId, secondary: ObjectId) -> Vec<Command> {
    let (a, b) = (assembly_of(project, main), assembly_of(project, secondary));
    if let (Some(a), Some(b)) = (a, b)
        && a.id == b.id
    {
        return Vec::new();
    }
    let incoming: BTreeSet<ObjectId> = match b {
        Some(b) => b.members.clone(),
        None => BTreeSet::from([secondary]),
    };
    let mut out = Vec::new();
    if let Some(b) = b {
        out.push(Command::RemoveAssembly { id: b.id });
    }
    match a {
        Some(a) => {
            let mut grown = a.clone();
            grown.members.extend(&incoming);
            out.push(Command::ReplaceAssembly { assembly: grown });
        }
        None => {
            let mut members = incoming;
            members.insert(main);
            out.push(Command::CreateAssembly {
                assembly: Assembly {
                    id: ObjectId(project.next_id),
                    name: project.elements[&main].name.clone(),
                    main_part: main,
                    members,
                    children: BTreeSet::new(),
                    properties: BTreeMap::new(),
                },
            });
        }
    }
    out
}
