//! Column stiffeners («рёбра жёсткости колонны»): a pair of plates on each
//! side of the column web at the levels of the beam flanges, welded to the
//! web and both column flanges.
use crate::{child_id, instance_changed, retain_children};
use open_bim_model::{
    ConnectionInstance, Detail, DetailKind, Element, ElementShape, Frame, LengthMm, ModelError,
    ObjectId, Point3, ProfileShape, Project, TemplateKind, WeldSides, WeldType,
};
use std::collections::BTreeSet;

/// Child roles: four plates (top +x, top −x, bottom +x, bottom −x across the
/// column) and one weld each.
pub const STIFFENER_ROLES: [&str; 8] = [
    "stiffener-0",
    "stiffener-1",
    "stiffener-2",
    "stiffener-3",
    "weld-0",
    "weld-1",
    "weld-2",
    "weld-3",
];

#[derive(Clone, Debug)]
pub struct StiffenerGeometry {
    /// Plate shapes in role order.
    pub plates: Vec<ElementShape>,
    /// Weld paths in role order, world coordinates.
    pub welds: Vec<Vec<Point3>>,
    pub material: ObjectId,
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| a[i] * b[i]).sum()
}

/// Height and flange of an I-section or channel.
fn depth_and_flange(
    project: &Project,
    profile: ObjectId,
) -> Result<(ProfileShape, f64, f64), ModelError> {
    let shape = project
        .profiles
        .get(&profile)
        .ok_or(ModelError::MissingReference(profile))?
        .shape
        .clone();
    match shape {
        ProfileShape::ISection { height, flange, .. }
        | ProfileShape::Channel { height, flange, .. } => Ok((shape, height, flange)),
        _ => Err(ModelError::InvalidValue(
            "Рёбра жёсткости ставятся по полкам двутавра или швеллера".into(),
        )),
    }
}

pub fn stiffener_geometry(
    project: &Project,
    instance: &ConnectionInstance,
) -> Result<StiffenerGeometry, ModelError> {
    let template = &instance.template;
    template.validate()?;
    let TemplateKind::ColumnStiffeners { chamfer } = template.kind else {
        return Err(ModelError::InvalidValue(
            "Ожидаются рёбра жёсткости колонны".into(),
        ));
    };
    let invalid = |m: &str| Err(ModelError::InvalidValue(m.into()));
    let beam_id = instance
        .secondary
        .ok_or_else(|| ModelError::InvalidValue("Рёбрам жёсткости нужна балка".into()))?;
    if beam_id == instance.primary {
        return invalid("Родители узла должны различаться");
    }
    let member = |id: ObjectId| -> Result<(&Element, ObjectId, f64, Frame), ModelError> {
        let element = project
            .elements
            .get(&id)
            .ok_or(ModelError::MissingReference(id))?;
        let ElementShape::Member {
            profile,
            length,
            frame,
            ..
        } = &element.shape
        else {
            return Err(ModelError::InvalidValue(
                "Рёбра жёсткости связывают колонну и балку".into(),
            ));
        };
        frame.validate()?;
        Ok((element, *profile, length.value(), *frame))
    };
    let (column, column_profile, column_length, cf) = member(instance.primary)?;
    let (_, beam_profile, _, bf) = member(beam_id)?;
    let ProfileShape::ISection {
        height: h,
        width: b,
        web,
        flange: tf,
    } = project
        .profiles
        .get(&column_profile)
        .ok_or(ModelError::MissingReference(column_profile))?
        .shape
    else {
        return invalid("Рёбра жёсткости ставятся в двутавровую колонну");
    };
    let (_, hb, tfb) = depth_and_flange(project, beam_profile)?;
    let [xc, yc, zc] = cf.axes;
    let [_, yb, zb] = bf.axes;
    if dot(zb, zc).abs() > 1e-6 || dot(yb, zc).abs() < 1. - 1e-6 {
        return invalid("Балка должна быть перпендикулярна колонне, стенкой вдоль её оси");
    }
    let t = template.thickness;
    let width = b / 2. - web / 2.;
    let height = h - 2. * tf;
    if chamfer * 2. >= height - 0.001 || chamfer >= width - 0.001 {
        return invalid("Срез угла ребра больше самого ребра");
    }
    // Levels of the beam flange centres along the column axis.
    let level = |offset: f64| {
        let p: [f64; 3] = std::array::from_fn(|i| bf.origin.0[i] + yb[i] * offset - cf.origin.0[i]);
        dot(p, zc)
    };
    let offset = hb / 2. - tfb / 2.;
    let mut levels = [level(offset), level(-offset)];
    levels.sort_by(|a, b| b.total_cmp(a));
    if levels[0] - levels[1] < t + 0.001 {
        return invalid("Полки балки слишком близко для двух рядов рёбер");
    }
    if levels
        .iter()
        .any(|z| *z - t / 2. < -0.001 || *z + t / 2. > column_length + 0.001)
    {
        return invalid("Полки балки выходят за длину колонны");
    }
    let c = chamfer;
    // Chamfers clear the web-to-flange fillets on the web side.
    let near_web: Vec<[f64; 2]> = if c > 0. {
        vec![
            [c, 0.],
            [width, 0.],
            [width, height],
            [c, height],
            [0., height - c],
            [0., c],
        ]
    } else {
        vec![[0., 0.], [width, 0.], [width, height], [0., height]]
    };
    let mirrored: Vec<[f64; 2]> = if c > 0. {
        vec![
            [0., 0.],
            [width - c, 0.],
            [width, c],
            [width, height - c],
            [width - c, height],
            [0., height],
        ]
    } else {
        near_web.clone()
    };
    let thickness = LengthMm::new(t).map_err(|e| ModelError::InvalidValue(e.to_string()))?;
    let mut plates = Vec::new();
    let mut welds = Vec::new();
    for z in levels {
        for side in [1., -1.] {
            let x0 = if side > 0. { web / 2. } else { -b / 2. };
            let frame = Frame {
                origin: cf.world(Point3([x0, -h / 2. + tf, z - t / 2.])),
                axes: [xc, yc, zc],
            };
            frame.validate()?;
            let contour = if side > 0. {
                near_web.clone()
            } else {
                mirrored.clone()
            };
            // Along the flange, the web and the other flange, at mid-thickness.
            let path: Vec<[f64; 2]> = if side > 0. {
                if c > 0. {
                    vec![
                        [width, 0.],
                        [c, 0.],
                        [0., c],
                        [0., height - c],
                        [c, height],
                        [width, height],
                    ]
                } else {
                    vec![[width, 0.], [0., 0.], [0., height], [width, height]]
                }
            } else if c > 0. {
                vec![
                    [0., 0.],
                    [width - c, 0.],
                    [width, c],
                    [width, height - c],
                    [width - c, height],
                    [0., height],
                ]
            } else {
                vec![[0., 0.], [width, 0.], [width, height], [0., height]]
            };
            welds.push(
                path.iter()
                    .map(|p| frame.world(Point3([p[0], p[1], t / 2.])))
                    .collect(),
            );
            plates.push(ElementShape::Plate {
                contour,
                thickness,
                frame,
            });
        }
    }
    Ok(StiffenerGeometry {
        plates,
        welds,
        material: column.material,
    })
}

pub fn rebuild_stiffeners(project: &mut Project, previous: &Project) -> Result<(), ModelError> {
    let instances: Vec<_> = project
        .connections
        .values()
        .filter(|c| matches!(c.template.kind, TemplateKind::ColumnStiffeners { .. }))
        .cloned()
        .collect();
    for mut instance in instances {
        if !instance_changed(project, previous, &instance) {
            continue;
        }
        let recipe = match stiffener_geometry(project, &instance) {
            Ok(recipe) => recipe,
            Err(error) => {
                crate::keep_failed(project, previous, instance, error)?;
                continue;
            }
        };
        retain_children(project, &mut instance, &STIFFENER_ROLES);
        let t = instance.template.clone();
        for (i, (shape, path)) in recipe.plates.into_iter().zip(recipe.welds).enumerate() {
            let plate = child_id(project, &mut instance, STIFFENER_ROLES[i])?;
            let weld = child_id(project, &mut instance, STIFFENER_ROLES[4 + i])?;
            project.elements.insert(
                plate,
                Element {
                    id: plate,
                    name: format!("{} — ребро", t.name),
                    material: recipe.material,
                    shape,
                    properties: project
                        .elements
                        .get(&plate)
                        .map(|e| e.properties.clone())
                        .unwrap_or_default(),
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
                        path,
                        shop: true,
                        sides: WeldSides::Both,
                        around: false,
                        process: Default::default(),
                    },
                },
            );
        }
        project.connections.insert(instance.id, instance);
    }
    Ok(())
}
