//! Custom components («свои компоненты»): plates, bolt groups and welds at a
//! member end, laid out by formulas over named variables. The definition is
//! stored in the component itself, so a model keeps working without the file
//! the definition came from.
//!
//! Coordinates are taken at the connected end of the main part: `x` across
//! the section, `y` up the section, `z` from the end into the part. Built-in
//! names: `h`, `b`, `tw`, `tf`, `L` of the main part, `hs`, `bs` of the
//! support (0 without one), and `t`, `d`, `e` of the template (thickness,
//! bolt diameter, edge distance).
use crate::ModelError;
use crate::formula::{self, MAX_FORMULA};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Names every formula may use without defining them.
pub const BUILTIN_NAMES: [&str; 10] = ["h", "b", "tw", "tf", "L", "hs", "bs", "t", "d", "e"];

pub const MAX_VARIABLES: usize = 64;
pub const MAX_PLATES: usize = 32;
pub const MAX_BOLT_GROUPS: usize = 16;
pub const MAX_BOLTS_PER_GROUP: u32 = 64;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomDefinition {
    pub variables: Vec<Variable>,
    pub plates: Vec<CustomPlate>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bolts: Vec<CustomBolts>,
}

/// A named value: its formula may use the built-in names and the variables
/// above it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Variable {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    pub formula: String,
}

/// Plane of a plate at the member end.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatePlane {
    /// Across the member at `z = offset`; contour in (x, y); the thickness
    /// goes out of the member (towards −z).
    End,
    /// Parallel to the web at `x = offset`; contour in (z, y); the thickness
    /// goes towards +x.
    Web,
    /// Parallel to the flanges at `y = offset`; contour in (x, z); the
    /// thickness goes towards +y.
    Flange,
}

/// Which part a plate is welded to, along the first side of its contour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeldTo {
    Main,
    Support,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomPlate {
    pub name: String,
    pub plane: PlatePlane,
    pub offset: String,
    /// Corner points, each coordinate a formula.
    pub contour: Vec<[String; 2]>,
    pub thickness: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weld: Option<WeldTo>,
}

/// A rectangular bolt group through plate `plate`: the first bolt at
/// (`x`, `y`) in the plate's contour coordinates, `count_x` × `count_y`
/// bolts at `spacing_x`, `spacing_y`. With `through_main` the main part is
/// drilled too.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomBolts {
    pub plate: usize,
    pub x: String,
    pub y: String,
    pub count_x: u32,
    pub count_y: u32,
    pub spacing_x: String,
    pub spacing_y: String,
    #[serde(default)]
    pub through_main: bool,
}

impl CustomBolts {
    pub fn count(&self) -> u32 {
        self.count_x * self.count_y
    }
}

impl CustomDefinition {
    pub fn validate(&self) -> Result<(), ModelError> {
        let bad = |m: String| Err(ModelError::InvalidValue(m));
        if self.plates.is_empty() {
            return bad("В своём компоненте нет ни одной пластины".into());
        }
        if self.variables.len() > MAX_VARIABLES
            || self.plates.len() > MAX_PLATES
            || self.bolts.len() > MAX_BOLT_GROUPS
        {
            return bad(format!(
                "Не больше {MAX_VARIABLES} переменных, {MAX_PLATES} пластин и {MAX_BOLT_GROUPS} групп болтов"
            ));
        }
        let mut names: BTreeSet<&str> = BUILTIN_NAMES.into_iter().collect();
        for v in &self.variables {
            if !formula::valid_name(&v.name) {
                return bad(format!("Имя переменной «{}» недопустимо", v.name));
            }
            if !names.insert(&v.name) {
                return bad(format!("Имя «{}» уже занято", v.name));
            }
            if v.label.len() > 256 {
                return bad("Слишком длинная подпись переменной".into());
            }
        }
        let long = |f: &str| f.chars().count() > MAX_FORMULA;
        for p in &self.plates {
            if p.name.trim().is_empty() || p.name.len() > 256 {
                return bad("У пластины должно быть имя".into());
            }
            if p.contour.len() < 3 || p.contour.len() > 64 {
                return bad(format!("Контур «{}»: от 3 до 64 точек", p.name));
            }
            if long(&p.offset) || long(&p.thickness) || p.contour.iter().flatten().any(|f| long(f))
            {
                return bad(format!("Формула длиннее {MAX_FORMULA} знаков"));
            }
        }
        for g in &self.bolts {
            if g.plate >= self.plates.len() {
                return bad("Группа болтов ссылается на несуществующую пластину".into());
            }
            if g.count_x == 0 || g.count_y == 0 || g.count() > MAX_BOLTS_PER_GROUP {
                return bad(format!(
                    "В группе болтов от 1 до {MAX_BOLTS_PER_GROUP} болтов"
                ));
            }
            if [&g.x, &g.y, &g.spacing_x, &g.spacing_y]
                .into_iter()
                .any(|f| long(f))
            {
                return bad(format!("Формула длиннее {MAX_FORMULA} знаков"));
            }
        }
        Ok(())
    }

    /// Values of the variables in order, on top of `builtins`.
    pub fn values(
        &self,
        builtins: &BTreeMap<String, f64>,
    ) -> Result<BTreeMap<String, f64>, String> {
        let mut values = builtins.clone();
        for v in &self.variables {
            let value =
                formula::eval(&v.formula, &values).map_err(|e| format!("{}: {e}", v.name))?;
            values.insert(v.name.clone(), value);
        }
        Ok(values)
    }

    /// Child roles of a component built from this definition.
    pub fn roles(&self) -> Vec<String> {
        let mut roles: Vec<String> = (0..self.plates.len())
            .map(|i| format!("plate-{i}"))
            .collect();
        for (i, p) in self.plates.iter().enumerate() {
            if p.weld.is_some() {
                roles.push(format!("weld-{i}"));
            }
        }
        for (j, g) in self.bolts.iter().enumerate() {
            roles.push(format!("bolts-{j}"));
            for k in 0..g.count() {
                roles.push(format!("hole-{j}-{k}"));
                if g.through_main {
                    roles.push(format!("main-hole-{j}-{k}"));
                }
            }
        }
        roles
    }
}

/// An example definition: an end plate with four bolts, all sized from the
/// section.
pub fn example_definition() -> CustomDefinition {
    let s = |v: &str| v.to_string();
    CustomDefinition {
        variables: vec![
            Variable {
                name: s("свес"),
                label: s("Свес пластины за сечение"),
                formula: s("e + d"),
            },
            Variable {
                name: s("ширина"),
                label: s("Ширина пластины"),
                formula: s("b + 2 * свес"),
            },
            Variable {
                name: s("высота"),
                label: s("Высота пластины"),
                formula: s("h + 2 * свес"),
            },
        ],
        plates: vec![CustomPlate {
            name: s("Торцевая пластина"),
            plane: PlatePlane::End,
            offset: s("0"),
            contour: vec![
                [s("-ширина / 2"), s("-высота / 2")],
                [s("ширина / 2"), s("-высота / 2")],
                [s("ширина / 2"), s("высота / 2")],
                [s("-ширина / 2"), s("высота / 2")],
            ],
            thickness: s("max(t, d)"),
            weld: Some(WeldTo::Main),
        }],
        bolts: vec![CustomBolts {
            plate: 0,
            x: s("-ширина / 2 + e"),
            y: s("-высота / 2 + e"),
            count_x: 2,
            count_y: 2,
            spacing_x: s("ширина - 2 * e"),
            spacing_y: s("высота - 2 * e"),
            through_main: false,
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_example_is_valid_and_evaluates_on_an_i300() {
        let d = example_definition();
        d.validate().unwrap();
        let builtins = BTreeMap::from(
            [("h", 300.), ("b", 150.), ("d", 20.), ("e", 45.)].map(|(k, v)| (k.to_string(), v)),
        );
        let values = d.values(&builtins).unwrap();
        assert_eq!(values["ширина"], 280.);
        assert_eq!(values["высота"], 430.);
        assert_eq!(d.roles().len(), 1 + 1 + 1 + 4);
    }

    #[test]
    fn names_must_be_new_and_formulas_may_use_only_earlier_ones() {
        let mut d = example_definition();
        d.variables[0].name = "h".into();
        assert!(d.validate().is_err());
        let mut d = example_definition();
        d.variables.swap(0, 1);
        let err = d.values(&BTreeMap::new()).unwrap_err();
        assert!(err.contains("ширина"), "{err}");
        let mut d = example_definition();
        d.bolts[0].plate = 3;
        assert!(d.validate().is_err());
    }
}
