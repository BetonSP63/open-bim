//! «Вставить»: the selection is kept in a clipboard (a snapshot of the model
//! and the selected objects) and pasted at a picked point, with the cuts,
//! bolts and welds between the copied parts. The clipboard outlives later
//! edits and deletions of the originals.
use crate::tools::{Edit, edit_commands};
use open_bim_commands::Command;
use open_bim_model::management::{LOCK_PROPERTY, SEQUENCE_PREFIX};
use open_bim_model::{ObjectId, Point3, Project};
use open_bim_renderer::DVec3;
use std::collections::BTreeSet;

#[derive(Clone, Debug)]
pub struct Clipboard {
    snapshot: Project,
    ids: BTreeSet<ObjectId>,
    /// The point that lands on the picked point.
    pub base: DVec3,
}

impl Clipboard {
    /// Parts and grid lines of the selection; `None` when there are none.
    pub fn copy(project: &Project, selected: &BTreeSet<ObjectId>) -> Option<Self> {
        let ids: BTreeSet<ObjectId> = selected
            .iter()
            .copied()
            .filter(|id| project.elements.contains_key(id) || project.axes.contains_key(id))
            .collect();
        let base = ids
            .iter()
            .find_map(|id| {
                open_bim_renderer::overlay::handle_points(project, *id)
                    .first()
                    .copied()
            })?
            .1;
        Some(Self {
            snapshot: project.clone(),
            ids,
            base,
        })
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// Commands creating the copies with the base moved to `at`. New ids
    /// follow the current model; IFC GUIDs, locks and sequence numbers stay
    /// with the originals.
    pub fn paste(&self, project: &Project, at: DVec3) -> Result<Vec<Command>, String> {
        let mut snapshot = self.snapshot.clone();
        snapshot.next_id = snapshot.next_id.max(project.next_id);
        let delta = at - self.base;
        let mut commands =
            edit_commands(&snapshot, &self.ids, &Edit::Copy(Point3(delta.to_array())))?;
        for command in &mut commands {
            if let Command::Create { element } | Command::CreateSwept { element, .. } = command {
                element.properties.retain(|k, _| {
                    k != open_bim_ifc::GUID_PROPERTY
                        && k != LOCK_PROPERTY
                        && !k.starts_with(SEQUENCE_PREFIX)
                });
            }
        }
        // Materials and profiles deleted since the copy come back from the
        // snapshot.
        let mut restore = Vec::new();
        let mut restored = BTreeSet::new();
        for command in &commands {
            let element = match command {
                Command::Create { element } | Command::CreateSwept { element, .. } => element,
                _ => continue,
            };
            let profile = match &element.shape {
                open_bim_model::ElementShape::Member { profile, .. } => Some(*profile),
                _ => None,
            };
            for id in [element.material].into_iter().chain(profile) {
                if project.contains_id(id) || !restored.insert(id) {
                    continue;
                }
                if let Some(material) = self.snapshot.materials.get(&id) {
                    restore.push(Command::CreateMaterial {
                        material: material.clone(),
                    });
                } else if let Some(profile) = self.snapshot.profiles.get(&id) {
                    restore.push(Command::CreateProfile {
                        profile: profile.clone(),
                    });
                } else {
                    return Err("В модели нет материала или профиля скопированной детали".into());
                }
            }
        }
        restore.extend(commands);
        let commands = restore;
        Ok(commands)
    }
}
