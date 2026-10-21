//! Adding an imported model to the open project: new ids after the
//! project's own, materials and sections reused by name, a far-away model
//! moved next to the origin.
use open_bim_model::{
    Assembly, Detail, DetailKind, Element, ElementShape, Material, ObjectId, Profile, Project,
};
use std::collections::BTreeMap;

/// Objects to create in the target project, in this order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Merge {
    pub materials: Vec<Material>,
    pub profiles: Vec<Profile>,
    pub elements: Vec<Element>,
    pub details: Vec<Detail>,
    pub assemblies: Vec<Assembly>,
    /// Shift applied to every imported object, mm (zero unless the model
    /// lies more than 1 km away from the origin).
    pub shift: [f64; 3],
}

/// Farther than this from the origin a model is moved to it, mm.
const FAR_MM: f64 = 1e6;

/// Remap `imported` into ids free in `target`.
pub fn merge(target: &Project, imported: &Project) -> Merge {
    let mut next = target.next_id.max(1);
    let mut id = || {
        let v = ObjectId(next);
        next += 1;
        v
    };
    let mut out = Merge::default();
    let mut ids: BTreeMap<ObjectId, ObjectId> = BTreeMap::new();
    let used_materials: Vec<ObjectId> = imported.elements.values().map(|e| e.material).collect();
    for m in imported.materials.values() {
        if !used_materials.contains(&m.id) {
            continue;
        }
        let mapped = match target.materials.values().find(|t| t.name == m.name) {
            Some(t) => t.id,
            None => {
                let new = id();
                out.materials.push(Material {
                    id: new,
                    ..m.clone()
                });
                new
            }
        };
        ids.insert(m.id, mapped);
    }
    let used_profiles: Vec<ObjectId> = imported
        .elements
        .values()
        .filter_map(|e| match &e.shape {
            ElementShape::Member { profile, .. } => Some(*profile),
            ElementShape::Plate { .. } => None,
        })
        .collect();
    for p in imported.profiles.values() {
        if !used_profiles.contains(&p.id) {
            continue;
        }
        let mapped = match target
            .profiles
            .values()
            .find(|t| t.name == p.name && t.shape == p.shape)
        {
            Some(t) => t.id,
            None => {
                let new = id();
                out.profiles.push(Profile {
                    id: new,
                    ..p.clone()
                });
                new
            }
        };
        ids.insert(p.id, mapped);
    }
    // A model in survey coordinates is moved by whole metres next to the origin.
    let origins: Vec<[f64; 3]> = imported
        .elements
        .values()
        .map(|e| e.shape.frame().origin.0)
        .collect();
    let low = origins.iter().fold([f64::INFINITY; 3], |lo, p| {
        std::array::from_fn(|i| lo[i].min(p[i]))
    });
    if low.iter().all(|v| v.is_finite()) && low[0].hypot(low[1]) > FAR_MM {
        out.shift = [
            -(low[0] / 1000.).floor() * 1000.,
            -(low[1] / 1000.).floor() * 1000.,
            0.,
        ];
    }
    let shift = out.shift;
    for e in imported.elements.values() {
        let new = id();
        ids.insert(e.id, new);
        let mut e = e.clone();
        e.id = new;
        e.material = ids[&e.material];
        match &mut e.shape {
            ElementShape::Member { profile, frame, .. } => {
                *profile = ids[profile];
                frame.origin.0 = std::array::from_fn(|i| frame.origin.0[i] + shift[i]);
            }
            ElementShape::Plate { frame, .. } => {
                frame.origin.0 = std::array::from_fn(|i| frame.origin.0[i] + shift[i]);
            }
        }
        out.elements.push(e);
    }
    for d in imported.details.values() {
        let mut d = d.clone();
        match &mut d.kind {
            DetailKind::PlaneCut { part, .. }
            | DetailKind::BoxCut { part, .. }
            | DetailKind::PrismCut { part, .. }
            | DetailKind::Hole { part, .. } => match ids.get(part) {
                Some(p) => *part = *p,
                None => continue,
            },
            // The importer makes no bolts or welds.
            _ => continue,
        }
        d.id = id();
        out.details.push(d);
    }
    let assembly_ids: BTreeMap<ObjectId, ObjectId> =
        imported.assemblies.keys().map(|a| (*a, id())).collect();
    for a in imported.assemblies.values() {
        out.assemblies.push(Assembly {
            id: assembly_ids[&a.id],
            name: a.name.clone(),
            main_part: ids[&a.main_part],
            members: a.members.iter().map(|m| ids[m]).collect(),
            children: a.children.iter().map(|c| assembly_ids[c]).collect(),
            properties: a.properties.clone(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merged_model_validates_next_to_the_existing_one() {
        let target = open_bim_model::example_steel_frame().unwrap();
        let mut imported = open_bim_model::example_steel_frame().unwrap();
        for e in imported.elements.values_mut() {
            match &mut e.shape {
                ElementShape::Member { frame, .. } | ElementShape::Plate { frame, .. } => {
                    frame.origin.0[0] += 5.0e6;
                }
            }
        }
        let m = merge(&target, &imported);
        assert!(
            m.materials.is_empty() && m.profiles.is_empty(),
            "reused by name"
        );
        assert!(m.shift[0] < -4.9e6);
        let mut p = target.clone();
        for e in &m.elements {
            p.next_id = p.next_id.max(e.id.0 + 1);
            p.elements.insert(e.id, e.clone());
        }
        for d in &m.details {
            p.next_id = p.next_id.max(d.id.0 + 1);
            p.details.insert(d.id, d.clone());
        }
        p.validate().unwrap();
        assert_eq!(p.elements.len(), 2 * target.elements.len());
    }
}
