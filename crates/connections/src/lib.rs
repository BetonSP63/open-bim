//! Open steel connection templates and ID-free geometry recipes.
pub use open_bim_model::{ConnectionInstance, ConnectionTemplate, TemplateKind};
mod base_plate;
pub use base_plate::{BasePlateGeometry, base_plate_geometry, rebuild_base_plates};
mod end_plate;
pub use end_plate::{EndPlateGeometry, end_plate_geometry, rebuild_end_plates};
mod fin_plate;
pub use fin_plate::{FinPlateGeometry, fin_plate_geometry, fin_plate_roles, rebuild_fin_plates};
mod stiffeners;
pub use stiffeners::{STIFFENER_ROLES, StiffenerGeometry, rebuild_stiffeners, stiffener_geometry};
mod beam_splice;
pub use beam_splice::{
    BeamSpliceGeometry, MAX_SPLICE_GAP, SpliceBolts, beam_splice_geometry, rebuild_beam_splices,
};
mod clip_angle;
pub use clip_angle::{ClipAngleGeometry, clip_angle_geometry, rebuild_clip_angles};

pub(crate) fn child_id(
    project: &mut open_bim_model::Project,
    instance: &mut ConnectionInstance,
    role: &str,
) -> Result<open_bim_model::ObjectId, open_bim_model::ModelError> {
    if let Some(id) = instance.children.get(role) {
        if !project.elements.contains_key(id)
            && !project.details.contains_key(id)
            && !project.profiles.contains_key(id)
        {
            return Err(open_bim_model::ModelError::MissingReference(*id));
        }
        return Ok(*id);
    }
    let id = project.allocate_id()?;
    instance.children.insert(role.into(), id);
    Ok(id)
}

pub(crate) fn retain_children(
    project: &mut open_bim_model::Project,
    instance: &mut ConnectionInstance,
    roles: &[&str],
) {
    let retired: Vec<_> = instance
        .children
        .iter()
        .filter(|(role, _)| !roles.contains(&role.as_str()))
        .map(|(role, id)| (role.clone(), *id))
        .collect();
    for (role, id) in retired {
        instance.children.remove(&role);
        project.elements.remove(&id);
        project.details.remove(&id);
        project.profiles.remove(&id);
    }
}

pub(crate) fn instance_changed(
    project: &open_bim_model::Project,
    previous: &open_bim_model::Project,
    instance: &ConnectionInstance,
) -> bool {
    previous.connections.get(&instance.id) != Some(instance)
        || std::iter::once(instance.primary)
            .chain(instance.secondary)
            .any(|id| {
                let element = project.elements.get(&id);
                element != previous.elements.get(&id)
                    || match element.map(|e| &e.shape) {
                        Some(open_bim_model::ElementShape::Member { profile, .. }) => {
                            project.profiles.get(profile) != previous.profiles.get(profile)
                        }
                        _ => false,
                    }
            })
        || instance.children.values().any(|id| {
            project.elements.get(id) != previous.elements.get(id)
                || project.details.get(id) != previous.details.get(id)
                || project.profiles.get(id) != previous.profiles.get(id)
        })
}

/// Generated shapes are controlled by the template and parents. Refuse direct
/// geometry edits instead of reporting success and silently discarding them.
pub fn reject_generated_geometry_edits(
    project: &open_bim_model::Project,
    previous: &open_bim_model::Project,
) -> Result<(), open_bim_model::ModelError> {
    for instance in project.connections.values() {
        if previous.connections.get(&instance.id) != Some(instance)
            || std::iter::once(instance.primary)
                .chain(instance.secondary)
                .any(|id| project.elements.get(&id) != previous.elements.get(&id))
        {
            continue;
        }
        for id in instance.children.values() {
            if let (Some(old), Some(new)) = (previous.details.get(id), project.details.get(id))
                && old != new
            {
                return Err(open_bim_model::ModelError::InvalidValue(
                    "Крепёж управляется соединением; измените шаблон или родителя узла".into(),
                ));
            }
            if let (Some(old), Some(new)) = (previous.elements.get(id), project.elements.get(id))
                && old.shape != new.shape
            {
                return Err(open_bim_model::ModelError::InvalidValue(
                    "Геометрия детали управляется соединением; измените шаблон или родителя узла"
                        .into(),
                ));
            }
        }
    }
    Ok(())
}
