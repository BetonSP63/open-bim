use crate::{ModelError, ObjectId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Material {
    pub id: ObjectId,
    pub name: String,
    pub density_kg_m3: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub id: ObjectId,
    pub name: String,
    pub shape: ProfileShape,
    pub source: String,
}

/// Sharp-corner analytical sections; fillets require separately verified geometry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProfileShape {
    ISection {
        height: f64,
        width: f64,
        web: f64,
        flange: f64,
    },
    Angle {
        height: f64,
        width: f64,
        thickness: f64,
    },
    Channel {
        height: f64,
        width: f64,
        web: f64,
        flange: f64,
    },
    CircularHollow {
        diameter: f64,
        thickness: f64,
    },
    RectangularHollow {
        height: f64,
        width: f64,
        thickness: f64,
    },
    Rectangle {
        height: f64,
        width: f64,
    },
}

impl ProfileShape {
    pub fn area_mm2(&self) -> Result<f64, ModelError> {
        let invalid = || ModelError::InvalidValue("Размеры профиля".into());
        let valid = |values: &[f64]| {
            values
                .iter()
                .all(|v| v.is_finite() && *v > 0.001 && *v <= 1e7)
        };
        let area = match *self {
            Self::ISection {
                height,
                width,
                web,
                flange,
            }
            | Self::Channel {
                height,
                width,
                web,
                flange,
            } => {
                if !valid(&[height, width, web, flange]) || web >= width || 2.0 * flange >= height {
                    return Err(invalid());
                }
                2.0 * width * flange + (height - 2.0 * flange) * web
            }
            Self::Angle {
                height,
                width,
                thickness,
            } => {
                if !valid(&[height, width, thickness]) || thickness >= height.min(width) {
                    return Err(invalid());
                }
                (height + width - thickness) * thickness
            }
            Self::CircularHollow {
                diameter,
                thickness,
            } => {
                if !valid(&[diameter, thickness]) || 2.0 * thickness >= diameter {
                    return Err(invalid());
                }
                std::f64::consts::PI * thickness * (diameter - thickness)
            }
            Self::RectangularHollow {
                height,
                width,
                thickness,
            } => {
                if !valid(&[height, width, thickness]) || 2.0 * thickness >= height.min(width) {
                    return Err(invalid());
                }
                height * width - (height - 2.0 * thickness) * (width - 2.0 * thickness)
            }
            Self::Rectangle { height, width } => {
                if !valid(&[height, width]) {
                    return Err(invalid());
                }
                height * width
            }
        };
        Ok(area)
    }
}

pub(crate) fn builtins() -> (Vec<Material>, Vec<Profile>) {
    let material = Material {
        id: ObjectId(1),
        name: "S235 · тестовый материал".into(),
        density_kg_m3: 7850.0,
    };
    let shapes = [
        (
            "I 300×150×8×12",
            ProfileShape::ISection {
                height: 300.0,
                width: 150.0,
                web: 8.0,
                flange: 12.0,
            },
        ),
        (
            "L 100×75×8",
            ProfileShape::Angle {
                height: 100.0,
                width: 75.0,
                thickness: 8.0,
            },
        ),
        (
            "C 200×75×6×10",
            ProfileShape::Channel {
                height: 200.0,
                width: 75.0,
                web: 6.0,
                flange: 10.0,
            },
        ),
        (
            "CHS 114×6",
            ProfileShape::CircularHollow {
                diameter: 114.0,
                thickness: 6.0,
            },
        ),
        (
            "RHS 150×100×6",
            ProfileShape::RectangularHollow {
                height: 150.0,
                width: 100.0,
                thickness: 6.0,
            },
        ),
        (
            "R 100×50",
            ProfileShape::Rectangle {
                height: 100.0,
                width: 50.0,
            },
        ),
    ];
    let profiles = shapes.into_iter().enumerate().map(|(i, (name, shape))| Profile {
        id: ObjectId(i as u64 + 2), name: name.into(), shape,
        source: "Аналитическое сечение без скруглений; синтетический тестовый каталог Open BIM, не сортамент ГОСТ".into(),
    }).collect();
    (vec![material], profiles)
}
