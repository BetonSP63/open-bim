use crate::Point3;
use truck_meshalgo::prelude::*;
use truck_modeling::{Solid, Vector3, Wire, builder};
use truck_topology::shell::ShellCondition;

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
            Self::validated(solid, origin)
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

    pub fn difference(&self, other: &Self) -> Result<Self, BodyError> {
        self.boolean(other, true)
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

    fn validated(solid: Solid, origin: Point3) -> Result<Self, BodyError> {
        if solid.boundaries().is_empty()
            || solid
                .boundaries()
                .iter()
                .any(|shell| shell.shell_condition() != ShellCondition::Closed)
        {
            return Err(BodyError::OpenOrInvertedBody);
        }
        let body = Self { solid, origin };
        let mesh = body.polygon();
        if mesh.shell_condition() != ShellCondition::Closed {
            return Err(BodyError::OpenDisplayMesh);
        }
        if !mesh.volume().is_finite() || mesh.volume() <= 0.0 {
            return Err(BodyError::InvalidVolume);
        }
        Ok(body)
    }

    fn polygon(&self) -> PolygonMesh {
        let mut mesh = self
            .solid
            .robust_triangulation(TOLERANCE_MM * 2.0)
            .to_polygon();
        // truck's welding normalizes each axis by its bounding-box span.
        // Convert the absolute millimetre tolerance to that normalized frame.
        let mut minimum = [f64::INFINITY; 3];
        let mut maximum = [f64::NEG_INFINITY; 3];
        for point in mesh.positions() {
            for i in 0..3 {
                minimum[i] = minimum[i].min(point[i]);
                maximum[i] = maximum[i].max(point[i]);
            }
        }
        let extent = (0..3)
            .map(|i| maximum[i] - minimum[i])
            .fold(1.0_f64, f64::max);
        mesh.put_together_same_attrs(2.0 * TOLERANCE_MM / extent);
        mesh.remove_degenerate_faces().remove_unused_attrs();
        mesh
    }

    pub fn volume_mm3(&self) -> Result<f64, BodyError> {
        guarded(|| Ok(self.polygon().volume()))
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
