//! Graphic objects drawn on a sheet by the user: lines and polylines,
//! rectangles, circles, hatched areas (ГОСТ 2.306 patterns) and masking
//! (wipeout) areas. Coordinates are sheet millimetres; the objects are part
//! of the drawing definition and survive regeneration.
use crate::ModelError;
use serde::{Deserialize, Serialize};

/// Line type (ГОСТ 2.303).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineType {
    #[default]
    Solid,
    Dashed,
    DashDot,
}

impl LineType {
    pub const ALL: [LineType; 3] = [Self::Solid, Self::Dashed, Self::DashDot];
    pub fn label(self) -> &'static str {
        match self {
            Self::Solid => "Сплошная",
            Self::Dashed => "Штриховая",
            Self::DashDot => "Штрихпунктирная",
        }
    }
}

/// Hatch pattern of a section material (ГОСТ 2.306).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HatchPattern {
    /// Metals: parallel thin lines at 45°.
    #[default]
    Metal,
    /// Non-metals: crossing lines.
    NonMetal,
}

impl HatchPattern {
    pub const ALL: [HatchPattern; 2] = [Self::Metal, Self::NonMetal];
    pub fn label(self) -> &'static str {
        match self {
            Self::Metal => "Металл (45°)",
            Self::NonMetal => "Неметалл (сетка)",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SheetObjectKind {
    /// Open polyline of two or more points (a line has two).
    Line {
        points: Vec<[f64; 2]>,
    },
    Rectangle {
        a: [f64; 2],
        b: [f64; 2],
    },
    Circle {
        center: [f64; 2],
        radius: f64,
    },
    /// Closed area filled with pattern lines at `angle` degrees, `spacing` mm apart.
    Hatch {
        polygon: Vec<[f64; 2]>,
        pattern: HatchPattern,
        angle: f64,
        spacing: f64,
    },
    /// Closed area hiding everything drawn under it.
    Wipeout {
        polygon: Vec<[f64; 2]>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SheetObject {
    pub id: u32,
    pub kind: SheetObjectKind,
    #[serde(default, skip_serializing_if = "is_solid")]
    pub line_type: LineType,
    /// Thick (S) line instead of thin (S/3).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub thick: bool,
}

fn is_solid(t: &LineType) -> bool {
    *t == LineType::Solid
}

impl SheetObject {
    /// Move by a sheet offset.
    pub fn moved(&self, d: [f64; 2]) -> Self {
        let shift = |p: [f64; 2]| [p[0] + d[0], p[1] + d[1]];
        let kind = match &self.kind {
            SheetObjectKind::Line { points } => SheetObjectKind::Line {
                points: points.iter().map(|p| shift(*p)).collect(),
            },
            SheetObjectKind::Rectangle { a, b } => SheetObjectKind::Rectangle {
                a: shift(*a),
                b: shift(*b),
            },
            SheetObjectKind::Circle { center, radius } => SheetObjectKind::Circle {
                center: shift(*center),
                radius: *radius,
            },
            SheetObjectKind::Hatch {
                polygon,
                pattern,
                angle,
                spacing,
            } => SheetObjectKind::Hatch {
                polygon: polygon.iter().map(|p| shift(*p)).collect(),
                pattern: *pattern,
                angle: *angle,
                spacing: *spacing,
            },
            SheetObjectKind::Wipeout { polygon } => SheetObjectKind::Wipeout {
                polygon: polygon.iter().map(|p| shift(*p)).collect(),
            },
        };
        Self {
            kind,
            ..self.clone()
        }
    }

    pub fn validate(&self, paper: [f64; 2]) -> Result<(), ModelError> {
        let invalid = |m: &str| Err(ModelError::InvalidValue(format!("Объект чертежа: {m}")));
        let on_sheet = |p: &[f64; 2]| {
            p.iter().all(|v| v.is_finite())
                && (-1. ..=paper[0] + 1.).contains(&p[0])
                && (-1. ..=paper[1] + 1.).contains(&p[1])
        };
        let area = |poly: &[[f64; 2]]| {
            poly.len() >= 3
                && poly.len() <= 10_000
                && poly.iter().all(on_sheet)
                && crate::polygon_area(poly).is_ok()
        };
        match &self.kind {
            SheetObjectKind::Line { points } => {
                if points.len() < 2 || points.len() > 10_000 || !points.iter().all(on_sheet) {
                    return invalid("линия");
                }
            }
            SheetObjectKind::Rectangle { a, b } => {
                if !on_sheet(a)
                    || !on_sheet(b)
                    || (a[0] - b[0]).abs() < 0.1
                    || (a[1] - b[1]).abs() < 0.1
                {
                    return invalid("прямоугольник");
                }
            }
            SheetObjectKind::Circle { center, radius } => {
                if !on_sheet(center) || !(radius.is_finite() && (0.1..=2000.).contains(radius)) {
                    return invalid("окружность");
                }
            }
            SheetObjectKind::Hatch {
                polygon,
                angle,
                spacing,
                ..
            } => {
                if !area(polygon)
                    || !angle.is_finite()
                    || !(spacing.is_finite() && (0.5..=50.).contains(spacing))
                {
                    return invalid("штриховка: контур, угол, шаг 0,5…50 мм");
                }
            }
            SheetObjectKind::Wipeout { polygon } => {
                if !area(polygon) {
                    return invalid("маскировка: замкнутый контур");
                }
            }
        }
        Ok(())
    }
}
