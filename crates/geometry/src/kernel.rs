use crate::Point3;
use truck_meshalgo::prelude::*;
use truck_modeling::{Solid, Vector3, Wire, builder};
use truck_topology::shell::ShellCondition;
#[path = "planar_drill.rs"]
mod planar_drill;

/// Absolute modeling tolerance in millimetres. All features must exceed 10× it.
pub const TOLERANCE_MM: f64 = 0.0001;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyError {
    InvalidInput,
    DegenerateContour,
    KernelFailure,
    OpenOrInvertedBody,
    OpenDisplayMesh,
    InvalidVolume,
}

impl std::fmt::Display for BodyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidInput => "Некорректные размеры или координаты",
            Self::DegenerateContour => "Контур вырожден или пересекает себя",
            Self::KernelFailure => "Геометрическое ядро не смогло выполнить операцию",
            Self::OpenOrInvertedBody => "Результат не является замкнутым ориентированным телом",
            Self::OpenDisplayMesh => "Сетка отображения содержит незамкнутые границы",
            Self::InvalidVolume => "Тело имеет некорректный ориентированный объём",
        })
    }
}

impl std::error::Error for BodyError {}

/// Exact B-rep in a local f64 frame; callers never manipulate raw topology.
#[derive(Clone, Debug)]
pub struct Body {
    solid: Solid,
    origin: Point3,
    exact_volume: Option<f64>,
    mesh: Option<PolygonMesh>,
    planar_faces: Option<Vec<Vec<[f64; 3]>>>,
    planar_bores: Vec<planar_drill::FlatBore>,
}

/// Local display geometry. World placement is separate to avoid f32 precision loss.
#[derive(Clone, Debug)]
pub struct Mesh {
    pub origin: Point3,
    pub positions: Vec<[f32; 3]>,
    pub triangles: Vec<[u32; 3]>,
}

/// Local CPU geometry with double-precision vertices.
pub type Tessellation64 = (Vec<[f64; 3]>, Vec<[u32; 3]>);

impl Body {
    pub fn prism(contour: &[[f64; 2]], length: f64, origin: Point3) -> Result<Self, BodyError> {
        if !origin.0.iter().all(|v| v.is_finite() && v.abs() <= 1e9)
            || !length.is_finite()
            || length <= TOLERANCE_MM * 10.0
            || contour.len() < 3
            || contour.len() > 10000
            || !contour
                .iter()
                .flatten()
                .all(|v| v.is_finite() && v.abs() <= 1e7)
        {
            return Err(BodyError::InvalidInput);
        }
        let mut area = 0.0;
        for i in 0..contour.len() {
            let p = contour[i];
            let q = contour[(i + 1) % contour.len()];
            if (q[0] - p[0]).hypot(q[1] - p[1]) <= TOLERANCE_MM * 10.0 {
                return Err(BodyError::DegenerateContour);
            }
            area += p[0] * q[1] - p[1] * q[0];
            for j in (i + 1)..contour.len() {
                if j == i + 1 || (i == 0 && j == contour.len() - 1) {
                    continue;
                }
                if segments_cross(p, q, contour[j], contour[(j + 1) % contour.len()]) {
                    return Err(BodyError::DegenerateContour);
                }
            }
        }
        if area.abs() <= TOLERANCE_MM * TOLERANCE_MM {
            return Err(BodyError::DegenerateContour);
        }
        let mut contour = contour.to_vec();
        if area < 0.0 {
            contour.reverse();
        }
        guarded(|| {
            let vertices: Vec<_> = contour
                .iter()
                .map(|p| builder::vertex(truck_modeling::Point3::new(p[0], p[1], 0.0)))
                .collect();
            let wire: Wire = (0..vertices.len())
                .map(|i| builder::line(&vertices[i], &vertices[(i + 1) % vertices.len()]))
                .collect();
            let face = builder::try_attach_plane(&[wire]).map_err(|_| BodyError::KernelFailure)?;
            let solid = builder::tsweep(&face, Vector3::new(0.0, 0.0, length));
            let mut body = Self::validated(solid, origin)?;
            body.exact_volume = Some(area.abs() * 0.5 * length);
            let lower: Vec<_> = contour.iter().map(|p| [p[0], p[1], 0.0]).collect();
            let upper: Vec<_> = contour.iter().map(|p| [p[0], p[1], length]).collect();
            let mut reversed = lower.clone();
            reversed.reverse();
            let mut faces = vec![reversed, upper.clone()];
            for i in 0..contour.len() {
                let j = (i + 1) % contour.len();
                faces.push(vec![lower[i], lower[j], upper[j], upper[i]]);
            }
            body.planar_faces = Some(faces);
            Ok(body)
        })
    }

    /// Circular through-holes are exact swept circles, never polygonal bores.
    /// Centres/radii are local XY; holes must be wholly inside and disjoint.
    pub fn prism_with_holes(
        contour: &[[f64; 2]],
        length: f64,
        holes: &[([f64; 2], f64)],
        origin: Point3,
    ) -> Result<Self, BodyError> {
        let base = Self::prism(contour, length, origin)?;
        if holes.is_empty() {
            return Ok(base);
        }
        if holes.len() > 10000 {
            return Err(BodyError::InvalidInput);
        }
        for (i, (centre, radius)) in holes.iter().enumerate() {
            if !centre.iter().all(|v| v.is_finite())
                || !radius.is_finite()
                || *radius <= 10.0 * TOLERANCE_MM
            {
                return Err(BodyError::InvalidInput);
            }
            let mut inside = false;
            for j in 0..contour.len() {
                let a = contour[j];
                let b = contour[(j + 1) % contour.len()];
                let ab = [b[0] - a[0], b[1] - a[1]];
                let t = (((centre[0] - a[0]) * ab[0] + (centre[1] - a[1]) * ab[1])
                    / (ab[0] * ab[0] + ab[1] * ab[1]))
                    .clamp(0.0, 1.0);
                if (centre[0] - a[0] - t * ab[0]).hypot(centre[1] - a[1] - t * ab[1])
                    <= radius + 10.0 * TOLERANCE_MM
                {
                    return Err(BodyError::InvalidInput);
                }
                if (a[1] > centre[1]) != (b[1] > centre[1])
                    && centre[0] < (b[0] - a[0]) * (centre[1] - a[1]) / (b[1] - a[1]) + a[0]
                {
                    inside = !inside;
                }
            }
            if !inside
                || holes[..i].iter().any(|(other, other_radius)| {
                    (centre[0] - other[0]).hypot(centre[1] - other[1])
                        <= radius + other_radius + 10.0 * TOLERANCE_MM
                })
            {
                return Err(BodyError::InvalidInput);
            }
        }
        guarded(|| {
            let mut points = contour.to_vec();
            let area: f64 = (0..points.len())
                .map(|i| {
                    let a = points[i];
                    let b = points[(i + 1) % points.len()];
                    a[0] * b[1] - a[1] * b[0]
                })
                .sum();
            if area < 0.0 {
                points.reverse();
            }
            let vertices: Vec<_> = points
                .iter()
                .map(|p| builder::vertex(truck_modeling::Point3::new(p[0], p[1], 0.0)))
                .collect();
            let outer: Wire = (0..vertices.len())
                .map(|i| builder::line(&vertices[i], &vertices[(i + 1) % vertices.len()]))
                .collect();
            let mut wires = vec![outer];
            for (centre, radius) in holes {
                let vertex = builder::vertex(truck_modeling::Point3::new(
                    centre[0] + radius,
                    centre[1],
                    0.0,
                ));
                wires.push(builder::rsweep(
                    &vertex,
                    truck_modeling::Point3::new(centre[0], centre[1], 0.0),
                    -Vector3::unit_z(),
                    truck_modeling::Rad(std::f64::consts::TAU),
                ));
            }
            let face = builder::try_attach_plane(&wires).map_err(|_| BodyError::KernelFailure)?;
            let solid = builder::tsweep(&face, Vector3::new(0.0, 0.0, length));
            let mesh = crate::prism_mesh::prism_mesh(&points, length, holes)?;
            let mut body = Self::validated_mesh(solid, origin, Some(mesh))?;
            body.exact_volume = Some(
                base.volume_mm3()?
                    - holes
                        .iter()
                        .map(|(_, r)| std::f64::consts::PI * r * r * length)
                        .sum::<f64>(),
            );
            Ok(body)
        })
    }

    /// Exact extrusion with disjoint circular and polygonal through voids.
    /// Invalid or intersecting loops are rejected rather than approximated.
    pub fn prism_with_voids(
        contour: &[[f64; 2]],
        length: f64,
        holes: &[([f64; 2], f64)],
        voids: &[Vec<[f64; 2]>],
        origin: Point3,
    ) -> Result<Self, BodyError> {
        if voids.is_empty() {
            return Self::prism_with_holes(contour, length, holes, origin);
        }
        if voids.len() > 10000 {
            return Err(BodyError::InvalidInput);
        }
        let base = Self::prism_with_holes(contour, length, holes, origin)?;
        let inside = |p: [f64; 2], ring: &[[f64; 2]]| {
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
        };
        let distance = |p: [f64; 2], a: [f64; 2], b: [f64; 2]| {
            let ab = [b[0] - a[0], b[1] - a[1]];
            let t = (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1])
                / (ab[0] * ab[0] + ab[1] * ab[1]))
                .clamp(0.0, 1.0);
            (p[0] - a[0] - t * ab[0]).hypot(p[1] - a[1] - t * ab[1])
        };
        let touches = |a: &[[f64; 2]], b: &[[f64; 2]]| {
            (0..a.len()).any(|i| {
                (0..b.len()).any(|j| {
                    let (p, q) = (a[i], a[(i + 1) % a.len()]);
                    let (r, s) = (b[j], b[(j + 1) % b.len()]);
                    segments_cross(p, q, r, s)
                        || [
                            distance(p, r, s),
                            distance(q, r, s),
                            distance(r, p, q),
                            distance(s, p, q),
                        ]
                        .into_iter()
                        .any(|d| d <= TOLERANCE_MM * 10.0)
                })
            })
        };
        let mut removed = 0.0;
        let mut normalized = Vec::new();
        for (i, v) in voids.iter().enumerate() {
            let primitive = Self::prism(v, length, origin)?;
            if !v.iter().all(|p| inside(*p, contour))
                || touches(v, contour)
                || voids[..i]
                    .iter()
                    .any(|other| touches(v, other) || inside(v[0], other) || inside(other[0], v))
                || holes.iter().any(|(c, r)| {
                    inside(*c, v)
                        || (0..v.len()).any(|j| {
                            distance(*c, v[j], v[(j + 1) % v.len()]) <= r + TOLERANCE_MM * 10.0
                        })
                })
            {
                return Err(BodyError::InvalidInput);
            }
            removed += primitive.volume_mm3()?;
            let signed: f64 = (0..v.len())
                .map(|j| {
                    let a = v[j];
                    let b = v[(j + 1) % v.len()];
                    a[0] * b[1] - a[1] * b[0]
                })
                .sum();
            let mut v = v.clone();
            if signed < 0.0 {
                v.reverse();
            }
            normalized.push(v);
        }
        guarded(|| {
            let mut outer = contour.to_vec();
            let signed: f64 = (0..outer.len())
                .map(|i| {
                    let a = outer[i];
                    let b = outer[(i + 1) % outer.len()];
                    a[0] * b[1] - a[1] * b[0]
                })
                .sum();
            if signed < 0.0 {
                outer.reverse();
            }
            let wire = |points: &[[f64; 2]]| -> Wire {
                let vertices: Vec<_> = points
                    .iter()
                    .map(|p| builder::vertex(truck_modeling::Point3::new(p[0], p[1], 0.0)))
                    .collect();
                (0..vertices.len())
                    .map(|i| builder::line(&vertices[i], &vertices[(i + 1) % vertices.len()]))
                    .collect()
            };
            let mut wires = vec![wire(&outer)];
            wires.extend(normalized.iter().map(|v| wire(v).inverse()));
            for (centre, radius) in holes {
                let vertex = builder::vertex(truck_modeling::Point3::new(
                    centre[0] + radius,
                    centre[1],
                    0.0,
                ));
                wires.push(builder::rsweep(
                    &vertex,
                    truck_modeling::Point3::new(centre[0], centre[1], 0.0),
                    -Vector3::unit_z(),
                    truck_modeling::Rad(std::f64::consts::TAU),
                ));
            }
            let face = builder::try_attach_plane(&wires).map_err(|_| BodyError::KernelFailure)?;
            let solid = builder::tsweep(&face, Vector3::new(0.0, 0.0, length));
            let mesh = crate::prism_mesh::prism_void_mesh(&outer, length, holes, &normalized)?;
            let mut body = Self::validated_mesh(solid, origin, Some(mesh))?;
            let volume = base.volume_mm3()? - removed;
            if !volume.is_finite() || volume <= 0.0 {
                return Err(BodyError::InvalidVolume);
            }
            body.exact_volume = Some(volume);
            Ok(body)
        })
    }

    /// Exact annular extrusion, with clockwise inner and anticlockwise outer circles.
    pub fn tube(outer: f64, inner: f64, length: f64, origin: Point3) -> Result<Self, BodyError> {
        if ![outer, inner, length]
            .iter()
            .all(|v| v.is_finite() && *v > 0.001)
            || inner >= outer - 0.001
            || !origin.0.iter().all(|v| v.is_finite() && v.abs() <= 1e9)
        {
            return Err(BodyError::InvalidInput);
        }
        guarded(|| {
            let centre = truck_modeling::Point3::new(0.0, 0.0, 0.0);
            let wires = [(outer, 1.0), (inner, -1.0)].map(|(radius, direction)| {
                let vertex = builder::vertex(truck_modeling::Point3::new(radius, 0.0, 0.0));
                builder::rsweep(
                    &vertex,
                    centre,
                    Vector3::unit_z() * direction,
                    truck_modeling::Rad(std::f64::consts::TAU),
                )
            });
            let face = builder::try_attach_plane(&wires).map_err(|_| BodyError::KernelFailure)?;
            let n = 4096;
            let mut positions = Vec::with_capacity(n * 4);
            for z in [0.0, length] {
                for radius in [outer, inner] {
                    for i in 0..n {
                        let a = std::f64::consts::TAU * i as f64 / n as f64;
                        positions.push(truck_modeling::Point3::new(
                            radius * a.cos(),
                            radius * a.sin(),
                            z,
                        ));
                    }
                }
            }
            let mut triangles: Vec<[usize; 3]> = Vec::with_capacity(n * 8);
            let mut quad = |v: [usize; 4]| {
                triangles.push([v[0], v[1], v[2]]);
                triangles.push([v[0], v[2], v[3]]);
            };
            for i in 0..n {
                let j = (i + 1) % n;
                quad([i, j, j + 2 * n, i + 2 * n]);
                quad([i + n, i + 3 * n, j + 3 * n, j + n]);
                quad([i, i + n, j + n, j]);
                quad([i + 2 * n, j + 2 * n, j + 3 * n, i + 3 * n]);
            }
            let mesh = PolygonMesh::new(
                StandardAttributes {
                    positions,
                    ..Default::default()
                },
                Faces::from_iter(triangles.iter()),
            );
            let mut body = Self::validated_mesh(
                builder::tsweep(&face, Vector3::new(0.0, 0.0, length)),
                origin,
                Some(mesh),
            )?;
            body.exact_volume =
                Some(std::f64::consts::PI * (outer * outer - inner * inner) * length);
            Ok(body)
        })
    }

    pub fn rectangular_tube(
        width: f64,
        height: f64,
        thickness: f64,
        length: f64,
        origin: Point3,
    ) -> Result<Self, BodyError> {
        let base = Self::prism(
            &[
                [-width / 2.0, -height / 2.0],
                [width / 2.0, -height / 2.0],
                [width / 2.0, height / 2.0],
                [-width / 2.0, height / 2.0],
            ],
            length,
            origin,
        )?;
        if !thickness.is_finite() || thickness <= 0.001 || 2.0 * thickness >= width.min(height) {
            return Err(BodyError::InvalidInput);
        }
        guarded(|| {
            let iw = width / 2.0 - thickness;
            let ih = height / 2.0 - thickness;
            let contours = [
                [
                    [-width / 2.0, -height / 2.0],
                    [width / 2.0, -height / 2.0],
                    [width / 2.0, height / 2.0],
                    [-width / 2.0, height / 2.0],
                ],
                [[-iw, -ih], [-iw, ih], [iw, ih], [iw, -ih]],
            ];
            let wires: Vec<Wire> = contours
                .iter()
                .map(|points| {
                    let vertices: Vec<_> = points
                        .iter()
                        .map(|p| builder::vertex(truck_modeling::Point3::new(p[0], p[1], 0.0)))
                        .collect();
                    (0..4)
                        .map(|i| builder::line(&vertices[i], &vertices[(i + 1) % 4]))
                        .collect()
                })
                .collect();
            let face = builder::try_attach_plane(&wires).map_err(|_| BodyError::KernelFailure)?;
            let mut body = Self::validated(
                builder::tsweep(&face, Vector3::new(0.0, 0.0, length)),
                origin,
            )?;
            body.exact_volume = Some(base.volume_mm3()? - 4.0 * iw * ih * length);
            Ok(body)
        })
    }

    pub fn cuboid(size: [f64; 3], origin: Point3) -> Result<Self, BodyError> {
        if !size
            .iter()
            .all(|v| v.is_finite() && *v > 10.0 * TOLERANCE_MM)
        {
            return Err(BodyError::InvalidInput);
        }
        Self::prism(
            &[
                [0.0, 0.0],
                [size[0], 0.0],
                [size[0], size[1]],
                [0.0, size[1]],
            ],
            size[2],
            origin,
        )
    }

    pub fn intersection(&self, other: &Self) -> Result<Self, BodyError> {
        self.boolean(other, false)
    }

    pub fn cylinder(radius: f64, length: f64, origin: Point3) -> Result<Self, BodyError> {
        if ![radius, length].iter().all(|v| v.is_finite() && *v > 0.001)
            || !origin.0.iter().all(|v| v.is_finite() && v.abs() <= 1e9)
        {
            return Err(BodyError::InvalidInput);
        }
        guarded(|| {
            let vertex = builder::vertex(truck_modeling::Point3::new(
                radius / 2.0_f64.sqrt(),
                radius / 2.0_f64.sqrt(),
                0.0,
            ));
            let wire = builder::rsweep(
                &vertex,
                truck_modeling::Point3::new(0.0, 0.0, 0.0),
                Vector3::unit_z(),
                truck_modeling::Rad(std::f64::consts::TAU),
            );
            let face = builder::try_attach_plane(&[wire]).map_err(|_| BodyError::KernelFailure)?;
            let n = 4096;
            let mut positions = Vec::with_capacity(n * 2 + 2);
            for z in [0.0, length] {
                for i in 0..n {
                    let a = std::f64::consts::TAU * i as f64 / n as f64;
                    positions.push(truck_modeling::Point3::new(
                        radius * a.cos(),
                        radius * a.sin(),
                        z,
                    ));
                }
            }
            positions.push(truck_modeling::Point3::new(0.0, 0.0, 0.0));
            positions.push(truck_modeling::Point3::new(0.0, 0.0, length));
            let mut triangles: Vec<[usize; 3]> = Vec::with_capacity(n * 4);
            for i in 0..n {
                let j = (i + 1) % n;
                triangles.extend([
                    [i, j, j + n],
                    [i, j + n, i + n],
                    [2 * n, j, i],
                    [2 * n + 1, i + n, j + n],
                ]);
            }
            let mesh = PolygonMesh::new(
                StandardAttributes {
                    positions,
                    ..Default::default()
                },
                Faces::from_iter(triangles.iter()),
            );
            let mut body = Self::validated_mesh(
                builder::tsweep(&face, Vector3::new(0.0, 0.0, length)),
                origin,
                Some(mesh),
            )?;
            body.exact_volume = Some(std::f64::consts::PI * radius * radius * length);
            Ok(body)
        })
    }

    /// Rigid local transformation; the separate world origin stays unchanged.
    pub fn rigid_transform(
        &self,
        axes: [[f64; 3]; 3],
        translation: [f64; 3],
    ) -> Result<Self, BodyError> {
        use truck_modeling::{InnerSpace, Matrix4, Vector4};
        if !axes
            .iter()
            .flatten()
            .chain(translation.iter())
            .all(|v| v.is_finite())
        {
            return Err(BodyError::InvalidInput);
        }
        let [x, y, z] = axes.map(|a| Vector3::new(a[0], a[1], a[2]));
        if [x, y, z]
            .iter()
            .any(|a| (a.magnitude2() - 1.0).abs() > 1e-9)
            || x.dot(y).abs() > 1e-9
            || y.dot(z).abs() > 1e-9
            || z.dot(x).abs() > 1e-9
            || (x.cross(y) - z).magnitude() > 1e-9
        {
            return Err(BodyError::InvalidInput);
        }
        guarded(|| {
            let matrix = Matrix4::from_cols(
                Vector4::new(x.x, x.y, x.z, 0.0),
                Vector4::new(y.x, y.y, y.z, 0.0),
                Vector4::new(z.x, z.y, z.z, 0.0),
                Vector4::new(translation[0], translation[1], translation[2], 1.0),
            );
            let transform = |p: [f64; 3]| {
                std::array::from_fn(|i| {
                    translation[i] + (0..3).map(|j| axes[j][i] * p[j]).sum::<f64>()
                })
            };
            let mesh = self.polygon();
            let positions = mesh
                .positions()
                .iter()
                .map(|p| {
                    let q = transform([p.x, p.y, p.z]);
                    truck_modeling::Point3::new(q[0], q[1], q[2])
                })
                .collect();
            let mut body = Self::validated_mesh(
                builder::transformed(&self.solid, matrix),
                self.origin,
                Some(PolygonMesh::new(
                    StandardAttributes {
                        positions,
                        ..Default::default()
                    },
                    mesh.faces().clone(),
                )),
            )?;
            body.exact_volume = self.exact_volume;
            body.planar_faces = self.planar_faces.as_ref().map(|faces| {
                faces
                    .iter()
                    .map(|f| f.iter().map(|p| transform(*p)).collect())
                    .collect()
            });
            body.planar_bores = self
                .planar_bores
                .iter()
                .map(|b| planar_drill::FlatBore {
                    start: transform(b.start),
                    end: transform(b.end),
                    radius: b.radius,
                })
                .collect();
            Ok(body)
        })
    }

    pub fn difference(&self, other: &Self) -> Result<Self, BodyError> {
        self.boolean(other, true)
    }

    /// Drill an exact cylinder. Uniform extrusions use shared cavity topology,
    /// avoiding coincident intersections in the general Boolean algorithm.
    pub fn drill(
        &self,
        axes: [[f64; 3]; 3],
        start: [f64; 3],
        radius: f64,
        depth: f64,
    ) -> Result<Self, BodyError> {
        let tool = Self::cylinder(radius, depth, self.origin)?.rigid_transform(axes, start)?;
        if let Some(result) = self.drill_planar_slab(axes, start, radius, depth) {
            return result;
        }
        if !self.planar_bores.is_empty() {
            return self.difference(&tool);
        }
        let Some(faces) = &self.planar_faces else {
            return self.difference(&tool);
        };
        let local =
            |p: [f64; 3]| axes.map(|a| (0..3).map(|i| a[i] * (p[i] - start[i])).sum::<f64>());
        let points: Vec<_> = faces.iter().flatten().map(|p| local(*p)).collect();
        let lower = points.iter().map(|p| p[2]).fold(f64::INFINITY, f64::min);
        let upper = points
            .iter()
            .map(|p| p[2])
            .fold(f64::NEG_INFINITY, f64::max);
        let near = |a: f64, b: f64| (a - b).abs() < TOLERANCE_MM;
        if !points
            .iter()
            .all(|p| near(p[2], lower) || near(p[2], upper))
        {
            return self.difference(&tool);
        }
        let Some(bottom) = faces
            .iter()
            .find(|f| f.iter().all(|p| near(local(*p)[2], lower)))
        else {
            return self.difference(&tool);
        };
        let contour: Vec<_> = bottom
            .iter()
            .map(|p| {
                let p = local(*p);
                [p[0], p[1]]
            })
            .collect();
        if points
            .iter()
            .filter(|p| near(p[2], upper))
            .any(|p| !contour.iter().any(|q| near(p[0], q[0]) && near(p[1], q[1])))
        {
            return self.difference(&tool);
        }
        let length = upper - lower;
        let from = 0.0_f64.max(lower) - lower;
        let to = depth.min(upper) - lower;
        if to - from <= 0.001 {
            return Err(BodyError::InvalidInput);
        }
        // Validate circle containment and topology even for a blind hole.
        let mut result = if near(from, 0.0) && near(to, length) {
            Self::prism_with_holes(&contour, length, &[([0.0, 0.0], radius)], self.origin)?
        } else if near(to, length) {
            Self::blind_prism(&contour, length, radius, from, self.origin)?
        } else if near(from, 0.0) {
            // Reverse drilling direction to construct a cavity open at the top.
            let reversed: Vec<_> = contour.iter().map(|p| [p[0], -p[1]]).collect();
            Self::blind_prism(&reversed, length, radius, length - to, self.origin)?
                .rigid_transform(
                    [[1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, -1.0]],
                    [0.0, 0.0, length],
                )?
        } else {
            return self.difference(&tool);
        };
        let translation = std::array::from_fn(|i| start[i] + axes[2][i] * lower);
        result = result.rigid_transform(axes, translation)?;
        Ok(result)
    }

    fn blind_prism(
        contour: &[[f64; 2]],
        length: f64,
        radius: f64,
        floor: f64,
        origin: Point3,
    ) -> Result<Self, BodyError> {
        Self::prism_with_holes(contour, length, &[([0.0, 0.0], radius)], origin)?;
        guarded(|| {
            let base = Self::prism(contour, length, origin)?;
            let cylinder = Self::cylinder(radius, length - floor, origin)?.rigid_transform(
                [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
                [0.0, 0.0, floor],
            )?;
            let at_top = |face: &truck_modeling::Face| {
                face.boundaries().iter().flat_map(|w| w.iter()).all(|e| {
                    (e.front().point().z - length).abs() < 1e-8
                        && (e.back().point().z - length).abs() < 1e-8
                })
            };
            let base_top = base.solid.boundaries()[0]
                .iter()
                .find(|f| at_top(f))
                .ok_or(BodyError::KernelFailure)?;
            let cylinder_top = cylinder.solid.boundaries()[0]
                .iter()
                .find(|f| at_top(f))
                .ok_or(BodyError::KernelFailure)?;
            let mut wires = base_top.boundaries().to_vec();
            wires.extend(cylinder_top.boundaries().iter().map(|w| w.inverse()));
            let top = builder::try_attach_plane(&wires).map_err(|_| BodyError::KernelFailure)?;
            let mut shell: truck_modeling::Shell = base.solid.boundaries()[0]
                .iter()
                .filter(|f| !at_top(f))
                .cloned()
                .collect();
            shell.push(top);
            shell.extend(
                cylinder.solid.boundaries()[0]
                    .iter()
                    .filter(|f| !at_top(f))
                    .map(|f| f.inverse()),
            );
            let mesh = crate::prism_mesh::blind_prism_mesh(contour, length, radius, floor)?;
            let mut body = Self::validated_mesh(Solid::new(vec![shell]), origin, Some(mesh))?;
            body.exact_volume = Some(
                base.volume_mm3()? - std::f64::consts::PI * radius * radius * (length - floor),
            );
            Ok(body)
        })
    }

    /// Keep the local half-space normal·point <= offset. The cutter is a
    /// bounded exact planar solid enclosing this body's complete mesh bounds.
    pub fn clip_plane(&self, normal: [f64; 3], offset: f64) -> Result<Self, BodyError> {
        use truck_modeling::{InnerSpace, Matrix4, Vector4};
        if !normal.iter().all(|v| v.is_finite()) || !offset.is_finite() {
            return Err(BodyError::InvalidInput);
        }
        let n = Vector3::new(normal[0], normal[1], normal[2]);
        let magnitude = n.magnitude();
        if magnitude < 1e-12 {
            return Err(BodyError::InvalidInput);
        }
        let n = n / magnitude;
        let offset = offset / magnitude;
        let points = self.polygon();
        if points
            .positions()
            .iter()
            .all(|p| n.dot(Vector3::new(p.x, p.y, p.z)) <= offset)
        {
            return Ok(self.clone());
        }
        let q = n * offset;
        let span = points
            .positions()
            .iter()
            .map(|p| (Vector3::new(p.x, p.y, p.z) - q).magnitude())
            .fold(1.0_f64, f64::max)
            + 1.0;
        let reference = if n.x.abs() < 0.8 {
            Vector3::unit_x()
        } else {
            Vector3::unit_y()
        };
        let u = n.cross(reference).normalize();
        let z = -n;
        let v = z.cross(u);
        // Exact planar fallback avoids Boolean degeneracy when the section
        // passes through existing vertices. Preserve the requested plane.
        if let Some(faces) = &self.planar_faces
            && self.planar_bores.is_empty()
            && faces.iter().all(|face| convex_face(face))
        {
            return self.clip_polyhedron(faces, n, offset, u, v);
        }
        let mut tool = Self::prism(
            &[[-span, -span], [span, -span], [span, span], [-span, span]],
            2.0 * span,
            self.origin,
        )?;
        tool.solid = builder::transformed(
            &tool.solid,
            Matrix4::from_cols(
                Vector4::new(u.x, u.y, u.z, 0.0),
                Vector4::new(v.x, v.y, v.z, 0.0),
                Vector4::new(z.x, z.y, z.z, 0.0),
                Vector4::new(q.x, q.y, q.z, 1.0),
            ),
        );
        tool.mesh = None;
        self.intersection(&tool)
    }

    fn boolean(&self, other: &Self, subtract: bool) -> Result<Self, BodyError> {
        guarded(|| {
            let offset = Vector3::new(
                other.origin.0[0] - self.origin.0[0],
                other.origin.0[1] - self.origin.0[1],
                other.origin.0[2] - self.origin.0[2],
            );
            let mut tool = builder::translated(&other.solid, offset);
            if subtract {
                tool.not();
            }
            let result = truck_shapeops::and(&self.solid, &tool, TOLERANCE_MM)
                .ok_or(BodyError::KernelFailure)?;
            Self::validated(result, self.origin)
        })
    }

    fn clip_polyhedron(
        &self,
        faces: &[Vec<[f64; 3]>],
        n: Vector3,
        offset: f64,
        u: Vector3,
        v: Vector3,
    ) -> Result<Self, BodyError> {
        use truck_modeling::InnerSpace;
        guarded(|| {
            let dot = |p: [f64; 3]| n.dot(Vector3::new(p[0], p[1], p[2])) - offset;
            let mut clipped = Vec::new();
            let mut cap = Vec::<[f64; 3]>::new();
            for face in faces {
                let mut polygon = Vec::new();
                for i in 0..face.len() {
                    let a = face[i];
                    let b = face[(i + 1) % face.len()];
                    let da = dot(a);
                    let db = dot(b);
                    if da <= 0.0 {
                        polygon.push(a);
                    }
                    if (da < 0.0 && db > 0.0) || (da > 0.0 && db < 0.0) {
                        let t = da / (da - db);
                        let p = std::array::from_fn(|j| a[j] + t * (b[j] - a[j]));
                        polygon.push(p);
                        cap.push(p);
                    }
                    if da.abs() < 1e-9 {
                        cap.push(a);
                    }
                }
                polygon.dedup_by(|a, b| Point3(*a).distance(Point3(*b)) < 1e-9);
                if polygon.len() > 2 {
                    clipped.push(polygon);
                }
            }
            cap.sort_by(|a, b| {
                a[0].total_cmp(&b[0])
                    .then(a[1].total_cmp(&b[1]))
                    .then(a[2].total_cmp(&b[2]))
            });
            cap.dedup_by(|a, b| Point3(*a).distance(Point3(*b)) < 1e-8);
            if cap.len() < 3 {
                return Err(BodyError::InvalidVolume);
            }
            let centre: [f64; 3] =
                std::array::from_fn(|i| cap.iter().map(|p| p[i]).sum::<f64>() / cap.len() as f64);
            // v was constructed for -n; reverse it so the cap points outwards.
            let angle = |p: [f64; 3]| {
                let d = Vector3::new(p[0] - centre[0], p[1] - centre[1], p[2] - centre[2]);
                (-v).dot(d).atan2(u.dot(d))
            };
            cap.sort_by(|a, b| angle(*a).total_cmp(&angle(*b)));
            clipped.push(cap);
            let mut vertices = Vec::<([f64; 3], truck_modeling::Vertex)>::new();
            let mut edges =
                std::collections::BTreeMap::<(usize, usize), truck_modeling::Edge>::new();
            let mut shell = truck_modeling::Shell::new();
            for polygon in &clipped {
                let indices: Vec<_> = polygon
                    .iter()
                    .map(|p| {
                        if let Some(i) = vertices
                            .iter()
                            .position(|(q, _)| Point3(*p).distance(Point3(*q)) < 1e-8)
                        {
                            i
                        } else {
                            let i = vertices.len();
                            vertices.push((
                                *p,
                                builder::vertex(truck_modeling::Point3::new(p[0], p[1], p[2])),
                            ));
                            i
                        }
                    })
                    .collect();
                let mut wire = Wire::new();
                for i in 0..indices.len() {
                    let a = indices[i];
                    let b = indices[(i + 1) % indices.len()];
                    if a == b {
                        continue;
                    }
                    if let Some(edge) = edges.get(&(b, a)) {
                        wire.push_back(edge.inverse());
                    } else {
                        let edge = builder::line(&vertices[a].1, &vertices[b].1);
                        edges.insert((a, b), edge.clone());
                        wire.push_back(edge);
                    }
                }
                shell.push(
                    builder::try_attach_plane(&[wire]).map_err(|_| BodyError::KernelFailure)?,
                );
            }
            let solid = Solid::new(vec![shell]);
            let mut body = Self::validated(solid, self.origin)?;
            body.planar_faces = Some(clipped);
            Ok(body)
        })
    }

    fn validated(solid: Solid, origin: Point3) -> Result<Self, BodyError> {
        Self::validated_mesh(solid, origin, None)
    }

    fn validated_mesh(
        solid: Solid,
        origin: Point3,
        mesh: Option<PolygonMesh>,
    ) -> Result<Self, BodyError> {
        if solid.boundaries().is_empty()
            || solid
                .boundaries()
                .iter()
                .any(|shell| shell.shell_condition() != ShellCondition::Closed)
        {
            return Err(BodyError::OpenOrInvertedBody);
        }
        let mut body = Self {
            solid,
            origin,
            exact_volume: None,
            mesh,
            planar_faces: None,
            planar_bores: Vec::new(),
        };
        let mesh = body.polygon();
        if mesh.shell_condition() != ShellCondition::Closed {
            return Err(BodyError::OpenDisplayMesh);
        }
        if !mesh.volume().is_finite() || mesh.volume() <= 0.0 {
            return Err(BodyError::InvalidVolume);
        }
        body.mesh = Some(mesh);
        Ok(body)
    }

    fn polygon(&self) -> PolygonMesh {
        if let Some(mesh) = &self.mesh {
            return mesh.clone();
        }
        let mesh = self
            .solid
            .robust_triangulation(TOLERANCE_MM * 0.5)
            .to_polygon();
        Self::weld_polygon(mesh)
    }

    fn weld_polygon(mesh: PolygonMesh) -> PolygonMesh {
        Self::weld_polygon_at_tolerance(mesh, 2.0 * TOLERANCE_MM)
    }

    fn weld_polygon_at_tolerance(mut mesh: PolygonMesh, tolerance: f64) -> PolygonMesh {
        // CPU topology/volume only need positions. Welding the repeated normals
        // of a fine cylindrical surface has quadratic cost in truck's filter.
        let faces: Vec<_> = mesh
            .faces()
            .triangle_iter()
            .map(|face| face.map(|vertex| vertex.pos))
            .collect();
        mesh = PolygonMesh::new(
            StandardAttributes {
                positions: mesh.positions().to_vec(),
                ..Default::default()
            },
            Faces::from_iter(faces.iter()),
        );
        // Spatial buckets keep exact duplicates and neighbouring tessellation
        // seams linear in typical meshes; compare all 27 adjacent cells.
        let mut buckets: std::collections::HashMap<[i64; 3], Vec<usize>> = Default::default();
        let mut positions: Vec<truck_modeling::Point3> = Vec::new();
        let mut remap = Vec::with_capacity(mesh.positions().len());
        for p in mesh.positions() {
            let cell = [p.x, p.y, p.z].map(|v| (v / tolerance).floor() as i64);
            let mut found = None;
            'search: for x in -1..=1 {
                for y in -1..=1 {
                    for z in -1..=1 {
                        if let Some(indices) = buckets.get(&[cell[0] + x, cell[1] + y, cell[2] + z])
                        {
                            for index in indices {
                                let q = positions[*index];
                                if (p.x - q.x).powi(2) + (p.y - q.y).powi(2) + (p.z - q.z).powi(2)
                                    <= tolerance * tolerance
                                {
                                    found = Some(*index);
                                    break 'search;
                                }
                            }
                        }
                    }
                }
            }
            let index = found.unwrap_or_else(|| {
                let index = positions.len();
                positions.push(*p);
                buckets.entry(cell).or_default().push(index);
                index
            });
            remap.push(index);
        }
        let faces: Vec<Vec<StandardVertex>> = mesh
            .faces()
            .face_iter()
            .map(|face| {
                face.iter()
                    .map(|v| StandardVertex {
                        pos: remap[v.pos],
                        uv: None,
                        nor: None,
                    })
                    .collect()
            })
            .collect();
        mesh = PolygonMesh::new(
            StandardAttributes {
                positions,
                ..Default::default()
            },
            Faces::from_iter(faces.iter()),
        );
        mesh.remove_degenerate_faces().remove_unused_attrs();
        mesh
    }

    pub fn volume_mm3(&self) -> Result<f64, BodyError> {
        guarded(|| Ok(self.exact_volume.unwrap_or_else(|| self.polygon().volume())))
    }

    /// Double-precision tessellation for CPU selection; float conversion belongs to the GPU boundary.
    pub fn tessellation_f64(&self) -> Result<Tessellation64, BodyError> {
        guarded(|| {
            let mesh = self.polygon();
            Ok((
                mesh.positions().iter().map(|p| [p.x, p.y, p.z]).collect(),
                mesh.faces()
                    .triangle_iter()
                    .map(|f| [f[0].pos as u32, f[1].pos as u32, f[2].pos as u32])
                    .collect(),
            ))
        })
    }

    pub fn display_mesh(&self) -> Result<Mesh, BodyError> {
        guarded(|| {
            let mesh = self.polygon();
            let positions = mesh
                .positions()
                .iter()
                .map(|p| [p.x as f32, p.y as f32, p.z as f32])
                .collect();
            let triangles = mesh
                .faces()
                .triangle_iter()
                .map(|face| [face[0].pos as u32, face[1].pos as u32, face[2].pos as u32])
                .collect();
            Ok(Mesh {
                origin: self.origin,
                positions,
                triangles,
            })
        })
    }
}

fn guarded<T>(operation: impl FnOnce() -> Result<T, BodyError>) -> Result<T, BodyError> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation))
        .unwrap_or(Err(BodyError::KernelFailure))
}

fn segments_cross(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    fn orient(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
        (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
    }
    let ab_c = orient(a, b, c);
    let ab_d = orient(a, b, d);
    let cd_a = orient(c, d, a);
    let cd_b = orient(c, d, b);
    if ab_c * ab_d < 0.0 && cd_a * cd_b < 0.0 {
        return true;
    }
    // Reject nonadjacent contact and collinear overlap as degenerate contours.
    [
        (ab_c, c, a, b),
        (ab_d, d, a, b),
        (cd_a, a, c, d),
        (cd_b, b, c, d),
    ]
    .iter()
    .any(|(o, p, x, y)| {
        o.abs() <= TOLERANCE_MM * TOLERANCE_MM
            && p[0] >= x[0].min(y[0]) - TOLERANCE_MM
            && p[0] <= x[0].max(y[0]) + TOLERANCE_MM
            && p[1] >= x[1].min(y[1]) - TOLERANCE_MM
            && p[1] <= x[1].max(y[1]) + TOLERANCE_MM
    })
}

fn convex_face(face: &[[f64; 3]]) -> bool {
    use truck_modeling::InnerSpace;
    let vector = |a: [f64; 3], b: [f64; 3]| Vector3::new(b[0] - a[0], b[1] - a[1], b[2] - a[2]);
    let mut normal = Vector3::new(0.0, 0.0, 0.0);
    for i in 0..face.len() {
        let cross = vector(face[i], face[(i + 1) % face.len()]).cross(vector(
            face[(i + 1) % face.len()],
            face[(i + 2) % face.len()],
        ));
        if cross.magnitude2() > 1e-16 {
            normal = cross;
            break;
        }
    }
    normal.magnitude2() > 1e-16
        && (0..face.len()).all(|i| {
            vector(face[i], face[(i + 1) % face.len()])
                .cross(vector(
                    face[(i + 1) % face.len()],
                    face[(i + 2) % face.len()],
                ))
                .dot(normal)
                >= -1e-8
        })
}
