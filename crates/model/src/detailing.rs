use crate::{Frame, ModelError, ObjectId, Point3, Project};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeldType {
    /// Угловой шов таврового соединения.
    Fillet,
    /// Стыковой со скосом двух кромок (V).
    Butt,
    /// Стыковой без скоса кромок.
    ButtSquare,
    /// Стыковой с двумя симметричными скосами (X), двусторонний.
    ButtX,
    /// Нахлёсточный.
    Lap,
}

/// Which sides of the joint are welded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeldSides {
    #[default]
    One,
    Both,
}

impl WeldType {
    pub const ALL: [WeldType; 5] = [
        Self::Fillet,
        Self::ButtSquare,
        Self::Butt,
        Self::ButtX,
        Self::Lap,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Fillet => "угловой (тавровое соединение)",
            Self::ButtSquare => "стыковой без скоса кромок",
            Self::Butt => "стыковой V-образный",
            Self::ButtX => "стыковой X-образный",
            Self::Lap => "нахлёсточный",
        }
    }
    /// Joint designation in the standard of `process` for the sides. ГОСТ
    /// 5264 and 14771 share the designations; ГОСТ 8713 has no С2 and С17,
    /// its one-sided butt joints are С4 (square) and С18 (bevelled).
    pub fn joint(self, sides: WeldSides, process: WeldProcess) -> &'static str {
        match (self, sides, process) {
            (Self::ButtSquare, WeldSides::One, WeldProcess::Submerged) => return "С4",
            (Self::Butt, WeldSides::One, WeldProcess::Submerged) => return "С18",
            _ => {}
        }
        match (self, sides) {
            (Self::Fillet, WeldSides::One) => "Т1",
            (Self::Fillet, WeldSides::Both) => "Т3",
            (Self::ButtSquare, WeldSides::One) => "С2",
            (Self::ButtSquare, WeldSides::Both) => "С7",
            (Self::Butt, WeldSides::One) => "С17",
            (Self::Butt, WeldSides::Both) => "С21",
            (Self::ButtX, _) => "С25",
            (Self::Lap, WeldSides::One) => "Н1",
            (Self::Lap, WeldSides::Both) => "Н2",
        }
    }
}

/// Welding process; it names the standard of the joint types.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeldProcess {
    /// Ручная дуговая сварка, ГОСТ 5264-80.
    #[default]
    Manual,
    /// Дуговая сварка в защитном газе, ГОСТ 14771-76.
    GasShielded,
    /// Автоматическая и механизированная сварка под флюсом, ГОСТ 8713-79.
    Submerged,
}

impl WeldProcess {
    pub const ALL: [WeldProcess; 3] = [Self::Manual, Self::GasShielded, Self::Submerged];
    pub fn standard(self) -> &'static str {
        match self {
            Self::Manual => "ГОСТ 5264-80",
            Self::GasShielded => "ГОСТ 14771-76",
            Self::Submerged => "ГОСТ 8713-79",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Manual => "ручная дуговая",
            Self::GasShielded => "в защитном газе",
            Self::Submerged => "под флюсом",
        }
    }
    /// Title of the standard for the list of referenced documents.
    pub fn title(self) -> &'static str {
        match self {
            Self::Manual => {
                "Ручная дуговая сварка. Соединения сварные. Основные типы, конструктивные элементы и размеры"
            }
            Self::GasShielded => {
                "Дуговая сварка в защитном газе. Соединения сварные. Основные типы, конструктивные элементы и размеры"
            }
            Self::Submerged => {
                "Сварка под флюсом. Соединения сварные. Основные типы, конструктивные элементы и размеры"
            }
        }
    }
}

/// Nut and washer standards of a bolt set: high-strength bolts of ГОСТ Р
/// 52644 come with ГОСТ Р 52645 nuts and ГОСТ Р 52646 washers, other bolts
/// with ГОСТ 5915-70 nuts and ГОСТ 11371-78 washers.
pub fn bolt_set(standard: &str) -> (&'static str, &'static str) {
    if standard.contains("52644") {
        ("ГОСТ Р 52645-2006", "ГОСТ Р 52646-2006")
    } else {
        ("ГОСТ 5915-70", "ГОСТ 11371-78")
    }
}

fn is_manual(p: &WeldProcess) -> bool {
    *p == WeldProcess::Manual
}

fn is_one_side(s: &WeldSides) -> bool {
    *s == WeldSides::One
}

fn size_text(size: f64) -> String {
    if size.fract().abs() < 1e-9 {
        format!("{size:.0}")
    } else {
        format!("{size}").replace('.', ",")
    }
}

/// Leg sign of ГОСТ 2.312 (right triangle). The sheet font has no glyph for
/// it, so drawings draw it as lines and the screen shows «Δ».
pub const LEG_SIGN: char = '⊿';

/// Weld mark text in the style of ГОСТ 2.312: standard, joint, leg size
/// (fillet and lap welds), «по контуру» and site welding.
pub fn weld_mark(
    process: WeldProcess,
    weld_type: WeldType,
    sides: WeldSides,
    size: f64,
    around: bool,
    shop: bool,
) -> String {
    let leg = matches!(weld_type, WeldType::Fillet | WeldType::Lap)
        .then(|| format!("-{LEG_SIGN}{}", size_text(size)))
        .unwrap_or_default();
    let mut mark = format!(
        "{}-{}{leg}",
        process.standard(),
        weld_type.joint(sides, process)
    );
    if around {
        mark.push_str(" по контуру");
    }
    if !shop {
        mark.push_str(" (монт.)");
    }
    mark
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Detail {
    pub id: ObjectId,
    pub kind: DetailKind,
}

/// Feature coordinates are local to the part; bolt frames and weld paths are
/// world coordinates. Hole references are stable IDs, never vector offsets.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DetailKind {
    PlaneCut {
        part: ObjectId,
        normal: [f64; 3],
        offset: f64,
    },
    BoxCut {
        part: ObjectId,
        origin: Point3,
        size: [f64; 3],
    },
    /// Prism cut in the part's local frame: `contour` in the XY plane of
    /// `frame`, extruded `length` along its Z (cut by polygon, cut by part).
    PrismCut {
        part: ObjectId,
        frame: crate::Frame,
        contour: Vec<[f64; 2]>,
        length: f64,
    },
    Hole {
        part: ObjectId,
        frame: Frame,
        diameter: f64,
        depth: f64,
    },
    BoltGroup {
        parts: BTreeSet<ObjectId>,
        holes: BTreeSet<ObjectId>,
        frame: Frame,
        diameter: f64,
        clearance: f64,
        steps_x: Vec<f64>,
        steps_y: Vec<f64>,
        standard: String,
        grade: String,
        /// Bolt positions in the group frame for the «Окружность» and
        /// «Список» shapes; empty for «Массив» (`steps_x`/`steps_y`).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        points: Vec<[f64; 2]>,
    },
    /// `parts` has one part for a weld on a part («шов на детали»).
    Weld {
        parts: BTreeSet<ObjectId>,
        weld_type: WeldType,
        size: f64,
        path: Vec<Point3>,
        shop: bool,
        #[serde(default, skip_serializing_if = "is_one_side")]
        sides: WeldSides,
        /// Welded all around the joint («по контуру»).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        around: bool,
        #[serde(default, skip_serializing_if = "is_manual")]
        process: WeldProcess,
    },
}

/// Bolt positions of a group in its frame: the array of `steps_x` ×
/// `steps_y` from the origin, or the explicit `points` when given.
pub fn bolt_positions(steps_x: &[f64], steps_y: &[f64], points: &[[f64; 2]]) -> Vec<[f64; 2]> {
    if !points.is_empty() {
        return points.to_vec();
    }
    let prefix = |steps: &[f64]| {
        let mut out = vec![0.0];
        for step in steps {
            out.push(out.last().unwrap() + step);
        }
        out
    };
    let ys = prefix(steps_y);
    prefix(steps_x)
        .into_iter()
        .flat_map(|x| ys.iter().map(move |y| [x, *y]))
        .collect()
}

/// Bolts on a circle of `diameter` around the group origin, the first on its X axis.
pub fn circle_points(count: usize, diameter: f64) -> Vec<[f64; 2]> {
    (0..count)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / count as f64;
            let p = [diameter / 2.0 * a.cos(), diameter / 2.0 * a.sin()];
            // Exact zeros keep saved files and hashes clean.
            p.map(|v| if v.abs() < 1e-9 { 0.0 } else { v })
        })
        .collect()
}

impl DetailKind {
    /// Bolt positions of a bolt group in its frame; empty for other details.
    pub fn bolt_positions(&self) -> Vec<[f64; 2]> {
        match self {
            Self::BoltGroup {
                steps_x,
                steps_y,
                points,
                ..
            } => bolt_positions(steps_x, steps_y, points),
            _ => Vec::new(),
        }
    }
    pub fn part(&self) -> Option<ObjectId> {
        match self {
            Self::PlaneCut { part, .. }
            | Self::BoxCut { part, .. }
            | Self::PrismCut { part, .. }
            | Self::Hole { part, .. } => Some(*part),
            _ => None,
        }
    }
    pub fn references(&self) -> BTreeSet<ObjectId> {
        match self {
            Self::BoltGroup { parts, holes, .. } => parts.union(holes).copied().collect(),
            Self::Weld { parts, .. } => parts.clone(),
            _ => self.part().into_iter().collect(),
        }
    }
    pub fn validate(&self, project: &Project) -> Result<(), ModelError> {
        let invalid = || ModelError::InvalidValue("Invalid steel detailing parameters".into());
        let positive = |v: f64| v.is_finite() && v > 0.001 && v <= 1e7;
        let part = |id: ObjectId| {
            project
                .elements
                .contains_key(&id)
                .then_some(())
                .ok_or(ModelError::MissingReference(id))
        };
        match self {
            Self::PlaneCut {
                part: id,
                normal,
                offset,
            } => {
                part(*id)?;
                if !normal.iter().all(|v| v.is_finite())
                    || normal.iter().map(|v| v * v).sum::<f64>() < 1e-16
                    || !offset.is_finite()
                {
                    return Err(invalid());
                }
            }
            Self::BoxCut {
                part: id,
                origin,
                size,
            } => {
                part(*id)?;
                crate::validation::valid_point(*origin)?;
                if !size.iter().all(|v| positive(*v)) {
                    return Err(invalid());
                }
            }
            Self::PrismCut {
                part: id,
                frame,
                contour,
                length,
            } => {
                part(*id)?;
                frame.validate()?;
                if contour.len() < 3
                    || contour.len() > 10_000
                    || !contour
                        .iter()
                        .flatten()
                        .all(|v| v.is_finite() && v.abs() <= 1e7)
                    || !positive(*length)
                    || crate::polygon_area(contour).is_err()
                {
                    return Err(invalid());
                }
            }
            Self::Hole {
                part: id,
                frame,
                diameter,
                depth,
            } => {
                part(*id)?;
                frame.validate()?;
                if !positive(*diameter) || !positive(*depth) {
                    return Err(invalid());
                }
            }
            Self::BoltGroup {
                parts,
                holes,
                frame,
                diameter,
                clearance,
                steps_x,
                steps_y,
                standard,
                grade,
                points,
            } => {
                frame.validate()?;
                let pitch = *diameter + clearance;
                let points_ok = points.is_empty()
                    || (steps_x.is_empty()
                        && steps_y.is_empty()
                        && points.len() <= 1000
                        && points
                            .iter()
                            .all(|p| p.iter().all(|v| v.is_finite() && v.abs() <= 1e7))
                        && points.iter().enumerate().all(|(i, a)| {
                            points[i + 1..]
                                .iter()
                                .all(|b| (a[0] - b[0]).hypot(a[1] - b[1]) > pitch)
                        }));
                if !points_ok
                    || parts.is_empty()
                    || !positive(*diameter)
                    || !clearance.is_finite()
                    || *clearance < 0.0
                    || !positive(*diameter + clearance)
                    || standard.trim().is_empty()
                    || grade.trim().is_empty()
                    || standard.len() > 1024
                    || grade.len() > 1024
                    || standard.contains('\0')
                    || grade.contains('\0')
                    || steps_x.len() > 1000
                    || steps_y.len() > 1000
                    || !steps_x
                        .iter()
                        .chain(steps_y)
                        .all(|v| positive(*v) && *v > *diameter + clearance)
                    || (steps_x.len() + 1) * (steps_y.len() + 1) > 10000
                {
                    return Err(invalid());
                }
                for id in parts {
                    part(*id)?;
                }
                let positions = bolt_positions(steps_x, steps_y, points);
                let count = positions.len();
                if holes.len() != count * parts.len() {
                    return Err(invalid());
                }
                let mut per_part = std::collections::BTreeMap::<ObjectId, usize>::new();
                let mut occupied = std::collections::BTreeSet::new();
                for id in holes {
                    let detail = project
                        .details
                        .get(id)
                        .ok_or(ModelError::MissingReference(*id))?;
                    let DetailKind::Hole {
                        part,
                        diameter: hole_diameter,
                        frame: hole_frame,
                        ..
                    } = &detail.kind
                    else {
                        return Err(invalid());
                    };
                    if !parts.contains(part) || (*hole_diameter - diameter - clearance).abs() > 1e-8
                    {
                        return Err(invalid());
                    }
                    let parent = project.elements[part].shape.frame();
                    let world = parent.world(hole_frame.origin);
                    let delta: [f64; 3] = std::array::from_fn(|i| world.0[i] - frame.origin.0[i]);
                    let dot = |a: [f64; 3], b: [f64; 3]| (0..3).map(|i| a[i] * b[i]).sum::<f64>();
                    let world_axis: [f64; 3] = std::array::from_fn(|i| {
                        (0..3)
                            .map(|j| parent.axes[j][i] * hole_frame.axes[2][j])
                            .sum()
                    });
                    if dot(world_axis, frame.axes[2]).abs() < 1.0 - 1e-9 {
                        return Err(invalid());
                    }
                    let (x, y) = (dot(delta, frame.axes[0]), dot(delta, frame.axes[1]));
                    let index = positions
                        .iter()
                        .position(|p| (p[0] - x).abs() < 1e-6 && (p[1] - y).abs() < 1e-6)
                        .ok_or_else(invalid)?;
                    if !occupied.insert((*part, index)) {
                        return Err(invalid());
                    }
                    *per_part.entry(*part).or_default() += 1;
                }
                if parts.iter().any(|p| per_part.get(p) != Some(&count)) {
                    return Err(invalid());
                }
            }
            Self::Weld {
                parts, size, path, ..
            } => {
                if parts.is_empty() || !positive(*size) || path.len() < 2 || path.len() > 10000 {
                    return Err(invalid());
                }
                for id in parts {
                    part(*id)?;
                }
                for point in path {
                    crate::validation::valid_point(*point)?;
                }
                if path.windows(2).any(|p| p[0].distance(p[1]) <= 0.001) {
                    return Err(invalid());
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_name_the_standard_of_the_process() {
        let mark = |p, t, s| weld_mark(p, t, s, 6.0, false, true);
        use {WeldProcess::*, WeldSides::*, WeldType::*};
        assert_eq!(mark(Manual, Fillet, One), "ГОСТ 5264-80-Т1-⊿6");
        assert_eq!(mark(GasShielded, Fillet, Both), "ГОСТ 14771-76-Т3-⊿6");
        assert_eq!(mark(GasShielded, Butt, One), "ГОСТ 14771-76-С17");
        // ГОСТ 8713 has no С2 and С17.
        assert_eq!(mark(Submerged, ButtSquare, One), "ГОСТ 8713-79-С4");
        assert_eq!(mark(Submerged, Butt, One), "ГОСТ 8713-79-С18");
        assert_eq!(mark(Submerged, Butt, Both), "ГОСТ 8713-79-С21");
        assert_eq!(mark(Submerged, Lap, Both), "ГОСТ 8713-79-Н2-⊿6");
        assert_eq!(
            weld_mark(Manual, Fillet, One, 8.0, true, false),
            "ГОСТ 5264-80-Т1-⊿8 по контуру (монт.)"
        );
    }

    #[test]
    fn manual_welds_keep_their_saved_form() {
        let json = r#"{"kind":"weld","parts":[1,2],"weld_type":"fillet","size":6.0,"path":[[0.0,0.0,0.0],[1.0,0.0,0.0]],"shop":true}"#;
        let kind: DetailKind = serde_json::from_str(json).unwrap();
        assert!(matches!(
            kind,
            DetailKind::Weld {
                process: WeldProcess::Manual,
                ..
            }
        ));
        assert_eq!(serde_json::to_string(&kind).unwrap(), json);
    }
}
