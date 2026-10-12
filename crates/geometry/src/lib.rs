//! Double precision coordinates in millimetres; independent of presentation.

mod kernel;
mod prism_mesh;
pub use kernel::{Body, BodyError, Mesh, TOLERANCE_MM, Tessellation64};

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Point3(pub [f64; 3]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeometryError {
    NonFiniteCoordinate,
    NonPositiveLength,
}

impl std::fmt::Display for GeometryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFiniteCoordinate => f.write_str("Координаты должны быть конечными числами"),
            Self::NonPositiveLength => f.write_str("Размер должен быть положительным"),
        }
    }
}

impl std::error::Error for GeometryError {}

impl Point3 {
    pub fn new(x: f64, y: f64, z: f64) -> Result<Self, GeometryError> {
        let coordinates = [x, y, z];
        if !coordinates.iter().all(|v| v.is_finite()) {
            return Err(GeometryError::NonFiniteCoordinate);
        }
        Ok(Self(coordinates))
    }

    pub fn distance(self, other: Self) -> f64 {
        let delta = std::array::from_fn::<_, 3, _>(|i| self.0[i] - other.0[i]);
        delta[0].hypot(delta[1]).hypot(delta[2])
    }
}

/// Validated physical length in millimetres.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct LengthMm(f64);

impl LengthMm {
    pub fn new(value: f64) -> Result<Self, GeometryError> {
        if !value.is_finite() || value <= 0.0 {
            return Err(GeometryError::NonPositiveLength);
        }
        Ok(Self(value))
    }

    pub fn value(self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for LengthMm {
    type Error = GeometryError;
    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<LengthMm> for f64 {
    fn from(value: LengthMm) -> Self {
        value.value()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_physical_input() {
        for value in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(LengthMm::new(value).is_err());
        }
        assert!(Point3::new(f64::NAN, 0.0, 0.0).is_err());
    }

    #[test]
    fn calculates_distance_at_large_world_offset() {
        let a = Point3::new(1e6, 1e6, 1e6).unwrap();
        let b = Point3::new(1e6 + 3.0, 1e6 + 4.0, 1e6).unwrap();
        assert_eq!(a.distance(b), 5.0);
    }
}
