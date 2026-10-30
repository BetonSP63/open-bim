use crate::{DetailKind, ElementShape, ModelError, ObjectId, Point3, ProfileShape, Project};
use open_bim_geometry::Body;

impl Project {
    /// Outer surface of a part body (holes and cuts included), mm²: the
    /// area to paint or galvanize.
    pub fn element_surface_mm2(&self, id: ObjectId) -> Result<f64, ModelError> {
        let (points, triangles) = self
            .element_body(id)?
            .tessellation_f64()
            .map_err(|e| ModelError::InvalidValue(e.to_string()))?;
        Ok(triangles
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|i| points[i as usize]);
                let u: [f64; 3] = std::array::from_fn(|k| b[k] - a[k]);
                let v: [f64; 3] = std::array::from_fn(|k| c[k] - a[k]);
                let n = [
                    u[1] * v[2] - u[2] * v[1],
                    u[2] * v[0] - u[0] * v[2],
                    u[0] * v[1] - u[1] * v[0],
                ];
                (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt() / 2.0
            })
            .sum())
    }

    /// World axis-aligned box [min, max] of a part: the corners of its
    /// local section box swept along the extrusion. Hollow sections use the
    /// outer box.
    pub fn element_bounds(&self, id: ObjectId) -> Result<[[f64; 3]; 2], ModelError> {
        let part = self
            .elements
            .get(&id)
            .ok_or(ModelError::MissingReference(id))?;
        if self.swept_parts.contains_key(&id) {
            let (points, _) = self
                .element_body(id)?
                .tessellation_f64()
                .map_err(|e| ModelError::InvalidValue(e.to_string()))?;
            let mut bounds = [[f64::INFINITY; 3], [f64::NEG_INFINITY; 3]];
            for point in points {
                let world = part.shape.frame().world(Point3(point));
                for i in 0..3 {
                    bounds[0][i] = bounds[0][i].min(world.0[i]);
                    bounds[1][i] = bounds[1][i].max(world.0[i]);
                }
            }
            return Ok(bounds);
        }
        let (points, depth) = match &part.shape {
            ElementShape::Plate {
                contour, thickness, ..
            } => (contour.clone(), thickness.value()),
            ElementShape::Member {
                profile, length, ..
            } => {
                let shape = &self
                    .profiles
                    .get(profile)
                    .ok_or(ModelError::MissingReference(*profile))?
                    .shape;
                let points = match *shape {
                    ProfileShape::CircularHollow { diameter, .. } => vec![
                        [-diameter / 2., -diameter / 2.],
                        [diameter / 2., diameter / 2.],
                    ],
                    ProfileShape::RectangularHollow { width, height, .. } => {
                        vec![[-width / 2., -height / 2.], [width / 2., height / 2.]]
                    }
                    ref shape @ (ProfileShape::Polygon { .. } | ProfileShape::Welded { .. }) => {
                        shape.outline()?.0
                    }
                    ref shape => shape.solid_contour()?,
                };
                (points, length.value())
            }
        };
        let fold = |axis: usize, f: fn(f64, f64) -> f64, start: f64| {
            points.iter().map(|p| p[axis]).fold(start, f)
        };
        let lo = [
            fold(0, f64::min, f64::INFINITY),
            fold(1, f64::min, f64::INFINITY),
        ];
        let hi = [
            fold(0, f64::max, f64::NEG_INFINITY),
            fold(1, f64::max, f64::NEG_INFINITY),
        ];
        let mut out = [[f64::INFINITY; 3], [f64::NEG_INFINITY; 3]];
        for x in [lo[0], hi[0]] {
            for y in [lo[1], hi[1]] {
                for z in [0., depth] {
                    let p = part.shape.frame().world(Point3([x, y, z]));
                    out[0] = std::array::from_fn(|i| out[0][i].min(p.0[i]));
                    out[1] = std::array::from_fn(|i| out[1][i].max(p.0[i]));
                }
            }
        }
        Ok(out)
    }

    /// Exact local part body. Placement remains in the element frame.
    pub fn element_body(&self, id: ObjectId) -> Result<Body, ModelError> {
        let element = self
            .elements
            .get(&id)
            .ok_or(ModelError::MissingReference(id))?;
        if let Some(swept) = self.swept_parts.get(&id) {
            swept.validate(&element.shape)?;
            if self.details.values().any(|d| d.kind.part() == Some(id)) {
                return Err(ModelError::InvalidValue(
                    "Обработка гнутых деталей пока не поддерживается".into(),
                ));
            }
            let err = |e: open_bim_geometry::BodyError| ModelError::InvalidValue(e.to_string());
            return match (swept, &element.shape) {
                (
                    crate::SweptPart::CurvedBeam {
                        center,
                        radius,
                        start_angle,
                        sweep_angle,
                    },
                    ElementShape::Member { profile, .. },
                ) => {
                    let shape = &self
                        .profiles
                        .get(profile)
                        .ok_or(ModelError::MissingReference(*profile))?
                        .shape;
                    Body::sweep_arc(
                        &shape.solid_contour()?,
                        *center,
                        *radius,
                        *start_angle,
                        *sweep_angle,
                        Point3([0.; 3]),
                    )
                    .map_err(err)
                }
                (
                    crate::SweptPart::PolyBeam {
                        points,
                        bend_radius,
                    },
                    ElementShape::Member { profile, .. },
                ) => {
                    let shape = &self
                        .profiles
                        .get(profile)
                        .ok_or(ModelError::MissingReference(*profile))?
                        .shape;
                    let path = crate::swept::rounded_path(points, *bend_radius)?;
                    Body::sweep_polyline(&shape.solid_contour()?, &path, Point3([0.; 3]))
                        .map_err(err)
                }
                (
                    crate::SweptPart::BentPlate {
                        width,
                        first_length,
                        second_length,
                        radius,
                        angle,
                    },
                    ElementShape::Plate { thickness, .. },
                ) => {
                    let contour = crate::swept::bent_plate_contour(
                        *first_length,
                        *second_length,
                        *radius,
                        *angle,
                        thickness.value(),
                    )?;
                    Body::prism(&contour, *width, Point3([0.; 3])).map_err(err)
                }
                _ => Err(ModelError::InvalidValue(
                    "Несовместимая форма гнутой детали".into(),
                )),
            };
        }
        let mut hollow = None;
        let (contour, length) = match &element.shape {
            ElementShape::Plate {
                contour, thickness, ..
            } => (contour.clone(), thickness.value()),
            ElementShape::Member {
                profile, length, ..
            } => {
                let shape = &self
                    .profiles
                    .get(profile)
                    .ok_or(ModelError::MissingReference(*profile))?
                    .shape;
                match shape {
                    ProfileShape::CircularHollow { .. }
                    | ProfileShape::RectangularHollow { .. } => {
                        hollow = Some(shape);
                        (Vec::new(), length.value())
                    }
                    ProfileShape::Polygon { .. } | ProfileShape::Welded { .. } => {
                        let (outer, voids) = shape.outline()?;
                        if !voids.is_empty() {
                            hollow = Some(shape);
                        }
                        (outer, length.value())
                    }
                    _ => (shape.solid_contour()?, length.value()),
                }
            }
        };
        let details: Vec<_> = self
            .details
            .values()
            .filter(|d| d.kind.part() == Some(id))
            .collect();
        let mut holes = Vec::new();
        let mut drilled = Vec::new();
        for detail in &details {
            if let DetailKind::Hole {
                frame,
                diameter,
                depth,
                ..
            } = &detail.kind
            {
                frame.validate()?;
                let z = frame.axes[2];
                let end = frame.origin.0[2] + z[2] * depth;
                if hollow.is_some()
                    || z[0].abs() > 1e-9
                    || z[1].abs() > 1e-9
                    || frame.origin.0[2].min(end) > 0.0
                    || frame.origin.0[2].max(end) < length
                {
                    drilled.push((frame, diameter, depth));
                    continue;
                }
                holes.push(([frame.origin.0[0], frame.origin.0[1]], diameter / 2.0));
            }
        }
        let geometry_error =
            |e: open_bim_geometry::BodyError| ModelError::InvalidValue(e.to_string());
        if hollow.is_none()
            && drilled.is_empty()
            && let Some(body) = uniform_cut_body(&contour, length, &holes, &details)
        {
            return Ok(body);
        }
        let mut body = match hollow {
            Some(shape) => match *shape {
                ProfileShape::CircularHollow {
                    diameter,
                    thickness,
                } => Body::tube(
                    diameter / 2.0,
                    diameter / 2.0 - thickness,
                    length,
                    Point3([0.0; 3]),
                ),
                ProfileShape::RectangularHollow {
                    width,
                    height,
                    thickness,
                } => Body::rectangular_tube(width, height, thickness, length, Point3([0.0; 3])),
                // Box-like user sections: the outline with its voids.
                ref shape => {
                    let voids = shape.outline()?.1;
                    Body::prism_with_voids(&contour, length, &[], &voids, Point3([0.0; 3]))
                }
            }
            .map_err(geometry_error)?,
            None => Body::prism_with_holes(&contour, length, &holes, Point3([0.0; 3]))
                .map_err(geometry_error)?,
        };
        for (frame, diameter, depth) in drilled {
            body = body
                .drill(frame.axes, frame.origin.0, diameter / 2.0, *depth)
                .map_err(geometry_error)?;
        }
        // Half-space clips and Boolean subtractions commute; clipping the
        // clean solid first keeps the kernel away from clipping Boolean output.
        let mut ordered = details.clone();
        ordered.sort_by_key(|d| !matches!(d.kind, DetailKind::PlaneCut { .. }));
        for detail in ordered {
            body = match &detail.kind {
                DetailKind::PlaneCut { normal, offset, .. } => {
                    body.clip_plane(*normal, *offset).map_err(geometry_error)?
                }
                DetailKind::BoxCut { origin, size, .. } => {
                    let tool = Body::cuboid(*size, *origin).map_err(geometry_error)?;
                    body.difference(&tool).map_err(geometry_error)?
                }
                DetailKind::PrismCut {
                    frame,
                    contour,
                    length,
                    ..
                } => {
                    let tool = Body::prism(contour, *length, Point3([0.0; 3]))
                        .and_then(|b| b.rigid_transform(frame.axes, frame.origin.0))
                        .map_err(geometry_error)?;
                    body.difference(&tool).map_err(geometry_error)?
                }
                _ => body,
            };
        }
        Ok(body)
    }
}

/// Set subtraction and half-space intersection commute. For a uniform section
/// with separate through voids, construct the resulting exact extrusion once.
/// Unsupported orientations or intersecting loops retain the general kernel path.
fn uniform_cut_body(
    contour: &[[f64; 2]],
    length: f64,
    holes: &[([f64; 2], f64)],
    details: &[&crate::Detail],
) -> Option<Body> {
    let mut contour = contour.to_vec();
    let mut voids = Vec::new();
    let mut changed = false;
    for detail in details {
        match &detail.kind {
            DetailKind::PlaneCut { normal, offset, .. } => {
                if normal[2].abs() > 1e-12 {
                    return None;
                }
                changed = true;
                let mut clipped = Vec::new();
                for i in 0..contour.len() {
                    let a = contour[i];
                    let b = contour[(i + 1) % contour.len()];
                    let da = normal[0] * a[0] + normal[1] * a[1] - offset;
                    let db = normal[0] * b[0] + normal[1] * b[1] - offset;
                    if da <= 0.0 {
                        clipped.push(a);
                    }
                    if (da < 0.0 && db > 0.0) || (da > 0.0 && db < 0.0) {
                        let t = da / (da - db);
                        clipped.push([a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]);
                    }
                }
                contour = clipped;
                if contour.len() < 3 {
                    return None;
                }
            }
            DetailKind::BoxCut { origin, size, .. } => {
                if origin.0[2] > 0.0 || origin.0[2] + size[2] < length {
                    return None;
                }
                changed = true;
                let [x, y, _] = origin.0;
                voids.push(vec![
                    [x, y],
                    [x + size[0], y],
                    [x + size[0], y + size[1]],
                    [x, y + size[1]],
                ]);
            }
            _ => {}
        }
    }
    if !changed {
        return None;
    }
    Body::prism_with_voids(&contour, length, holes, &voids, Point3([0.0; 3])).ok()
}
