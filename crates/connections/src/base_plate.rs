use open_bim_model::{
    ConnectionInstance, ElementShape, Frame, LengthMm, ModelError, ObjectId, Point3, ProfileShape,
    Project, TemplateKind,
};
use open_bim_model::{Detail, DetailKind, Element, WeldType};
use std::collections::BTreeSet;

/// Rebuild base plates in a staged project. The caller publishes only after all
/// generators and model validation succeed, so failures retain the prior result.
pub fn rebuild_base_plates(project: &mut Project, previous: &Project) -> Result<(), ModelError> {
    let instances: Vec<_> = project
        .connections
        .values()
        .filter(|c| matches!(c.template.kind, TemplateKind::BasePlate { .. }))
        .cloned()
        .collect();
    for mut instance in instances {
        let parent = project.elements.get(&instance.primary);
        let profile_unchanged = match parent.map(|p| &p.shape) {
            Some(ElementShape::Member { profile, .. }) => {
                project.profiles.get(profile) == previous.profiles.get(profile)
            }
            _ => false,
        };
        if previous.connections.get(&instance.id) == Some(&instance)
            && parent == previous.elements.get(&instance.primary)
            && profile_unchanged
            && instance.children.values().all(|id| {
                project.elements.get(id) == previous.elements.get(id)
                    && project.details.get(id) == previous.details.get(id)
            })
        {
            continue;
        }
        let recipe = base_plate_geometry(project, &instance)?;
        let mut child = |role: &str| -> Result<ObjectId, ModelError> {
            if let Some(id) = instance.children.get(role) {
                if !project.elements.contains_key(id) && !project.details.contains_key(id) {
                    return Err(ModelError::MissingReference(*id));
                }
                return Ok(*id);
            }
            let id = project.allocate_id()?;
            instance.children.insert(role.into(), id);
            Ok(id)
        };
        let plate = child("plate")?;
        let mut holes = Vec::new();
        for i in 0..4 {
            holes.push(child(&format!("hole-{i}"))?);
        }
        let bolts = child("bolts")?;
        let weld = child("weld")?;
        let t = &instance.template;
        project.elements.insert(
            plate,
            Element {
                id: plate,
                name: format!("{} — плита", t.name),
                material: recipe.material,
                shape: recipe.shape,
                properties: Default::default(),
            },
        );
        for (id, position) in holes.iter().zip(recipe.holes) {
            project.details.insert(
                *id,
                Detail {
                    id: *id,
                    kind: DetailKind::Hole {
                        part: plate,
                        frame: Frame {
                            origin: Point3([position[0], position[1], -1.0]),
                            ..Frame::default()
                        },
                        diameter: t.bolt_diameter + t.clearance,
                        depth: t.thickness + 2.0,
                    },
                },
            );
        }
        project.details.insert(
            bolts,
            Detail {
                id: bolts,
                kind: DetailKind::BoltGroup {
                    parts: BTreeSet::from([plate]),
                    holes: holes.into_iter().collect(),
                    frame: recipe.bolt_frame,
                    diameter: t.bolt_diameter,
                    clearance: t.clearance,
                    steps_x: recipe.steps_x,
                    steps_y: recipe.steps_y,
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
                    path: recipe.weld_path,
                    shop: true,
                },
            },
        );
        project.element_body(plate)?;
        project.connections.insert(instance.id, instance);
    }
    Ok(())
}

/// ID-free geometric recipe. The transaction generator assigns semantic IDs.
#[derive(Clone, Debug)]
pub struct BasePlateGeometry {
    pub shape: ElementShape,
    pub material: ObjectId,
    pub bolt_frame: Frame,
    pub steps_x: Vec<f64>,
    pub steps_y: Vec<f64>,
    pub holes: Vec<[f64; 2]>,
    pub weld_path: Vec<Point3>,
}

pub fn base_plate_geometry(
    project: &Project,
    instance: &ConnectionInstance,
) -> Result<BasePlateGeometry, ModelError> {
    let template = &instance.template;
    template.validate()?;
    let TemplateKind::BasePlate { overhang } = template.kind else {
        return Err(ModelError::InvalidValue(
            "Ожидается шаблон опорной плиты".into(),
        ));
    };
    if instance.secondary.is_some() {
        return Err(ModelError::InvalidValue(
            "Опорная плита крепится к одному родителю".into(),
        ));
    }
    let parent = project
        .elements
        .get(&instance.primary)
        .ok_or(ModelError::MissingReference(instance.primary))?;
    let ElementShape::Member {
        profile,
        length,
        frame,
        ..
    } = &parent.shape
    else {
        return Err(ModelError::InvalidValue(
            "Родитель опорной плиты должен иметь сечение".into(),
        ));
    };
    frame.validate()?;
    let section = &project
        .profiles
        .get(profile)
        .ok_or(ModelError::MissingReference(*profile))?
        .shape;
    section.area_mm2()?;
    let (w, h) = match *section {
        ProfileShape::ISection { width, height, .. }
        | ProfileShape::Angle { width, height, .. }
        | ProfileShape::Channel { width, height, .. }
        | ProfileShape::Rectangle { width, height }
        | ProfileShape::RectangularHollow { width, height, .. } => (width, height),
        ProfileShape::CircularHollow { diameter, .. } => (diameter, diameter),
    };
    let width = w + 2.0 * overhang;
    let height = h + 2.0 * overhang;
    let e = template.edge_distance;
    let z = if instance.at_end { length.value() } else { 0.0 };
    let plate_z = if instance.at_end {
        z
    } else {
        -template.thickness
    };
    let plate_frame = Frame {
        origin: frame.world(Point3([-width / 2.0, -height / 2.0, plate_z])),
        axes: frame.axes,
    };
    let bolt_frame = Frame {
        origin: plate_frame.world(Point3([e, e, -1.0])),
        axes: frame.axes,
    };
    let mut perimeter = match *section {
        ProfileShape::CircularHollow { diameter, .. } => (0..64)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / 64.0;
                [diameter / 2.0 * a.cos(), diameter / 2.0 * a.sin()]
            })
            .collect::<Vec<_>>(),
        ProfileShape::RectangularHollow { width, height, .. } => vec![
            [-width / 2.0, -height / 2.0],
            [width / 2.0, -height / 2.0],
            [width / 2.0, height / 2.0],
            [-width / 2.0, height / 2.0],
        ],
        _ => section.solid_contour()?,
    };
    perimeter.push(perimeter[0]);
    let weld_path = perimeter
        .into_iter()
        .map(|p| frame.world(Point3([p[0], p[1], z])))
        .collect();
    Ok(BasePlateGeometry {
        shape: ElementShape::Plate {
            contour: vec![[0.0, 0.0], [width, 0.0], [width, height], [0.0, height]],
            thickness: LengthMm::new(template.thickness)
                .map_err(|e| ModelError::InvalidValue(e.to_string()))?,
            frame: plate_frame,
        },
        material: parent.material,
        bolt_frame,
        steps_x: vec![width - 2.0 * e],
        steps_y: vec![height - 2.0 * e],
        holes: vec![
            [e, e],
            [width - e, e],
            [e, height - e],
            [width - e, height - e],
        ],
        weld_path,
    })
}
