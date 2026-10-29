use crate::end_plate::{bounds, contact_material_depth, local_frame};
use crate::{child_id, instance_changed, retain_children};
use open_bim_model::{
    ConnectionInstance, Detail, DetailKind, Element, ElementShape, Frame, LengthMm, MemberKind,
    ModelError, ObjectId, Point3, Profile, ProfileShape, Project, TemplateKind,
};
use std::collections::BTreeSet;

#[derive(Clone, Debug)]
pub struct ClipAngleGeometry {
    pub corner: Frame,
    pub material: ObjectId,
    pub primary_bolts: Frame,
    pub support_bolts: Frame,
    pub primary_depth: f64,
    pub support_depth: f64,
    pub support: ObjectId,
}

pub fn clip_angle_geometry(
    project: &Project,
    instance: &ConnectionInstance,
) -> Result<ClipAngleGeometry, ModelError> {
    instance.template.validate()?;
    let TemplateKind::ClipAngle { leg, height, pitch } = instance.template.kind else {
        return Err(ModelError::InvalidValue(
            "Ожидается уголковое соединение".into(),
        ));
    };
    let support = instance
        .secondary
        .ok_or_else(|| ModelError::InvalidValue("Уголковому соединению нужна опора".into()))?;
    if support == instance.primary {
        return Err(ModelError::InvalidValue(
            "Родители узла должны различаться".into(),
        ));
    }
    let source = project
        .elements
        .get(&instance.primary)
        .ok_or(ModelError::MissingReference(instance.primary))?;
    let ElementShape::Member {
        profile,
        length,
        frame,
        ..
    } = &source.shape
    else {
        return Err(ModelError::InvalidValue(
            "Первый родитель уголка должен иметь сечение".into(),
        ));
    };
    frame.validate()?;
    let section = &project
        .profiles
        .get(profile)
        .ok_or(ModelError::MissingReference(*profile))?
        .shape;
    let (face, web, clear_height) = match *section {
        ProfileShape::ISection {
            height,
            web,
            flange,
            ..
        } => (web / 2., web, height - 2. * flange),
        ProfileShape::Channel {
            width,
            height,
            web,
            flange,
        } => (-width / 2. + web, web, height - 2. * flange),
        ProfileShape::Angle {
            width,
            height,
            thickness,
        } => (-width / 2. + thickness, thickness, height - 2. * thickness),
        ProfileShape::Rectangle { width, height } => (width / 2., width, height),
        ProfileShape::RectangularHollow {
            width,
            height,
            thickness,
        } => (width / 2., thickness, height - 2. * thickness),
        // Welded and contour sections: the centre plate as an I-section web.
        ProfileShape::Polygon { .. } | ProfileShape::Welded { .. } => {
            let [height, _, web, flange] = section.sizes();
            (web / 2., web, height - 2. * flange)
        }
        ProfileShape::CircularHollow { .. } => {
            return Err(ModelError::InvalidValue(
                "Для уголкового узла нужна плоская стенка балки".into(),
            ));
        }
    };
    if height > clear_height || leg > length.value() {
        return Err(ModelError::InvalidValue(
            "Уголок не помещается на стенке или длине балки".into(),
        ));
    }
    let t = instance.template.thickness;
    let edge = instance.template.edge_distance;
    let radius = (instance.template.bolt_diameter + instance.template.clearance) / 2.;
    if leg - edge - radius <= t + 0.001 {
        return Err(ModelError::InvalidValue(
            "Отверстие уголка пересекает внутренний угол".into(),
        ));
    }
    let d = if instance.at_end { -1. } else { 1. };
    let corner = Frame {
        origin: frame.world(Point3([
            face,
            -d * height / 2.,
            if instance.at_end { length.value() } else { 0. },
        ])),
        axes: [
            frame.axes[2].map(|v| d * v),
            frame.axes[0],
            frame.axes[1].map(|v| d * v),
        ],
    };
    corner.validate()?;
    let row = (height - pitch) / 2.;
    let primary_bolts = Frame {
        origin: corner.world(Point3([leg - edge, t + 1., row])),
        axes: [corner.axes[0], corner.axes[2], corner.axes[1].map(|v| -v)],
    };
    let support_bolts = Frame {
        origin: corner.world(Point3([t + 1., leg - edge, row])),
        axes: [
            corner.axes[1].map(|v| -v),
            corner.axes[2],
            corner.axes[0].map(|v| -v),
        ],
    };
    primary_bolts.validate()?;
    support_bolts.validate()?;
    let host = project
        .elements
        .get(&support)
        .ok_or(ModelError::MissingReference(support))?;
    let hf = *host.shape.frame();
    hf.validate()?;
    let extent = bounds(project, &host.shape)?;
    let direction = hf
        .axes
        .map(|a| (0..3).map(|i| a[i] * support_bolts.axes[2][i]).sum::<f64>());
    let axis = direction
        .iter()
        .position(|v| v.abs() > 1. - 1e-9)
        .ok_or_else(|| ModelError::InvalidValue("Уголок требует плоскую грань опоры".into()))?;
    if direction
        .iter()
        .enumerate()
        .any(|(i, v)| i != axis && v.abs() > 1e-9)
    {
        return Err(ModelError::InvalidValue(
            "Полка уголка не параллельна опоре".into(),
        ));
    }
    let contact = local_frame(corner, hf).origin.0[axis];
    let expected = extent[axis][if direction[axis] > 0. { 0 } else { 1 }];
    if (contact - expected).abs() > 0.001 {
        return Err(ModelError::InvalidValue(
            "Полка уголка не прилегает к опоре".into(),
        ));
    }
    if axis != 2
        && let ElementShape::Member { profile, .. } = &host.shape
        && matches!(
            project.profiles[profile].shape,
            ProfileShape::CircularHollow { .. }
        )
    {
        return Err(ModelError::InvalidValue(
            "У круглой опоры нет плоской боковой грани".into(),
        ));
    }
    if axis != 2 {
        let (outline, length) = match &host.shape {
            ElementShape::Plate {
                contour, thickness, ..
            } => (contour.clone(), thickness.value()),
            ElementShape::Member {
                profile, length, ..
            } => {
                let section = &project.profiles[profile].shape;
                let outline = match *section {
                    ProfileShape::RectangularHollow { width, height, .. } => vec![
                        [-width / 2., -height / 2.],
                        [width / 2., -height / 2.],
                        [width / 2., height / 2.],
                        [-width / 2., height / 2.],
                    ],
                    _ => section.solid_contour()?,
                };
                (outline, length.value())
            }
        };
        let other = 1 - axis;
        for row in [0., pitch] {
            let p = local_frame(
                Frame {
                    origin: support_bolts.world(Point3([0., row, 0.])),
                    ..support_bolts
                },
                hf,
            )
            .origin
            .0;
            let covered = (0..outline.len()).any(|i| {
                let a = outline[i];
                let b = outline[(i + 1) % outline.len()];
                (a[axis] - expected).abs() < 0.001
                    && (b[axis] - expected).abs() < 0.001
                    && p[other] - radius > a[other].min(b[other]) + 0.001
                    && p[other] + radius < a[other].max(b[other]) - 0.001
            });
            if !covered || p[2] - radius <= 0.001 || p[2] + radius >= length - 0.001 {
                return Err(ModelError::InvalidValue(
                    "Отверстие уголка выходит за плоскую грань опоры".into(),
                ));
            }
        }
    }
    Ok(ClipAngleGeometry {
        corner,
        material: source.material,
        primary_bolts,
        support_bolts,
        primary_depth: t + web + 2.,
        support_depth: t
            + contact_material_depth(project, &host.shape, axis, direction[axis])?
            + 2.,
        support,
    })
}

pub fn rebuild_clip_angles(project: &mut Project, previous: &Project) -> Result<(), ModelError> {
    let instances: Vec<_> = project
        .connections
        .values()
        .filter(|c| matches!(c.template.kind, TemplateKind::ClipAngle { .. }))
        .cloned()
        .collect();
    for mut instance in instances {
        if !instance_changed(project, previous, &instance) {
            continue;
        }
        let recipe = match clip_angle_geometry(project, &instance) {
            Ok(recipe) => recipe,
            Err(error) => {
                crate::keep_failed(project, previous, instance, error)?;
                continue;
            }
        };
        retain_children(
            project,
            &mut instance,
            &[
                "angle",
                "angle-profile",
                "angle-hole-0",
                "angle-hole-1",
                "angle-hole-2",
                "angle-hole-3",
                "primary-hole-0",
                "primary-hole-1",
                "support-hole-0",
                "support-hole-1",
                "primary-bolts",
                "support-bolts",
            ],
        );
        let angle = child_id(project, &mut instance, "angle")?;
        let profile = child_id(project, &mut instance, "angle-profile")?;
        let mut holes = Vec::new();
        for role in [
            "angle-hole-0",
            "angle-hole-1",
            "angle-hole-2",
            "angle-hole-3",
            "primary-hole-0",
            "primary-hole-1",
            "support-hole-0",
            "support-hole-1",
        ] {
            holes.push(child_id(project, &mut instance, role)?);
        }
        let primary_group = child_id(project, &mut instance, "primary-bolts")?;
        let support_group = child_id(project, &mut instance, "support-bolts")?;
        let template = &instance.template;
        let TemplateKind::ClipAngle { leg, height, pitch } = template.kind else {
            unreachable!()
        };
        project.profiles.insert(
            profile,
            Profile {
                id: profile,
                name: format!("L {leg}×{leg}×{}", template.thickness),
                shape: ProfileShape::Angle {
                    height: leg,
                    width: leg,
                    thickness: template.thickness,
                },
                source: "Open BIM connection template v1".into(),
                standard: None,
                area_mm2: None,
            },
        );
        let shape = ElementShape::Member {
            member_kind: MemberKind::Beam,
            profile,
            length: LengthMm::new(height).map_err(|e| ModelError::InvalidValue(e.to_string()))?,
            frame: Frame {
                origin: recipe.corner.world(Point3([leg / 2., leg / 2., 0.])),
                ..recipe.corner
            },
            position: Default::default(),
        };
        project.elements.insert(
            angle,
            Element {
                id: angle,
                name: format!("{} — уголок", template.name),
                material: recipe.material,
                shape,
                properties: project
                    .elements
                    .get(&angle)
                    .map(|e| e.properties.clone())
                    .unwrap_or_default(),
            },
        );
        for (group_index, (frame, depth, target, group)) in [
            (
                recipe.primary_bolts,
                recipe.primary_depth,
                instance.primary,
                primary_group,
            ),
            (
                recipe.support_bolts,
                recipe.support_depth,
                recipe.support,
                support_group,
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let mut associated = BTreeSet::new();
            for (part_index, part) in [angle, target].into_iter().enumerate() {
                for row in 0..2 {
                    let index = if part_index == 0 {
                        group_index * 2 + row
                    } else {
                        4 + group_index * 2 + row
                    };
                    let id = holes[index];
                    let local = local_frame(
                        Frame {
                            origin: frame.world(Point3([0., pitch * row as f64, 0.])),
                            ..frame
                        },
                        *project.elements[&part].shape.frame(),
                    );
                    project.details.insert(
                        id,
                        Detail {
                            id,
                            kind: DetailKind::Hole {
                                part,
                                frame: local,
                                diameter: template.bolt_diameter + template.clearance,
                                depth,
                            },
                        },
                    );
                    associated.insert(id);
                }
            }
            project.details.insert(
                group,
                Detail {
                    id: group,
                    kind: DetailKind::BoltGroup {
                        parts: BTreeSet::from([angle, target]),
                        holes: associated,
                        frame,
                        diameter: template.bolt_diameter,
                        clearance: template.clearance,
                        steps_x: vec![],
                        steps_y: vec![pitch],
                        standard: template.bolt_standard.clone(),
                        grade: template.bolt_grade.clone(),
                        points: Vec::new(),
                    },
                },
            );
        }
        for part in [angle, instance.primary, recipe.support] {
            project.element_body(part).map_err(|e| {
                ModelError::InvalidValue(format!(
                    "Крепёж уголкового узла не размещается в детали {part}: {e}"
                ))
            })?;
        }
        project.connections.insert(instance.id, instance);
    }
    Ok(())
}
