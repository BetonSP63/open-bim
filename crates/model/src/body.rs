use crate::{DetailKind, ElementShape, ModelError, ObjectId, Point3, ProfileShape, Project};
use open_bim_geometry::Body;

impl Project {
    /// Exact local part body. Placement remains in the element frame.
    pub fn element_body(&self, id: ObjectId) -> Result<Body, ModelError> {
        let element = self
            .elements
            .get(&id)
            .ok_or(ModelError::MissingReference(id))?;
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
                if z[0].abs() > 1e-9
                    || z[1].abs() > 1e-9
                    || frame.origin.0[2].min(end) > 0.0
                    || frame.origin.0[2].max(end) < length
                {
                    return Err(ModelError::InvalidValue(
                        "Непродольное или глухое отверстие требует операции сверления тела".into(),
                    ));
                }
                holes.push(([frame.origin.0[0], frame.origin.0[1]], diameter / 2.0));
            }
        }
        let geometry_error =
            |e: open_bim_geometry::BodyError| ModelError::InvalidValue(e.to_string());
        let mut body = match hollow {
            Some(shape) => {
                if !holes.is_empty() {
                    return Err(ModelError::InvalidValue(
                        "Сверление стенки полого профиля требует поперечного инструмента".into(),
                    ));
                }
                match *shape {
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
                    _ => unreachable!(),
                }
                .map_err(geometry_error)?
            }
            None => Body::prism_with_holes(&contour, length, &holes, Point3([0.0; 3]))
                .map_err(geometry_error)?,
        };
        for detail in details {
            body = match &detail.kind {
                DetailKind::PlaneCut { normal, offset, .. } => {
                    body.clip_plane(*normal, *offset).map_err(geometry_error)?
                }
                DetailKind::BoxCut { origin, size, .. } => {
                    let tool = Body::cuboid(*size, *origin).map_err(geometry_error)?;
                    body.difference(&tool).map_err(geometry_error)?
                }
                _ => body,
            };
        }
        Ok(body)
    }
}
