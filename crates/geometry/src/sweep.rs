use super::{Body, BodyError, Point3, TOLERANCE_MM, guarded, segments_cross};
use truck_modeling::{Solid, Vector3, Wire, builder};

fn contour_properties(contour: &[[f64; 2]]) -> Result<(Vec<[f64; 2]>, f64, f64), BodyError> {
    if contour.len() < 3
        || contour.len() > 10000
        || !contour
            .iter()
            .flatten()
            .all(|v| v.is_finite() && v.abs() <= 1e7)
    {
        return Err(BodyError::InvalidInput);
    }
    let mut twice_area = 0.0;
    let mut first_moment = 0.0;
    for i in 0..contour.len() {
        let a = contour[i];
        let b = contour[(i + 1) % contour.len()];
        if (a[0] - b[0]).hypot(a[1] - b[1]) <= TOLERANCE_MM * 10.0 {
            return Err(BodyError::DegenerateContour);
        }
        for j in (i + 1)..contour.len() {
            if j != i + 1
                && !(i == 0 && j == contour.len() - 1)
                && segments_cross(a, b, contour[j], contour[(j + 1) % contour.len()])
            {
                return Err(BodyError::DegenerateContour);
            }
        }
        let cross = a[0] * b[1] - b[0] * a[1];
        twice_area += cross;
        first_moment += (a[0] + b[0]) * cross;
    }
    if twice_area.abs() <= TOLERANCE_MM * TOLERANCE_MM {
        return Err(BodyError::DegenerateContour);
    }
    let centroid = first_moment / (3.0 * twice_area);
    let mut ordered = contour.to_vec();
    if twice_area < 0.0 {
        ordered.reverse();
    }
    Ok((ordered, twice_area.abs() / 2.0, centroid))
}

fn valid_origin(origin: Point3) -> bool {
    origin.0.iter().all(|v| v.is_finite() && v.abs() <= 1e9)
}

// Conservative test of the XY envelopes of two swept segments. Overlapping
// envelopes are rejected even if a concave section leaves empty space there.
fn polygons_overlap(a: &[[f64; 2]; 4], b: &[[f64; 2]; 4]) -> bool {
    fn inside(p: [f64; 2], polygon: &[[f64; 2]; 4]) -> bool {
        let mut inside = false;
        for i in 0..4 {
            let a = polygon[i];
            let b = polygon[(i + 1) % 4];
            if (a[1] > p[1]) != (b[1] > p[1])
                && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
            {
                inside = !inside;
            }
        }
        inside
    }
    (0..4).any(|i| (0..4).any(|j| segments_cross(a[i], a[(i + 1) % 4], b[j], b[(j + 1) % 4])))
        || inside(a[0], b)
        || inside(b[0], a)
}

fn convex_envelope(p: &[[f64; 2]; 4]) -> bool {
    let turns: Vec<_> = (0..4)
        .map(|i| {
            let a = p[i];
            let b = p[(i + 1) % 4];
            let c = p[(i + 2) % 4];
            (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0])
        })
        .collect();
    turns.iter().all(|x| *x > 0.0) || turns.iter().all(|x| *x < 0.0)
}

impl Body {
    /// Sweeps a closed section around an arc in local XY. Section coordinates are
    /// radial offset and height along Z. The arc must not cross the rotation axis.
    pub fn sweep_arc(
        contour: &[[f64; 2]],
        center: [f64; 2],
        radius: f64,
        start_angle: f64,
        sweep_angle: f64,
        origin: Point3,
    ) -> Result<Self, BodyError> {
        let (mut contour, area, centroid) = contour_properties(contour)?;
        if !valid_origin(origin)
            || !center.iter().all(|v| v.is_finite() && v.abs() <= 1e7)
            || !radius.is_finite()
            || radius > 1e7
            || radius <= TOLERANCE_MM * 10.0
            || !start_angle.is_finite()
            || !sweep_angle.is_finite()
            || sweep_angle.abs() <= 1e-9
            || sweep_angle.abs() >= std::f64::consts::TAU
            || contour.iter().any(|p| radius + p[0] <= TOLERANCE_MM * 10.0)
        {
            return Err(BodyError::InvalidInput);
        }
        if sweep_angle > 0.0 {
            contour.reverse();
        }
        guarded(|| {
            let (sin, cos) = start_angle.sin_cos();
            let vertices: Vec<_> = contour
                .iter()
                .map(|p| {
                    builder::vertex(truck_modeling::Point3::new(
                        center[0] + (radius + p[0]) * cos,
                        center[1] + (radius + p[0]) * sin,
                        p[1],
                    ))
                })
                .collect();
            let wire: Wire = (0..vertices.len())
                .map(|i| builder::line(&vertices[i], &vertices[(i + 1) % vertices.len()]))
                .collect();
            let face = builder::try_attach_plane(&[wire]).map_err(|_| BodyError::KernelFailure)?;
            let solid: Solid = builder::rsweep(
                &face,
                truck_modeling::Point3::new(center[0], center[1], 0.0),
                Vector3::new(0.0, 0.0, 1.0),
                truck_modeling::Rad(sweep_angle),
            );
            let mut body = Self::validated(solid, origin)?;
            body.exact_volume = Some(area * (radius + centroid) * sweep_angle.abs());
            Ok(body)
        })
    }
}

impl Body {
    /// Sweeps a section along a planar polyline. At corners the sections are
    /// mitred on the angle bisector; non-planar side quads are triangulated.
    pub fn sweep_polyline(
        contour: &[[f64; 2]],
        path: &[[f64; 2]],
        origin: Point3,
    ) -> Result<Self, BodyError> {
        let (contour, _, _) = contour_properties(contour)?;
        if !valid_origin(origin)
            || path.len() < 2
            || path.len() > 1000
            || path.len().saturating_mul(contour.len()) > 50_000
            || !path
                .iter()
                .flatten()
                .all(|v| v.is_finite() && v.abs() <= 1e7)
        {
            return Err(BodyError::InvalidInput);
        }
        let mut directions = Vec::new();
        let mut lengths = Vec::new();
        for pair in path.windows(2) {
            let delta = [pair[1][0] - pair[0][0], pair[1][1] - pair[0][1]];
            let length = delta[0].hypot(delta[1]);
            if length <= TOLERANCE_MM * 10.0 {
                return Err(BodyError::InvalidInput);
            }
            directions.push([delta[0] / length, delta[1] / length]);
            lengths.push(length);
        }
        for i in 0..path.len() - 1 {
            for j in i + 2..path.len() - 1 {
                if segments_cross(path[i], path[i + 1], path[j], path[j + 1]) {
                    return Err(BodyError::InvalidInput);
                }
            }
        }
        let normals: Vec<_> = directions.iter().map(|d| [-d[1], d[0]]).collect();
        let extent = contour.iter().map(|p| p[0].abs()).fold(0.0, f64::max);
        let mut offsets = vec![normals[0]];
        for i in 1..path.len() - 1 {
            let n = [
                normals[i - 1][0] + normals[i][0],
                normals[i - 1][1] + normals[i][1],
            ];
            let dot = n[0] * normals[i][0] + n[1] * normals[i][1];
            if dot <= 1e-6 {
                return Err(BodyError::InvalidInput);
            }
            let miter = [n[0] / dot, n[1] / dot];
            let along = (miter[0] * directions[i][0] + miter[1] * directions[i][1]).abs() * extent;
            if along * 2.0 >= lengths[i - 1].min(lengths[i]) {
                return Err(BodyError::InvalidInput);
            }
            offsets.push(miter);
        }
        offsets.push(*normals.last().ok_or(BodyError::InvalidInput)?);
        let (minimum, maximum) = contour
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                (lo.min(p[0]), hi.max(p[0]))
            });
        let envelopes: Vec<[[f64; 2]; 4]> = (0..path.len() - 1)
            .map(|i| {
                let at = |j: usize, x: f64| {
                    [
                        path[j][0] + offsets[j][0] * x,
                        path[j][1] + offsets[j][1] * x,
                    ]
                };
                [
                    at(i, minimum),
                    at(i + 1, minimum),
                    at(i + 1, maximum),
                    at(i, maximum),
                ]
            })
            .collect();
        if envelopes.iter().any(|envelope| !convex_envelope(envelope)) {
            return Err(BodyError::InvalidInput);
        }
        for i in 0..envelopes.len() {
            for j in i + 2..envelopes.len() {
                if polygons_overlap(&envelopes[i], &envelopes[j]) {
                    return Err(BodyError::InvalidInput);
                }
            }
        }
        guarded(|| {
            let cap = crate::prism_mesh::prism_mesh(&contour, 1.0, &[])?;
            let count = contour.len();
            let coords: Vec<_> = path
                .iter()
                .zip(&offsets)
                .flat_map(|(p, n)| {
                    contour
                        .iter()
                        .map(move |c| [p[0] + n[0] * c[0], p[1] + n[1] * c[0], c[1]])
                })
                .collect();
            let mut triangles = Vec::new();
            for triangle in cap.faces().triangle_iter().map(|f| f.map(|v| v.pos)) {
                if triangle.iter().all(|i| *i < count) {
                    triangles.push(triangle);
                    triangles.push([
                        triangle[2] + (path.len() - 1) * count,
                        triangle[1] + (path.len() - 1) * count,
                        triangle[0] + (path.len() - 1) * count,
                    ]);
                }
            }
            for k in 0..path.len() - 1 {
                for i in 0..count {
                    let a = k * count + i;
                    let b = k * count + (i + 1) % count;
                    triangles.extend([[a, b, b + count], [a, b + count, a + count]]);
                }
            }
            let vertices: Vec<_> = coords
                .iter()
                .map(|p| builder::vertex(truck_modeling::Point3::new(p[0], p[1], p[2])))
                .collect();
            let mut edges =
                std::collections::BTreeMap::<(usize, usize), truck_modeling::Edge>::new();
            let mut shell = truck_modeling::Shell::new();
            for triangle in triangles {
                let mut wire = Wire::new();
                for i in 0..3 {
                    let a = triangle[i];
                    let b = triangle[(i + 1) % 3];
                    if let Some(edge) = edges.get(&(b, a)) {
                        wire.push_back(edge.inverse());
                    } else {
                        let edge = builder::line(&vertices[a], &vertices[b]);
                        edges.insert((a, b), edge.clone());
                        wire.push_back(edge);
                    }
                }
                shell.push(
                    builder::try_attach_plane(&[wire]).map_err(|_| BodyError::KernelFailure)?,
                );
            }
            Self::validated(Solid::new(vec![shell]), origin)
        })
    }
}
