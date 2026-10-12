use crate::BodyError;
use spade::{ConstrainedDelaunayTriangulation, HasPosition, Point2, Triangulation};
use truck_meshalgo::prelude::*;

#[derive(Clone, Copy)]
struct Vertex {
    position: Point2<f64>,
    index: usize,
}
impl HasPosition for Vertex {
    type Scalar = f64;
    fn position(&self) -> Point2<f64> {
        self.position
    }
}

pub(crate) fn blind_prism_mesh(
    contour: &[[f64; 2]],
    length: f64,
    radius: f64,
    floor: f64,
) -> Result<PolygonMesh, BodyError> {
    let mut contour = contour.to_vec();
    let signed: f64 = (0..contour.len())
        .map(|i| {
            let a = contour[i];
            let b = contour[(i + 1) % contour.len()];
            a[0] * b[1] - b[0] * a[1]
        })
        .sum();
    if signed < 0.0 {
        contour.reverse();
    }
    let through = prism_mesh(&contour, length, &[([0.0, 0.0], radius)])?;
    let plain = prism_mesh(&contour, length, &[])?;
    let outer = contour.len();
    let total = through.positions().len() / 2;
    let mut positions = through.positions().to_vec();
    for p in &mut positions[outer..total] {
        p.z = floor;
    }
    let mut triangles: Vec<[usize; 3]> = through
        .faces()
        .triangle_iter()
        .map(|f| f.map(|v| v.pos))
        .filter(|t| !t.iter().all(|i| *i < total))
        .collect();
    triangles.extend(
        plain
            .faces()
            .triangle_iter()
            .map(|f| f.map(|v| v.pos))
            .filter(|t| t.iter().all(|i| *i < outer)),
    );
    let centre = positions.len();
    positions.push(truck_modeling::Point3::new(0.0, 0.0, floor));
    for i in outer..total {
        let next = if i + 1 == total { outer } else { i + 1 };
        triangles.push([centre, i, next]);
    }
    Ok(PolygonMesh::new(
        StandardAttributes {
            positions,
            ..Default::default()
        },
        Faces::from_iter(triangles.iter()),
    ))
}

pub(crate) fn prism_mesh(
    contour: &[[f64; 2]],
    length: f64,
    holes: &[([f64; 2], f64)],
) -> Result<PolygonMesh, BodyError> {
    prism_void_mesh(contour, length, holes, &[])
}

pub(crate) fn prism_void_mesh(
    contour: &[[f64; 2]],
    length: f64,
    holes: &[([f64; 2], f64)],
    voids: &[Vec<[f64; 2]>],
) -> Result<PolygonMesh, BodyError> {
    let area: f64 = (0..contour.len())
        .map(|i| {
            let a = contour[i];
            let b = contour[(i + 1) % contour.len()];
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        .abs()
        / 2.0;
    let circular_removed: f64 = holes
        .iter()
        .map(|(_, r)| std::f64::consts::PI * r * r)
        .sum();
    let removed = circular_removed + voids.iter().map(|v| polygon_area(v)).sum::<f64>();
    if removed >= area {
        return Err(BodyError::InvalidVolume);
    }
    // Bound signed-volume error of the inscribed circular display polygon.
    // Exact B-rep circles are independent of this tessellation.
    let sagitta_segments = holes
        .iter()
        .map(|(_, radius)| {
            (std::f64::consts::PI
                / (1.0 - crate::TOLERANCE_MM * 0.5 / radius)
                    .clamp(-1.0, 1.0)
                    .acos())
            .ceil() as usize
        })
        .max()
        .unwrap_or(0);
    let n = (256_usize).max(sagitta_segments).max(
        (2.0 * std::f64::consts::PI.powi(2) / 3.0 * circular_removed / (area - removed) / 5e-7)
            .sqrt()
            .ceil() as usize,
    );
    if n > 65536 || holes.len().saturating_mul(n) > 2_000_000 {
        return Err(BodyError::InvalidInput);
    }
    let mut points = contour.to_vec();
    let mut loops = vec![(0, contour.len())];
    for (centre, radius) in holes {
        loops.push((points.len(), n));
        points.extend((0..n).map(|i| {
            let a = std::f64::consts::TAU * i as f64 / n as f64;
            [centre[0] + radius * a.cos(), centre[1] + radius * a.sin()]
        }));
    }
    for v in voids {
        loops.push((points.len(), v.len()));
        points.extend(v);
    }
    if points.len() > 2_000_000 {
        return Err(BodyError::InvalidInput);
    }
    let mut cdt = ConstrainedDelaunayTriangulation::<Vertex>::new();
    let handles: Vec<_> = points
        .iter()
        .enumerate()
        .map(|(index, p)| {
            cdt.insert(Vertex {
                position: Point2::new(p[0], p[1]),
                index,
            })
            .map_err(|_| BodyError::KernelFailure)
        })
        .collect::<Result<_, _>>()?;
    for (start, count) in &loops {
        for i in 0..*count {
            let a = handles[start + i];
            let b = handles[start + (i + 1) % count];
            if !cdt.can_add_constraint(a, b) {
                return Err(BodyError::KernelFailure);
            }
            cdt.add_constraint(a, b);
        }
    }
    let inside_outer = |p: [f64; 2]| {
        let mut inside = false;
        for i in 0..contour.len() {
            let a = contour[i];
            let b = contour[(i + 1) % contour.len()];
            if (a[1] > p[1]) != (b[1] > p[1])
                && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
            {
                inside = !inside;
            }
        }
        inside
    };
    let inside_hole = |p: [f64; 2]| {
        holes.iter().any(|(centre, r)| {
            let dx = p[0] - centre[0];
            let dy = p[1] - centre[1];
            let step = std::f64::consts::TAU / n as f64;
            let a = dy.atan2(dx).rem_euclid(step) - step / 2.0;
            dx.hypot(dy) < r * (step / 2.0).cos() / a.cos()
        })
    };
    let total = points.len();
    let positions = [0.0, length]
        .into_iter()
        .flat_map(|z| {
            points
                .iter()
                .map(move |p| truck_modeling::Point3::new(p[0], p[1], z))
        })
        .collect();
    let mut triangles: Vec<[usize; 3]> = Vec::new();
    for face in cdt.inner_faces() {
        let ids = face.vertices().map(|v| v.data().index);
        let p: [f64; 2] =
            std::array::from_fn(|i| ids.iter().map(|v| points[*v][i]).sum::<f64>() / 3.0);
        if !inside_outer(p) || inside_hole(p) || voids.iter().any(|v| inside_polygon(p, v)) {
            continue;
        }
        triangles.push([ids[0], ids[2], ids[1]]);
        triangles.push(ids.map(|v| v + total));
    }
    for (loop_index, (start, count)) in loops.iter().enumerate() {
        for i in 0..*count {
            let a = start + i;
            let b = start + (i + 1) % count;
            if loop_index == 0 {
                triangles.extend([[a, b, b + total], [a, b + total, a + total]]);
            } else {
                triangles.extend([[a, a + total, b + total], [a, b + total, b]]);
            }
        }
    }
    Ok(PolygonMesh::new(
        StandardAttributes {
            positions,
            ..Default::default()
        },
        Faces::from_iter(triangles.iter()),
    ))
}

fn polygon_area(contour: &[[f64; 2]]) -> f64 {
    (0..contour.len())
        .map(|i| {
            let a = contour[i];
            let b = contour[(i + 1) % contour.len()];
            a[0] * b[1] - a[1] * b[0]
        })
        .sum::<f64>()
        .abs()
        / 2.0
}

fn inside_polygon(p: [f64; 2], contour: &[[f64; 2]]) -> bool {
    let mut inside = false;
    for i in 0..contour.len() {
        let a = contour[i];
        let b = contour[(i + 1) % contour.len()];
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            inside = !inside;
        }
    }
    inside
}
