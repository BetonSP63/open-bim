//! Welded haunch («вута»): a triangular web plate under the beam end and a
//! flange plate along its sloped edge, welded to the beam bottom flange and
//! to the support face.
use crate::{child_id, instance_changed, retain_children};
use open_bim_model::{
    ConnectionInstance, Detail, DetailKind, Element, ElementShape, Frame, HAUNCH_ROLES, LengthMm,
    ModelError, ObjectId, Point3, ProfileShape, Project, TemplateKind, WeldSides, WeldType,
};
use std::collections::BTreeSet;

#[derive(Clone, Debug)]
pub struct HaunchGeometry {
    pub web: ElementShape,
    pub flange: ElementShape,
    /// Weld paths, world coordinates: web to beam, flange to web, flange tip
    /// to beam, haunch end to support.
    pub welds: [Vec<Point3>; 4],
    pub material: ObjectId,
    pub support: ObjectId,
}

pub fn haunch_geometry(
    project: &Project,
    instance: &ConnectionInstance,
) -> Result<HaunchGeometry, ModelError> {
    let template = &instance.template;
    template.validate()?;
    let TemplateKind::Haunch { length, depth } = template.kind else {
        return Err(ModelError::InvalidValue("Ожидается вута".into()));
    };
    let invalid = |m: &str| Err(ModelError::InvalidValue(m.into()));
    let support = instance
        .secondary
        .ok_or_else(|| ModelError::InvalidValue("Вуте нужна опора".into()))?;
    if support == instance.primary {
        return invalid("Родители узла должны различаться");
    }
    let beam = project
        .elements
        .get(&instance.primary)
        .ok_or(ModelError::MissingReference(instance.primary))?;
    let ElementShape::Member {
        profile,
        length: beam_length,
        frame,
        ..
    } = &beam.shape
    else {
        return invalid("Вута ставится под балку");
    };
    frame.validate()?;
    let ProfileShape::ISection {
        height: h,
        width: b,
        flange: tf,
        ..
    } = project
        .profiles
        .get(profile)
        .ok_or(ModelError::MissingReference(*profile))?
        .shape
    else {
        return invalid("Вута ставится под двутавровую балку");
    };
    let len = beam_length.value();
    if length > len / 2. + 0.001 {
        return invalid("Вута длиннее половины балки");
    }
    if depth > 2. * h + 0.001 {
        return invalid("Вута выше двух высот балки");
    }
    let t = template.thickness;
    let [xa, ya, za] = frame.axes;
    let neg = |a: [f64; 3]| a.map(|v| -v);
    let at = |x: f64, y: f64, z: f64| frame.world(Point3([x, y, z]));
    let (end, inner) = if instance.at_end {
        (len, len - length)
    } else {
        (0., length)
    };
    let outward = if instance.at_end { za } else { neg(za) };
    let bottom = -h / 2.;
    // The haunch end, at the beam end plane, must lie on the support face.
    crate::touch_support_face(
        project,
        support,
        at(0., bottom - depth / 2., end),
        outward,
        "Конец балки не доходит до грани опоры: вута приваривается к опоре",
    )?;
    let z0 = end.min(inner);
    let contour = if instance.at_end {
        vec![[depth, 0.], [depth, length], [0., length]]
    } else {
        vec![[0., 0.], [depth, 0.], [depth, length]]
    };
    let thickness = |v: f64| LengthMm::new(v).map_err(|e| ModelError::InvalidValue(e.to_string()));
    let web = ElementShape::Plate {
        contour,
        thickness: thickness(t)?,
        frame: Frame {
            origin: at(-t / 2., bottom - depth, z0),
            axes: [ya, za, xa],
        },
    };
    // Flange plate under the sloped edge, from the beam flange to the tip;
    // its thickness points away from the web plate.
    let slope = (length * length + depth * depth).sqrt();
    let dir: [f64; 3] = {
        let s = if instance.at_end { 1. } else { -1. };
        std::array::from_fn(|i| (-depth * ya[i] + s * length * za[i]) / slope)
    };
    let (origin, across) = if instance.at_end {
        (at(b / 2., bottom, inner), neg(xa))
    } else {
        (at(-b / 2., bottom, inner), xa)
    };
    let normal = [
        dir[1] * across[2] - dir[2] * across[1],
        dir[2] * across[0] - dir[0] * across[2],
        dir[0] * across[1] - dir[1] * across[0],
    ];
    let flange_frame = Frame {
        origin,
        axes: [dir, across, normal],
    };
    web.frame().validate()?;
    flange_frame.validate()?;
    let flange = ElementShape::Plate {
        contour: vec![[0., 0.], [slope, 0.], [slope, b], [0., b]],
        thickness: thickness(tf)?,
        frame: flange_frame,
    };
    let tip = at(0., bottom - depth, end);
    let root = at(0., bottom, inner);
    let welds = [
        vec![root, at(0., bottom, end)],
        vec![root, tip],
        vec![at(-b / 2., bottom, inner), at(b / 2., bottom, inner)],
        vec![at(0., bottom, end), tip],
    ];
    Ok(HaunchGeometry {
        web,
        flange,
        welds,
        material: beam.material,
        support,
    })
}

pub fn rebuild_haunches(project: &mut Project, previous: &Project) -> Result<(), ModelError> {
    let instances: Vec<_> = project
        .connections
        .values()
        .filter(|c| matches!(c.template.kind, TemplateKind::Haunch { .. }))
        .cloned()
        .collect();
    for mut instance in instances {
        if !instance_changed(project, previous, &instance) {
            continue;
        }
        let recipe = match haunch_geometry(project, &instance) {
            Ok(recipe) => recipe,
            Err(error) => {
                crate::keep_failed(project, previous, instance, error)?;
                continue;
            }
        };
        retain_children(project, &mut instance, &HAUNCH_ROLES);
        let t = instance.template.clone();
        let web = child_id(project, &mut instance, "web-plate")?;
        let flange = child_id(project, &mut instance, "flange-plate")?;
        for (id, shape, what) in [
            (web, recipe.web, "стенка вуты"),
            (flange, recipe.flange, "полка вуты"),
        ] {
            project.elements.insert(
                id,
                Element {
                    id,
                    name: format!("{} — {what}", t.name),
                    material: recipe.material,
                    shape,
                    properties: project
                        .elements
                        .get(&id)
                        .map(|e| e.properties.clone())
                        .unwrap_or_default(),
                },
            );
        }
        let parts: [BTreeSet<ObjectId>; 4] = [
            [web, instance.primary].into(),
            [flange, web].into(),
            [flange, instance.primary].into(),
            [recipe.support, web, flange].into(),
        ];
        for ((role, parts), path) in HAUNCH_ROLES[2..].iter().zip(parts).zip(recipe.welds) {
            let id = child_id(project, &mut instance, role)?;
            project.details.insert(
                id,
                Detail {
                    id,
                    kind: DetailKind::Weld {
                        parts,
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
