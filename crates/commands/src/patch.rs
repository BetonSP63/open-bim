use open_bim_model::{Assembly, Element, GridAxis, Level, Material, ObjectId, Profile, Project};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
struct Delta<T> {
    before: Option<T>,
    after: Option<T>,
}

type MapDelta<T> = BTreeMap<ObjectId, Delta<T>>;

fn differences<T: Clone + PartialEq>(
    before: &BTreeMap<ObjectId, T>,
    after: &BTreeMap<ObjectId, T>,
) -> MapDelta<T> {
    before
        .keys()
        .chain(after.keys())
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter_map(|id| {
            let old = before.get(&id);
            let new = after.get(&id);
            (old != new).then(|| {
                (
                    id,
                    Delta {
                        before: old.cloned(),
                        after: new.cloned(),
                    },
                )
            })
        })
        .collect()
}

fn apply<T: Clone>(map: &mut BTreeMap<ObjectId, T>, changes: &MapDelta<T>, forward: bool) {
    for (id, change) in changes {
        match if forward {
            &change.after
        } else {
            &change.before
        } {
            Some(value) => {
                map.insert(*id, value.clone());
            }
            None => {
                map.remove(id);
            }
        }
    }
}

/// History holds only changed records, not a complete project per edit.
#[derive(Clone, Debug)]
pub(crate) struct Patch {
    elements: MapDelta<Element>,
    assemblies: MapDelta<Assembly>,
    materials: MapDelta<Material>,
    profiles: MapDelta<Profile>,
    axes: MapDelta<GridAxis>,
    levels: MapDelta<Level>,
    details: MapDelta<open_bim_model::Detail>,
    before_meta: (u64, u64, String),
    after_meta: (u64, u64, String),
    pub touched: BTreeSet<ObjectId>,
    pub label: String,
}

impl Patch {
    pub fn between(before: &Project, after: &Project, label: String) -> Self {
        let elements = differences(&before.elements, &after.elements);
        let assemblies = differences(&before.assemblies, &after.assemblies);
        let materials = differences(&before.materials, &after.materials);
        let profiles = differences(&before.profiles, &after.profiles);
        let axes = differences(&before.axes, &after.axes);
        let levels = differences(&before.levels, &after.levels);
        let details = differences(&before.details, &after.details);
        let mut touched: BTreeSet<_> = elements
            .keys()
            .chain(assemblies.keys())
            .chain(materials.keys())
            .chain(profiles.keys())
            .chain(axes.keys())
            .chain(levels.keys())
            .chain(details.keys())
            .copied()
            .collect();
        dependencies(before, &mut touched);
        dependencies(after, &mut touched);
        Self {
            elements,
            assemblies,
            materials,
            profiles,
            axes,
            levels,
            details,
            before_meta: (before.revision, before.next_id, before.name.clone()),
            after_meta: (after.revision, after.next_id, after.name.clone()),
            touched,
            label,
        }
    }

    pub fn apply(&self, project: &mut Project, forward: bool) {
        apply(&mut project.elements, &self.elements, forward);
        apply(&mut project.assemblies, &self.assemblies, forward);
        apply(&mut project.materials, &self.materials, forward);
        apply(&mut project.profiles, &self.profiles, forward);
        apply(&mut project.axes, &self.axes, forward);
        apply(&mut project.levels, &self.levels, forward);
        apply(&mut project.details, &self.details, forward);
        let metadata = if forward {
            &self.after_meta
        } else {
            &self.before_meta
        };
        project.revision = metadata.0;
        project.next_id = metadata.1;
        project.name = metadata.2.clone();
    }
}

fn dependencies(project: &Project, touched: &mut BTreeSet<ObjectId>) {
    for element in project.elements.values() {
        let profile_changed = match &element.shape {
            open_bim_model::ElementShape::Member { profile, .. } => touched.contains(profile),
            _ => false,
        };
        if touched.contains(&element.material) || profile_changed {
            touched.insert(element.id);
        }
    }
    loop {
        let count = touched.len();
        for detail in project.details.values() {
            if touched.contains(&detail.id)
                && let Some(part) = detail.kind.part()
            {
                touched.insert(part);
            }
            if !detail.kind.references().is_disjoint(touched) {
                touched.insert(detail.id);
            }
        }
        for assembly in project.assemblies.values() {
            if assembly
                .members
                .iter()
                .chain(&assembly.children)
                .any(|id| touched.contains(id))
            {
                touched.insert(assembly.id);
            }
        }
        if count == touched.len() {
            break;
        }
    }
}
