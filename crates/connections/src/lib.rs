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
mod haunch;
pub use haunch::{HaunchGeometry, haunch_geometry, rebuild_haunches};
mod gusset;
pub use gusset::{GussetGeometry, MAX_GUSSET_REACH, gusset_geometry, rebuild_gussets};
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

/// Refuse unless `point` lies on the flat face of `support` that faces back
/// along `outward` (the direction from the connected part to the support).
pub(crate) fn touch_support_face(
    project: &open_bim_model::Project,
    support: open_bim_model::ObjectId,
    point: open_bim_model::Point3,
    outward: [f64; 3],
    miss: &str,
) -> Result<(), open_bim_model::ModelError> {
    use open_bim_model::{ElementShape, Frame, ModelError, ProfileShape};
    let host = project
        .elements
        .get(&support)
        .ok_or(ModelError::MissingReference(support))?;
    let hf = *host.shape.frame();
    hf.validate()?;
    let extent = end_plate::bounds(project, &host.shape)?;
    let projected = hf
        .axes
        .map(|a| (0..3).map(|i| a[i] * outward[i]).sum::<f64>());
    let axis = projected
        .iter()
        .position(|v| v.abs() > 1. - 1e-9)
        .ok_or_else(|| {
            ModelError::InvalidValue("Пластина требует плоскую грань опоры поперёк балки".into())
        })?;
    if axis != 2
        && let ElementShape::Member { profile, .. } = &host.shape
        && matches!(
            project.profiles[profile].shape,
            ProfileShape::CircularHollow { .. }
        )
    {
        return Err(ModelError::InvalidValue(
            "У круглой опоры нет плоской грани для пластины".into(),
        ));
    }
    let contact = end_plate::local_frame(
        Frame {
            origin: point,
            ..hf
        },
        hf,
    )
    .origin
    .0[axis];
    let expected = extent[axis][if projected[axis] > 0. { 0 } else { 1 }];
    if (contact - expected).abs() > 0.001 {
        return Err(ModelError::InvalidValue(miss.into()));
    }
    Ok(())
}

/// Whether the connection's geometry can be built (parents, faces, sizes),
/// without generating its parts.
pub fn check_geometry(
    project: &open_bim_model::Project,
    instance: &ConnectionInstance,
) -> Result<(), open_bim_model::ModelError> {
    match instance.template.kind {
        TemplateKind::BasePlate { .. } => base_plate_geometry(project, instance).map(drop),
        TemplateKind::EndPlate { .. } => end_plate_geometry(project, instance).map(drop),
        TemplateKind::ClipAngle { .. } => clip_angle_geometry(project, instance).map(drop),
        TemplateKind::FinPlate { .. } => fin_plate_geometry(project, instance).map(drop),
        TemplateKind::ColumnStiffeners { .. } => stiffener_geometry(project, instance).map(drop),
        TemplateKind::BeamSplice { .. } => beam_splice_geometry(project, instance).map(drop),
        TemplateKind::Haunch { .. } => haunch_geometry(project, instance).map(drop),
        TemplateKind::Gusset { .. } => gusset_geometry(project, instance).map(drop),
    }
}
