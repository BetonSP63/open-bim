use crate::{BasePlateGeometry, base_plate_geometry};
use open_bim_model::{
    ConnectionInstance, Detail, DetailKind, Element, ElementShape, Frame, ModelError, ObjectId,
    Point3, ProfileShape, Project, TemplateKind, WeldType,
};
use std::collections::BTreeSet;

pub(crate) fn local_frame(world: Frame, parent: Frame) -> Frame {
    let dot = |a: [f64; 3], b: [f64; 3]| (0..3).map(|i| a[i] * b[i]).sum::<f64>();
    let delta = std::array::from_fn(|i| world.origin.0[i] - parent.origin.0[i]);
    Frame {
        origin: Point3(parent.axes.map(|a| dot(a, delta))),
        axes: world.axes.map(|a| parent.axes.map(|b| dot(a, b))),
    }
}

fn bounds(project: &Project, shape: &ElementShape) -> Result<[[f64; 2]; 3], ModelError> {
    let (contour, depth) = match shape {
        ElementShape::Plate {
            contour, thickness, ..
        } => (contour.clone(), thickness.value()),
        ElementShape::Member {
            profile, length, ..
        } => {
            let section = &project
                .profiles
                .get(profile)
                .ok_or(ModelError::MissingReference(*profile))?
                .shape;
            let contour = match *section {
                ProfileShape::CircularHollow { diameter, .. } => vec![
                    [-diameter / 2., -diameter / 2.],
                    [diameter / 2., diameter / 2.],
                ],
                ProfileShape::RectangularHollow { width, height, .. } => {
                    vec![[-width / 2., -height / 2.], [width / 2., height / 2.]]
                }
                _ => section.solid_contour()?,
            };
            (contour, length.value())
        }
    };
    Ok(std::array::from_fn(|i| {
        if i == 2 {
            [0., depth]
        } else {
            [
                contour.iter().map(|p| p[i]).fold(f64::INFINITY, f64::min),
                contour
                    .iter()
                    .map(|p| p[i])
                    .fold(f64::NEG_INFINITY, f64::max),
            ]
        }
    }))
}

#[derive(Clone, Debug)]
pub struct EndPlateGeometry {
    pub plate: BasePlateGeometry,
    pub support: ObjectId,
    pub hole_depth: f64,
}

pub fn end_plate_geometry(
    project: &Project,
    instance: &ConnectionInstance,
) -> Result<EndPlateGeometry, ModelError> {
    let TemplateKind::EndPlate { overhang } = instance.template.kind else {
        return Err(ModelError::InvalidValue(
            "Ожидается торцевая пластина".into(),
        ));
    };
    let support = instance
        .secondary
        .ok_or_else(|| ModelError::InvalidValue("Для торцевой пластины нужна опора".into()))?;
    if support == instance.primary {
        return Err(ModelError::InvalidValue(
            "Родители узла должны различаться".into(),
        ));
    }
    let host = project
        .elements
        .get(&support)
        .ok_or(ModelError::MissingReference(support))?;
    host.shape.frame().validate()?;
    let mut base = instance.clone();
    base.secondary = None;
    base.template.kind = TemplateKind::BasePlate { overhang };
    let mut plate = base_plate_geometry(project, &base)?;
    let pf = *plate.shape.frame();
    let hf = *host.shape.frame();
    let extent = bounds(project, &host.shape)?;
    let direction = if instance.at_end { 1.0 } else { -1.0 };
    let outward = pf.axes[2].map(|v| v * direction);
    let projected = hf
        .axes
        .map(|a| (0..3).map(|i| a[i] * outward[i]).sum::<f64>());
    let axis = projected
        .iter()
        .position(|v| v.abs() > 1.0 - 1e-9)
        .ok_or_else(|| {
            ModelError::InvalidValue(
                "Торцевая пластина требует плоскую грань опоры по направлению балки".into(),
            )
        })?;
    if projected
        .iter()
        .enumerate()
        .any(|(i, v)| i != axis && v.abs() > 1e-9)
    {
        return Err(ModelError::InvalidValue(
            "Пластина не параллельна грани опоры".into(),
        ));
    }
    let free_face = pf.world(Point3([
        0.,
        0.,
        if instance.at_end {
            instance.template.thickness
        } else {
            0.
        },
    ]));
    let contact = local_frame(
        Frame {
            origin: free_face,
            ..pf
        },
        hf,
    )
    .origin
    .0[axis];
    let expected = extent[axis][if projected[axis] > 0. { 0 } else { 1 }];
    if (contact - expected).abs() > 0.001 {
        return Err(ModelError::InvalidValue(
            "Наружная грань торцевой пластины не прилегает к опоре".into(),
        ));
    }
    if !instance.at_end {
        let ElementShape::Plate { contour, .. } = &plate.shape else {
            unreachable!()
        };
        let height = contour[2][1];
        plate.bolt_frame = Frame {
            origin: pf.world(Point3([
                instance.template.edge_distance,
                height - instance.template.edge_distance,
                instance.template.thickness + 1.,
            ])),
            axes: [pf.axes[0], pf.axes[1].map(|v| -v), pf.axes[2].map(|v| -v)],
        };
    }
    if axis != 2 {
        let (outline, length) = match &host.shape {
            ElementShape::Plate {
                contour, thickness, ..
            } => (contour.clone(), thickness.value()),
            ElementShape::Member {
                profile, length, ..
            } => {
                let section = &project
                    .profiles
                    .get(profile)
                    .ok_or(ModelError::MissingReference(*profile))?
                    .shape;
                let outline = match *section {
                    ProfileShape::CircularHollow { .. } => {
                        return Err(ModelError::InvalidValue(
                            "У круглой трубы нет плоской боковой грани для торцевой пластины"
                                .into(),
                        ));
                    }
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
        let radius = (instance.template.bolt_diameter + instance.template.clearance) / 2.;
        for x in [0., plate.steps_x[0]] {
            for y in [0., plate.steps_y[0]] {
                let p = local_frame(
                    Frame {
                        origin: plate.bolt_frame.world(Point3([x, y, 0.])),
                        ..plate.bolt_frame
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
                        "Отверстие крепежа выходит за плоскую грань опоры".into(),
                    ));
                }
            }
        }
    }
    Ok(EndPlateGeometry {
        plate,
        support,
        hole_depth: instance.template.thickness + extent[axis][1] - extent[axis][0] + 2.,
    })
}

pub fn rebuild_end_plates(project: &mut Project, previous: &Project) -> Result<(), ModelError> {
    let instances: Vec<_> = project
        .connections
        .values()
        .filter(|c| matches!(c.template.kind, TemplateKind::EndPlate { .. }))
        .cloned()
        .collect();
    for mut instance in instances {
        if !crate::instance_changed(project, previous, &instance) {
            continue;
        }
        let recipe = end_plate_geometry(project, &instance)?;
        crate::retain_children(
            project,
            &mut instance,
            &[
                "plate",
                "hole-0",
                "hole-1",
                "hole-2",
                "hole-3",
                "support-hole-0",
                "support-hole-1",
                "support-hole-2",
                "support-hole-3",
                "bolts",
                "weld",
            ],
        );
        let plate = crate::child_id(project, &mut instance, "plate")?;
        let mut holes = Vec::new();
        for part_role in ["plate", "support"] {
            for i in 0..4 {
                let role = if part_role == "plate" {
                    format!("hole-{i}")
                } else {
                    format!("support-hole-{i}")
                };
                holes.push(crate::child_id(project, &mut instance, &role)?);
            }
        }
        let bolts = crate::child_id(project, &mut instance, "bolts")?;
        let weld = crate::child_id(project, &mut instance, "weld")?;
        let t = &instance.template;
        project.elements.insert(
            plate,
            Element {
                id: plate,
                name: format!("{} — торцевая пластина", t.name),
                material: recipe.plate.material,
                shape: recipe.plate.shape,
                properties: project
                    .elements
                    .get(&plate)
                    .map(|e| e.properties.clone())
                    .unwrap_or_default(),
            },
        );
        let bolt_frame = recipe.plate.bolt_frame;
        let mut all_holes = BTreeSet::new();
        for (part_index, part) in [plate, recipe.support].into_iter().enumerate() {
            let parent = *project.elements[&part].shape.frame();
            let mut index = 0;
            for x in [0., recipe.plate.steps_x[0]] {
                for y in [0., recipe.plate.steps_y[0]] {
                    let id = holes[part_index * 4 + index];
                    index += 1;
                    let frame = local_frame(
                        Frame {
                            origin: bolt_frame.world(Point3([x, y, 0.])),
                            ..bolt_frame
                        },
                        parent,
                    );
                    project.details.insert(
                        id,
                        Detail {
                            id,
                            kind: DetailKind::Hole {
                                part,
                                frame,
                                diameter: t.bolt_diameter + t.clearance,
                                depth: recipe.hole_depth,
                            },
                        },
                    );
                    all_holes.insert(id);
                }
            }
        }
        project.details.insert(
            bolts,
            Detail {
                id: bolts,
                kind: DetailKind::BoltGroup {
                    parts: BTreeSet::from([plate, recipe.support]),
                    holes: all_holes,
                    frame: bolt_frame,
                    diameter: t.bolt_diameter,
                    clearance: t.clearance,
                    steps_x: recipe.plate.steps_x,
                    steps_y: recipe.plate.steps_y,
                    standard: t.bolt_standard.clone(),
                    grade: t.bolt_grade.clone(),
                },
            },
        );
        project.details.insert(
            weld,
            Detail {
                id: weld,
                kind: DetailKind::Weld {
                    parts: BTreeSet::from([instance.primary, plate]),
                    weld_type: WeldType::Fillet,
                    size: t.weld_size,
                    path: recipe.plate.weld_path,
                    shop: true,
                },
            },
        );
        project.element_body(plate)?;
        project
            .element_body(recipe.support)
            .map_err(|e| ModelError::InvalidValue(format!("Крепёж не размещается в опоре: {e}")))?;
        project.connections.insert(instance.id, instance);
    }
    Ok(())
}
