use super::*;

#[derive(Clone, Debug)]
pub(super) struct FlatBore {
    pub start: [f64; 3],
    pub end: [f64; 3],
    pub radius: f64,
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| a[i] * b[i]).sum()
}
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}
fn unit(v: [f64; 3]) -> [f64; 3] {
    let n = dot(v, v).sqrt();
    v.map(|x| x / n)
}
fn basis(z: [f64; 3]) -> [[f64; 3]; 3] {
    let z = unit(z);
    let reference = if z[0].abs() < 0.9 {
        [1., 0., 0.]
    } else {
        [0., 1., 0.]
    };
    let x = unit(std::array::from_fn(|i| {
        reference[i] - z[i] * dot(reference, z)
    }));
    let y = [
        z[1] * x[2] - z[2] * x[1],
        z[2] * x[0] - z[0] * x[2],
        z[0] * x[1] - z[1] * x[0],
    ];
    [x, y, z]
}
fn inside(p: [f64; 2], ring: &[[f64; 2]]) -> bool {
    let mut yes = false;
    for i in 0..ring.len() {
        let a = ring[i];
        let b = ring[(i + 1) % ring.len()];
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            yes = !yes;
        }
    }
    yes
}
fn contains_circle(ring: &[[f64; 2]], c: [f64; 2], radius: f64) -> bool {
    inside(c, ring)
        && (0..ring.len()).all(|i| {
            let a = ring[i];
            let b = ring[(i + 1) % ring.len()];
            let d = [b[0] - a[0], b[1] - a[1]];
            let norm = d[0] * d[0] + d[1] * d[1];
            if norm <= TOLERANCE_MM * TOLERANCE_MM {
                return false;
            }
            let t = (((c[0] - a[0]) * d[0] + (c[1] - a[1]) * d[1]) / norm).clamp(0., 1.);
            (c[0] - a[0] - t * d[0]).hypot(c[1] - a[1] - t * d[1]) > radius + 10. * TOLERANCE_MM
        })
}
fn bounds(b: &FlatBore) -> [[f64; 2]; 3] {
    let z = unit(sub(b.end, b.start));
    std::array::from_fn(|i| {
        let r = b.radius * (1. - z[i] * z[i]).max(0.).sqrt();
        [b.start[i].min(b.end[i]) - r, b.start[i].max(b.end[i]) + r]
    })
}

impl Body {
    pub(super) fn drill_planar_slab(
        &self,
        axes: [[f64; 3]; 3],
        start: [f64; 3],
        radius: f64,
        depth: f64,
    ) -> Option<Result<Self, BodyError>> {
        let faces = self.planar_faces.as_ref()?;
        if self.solid.boundaries().len() != 1 {
            return None;
        }
        let local = |p: [f64; 3]| axes.map(|a| dot(a, sub(p, start)));
        let mut targets = Vec::new();
        for face in faces {
            let points: Vec<_> = face.iter().map(|p| local(*p)).collect();
            let z = points[0][2];
            let contour: Vec<_> = points.iter().map(|p| [p[0], p[1]]).collect();
            if points.iter().all(|p| (p[2] - z).abs() < TOLERANCE_MM)
                && contains_circle(&contour, [0., 0.], radius)
            {
                targets.push((z, face));
            }
        }
        if targets.len() != 2 {
            return None;
        }
        targets.sort_by(|a, b| a.0.total_cmp(&b.0));
        let lower = targets[0].0;
        let upper = targets[1].0;
        if lower < -TOLERANCE_MM || upper > depth + TOLERANCE_MM || upper - lower <= 0.001 {
            return None;
        }
        let bore = FlatBore {
            start: std::array::from_fn(|i| start[i] + axes[2][i] * lower),
            end: std::array::from_fn(|i| start[i] + axes[2][i] * upper),
            radius,
        };
        let bb = bounds(&bore);
        if self.planar_bores.iter().any(|old| {
            let ob = bounds(old);
            !(0..3)
                .any(|i| bb[i][1] < ob[i][0] - TOLERANCE_MM || ob[i][1] < bb[i][0] - TOLERANCE_MM)
        }) {
            return None;
        }
        Some(guarded(|| {
            let cylinder = Self::cylinder(radius, upper - lower, self.origin)?
                .rigid_transform(basis(axes[2]), bore.start)?;
            let cap_at = |f: &truck_modeling::Face, z: f64| {
                f.boundaries().iter().flat_map(|w| w.iter()).all(|e| {
                    let a = e.front().point();
                    let b = e.back().point();
                    (local([a.x, a.y, a.z])[2] - z).abs() < TOLERANCE_MM
                        && (local([b.x, b.y, b.z])[2] - z).abs() < TOLERANCE_MM
                })
            };
            let mut shell = truck_modeling::Shell::new();
            let mut replaced = 0;
            for face in &self.solid.boundaries()[0] {
                let boundaries = face.boundaries();
                let outer = boundaries.first().ok_or(BodyError::KernelFailure)?;
                let outer_points: Vec<_> = outer
                    .iter()
                    .map(|e| {
                        let p = e.front().point();
                        [p.x, p.y, p.z]
                    })
                    .collect();
                let target = targets.iter().find(|(_, ring)| {
                    outer_points.len() == ring.len()
                        && outer_points.iter().all(|p| {
                            ring.iter().any(|q| {
                                dot(sub(*p, *q), sub(*p, *q)) < TOLERANCE_MM * TOLERANCE_MM
                            })
                        })
                });
                let Some((z, _)) = target else {
                    shell.push(face.clone());
                    continue;
                };
                let cap = cylinder.solid.boundaries()[0]
                    .iter()
                    .find(|f| cap_at(f, *z))
                    .ok_or(BodyError::KernelFailure)?;
                let mut wires = face.boundaries().to_vec();
                wires.extend(cap.boundaries().iter().map(|w| w.inverse()));
                let new =
                    builder::try_attach_plane(&wires).map_err(|_| BodyError::KernelFailure)?;
                shell.push(new);
                replaced += 1;
            }
            if replaced != 2 {
                return Err(BodyError::KernelFailure);
            }
            shell.extend(
                cylinder.solid.boundaries()[0]
                    .iter()
                    .filter(|f| !cap_at(f, lower) && !cap_at(f, upper))
                    .map(|f| f.inverse()),
            );
            let volume =
                self.volume_mm3()? - std::f64::consts::PI * radius * radius * (upper - lower);
            if volume <= 0. || !volume.is_finite() {
                return Err(BodyError::InvalidVolume);
            }
            let mut bores = self.planar_bores.clone();
            bores.push(bore);
            let (mesh, segments) = planar_mesh(faces, &bores, volume)?;
            let tolerance = bores
                .iter()
                .map(|b| b.radius * (std::f64::consts::PI / segments as f64).sin() * 0.25)
                .fold(2.0 * TOLERANCE_MM, f64::min)
                .max(1e-10);
            let mesh = Self::weld_polygon_at_tolerance(mesh, tolerance);
            let mut body = Self::validated_mesh(Solid::new(vec![shell]), self.origin, Some(mesh))?;
            body.exact_volume = Some(volume);
            body.planar_faces = Some(faces.clone());
            body.planar_bores = bores;
            Ok(body)
        }))
    }
}

fn planar_mesh(
    faces: &[Vec<[f64; 3]>],
    bores: &[FlatBore],
    net_volume: f64,
) -> Result<(PolygonMesh, usize), BodyError> {
    let removed: f64 = bores
        .iter()
        .map(|b| {
            std::f64::consts::PI
                * b.radius
                * b.radius
                * dot(sub(b.end, b.start), sub(b.end, b.start)).sqrt()
        })
        .sum();
    let mut segments = 256_usize.max(
        (2. * std::f64::consts::PI.powi(2) / 3. * removed / net_volume / 5e-7)
            .sqrt()
            .ceil() as usize,
    );
    let mut specs = Vec::new();
    for face in faces {
        let origin = face[0];
        let mut normal = [0.; 3];
        for i in 0..face.len() {
            let a = sub(face[i], origin);
            let b = sub(face[(i + 1) % face.len()], origin);
            normal[0] += a[1] * b[2] - a[2] * b[1];
            normal[1] += a[2] * b[0] - a[0] * b[2];
            normal[2] += a[0] * b[1] - a[1] * b[0];
        }
        let axes = basis(normal);
        let project = |p: [f64; 3]| axes.map(|a| dot(a, sub(p, origin)));
        let contour: Vec<_> = face
            .iter()
            .map(|p| {
                let q = project(*p);
                [q[0], q[1]]
            })
            .collect();
        let mut holes = Vec::new();
        for b in bores {
            for endpoint in [b.start, b.end] {
                let p = project(endpoint);
                let c = [p[0], p[1]];
                if p[2].abs() < TOLERANCE_MM && contains_circle(&contour, c, b.radius) {
                    holes.push((c, b.radius));
                }
            }
        }
        let trial = crate::prism_mesh::prism_mesh(&contour, 1., &holes)?;
        if !holes.is_empty() {
            segments = segments.max((trial.positions().len() / 2 - contour.len()) / holes.len());
        }
        specs.push((origin, axes, contour, holes));
    }
    if segments > 65536 || bores.len().saturating_mul(segments) > 1_000_000 {
        return Err(BodyError::InvalidInput);
    }
    let mut positions = Vec::new();
    let mut triangles: Vec<[usize; 3]> = Vec::new();
    for (origin, axes, contour, holes) in specs {
        let mesh =
            crate::prism_mesh::prism_void_mesh_at_least(&contour, 1., &holes, &[], segments)?;
        let total = mesh.positions().len() / 2;
        let offset = positions.len();
        positions.extend(mesh.positions()[total..].iter().map(|p| {
            truck_modeling::Point3::from(std::array::from_fn::<_, 3, _>(|i| {
                origin[i] + axes[0][i] * p.x + axes[1][i] * p.y
            }))
        }));
        triangles.extend(
            mesh.faces()
                .triangle_iter()
                .map(|t| t.map(|v| v.pos))
                .filter(|t| t.iter().all(|i| *i >= total))
                .map(|t| t.map(|i| i - total + offset)),
        );
    }
    for b in bores {
        let axes = basis(sub(b.end, b.start));
        let offset = positions.len();
        for centre in [b.start, b.end] {
            for i in 0..segments {
                let a = std::f64::consts::TAU * i as f64 / segments as f64;
                positions.push(truck_modeling::Point3::from(
                    std::array::from_fn::<_, 3, _>(|j| {
                        centre[j] + b.radius * (axes[0][j] * a.cos() + axes[1][j] * a.sin())
                    }),
                ));
            }
        }
        for i in 0..segments {
            let a = offset + i;
            let next = offset + (i + 1) % segments;
            triangles.extend([
                [a, a + segments, next + segments],
                [a, next + segments, next],
            ]);
        }
    }
    if positions.len() > 2_000_000 {
        return Err(BodyError::InvalidInput);
    }
    Ok((
        PolygonMesh::new(
            StandardAttributes {
                positions,
                ..Default::default()
            },
            Faces::from_iter(triangles.iter()),
        ),
        segments,
    ))
}
