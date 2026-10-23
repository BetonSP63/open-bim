use crate::{ModelError, ObjectId, Point3};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Frame {
    pub origin: Point3,
    /// Columns: local x, y, z directions in world coordinates.
    pub axes: [[f64; 3]; 3],
}

impl Default for Frame {
    fn default() -> Self {
        Self {
            origin: Point3([0.0; 3]),
            axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        }
    }
}

impl Frame {
    pub fn validate(&self) -> Result<(), ModelError> {
        crate::validation::valid_point(self.origin)?;
        let dot = |a: [f64; 3], b: [f64; 3]| a.into_iter().zip(b).map(|(x, y)| x * y).sum::<f64>();
        for i in 0..3 {
            if !self.axes[i].iter().all(|v| v.is_finite())
                || (dot(self.axes[i], self.axes[i]) - 1.0).abs() > 1e-9
            {
                return Err(ModelError::InvalidValue(
                    "Оси локальной системы должны быть единичными".into(),
                ));
            }
            for j in i + 1..3 {
                if dot(self.axes[i], self.axes[j]).abs() > 1e-9 {
                    return Err(ModelError::InvalidValue(
                        "Оси должны быть ортогональны".into(),
                    ));
                }
            }
        }
        let [x, y, z] = self.axes;
        let cross = [
            x[1] * y[2] - x[2] * y[1],
            x[2] * y[0] - x[0] * y[2],
            x[0] * y[1] - x[1] * y[0],
        ];
        if dot(cross, z) < 1.0 - 1e-9 {
            return Err(ModelError::InvalidValue(
                "Локальная система должна быть правой".into(),
            ));
        }
        Ok(())
    }

    pub fn world(&self, local: Point3) -> Point3 {
        Point3(std::array::from_fn(|i| {
            self.origin.0[i] + (0..3).map(|j| self.axes[j][i] * local.0[j]).sum::<f64>()
        }))
    }

    pub fn along(start: Point3, end: Point3) -> Result<(Self, crate::LengthMm), ModelError> {
        crate::validation::valid_point(start)?;
        crate::validation::valid_point(end)?;
        let length = start.distance(end);
        if length <= 0.001 {
            return Err(ModelError::InvalidValue("Нулевая длина элемента".into()));
        }
        let z: [f64; 3] = std::array::from_fn(|i| (end.0[i] - start.0[i]) / length);
        let reference = if z[2].abs() < 0.9 {
            [0.0, 0.0, 1.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        let mut x = [
            reference[1] * z[2] - reference[2] * z[1],
            reference[2] * z[0] - reference[0] * z[2],
            reference[0] * z[1] - reference[1] * z[0],
        ];
        let norm = x.into_iter().map(|v| v * v).sum::<f64>().sqrt();
        x.iter_mut().for_each(|v| *v /= norm);
        let y = [
            z[1] * x[2] - z[2] * x[1],
            z[2] * x[0] - z[0] * x[2],
            z[0] * x[1] - z[1] * x[0],
        ];
        let frame = Self {
            origin: start,
            axes: [x, y, z],
        };
        frame.validate()?;
        Ok((
            frame,
            crate::LengthMm::new(length).map_err(|e| ModelError::InvalidValue(e.to_string()))?,
        ))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GridAxis {
    pub id: ObjectId,
    pub name: String,
    pub start: Point3,
    pub end: Point3,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Level {
    pub id: ObjectId,
    pub name: String,
    pub elevation_mm: f64,
}

/// Look and behaviour of the grid («Прямоугольная сетка» properties).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct GridSettings {
    /// Part ends lying on a grid line move with it when the grid changes
    /// («Магнитная плоскость сетки»).
    pub magnetic: bool,
    /// Line colour, RGB.
    pub color: [u8; 3],
    /// Label text height in points and its colour.
    pub label_size: f32,
    pub label_color: [u8; 3],
}

impl Default for GridSettings {
    fn default() -> Self {
        Self {
            magnetic: false,
            color: [0, 0, 0],
            label_size: 12.,
            label_color: [0, 0, 0],
        }
    }
}

impl GridSettings {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn validate(&self) -> Result<(), crate::ModelError> {
        if !(6.0..=48.0).contains(&self.label_size) {
            return Err(crate::ModelError::InvalidValue(
                "Размер шрифта подписи сетки: от 6 до 48".into(),
            ));
        }
        Ok(())
    }
}
