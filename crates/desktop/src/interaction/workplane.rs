//! Work plane: the plane picks are projected onto («Рабочая плоскость»).
use open_bim_renderer::DVec3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Workplane {
    pub origin: DVec3,
    /// Local x, y, z axes in world coordinates (right-handed, orthonormal).
    pub axes: [DVec3; 3],
}

impl Default for Workplane {
    fn default() -> Self {
        Self {
            origin: DVec3::ZERO,
            axes: [DVec3::X, DVec3::Y, DVec3::Z],
        }
    }
}

impl Workplane {
    /// Plane through `a` with x towards `b` and `c` on the positive-y side.
    pub fn from_points(a: DVec3, b: DVec3, c: DVec3) -> Result<Self, String> {
        let x = (b - a)
            .try_normalize()
            .ok_or("Первые две точки совпадают")?;
        let z = x
            .cross(c - a)
            .try_normalize()
            .ok_or("Три точки лежат на одной прямой")?;
        Ok(Self {
            origin: a,
            axes: [x, z.cross(x), z],
        })
    }

    /// Horizontal plane at elevation `z` (plan views, levels).
    pub fn at_elevation(z: f64) -> Self {
        Self {
            origin: DVec3::new(0., 0., z),
            ..Self::default()
        }
    }

    /// Intersection of a ray with the plane; None when parallel or behind.
    pub fn intersect(&self, origin: DVec3, direction: DVec3) -> Option<DVec3> {
        let normal = self.axes[2];
        let denominator = direction.dot(normal);
        if denominator.abs() < 1e-12 {
            return None;
        }
        let t = (self.origin - origin).dot(normal) / denominator;
        (t >= 0.0 && t.is_finite()).then(|| origin + direction * t)
    }

    pub fn to_local(&self, p: DVec3) -> DVec3 {
        let d = p - self.origin;
        DVec3::new(
            d.dot(self.axes[0]),
            d.dot(self.axes[1]),
            d.dot(self.axes[2]),
        )
    }

    pub fn to_world(&self, local: DVec3) -> DVec3 {
        self.origin + self.axes[0] * local.x + self.axes[1] * local.y + self.axes[2] * local.z
    }

    pub fn vector_to_world(&self, local: DVec3) -> DVec3 {
        self.axes[0] * local.x + self.axes[1] * local.y + self.axes[2] * local.z
    }
}
