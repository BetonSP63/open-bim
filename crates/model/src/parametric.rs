//! «Параметрическое моделирование»: named model variables given by formulas
//! (`Пролёт = 6000`, `Высота = Пролёт / 2`) and part dimensions bound to
//! formulas over them; changing a variable moves the bound parts.
use serde::{Deserialize, Serialize};

use crate::ObjectId;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Variable {
    pub name: String,
    pub formula: String,
}

/// A part dimension a formula drives: the reference line ends of a member,
/// the origin or the thickness of a plate, mm.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParamField {
    StartX,
    StartY,
    StartZ,
    EndX,
    EndY,
    EndZ,
    OriginX,
    OriginY,
    OriginZ,
    Thickness,
}

impl ParamField {
    pub const MEMBER: [Self; 6] = [
        Self::StartX,
        Self::StartY,
        Self::StartZ,
        Self::EndX,
        Self::EndY,
        Self::EndZ,
    ];
    pub const PLATE: [Self; 4] = [Self::OriginX, Self::OriginY, Self::OriginZ, Self::Thickness];
    pub fn label(self) -> &'static str {
        match self {
            Self::StartX => "Начало X",
            Self::StartY => "Начало Y",
            Self::StartZ => "Начало Z",
            Self::EndX => "Конец X",
            Self::EndY => "Конец Y",
            Self::EndZ => "Конец Z",
            Self::OriginX => "Начало пластины X",
            Self::OriginY => "Начало пластины Y",
            Self::OriginZ => "Начало пластины Z",
            Self::Thickness => "Толщина",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub part: ObjectId,
    pub field: ParamField,
    pub formula: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Parametric {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<Variable>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub bindings: Vec<Binding>,
}

impl Parametric {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Variable values in order; each formula may use the ones above it.
    pub fn values(&self) -> Result<std::collections::BTreeMap<String, f64>, String> {
        let mut values = std::collections::BTreeMap::new();
        for v in &self.variables {
            let value = crate::formula::eval(&v.formula, &values)
                .map_err(|e| format!("{}: {e}", v.name))?;
            values.insert(v.name.clone(), value);
        }
        Ok(values)
    }

    pub fn validate(&self) -> Result<(), crate::ModelError> {
        let bad = |m: String| Err(crate::ModelError::InvalidValue(m));
        let mut names = std::collections::BTreeSet::new();
        for v in &self.variables {
            if !crate::formula::valid_name(&v.name) || !names.insert(v.name.as_str()) {
                return bad(format!(
                    "Параметр «{}»: имя неверно или повторяется",
                    v.name
                ));
            }
            if v.formula.len() > crate::formula::MAX_FORMULA {
                return bad(format!("Параметр «{}»: формула слишком длинная", v.name));
            }
        }
        if self.variables.len() > 1000 || self.bindings.len() > 100_000 {
            return bad("Слишком много параметров или связей".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        for b in &self.bindings {
            if !seen.insert((b.part, b.field)) {
                return bad(format!("Деталь #{}: размер связан дважды", b.part.0));
            }
            if b.formula.len() > crate::formula::MAX_FORMULA {
                return bad(format!("Деталь #{}: формула слишком длинная", b.part.0));
            }
        }
        self.values().map(|_| ()).or_else(bad)
    }
}
