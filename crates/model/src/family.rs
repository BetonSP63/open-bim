//! Parametric families («Семейства»): objects the engineer draws without
//! code. Reference planes stand on formulas over the parameters, bodies are
//! tied to the planes by naming them in their coordinates, and a family
//! has type sizes («типоразмеры»). A placed family is one model object that
//! builds ordinary parts and an assembly, so numbering, drawings and lists
//! need nothing new.
//!
//! Local axes: X0, Y0, Z0 are the base planes through the insertion point.
//! A family placed by two points runs along X, and `L` is the distance
//! between the points.
use crate::{
    Assembly, Element, ElementShape, Frame, LengthMm, MemberKind, ModelError, ObjectId,
    PartPosition, Point3, Project, PropertyValue, formula,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_FAMILY_PARAMETERS: usize = 128;
pub const MAX_FAMILY_PLANES: usize = 128;
pub const MAX_FAMILY_BODIES: usize = 64;
pub const MAX_FAMILY_TYPES: usize = 256;
pub const MAX_ARRAY_COUNT: usize = 200;
pub const MAX_FAMILY_PARTS: usize = 2000;

/// Deepest nesting of families in families.
pub const MAX_NESTING: usize = 4;

/// Names every family formula may use without defining them: the base
/// planes and the placement length.
pub const FAMILY_BUILTINS: [&str; 4] = ["X0", "Y0", "Z0", "L"];

/// Property the generated parts carry with the family name.
pub const FAMILY_PROPERTY: &str = "Семейство";
/// Property the generated parts carry with the type size.
pub const TYPE_PROPERTY: &str = "Типоразмер";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParamKind {
    Length,
    Angle,
    Number,
    Integer,
    YesNo,
    Material,
    Profile,
    Text,
}

impl ParamKind {
    pub const ALL: [Self; 8] = [
        Self::Length,
        Self::Angle,
        Self::Number,
        Self::Integer,
        Self::YesNo,
        Self::Material,
        Self::Profile,
        Self::Text,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Length => "Длина, мм",
            Self::Angle => "Угол, °",
            Self::Number => "Число",
            Self::Integer => "Целое",
            Self::YesNo => "Да/нет",
            Self::Material => "Материал",
            Self::Profile => "Профиль",
            Self::Text => "Текст",
        }
    }
    /// Numeric kinds take formulas; the others hold text.
    pub fn numeric(self) -> bool {
        matches!(
            self,
            Self::Length | Self::Angle | Self::Number | Self::Integer | Self::YesNo
        )
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FamilyParameter {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    pub kind: ParamKind,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub group: String,
    /// A type parameter takes its value from the type size; an instance
    /// parameter can differ for every placed object.
    #[serde(default)]
    pub of_type: bool,
    /// Default value; numeric kinds may write it as a formula over the
    /// names above.
    pub value: String,
    /// With a formula the value is computed and cannot be changed.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub formula: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
}

/// A type size: values of the type parameters by name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FamilyType {
    pub name: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub values: BTreeMap<String, String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    pub const ALL: [Self; 3] = [Self::X, Self::Y, Self::Z];
    fn index(self) -> usize {
        match self {
            Self::X => 0,
            Self::Y => 1,
            Self::Z => 2,
        }
    }
}

/// A reference plane across `axis` at the formula `at` (which may name
/// parameters and the planes above); its name is a value in later
/// formulas, so a body names a plane to stand on it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefPlane {
    pub name: String,
    pub axis: Axis,
    pub at: String,
}

/// A repeated body: `count` copies, each `step` further along (x, y, z).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BodyArray {
    pub count: String,
    pub step: [String; 3],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BodyShape {
    /// A plate across `normal` with its face at `at`; the thickness goes
    /// towards + `normal`. Contour points are in the other two axes in
    /// order (y, z) across X, (x, z) across Y, (x, y) across Z.
    Plate {
        normal: Axis,
        at: String,
        contour: Vec<[String; 2]>,
        thickness: String,
    },
    /// A beam on the reference line `start`→`end`; `profile` is a
    /// catalogue name or a profile parameter; `rotation` turns it about its
    /// axis, degrees.
    Member {
        profile: String,
        start: [String; 3],
        end: [String; 3],
        #[serde(default, skip_serializing_if = "String::is_empty")]
        rotation: String,
    },
    /// A nested family (one of `Family::nested`) with its insertion point
    /// at `at`, turned about Z by `rotation` degrees; `values` give its
    /// parameters (type ones too) as formulas over this family's names, or
    /// a text / text parameter for material, profile and text kinds; `L`
    /// sets its length.
    Nested {
        family: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        type_name: String,
        at: [String; 3],
        #[serde(default, skip_serializing_if = "String::is_empty")]
        rotation: String,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        values: BTreeMap<String, String>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FamilyBody {
    pub name: String,
    /// A steel grade («С255») or a material parameter.
    pub material: String,
    /// Shown while this formula is above zero; empty always shows.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub visible: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub array: Option<BodyArray>,
    pub shape: BodyShape,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Placement {
    /// At a point with a rotation.
    #[default]
    Point,
    /// Between two points; `L` is their distance.
    TwoPoints,
}

/// How the lists show a placed family.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpecMode {
    /// One shipping mark, its parts as positions.
    #[default]
    Assembly,
    /// One line with the total mass (an embedded item, a product).
    Item,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Family {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default)]
    pub placement: Placement,
    #[serde(default)]
    pub spec: SpecMode,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<FamilyParameter>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub types: Vec<FamilyType>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub planes: Vec<RefPlane>,
    pub bodies: Vec<FamilyBody>,
    /// Families its bodies nest, carried inside so the definition stands
    /// alone.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nested: Vec<Family>,
}

/// Evaluated parameter and plane values of one family object.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FamilyValues {
    pub numbers: BTreeMap<String, f64>,
    pub texts: BTreeMap<String, String>,
}

/// A part in family coordinates.
#[derive(Clone, Debug, PartialEq)]
pub enum LocalShape {
    Plate {
        contour: Vec<[f64; 2]>,
        thickness: f64,
        frame: Frame,
    },
    Member {
        profile: String,
        start: Point3,
        end: Point3,
        rotation: f64,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct FamilyPart {
    /// Stable role of the part in its object: «body-<body>-<copy>».
    pub role: String,
    pub name: String,
    pub material: String,
    pub shape: LocalShape,
}

fn bad<T>(message: String) -> Result<T, ModelError> {
    Err(ModelError::InvalidValue(message))
}

fn long(src: &str) -> bool {
    src.chars().count() > formula::MAX_FORMULA
}

/// «да», «true» and «1» are yes; «нет», «false» and «0» are no.
fn yes_no(src: &str) -> Option<&'static str> {
    match src.trim().to_lowercase().as_str() {
        "да" | "true" => Some("1"),
        "нет" | "false" => Some("0"),
        _ => None,
    }
}

impl Family {
    pub fn parameter(&self, name: &str) -> Option<&FamilyParameter> {
        self.parameters.iter().find(|p| p.name == name)
    }

    pub fn family_type(&self, name: &str) -> Option<&FamilyType> {
        self.types.iter().find(|t| t.name == name)
    }

    pub fn nested_family(&self, name: &str) -> Option<&Family> {
        self.nested.iter().find(|f| f.name == name)
    }

    pub fn validate(&self) -> Result<(), ModelError> {
        self.validate_at(0)
    }

    fn validate_at(&self, depth: usize) -> Result<(), ModelError> {
        if depth > MAX_NESTING {
            return bad(format!("Семейства вложены глубже {MAX_NESTING} уровней"));
        }
        let mut nested = BTreeSet::new();
        for n in &self.nested {
            if !nested.insert(&n.name) {
                return bad(format!("Вложенное семейство «{}» повторяется", n.name));
            }
            n.validate_at(depth + 1)?;
        }
        let name = self.name.trim();
        if name.is_empty() || name.chars().count() > 128 || name != self.name {
            return bad("Имя семейства: от 1 до 128 знаков без пробелов по краям".into());
        }
        if self.description.len() > 4096 {
            return bad("Слишком длинное описание семейства".into());
        }
        if self.parameters.len() > MAX_FAMILY_PARAMETERS
            || self.planes.len() > MAX_FAMILY_PLANES
            || self.bodies.len() > MAX_FAMILY_BODIES
            || self.types.len() > MAX_FAMILY_TYPES
        {
            return bad(format!(
                "Не больше {MAX_FAMILY_PARAMETERS} параметров, {MAX_FAMILY_PLANES} плоскостей, \
                 {MAX_FAMILY_BODIES} тел и {MAX_FAMILY_TYPES} типоразмеров"
            ));
        }
        if self.bodies.is_empty() {
            return bad(format!("В семействе «{}» нет ни одного тела", self.name));
        }
        let mut names: BTreeSet<&str> = FAMILY_BUILTINS.into_iter().collect();
        for p in &self.parameters {
            if !formula::valid_name(&p.name) || !names.insert(&p.name) {
                return bad(format!(
                    "Параметр «{}»: имя недопустимо или уже занято",
                    p.name
                ));
            }
            if p.label.len() > 256 || p.group.len() > 256 {
                return bad(format!("Параметр «{}»: слишком длинная подпись", p.name));
            }
            if long(&p.value) || long(&p.formula) {
                return bad(format!(
                    "Параметр «{}»: формула длиннее {} знаков",
                    p.name,
                    formula::MAX_FORMULA
                ));
            }
            if !p.kind.numeric() && !p.formula.is_empty() {
                return bad(format!(
                    "Параметр «{}»: формула задаётся только числам",
                    p.name
                ));
            }
            if !p.formula.is_empty() && p.of_type {
                return bad(format!(
                    "Параметр «{}» вычисляется формулой и не входит в типоразмер",
                    p.name
                ));
            }
            let limits = [p.min, p.max];
            if limits.iter().flatten().any(|v| !v.is_finite())
                || p.min.zip(p.max).is_some_and(|(a, b)| a > b)
                || (!p.kind.numeric() && limits.iter().any(Option::is_some))
            {
                return bad(format!("Параметр «{}»: неверные пределы", p.name));
            }
        }
        let mut types = BTreeSet::new();
        for t in &self.types {
            if t.name.trim().is_empty() || t.name.chars().count() > 128 || !types.insert(&t.name) {
                return bad(format!(
                    "Типоразмер «{}»: имя пустое или повторяется",
                    t.name
                ));
            }
            for (key, value) in &t.values {
                if !self.parameter(key).is_some_and(|p| p.of_type) {
                    return bad(format!("Типоразмер «{}»: «{key}» не параметр типа", t.name));
                }
                if long(value) {
                    return bad(format!("Типоразмер «{}»: «{key}» слишком длинно", t.name));
                }
            }
        }
        for plane in &self.planes {
            if !formula::valid_name(&plane.name) || !names.insert(&plane.name) {
                return bad(format!(
                    "Плоскость «{}»: имя недопустимо или уже занято",
                    plane.name
                ));
            }
            if long(&plane.at) {
                return bad(format!(
                    "Плоскость «{}»: формула слишком длинная",
                    plane.name
                ));
            }
        }
        for b in &self.bodies {
            if b.name.trim().is_empty() || b.name.chars().count() > 128 {
                return bad("У тела должно быть имя до 128 знаков".into());
            }
            let what = |m: &str| format!("Тело «{}»: {m}", b.name);
            if b.material.trim().is_empty() || long(&b.material) || long(&b.visible) {
                return bad(what("укажите материал; формулы до 256 знаков"));
            }
            if let Some(a) = &b.array
                && (long(&a.count) || a.step.iter().any(|s| long(s)))
            {
                return bad(what("формула массива слишком длинная"));
            }
            match &b.shape {
                BodyShape::Plate {
                    at,
                    contour,
                    thickness,
                    ..
                } => {
                    if !(3..=64).contains(&contour.len()) {
                        return bad(what("в контуре от 3 до 64 точек"));
                    }
                    if long(at) || long(thickness) || contour.iter().flatten().any(|f| long(f)) {
                        return bad(what("формула слишком длинная"));
                    }
                }
                BodyShape::Member {
                    profile,
                    start,
                    end,
                    rotation,
                } => {
                    if profile.trim().is_empty()
                        || long(profile)
                        || long(rotation)
                        || start.iter().chain(end).any(|f| long(f))
                    {
                        return bad(what("укажите профиль; формулы до 256 знаков"));
                    }
                }
                BodyShape::Nested {
                    family,
                    type_name,
                    at,
                    rotation,
                    values,
                } => {
                    let Some(child) = self.nested_family(family) else {
                        return bad(what(&format!("нет вложенного семейства «{family}»")));
                    };
                    if !type_name.is_empty() && child.family_type(type_name).is_none() {
                        return bad(what(&format!("у «{family}» нет типоразмера «{type_name}»")));
                    }
                    for (k, v) in values {
                        let known =
                            k == "L" || child.parameter(k).is_some_and(|p| p.formula.is_empty());
                        if !known {
                            return bad(what(&format!("у «{family}» нет параметра «{k}»")));
                        }
                        if long(v) {
                            return bad(what("формула слишком длинная"));
                        }
                    }
                    if long(rotation) || at.iter().any(|f| long(f)) {
                        return bad(what("формула слишком длинная"));
                    }
                }
            }
        }
        Ok(())
    }

    /// The type sizes as CSV for a spreadsheet: «;» between fields, the
    /// first column the type name, then one column per type parameter;
    /// an empty cell keeps the parameter's default.
    pub fn types_csv(&self) -> String {
        let columns: Vec<&str> = self
            .parameters
            .iter()
            .filter(|p| p.of_type)
            .map(|p| p.name.as_str())
            .collect();
        let field = |v: &str| {
            if v.contains([';', '"', '\n']) {
                format!("\"{}\"", v.replace('"', "\"\""))
            } else {
                v.to_string()
            }
        };
        let mut out = std::iter::once("Типоразмер")
            .chain(columns.iter().copied())
            .map(field)
            .collect::<Vec<_>>()
            .join(";");
        out.push('\n');
        for t in &self.types {
            let row: Vec<String> = std::iter::once(field(&t.name))
                .chain(
                    columns
                        .iter()
                        .map(|c| field(t.values.get(*c).map(String::as_str).unwrap_or(""))),
                )
                .collect();
            out.push_str(&row.join(";"));
            out.push('\n');
        }
        out
    }

    /// Replace the type sizes with those of `csv` (as `types_csv` writes
    /// it; «,» also separates fields when the header has no «;»).
    pub fn read_types_csv(&mut self, csv: &str) -> Result<(), String> {
        let csv = csv.trim_start_matches('\u{feff}');
        let header_line = csv.lines().next().ok_or("Пустой файл")?;
        let sep = if header_line.contains(';') { ';' } else { ',' };
        let rows = csv_rows(csv, sep)?;
        let (header, rows) = rows.split_first().ok_or("Пустой файл")?;
        for name in &header[1..] {
            if !self.parameter(name).is_some_and(|p| p.of_type) {
                return Err(format!("«{name}» не параметр типа этого семейства"));
            }
        }
        let mut types = Vec::new();
        for (i, row) in rows.iter().enumerate() {
            if row.iter().all(|c| c.trim().is_empty()) {
                continue;
            }
            if row.len() > header.len() {
                return Err(format!("Строка {}: лишние значения", i + 2));
            }
            types.push(FamilyType {
                name: row[0].trim().to_string(),
                values: header[1..]
                    .iter()
                    .zip(&row[1..])
                    .filter(|(_, v)| !v.trim().is_empty())
                    .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
                    .collect(),
            });
        }
        let mut next = self.clone();
        next.types = types;
        next.validate().map_err(|e| e.to_string())?;
        *self = next;
        Ok(())
    }

    /// Parameter and plane values of an object of type `type_name` with
    /// instance values `instance`; `length` is `L`, `extra` are model
    /// parameters the instance values may use.
    pub fn values(
        &self,
        type_name: &str,
        instance: &BTreeMap<String, String>,
        length: f64,
        extra: &BTreeMap<String, f64>,
    ) -> Result<FamilyValues, String> {
        self.values_with(type_name, instance, length, extra, false)
    }

    /// `values`; with `nested` the given values may set type parameters
    /// too, as a host family sets those of a family it nests.
    fn values_with(
        &self,
        type_name: &str,
        instance: &BTreeMap<String, String>,
        length: f64,
        extra: &BTreeMap<String, f64>,
        nested: bool,
    ) -> Result<FamilyValues, String> {
        let family_type = if type_name.is_empty() {
            None
        } else {
            Some(
                self.family_type(type_name)
                    .ok_or_else(|| format!("Нет типоразмера «{type_name}»"))?,
            )
        };
        for key in instance.keys() {
            match self.parameter(key) {
                Some(p) if (nested || !p.of_type) && p.formula.is_empty() => {}
                Some(_) => {
                    return Err(format!(
                        "«{key}» задаётся типоразмером или формулой, а не у объекта"
                    ));
                }
                None => return Err(format!("В семействе нет параметра «{key}»")),
            }
        }
        let mut numbers = extra.clone();
        for (name, value) in FAMILY_BUILTINS.into_iter().zip([0., 0., 0., length]) {
            numbers.insert(name.into(), value);
        }
        let mut texts = BTreeMap::new();
        for p in &self.parameters {
            let given = match instance.get(&p.name) {
                Some(v) if nested || !p.of_type => Some(v),
                _ if p.of_type => family_type.and_then(|t| t.values.get(&p.name)),
                _ => None,
            };
            let src = if p.formula.is_empty() {
                given.unwrap_or(&p.value).as_str()
            } else {
                p.formula.as_str()
            };
            if !p.kind.numeric() {
                if src.trim().is_empty() && p.kind != ParamKind::Text {
                    return Err(format!("{}: значение не задано", p.name));
                }
                texts.insert(p.name.clone(), src.to_string());
                continue;
            }
            let src = match p.kind {
                ParamKind::YesNo => yes_no(src).unwrap_or(src),
                _ => src,
            };
            let mut v = formula::eval(src, &numbers).map_err(|e| format!("{}: {e}", p.name))?;
            match p.kind {
                ParamKind::Integer if (v - v.round()).abs() > 1e-9 => {
                    return Err(format!("{}: нужно целое число, получено {v}", p.name));
                }
                ParamKind::Integer => v = v.round(),
                ParamKind::YesNo => v = if v > 0. { 1. } else { 0. },
                _ => {}
            }
            if let Some(min) = p.min.filter(|m| v < *m - 1e-9) {
                return Err(format!("{} = {v} меньше наименьшего {min}", p.name));
            }
            if let Some(max) = p.max.filter(|m| v > *m + 1e-9) {
                return Err(format!("{} = {v} больше наибольшего {max}", p.name));
            }
            numbers.insert(p.name.clone(), v);
        }
        for plane in &self.planes {
            let v = formula::eval(&plane.at, &numbers)
                .map_err(|e| format!("Плоскость {}: {e}", plane.name))?;
            numbers.insert(plane.name.clone(), v);
        }
        Ok(FamilyValues { numbers, texts })
    }

    /// The parts of an object with `values`, in family coordinates.
    pub fn evaluate(&self, values: &FamilyValues) -> Result<Vec<FamilyPart>, String> {
        let mut parts = Vec::new();
        for (i, b) in self.bodies.iter().enumerate() {
            let what = |m: &str| format!("{} — {m}", b.name);
            let f = |src: &str, part: &str| {
                formula::eval(src, &values.numbers).map_err(|e| format!("{}: {e}", what(part)))
            };
            if !b.visible.is_empty() && f(&b.visible, "видимость")? <= 0. {
                continue;
            }
            let (count, step) = match &b.array {
                None => (1, [0.; 3]),
                Some(a) => {
                    let n = f(&a.count, "число в массиве")?.round();
                    if !(0. ..=MAX_ARRAY_COUNT as f64).contains(&n) {
                        return Err(what(&format!("в массиве от 0 до {MAX_ARRAY_COUNT} копий")));
                    }
                    let step = [
                        f(&a.step[0], "шаг x")?,
                        f(&a.step[1], "шаг y")?,
                        f(&a.step[2], "шаг z")?,
                    ];
                    (n as usize, step)
                }
            };
            let material = values
                .texts
                .get(&b.material)
                .filter(|_| {
                    self.parameter(&b.material)
                        .is_some_and(|p| p.kind == ParamKind::Material)
                })
                .unwrap_or(&b.material)
                .clone();
            // (role suffix, name, material, shape) of the body's pieces.
            let single =
                |shape: LocalShape| vec![(String::new(), b.name.clone(), material.clone(), shape)];
            let pieces = match &b.shape {
                BodyShape::Plate {
                    normal,
                    at,
                    contour,
                    thickness,
                } => {
                    let at = f(at, "положение")?;
                    let t = f(thickness, "толщина")?;
                    if t <= 0. {
                        return Err(what(&format!("толщина {t} мм должна быть больше нуля")));
                    }
                    let mut points = Vec::new();
                    for (k, [u, v]) in contour.iter().enumerate() {
                        let point = format!("точка {}", k + 1);
                        points.push([f(u, &point)?, f(v, &point)?]);
                    }
                    single(plate(*normal, at, &points, t).map_err(|e| what(&e))?)
                }
                BodyShape::Member {
                    profile,
                    start,
                    end,
                    rotation,
                } => {
                    let profile = values
                        .texts
                        .get(profile)
                        .filter(|_| {
                            self.parameter(profile)
                                .is_some_and(|p| p.kind == ParamKind::Profile)
                        })
                        .unwrap_or(profile)
                        .clone();
                    let point = |p: &[String; 3], part: &str| -> Result<Point3, String> {
                        Ok(Point3([f(&p[0], part)?, f(&p[1], part)?, f(&p[2], part)?]))
                    };
                    let (s, e) = (point(start, "начало")?, point(end, "конец")?);
                    if s.distance(e) <= 0.001 {
                        return Err(what("начало и конец совпадают"));
                    }
                    let rotation = if rotation.is_empty() {
                        0.
                    } else {
                        f(rotation, "поворот")?
                    };
                    single(LocalShape::Member {
                        profile,
                        start: s,
                        end: e,
                        rotation,
                    })
                }
                BodyShape::Nested {
                    family,
                    type_name,
                    at,
                    rotation,
                    values: given,
                } => {
                    let child = self
                        .nested_family(family)
                        .ok_or_else(|| what(&format!("нет вложенного семейства «{family}»")))?;
                    let mut mapped = BTreeMap::new();
                    let mut length = 0.;
                    for (k, src) in given {
                        let what_k = format!("«{k}»");
                        match child.parameter(k) {
                            _ if k == "L" => length = f(src, &what_k)?,
                            Some(p) if p.kind.numeric() => {
                                mapped.insert(k.clone(), format!("{}", f(src, &what_k)?));
                            }
                            _ => {
                                let text = values.texts.get(src).unwrap_or(src);
                                mapped.insert(k.clone(), text.clone());
                            }
                        }
                    }
                    let child_values = child
                        .values_with(type_name, &mapped, length, &BTreeMap::new(), true)
                        .map_err(|e| what(&format!("{family}: {e}")))?;
                    let turn = if rotation.is_empty() {
                        0.
                    } else {
                        f(rotation, "поворот")?
                    }
                    .to_radians();
                    let (c, sn) = (turn.cos(), turn.sin());
                    let place = Frame {
                        origin: Point3([f(&at[0], "x")?, f(&at[1], "y")?, f(&at[2], "z")?]),
                        axes: [[c, sn, 0.], [-sn, c, 0.], [0., 0., 1.]],
                    };
                    child
                        .evaluate(&child_values)
                        .map_err(|e| what(&e))?
                        .into_iter()
                        .map(|p| {
                            (
                                format!(".{}", p.role),
                                p.name,
                                p.material,
                                placed(&p.shape, &place),
                            )
                        })
                        .collect()
                }
            };
            for k in 0..count {
                let shift = Frame {
                    origin: Point3(step.map(|s| s * k as f64)),
                    ..Frame::default()
                };
                for (suffix, name, material, shape) in &pieces {
                    if parts.len() >= MAX_FAMILY_PARTS {
                        return Err(format!("Больше {MAX_FAMILY_PARTS} деталей в одном объекте"));
                    }
                    parts.push(FamilyPart {
                        role: format!("body-{i}-{k}{suffix}"),
                        name: name.clone(),
                        material: material.clone(),
                        shape: placed(shape, &shift),
                    });
                }
            }
        }
        if parts.is_empty() {
            return Err(format!("Семейство «{}» не дало ни одной детали", self.name));
        }
        Ok(parts)
    }
}

/// Fields of `csv` by rows, with «"» quoting.
fn csv_rows(csv: &str, sep: char) -> Result<Vec<Vec<String>>, String> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = csv.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            c if c == sep && !quoted => row.push(std::mem::take(&mut field)),
            '\r' if !quoted => {}
            '\n' if !quoted => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            c => field.push(c),
        }
        if rows.len() > MAX_FAMILY_TYPES + 1 {
            return Err(format!("Не больше {MAX_FAMILY_TYPES} типоразмеров"));
        }
    }
    if quoted {
        return Err("Не закрыты кавычки".into());
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    Ok(rows)
}

/// `shape` in the coordinates `frame` is given in.
fn placed(shape: &LocalShape, frame: &Frame) -> LocalShape {
    match shape {
        LocalShape::Plate {
            contour,
            thickness,
            frame: own,
        } => LocalShape::Plate {
            contour: contour.clone(),
            thickness: *thickness,
            frame: Frame {
                origin: frame.world(own.origin),
                axes: own.axes.map(|a| direction(frame, a)),
            },
        },
        LocalShape::Member {
            profile,
            start,
            end,
            rotation,
        } => LocalShape::Member {
            profile: profile.clone(),
            start: frame.world(*start),
            end: frame.world(*end),
            rotation: *rotation,
        },
    }
}

/// A plate across `normal` at `at` with its contour in the other two axes;
/// the frame stays right-handed with the thickness towards + `normal`.
fn plate(normal: Axis, at: f64, points: &[[f64; 2]], thickness: f64) -> Result<LocalShape, String> {
    let unit = |i: usize| std::array::from_fn(|j| f64::from(u8::from(i == j)));
    // In-plane axes and whether the second runs against its world axis.
    let (u, v, flip) = match normal {
        Axis::X => (1, 2, false),
        Axis::Y => (0, 2, true),
        Axis::Z => (0, 1, false),
    };
    let mut contour: Vec<[f64; 2]> = points
        .iter()
        .map(|p| [p[0], if flip { -p[1] } else { p[1] }])
        .collect();
    let area: f64 = (0..contour.len())
        .map(|i| {
            let (p, q) = (contour[i], contour[(i + 1) % contour.len()]);
            p[0] * q[1] - q[0] * p[1]
        })
        .sum();
    if area < 0. {
        contour.reverse();
    }
    crate::polygon_area(&contour).map_err(|e| format!("контур: {e}"))?;
    let mut origin = [0.; 3];
    origin[normal.index()] = at;
    let b: [f64; 3] = unit(v);
    Ok(LocalShape::Plate {
        contour,
        thickness,
        frame: Frame {
            origin: Point3(origin),
            axes: [
                unit(u),
                if flip { b.map(|c| -c) } else { b },
                unit(normal.index()),
            ],
        },
    })
}

/// A placed family: its type size, placement and instance values, and the
/// parts it built.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FamilyInstance {
    pub id: ObjectId,
    pub family: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub type_name: String,
    /// Insertion point and the directions of X0, Y0, Z0 in the model.
    pub frame: Frame,
    /// `L`: distance between the two points of a two-point family, mm.
    #[serde(default)]
    pub length: f64,
    /// Instance parameter values, or formulas over model parameters.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub values: BTreeMap<String, String>,
    /// Built objects by role («body-0-0», «assembly»); the generator owns
    /// them.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub children: BTreeMap<String, ObjectId>,
}

impl FamilyInstance {
    /// Frame and length of a two-point family from `start` to `end`: X runs
    /// along, Z up as far as the direction allows.
    pub fn between(start: Point3, end: Point3) -> Result<(Frame, f64), ModelError> {
        crate::validation::valid_point(start)?;
        crate::validation::valid_point(end)?;
        let length = start.distance(end);
        if length <= 0.001 {
            return bad("Точки семейства совпадают".into());
        }
        let x: [f64; 3] = std::array::from_fn(|i| (end.0[i] - start.0[i]) / length);
        let up = if x[2].abs() < 0.9 {
            [0., 0., 1.]
        } else {
            [0., 1., 0.]
        };
        let norm = |v: [f64; 3]| {
            let n = v.iter().map(|c| c * c).sum::<f64>().sqrt();
            v.map(|c| c / n)
        };
        let y = norm(cross(up, x));
        let z = cross(x, y);
        let frame = Frame {
            origin: start,
            axes: [x, y, z],
        };
        frame.validate()?;
        Ok((frame, length))
    }

    pub fn validate(&self, project: &Project) -> Result<(), ModelError> {
        self.frame.validate()?;
        if !self.length.is_finite() || !(0. ..=1e6).contains(&self.length) {
            return bad("Длина семейства от 0 до 1 000 000 мм".into());
        }
        let family = project.families.get(&self.family).ok_or_else(|| {
            ModelError::InvalidValue(format!("В проекте нет семейства «{}»", self.family))
        })?;
        if !self.type_name.is_empty() && family.family_type(&self.type_name).is_none() {
            return bad(format!(
                "У семейства «{}» нет типоразмера «{}»",
                self.family, self.type_name
            ));
        }
        if self.values.len() > MAX_FAMILY_PARAMETERS
            || self.values.values().any(|v| long(v))
            || self.children.len() > MAX_FAMILY_PARTS + 1
        {
            return bad("Слишком много значений или деталей семейства".into());
        }
        for id in self.children.values() {
            if !project.elements.contains_key(id) && !project.assemblies.contains_key(id) {
                return Err(ModelError::MissingReference(*id));
            }
        }
        Ok(())
    }
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// A direction of the family in the model.
fn direction(frame: &Frame, v: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| (0..3).map(|j| frame.axes[j][i] * v[j]).sum())
}

/// A project material for `name`: the material of that name, the steel
/// grade before « · », or the catalogue grade added to the project.
fn material_id(project: &mut Project, name: &str) -> Result<ObjectId, ModelError> {
    let name = name.trim();
    if let Some(m) = project
        .materials
        .values()
        .find(|m| m.name == name || m.name.split(" · ").next() == Some(name))
    {
        return Ok(m.id);
    }
    let id = project.allocate_id()?;
    let material = crate::library::steel(id, name).ok_or_else(|| {
        ModelError::InvalidValue(format!(
            "Нет материала «{name}» ни в проекте, ни в каталоге"
        ))
    })?;
    project.materials.insert(id, material);
    Ok(id)
}

/// A project profile for `name`, copied from the catalogue when missing.
fn profile_id(project: &mut Project, name: &str) -> Result<ObjectId, ModelError> {
    let name = name.trim();
    if let Some(p) = project.profiles.values().find(|p| p.name == name) {
        return Ok(p.id);
    }
    let entry = crate::library::section(name).ok_or_else(|| {
        ModelError::InvalidValue(format!("Нет профиля «{name}» ни в проекте, ни в каталоге"))
    })?;
    let id = project.allocate_id()?;
    project.profiles.insert(id, entry.profile(id));
    Ok(id)
}

/// Model parameters by name, for instance values written as formulas.
fn model_values(project: &Project) -> BTreeMap<String, f64> {
    project.parametric.values().unwrap_or_default()
}

/// The parts of `instance` in family coordinates.
pub fn instance_parts(
    project: &Project,
    instance: &FamilyInstance,
) -> Result<Vec<FamilyPart>, ModelError> {
    let family = project.families.get(&instance.family).ok_or_else(|| {
        ModelError::InvalidValue(format!("В проекте нет семейства «{}»", instance.family))
    })?;
    let fail = |e: String| ModelError::InvalidValue(format!("Семейство «{}»: {e}", family.name));
    let values = family
        .values(
            &instance.type_name,
            &instance.values,
            instance.length,
            &model_values(project),
        )
        .map_err(fail)?;
    family.evaluate(&values).map_err(fail)
}

/// Build again every family object whose instance, definition or model
/// parameters changed since `previous`; refuse direct edits of the parts
/// a family owns.
pub fn rebuild_families(project: &mut Project, previous: &Project) -> Result<(), ModelError> {
    let parametric_changed = project.parametric != previous.parametric;
    let instances: Vec<FamilyInstance> = project.family_instances.values().cloned().collect();
    for mut instance in instances {
        let changed = parametric_changed
            || previous.family_instances.get(&instance.id) != Some(&instance)
            || project.families.get(&instance.family) != previous.families.get(&instance.family);
        if !changed {
            for id in instance.children.values() {
                let (old, new) = (previous.elements.get(id), project.elements.get(id));
                if let (Some(old), Some(new)) = (old, new)
                    && (old.shape != new.shape || old.material != new.material)
                {
                    return bad(format!(
                        "Деталь «{}» строит семейство «{}»: измените его параметры",
                        old.name, instance.family
                    ));
                }
            }
            continue;
        }
        let parts = instance_parts(project, &instance)?;
        let family = project.families[&instance.family].clone();
        let keep: BTreeSet<String> = parts
            .iter()
            .map(|p| p.role.clone())
            .chain(["assembly".to_string()])
            .collect();
        for (role, id) in std::mem::take(&mut instance.children) {
            if keep.contains(&role) {
                instance.children.insert(role, id);
            } else {
                project.elements.remove(&id);
                project.assemblies.remove(&id);
            }
        }
        let frame = instance.frame;
        let mut ids = Vec::new();
        for part in &parts {
            let material = material_id(project, &part.material)?;
            let shape = match &part.shape {
                LocalShape::Plate {
                    contour,
                    thickness,
                    frame: local,
                } => ElementShape::Plate {
                    contour: contour.clone(),
                    thickness: LengthMm::new(*thickness)
                        .map_err(|e| ModelError::InvalidValue(e.to_string()))?,
                    frame: Frame {
                        origin: frame.world(local.origin),
                        axes: local.axes.map(|a| direction(&frame, a)),
                    },
                },
                LocalShape::Member {
                    profile,
                    start,
                    end,
                    rotation,
                } => {
                    let profile = profile_id(project, profile)?;
                    project.new_member(
                        MemberKind::Beam,
                        profile,
                        frame.world(*start),
                        frame.world(*end),
                        PartPosition {
                            rotation_angle: *rotation,
                            ..PartPosition::default()
                        },
                    )?
                }
            };
            let id = match instance.children.get(&part.role) {
                Some(id) => *id,
                None => {
                    let id = project.allocate_id()?;
                    instance.children.insert(part.role.clone(), id);
                    id
                }
            };
            let mut properties = project
                .elements
                .get(&id)
                .map(|e| e.properties.clone())
                .unwrap_or_default();
            properties.insert(
                FAMILY_PROPERTY.into(),
                PropertyValue::Text(family.name.clone()),
            );
            if instance.type_name.is_empty() {
                properties.remove(TYPE_PROPERTY);
            } else {
                properties.insert(
                    TYPE_PROPERTY.into(),
                    PropertyValue::Text(instance.type_name.clone()),
                );
            }
            project.elements.insert(
                id,
                Element {
                    id,
                    name: part.name.clone(),
                    material,
                    shape,
                    properties,
                },
            );
            ids.push(id);
        }
        let assembly = match instance.children.get("assembly") {
            Some(id) => *id,
            None => {
                let id = project.allocate_id()?;
                instance.children.insert("assembly".into(), id);
                id
            }
        };
        let mut properties = project
            .assemblies
            .get(&assembly)
            .map(|a| a.properties.clone())
            .unwrap_or_default();
        properties.insert(
            FAMILY_PROPERTY.into(),
            PropertyValue::Text(family.name.clone()),
        );
        project.assemblies.insert(
            assembly,
            Assembly {
                id: assembly,
                name: if instance.type_name.is_empty() {
                    family.name.clone()
                } else {
                    format!("{} {}", family.name, instance.type_name)
                },
                main_part: ids[0],
                members: ids.iter().copied().collect(),
                children: BTreeSet::new(),
                properties,
            },
        );
        project.family_instances.insert(instance.id, instance);
    }
    Ok(())
}

/// Ready families to start from: a support seat with type sizes and a
/// ladder placed by two points.
pub fn sample_families() -> Vec<Family> {
    let s = |v: &str| v.to_string();
    let length = |name: &str, label: &str, of_type: bool, value: &str| FamilyParameter {
        name: s(name),
        label: s(label),
        kind: ParamKind::Length,
        group: s("Размеры"),
        of_type,
        value: s(value),
        formula: String::new(),
        min: Some(1.),
        max: None,
    };
    let material = FamilyParameter {
        name: s("Материал"),
        label: s("Сталь"),
        kind: ParamKind::Material,
        group: s("Материалы"),
        of_type: true,
        value: s("С255"),
        formula: String::new(),
        min: None,
        max: None,
    };
    let sizes = |name: &str, v: [&str; 4]| FamilyType {
        name: s(name),
        values: ["B", "A", "H", "t"]
            .into_iter()
            .zip(v)
            .map(|(k, v)| (s(k), s(v)))
            .collect(),
    };
    let seat = Family {
        name: s("Столик опорный"),
        nested: Vec::new(),
        description: s("Горизонтальная пластина и рёбра под ней"),
        placement: Placement::Point,
        spec: SpecMode::Assembly,
        parameters: vec![
            material.clone(),
            length("B", "Ширина столика", true, "200"),
            length("A", "Вылет", true, "120"),
            length("H", "Высота рёбер", true, "150"),
            length("t", "Толщина столика", true, "16"),
            length("tr", "Толщина ребра", false, "12"),
            FamilyParameter {
                name: s("n"),
                label: s("Число рёбер"),
                kind: ParamKind::Integer,
                group: s("Рёбра"),
                of_type: false,
                value: s("2"),
                formula: String::new(),
                min: Some(0.),
                max: Some(4.),
            },
            length("отступ", "Отступ рёбер от края", false, "20"),
        ],
        types: vec![
            sizes("С-1", ["150", "100", "120", "14"]),
            sizes("С-2", ["200", "120", "150", "16"]),
            sizes("С-3", ["250", "150", "200", "20"]),
        ],
        planes: vec![
            RefPlane {
                name: s("Лево"),
                axis: Axis::X,
                at: s("X0 - B / 2"),
            },
            RefPlane {
                name: s("Право"),
                axis: Axis::X,
                at: s("X0 + B / 2"),
            },
            RefPlane {
                name: s("Перед"),
                axis: Axis::Y,
                at: s("Y0 + A"),
            },
            RefPlane {
                name: s("Низ_столика"),
                axis: Axis::Z,
                at: s("Z0 - t"),
            },
            RefPlane {
                name: s("Низ_рёбер"),
                axis: Axis::Z,
                at: s("Низ_столика - H"),
            },
        ],
        bodies: vec![
            FamilyBody {
                name: s("Столик"),
                material: s("Материал"),
                visible: String::new(),
                array: None,
                shape: BodyShape::Plate {
                    normal: Axis::Z,
                    at: s("Низ_столика"),
                    contour: vec![
                        [s("Лево"), s("Y0")],
                        [s("Право"), s("Y0")],
                        [s("Право"), s("Перед")],
                        [s("Лево"), s("Перед")],
                    ],
                    thickness: s("t"),
                },
            },
            FamilyBody {
                name: s("Ребро"),
                material: s("Материал"),
                visible: s("n > 0"),
                array: Some(BodyArray {
                    count: s("n"),
                    step: [
                        s("if(n > 1, (B - 2 * отступ - tr) / (n - 1), 0)"),
                        s("0"),
                        s("0"),
                    ],
                }),
                shape: BodyShape::Plate {
                    normal: Axis::X,
                    at: s("if(n > 1, Лево + отступ, X0 - tr / 2)"),
                    contour: vec![
                        [s("Y0"), s("Низ_столика")],
                        [s("Перед"), s("Низ_столика")],
                        [s("Y0"), s("Низ_рёбер")],
                    ],
                    thickness: s("tr"),
                },
            },
        ],
    };
    let profile = |name: &str, label: &str, value: &str| FamilyParameter {
        name: s(name),
        label: s(label),
        kind: ParamKind::Profile,
        group: s("Профили"),
        of_type: true,
        value: s(value),
        formula: String::new(),
        min: None,
        max: None,
    };
    let ladder = Family {
        name: s("Стремянка"),
        nested: Vec::new(),
        description: s("Две тетивы и ступени с шагом; ставится по двум точкам"),
        placement: Placement::TwoPoints,
        spec: SpecMode::Assembly,
        parameters: vec![
            material,
            profile("Тетива", "Профиль тетивы", "L63×5"),
            profile("Ступень", "Профиль ступени", "L50×5"),
            length("B", "Ширина", true, "600"),
            length("шаг", "Шаг ступеней", false, "300"),
            length("отступ", "Первая ступень от начала", false, "150"),
            FamilyParameter {
                name: s("ступеней"),
                label: s("Число ступеней"),
                kind: ParamKind::Integer,
                group: s("Ступени"),
                of_type: false,
                value: String::new(),
                formula: s("max(floor((L - 2 * отступ) / шаг) + 1, 0)"),
                min: None,
                max: None,
            },
        ],
        types: vec![FamilyType {
            name: s("Ш600"),
            values: BTreeMap::new(),
        }],
        planes: vec![
            RefPlane {
                name: s("Левая"),
                axis: Axis::Y,
                at: s("Y0 + B / 2"),
            },
            RefPlane {
                name: s("Правая"),
                axis: Axis::Y,
                at: s("Y0 - B / 2"),
            },
        ],
        bodies: vec![
            FamilyBody {
                name: s("Тетива"),
                material: s("Материал"),
                visible: String::new(),
                array: Some(BodyArray {
                    count: s("2"),
                    step: [s("0"), s("Правая - Левая"), s("0")],
                }),
                shape: BodyShape::Member {
                    profile: s("Тетива"),
                    start: [s("X0"), s("Левая"), s("Z0")],
                    end: [s("L"), s("Левая"), s("Z0")],
                    rotation: String::new(),
                },
            },
            FamilyBody {
                name: s("Ступень"),
                material: s("Материал"),
                visible: s("ступеней > 0"),
                array: Some(BodyArray {
                    count: s("ступеней"),
                    step: [s("шаг"), s("0"), s("0")],
                }),
                shape: BodyShape::Member {
                    profile: s("Ступень"),
                    start: [s("отступ"), s("Левая"), s("Z0")],
                    end: [s("отступ"), s("Правая"), s("Z0")],
                    rotation: String::new(),
                },
            },
        ],
    };
    vec![seat, ladder]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(name: &str) -> Family {
        sample_families()
            .into_iter()
            .find(|f| f.name == name)
            .unwrap()
    }

    #[test]
    fn samples_are_valid_and_every_type_builds() {
        for family in sample_families() {
            family.validate().unwrap();
            for t in &family.types {
                let values = family
                    .values(&t.name, &BTreeMap::new(), 3000., &BTreeMap::new())
                    .unwrap();
                assert!(!family.evaluate(&values).unwrap().is_empty(), "{}", t.name);
            }
        }
    }

    #[test]
    fn type_values_planes_and_arrays_drive_the_parts() {
        let seat = sample("Столик опорный");
        let values = seat
            .values("С-3", &BTreeMap::new(), 0., &BTreeMap::new())
            .unwrap();
        assert_eq!(values.numbers["Лево"], -125.);
        assert_eq!(values.numbers["Низ_рёбер"], -220.);
        assert_eq!(values.texts["Материал"], "С255");
        let parts = seat.evaluate(&values).unwrap();
        assert_eq!(parts.len(), 3);
        let LocalShape::Plate { frame, .. } = &parts[2].shape else {
            panic!()
        };
        // Second rib: 250 − 2·20 − 12 = 198 further along X.
        assert!((frame.origin.0[0] - (-125. + 20. + 198.)).abs() < 1e-9);
        // No ribs: the rib body is hidden.
        let no_ribs = BTreeMap::from([("n".to_string(), "0".to_string())]);
        let values = seat.values("С-1", &no_ribs, 0., &BTreeMap::new()).unwrap();
        assert_eq!(seat.evaluate(&values).unwrap().len(), 1);
    }

    #[test]
    fn instance_values_respect_kinds_limits_and_formulas() {
        let seat = sample("Столик опорный");
        let get = |k: &str, v: &str| {
            seat.values(
                "С-1",
                &BTreeMap::from([(k.to_string(), v.to_string())]),
                0.,
                &BTreeMap::from([("Пролёт".to_string(), 6000.)]),
            )
        };
        assert!(get("n", "5").unwrap_err().contains("больше"));
        assert!(get("n", "1.5").unwrap_err().contains("целое"));
        assert!(get("B", "100").unwrap_err().contains("типоразмером"));
        assert!(get("нет", "1").unwrap_err().contains("нет параметра"));
        assert_eq!(get("tr", "Пролёт / 500").unwrap().numbers["tr"], 12.);
        assert!(
            seat.values("С-9", &BTreeMap::new(), 0., &BTreeMap::new())
                .is_err()
        );
    }

    #[test]
    fn ladder_rungs_follow_the_length() {
        let ladder = sample("Стремянка");
        let count = |len: f64| {
            let values = ladder
                .values("Ш600", &BTreeMap::new(), len, &BTreeMap::new())
                .unwrap();
            ladder.evaluate(&values).unwrap().len() - 2
        };
        assert_eq!(count(3000.), 10);
        assert_eq!(count(3300.), 11);
        assert_eq!(count(200.), 0);
    }

    #[test]
    fn plates_across_every_axis_keep_right_handed_frames() {
        let points = [[0., 0.], [100., 0.], [100., 50.], [0., 50.]];
        for normal in Axis::ALL {
            let LocalShape::Plate { frame, contour, .. } = plate(normal, 10., &points, 8.).unwrap()
            else {
                panic!()
            };
            frame.validate().unwrap();
            assert_eq!(cross(frame.axes[0], frame.axes[1]), frame.axes[2]);
            assert_eq!(frame.axes[2][normal.index()], 1.);
            assert!(crate::polygon_area(&contour).unwrap() > 0.);
        }
    }

    #[test]
    fn validation_catches_clashing_names_and_bad_type_values() {
        let mut f = sample("Столик опорный");
        f.planes[0].name = "B".into();
        assert!(f.validate().is_err());
        let mut f = sample("Столик опорный");
        f.types[0].values.insert("tr".into(), "10".into());
        assert!(f.validate().is_err());
        let mut f = sample("Столик опорный");
        f.parameters[0].formula = "1".into();
        assert!(f.validate().is_err());
    }

    #[test]
    fn types_round_trip_through_csv() {
        let mut seat = sample("Столик опорный");
        let csv = seat.types_csv();
        assert!(csv.starts_with("Типоразмер;Материал;B;A;H;t\n"), "{csv}");
        let mut copy = seat.clone();
        copy.types.clear();
        copy.read_types_csv(&csv).unwrap();
        assert_eq!(copy.types, seat.types);
        seat.read_types_csv("Типоразмер,B\nС-9,300\n\n\"С;10\",\"350\"\n")
            .unwrap();
        assert_eq!(seat.types.len(), 2);
        assert_eq!(seat.types[1].name, "С;10");
        assert!(seat.read_types_csv("Типоразмер;tr\nX;1").is_err());
        assert!(seat.read_types_csv("Типоразмер;B\nX;1\nX;2").is_err());
    }

    #[test]
    fn a_nested_family_follows_the_host_parameters() {
        let seat = sample("Столик опорный");
        let mut host = sample("Стремянка");
        host.nested.push(seat);
        host.bodies.push(FamilyBody {
            name: "Площадка".into(),
            material: "Материал".into(),
            visible: String::new(),
            array: Some(BodyArray {
                count: "2".into(),
                step: ["L".into(), "0".into(), "0".into()],
            }),
            shape: BodyShape::Nested {
                family: "Столик опорный".into(),
                type_name: "С-1".into(),
                at: ["X0".into(), "Y0".into(), "Z0".into()],
                rotation: "90".into(),
                values: BTreeMap::from([
                    ("B".to_string(), "B".to_string()),
                    ("n".to_string(), "1".to_string()),
                    ("Материал".to_string(), "Материал".to_string()),
                ]),
            },
        });
        host.validate().unwrap();
        let values = host
            .values("Ш600", &BTreeMap::new(), 3000., &BTreeMap::new())
            .unwrap();
        let parts = host.evaluate(&values).unwrap();
        let nested: Vec<&FamilyPart> = parts
            .iter()
            .filter(|p| p.role.starts_with("body-2-"))
            .collect();
        // Two copies of a seat with one rib.
        assert_eq!(nested.len(), 4);
        assert_eq!(nested[0].role, "body-2-0.body-0-0");
        assert_eq!(nested[2].role, "body-2-1.body-0-0");
        let LocalShape::Plate { contour, frame, .. } = &nested[2].shape else {
            panic!()
        };
        // The seat plate takes the ladder width (600) and turns 90° about Z:
        // its width now runs along Y; the second copy stands at L.
        let ys: Vec<f64> = contour
            .iter()
            .map(|p| frame.world(Point3([p[0], p[1], 0.])).0[1])
            .collect();
        let span = ys.iter().copied().fold(f64::MIN, f64::max)
            - ys.iter().copied().fold(f64::MAX, f64::min);
        assert!((span - 600.).abs() < 1e-6, "{span}");
        assert!((frame.origin.0[0] - 3000.).abs() < 1e-6);
        // A wrong nested name or a nesting cycle deeper than allowed fail.
        let mut bad = host.clone();
        if let BodyShape::Nested { family, .. } = &mut bad.bodies[2].shape {
            *family = "Нет".into();
        }
        assert!(bad.validate().is_err());
        let mut deep = sample("Столик опорный");
        for _ in 0..=MAX_NESTING {
            let mut up = sample("Столик опорный");
            up.nested.push(deep);
            deep = up;
        }
        assert!(deep.validate().is_err());
    }

    #[test]
    fn two_points_give_x_along_and_z_up() {
        let (frame, len) =
            FamilyInstance::between(Point3([0., 0., 0.]), Point3([3000., 4000., 0.])).unwrap();
        assert_eq!(len, 5000.);
        assert_eq!(frame.axes[2], [0., 0., 1.]);
        let (frame, _) =
            FamilyInstance::between(Point3([0., 0., 0.]), Point3([0., 0., 3000.])).unwrap();
        frame.validate().unwrap();
    }
}
