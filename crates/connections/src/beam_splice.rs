//! Bolted beam splice («стык балок на накладках»): two beams end to end on
//! one axis, joined by plates on both sides of the web and outside both
//! flanges. Each plate set is bolted to each beam with its own bolt group.
use crate::end_plate::local_frame;
use crate::{child_id, instance_changed, retain_children};
use open_bim_model::{
    ConnectionInstance, Detail, DetailKind, Element, ElementShape, Frame, LengthMm, ModelError,
    ObjectId, Point3, ProfileShape, Project, TemplateKind, beam_splice_roles,
};
use std::collections::BTreeSet;

/// Largest gap between the beam ends that the splice bridges, mm.
pub const MAX_SPLICE_GAP: f64 = 50.0;

/// One bolt group of the splice.
#[derive(Clone, Debug)]
pub struct SpliceBolts {
    pub role: &'static str,
    pub prefix: &'static str,
    /// (tag, part role or `None` for the beam on this side).
    pub parts: Vec<(&'static str, Option<&'static str>)>,
    pub second_beam: bool,
    pub frame: Frame,
    pub steps_x: Vec<f64>,
    pub steps_y: Vec<f64>,
    pub hole_depth: f64,
}

#[derive(Clone, Debug)]
pub struct BeamSpliceGeometry {
    /// (role, plate shape).
    pub plates: Vec<(&'static str, ElementShape)>,
    pub bolts: Vec<SpliceBolts>,
    pub second: ObjectId,
    pub material: ObjectId,
    /// Gap between the beam ends, mm.
    pub gap: f64,
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| a[i] * b[i]).sum()
}

pub fn beam_splice_geometry(
    project: &Project,
    instance: &ConnectionInstance,
) -> Result<BeamSpliceGeometry, ModelError> {
    let template = &instance.template;
    template.validate()?;
    let TemplateKind::BeamSplice { rows, pitch } = template.kind else {
        return Err(ModelError::InvalidValue("Ожидается стык балок".into()));
    };
    let invalid = |m: &str| Err(ModelError::InvalidValue(m.into()));
    let second = instance
        .secondary
        .ok_or_else(|| ModelError::InvalidValue("Стыку нужна вторая балка".into()))?;
    if second == instance.primary {
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
            return Err(ModelError::InvalidValue("Стык соединяет две балки".into()));
        };
        frame.validate()?;
        Ok((element, *profile, length.value(), *frame))
    };
    let (beam, profile, len, af) = member(instance.primary)?;
    let (_, second_profile, second_len, bf) = member(second)?;
    let section = &project
        .profiles
        .get(&profile)
        .ok_or(ModelError::MissingReference(profile))?
        .shape;
    let second_section = &project
        .profiles
        .get(&second_profile)
        .ok_or(ModelError::MissingReference(second_profile))?
        .shape;
    let ProfileShape::ISection {
        height: h,
        width: b,
        web,
        flange: tf,
    } = *section
    else {
        return invalid("Стык на накладках делается для двутавров");
    };
    if section != second_section {
        return invalid("У стыкуемых балок должно быть одно сечение");
    }
    let [xa, ya, za] = af.axes;
    if dot(za, bf.axes[2]).abs() < 1. - 1e-9 || dot(ya, bf.axes[1]) < 1. - 1e-9 {
        return invalid("Балки должны лежать на одной оси, полками в одну сторону");
    }
    // The second beam's ends in the main beam's coordinates.
    let local = |p: Point3| {
        let d: [f64; 3] = std::array::from_fn(|i| p.0[i] - af.origin.0[i]);
        [dot(d, xa), dot(d, ya), dot(d, za)]
    };
    let ends = [
        local(bf.origin),
        local(bf.world(Point3([0., 0., second_len]))),
    ];
    if ends.iter().any(|e| e[0].abs() > 0.5 || e[1].abs() > 0.5) {
        return invalid("Балки должны лежать на одной оси, полками в одну сторону");
    }
    let s = if instance.at_end { 1. } else { -1. };
    let joint = if instance.at_end { len } else { 0. };
    let near = ends
        .iter()
        .map(|e| s * (e[2] - joint))
        .fold(f64::INFINITY, f64::min);
    if !(-0.001..=MAX_SPLICE_GAP + 0.001).contains(&near) {
        return invalid("Вторая балка должна стоять встык с зазором до 50 мм");
    }
    let gap = near.max(0.);
    let t = template.thickness;
    let edge = template.edge_distance;
    let hole = template.bolt_diameter + template.clearance;
    let lap = 2. * edge + pitch;
    if lap > len.min(second_len) - 0.001 {
        return invalid("Накладка длиннее балки");
    }
    let web_height = 2. * edge + pitch * (rows as f64 - 1.);
    if web_height > h - 2. * tf - 20. + 0.001 {
        return invalid("Накладка на стенку не помещается между полками");
    }
    let gauge = b - 2. * edge;
    if gauge / 2. - hole / 2. < web / 2. + 5. {
        return invalid("Полка узкая для двух болтов поперёк");
    }
    let length = 2. * lap + gap;
    let z0 = if instance.at_end {
        joint - lap
    } else {
        joint - lap - gap
    };
    let at = |x: f64, y: f64, z: f64| af.world(Point3([x, y, z]));
    let neg = |a: [f64; 3]| a.map(|v| -v);
    let thickness = LengthMm::new(t).map_err(|e| ModelError::InvalidValue(e.to_string()))?;
    let plate = |origin: Point3, axes: [[f64; 3]; 3], w: f64, l: f64| ElementShape::Plate {
        contour: vec![[0., 0.], [w, 0.], [w, l], [0., l]],
        thickness,
        frame: Frame { origin, axes },
    };
    let plates = vec![
        (
            "web-plate-0",
            plate(
                at(web / 2., -web_height / 2., z0),
                [ya, za, xa],
                web_height,
                length,
            ),
        ),
        (
            "web-plate-1",
            plate(
                at(-web / 2. - t, -web_height / 2., z0),
                [ya, za, xa],
                web_height,
                length,
            ),
        ),
        (
            "flange-plate-top",
            plate(at(-b / 2., h / 2., z0), [za, xa, ya], length, b),
        ),
        (
            "flange-plate-bottom",
            plate(at(-b / 2., -h / 2. - t, z0), [za, xa, ya], length, b),
        ),
    ];
    for (_, shape) in &plates {
        shape.frame().validate()?;
    }
    // First bolt column of each side, nearer the free end of the plates.
    let first = |second_beam: bool| match (instance.at_end, second_beam) {
        (true, false) => joint - edge - pitch,
        (true, true) => joint + gap + edge,
        (false, false) => joint + edge,
        (false, true) => joint - gap - edge - pitch,
    };
    let mut bolts = Vec::new();
    for second_beam in [false, true] {
        let z = first(second_beam);
        let pick = |a: &'static str, b: &'static str| if second_beam { b } else { a };
        bolts.push(SpliceBolts {
            role: pick("web-bolts-a", "web-bolts-b"),
            prefix: pick("web-a", "web-b"),
            parts: vec![
                ("plate0", Some("web-plate-0")),
                ("plate1", Some("web-plate-1")),
                ("beam", None),
            ],
            second_beam,
            frame: Frame {
                origin: at(web / 2. + t + 1., -web_height / 2. + edge, z),
                axes: [za, ya, neg(xa)],
            },
            steps_x: vec![pitch],
            steps_y: vec![pitch; rows as usize - 1],
            hole_depth: 2. * t + web + 2.,
        });
        bolts.push(SpliceBolts {
            role: pick("top-bolts-a", "top-bolts-b"),
            prefix: pick("top-a", "top-b"),
            parts: vec![("plate", Some("flange-plate-top")), ("beam", None)],
            second_beam,
            frame: Frame {
                origin: at(gauge / 2., h / 2. + t + 1., z),
                axes: [za, neg(xa), neg(ya)],
            },
            steps_x: vec![pitch],
            steps_y: vec![gauge],
            hole_depth: t + tf + 2.,
        });
        bolts.push(SpliceBolts {
            role: pick("bottom-bolts-a", "bottom-bolts-b"),
            prefix: pick("bottom-a", "bottom-b"),
            parts: vec![("plate", Some("flange-plate-bottom")), ("beam", None)],
            second_beam,
            frame: Frame {
                origin: at(-gauge / 2., -h / 2. - t - 1., z),
                axes: [za, xa, ya],
            },
            steps_x: vec![pitch],
            steps_y: vec![gauge],
            hole_depth: t + tf + 2.,
        });
    }
    for group in &bolts {
        group.frame.validate()?;
    }
    Ok(BeamSpliceGeometry {
        plates,
        bolts,
        second,
        material: beam.material,
        gap,
    })
}

pub fn rebuild_beam_splices(project: &mut Project, previous: &Project) -> Result<(), ModelError> {
    let instances: Vec<_> = project
        .connections
        .values()
        .filter(|c| matches!(c.template.kind, TemplateKind::BeamSplice { .. }))
        .cloned()
        .collect();
    for mut instance in instances {
        if !instance_changed(project, previous, &instance) {
            continue;
        }
        let TemplateKind::BeamSplice { rows, .. } = instance.template.kind else {
            continue;
        };
        let recipe = match beam_splice_geometry(project, &instance) {
            Ok(recipe) => recipe,
            Err(error) => {
                crate::keep_failed(project, previous, instance, error)?;
                continue;
            }
        };
        let roles = beam_splice_roles(rows);
        let refs: Vec<&str> = roles.iter().map(String::as_str).collect();
        retain_children(project, &mut instance, &refs);
        let t = instance.template.clone();
        for (role, shape) in recipe.plates {
            let id = child_id(project, &mut instance, role)?;
            project.elements.insert(
                id,
                Element {
                    id,
                    name: format!("{} — накладка", t.name),
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
        for group in recipe.bolts {
            let beam = if group.second_beam {
                recipe.second
            } else {
                instance.primary
            };
            let mut points = Vec::new();
            let xs = [0., group.steps_x[0]];
            let mut ys = vec![0.];
            for step in &group.steps_y {
                ys.push(ys.last().unwrap() + step);
            }
            for x in xs {
                for y in &ys {
                    points.push(group.frame.world(Point3([x, *y, 0.])));
                }
            }
            let mut parts = BTreeSet::new();
            let mut holes = BTreeSet::new();
            for (tag, role) in &group.parts {
                let part = match role {
                    Some(role) => instance.children[*role],
                    None => beam,
                };
                parts.insert(part);
                for (k, origin) in points.iter().enumerate() {
                    let id = child_id(
                        project,
                        &mut instance,
                        &format!("{}-{tag}-{k}", group.prefix),
                    )?;
                    let frame = local_frame(
                        Frame {
                            origin: *origin,
                            ..group.frame
                        },
                        *project.elements[&part].shape.frame(),
                    );
                    project.details.insert(
                        id,
                        Detail {
                            id,
                            kind: DetailKind::Hole {
                                part,
                                frame,
                                diameter: t.bolt_diameter + t.clearance,
                                depth: group.hole_depth,
                                slot: 0.0,
                            },
                        },
                    );
                    holes.insert(id);
                }
            }
            let id = child_id(project, &mut instance, group.role)?;
            project.details.insert(
                id,
                Detail {
                    id,
                    kind: DetailKind::BoltGroup {
                        parts,
                        holes,
                        frame: group.frame,
                        diameter: t.bolt_diameter,
                        clearance: t.clearance,
                        steps_x: group.steps_x,
                        steps_y: group.steps_y,
                        standard: t.bolt_standard.clone(),
                        grade: t.bolt_grade.clone(),
                        points: Vec::new(),
                    },
                },
            );
        }
        for role in [
            "web-plate-0",
            "web-plate-1",
            "flange-plate-top",
            "flange-plate-bottom",
        ] {
            let part = instance.children[role];
            project.element_body(part).map_err(|e| {
                ModelError::InvalidValue(format!("Болты стыка не размещаются в накладке: {e}"))
            })?;
        }
        for part in [instance.primary, recipe.second] {
            project.element_body(part).map_err(|e| {
                ModelError::InvalidValue(format!("Болты стыка не размещаются в балке {part}: {e}"))
            })?;
        }
        project.connections.insert(instance.id, instance);
    }
    Ok(())
}
