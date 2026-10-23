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
    connections: MapDelta<open_bim_model::ConnectionInstance>,
    drawings: MapDelta<open_bim_model::Drawing>,
    construction: MapDelta<open_bim_model::Construction>,
    dimensions: MapDelta<open_bim_model::ModelDimension>,
    plugins: MapDelta<open_bim_model::PluginInstance>,
    /// Document attributes and changes before and after, when they differ.
    document: Option<Delta<(open_bim_model::DocumentInfo, Vec<open_bim_model::Change>)>>,
    /// Stored numbering marks before and after, when they differ.
    numbering: Option<Delta<BTreeMap<ObjectId, String>>>,
    /// Grid properties before and after, when they differ.
    grid: Option<Delta<open_bim_model::GridSettings>>,
    management: Option<Delta<open_bim_model::Management>>,
    analysis: Option<Delta<open_bim_model::Analysis>>,
    parametric: Option<Delta<open_bim_model::Parametric>>,
    concrete: Option<Delta<open_bim_model::Concrete>>,
    shapes: Option<Delta<open_bim_model::Shapes>>,
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
        let connections = differences(&before.connections, &after.connections);
        let drawings = differences(&before.drawings, &after.drawings);
        let construction = differences(&before.construction, &after.construction);
        let dimensions = differences(&before.dimensions, &after.dimensions);
        let plugins = differences(&before.plugins, &after.plugins);
        let document =
            (before.document != after.document || before.changes != after.changes).then(|| Delta {
                before: Some((before.document.clone(), before.changes.clone())),
                after: Some((after.document.clone(), after.changes.clone())),
            });
        let grid = (before.grid != after.grid).then(|| Delta {
            before: Some(before.grid.clone()),
            after: Some(after.grid.clone()),
        });
        let management = (before.management != after.management).then(|| Delta {
            before: Some(before.management.clone()),
            after: Some(after.management.clone()),
        });
        let analysis = (before.analysis != after.analysis).then(|| Delta {
            before: Some(before.analysis.clone()),
            after: Some(after.analysis.clone()),
        });
        let parametric = (before.parametric != after.parametric).then(|| Delta {
            before: Some(before.parametric.clone()),
            after: Some(after.parametric.clone()),
        });
        let concrete = (before.concrete != after.concrete).then(|| Delta {
            before: Some(before.concrete.clone()),
            after: Some(after.concrete.clone()),
        });
        let shapes = (before.shapes != after.shapes).then(|| Delta {
            before: Some(before.shapes.clone()),
            after: Some(after.shapes.clone()),
        });
        let numbering = (before.numbering != after.numbering).then(|| Delta {
            before: Some(before.numbering.clone()),
            after: Some(after.numbering.clone()),
        });
        let mut touched: BTreeSet<_> = elements
            .keys()
            .chain(assemblies.keys())
            .chain(materials.keys())
            .chain(profiles.keys())
            .chain(axes.keys())
            .chain(levels.keys())
            .chain(details.keys())
            .chain(connections.keys())
            .chain(drawings.keys())
            .chain(construction.keys())
            .chain(dimensions.keys())
            .chain(plugins.keys())
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
            connections,
            drawings,
            construction,
            dimensions,
            plugins,
            document,
            numbering,
            grid,
            management,
            analysis,
            parametric,
            concrete,
            shapes,
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
        apply(&mut project.connections, &self.connections, forward);
        apply(&mut project.drawings, &self.drawings, forward);
        apply(&mut project.construction, &self.construction, forward);
        apply(&mut project.dimensions, &self.dimensions, forward);
        apply(&mut project.plugins, &self.plugins, forward);
        if let Some(delta) = &self.document {
            let side = if forward { &delta.after } else { &delta.before };
            if let Some((document, changes)) = side {
                project.document = document.clone();
                project.changes = changes.clone();
            }
        }
        if let Some(delta) = &self.numbering {
            let side = if forward { &delta.after } else { &delta.before };
            if let Some(marks) = side {
                project.numbering = marks.clone();
            }
        }
        if let Some(delta) = &self.grid {
            let side = if forward { &delta.after } else { &delta.before };
            if let Some(grid) = side {
                project.grid = grid.clone();
            }
        }
        if let Some(delta) = &self.management {
            let side = if forward { &delta.after } else { &delta.before };
            if let Some(m) = side {
                project.management = m.clone();
            }
        }
        if let Some(delta) = &self.analysis {
            let side = if forward { &delta.after } else { &delta.before };
            if let Some(a) = side {
                project.analysis = a.clone();
            }
        }
        if let Some(delta) = &self.parametric {
            let side = if forward { &delta.after } else { &delta.before };
            if let Some(p) = side {
                project.parametric = p.clone();
            }
        }
        if let Some(delta) = &self.concrete {
            let side = if forward { &delta.after } else { &delta.before };
            if let Some(c) = side {
                project.concrete = c.clone();
            }
        }
        if let Some(delta) = &self.shapes {
            let side = if forward { &delta.after } else { &delta.before };
            if let Some(s) = side {
                project.shapes = s.clone();
            }
        }
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
        for connection in project.connections.values() {
            if touched.contains(&connection.id)
                || touched.contains(&connection.primary)
                || connection.secondary.is_some_and(|id| touched.contains(&id))
                || connection.children.values().any(|id| touched.contains(id))
            {
                touched.insert(connection.id);
                touched.extend(connection.children.values());
            }
        }
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
