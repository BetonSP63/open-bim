//! Custom components («свои компоненты»): plates, bolt groups and welds at
//! the end of the main part, sized by the formulas of their definition.
use crate::end_plate::local_frame;
use crate::{child_id, instance_changed, retain_children};
use open_bim_model::{
    BUILTIN_NAMES, ConnectionInstance, CustomDefinition, Detail, DetailKind, Element, ElementShape,
    Frame, LengthMm, ModelError, ObjectId, PlatePlane, Point3, ProfileShape, Project, TemplateKind,
    WeldSides, WeldTo, WeldType, formula,
};
use std::collections::{BTreeMap, BTreeSet};

/// (height, width, web, flange) of a section; hollow and angle sections give
/// their wall for web and flange, a solid bar its width and height.
pub fn section_sizes(shape: &ProfileShape) -> [f64; 4] {
    shape.sizes()
}

fn member_sizes(project: &Project, id: ObjectId) -> Option<([f64; 4], f64)> {
    let ElementShape::Member {
        profile, length, ..
    } = &project.elements.get(&id)?.shape
    else {
        return None;
    };
    Some((
        section_sizes(&project.profiles.get(profile)?.shape),
        length.value(),
    ))
}

/// Built-in values of `instance`: the main part's section and length, the
/// support's section and the template sizes.
pub fn builtins(project: &Project, instance: &ConnectionInstance) -> BTreeMap<String, f64> {
    let t = &instance.template;
    let ([h, b, tw, tf], len) = member_sizes(project, instance.primary).unwrap_or_default();
    let [hs, bs, ..] = instance
        .secondary
        .and_then(|s| member_sizes(project, s))
        .map(|(s, _)| s)
        .unwrap_or_default();
    BUILTIN_NAMES
        .into_iter()
        .zip([
            h,
            b,
            tw,
            tf,
            len,
            hs,
            bs,
            t.thickness,
            t.bolt_diameter,
            t.edge_distance,
        ])
        .map(|(k, v)| (k.to_string(), v))
        .collect()
}

#[derive(Clone, Debug)]
pub struct CustomPlateShape {
    pub name: String,
    pub shape: ElementShape,
    /// World points of the first contour side, for its weld.
    pub first_side: [Point3; 2],
    /// The plate frame's y runs against the contour's v (to keep the frame
    /// right-handed with the thickness direction).
    pub flipped: bool,
}

#[derive(Clone, Debug)]
pub struct CustomBoltShape {
    pub plate: usize,
    pub frame: Frame,
    pub steps_x: Vec<f64>,
    pub steps_y: Vec<f64>,
    /// Bolt centres in the bolt frame.
    pub points: Vec<[f64; 2]>,
    pub plate_thickness: f64,
}

#[derive(Clone, Debug)]
pub struct CustomGeometry {
    pub plates: Vec<CustomPlateShape>,
    pub bolts: Vec<CustomBoltShape>,
    pub values: BTreeMap<String, f64>,
    pub material: ObjectId,
    /// Thickest wall of the main part, for drilling through it.
    pub main_wall: f64,
}

fn definition(instance: &ConnectionInstance) -> Result<&CustomDefinition, ModelError> {
    match &instance.template.kind {
        TemplateKind::Custom { definition } => Ok(definition),
        _ => Err(ModelError::InvalidValue("Ожидается свой компонент".into())),
    }
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub fn custom_geometry(
    project: &Project,
    instance: &ConnectionInstance,
) -> Result<CustomGeometry, ModelError> {
    instance.template.validate()?;
    let def = definition(instance)?;
    let invalid = |m: String| ModelError::InvalidValue(m);
    if instance.secondary == Some(instance.primary) {
        return Err(invalid("Родители узла должны различаться".into()));
    }
    let main = project
        .elements
        .get(&instance.primary)
        .ok_or(ModelError::MissingReference(instance.primary))?;
    let ElementShape::Member { length, frame, .. } = &main.shape else {
        return Err(invalid("Свой компонент ставится на конец стержня".into()));
    };
    frame.validate()?;
    if let Some(s) = instance.secondary {
        project
            .elements
            .get(&s)
            .ok_or(ModelError::MissingReference(s))?;
    }
    let len = length.value();
    let values = def.values(&builtins(project, instance)).map_err(invalid)?;
    let f = |src: &str, what: &str| {
        formula::eval(src, &values).map_err(|e| invalid(format!("{what}: {e}")))
    };
    // End coordinates: x' across, y' up, z' into the part from its end.
    let [xa, ya, za] = frame.axes;
    let s = if instance.at_end { -1. } else { 1. };
    let dirs = [xa.map(|v| s * v), ya, za.map(|v| s * v)];
    let world = |p: [f64; 3]| {
        let z = if instance.at_end { len - p[2] } else { p[2] };
        frame.world(Point3([s * p[0], p[1], z]))
    };
    let mut plates = Vec::new();
    for p in &def.plates {
        let what = |part: &str| format!("{} — {part}", p.name);
        let offset = f(&p.offset, &what("положение"))?;
        let thickness = f(&p.thickness, &what("толщина"))?;
        let mut contour = Vec::new();
        for (i, [u, v]) in p.contour.iter().enumerate() {
            contour.push([
                f(u, &what(&format!("точка {}", i + 1)))?,
                f(v, &what(&format!("точка {}", i + 1)))?,
            ]);
        }
        // In-plane axes (u, v), thickness direction n, and the 3D point of
        // a contour point.
        let (u, v, n): (usize, usize, [f64; 3]) = match p.plane {
            PlatePlane::End => (0, 1, dirs[2].map(|c| -c)),
            PlatePlane::Web => (2, 1, dirs[0]),
            PlatePlane::Flange => (0, 2, dirs[1]),
        };
        let at = |q: [f64; 2]| {
            let mut p3 = [0.; 3];
            p3[u] = q[0];
            p3[v] = q[1];
            let normal = match p.plane {
                PlatePlane::End => 2,
                PlatePlane::Web => 0,
                PlatePlane::Flange => 1,
            };
            p3[normal] = offset;
            world(p3)
        };
        let (a, mut b) = (dirs[u], dirs[v]);
        let flip = (0..3).map(|i| cross(a, b)[i] * n[i]).sum::<f64>() < 0.;
        if flip {
            b = b.map(|c| -c);
        }
        let mut local: Vec<[f64; 2]> = contour
            .iter()
            .map(|q| [q[0], if flip { -q[1] } else { q[1] }])
            .collect();
        let area: f64 = (0..local.len())
            .map(|i| {
                let (p, q) = (local[i], local[(i + 1) % local.len()]);
                p[0] * q[1] - q[0] * p[1]
            })
            .sum();
        if area < 0. {
            local.reverse();
        }
        open_bim_model::polygon_area(&local)
            .map_err(|e| invalid(format!("{}: {e}", what("контур"))))?;
        let plate_frame = Frame {
            origin: at([0., 0.]),
            axes: [a, b, n],
        };
        plate_frame.validate()?;
        plates.push(CustomPlateShape {
            name: p.name.clone(),
            shape: ElementShape::Plate {
                contour: local,
                thickness: LengthMm::new(thickness)
                    .map_err(|e| invalid(format!("{}: {e}", what("толщина"))))?,
                frame: plate_frame,
            },
            first_side: [at(contour[0]), at(contour[1])],
            flipped: flip,
        });
    }
    let mut bolts = Vec::new();
    for (j, g) in def.bolts.iter().enumerate() {
        let what = |part: &str| format!("Болты {}: {part}", j + 1);
        let [x, y, sx, sy] = [
            (&g.x, "x"),
            (&g.y, "y"),
            (&g.spacing_x, "шаг x"),
            (&g.spacing_y, "шаг y"),
        ]
        .map(|(src, part)| f(src, &what(part)));
        let (x, y, sx, sy) = (x?, y?, sx?, sy?);
        let hole = instance.template.bolt_diameter + instance.template.clearance;
        if (g.count_x > 1 && sx <= hole) || (g.count_y > 1 && sy <= hole) {
            return Err(invalid(what("шаг меньше отверстия")));
        }
        let ElementShape::Plate {
            frame: pf,
            thickness,
            contour,
        } = &plates[g.plate].shape
        else {
            unreachable!()
        };
        // Rows run up the plate frame's y from the lowest one.
        let rows = f64::from(g.count_y - 1) * sy;
        let y0 = if plates[g.plate].flipped {
            -(y + rows)
        } else {
            y
        };
        let points: Vec<[f64; 2]> = (0..g.count_y)
            .flat_map(|r| (0..g.count_x).map(move |c| [f64::from(c) * sx, f64::from(r) * sy]))
            .collect();
        let inside = |q: [f64; 2]| {
            let mut inside = false;
            for i in 0..contour.len() {
                let (a, b) = (contour[i], contour[(i + 1) % contour.len()]);
                if (a[1] > q[1]) != (b[1] > q[1])
                    && q[0] < a[0] + (q[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0])
                {
                    inside = !inside;
                }
            }
            inside
        };
        let origin = [x, y0];
        if points
            .iter()
            .any(|p| !inside([origin[0] + p[0], origin[1] + p[1]]))
        {
            return Err(invalid(what("болт вне пластины")));
        }
        let bolt_frame = Frame {
            origin: pf.world(Point3([origin[0], origin[1], -1.])),
            axes: pf.axes,
        };
        bolts.push(CustomBoltShape {
            plate: g.plate,
            frame: bolt_frame,
            steps_x: vec![sx; g.count_x as usize - 1],
            steps_y: vec![sy; g.count_y as usize - 1],
            points,
            plate_thickness: thickness.value(),
        });
    }
    let [_, _, tw, tf] = member_sizes(project, instance.primary)
        .map(|(s, _)| s)
        .unwrap_or_default();
    Ok(CustomGeometry {
        plates,
        bolts,
        values,
        material: main.material,
        main_wall: tw.max(tf),
    })
}

pub fn rebuild_custom(project: &mut Project, previous: &Project) -> Result<(), ModelError> {
    let instances: Vec<_> = project
        .connections
        .values()
        .filter(|c| matches!(c.template.kind, TemplateKind::Custom { .. }))
        .cloned()
        .collect();
    for mut instance in instances {
        if !instance_changed(project, previous, &instance) {
            continue;
        }
        let recipe = match custom_geometry(project, &instance) {
            Ok(recipe) => recipe,
            Err(error) => {
                crate::keep_failed(project, previous, instance, error)?;
                continue;
            }
        };
        let def = definition(&instance)?.clone();
        let roles = def.roles();
        let roles: Vec<&str> = roles.iter().map(String::as_str).collect();
        retain_children(project, &mut instance, &roles);
        let t = instance.template.clone();
        let mut plate_ids = Vec::new();
        for (i, plate) in recipe.plates.iter().enumerate() {
            let id = child_id(project, &mut instance, &format!("plate-{i}"))?;
            project.elements.insert(
                id,
                Element {
                    id,
                    name: format!("{} — {}", t.name, plate.name),
                    material: recipe.material,
                    shape: plate.shape.clone(),
                    properties: project
                        .elements
                        .get(&id)
                        .map(|e| e.properties.clone())
                        .unwrap_or_default(),
                },
            );
            plate_ids.push(id);
        }
        for (i, (plate, spec)) in recipe.plates.iter().zip(&def.plates).enumerate() {
            let Some(to) = spec.weld else { continue };
            let other = match to {
                WeldTo::Main => instance.primary,
                WeldTo::Support => instance.secondary.ok_or_else(|| {
                    ModelError::InvalidValue(format!(
                        "Пластина «{}» приваривается к опоре, а опоры нет",
                        spec.name
                    ))
                })?,
            };
            let id = child_id(project, &mut instance, &format!("weld-{i}"))?;
            project.details.insert(
                id,
                Detail {
                    id,
                    kind: DetailKind::Weld {
                        parts: [plate_ids[i], other].into(),
                        weld_type: WeldType::Fillet,
                        size: t.weld_size,
                        path: plate.first_side.to_vec(),
                        shop: true,
                        sides: WeldSides::Both,
                        around: false,
                        process: Default::default(),
                        gost: Default::default(),
                    },
                },
            );
        }
        for (j, (g, spec)) in recipe.bolts.iter().zip(&def.bolts).enumerate() {
            let plate = plate_ids[g.plate];
            let mut holes = BTreeSet::new();
            let mut parts = BTreeSet::from([plate]);
            for (k, p) in g.points.iter().enumerate() {
                let centre = g.frame.world(Point3([p[0], p[1], 0.]));
                let mut drill = vec![(
                    format!("hole-{j}-{k}"),
                    plate,
                    centre,
                    g.plate_thickness + 2.,
                )];
                if spec.through_main {
                    // From behind the plate through the main part's wall,
                    // on either side of the plate.
                    let back = recipe.main_wall + 2.;
                    let start = Point3(std::array::from_fn(|i| {
                        centre.0[i] - g.frame.axes[2][i] * back
                    }));
                    drill.push((
                        format!("main-hole-{j}-{k}"),
                        instance.primary,
                        start,
                        g.plate_thickness + 2. * back + 2.,
                    ));
                    parts.insert(instance.primary);
                }
                for (role, part, origin, depth) in drill {
                    let id = child_id(project, &mut instance, &role)?;
                    let frame = local_frame(
                        Frame { origin, ..g.frame },
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
                                depth,
                            },
                        },
                    );
                    holes.insert(id);
                }
            }
            let id = child_id(project, &mut instance, &format!("bolts-{j}"))?;
            project.details.insert(
                id,
                Detail {
                    id,
                    kind: DetailKind::BoltGroup {
                        parts,
                        holes,
                        frame: g.frame,
                        diameter: t.bolt_diameter,
                        clearance: t.clearance,
                        steps_x: g.steps_x.clone(),
                        steps_y: g.steps_y.clone(),
                        standard: t.bolt_standard.clone(),
                        grade: t.bolt_grade.clone(),
                        points: Vec::new(),
                    },
                },
            );
        }
        for part in plate_ids.iter().copied().chain([instance.primary]) {
            project.element_body(part).map_err(|e| {
                ModelError::InvalidValue(format!("Свой компонент не строится в детали {part}: {e}"))
            })?;
        }
        project.connections.insert(instance.id, instance);
    }
    Ok(())
}
