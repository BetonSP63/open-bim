//! Seat («опорный столик»): a horizontal plate under the bottom flange of the
//! beam end, welded to the support face and stiffened by a triangular plate
//! below it; two bolts fix the beam flange to the seat.
use crate::end_plate::local_frame;
use crate::{child_id, instance_changed, retain_children};
use open_bim_model::{
    ConnectionInstance, Detail, DetailKind, Element, ElementShape, Frame, LengthMm, ModelError,
    ObjectId, Point3, ProfileShape, Project, SEAT_ROLES, TemplateKind, WeldSides, WeldType,
};
use std::collections::BTreeSet;

#[derive(Clone, Debug)]
pub struct SeatGeometry {
    pub seat: ElementShape,
    pub stiffener: ElementShape,
    /// Bolt axis up through the seat and the flange; the two bolts are
    /// `spacing` apart along its x.
    pub bolt_frame: Frame,
    pub spacing: f64,
    pub hole_depth: f64,
    /// Seat edge on the support face; stiffener edge on the support face.
    pub welds: [Vec<Point3>; 2],
    pub material: ObjectId,
    pub support: ObjectId,
}

pub fn seat_geometry(
    project: &Project,
    instance: &ConnectionInstance,
) -> Result<SeatGeometry, ModelError> {
    let template = &instance.template;
    template.validate()?;
    let TemplateKind::Seat {
        length,
        width,
        depth,
    } = template.kind
    else {
        return Err(ModelError::InvalidValue("Ожидается опорный столик".into()));
    };
    let invalid = |m: &str| Err(ModelError::InvalidValue(m.into()));
    let support = instance
        .secondary
        .ok_or_else(|| ModelError::InvalidValue("Столику нужна опора".into()))?;
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
        return invalid("Столик ставится под балку");
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
        return invalid("Столик ставится под двутавровую балку");
    };
    let len = beam_length.value();
    if length > len / 2. + 0.001 {
        return invalid("Столик длиннее половины балки");
    }
    if width < b - 0.001 {
        return invalid("Столик уже полки балки");
    }
    let spacing = b - 2. * template.edge_distance;
    if spacing <= template.bolt_diameter + template.clearance {
        return invalid("Полка балки узка для двух болтов при заданном расстоянии до края");
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
    let under = bottom - t;
    crate::touch_support_face(
        project,
        support,
        at(0., bottom - t / 2., end),
        outward,
        "Конец балки не доходит до грани опоры: столик приваривается к опоре",
    )?;
    let thickness = |v: f64| LengthMm::new(v).map_err(|e| ModelError::InvalidValue(e.to_string()));
    // Seat: x across, y' = −z of the beam, thickness up.
    let horizontal = [xa, neg(za), ya];
    let (z0, z1) = (end.min(inner), end.max(inner));
    let seat_frame = Frame {
        origin: at(0., under, 0.),
        axes: horizontal,
    };
    seat_frame.validate()?;
    let seat = ElementShape::Plate {
        contour: vec![
            [-width / 2., -z1],
            [width / 2., -z1],
            [width / 2., -z0],
            [-width / 2., -z0],
        ],
        thickness: thickness(t)?,
        frame: seat_frame,
    };
    // Stiffener in the web plane: (u, v) = (y, z) of the beam.
    let mut triangle = vec![[under, end], [under - depth, end], [under, inner]];
    let signed: f64 = (0..3)
        .map(|i| {
            let (p, q) = (triangle[i], triangle[(i + 1) % 3]);
            p[0] * q[1] - q[0] * p[1]
        })
        .sum();
    if signed < 0. {
        triangle.reverse();
    }
    let stiffener_frame = Frame {
        origin: at(-t / 2., 0., 0.),
        axes: [ya, za, xa],
    };
    stiffener_frame.validate()?;
    let stiffener = ElementShape::Plate {
        contour: triangle,
        thickness: thickness(t)?,
        frame: stiffener_frame,
    };
    let middle = (end + inner) / 2.;
    let bolt_frame = Frame {
        origin: at(-spacing / 2., under - 1., middle),
        axes: horizontal,
    };
    bolt_frame.validate()?;
    let welds = [
        vec![at(-width / 2., under, end), at(width / 2., under, end)],
        vec![at(0., under, end), at(0., under - depth, end)],
    ];
    Ok(SeatGeometry {
        seat,
        stiffener,
        bolt_frame,
        spacing,
        hole_depth: t + tf + 2.,
        welds,
        material: beam.material,
        support,
    })
}

pub fn rebuild_seats(project: &mut Project, previous: &Project) -> Result<(), ModelError> {
    let instances: Vec<_> = project
        .connections
        .values()
        .filter(|c| matches!(c.template.kind, TemplateKind::Seat { .. }))
        .cloned()
        .collect();
    for mut instance in instances {
        if !instance_changed(project, previous, &instance) {
            continue;
        }
        let recipe = match seat_geometry(project, &instance) {
            Ok(recipe) => recipe,
            Err(error) => {
                crate::keep_failed(project, previous, instance, error)?;
                continue;
            }
        };
        retain_children(project, &mut instance, &SEAT_ROLES);
        let t = instance.template.clone();
        let seat = child_id(project, &mut instance, "seat-plate")?;
        let stiffener = child_id(project, &mut instance, "stiffener")?;
        for (id, shape, what) in [
            (seat, recipe.seat, "столик"),
            (stiffener, recipe.stiffener, "ребро столика"),
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
        let mut holes = BTreeSet::new();
        for i in 0..2 {
            let origin = recipe
                .bolt_frame
                .world(Point3([recipe.spacing * i as f64, 0., 0.]));
            for (role, part) in [("seat-hole", seat), ("beam-hole", instance.primary)] {
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
        let bolts = child_id(project, &mut instance, "bolts")?;
        project.details.insert(
            bolts,
            Detail {
                id: bolts,
                kind: DetailKind::BoltGroup {
                    parts: BTreeSet::from([seat, instance.primary]),
                    holes,
                    frame: recipe.bolt_frame,
                    diameter: t.bolt_diameter,
                    clearance: t.clearance,
                    steps_x: vec![recipe.spacing],
                    steps_y: vec![],
                    standard: t.bolt_standard.clone(),
                    grade: t.bolt_grade.clone(),
                    points: Vec::new(),
                },
            },
        );
        let parts: [BTreeSet<ObjectId>; 2] = [
            [recipe.support, seat].into(),
            [recipe.support, seat, stiffener].into(),
        ];
        for ((role, parts), path) in ["weld-seat", "weld-stiffener"]
            .into_iter()
            .zip(parts)
            .zip(recipe.welds)
        {
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
        for part in [seat, instance.primary] {
            project.element_body(part).map_err(|e| {
                ModelError::InvalidValue(format!(
                    "Болты столика не размещаются в детали {part}: {e}"
                ))
            })?;
        }
        project.connections.insert(instance.id, instance);
    }
    Ok(())
}
