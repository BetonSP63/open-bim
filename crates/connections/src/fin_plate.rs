//! Hinged beam connection on a fin plate («шарнир на пластине»): a plate
//! welded to the support face and bolted to the beam web with one vertical
//! row of bolts.
use crate::end_plate::{bounds, local_frame};
use crate::{child_id, instance_changed, retain_children};
use open_bim_model::{
    ConnectionInstance, Detail, DetailKind, Element, ElementShape, Frame, LengthMm, ModelError,
    ObjectId, Point3, ProfileShape, Project, TemplateKind, WeldSides, WeldType,
};
use std::collections::BTreeSet;

#[derive(Clone, Debug)]
pub struct FinPlateGeometry {
    pub shape: ElementShape,
    pub material: ObjectId,
    pub bolt_frame: Frame,
    pub hole_depth: f64,
    pub rows: u32,
    pub pitch: f64,
    pub weld_path: Vec<Point3>,
    pub support: ObjectId,
}

/// Child roles of a fin plate with `rows` bolts.
pub fn fin_plate_roles(rows: u32) -> Vec<String> {
    let mut roles = vec!["plate".to_string(), "bolts".into(), "weld".into()];
    for i in 0..rows {
        roles.push(format!("plate-hole-{i}"));
        roles.push(format!("web-hole-{i}"));
    }
    roles
}

pub fn fin_plate_geometry(
    project: &Project,
    instance: &ConnectionInstance,
) -> Result<FinPlateGeometry, ModelError> {
    let template = &instance.template;
    template.validate()?;
    let TemplateKind::FinPlate {
        height,
        lap,
        gap,
        pitch,
        rows,
    } = template.kind
    else {
        return Err(ModelError::InvalidValue(
            "Ожидается узел на пластине".into(),
        ));
    };
    let invalid = |m: &str| Err(ModelError::InvalidValue(m.into()));
    let support = instance
        .secondary
        .ok_or_else(|| ModelError::InvalidValue("Узлу на пластине нужна опора".into()))?;
    if support == instance.primary {
        return invalid("Родители узла должны различаться");
    }
    let beam = project
        .elements
        .get(&instance.primary)
        .ok_or(ModelError::MissingReference(instance.primary))?;
    let ElementShape::Member {
        profile,
        length,
        frame,
        ..
    } = &beam.shape
    else {
        return invalid("Основной элемент узла на пластине — балка");
    };
    frame.validate()?;
    let section = &project
        .profiles
        .get(profile)
        .ok_or(ModelError::MissingReference(*profile))?
        .shape;
    // Plate on the +x face of the web, between the flanges.
    let (face, web, clear) = match *section {
        ProfileShape::ISection {
            height: h,
            web,
            flange,
            ..
        } => (web / 2., web, h - 2. * flange),
        ProfileShape::Channel {
            width,
            height: h,
            web,
            flange,
        } => (-width / 2. + web, web, h - 2. * flange),
        _ => return invalid("Узел на пластине ставится на двутавр или швеллер"),
    };
    let t = template.thickness;
    let edge = template.edge_distance;
    let len = length.value();
    if height > clear + 0.001 {
        return invalid("Пластина выше стенки балки между полками");
    }
    if lap > len - 0.001 {
        return invalid("Нахлёст пластины длиннее балки");
    }
    let x_out = face + t;
    let y0 = -height / 2. + (height - pitch * (rows as f64 - 1.)) / 2.;
    let z_bolts = if instance.at_end {
        len - lap + edge
    } else {
        lap - edge
    };
    let [xb, yb, zb] = frame.axes;
    let neg = |a: [f64; 3]| a.map(|v| -v);
    // Plate frame: x from the free end towards the support, y up the web,
    // z (thickness) right-handed.
    let plate_frame = if instance.at_end {
        Frame {
            origin: frame.world(Point3([x_out, -height / 2., len - lap])),
            axes: [zb, yb, neg(xb)],
        }
    } else {
        Frame {
            origin: frame.world(Point3([face, -height / 2., lap])),
            axes: [neg(zb), yb, xb],
        }
    };
    plate_frame.validate()?;
    let plate_length = lap + gap;
    let bolt_frame = Frame {
        origin: frame.world(Point3([x_out + 1., y0, z_bolts])),
        axes: [zb, yb, neg(xb)],
    };
    bolt_frame.validate()?;
    // The support face must lie where the plate ends.
    let host = project
        .elements
        .get(&support)
        .ok_or(ModelError::MissingReference(support))?;
    let hf = *host.shape.frame();
    hf.validate()?;
    let extent = bounds(project, &host.shape)?;
    let outward = if instance.at_end { zb } else { neg(zb) };
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
        return invalid("У круглой опоры нет плоской грани для пластины");
    }
    let weld_mid = plate_frame.world(Point3([plate_length, height / 2., t / 2.]));
    let contact = local_frame(
        Frame {
            origin: weld_mid,
            ..hf
        },
        hf,
    )
    .origin
    .0[axis];
    let expected = extent[axis][if projected[axis] > 0. { 0 } else { 1 }];
    if (contact - expected).abs() > 0.001 {
        return invalid(
            "Торец балки с зазором не доходит до грани опоры: проверьте зазор и конец балки",
        );
    }
    let weld_path = vec![
        plate_frame.world(Point3([plate_length, 0., t / 2.])),
        plate_frame.world(Point3([plate_length, height, t / 2.])),
    ];
    Ok(FinPlateGeometry {
        shape: ElementShape::Plate {
            contour: vec![
                [0., 0.],
                [plate_length, 0.],
                [plate_length, height],
                [0., height],
            ],
            thickness: LengthMm::new(t).map_err(|e| ModelError::InvalidValue(e.to_string()))?,
            frame: plate_frame,
        },
        material: beam.material,
        bolt_frame,
        hole_depth: t + web + 2.,
        rows,
        pitch,
        weld_path,
        support,
    })
}

pub fn rebuild_fin_plates(project: &mut Project, previous: &Project) -> Result<(), ModelError> {
    let instances: Vec<_> = project
        .connections
        .values()
        .filter(|c| matches!(c.template.kind, TemplateKind::FinPlate { .. }))
        .cloned()
        .collect();
    for mut instance in instances {
        if !instance_changed(project, previous, &instance) {
            continue;
        }
        let recipe = fin_plate_geometry(project, &instance)?;
        let roles = fin_plate_roles(recipe.rows);
        let refs: Vec<&str> = roles.iter().map(String::as_str).collect();
        retain_children(project, &mut instance, &refs);
        let plate = child_id(project, &mut instance, "plate")?;
        let bolts = child_id(project, &mut instance, "bolts")?;
        let weld = child_id(project, &mut instance, "weld")?;
        let t = instance.template.clone();
        project.elements.insert(
            plate,
            Element {
                id: plate,
                name: format!("{} — пластина", t.name),
                material: recipe.material,
                shape: recipe.shape,
                properties: project
                    .elements
                    .get(&plate)
                    .map(|e| e.properties.clone())
                    .unwrap_or_default(),
            },
        );
        let mut holes = BTreeSet::new();
        for i in 0..recipe.rows {
            let origin = recipe
                .bolt_frame
                .world(Point3([0., recipe.pitch * i as f64, 0.]));
            for (role, part) in [("plate-hole", plate), ("web-hole", instance.primary)] {
                let id = child_id(project, &mut instance, &format!("{role}-{i}"))?;
                let local = local_frame(
                    Frame {
                        origin,
                        ..recipe.bolt_frame
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
                            diameter: t.bolt_diameter + t.clearance,
                            depth: recipe.hole_depth,
                        },
                    },
                );
                holes.insert(id);
            }
        }
        project.details.insert(
            bolts,
            Detail {
                id: bolts,
                kind: DetailKind::BoltGroup {
                    parts: BTreeSet::from([plate, instance.primary]),
                    holes,
                    frame: recipe.bolt_frame,
                    diameter: t.bolt_diameter,
                    clearance: t.clearance,
                    steps_x: vec![],
                    steps_y: vec![recipe.pitch; recipe.rows as usize - 1],
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
                    parts: BTreeSet::from([recipe.support, plate]),
                    weld_type: WeldType::Fillet,
                    size: t.weld_size,
                    path: recipe.weld_path,
                    shop: true,
                    sides: WeldSides::Both,
                    around: false,
                },
            },
        );
        for part in [plate, instance.primary] {
            project.element_body(part).map_err(|e| {
                ModelError::InvalidValue(format!(
                    "Болты узла на пластине не размещаются в детали {part}: {e}"
                ))
            })?;
        }
        project.connections.insert(instance.id, instance);
    }
    Ok(())
}
