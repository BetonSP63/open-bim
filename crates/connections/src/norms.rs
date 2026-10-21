//! Russian design rules for connection details: bolts by ГОСТ 7798-70,
//! ГОСТ 7805-70 and ГОСТ Р 52644-2006, holes by ГОСТ 11284-75, bolt spacing
//! and minimum fillet welds by СП 16.13330.2017. The checks give advice: the
//! template may still be valid geometry while breaking a rule, and the user
//! decides.
use open_bim_model::{ConnectionTemplate, TemplateKind};

/// A bolt standard with its usual diameters and strength classes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoltStandard {
    pub name: &'static str,
    pub title: &'static str,
    pub diameters: &'static [f64],
    pub grades: &'static [&'static str],
    pub default_grade: &'static str,
}

pub const BOLT_STANDARDS: [BoltStandard; 3] = [
    BoltStandard {
        name: "ГОСТ 7798-70",
        title: "Болт нормальной точности (класс точности B)",
        diameters: &[12., 16., 20., 24., 27., 30., 36.],
        grades: &["5.6", "5.8", "8.8", "10.9"],
        default_grade: "5.8",
    },
    BoltStandard {
        name: "ГОСТ 7805-70",
        title: "Болт повышенной точности (класс точности A), отверстие по диаметру болта",
        diameters: &[12., 16., 20., 24., 27., 30., 36.],
        grades: &["5.6", "5.8", "8.8", "10.9"],
        default_grade: "8.8",
    },
    BoltStandard {
        name: "ГОСТ Р 52644-2006",
        title: "Высокопрочный болт для фрикционных и срезных соединений",
        diameters: &[16., 20., 22., 24., 27., 30., 36.],
        grades: &["10.9"],
        default_grade: "10.9",
    },
];

/// Default bolt standard of new connections.
pub const DEFAULT_BOLT_STANDARD: &str = "ГОСТ 7798-70";

pub fn bolt_standard(name: &str) -> Option<&'static BoltStandard> {
    BOLT_STANDARDS.iter().find(|s| s.name == name.trim())
}

/// Hole clearance (hole minus bolt diameter), mm: ГОСТ 11284-75 row 2 for
/// bolts of normal accuracy, +0,3 for accurate bolts, +3 for high-strength
/// bolts.
pub fn hole_clearance(standard: &str, diameter: f64) -> f64 {
    match bolt_standard(standard).map(|s| s.name) {
        Some("ГОСТ 7805-70") => 0.3,
        Some("ГОСТ Р 52644-2006") => 3.,
        _ if diameter >= 27. => 3.,
        _ => 2.,
    }
}

/// Smallest fillet weld leg by СП 16.13330.2017 (manual and semi-automatic
/// welding, steel up to С285), by the thickness of the thicker part, mm.
pub fn min_weld(thickness: f64) -> f64 {
    match thickness {
        t if t <= 5. => 4.,
        t if t <= 10. => 5.,
        t if t <= 16. => 6.,
        t if t <= 22. => 7.,
        t if t <= 32. => 8.,
        t if t <= 40. => 9.,
        _ => 10.,
    }
}

/// One rule the template breaks: the value (template field key), what the
/// rule says, and the value that meets it when there is one.
#[derive(Clone, Debug, PartialEq)]
pub struct Advice {
    pub key: &'static str,
    pub text: String,
    pub fix: Option<f64>,
}

/// Whether the node has bolts (stiffeners and haunches are welded only).
pub fn bolted(kind: &TemplateKind) -> bool {
    !matches!(
        kind,
        TemplateKind::ColumnStiffeners { .. } | TemplateKind::Haunch { .. }
    )
}

/// Whether the node has a weld (the clip angle is bolted only).
pub fn welded(kind: &TemplateKind) -> bool {
    !matches!(kind, TemplateKind::ClipAngle { .. })
}

/// Bolt pitch of the node, when it has more than one bolt in a row.
fn pitch(kind: &TemplateKind) -> Option<f64> {
    match *kind {
        TemplateKind::FinPlate { pitch, rows, .. } if rows > 1 => Some(pitch),
        TemplateKind::Gusset { pitch, bolts } if bolts > 1 => Some(pitch),
        TemplateKind::BeamSplice { pitch, .. } | TemplateKind::ClipAngle { pitch, .. } => {
            Some(pitch)
        }
        _ => None,
    }
}

/// Millimetres the Russian way: «22» or «12,5».
pub fn mm(v: f64) -> String {
    let rounded = (v * 10.).round() / 10.;
    if rounded.fract() == 0. {
        format!("{rounded:.0}")
    } else {
        format!("{rounded:.1}").replace('.', ",")
    }
}

fn up5(x: f64) -> f64 {
    (x / 5.).ceil() * 5.
}

fn down5(x: f64) -> f64 {
    (x / 5.).floor() * 5.
}

const SP: &str = "СП 16.13330.2017";

/// The rules of ГОСТ and СП the template breaks. Expects a valid template.
pub fn advice(t: &ConnectionTemplate) -> Vec<Advice> {
    let mut out = Vec::new();
    let kind = &t.kind;
    if bolted(kind) {
        let d = t.bolt_diameter;
        let hole = d + t.clearance;
        let th = t.thickness;
        match bolt_standard(&t.bolt_standard) {
            None => out.push(Advice {
                key: "bolt_standard",
                text: format!(
                    "Стандарт «{}» не российский: выберите ГОСТ 7798-70, ГОСТ 7805-70 или ГОСТ Р 52644-2006",
                    t.bolt_standard.trim()
                ),
                fix: None,
            }),
            Some(s) => {
                if !s.grades.contains(&t.bolt_grade.trim()) {
                    out.push(Advice {
                        key: "bolt_grade",
                        text: format!(
                            "Для {} класс прочности {}",
                            s.name,
                            if s.grades.len() == 1 {
                                format!("только {}", s.grades[0])
                            } else {
                                format!("один из: {}", s.grades.join(", "))
                            }
                        ),
                        fix: None,
                    });
                }
                if !s.diameters.contains(&d) {
                    let list: Vec<String> = s.diameters.iter().map(|d| format!("М{d}")).collect();
                    let near = s
                        .diameters
                        .iter()
                        .copied()
                        .min_by(|a, b| (a - d).abs().total_cmp(&(b - d).abs()));
                    out.push(Advice {
                        key: "bolt_diameter",
                        text: format!("По {} болты {}", s.name, list.join(", ")),
                        fix: near,
                    });
                }
                let usual = hole_clearance(s.name, d);
                if (t.clearance - usual).abs() > 0.05 {
                    out.push(Advice {
                        key: "clearance",
                        text: format!(
                            "Для {} М{} обычно отверстие Ø{} мм (зазор {} мм)",
                            s.name,
                            mm(d),
                            mm(d + usual),
                            mm(usual)
                        ),
                        fix: Some(usual),
                    });
                }
            }
        }
        if t.edge_distance < 1.5 * hole - 0.05 {
            out.push(Advice {
                key: "edge_distance",
                text: format!(
                    "По {SP} от центра болта до края не меньше 1,5d = {} мм поперёк усилия и 2d = {} мм вдоль усилия (d — отверстие Ø{} мм)",
                    mm(1.5 * hole),
                    mm(2. * hole),
                    mm(hole)
                ),
                fix: Some(up5(2. * hole)),
            });
        } else if t.edge_distance < 2. * hole - 0.05 {
            out.push(Advice {
                key: "edge_distance",
                text: format!(
                    "Хватает только поперёк усилия: вдоль усилия по {SP} нужно 2d = {} мм",
                    mm(2. * hole)
                ),
                fix: Some(up5(2. * hole)),
            });
        } else if t.edge_distance > 4. * hole + 0.05 {
            out.push(Advice {
                key: "edge_distance",
                text: format!(
                    "По {SP} от центра болта до края не больше 4d = {} мм",
                    mm(4. * hole)
                ),
                fix: Some(down5(4. * hole).max(up5(2. * hole))),
            });
        } else if t.edge_distance > 8. * th + 0.05 {
            out.push(Advice {
                key: "thickness",
                text: format!(
                    "По {SP} до края не больше 8t: для отступа {} мм толщина не меньше {} мм",
                    mm(t.edge_distance),
                    mm((t.edge_distance / 8.).ceil())
                ),
                fix: Some((t.edge_distance / 8.).ceil()),
            });
        }
        if let Some(p) = pitch(kind) {
            if p < 2.5 * hole - 0.05 {
                out.push(Advice {
                    key: "pitch",
                    text: format!(
                        "По {SP} между центрами болтов не меньше 2,5d = {} мм",
                        mm(2.5 * hole)
                    ),
                    fix: Some(up5(2.5 * hole)),
                });
            } else if p > 8. * hole + 0.05 {
                out.push(Advice {
                    key: "pitch",
                    text: format!(
                        "По {SP} в крайних рядах шаг не больше 8d = {} мм",
                        mm(8. * hole)
                    ),
                    fix: Some(down5(8. * hole).max(up5(2.5 * hole))),
                });
            } else if p > 12. * th + 0.05 {
                out.push(Advice {
                    key: "thickness",
                    text: format!(
                        "По {SP} в крайних рядах шаг не больше 12t: для шага {} мм толщина не меньше {} мм",
                        mm(p),
                        mm((p / 12.).ceil())
                    ),
                    fix: Some((p / 12.).ceil()),
                });
            }
        }
    }
    if welded(kind) {
        let k = t.weld_size;
        let min = min_weld(t.thickness);
        if 1.2 * t.thickness < min - 0.05 {
            out.push(Advice {
                key: "thickness",
                text: format!(
                    "Деталь тоньше 4 мм не приварить угловым швом по {SP}: катет не меньше 4 мм и не больше 1,2t"
                ),
                fix: Some(4.),
            });
        } else if k < min - 0.05 {
            out.push(Advice {
                key: "weld_size",
                text: format!(
                    "По {SP} при толщине {} мм катет шва не меньше {} мм",
                    mm(t.thickness),
                    mm(min)
                ),
                fix: Some(min),
            });
        } else if k > 1.2 * t.thickness + 0.05 {
            out.push(Advice {
                key: "weld_size",
                text: format!(
                    "По {SP} катет не больше 1,2t = {} мм",
                    mm(1.2 * t.thickness)
                ),
                fix: Some((1.2 * t.thickness).floor().max(min)),
            });
        }
    }
    out
}

/// Bring the template to the rules: apply every advised value, set a
/// Russian bolt standard and a strength class it allows.
pub fn apply_advice(t: &mut ConnectionTemplate, set: impl Fn(&mut ConnectionTemplate, &str, f64)) {
    for _ in 0..5 {
        let list = advice(t);
        if list.is_empty() {
            return;
        }
        for a in list {
            match (a.key, a.fix) {
                ("bolt_standard", _) => {
                    t.bolt_standard = DEFAULT_BOLT_STANDARD.into();
                }
                ("bolt_grade", _) => {
                    if let Some(s) = bolt_standard(&t.bolt_standard) {
                        t.bolt_grade = s.default_grade.into();
                    }
                }
                (key, Some(v)) => set(t, key, v),
                _ => {}
            }
        }
    }
}
