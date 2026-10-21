//! Component editor: make your own component from a catalogue one without
//! writing code. Every value has a plain-language hint, wrong values are
//! explained next to the field with a one-click fix, and a preview builds the
//! node on a sample column and beam so a mistake shows before it reaches the
//! model. Saved components live as JSON in the `components` folder next to the
//! project file and appear in the catalogue under «Мои компоненты».
use super::Workbench;
use super::components::{self, Component};
use crate::tools::connection_ui::ConnectionUi;
use eframe::egui;
use open_bim_drawings::projection::{PartMesh, V3};
use open_bim_model::{
    ConnectionInstance, ConnectionTemplate, Element, ElementShape, Frame, MemberKind, ObjectId,
    Point3, Profile, ProfileShape, Project, TemplateKind,
};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};

pub const GROUP: &str = "Мои компоненты";
const FOLDER: &str = "components";
const EXTENSION: &str = "component.json";

/// Part of the form a value belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Size,
    Bolts,
    Weld,
}

impl Section {
    pub fn title(self) -> &'static str {
        match self {
            Self::Size => "Размеры",
            Self::Bolts => "Болты",
            Self::Weld => "Сварка",
        }
    }
}

/// One numeric value of the form.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Field {
    pub key: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    /// «мм» or «шт».
    pub unit: &'static str,
    pub section: Section,
}

const fn field(
    key: &'static str,
    label: &'static str,
    unit: &'static str,
    section: Section,
    hint: &'static str,
) -> Field {
    Field {
        key,
        label,
        hint,
        unit,
        section,
    }
}

const PITCH: Field = field(
    "pitch",
    "Шаг болтов",
    "мм",
    Section::Bolts,
    "Расстояние между центрами соседних болтов. Обычно 2,5–3 диаметра болта.",
);

pub use open_bim_connections::norms::{bolted, welded};

/// Numeric values of a node of this kind, in form order.
pub fn fields(kind: &TemplateKind) -> Vec<Field> {
    use Section::*;
    let mut list = match kind {
        TemplateKind::BasePlate { .. } => vec![field(
            "overhang",
            "Свес плиты",
            "мм",
            Size,
            "Насколько плита выступает за контур колонны с каждой стороны. В свесе стоят анкерные болты.",
        )],
        TemplateKind::EndPlate { .. } => vec![field(
            "overhang",
            "Свес пластины",
            "мм",
            Size,
            "Насколько пластина выступает за контур балки. В свесе стоят болты к опоре.",
        )],
        TemplateKind::ClipAngle { .. } => vec![
            field(
                "leg",
                "Полка уголка",
                "мм",
                Size,
                "Ширина полки уголка. В ней должны поместиться болт с отступами и толщина уголка.",
            ),
            field(
                "height",
                "Длина уголка",
                "мм",
                Size,
                "Длина уголка вдоль стенки балки. Не больше высоты стенки между полками.",
            ),
            PITCH,
        ],
        TemplateKind::FinPlate { .. } => vec![
            field(
                "height",
                "Высота пластины",
                "мм",
                Size,
                "Высота пластины по стенке балки. Должна вместить все болты с отступами сверху и снизу.",
            ),
            field(
                "lap",
                "Нахлёст на балку",
                "мм",
                Size,
                "Насколько пластина заходит на стенку балки от её торца.",
            ),
            field(
                "gap",
                "Зазор до опоры",
                "мм",
                Size,
                "Зазор между торцом балки и опорой, чтобы балку можно было завести при монтаже. Обычно 10–20 мм.",
            ),
            field(
                "rows",
                "Болтов в ряду",
                "шт",
                Bolts,
                "Сколько болтов стоит в вертикальном ряду, от 1 до 12.",
            ),
            PITCH,
        ],
        TemplateKind::ColumnStiffeners { .. } => vec![field(
            "chamfer",
            "Срез угла у стенки",
            "мм",
            Size,
            "Срез внутреннего угла ребра, чтобы ребро не упиралось в скругление колонны. 0 — без среза.",
        )],
        TemplateKind::BeamSplice { .. } => vec![
            field(
                "rows",
                "Болтов по высоте стенки",
                "шт",
                Bolts,
                "Сколько болтов по высоте стенки в каждом ряду накладки, от 1 до 12.",
            ),
            PITCH,
        ],
        TemplateKind::Haunch { .. } => vec![
            field(
                "length",
                "Длина вуты",
                "мм",
                Size,
                "Длина вуты вдоль балки от грани опоры.",
            ),
            field(
                "depth",
                "Высота вуты",
                "мм",
                Size,
                "На сколько вута опускается ниже нижней полки балки у опоры.",
            ),
        ],
        TemplateKind::Gusset { .. } => vec![
            field(
                "bolts",
                "Болтов в ряду",
                "шт",
                Bolts,
                "Сколько болтов крепят связь к фасонке, от 1 до 12.",
            ),
            PITCH,
        ],
    };
    list.push(field(
        "thickness",
        match kind {
            TemplateKind::ClipAngle { .. } => "Толщина уголка",
            TemplateKind::ColumnStiffeners { .. } => "Толщина рёбер",
            _ => "Толщина пластины",
        },
        "мм",
        Size,
        "Толщина листа деталей узла. Обычно 8–20 мм.",
    ));
    if bolted(kind) {
        list.extend([
            field(
                "bolt_diameter",
                "Диаметр болта",
                "мм",
                Bolts,
                "Номинальный диаметр болта: 16 — это М16, 20 — М20. Удобно выбрать кнопкой ниже.",
            ),
            field(
                "clearance",
                "Зазор отверстия",
                "мм",
                Bolts,
                "На сколько отверстие больше болта. Обычно 2 мм для М16–М20 и 3 мм для М24 и больше.",
            ),
            field(
                "edge_distance",
                "Отступ от края",
                "мм",
                Bolts,
                "От центра болта до края детали. Обычно 1,5–2 диаметра отверстия.",
            ),
        ]);
    }
    if welded(kind) {
        list.push(field(
            "weld_size",
            "Катет шва",
            "мм",
            Weld,
            "Размер углового шва. Обычно 5–8 мм и не больше толщины тонкой детали.",
        ));
    }
    list
}

/// A value of the template by field key.
pub fn get(t: &ConnectionTemplate, key: &str) -> Option<f64> {
    let common = match key {
        "thickness" => Some(t.thickness),
        "edge_distance" => Some(t.edge_distance),
        "bolt_diameter" => Some(t.bolt_diameter),
        "clearance" => Some(t.clearance),
        "weld_size" => Some(t.weld_size),
        _ => None,
    };
    if common.is_some() {
        return common;
    }
    Some(match (&t.kind, key) {
        (
            TemplateKind::BasePlate { overhang } | TemplateKind::EndPlate { overhang },
            "overhang",
        ) => *overhang,
        (TemplateKind::ClipAngle { leg, .. }, "leg") => *leg,
        (TemplateKind::ClipAngle { height, .. }, "height") => *height,
        (TemplateKind::ClipAngle { pitch, .. }, "pitch") => *pitch,
        (TemplateKind::FinPlate { height, .. }, "height") => *height,
        (TemplateKind::FinPlate { lap, .. }, "lap") => *lap,
        (TemplateKind::FinPlate { gap, .. }, "gap") => *gap,
        (TemplateKind::FinPlate { pitch, .. }, "pitch") => *pitch,
        (TemplateKind::FinPlate { rows, .. }, "rows") => *rows as f64,
        (TemplateKind::ColumnStiffeners { chamfer }, "chamfer") => *chamfer,
        (TemplateKind::BeamSplice { rows, .. }, "rows") => *rows as f64,
        (TemplateKind::BeamSplice { pitch, .. }, "pitch") => *pitch,
        (TemplateKind::Haunch { length, .. }, "length") => *length,
        (TemplateKind::Haunch { depth, .. }, "depth") => *depth,
        (TemplateKind::Gusset { bolts, .. }, "bolts") => *bolts as f64,
        (TemplateKind::Gusset { pitch, .. }, "pitch") => *pitch,
        _ => return None,
    })
}

/// Set a value by field key; counts are rounded. False for an unknown key.
pub fn set(t: &mut ConnectionTemplate, key: &str, value: f64) -> bool {
    let count = if value.is_finite() {
        value.round().clamp(0., u32::MAX as f64) as u32
    } else {
        0
    };
    let slot = match key {
        "thickness" => &mut t.thickness,
        "edge_distance" => &mut t.edge_distance,
        "bolt_diameter" => &mut t.bolt_diameter,
        "clearance" => &mut t.clearance,
        "weld_size" => &mut t.weld_size,
        _ => match (&mut t.kind, key) {
            (
                TemplateKind::BasePlate { overhang } | TemplateKind::EndPlate { overhang },
                "overhang",
            ) => overhang,
            (TemplateKind::ClipAngle { leg, .. }, "leg") => leg,
            (TemplateKind::ClipAngle { height, .. }, "height") => height,
            (TemplateKind::ClipAngle { pitch, .. }, "pitch") => pitch,
            (TemplateKind::FinPlate { height, .. }, "height") => height,
            (TemplateKind::FinPlate { lap, .. }, "lap") => lap,
            (TemplateKind::FinPlate { gap, .. }, "gap") => gap,
            (TemplateKind::FinPlate { pitch, .. }, "pitch") => pitch,
            (TemplateKind::FinPlate { rows, .. }, "rows")
            | (TemplateKind::BeamSplice { rows, .. }, "rows") => {
                *rows = count;
                return true;
            }
            (TemplateKind::Gusset { bolts, .. }, "bolts") => {
                *bolts = count;
                return true;
            }
            (TemplateKind::ColumnStiffeners { chamfer }, "chamfer") => chamfer,
            (TemplateKind::BeamSplice { pitch, .. }, "pitch") => pitch,
            (TemplateKind::Haunch { length, .. }, "length") => length,
            (TemplateKind::Haunch { depth, .. }, "depth") => depth,
            (TemplateKind::Gusset { pitch, .. }, "pitch") => pitch,
            _ => return false,
        },
    };
    *slot = value;
    true
}

/// A problem with one value, said the way a student understands it, and the
/// value that fixes it when there is one.
#[derive(Clone, Debug, PartialEq)]
pub struct Issue {
    /// Field key, or «name», «bolt_standard», «bolt_grade», «sample».
    pub key: &'static str,
    pub text: String,
    pub fix: Option<f64>,
}

pub use open_bim_connections::norms::mm;

/// Smallest multiple of 5 strictly above `x`.
fn above(x: f64) -> f64 {
    ((x / 5.).floor() + 1.) * 5.
}

/// Smallest multiple of 5 not below `x`.
fn at_least(x: f64) -> f64 {
    (x / 5.).ceil() * 5.
}

fn positive(v: f64) -> bool {
    v.is_finite() && v > 0.001 && v <= 10000.0
}

/// The catalogue component a node of this kind is based on.
pub fn base(kind: &TemplateKind) -> Component {
    components::catalog()
        .into_iter()
        .find(|c| std::mem::discriminant(&c.kind) == std::mem::discriminant(kind))
        .expect("every kind is in the catalogue")
}

/// Full parameters of a catalogue component: its own values plus the usual
/// bolts and welds.
pub fn base_template(c: &Component) -> ConnectionTemplate {
    if let Some(t) = &c.template {
        return t.clone();
    }
    let mut t = ConnectionUi::default().template;
    t.kind = c.kind.clone();
    t.thickness = c.thickness;
    t.name = c.name.clone();
    t.weld_size = t.weld_size.max(min_weld(t.thickness));
    t
}

/// Everything wrong with the template. Empty exactly when the template is
/// valid (`ConnectionTemplate::validate` passes).
pub fn check(t: &ConnectionTemplate) -> Vec<Issue> {
    let mut issues = Vec::new();
    let defaults = base_template(&base(&t.kind));
    let mut push = |key: &'static str, text: &str| {
        issues.push(Issue {
            key,
            text: text.into(),
            fix: None,
        })
    };
    if t.name.trim().is_empty() {
        push("name", "Введите имя компонента");
    } else if t.name.len() > 1024 {
        push("name", "Имя слишком длинное");
    }
    if t.bolt_standard.trim().is_empty() || t.bolt_standard.len() > 256 {
        push(
            "bolt_standard",
            "Укажите стандарт болта, например ГОСТ 7798-70",
        );
    }
    if t.bolt_grade.trim().is_empty() || t.bolt_grade.len() > 256 {
        push("bolt_grade", "Укажите класс прочности болта, например 8.8");
    }
    let range_text = "Введите число больше 0 и не больше 10 000";
    let must_be_positive = |key: &'static str, issues: &mut Vec<Issue>| {
        let v = get(t, key).unwrap_or(f64::NAN);
        let ok = positive(v);
        if !ok {
            issues.push(Issue {
                key,
                text: range_text.into(),
                fix: get(&defaults, key),
            });
        }
        ok
    };
    let mut base_ok = true;
    for key in ["thickness", "edge_distance", "bolt_diameter", "weld_size"] {
        base_ok &= must_be_positive(key, &mut issues);
    }
    let clearance_ok = t.clearance.is_finite() && (0.0..=100.0).contains(&t.clearance);
    if !clearance_ok {
        issues.push(Issue {
            key: "clearance",
            text: "Зазор отверстия — от 0 до 100 мм".into(),
            fix: Some(defaults.clearance),
        });
    }
    base_ok &= clearance_ok;
    let (e, d, th) = (t.edge_distance, t.bolt_diameter, t.thickness);
    let hole = d + t.clearance;
    if base_ok && e <= hole / 2. + 0.001 {
        issues.push(Issue {
            key: "edge_distance",
            text: format!(
                "Отверстие Ø{} мм не помещается: от центра до края нужно больше {} мм. Обычно берут {} мм.",
                mm(hole),
                mm(hole / 2.),
                mm(at_least(1.5 * hole)),
            ),
            fix: Some(above(hole / 2.).max(at_least(1.5 * hole))),
        });
    }
    let count_issue = |key: &'static str, n: u32, issues: &mut Vec<Issue>| {
        let ok = (1..=12).contains(&n);
        if !ok {
            issues.push(Issue {
                key,
                text: "Количество болтов — от 1 до 12".into(),
                fix: Some(f64::from(n.clamp(1, 12))),
            });
        }
        ok
    };
    let pitch_issue = |issues: &mut Vec<Issue>| {
        issues.push(Issue {
            key: "pitch",
            text: format!(
                "Шаг должен быть больше отверстия Ø{} мм, иначе отверстия сольются. Обычно {} мм.",
                mm(hole),
                mm(at_least(2.5 * d)),
            ),
            fix: Some(above(hole).max(at_least(2.5 * d))),
        })
    };
    match t.kind {
        TemplateKind::BasePlate { overhang } | TemplateKind::EndPlate { overhang } => {
            if must_be_positive("overhang", &mut issues) && base_ok && overhang <= e + d / 2. {
                issues.push(Issue {
                    key: "overhang",
                    text: format!(
                        "Свес мал для болта: нужно больше {} мм (отступ от края {} + половина болта {}).",
                        mm(e + d / 2.),
                        mm(e),
                        mm(d / 2.)
                    ),
                    fix: Some(above(e + d / 2.)),
                });
            }
        }
        TemplateKind::FinPlate {
            height,
            lap,
            gap,
            pitch,
            rows,
        } => {
            let height_ok = must_be_positive("height", &mut issues);
            let lap_ok = must_be_positive("lap", &mut issues);
            let pitch_ok = must_be_positive("pitch", &mut issues);
            if !(gap.is_finite() && (0.0..=100.0).contains(&gap)) {
                issues.push(Issue {
                    key: "gap",
                    text: "Зазор до опоры — от 0 до 100 мм".into(),
                    fix: get(&defaults, "gap"),
                });
            }
            let rows_ok = count_issue("rows", rows, &mut issues);
            if base_ok && pitch_ok && rows > 1 && pitch <= hole {
                pitch_issue(&mut issues);
            }
            if base_ok && height_ok && pitch_ok && rows_ok {
                let need = 2. * e + pitch * f64::from(rows - 1);
                if height < need {
                    issues.push(Issue {
                        key: "height",
                        text: format!(
                            "Пластина низкая для {rows} болт.: нужно не меньше {} мм (два отступа от края и шаги между болтами).",
                            mm(need)
                        ),
                        fix: Some(at_least(need)),
                    });
                }
            }
            if base_ok && lap_ok && lap < 2. * e {
                issues.push(Issue {
                    key: "lap",
                    text: format!(
                        "Нахлёст мал: нужно не меньше двух отступов от края, {} мм.",
                        mm(2. * e)
                    ),
                    fix: Some(at_least(2. * e)),
                });
            }
        }
        TemplateKind::Gusset { bolts, pitch } => {
            let pitch_ok = must_be_positive("pitch", &mut issues);
            count_issue("bolts", bolts, &mut issues);
            if base_ok && pitch_ok && bolts > 1 && pitch <= hole {
                pitch_issue(&mut issues);
            }
        }
        TemplateKind::Haunch { .. } => {
            must_be_positive("length", &mut issues);
            must_be_positive("depth", &mut issues);
        }
        TemplateKind::BeamSplice { rows, pitch } => {
            let pitch_ok = must_be_positive("pitch", &mut issues);
            count_issue("rows", rows, &mut issues);
            if base_ok && pitch_ok && pitch <= hole {
                pitch_issue(&mut issues);
            }
        }
        TemplateKind::ColumnStiffeners { chamfer } => {
            if !(chamfer.is_finite() && (0.0..=100.0).contains(&chamfer)) {
                issues.push(Issue {
                    key: "chamfer",
                    text: "Срез угла — от 0 до 100 мм".into(),
                    fix: get(&defaults, "chamfer"),
                });
            }
        }
        TemplateKind::ClipAngle { leg, height, pitch } => {
            let leg_ok = must_be_positive("leg", &mut issues);
            let height_ok = must_be_positive("height", &mut issues);
            let pitch_ok = must_be_positive("pitch", &mut issues);
            if base_ok && leg_ok && leg <= 2. * e + th {
                issues.push(Issue {
                    key: "leg",
                    text: format!(
                        "Полка мала: нужно больше {} мм (два отступа от края и толщина уголка).",
                        mm(2. * e + th)
                    ),
                    fix: Some(above(2. * e + th)),
                });
            }
            if base_ok && height_ok && pitch_ok && height < 2. * e + pitch {
                issues.push(Issue {
                    key: "height",
                    text: format!(
                        "Уголок короткий для двух болтов: нужно не меньше {} мм.",
                        mm(2. * e + pitch)
                    ),
                    fix: Some(at_least(2. * e + pitch)),
                });
            }
            if base_ok && pitch_ok && pitch <= hole {
                pitch_issue(&mut issues);
            }
        }
    }
    issues
}

/// Apply every suggested fix until nothing is left to fix (or 20 rounds).
pub fn fix_all(t: &mut ConnectionTemplate) {
    let defaults = base_template(&base(&t.kind));
    for _ in 0..20 {
        let issues = check(t);
        if issues.is_empty() {
            return;
        }
        for issue in issues {
            match (issue.key, issue.fix) {
                ("name", _) => t.name = defaults.name.clone(),
                ("bolt_standard", _) => t.bolt_standard = defaults.bolt_standard.clone(),
                ("bolt_grade", _) => t.bolt_grade = defaults.bolt_grade.clone(),
                (key, Some(v)) => {
                    set(t, key, v);
                }
                _ => {}
            }
        }
    }
}

pub use open_bim_connections::norms::{
    Advice, BOLT_STANDARDS, BoltStandard, advice, bolt_standard, hole_clearance, min_weld,
};

/// Pick a bolt diameter: the hole follows the bolt standard, and the edge
/// distance grows to 2d when the new hole needs more.
pub fn set_bolt(t: &mut ConnectionTemplate, diameter: f64) {
    t.bolt_diameter = diameter;
    t.clearance = hole_clearance(&t.bolt_standard, diameter);
    let hole = diameter + t.clearance;
    if t.edge_distance < 2. * hole {
        t.edge_distance = at_least(2. * hole);
    }
}

/// Pick a bolt standard: strength class, diameter and hole follow it.
pub fn set_standard(t: &mut ConnectionTemplate, s: &BoltStandard) {
    t.bolt_standard = s.name.into();
    if !s.grades.contains(&t.bolt_grade.as_str()) {
        t.bolt_grade = s.default_grade.into();
    }
    let d = s
        .diameters
        .iter()
        .copied()
        .min_by(|a, b| {
            (a - t.bolt_diameter)
                .abs()
                .total_cmp(&(b - t.bolt_diameter).abs())
        })
        .unwrap_or(t.bolt_diameter);
    set_bolt(t, d);
}

/// Bring a template to ГОСТ and СП 16.13330 while keeping it valid.
pub fn bring_to_norms(t: &mut ConnectionTemplate) {
    for _ in 0..5 {
        fix_all(t);
        if advice(t).is_empty() {
            return;
        }
        open_bim_connections::norms::apply_advice(t, |t, key, v| {
            set(t, key, v);
        });
    }
    fix_all(t);
}

/// A saved user component.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserComponent {
    pub version: u32,
    #[serde(default)]
    pub description: String,
    /// All parameters; `template.name` is the component's name.
    pub template: ConnectionTemplate,
}

impl UserComponent {
    pub fn name(&self) -> &str {
        &self.template.name
    }

    /// The catalogue entry: same picks as the component it is based on.
    pub fn component(&self) -> Component {
        let base = base(&self.template.kind);
        Component {
            name: self.template.name.clone(),
            group: GROUP.into(),
            description: if self.description.trim().is_empty() {
                format!("На основе «{}». {}", base.name, picks(&base))
            } else {
                self.description.clone()
            },
            needs_support: base.needs_support,
            kind: self.template.kind.clone(),
            thickness: self.template.thickness,
            template: Some(self.template.clone()),
        }
    }
}

/// What the user clicks to place a component of this base.
pub fn picks(base: &Component) -> String {
    base.description
        .split(". ")
        .find(|s| s.starts_with("Выберите"))
        .map(|s| s.trim_end_matches('.').to_string() + ".")
        .unwrap_or_default()
}

/// `components` folder next to the project file.
pub fn folder(project_path: &str) -> PathBuf {
    match Path::new(project_path).parent() {
        Some(p) if !p.as_os_str().is_empty() => p.join(FOLDER),
        _ => PathBuf::from(FOLDER),
    }
}

fn path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}.{EXTENSION}"))
}

/// Saved components sorted by name, and a message per file that could not
/// be read.
pub fn load_all(dir: &Path) -> (Vec<UserComponent>, Vec<String>) {
    let suffix = format!(".{EXTENSION}");
    let mut found = Vec::new();
    let mut errors = Vec::new();
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(&suffix))
        })
        .collect();
    files.sort();
    for file in files {
        match read(&file) {
            Ok(c) => found.push(c),
            Err(e) => errors.push(format!(
                "{}: {e}",
                file.file_name().unwrap_or_default().to_string_lossy()
            )),
        }
    }
    found.sort_by(|a, b| a.name().cmp(b.name()));
    (found, errors)
}

fn read(file: &Path) -> Result<UserComponent, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(file)
        .map_err(|e| e.to_string())?
        .take(65537)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 65536 {
        return Err("файл слишком большой".into());
    }
    let c: UserComponent = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if c.version != 1 {
        return Err(format!("неизвестная версия {}", c.version));
    }
    c.template.validate().map_err(|e| e.to_string())?;
    Ok(c)
}

/// Save a component; `old` is the name it had, removed after a rename.
pub fn save(dir: &Path, component: &UserComponent, old: Option<&str>) -> Result<(), String> {
    let name = crate::tools::property_files::check_name(component.name())
        .map_err(|e| e.replace("файла свойств", "компонента"))?;
    if let Some(issue) = check(&component.template).first() {
        return Err(issue.text.clone());
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(component).map_err(|e| e.to_string())?;
    std::fs::write(path(dir, name), json).map_err(|e| e.to_string())?;
    if let Some(old) = old.filter(|o| *o != name) {
        let _ = std::fs::remove_file(path(dir, old));
    }
    Ok(())
}

pub fn delete(dir: &Path, name: &str) -> Result<(), String> {
    std::fs::remove_file(path(dir, name)).map_err(|e| e.to_string())
}

/// Free name «base», «base 2», «base 3»… among `taken`.
pub fn free_name(base: &str, taken: &[String]) -> String {
    let base = base.trim();
    if !taken.iter().any(|t| t == base) {
        return base.into();
    }
    (2..)
        .map(|i| format!("{base} {i}"))
        .find(|n| !taken.contains(n))
        .expect("some number is free")
}

/// Which way the preview looks at the sample.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum View {
    #[default]
    Iso,
    Side,
    Top,
}

impl View {
    pub const ALL: [View; 3] = [View::Iso, View::Side, View::Top];
    pub fn label(self) -> &'static str {
        match self {
            Self::Iso => "Изометрия",
            Self::Side => "Сбоку",
            Self::Top => "Сверху",
        }
    }
    /// (right, up, toward the viewer).
    fn axes(self) -> (V3, V3, V3) {
        match self {
            Self::Side => ([0., 1., 0.], [0., 0., 1.], [1., 0., 0.]),
            Self::Top => ([0., 1., 0.], [-1., 0., 0.], [0., 0., 1.]),
            Self::Iso => {
                let toward = normalize([1.0, -0.9, 0.8]);
                let right = normalize(cross([0., 0., 1.], toward));
                (right, cross(toward, right), toward)
            }
        }
    }
}

fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize(a: V3) -> V3 {
    let l = dot(a, a).sqrt();
    a.map(|v| v / l)
}

pub type Segment = [[f64; 2]; 2];

/// The node built on a sample: lines of the joined parts (cut to the area
/// around the node) and of the node's own parts, in view millimetres.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Preview {
    pub parents: Vec<Segment>,
    pub parts: Vec<Segment>,
    /// (min, max) of the area shown.
    pub window: ([f64; 2], [f64; 2]),
    /// Why the node does not build on the sample.
    pub error: Option<String>,
    /// Number of node parts (plates, angles) and bolt groups built.
    pub plates: usize,
    pub bolts: usize,
}

fn member(
    document: &mut Project,
    kind: MemberKind,
    profile: ObjectId,
    start: [f64; 3],
    end: [f64; 3],
) -> Result<ObjectId, String> {
    let id = document.allocate_id().map_err(|e| e.to_string())?;
    let (frame, length) = Frame::along(Point3(start), Point3(end)).map_err(|e| e.to_string())?;
    let element = Element {
        id,
        name: match kind {
            MemberKind::Column => "Колонна",
            MemberKind::Beam => "Балка",
        }
        .into(),
        material: ObjectId(1),
        shape: ElementShape::Member {
            member_kind: kind,
            profile,
            length,
            frame,
            position: Default::default(),
        },
        properties: Default::default(),
    };
    document.elements.insert(id, element);
    Ok(id)
}

/// Sample parts for a node of this kind: (main, secondary). A column of the
/// default section with a beam (or a second beam, or an angle brace)
/// arriving at it, the way the catalogue description asks to pick them. The
/// end plate and the clip angle bear on the flange of the `wide` column,
/// since their bolts to the support sit outside the beam section.
fn sample_parts(
    document: &mut Project,
    t: &ConnectionTemplate,
    wide: ObjectId,
) -> Result<(ObjectId, Option<ObjectId>), String> {
    let (i_beam, angle) = (ObjectId(2), ObjectId(3));
    let column =
        |d: &mut Project| member(d, MemberKind::Column, i_beam, [0., 0., 0.], [0., 0., 3600.]);
    let beam_from = |d: &mut Project, y: f64| {
        member(
            d,
            MemberKind::Beam,
            i_beam,
            [0., y, 3000.],
            [0., 3000., 3000.],
        )
    };
    Ok(match t.kind {
        TemplateKind::BasePlate { .. } => (column(document)?, None),
        TemplateKind::ColumnStiffeners { .. } => {
            let c = column(document)?;
            (c, Some(beam_from(document, 150.)?))
        }
        TemplateKind::FinPlate { gap, .. } => {
            let c = column(document)?;
            (beam_from(document, 150. + gap)?, Some(c))
        }
        TemplateKind::EndPlate { .. } | TemplateKind::ClipAngle { .. } => {
            let c = member(
                document,
                MemberKind::Column,
                wide,
                [0., 0., 0.],
                [0., 0., 3600.],
            )?;
            let start = match t.kind {
                TemplateKind::EndPlate { .. } => 300. + t.thickness,
                _ => 300.,
            };
            (beam_from(document, start)?, Some(c))
        }
        TemplateKind::Haunch { .. } => {
            let c = column(document)?;
            (beam_from(document, 150.)?, Some(c))
        }
        TemplateKind::BeamSplice { .. } => {
            let left = member(
                document,
                MemberKind::Beam,
                i_beam,
                [0., 0., 3000.],
                [0., 1500., 3000.],
            )?;
            let right = member(
                document,
                MemberKind::Beam,
                i_beam,
                [0., 1510., 3000.],
                [0., 3000., 3000.],
            )?;
            (left, Some(right))
        }
        TemplateKind::Gusset { .. } => {
            let c = column(document)?;
            let brace = member(
                document,
                MemberKind::Beam,
                angle,
                [0., 400., 300.],
                [0., 3000., 3000.],
            )?;
            (brace, Some(c))
        }
    })
}

/// Build the node on its sample in a scratch copy of `project`'s catalogue
/// and draw it from `view`.
pub fn preview(project: &Project, t: &ConnectionTemplate, view: View) -> Preview {
    let mut out = Preview::default();
    if let Some(issue) = check(t).first() {
        out.error = Some(issue.text.clone());
        return out;
    }
    let mut sample = Project {
        profiles: project.profiles.clone(),
        materials: project.materials.clone(),
        ..Default::default()
    };
    let wide = ObjectId(project.next_id.max(100));
    sample.profiles.insert(
        wide,
        Profile {
            id: wide,
            name: "I600 (пример)".into(),
            shape: ProfileShape::ISection {
                height: 600.,
                width: 320.,
                web: 12.,
                flange: 20.,
            },
            source: "Пример для предпросмотра".into(),
            standard: None,
            area_mm2: None,
        },
    );
    sample.next_id = wide.0 + 1;
    // The generators run directly, without the whole-model check a real
    // edit makes, so the preview keeps up with typing.
    let built = (|| {
        let (main, secondary) = sample_parts(&mut sample, t, wide)?;
        let connection = ConnectionInstance {
            id: sample.allocate_id().map_err(|e| e.to_string())?,
            primary: main,
            secondary,
            at_end: components::at_end(&sample, main, secondary),
            template: t.clone(),
            children: Default::default(),
        };
        sample.connections.insert(connection.id, connection);
        let before = Project::default();
        use open_bim_connections as oc;
        for rebuild in [
            oc::rebuild_base_plates,
            oc::rebuild_end_plates,
            oc::rebuild_clip_angles,
            oc::rebuild_fin_plates,
            oc::rebuild_stiffeners,
            oc::rebuild_beam_splices,
            oc::rebuild_haunches,
            oc::rebuild_gussets,
        ] {
            rebuild(&mut sample, &before).map_err(|e| e.to_string())?;
        }
        Ok::<_, String>((main, secondary))
    })();
    let (main, secondary) = match built {
        Ok(b) => b,
        Err(e) => {
            out.error = Some(format!("На примере узел не строится: {e}"));
            return out;
        }
    };
    let p = &sample;
    let Some(c) = p.connections.values().next() else {
        out.error = Some("На примере узел не строится".into());
        return out;
    };
    // Bodies without holes tessellate fast; holes are drawn as circles.
    let mut bare = p.clone();
    bare.details
        .retain(|_, d| !matches!(d.kind, open_bim_model::DetailKind::Hole { .. }));
    let (right, up, toward) = view.axes();
    let flat = |a: V3| [dot(a, right), dot(a, up)];
    let edges = |id: ObjectId| -> Vec<Segment> {
        let mut lines: Vec<Segment> = PartMesh::new(&bare, id)
            .map(|m| {
                m.feature_edges(toward)
                    .into_iter()
                    .map(|(a, b)| [flat(a), flat(b)])
                    .collect()
            })
            .unwrap_or_default();
        lines.extend(holes(p, id).into_iter().map(|[a, b]| [flat(a), flat(b)]));
        lines
    };
    for id in c.children.values() {
        if p.elements.contains_key(id) {
            out.plates += 1;
            out.parts.extend(edges(*id));
        } else if matches!(
            p.details.get(id).map(|d| &d.kind),
            Some(open_bim_model::DetailKind::BoltGroup { .. })
        ) {
            out.bolts += 1;
        }
    }
    let mut lo = [f64::INFINITY; 2];
    let mut hi = [f64::NEG_INFINITY; 2];
    for s in &out.parts {
        for q in s {
            for k in 0..2 {
                lo[k] = lo[k].min(q[k]);
                hi[k] = hi[k].max(q[k]);
            }
        }
    }
    if !lo[0].is_finite() {
        out.error = Some("Узел построен, но у него нет деталей для показа".into());
        return out;
    }
    let size = (hi[0] - lo[0]).max(hi[1] - lo[1]);
    let margin = (size * 0.6).max(250.);
    out.window = (
        [lo[0] - margin, lo[1] - margin],
        [hi[0] + margin, hi[1] + margin],
    );
    for id in std::iter::once(main).chain(secondary) {
        out.parents
            .extend(edges(id).into_iter().filter_map(|s| clip(s, out.window)));
    }
    out
}

/// Circles of the holes in a part, world coordinates, at the hole entry.
fn holes(p: &Project, part: ObjectId) -> Vec<[V3; 2]> {
    let Some(element) = p.elements.get(&part) else {
        return Vec::new();
    };
    let pf = element.shape.frame();
    let mut lines = Vec::new();
    for d in p.details.values() {
        let open_bim_model::DetailKind::Hole {
            part: of,
            frame,
            diameter,
            ..
        } = &d.kind
        else {
            continue;
        };
        if *of != part {
            continue;
        }
        let r = diameter / 2.;
        let point = |a: f64| {
            let local = frame.world(Point3([r * a.cos(), r * a.sin(), 0.]));
            pf.world(local).0
        };
        const N: usize = 16;
        for i in 0..N {
            let a = std::f64::consts::TAU * i as f64 / N as f64;
            let b = std::f64::consts::TAU * (i + 1) as f64 / N as f64;
            lines.push([point(a), point(b)]);
        }
    }
    lines
}

/// The part of a segment inside the box (Liang–Barsky).
pub fn clip(s: Segment, (lo, hi): ([f64; 2], [f64; 2])) -> Option<Segment> {
    let [a, b] = s;
    let d = [b[0] - a[0], b[1] - a[1]];
    let (mut t0, mut t1) = (0f64, 1f64);
    for k in 0..2 {
        for (p, q) in [(-d[k], a[k] - lo[k]), (d[k], hi[k] - a[k])] {
            if p.abs() < 1e-12 {
                if q < 0. {
                    return None;
                }
            } else {
                let r = q / p;
                if p < 0. {
                    t0 = t0.max(r);
                } else {
                    t1 = t1.min(r);
                }
            }
        }
    }
    (t0 <= t1).then(|| {
        let at = |t: f64| [a[0] + d[0] * t, a[1] + d[1] * t];
        [at(t0), at(t1)]
    })
}

/// A component being made or changed.
#[derive(Debug)]
pub struct Draft {
    pub template: ConnectionTemplate,
    pub description: String,
    /// Name of the saved file this draft changes; `None` for a new one.
    pub editing: Option<String>,
    pub view: View,
    cache: Option<(ConnectionTemplate, View, Preview)>,
    /// Preview being built in the background, at most one at a time.
    pending: Option<std::sync::mpsc::Receiver<(ConnectionTemplate, View, Preview)>>,
}

impl Draft {
    /// A new component based on a catalogue one (built-in or saved).
    pub fn based_on(c: &Component, taken: &[String]) -> Self {
        let mut template = base_template(c);
        template.name = free_name(&format!("{} (мой)", base(&c.kind).name), taken);
        Self {
            template,
            description: String::new(),
            editing: None,
            view: View::Iso,
            cache: None,
            pending: None,
        }
    }

    pub fn edit(c: &UserComponent) -> Self {
        Self {
            template: c.template.clone(),
            description: c.description.clone(),
            editing: Some(c.name().into()),
            view: View::Iso,
            cache: None,
            pending: None,
        }
    }

    /// Switch the base component; values return to that component's.
    pub fn rebase(&mut self, c: &Component) {
        let name = std::mem::take(&mut self.template.name);
        self.template = base_template(c);
        self.template.name = name;
    }

    /// Back to the base component's values, keeping the name.
    pub fn reset(&mut self) {
        let c = base(&self.template.kind);
        self.rebase(&c);
    }

    pub fn component(&self) -> UserComponent {
        UserComponent {
            version: 1,
            description: self.description.trim().into(),
            template: self.template.clone(),
        }
    }

    /// Rules of ГОСТ and СП 16.13330 the values break, once they are valid.
    pub fn advice(&self) -> Vec<Advice> {
        if check(&self.template).is_empty() {
            advice(&self.template)
        } else {
            Vec::new()
        }
    }

    /// Problems with the values and the name.
    pub fn issues(&self) -> Vec<Issue> {
        let mut issues = check(&self.template);
        if !issues.iter().any(|i| i.key == "name")
            && let Err(e) = crate::tools::property_files::check_name(&self.template.name)
        {
            issues.insert(
                0,
                Issue {
                    key: "name",
                    text: e.replace("файла свойств", "компонента"),
                    fix: None,
                },
            );
        }
        issues
    }

    /// The latest preview and whether a newer one is being built. Building
    /// runs in the background, so typing and dragging values stay smooth.
    pub fn preview(&mut self, project: &Project) -> (Option<&Preview>, bool) {
        if let Some(rx) = &self.pending {
            match rx.try_recv() {
                Ok(done) => {
                    self.cache = Some(done);
                    self.pending = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.pending = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        let fresh = self
            .cache
            .as_ref()
            .is_some_and(|(t, v, _)| *t == self.template && *v == self.view);
        if !fresh && self.pending.is_none() {
            let (tx, rx) = std::sync::mpsc::channel();
            let catalogue = Project {
                profiles: project.profiles.clone(),
                materials: project.materials.clone(),
                next_id: project.next_id,
                ..Default::default()
            };
            let (t, v) = (self.template.clone(), self.view);
            std::thread::spawn(move || {
                let p = preview(&catalogue, &t, v);
                let _ = tx.send((t, v, p));
            });
            self.pending = Some(rx);
        }
        (self.cache.as_ref().map(|c| &c.2), !fresh)
    }
}

/// Editor window state.
#[derive(Debug, Default)]
pub struct EditorState {
    pub draft: Option<Draft>,
    pub message: Option<String>,
    /// Name of a saved component waiting for «Удалить?» to be confirmed.
    pub confirm_delete: Option<String>,
}

impl Workbench {
    fn components_dir(&self) -> PathBuf {
        folder(&self.tools.path)
    }

    /// Saved components, for the catalogue.
    pub fn user_components(&self) -> Vec<UserComponent> {
        load_all(&self.components_dir()).0
    }

    /// Open the editor with a new component based on `c`.
    pub fn new_component(&mut self, c: &Component) {
        let taken: Vec<String> = self
            .user_components()
            .iter()
            .map(|u| u.name().to_string())
            .collect();
        self.component_editor.draft = Some(Draft::based_on(c, &taken));
        self.component_editor.message = None;
        self.message("Редактор компонентов: задайте значения и сохраните");
    }

    /// Open the editor on a saved component.
    pub fn edit_component(&mut self, name: &str) {
        match self
            .user_components()
            .into_iter()
            .find(|u| u.name() == name)
        {
            Some(u) => {
                self.component_editor.draft = Some(Draft::edit(&u));
                self.component_editor.message = None;
            }
            None => self.message(format!("Компонент «{name}» не найден")),
        }
    }

    /// Save the draft; with `arm`, place it in the model right away.
    pub fn save_component(&mut self, arm: bool) -> Result<Component, String> {
        let dir = self.components_dir();
        let draft = self
            .component_editor
            .draft
            .as_ref()
            .ok_or("Нечего сохранять")?;
        if let Some(issue) = draft.issues().first() {
            return Err(issue.text.clone());
        }
        let saved = draft.component();
        let renamed = draft.editing.as_deref() != Some(saved.name());
        if renamed && path(&dir, saved.name()).exists() {
            return Err(format!(
                "Компонент «{}» уже есть: введите другое имя",
                saved.name()
            ));
        }
        save(&dir, &saved, draft.editing.as_deref())?;
        let component = saved.component();
        self.component_editor.draft = None;
        self.message(format!("Компонент «{}» сохранён в «{GROUP}»", saved.name()));
        if arm {
            self.arm_component(component.clone());
        }
        Ok(component)
    }

    /// Delete a saved component file.
    pub fn delete_component(&mut self, name: &str) -> Result<(), String> {
        delete(&self.components_dir(), name)?;
        self.message(format!("Компонент «{name}» удалён"));
        Ok(())
    }

    /// «Редактор компонентов» window.
    pub(super) fn component_editor_window(&mut self, ctx: &egui::Context) {
        if self.component_editor.draft.is_none() {
            return;
        }
        let mut open = true;
        let mut action = None;
        let project = self.editor.document.project().clone();
        let state = &mut self.component_editor;
        let draft = state.draft.as_mut().expect("checked above");
        let title = if draft.editing.is_some() {
            "Редактор компонентов — изменение"
        } else {
            "Редактор компонентов — новый"
        };
        egui::Window::new(title)
            .id(egui::Id::new("component-editor"))
            .open(&mut open)
            .collapsible(false)
            .default_width(760.)
            .show(ctx, |ui| {
                let issues = draft.issues();
                let advice = draft.advice();
                ui.columns(2, |cols| {
                    egui::ScrollArea::vertical()
                        .id_salt("component-editor-form")
                        .max_height(520.)
                        .show(&mut cols[0], |ui| form(ui, draft, &issues, &advice));
                    preview_panel(&mut cols[1], draft, &project);
                });
                ui.separator();
                if issues.is_empty() && advice.is_empty() {
                    ui.colored_label(
                        egui::Color32::from_rgb(40, 140, 60),
                        "✔ Значения в порядке и соответствуют ГОСТ и СП 16.13330. Можно сохранять.",
                    );
                } else if issues.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ui.colored_label(
                            ADVICE,
                            format!(
                                "Узел строится, но замечаний по ГОСТ и СП 16.13330: {}. Сохранить можно.",
                                advice.len()
                            ),
                        );
                        if ui
                            .button("Привести к нормам")
                            .on_hover_text(
                                "Подставить значения по ГОСТ и СП 16.13330 во все поля с замечаниями",
                            )
                            .clicked()
                        {
                            bring_to_norms(&mut draft.template);
                        }
                    });
                } else {
                    ui.horizontal_wrapped(|ui| {
                        ui.colored_label(
                            ui.visuals().error_fg_color,
                            format!("Исправьте значений: {}.", issues.len()),
                        );
                        if issues.iter().any(|i| i.fix.is_some())
                            && ui
                                .button("Исправить всё автоматически")
                                .on_hover_text(
                                    "Подставить подходящие значения во все поля с ошибкой",
                                )
                                .clicked()
                        {
                            fix_all(&mut draft.template);
                        }
                    });
                }
                ui.horizontal(|ui| {
                    let ok = issues.is_empty();
                    if ui
                        .add_enabled(ok, egui::Button::new("Сохранить и поставить в модель"))
                        .on_hover_text(
                            "Сохранить в «Мои компоненты» и сразу выбрать детали для узла",
                        )
                        .clicked()
                    {
                        action = Some(true);
                    }
                    if ui
                        .add_enabled(ok, egui::Button::new("Сохранить"))
                        .on_hover_text("Сохранить в «Мои компоненты» (Ctrl+F)")
                        .clicked()
                    {
                        action = Some(false);
                    }
                    if ui
                        .button("Вернуть исходные значения")
                        .on_hover_text("Значения компонента, на котором основан этот")
                        .clicked()
                    {
                        draft.reset();
                    }
                });
                if let Some(m) = &state.message {
                    ui.label(m);
                }
            });
        if !open {
            self.component_editor.draft = None;
            return;
        }
        if let Some(arm) = action
            && let Err(e) = self.save_component(arm)
        {
            self.component_editor.message = Some(e);
        }
    }
}

fn issue_label(ui: &mut egui::Ui, t: &mut ConnectionTemplate, issue: &Issue) {
    ui.horizontal_wrapped(|ui| {
        ui.colored_label(ui.visuals().error_fg_color, format!("⚠ {}", issue.text));
        if let Some(v) = issue.fix
            && ui.small_button(format!("Поставить {}", mm(v))).clicked()
        {
            set(t, issue.key, v);
        }
    });
}

/// Amber, for a rule the value breaks while still being buildable.
const ADVICE: egui::Color32 = egui::Color32::from_rgb(190, 120, 0);

fn advice_label(ui: &mut egui::Ui, t: &mut ConnectionTemplate, a: &Advice) {
    ui.horizontal_wrapped(|ui| {
        ui.colored_label(ADVICE, format!("ℹ {}", a.text));
        if let Some(v) = a.fix
            && ui.small_button(format!("Поставить {}", mm(v))).clicked()
        {
            set(t, a.key, v);
        }
    });
}

fn form(ui: &mut egui::Ui, draft: &mut Draft, issues: &[Issue], advice: &[Advice]) {
    ui.heading("1. На основе какого узла");
    let catalog = components::catalog();
    let current = base(&draft.template.kind);
    let mut chosen = None;
    egui::ComboBox::new("component-base", "")
        .selected_text(current.name.as_str())
        .width(260.)
        .show_ui(ui, |ui| {
            for c in &catalog {
                if ui
                    .selectable_label(c.name == current.name, c.name.as_str())
                    .on_hover_text(c.description.as_str())
                    .clicked()
                {
                    chosen = Some(c.clone());
                }
            }
        });
    if let Some(c) = chosen.filter(|c| c.name != current.name) {
        draft.rebase(&c);
    }
    ui.small(base(&draft.template.kind).description);

    ui.add_space(6.);
    ui.heading("2. Имя и описание");
    let label = ui.label("Имя в каталоге");
    ui.text_edit_singleline(&mut draft.template.name)
        .labelled_by(label.id);
    for issue in issues.iter().filter(|i| i.key == "name") {
        issue_label(ui, &mut draft.template, issue);
    }
    let label = ui.label("Описание (необязательно)");
    ui.add(
        egui::TextEdit::multiline(&mut draft.description)
            .desired_rows(2)
            .hint_text("Например: шарнир для балок до I 30 на болтах М16")
            .desired_width(f32::INFINITY),
    )
    .labelled_by(label.id);

    let list = fields(&draft.template.kind);
    for (n, section) in [Section::Size, Section::Bolts, Section::Weld]
        .into_iter()
        .filter(|s| list.iter().any(|f| f.section == *s))
        .enumerate()
    {
        ui.add_space(6.);
        ui.heading(format!("{}. {}", n + 3, section.title()));
        if section == Section::Bolts && bolted(&draft.template.kind) {
            ui.horizontal(|ui| {
                let label = ui.label("Стандарт болта");
                egui::ComboBox::new("component-standard", "")
                    .selected_text(draft.template.bolt_standard.as_str())
                    .width(170.)
                    .show_ui(ui, |ui| {
                        for s in &BOLT_STANDARDS {
                            if ui
                                .selectable_label(draft.template.bolt_standard == s.name, s.name)
                                .on_hover_text(s.title)
                                .clicked()
                            {
                                set_standard(&mut draft.template, s);
                            }
                        }
                    })
                    .response
                    .labelled_by(label.id)
                    .on_hover_text(
                        bolt_standard(&draft.template.bolt_standard)
                            .map(|s| s.title)
                            .unwrap_or("Выберите стандарт болта"),
                    );
            });
            let standard =
                bolt_standard(&draft.template.bolt_standard).unwrap_or(&BOLT_STANDARDS[0]);
            ui.horizontal_wrapped(|ui| {
                ui.label("Болт:");
                for &d in standard.diameters {
                    let on = draft.template.bolt_diameter == d;
                    let c = hole_clearance(standard.name, d);
                    if ui
                        .selectable_label(on, format!("М{d}"))
                        .on_hover_text(format!("Диаметр {d} мм, отверстие Ø{} мм", mm(d + c)))
                        .clicked()
                    {
                        set_bolt(&mut draft.template, d);
                    }
                }
            });
            ui.horizontal(|ui| {
                let label = ui.label("Класс прочности");
                egui::ComboBox::new("component-grade", "")
                    .selected_text(draft.template.bolt_grade.as_str())
                    .show_ui(ui, |ui| {
                        for g in standard.grades {
                            ui.selectable_value(&mut draft.template.bolt_grade, (*g).into(), *g);
                        }
                    })
                    .response
                    .labelled_by(label.id);
            });
            for issue in issues
                .iter()
                .filter(|i| matches!(i.key, "bolt_standard" | "bolt_grade"))
            {
                issue_label(ui, &mut draft.template, issue);
            }
            for a in advice
                .iter()
                .filter(|a| matches!(a.key, "bolt_standard" | "bolt_grade"))
            {
                advice_label(ui, &mut draft.template, a);
            }
        }
        egui::Grid::new(("component-fields", n))
            .num_columns(2)
            .spacing([8., 4.])
            .show(ui, |ui| {
                for f in list.iter().filter(|f| f.section == section) {
                    let label = ui.label(f.label).on_hover_text(f.hint);
                    let mut v = get(&draft.template, f.key).unwrap_or_default();
                    let drag = if f.unit == "шт" {
                        egui::DragValue::new(&mut v).range(1..=12).speed(0.1)
                    } else {
                        egui::DragValue::new(&mut v)
                            .range(0.0..=10000.0)
                            .speed(1.)
                            .max_decimals(1)
                    };
                    let r = ui
                        .horizontal(|ui| {
                            let r = ui.add(drag.suffix(format!(" {}", f.unit)));
                            ui.weak("ⓘ").on_hover_text(f.hint);
                            r
                        })
                        .inner
                        .labelled_by(label.id)
                        .on_hover_text(f.hint);
                    if r.changed() {
                        set(&mut draft.template, f.key, v);
                    }
                    ui.end_row();
                    for issue in issues.iter().filter(|i| i.key == f.key) {
                        ui.label("");
                        issue_label(ui, &mut draft.template, issue);
                        ui.end_row();
                    }
                    for a in advice.iter().filter(|a| a.key == f.key) {
                        ui.label("");
                        advice_label(ui, &mut draft.template, a);
                        ui.end_row();
                    }
                }
            });
    }
    // Problems with values the form does not show for this kind.
    let shown: Vec<&str> = list.iter().map(|f| f.key).collect();
    for issue in issues.iter().filter(|i| {
        !shown.contains(&i.key) && !matches!(i.key, "name" | "bolt_standard" | "bolt_grade")
    }) {
        issue_label(ui, &mut draft.template, issue);
    }
}

fn preview_panel(ui: &mut egui::Ui, draft: &mut Draft, project: &Project) {
    ui.horizontal(|ui| {
        ui.heading("Предпросмотр");
        for v in View::ALL {
            ui.selectable_value(&mut draft.view, v, v.label());
        }
    });
    let (preview, updating) = draft.preview(project);
    let preview = preview.cloned();
    let size = egui::vec2(ui.available_width().max(200.), 360.);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4., ui.visuals().extreme_bg_color);
    if updating {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(100));
        painter.text(
            rect.left_top() + egui::vec2(8., 8.),
            egui::Align2::LEFT_TOP,
            "Обновляю…",
            egui::FontId::proportional(12.),
            ui.visuals().weak_text_color(),
        );
    }
    let Some(preview) = preview else {
        return;
    };
    if let Some(e) = &preview.error {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "Узел не строится",
            egui::FontId::proportional(16.),
            ui.visuals().error_fg_color,
        );
        ui.colored_label(ui.visuals().error_fg_color, e);
        return;
    }
    let (lo, hi) = preview.window;
    let scale = ((rect.width() - 16.) as f64 / (hi[0] - lo[0]))
        .min((rect.height() - 16.) as f64 / (hi[1] - lo[1]));
    let center = [(lo[0] + hi[0]) / 2., (lo[1] + hi[1]) / 2.];
    let to_screen = |q: [f64; 2]| {
        rect.center()
            + egui::vec2(
                ((q[0] - center[0]) * scale) as f32,
                (-(q[1] - center[1]) * scale) as f32,
            )
    };
    let grey = ui.visuals().weak_text_color();
    let accent = egui::Color32::from_rgb(40, 120, 220);
    for s in &preview.parents {
        painter.line_segment([to_screen(s[0]), to_screen(s[1])], (1.0, grey));
    }
    for s in &preview.parts {
        painter.line_segment([to_screen(s[0]), to_screen(s[1])], (1.6, accent));
    }
    ui.horizontal_wrapped(|ui| {
        ui.colored_label(accent, "■ детали узла");
        ui.colored_label(grey, "■ соединяемые детали (пример)");
    });
    ui.small(format!(
        "Деталей узла: {}, групп болтов: {}. Пример строится на сечениях по умолчанию; в модели узел подстроится под ваши детали.",
        preview.plates, preview.bolts
    ));
}
