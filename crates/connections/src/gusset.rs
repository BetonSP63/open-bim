//! Brace gusset («фасонка связи»): a plate behind the vertical leg of an
//! angle brace, bolted to the leg with one row of bolts along the brace and
//! welded to the nearest side face of the support.
use crate::end_plate::{bounds, local_frame};
use crate::{child_id, instance_changed, retain_children};
use open_bim_model::{
    ConnectionInstance, Detail, DetailKind, Element, ElementShape, Frame, LengthMm, ModelError,
    ObjectId, Point3, ProfileShape, Project, TemplateKind, WeldSides, WeldType, gusset_roles,
};
use std::collections::BTreeSet;

/// Farthest the brace end may stop from the support face, mm.
pub const MAX_GUSSET_REACH: f64 = 1000.0;
/// Margin of the gusset around the brace leg, mm.
const MARGIN: f64 = 15.0;

#[derive(Clone, Debug)]
pub struct GussetGeometry {
    pub shape: ElementShape,
    pub material: ObjectId,
    pub bolt_frame: Frame,
    pub hole_depth: f64,
    pub bolts: u32,
    pub pitch: f64,
    pub weld_path: Vec<Point3>,
    pub support: ObjectId,
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| a[i] * b[i]).sum()
}

/// Convex hull, counter-clockwise, without collinear points.
fn hull(mut points: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    points.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    points.dedup_by(|a, b| (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9);
    let cross = |o: [f64; 2], a: [f64; 2], b: [f64; 2]| {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    };
    let mut lower: Vec<[f64; 2]> = Vec::new();
    for p in &points {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], *p) <= 1e-9
        {
            lower.pop();
        }
        lower.push(*p);
    }
    let mut upper: Vec<[f64; 2]> = Vec::new();
    for p in points.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], *p) <= 1e-9
        {
            upper.pop();
        }
        upper.push(*p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

pub fn gusset_geometry(
    project: &Project,
    instance: &ConnectionInstance,
) -> Result<GussetGeometry, ModelError> {
    let template = &instance.template;
    template.validate()?;
    let TemplateKind::Gusset { bolts, pitch } = template.kind else {
        return Err(ModelError::InvalidValue("Ожидается фасонка связи".into()));
    };
    let invalid = |m: &str| Err(ModelError::InvalidValue(m.into()));
    let support = instance
        .secondary
        .ok_or_else(|| ModelError::InvalidValue("Фасонке нужна опора".into()))?;
    if support == instance.primary {
        return invalid("Родители узла должны различаться");
    }
    let brace = project
        .elements
        .get(&instance.primary)
        .ok_or(ModelError::MissingReference(instance.primary))?;
    let ElementShape::Member {
        profile,
        length,
        frame: af,
        ..
    } = &brace.shape
    else {
        return invalid("Фасонка ставится на связь из уголка");
    };
    af.validate()?;
    let ProfileShape::Angle {
        height: h,
        width: w,
        thickness: ta,
    } = project
        .profiles
        .get(profile)
        .ok_or(ModelError::MissingReference(*profile))?
        .shape
    else {
        return invalid("Фасонка ставится на связь из уголка");
    };
    let len = length.value();
    let t = template.thickness;
    let edge = template.edge_distance;
    let hole = template.bolt_diameter + template.clearance;
    // Bolts on the middle of the free part of the vertical leg.
    let y_bolt = ta / 2.;
    if (h - ta) / 2. < hole / 2. + 5. {
        return invalid("Полка уголка узкая для болтов");
    }
    let row = 2. * edge + pitch * (bolts as f64 - 1.);
    if row > len / 2. + 0.001 {
        return invalid("Ряд болтов длиннее половины связи");
    }
    let [xa, ya, za] = af.axes;
    let (end, z_first) = if instance.at_end {
        (len, len - edge - pitch * (bolts as f64 - 1.))
    } else {
        (0., edge)
    };
    let far = if instance.at_end { len - row } else { row };
    // The support face the gusset reaches: a side face perpendicular to the
    // gusset plane, nearest the brace end and in front of it.
    let host = project
        .elements
        .get(&support)
        .ok_or(ModelError::MissingReference(support))?;
    let hf = *host.shape.frame();
    hf.validate()?;
    let extent = bounds(project, &host.shape)?;
    let tip = af.world(Point3([0., 0., end]));
    let tip_local = local_frame(Frame { origin: tip, ..hf }, hf).origin.0;
    let mut face: Option<(f64, usize, usize, f64)> = None;
    for axis in 0..2 {
        if dot(hf.axes[axis], xa).abs() > 1e-6 {
            continue;
        }
        for (side, sign) in [(0, -1.), (1, 1.)] {
            let distance = sign * (tip_local[axis] - extent[axis][side]);
            if distance > 0.001 && face.is_none_or(|(d, ..)| distance < d) {
                face = Some((distance, axis, side, sign));
            }
        }
    }
    let Some((_, axis, side, sign)) = face else {
        return invalid("У опоры нет грани поперёк плоскости фасонки перед концом связи");
    };
    let normal = hf.axes[axis].map(|v| v * sign);
    let mut on_face = [0.; 3];
    on_face[axis] = extent[axis][side];
    let q = hf.world(Point3(on_face));
    // In the gusset plane, coordinates (u, v) = (z, y) of the brace.
    let (nu, nv) = (dot(normal, za), dot(normal, ya));
    let d = dot(normal, std::array::from_fn(|i| q.0[i] - af.origin.0[i]));
    let f = |p: [f64; 2]| nu * p[0] + nv * p[1] - d;
    let (z_lo, z_hi) = (end.min(far), end.max(far));
    let corners = [
        [z_lo, -h / 2. - MARGIN],
        [z_hi, -h / 2. - MARGIN],
        [z_hi, h / 2. + MARGIN],
        [z_lo, h / 2. + MARGIN],
    ];
    if corners.iter().any(|c| f(*c) < 0.001) {
        return invalid("Конец связи заходит за грань опоры");
    }
    if f([end, 0.]) > MAX_GUSSET_REACH {
        return invalid("Конец связи дальше 1 м от опоры");
    }
    let projected: Vec<[f64; 2]> = corners
        .iter()
        .map(|c| [c[0] - f(*c) * nu, c[1] - f(*c) * nv])
        .collect();
    let mut points = corners.to_vec();
    points.extend(projected.iter().copied());
    let contour = hull(points);
    // The weld runs along the face between the extreme projections.
    let along = [-nv, nu];
    let s = |p: &[f64; 2]| p[0] * along[0] + p[1] * along[1];
    let (a, b) = projected
        .iter()
        .fold((projected[0], projected[0]), |(lo, hi), p| {
            (
                if s(p) < s(&lo) { *p } else { lo },
                if s(p) > s(&hi) { *p } else { hi },
            )
        });
    let x_mid = -w / 2. - t / 2.;
    let weld_path = vec![
        af.world(Point3([x_mid, a[1], a[0]])),
        af.world(Point3([x_mid, b[1], b[0]])),
    ];
    let plate_frame = Frame {
        origin: af.world(Point3([-w / 2., 0., 0.])),
        axes: [za, ya, xa.map(|v| -v)],
    };
    plate_frame.validate()?;
    let bolt_frame = Frame {
        origin: af.world(Point3([-w / 2. - t - 1., y_bolt, z_first])),
        axes: [ya, za, xa],
    };
    bolt_frame.validate()?;
    Ok(GussetGeometry {
        shape: ElementShape::Plate {
            contour,
            thickness: LengthMm::new(t).map_err(|e| ModelError::InvalidValue(e.to_string()))?,
            frame: plate_frame,
        },
        material: brace.material,
        bolt_frame,
        hole_depth: t + ta + 2.,
        bolts,
        pitch,
        weld_path,
        support,
    })
}

pub fn rebuild_gussets(project: &mut Project, previous: &Project) -> Result<(), ModelError> {
    let instances: Vec<_> = project
        .connections
        .values()
        .filter(|c| matches!(c.template.kind, TemplateKind::Gusset { .. }))
        .cloned()
        .collect();
    for mut instance in instances {
        if !instance_changed(project, previous, &instance) {
            continue;
        }
        let recipe = match gusset_geometry(project, &instance) {
            Ok(recipe) => recipe,
            Err(error) => {
                crate::keep_failed(project, previous, instance, error)?;
                continue;
            }
        };
        let roles = gusset_roles(recipe.bolts);
        let refs: Vec<&str> = roles.iter().map(String::as_str).collect();
        retain_children(project, &mut instance, &refs);
        let plate = child_id(project, &mut instance, "gusset")?;
        let bolts = child_id(project, &mut instance, "bolts")?;
        let weld = child_id(project, &mut instance, "weld")?;
        let t = instance.template.clone();
        project.elements.insert(
            plate,
            Element {
                id: plate,
                name: format!("{} — фасонка", t.name),
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
        for i in 0..recipe.bolts {
            let origin = recipe
                .bolt_frame
                .world(Point3([0., recipe.pitch * i as f64, 0.]));
            for (role, part) in [("gusset-hole", plate), ("brace-hole", instance.primary)] {
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
                    steps_y: vec![recipe.pitch; recipe.bolts as usize - 1],
                    standard: t.bolt_standard.clone(),
                    grade: t.bolt_grade.clone(),
                    points: Vec::new(),
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
                    process: Default::default(),
                    gost: Default::default(),
                },
            },
        );
        for part in [plate, instance.primary] {
            project.element_body(part).map_err(|e| {
                ModelError::InvalidValue(format!(
                    "Болты фасонки не размещаются в детали {part}: {e}"
                ))
            })?;
        }
        project.connections.insert(instance.id, instance);
    }
    Ok(())
}
