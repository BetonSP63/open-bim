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
    /// Standard of a catalogue section («ГОСТ 26020-83»), shown in the
    /// rolled-steel specification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub standard: Option<String>,
    /// Catalogue area with fillets, mm²; the mass uses it instead of the
    /// sharp-corner section.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub area_mm2: Option<f64>,
}

impl Profile {
    /// Area used for the mass: the catalogue area, else the section's own.
    pub fn area_mm2(&self) -> Result<f64, ModelError> {
        let own = self.shape.area_mm2()?;
        match self.area_mm2 {
            Some(a) if a.is_finite() && a > 0.0 && a <= 1e7 => Ok(a),
            Some(_) => Err(ModelError::InvalidValue(
                "Площадь профиля из каталога".into(),
            )),
            None => Ok(own),
        }
    }
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
    /// User contour with optional voids, in section coordinates.
    Polygon {
        outer: Vec<[f64; 2]>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        voids: Vec<Vec<[f64; 2]>>,
    },
    /// Section welded from plates; one member in the model, separate plates
    /// and seams in the specifications.
    Welded {
        plates: Vec<crate::sections::SectionPlate>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        welds: Vec<crate::sections::SectionWeld>,
    },
}

impl ProfileShape {
    /// Outer loop and voids of the section; circles become 64-gons.
    pub fn outline(&self) -> Result<crate::sections::Outline, ModelError> {
        self.area_mm2()?;
        let rect = |w: f64, h: f64| {
            vec![
                [-w / 2.0, -h / 2.0],
                [w / 2.0, -h / 2.0],
                [w / 2.0, h / 2.0],
                [-w / 2.0, h / 2.0],
            ]
        };
        Ok(match self {
            Self::Polygon { outer, voids } => (outer.clone(), voids.clone()),
            Self::Welded { plates, .. } => crate::sections::welded_outline(plates)?,
            Self::RectangularHollow {
                height,
                width,
                thickness,
            } => (
                rect(*width, *height),
                vec![rect(width - 2.0 * thickness, height - 2.0 * thickness)],
            ),
            Self::CircularHollow {
                diameter,
                thickness,
            } => {
                let circle = |r: f64| -> Vec<[f64; 2]> {
                    (0..64)
                        .map(|i| {
                            let a = i as f64 * std::f64::consts::TAU / 64.0;
                            [a.cos() * r, a.sin() * r]
                        })
                        .collect()
                };
                (
                    circle(diameter / 2.0),
                    vec![circle(diameter / 2.0 - thickness)],
                )
            }
            _ => (self.solid_contour()?, Vec::new()),
        })
    }

    /// Box of the section [min x, max x, min y, max y].
    pub fn bounds(&self) -> Result<[f64; 4], ModelError> {
        let (outer, _) = self.outline()?;
        Ok(outer.iter().fold(
            [
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
            ],
            |b, p| {
                [
                    b[0].min(p[0]),
                    b[1].max(p[0]),
                    b[2].min(p[1]),
                    b[3].max(p[1]),
                ]
            },
        ))
    }

    /// Nominal [height, width, web, flange] of a section: hollow sections and
    /// angles give their wall for both, a solid bar its width and height. A
    /// welded section takes the plate through the centre as web and the top
    /// plate as flange; a contour its mean wall (area over half perimeter).
    pub fn sizes(&self) -> [f64; 4] {
        match self {
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
            } => [*height, *width, *web, *flange],
            Self::Angle {
                height,
                width,
                thickness,
            }
            | Self::RectangularHollow {
                height,
                width,
                thickness,
            } => [*height, *width, *thickness, *thickness],
            Self::CircularHollow {
                diameter,
                thickness,
            } => [*diameter, *diameter, *thickness, *thickness],
            Self::Rectangle { height, width } => [*height, *width, *width, *height],
            Self::Polygon { .. } | Self::Welded { .. } => {
                let Ok([x0, x1, y0, y1]) = self.bounds() else {
                    return [0.0; 4];
                };
                let (h, b) = (y1 - y0, x1 - x0);
                if let Self::Welded { plates, .. } = self {
                    let web = plates
                        .iter()
                        .filter(|p| p.height > p.width)
                        .max_by(|a, b| a.height.total_cmp(&b.height))
                        .map_or(0.0, |p| p.width);
                    let flange = plates
                        .iter()
                        .filter(|p| p.width >= p.height)
                        .max_by(|a, b| a.center[1].total_cmp(&b.center[1]))
                        .map_or(0.0, |p| p.height);
                    let wall = plates
                        .iter()
                        .map(|p| p.thickness())
                        .fold(f64::INFINITY, f64::min);
                    let pick = |v: f64| if v > 0.0 { v } else { wall };
                    return [h, b, pick(web), pick(flange)];
                }
                let (Ok(area), Ok((outer, voids))) = (self.area_mm2(), self.outline()) else {
                    return [h, b, 0.0, 0.0];
                };
                let length = |r: &[[f64; 2]]| {
                    (0..r.len())
                        .map(|i| {
                            let (a, b) = (r[i], r[(i + 1) % r.len()]);
                            (b[0] - a[0]).hypot(b[1] - a[1])
                        })
                        .sum::<f64>()
                };
                let perimeter = length(&outer) + voids.iter().map(|v| length(v)).sum::<f64>();
                let wall = (2.0 * area / perimeter).min(h.min(b));
                [h, b, wall, wall]
            }
        }
    }

    pub fn solid_contour(&self) -> Result<Vec<[f64; 2]>, ModelError> {
        self.area_mm2()?;
        match self {
            Self::Polygon { outer, voids } if voids.is_empty() => return Ok(outer.clone()),
            Self::Welded { .. } => {
                let (outer, voids) = self.outline()?;
                if voids.is_empty() {
                    return Ok(outer);
                }
            }
            _ => {}
        }
        match *self {
            Self::ISection {
                height: h,
                width: w,
                web: tw,
                flange: tf,
            } => Ok([
                [-w / 2.0, -h / 2.0],
                [w / 2.0, -h / 2.0],
                [w / 2.0, -h / 2.0 + tf],
                [tw / 2.0, -h / 2.0 + tf],
                [tw / 2.0, h / 2.0 - tf],
                [w / 2.0, h / 2.0 - tf],
                [w / 2.0, h / 2.0],
                [-w / 2.0, h / 2.0],
                [-w / 2.0, h / 2.0 - tf],
                [-tw / 2.0, h / 2.0 - tf],
                [-tw / 2.0, -h / 2.0 + tf],
                [-w / 2.0, -h / 2.0 + tf],
            ]
            .to_vec()),
            Self::Angle {
                height: h,
                width: w,
                thickness: t,
            } => Ok([
                [-w / 2.0, -h / 2.0],
                [w / 2.0, -h / 2.0],
                [w / 2.0, -h / 2.0 + t],
                [-w / 2.0 + t, -h / 2.0 + t],
                [-w / 2.0 + t, h / 2.0],
                [-w / 2.0, h / 2.0],
            ]
            .to_vec()),
            Self::Channel {
                height: h,
                width: w,
                web: tw,
                flange: tf,
            } => Ok([
                [-w / 2.0, -h / 2.0],
                [w / 2.0, -h / 2.0],
                [w / 2.0, -h / 2.0 + tf],
                [-w / 2.0 + tw, -h / 2.0 + tf],
                [-w / 2.0 + tw, h / 2.0 - tf],
                [w / 2.0, h / 2.0 - tf],
                [w / 2.0, h / 2.0],
                [-w / 2.0, h / 2.0],
            ]
            .to_vec()),
            Self::Rectangle {
                height: h,
                width: w,
            } => Ok([
                [-w / 2.0, -h / 2.0],
                [w / 2.0, -h / 2.0],
                [w / 2.0, h / 2.0],
                [-w / 2.0, h / 2.0],
            ]
            .to_vec()),
            Self::CircularHollow { .. }
            | Self::RectangularHollow { .. }
            | Self::Polygon { .. }
            | Self::Welded { .. } => Err(ModelError::InvalidValue(
                "Полое сечение требует тела с внутренним контуром".into(),
            )),
        }
    }

    pub fn area_mm2(&self) -> Result<f64, ModelError> {
        let invalid = || ModelError::InvalidValue("Размеры профиля".into());
        let valid = |values: &[f64]| {
            values
                .iter()
                .all(|v| v.is_finite() && *v > 0.001 && *v <= 1e7)
        };
        match self {
            Self::Polygon { outer, voids } => {
                if outer.len() > crate::sections::MAX_POINTS
                    || voids.len() > 16
                    || voids.iter().any(|v| v.len() > crate::sections::MAX_POINTS)
                {
                    return Err(invalid());
                }
                let mut area = crate::polygon_area(outer)?;
                for v in voids {
                    area -= crate::polygon_area(v)?;
                }
                if area <= 0.001 {
                    return Err(invalid());
                }
                return Ok(area);
            }
            Self::Welded { plates, welds } => {
                crate::sections::check_welded(plates, welds)?;
                return Ok(plates.iter().map(|p| p.width * p.height).sum());
            }
            _ => {}
        };
        let area = match *self {
            Self::Polygon { .. } | Self::Welded { .. } => unreachable!(),
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
        source: "Аналитическое сечение без скруглений; синтетический тестовый каталог Open BIM, не сортамент ГОСТ".into(), standard: None, area_mm2: None }).collect();
    (vec![material], profiles)
}
