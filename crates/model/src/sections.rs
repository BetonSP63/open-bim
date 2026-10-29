//! User sections («Пользовательские сечения»): parametric sections drawn as
//! a contour whose corners are formulas over named variables, and welded
//! sections built from plates. A welded section is one member in the model;
//! specifications list its plates and seams separately.
//!
//! Section coordinates: `x` across (to the right), `y` up; the member axis
//! passes through the origin.
use crate::formula::{self, MAX_FORMULA};
use crate::{
    ElementShape, ModelError, ObjectId, Profile, ProfileShape, Project, Variable, WeldProcess,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_VARIABLES: usize = 64;
pub const MAX_POINTS: usize = 256;
pub const MAX_PLATES: usize = 32;

/// Outer loop and voids of a section.
pub type Outline = (Vec<[f64; 2]>, Vec<Vec<[f64; 2]>>);

/// One plate of a welded section: a rectangle `width` (along x) by `height`
/// (along y) centred at `center`. The smaller side is the plate thickness.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionPlate {
    pub name: String,
    pub center: [f64; 2],
    pub width: f64,
    pub height: f64,
}

impl SectionPlate {
    pub fn thickness(&self) -> f64 {
        self.width.min(self.height)
    }
    /// Width of the strip cut from sheet: the larger side.
    pub fn breadth(&self) -> f64 {
        self.width.max(self.height)
    }
    fn rect(&self) -> [f64; 4] {
        let [x, y] = self.center;
        [
            x - self.width / 2.,
            x + self.width / 2.,
            y - self.height / 2.,
            y + self.height / 2.,
        ]
    }
}

/// A longitudinal fillet seam between two plates along the whole member,
/// on one or both sides of the joint.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionWeld {
    pub plates: [usize; 2],
    pub leg: f64,
    pub sides: u8,
    #[serde(default)]
    pub process: WeldProcess,
}

/// Welded section geometry check: plates are valid, do not overlap and form
/// one connected piece; seams join plates that touch.
pub(crate) fn check_welded(
    plates: &[SectionPlate],
    welds: &[SectionWeld],
) -> Result<(), ModelError> {
    welded_outline(plates)?;
    for w in welds {
        let [a, b] = w.plates;
        if a == b || a >= plates.len() || b >= plates.len() {
            return Err(invalid(
                "Шов сварного сечения ссылается на несуществующий лист",
            ));
        }
        if !touch(&plates[a].rect(), &plates[b].rect()) {
            return Err(ModelError::InvalidValue(format!(
                "Шов: листы «{}» и «{}» не касаются",
                plates[a].name, plates[b].name
            )));
        }
        if !(w.leg.is_finite() && w.leg >= 1. && w.leg <= 100.) || !(1..=2).contains(&w.sides) {
            return Err(invalid("Катет шва 1…100 мм, сторон 1 или 2"));
        }
    }
    Ok(())
}

fn invalid(m: &str) -> ModelError {
    ModelError::InvalidValue(m.into())
}

fn touch(a: &[f64; 4], b: &[f64; 4]) -> bool {
    let eps = 1e-6;
    let x = a[0].max(b[0]) - a[1].min(b[1]);
    let y = a[2].max(b[2]) - a[3].min(b[3]);
    // Sharing a piece of an edge: gap zero on one axis, overlap on the other.
    (x.abs() < eps && y < -eps) || (y.abs() < eps && x < -eps)
}

/// Outline of the union of the plates: the outer loop and the voids (a box
/// section has one). Plates must not overlap and must form one piece.
pub(crate) fn welded_outline(
    plates: &[SectionPlate],
) -> Result<Outline, ModelError> {
    if plates.is_empty() || plates.len() > MAX_PLATES {
        return Err(invalid("Сварное сечение: от 1 до 32 листов"));
    }
    for p in plates {
        if !(p.center.iter().all(|v| v.is_finite() && v.abs() <= 1e5)
            && [p.width, p.height]
                .iter()
                .all(|v| v.is_finite() && *v >= 1. && *v <= 1e5))
        {
            return Err(ModelError::InvalidValue(format!(
                "Лист «{}»: размеры от 1 мм",
                p.name
            )));
        }
    }
    let rects: Vec<[f64; 4]> = plates.iter().map(SectionPlate::rect).collect();
    for i in 0..rects.len() {
        for j in 0..i {
            let (a, b) = (&rects[i], &rects[j]);
            if a[0].max(b[0]) < a[1].min(b[1]) - 1e-6 && a[2].max(b[2]) < a[3].min(b[3]) - 1e-6 {
                return Err(ModelError::InvalidValue(format!(
                    "Листы «{}» и «{}» перекрываются",
                    plates[i].name, plates[j].name
                )));
            }
        }
    }
    // Grid of the plate edges; a cell is solid when a plate covers it.
    let axis = |lo: usize, hi: usize| {
        let mut v: Vec<f64> = rects.iter().flat_map(|r| [r[lo], r[hi]]).collect();
        v.sort_by(f64::total_cmp);
        v.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
        v
    };
    let (xs, ys) = (axis(0, 1), axis(2, 3));
    let (nx, ny) = (xs.len() - 1, ys.len() - 1);
    let solid: Vec<Vec<Option<usize>>> = (0..nx)
        .map(|i| {
            (0..ny)
                .map(|j| {
                    let c = [(xs[i] + xs[i + 1]) / 2., (ys[j] + ys[j + 1]) / 2.];
                    rects
                        .iter()
                        .position(|r| c[0] > r[0] && c[0] < r[1] && c[1] > r[2] && c[1] < r[3])
                })
                .collect()
        })
        .collect();
    let filled = |i: isize, j: isize| {
        i >= 0
            && j >= 0
            && (i as usize) < nx
            && (j as usize) < ny
            && solid[i as usize][j as usize].is_some()
    };
    // One piece: every plate reachable from the first through shared edges.
    let mut reached = BTreeSet::new();
    let start = (0..nx)
        .flat_map(|i| (0..ny).map(move |j| (i, j)))
        .find(|&(i, j)| solid[i][j].is_some());
    let mut stack: Vec<(usize, usize)> = start.into_iter().collect();
    let mut seen = BTreeSet::new();
    while let Some((i, j)) = stack.pop() {
        if !seen.insert((i, j)) {
            continue;
        }
        reached.insert(solid[i][j].expect("solid cell"));
        for (di, dj) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            let (a, b) = (i as isize + di, j as isize + dj);
            if filled(a, b) {
                stack.push((a as usize, b as usize));
            }
        }
    }
    if reached.len() != plates.len() {
        return Err(invalid("Листы сварного сечения должны касаться друг друга"));
    }
    // Boundary edges with the solid on the left, as grid vertex indices.
    let mut next: BTreeMap<(usize, usize), Vec<(usize, usize)>> = BTreeMap::new();
    for (i, column) in solid.iter().enumerate() {
        for (j, cell) in column.iter().enumerate() {
            if cell.is_none() {
                continue;
            }
            let (ii, jj) = (i as isize, j as isize);
            if !filled(ii, jj - 1) {
                next.entry((i, j)).or_default().push((i + 1, j));
            }
            if !filled(ii + 1, jj) {
                next.entry((i + 1, j)).or_default().push((i + 1, j + 1));
            }
            if !filled(ii, jj + 1) {
                next.entry((i + 1, j + 1)).or_default().push((i, j + 1));
            }
            if !filled(ii - 1, jj) {
                next.entry((i, j + 1)).or_default().push((i, j));
            }
        }
    }
    if next.values().any(|v| v.len() > 1) {
        return Err(invalid("Листы сварного сечения касаются только углами"));
    }
    let mut loops = Vec::new();
    while let Some((&first, _)) = next.iter().next() {
        let mut ring = Vec::new();
        let mut at = first;
        loop {
            ring.push(at);
            let to = next.remove(&at).and_then(|v| v.into_iter().next());
            match to {
                Some(to) if to == first => break,
                Some(to) => at = to,
                None => return Err(invalid("Контур сварного сечения не замкнут")),
            }
        }
        let points: Vec<[f64; 2]> = ring.iter().map(|&(i, j)| [xs[i], ys[j]]).collect();
        loops.push(simplify(points));
    }
    let signed = |r: &[[f64; 2]]| {
        (0..r.len())
            .map(|i| {
                let (a, b) = (r[i], r[(i + 1) % r.len()]);
                a[0] * b[1] - a[1] * b[0]
            })
            .sum::<f64>()
    };
    let (outer, voids): (Vec<_>, Vec<_>) = loops.into_iter().partition(|r| signed(r) > 0.);
    let [outer] = <[Vec<[f64; 2]>; 1]>::try_from(outer)
        .map_err(|_| invalid("Сварное сечение должно быть одним телом"))?;
    Ok((outer, voids))
}

/// Drops corners lying on a straight run.
fn simplify(points: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    let n = points.len();
    (0..n)
        .filter(|&i| {
            let (a, b, c) = (points[(i + n - 1) % n], points[i], points[(i + 1) % n]);
            ((b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0])).abs() > 1e-9
        })
        .map(|i| points[i])
        .collect()
}

/// One plate of a welded member as the specifications list it.
#[derive(Clone, Debug, PartialEq)]
pub struct WeldedPiece {
    pub name: String,
    pub thickness: f64,
    /// Strip width, mm.
    pub breadth: f64,
    /// Member length, mm.
    pub length: f64,
    /// Share of the member's net mass by plate area, kg.
    pub mass_kg: f64,
}

/// One seam of a welded member along its length.
#[derive(Clone, Debug, PartialEq)]
pub struct WeldedSeam {
    pub leg: f64,
    pub sides: u8,
    pub process: WeldProcess,
    /// Seam length per side, mm.
    pub length: f64,
}

impl Project {
    fn welded(&self, id: ObjectId) -> Option<(&[SectionPlate], &[SectionWeld], f64)> {
        let ElementShape::Member {
            profile, length, ..
        } = &self.elements.get(&id)?.shape
        else {
            return None;
        };
        match &self.profiles.get(profile)?.shape {
            ProfileShape::Welded { plates, welds } => Some((plates, welds, length.value())),
            _ => None,
        }
    }

    /// Plates of a member with a welded section, `None` for other parts.
    /// The member's net mass (holes and cuts removed) is shared by plate area.
    pub fn welded_pieces(&self, id: ObjectId) -> Result<Option<Vec<WeldedPiece>>, ModelError> {
        let Some((plates, _, length)) = self.welded(id) else {
            return Ok(None);
        };
        let mass = self.element_mass_kg(id)?;
        let total: f64 = plates.iter().map(|p| p.width * p.height).sum();
        Ok(Some(
            plates
                .iter()
                .map(|p| WeldedPiece {
                    name: p.name.clone(),
                    thickness: p.thickness(),
                    breadth: p.breadth(),
                    length,
                    mass_kg: mass * p.width * p.height / total,
                })
                .collect(),
        ))
    }

    /// Seams of a member with a welded section (empty for other parts).
    pub fn welded_seams(&self, id: ObjectId) -> Vec<WeldedSeam> {
        let Some((_, welds, length)) = self.welded(id) else {
            return Vec::new();
        };
        welds
            .iter()
            .map(|w| WeldedSeam {
                leg: w.leg,
                sides: w.sides,
                process: w.process,
                length,
            })
            .collect()
    }
}

/// Kind of a user section.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectionKind {
    /// Closed contour with optional voids.
    Contour,
    /// Plates welded together.
    Welded,
}

impl SectionKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Contour => "Параметрические сечения",
            Self::Welded => "Сварные сечения",
        }
    }
}

/// A corner of a contour: two formulas.
pub type PointFormula = [String; 2];

/// One plate of a welded section definition, as formulas.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlateFormula {
    pub name: String,
    pub x: String,
    pub y: String,
    pub width: String,
    pub height: String,
}

/// A seam of a welded section definition: plate numbers (from 1), leg and
/// sides.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeldFormula {
    pub plates: [usize; 2],
    pub leg: String,
    pub sides: u8,
    #[serde(default)]
    pub process: WeldProcess,
}

/// A user section: variables, then either a contour (and voids) or plates
/// (and seams). Each variable's formula may use the variables above it; the
/// values chosen for a profile replace the formulas of the first
/// `parameters` variables.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionDefinition {
    pub name: String,
    pub variables: Vec<Variable>,
    /// How many of the variables are inputs shown when picking a size; the
    /// rest are derived.
    pub parameters: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outer: Vec<PointFormula>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub voids: Vec<Vec<PointFormula>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plates: Vec<PlateFormula>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub welds: Vec<WeldFormula>,
}

impl SectionDefinition {
    pub fn kind(&self) -> SectionKind {
        if self.plates.is_empty() {
            SectionKind::Contour
        } else {
            SectionKind::Welded
        }
    }

    /// Input variables with their default values.
    pub fn defaults(&self) -> Result<BTreeMap<String, f64>, ModelError> {
        let all = self.values(&BTreeMap::new())?;
        Ok(self.variables[..self.parameters.min(self.variables.len())]
            .iter()
            .map(|v| (v.name.clone(), all[&v.name]))
            .collect())
    }

    /// Every variable's value with `inputs` replacing the input formulas.
    pub fn values(
        &self,
        inputs: &BTreeMap<String, f64>,
    ) -> Result<BTreeMap<String, f64>, ModelError> {
        let mut values = BTreeMap::new();
        for (i, v) in self.variables.iter().enumerate() {
            let value = match inputs.get(&v.name) {
                Some(x) if i < self.parameters => *x,
                _ => formula::eval(&v.formula, &values).map_err(|e| {
                    ModelError::InvalidValue(format!("Переменная «{}»: {e}", v.name))
                })?,
            };
            if !value.is_finite() {
                return Err(ModelError::InvalidValue(format!(
                    "Переменная «{}» не число",
                    v.name
                )));
            }
            values.insert(v.name.clone(), value);
        }
        Ok(values)
    }

    /// The section shape for `inputs` (missing inputs take their defaults).
    pub fn shape(&self, inputs: &BTreeMap<String, f64>) -> Result<ProfileShape, ModelError> {
        let values = self.values(inputs)?;
        let eval = |what: &str, src: &str| {
            formula::eval(src, &values)
                .map_err(|e| ModelError::InvalidValue(format!("{what}: {e}")))
        };
        let ring = |what: &str, points: &[PointFormula]| -> Result<Vec<[f64; 2]>, ModelError> {
            points
                .iter()
                .enumerate()
                .map(|(i, [x, y])| {
                    let at = format!("{what}, точка {}", i + 1);
                    Ok([eval(&at, x)?, eval(&at, y)?])
                })
                .collect()
        };
        let shape = match self.kind() {
            SectionKind::Contour => ProfileShape::Polygon {
                outer: ring("Контур", &self.outer)?,
                voids: self
                    .voids
                    .iter()
                    .enumerate()
                    .map(|(i, v)| ring(&format!("Вырез {}", i + 1), v))
                    .collect::<Result<_, _>>()?,
            },
            SectionKind::Welded => ProfileShape::Welded {
                plates: self
                    .plates
                    .iter()
                    .map(|p| {
                        let at = |f: &str| format!("Лист «{}», {f}", p.name);
                        Ok(SectionPlate {
                            name: p.name.clone(),
                            center: [eval(&at("x"), &p.x)?, eval(&at("y"), &p.y)?],
                            width: eval(&at("ширина"), &p.width)?,
                            height: eval(&at("высота"), &p.height)?,
                        })
                    })
                    .collect::<Result<_, ModelError>>()?,
                welds: self
                    .welds
                    .iter()
                    .map(|w| {
                        Ok(SectionWeld {
                            plates: [w.plates[0].wrapping_sub(1), w.plates[1].wrapping_sub(1)],
                            leg: eval("Катет шва", &w.leg)?,
                            sides: w.sides,
                            process: w.process,
                        })
                    })
                    .collect::<Result<_, ModelError>>()?,
            },
        };
        shape.area_mm2()?;
        Ok(shape)
    }

    /// Profile name for `inputs`: the section name and the input values,
    /// «Двутавр сварной 600×250×10×16».
    pub fn profile_name(&self, inputs: &BTreeMap<String, f64>) -> Result<String, ModelError> {
        let values = self.values(inputs)?;
        let sizes: Vec<String> = self.variables[..self.parameters.min(self.variables.len())]
            .iter()
            .map(|v| number(values[&v.name]))
            .collect();
        Ok(if sizes.is_empty() {
            self.name.clone()
        } else {
            format!("{} {}", self.name, sizes.join("×"))
        })
    }

    /// The profile record to store in a project under `id`.
    pub fn profile(
        &self,
        id: ObjectId,
        inputs: &BTreeMap<String, f64>,
    ) -> Result<Profile, ModelError> {
        Ok(Profile {
            id,
            name: self.profile_name(inputs)?,
            shape: self.shape(inputs)?,
            source: format!("Пользовательское сечение «{}»", self.name),
            standard: None,
            area_mm2: None,
        })
    }

    pub fn validate(&self) -> Result<(), ModelError> {
        let bad = |m: String| {
            Err(ModelError::InvalidValue(format!(
                "Сечение «{}»: {m}",
                self.name
            )))
        };
        let name = self.name.trim();
        if name.is_empty() || name.chars().count() > 64 {
            return bad("имя пустое или длиннее 64 знаков".into());
        }
        if self.variables.len() > MAX_VARIABLES || self.parameters > self.variables.len() {
            return bad("неверный список переменных".into());
        }
        let mut names = BTreeSet::new();
        for v in &self.variables {
            if !formula::valid_name(&v.name) || !names.insert(v.name.as_str()) {
                return bad(format!("недопустимое или повторное имя «{}»", v.name));
            }
            if v.formula.chars().count() > MAX_FORMULA || v.label.chars().count() > 128 {
                return bad(format!("слишком длинная формула «{}»", v.name));
            }
        }
        let count =
            self.outer.len() + self.voids.iter().map(Vec::len).sum::<usize>() + self.plates.len();
        if count > MAX_POINTS * 4 || self.voids.len() > 16 || self.welds.len() > MAX_PLATES * 2 {
            return bad("слишком много точек".into());
        }
        if self.plates.is_empty() == self.outer.is_empty() {
            return bad("задайте либо контур, либо листы".into());
        }
        if !self.plates.is_empty() && !self.voids.is_empty() {
            return bad("у сварного сечения нет вырезов".into());
        }
        self.shape(&BTreeMap::new())
            .map(|_| ())
            .or_else(|e| bad(e.to_string()))
    }
}

/// Size in a profile name: whole millimetres or one decimal with a comma.
fn number(v: f64) -> String {
    let r = (v * 10.).round() / 10.;
    if (r - r.round()).abs() < 1e-9 {
        format!("{}", r.round() as i64)
    } else {
        format!("{r:.1}").replace('.', ",")
    }
}

/// User section definitions stored in the project.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Sections {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub definitions: Vec<SectionDefinition>,
}

impl Sections {
    pub fn is_default(&self) -> bool {
        self.definitions.is_empty()
    }

    pub fn get(&self, name: &str) -> Option<&SectionDefinition> {
        self.definitions.iter().find(|d| d.name == name)
    }

    /// The project's definitions, then built-in templates it does not
    /// override, in the order shown in the profile picker.
    pub fn all(&self) -> Vec<SectionDefinition> {
        let mut out = self.definitions.clone();
        out.extend(
            templates()
                .into_iter()
                .filter(|t| self.get(&t.name).is_none()),
        );
        out
    }

    pub fn validate(&self) -> Result<(), ModelError> {
        let mut names = BTreeSet::new();
        if self.definitions.len() > 1000 {
            return Err(invalid("Слишком много пользовательских сечений"));
        }
        for d in &self.definitions {
            if !names.insert(d.name.trim()) {
                return Err(ModelError::InvalidValue(format!(
                    "Сечение «{}» повторяется",
                    d.name
                )));
            }
            d.validate()?;
        }
        Ok(())
    }
}

fn var(name: &str, label: &str, formula: &str) -> Variable {
    Variable {
        name: name.into(),
        label: label.into(),
        formula: formula.into(),
    }
}

fn plate(name: &str, x: &str, y: &str, width: &str, height: &str) -> PlateFormula {
    PlateFormula {
        name: name.into(),
        x: x.into(),
        y: y.into(),
        width: width.into(),
        height: height.into(),
    }
}

fn weld(a: usize, b: usize, leg: &str) -> WeldFormula {
    WeldFormula {
        plates: [a, b],
        leg: leg.into(),
        sides: 2,
        // Belt seams of built-up members: automatic submerged arc welding.
        process: WeldProcess::Submerged,
    }
}

fn pt(x: &str, y: &str) -> PointFormula {
    [x.into(), y.into()]
}

/// Built-in section templates: welded I, box, tee and cross from plates,
/// and contour sections (Z, lipped channel, tee).
pub fn templates() -> Vec<SectionDefinition> {
    let leg = var(
        "k",
        "Катет поясных швов",
        "max(4, round(min(tf, tw) * 0.7))",
    );
    vec![
        SectionDefinition {
            name: "Двутавр сварной".into(),
            variables: vec![
                var("h", "Высота", "600"),
                var("b", "Ширина полок", "250"),
                var("tw", "Толщина стенки", "10"),
                var("tf", "Толщина полок", "16"),
                leg.clone(),
            ],
            parameters: 4,
            outer: vec![],
            voids: vec![],
            plates: vec![
                plate("Полка верхняя", "0", "h/2 - tf/2", "b", "tf"),
                plate("Стенка", "0", "0", "tw", "h - 2*tf"),
                plate("Полка нижняя", "0", "-h/2 + tf/2", "b", "tf"),
            ],
            welds: vec![weld(1, 2, "k"), weld(2, 3, "k")],
        },
        SectionDefinition {
            name: "Двутавр сварной несимметричный".into(),
            variables: vec![
                var("h", "Высота", "800"),
                var("b1", "Ширина верхней полки", "300"),
                var("t1", "Толщина верхней полки", "20"),
                var("b2", "Ширина нижней полки", "250"),
                var("t2", "Толщина нижней полки", "16"),
                var("tw", "Толщина стенки", "10"),
                var("k", "Катет поясных швов", "max(4, round(tw * 0.7))"),
            ],
            parameters: 6,
            outer: vec![],
            voids: vec![],
            plates: vec![
                plate("Полка верхняя", "0", "h/2 - t1/2", "b1", "t1"),
                plate("Стенка", "0", "(t2 - t1)/2", "tw", "h - t1 - t2"),
                plate("Полка нижняя", "0", "-h/2 + t2/2", "b2", "t2"),
            ],
            welds: vec![weld(1, 2, "k"), weld(2, 3, "k")],
        },
        SectionDefinition {
            name: "Коробка сварная".into(),
            variables: vec![
                var("h", "Высота", "500"),
                var("b", "Ширина", "400"),
                var("tw", "Толщина стенок", "12"),
                var("tf", "Толщина полок", "20"),
                var("a", "Расстояние между стенками в свету", "b - 2*tw - 60"),
                leg.clone(),
            ],
            parameters: 4,
            outer: vec![],
            voids: vec![],
            plates: vec![
                plate("Полка верхняя", "0", "h/2 - tf/2", "b", "tf"),
                plate("Стенка левая", "-a/2 - tw/2", "0", "tw", "h - 2*tf"),
                plate("Стенка правая", "a/2 + tw/2", "0", "tw", "h - 2*tf"),
                plate("Полка нижняя", "0", "-h/2 + tf/2", "b", "tf"),
            ],
            welds: vec![
                WeldFormula {
                    sides: 1,
                    ..weld(1, 2, "k")
                },
                WeldFormula {
                    sides: 1,
                    ..weld(1, 3, "k")
                },
                WeldFormula {
                    sides: 1,
                    ..weld(2, 4, "k")
                },
                WeldFormula {
                    sides: 1,
                    ..weld(3, 4, "k")
                },
            ],
        },
        SectionDefinition {
            name: "Тавр сварной".into(),
            variables: vec![
                var("h", "Высота", "300"),
                var("b", "Ширина полки", "200"),
                var("tw", "Толщина стенки", "10"),
                var("tf", "Толщина полки", "14"),
                leg.clone(),
            ],
            parameters: 4,
            outer: vec![],
            voids: vec![],
            plates: vec![
                plate("Полка", "0", "h/2 - tf/2", "b", "tf"),
                plate("Стенка", "0", "-tf/2", "tw", "h - tf"),
            ],
            welds: vec![weld(1, 2, "k")],
        },
        SectionDefinition {
            name: "Крест сварной".into(),
            variables: vec![
                var("h", "Высота", "400"),
                var("b", "Ширина", "400"),
                var("t", "Толщина листов", "16"),
                var("k", "Катет швов", "max(4, round(t * 0.7))"),
            ],
            parameters: 3,
            outer: vec![],
            voids: vec![],
            plates: vec![
                plate("Лист сквозной", "0", "0", "t", "h"),
                plate("Лист левый", "-(b + t)/4", "0", "(b - t)/2", "t"),
                plate("Лист правый", "(b + t)/4", "0", "(b - t)/2", "t"),
            ],
            welds: vec![weld(1, 2, "k"), weld(1, 3, "k")],
        },
        SectionDefinition {
            name: "Z-профиль".into(),
            variables: vec![
                var("h", "Высота", "200"),
                var("b", "Ширина полок", "70"),
                var("t", "Толщина", "3"),
            ],
            parameters: 3,
            outer: vec![
                pt("-t/2", "-h/2"),
                pt("b - t/2", "-h/2"),
                pt("b - t/2", "-h/2 + t"),
                pt("t/2", "-h/2 + t"),
                pt("t/2", "h/2"),
                pt("-b + t/2", "h/2"),
                pt("-b + t/2", "h/2 - t"),
                pt("-t/2", "h/2 - t"),
            ],
            voids: vec![],
            plates: vec![],
            welds: vec![],
        },
        SectionDefinition {
            name: "Швеллер с отгибами".into(),
            variables: vec![
                var("h", "Высота", "200"),
                var("b", "Ширина полок", "70"),
                var("c", "Отгиб", "20"),
                var("t", "Толщина", "2"),
            ],
            parameters: 4,
            outer: vec![
                pt("0", "-h/2"),
                pt("b", "-h/2"),
                pt("b", "-h/2 + c"),
                pt("b - t", "-h/2 + c"),
                pt("b - t", "-h/2 + t"),
                pt("t", "-h/2 + t"),
                pt("t", "h/2 - t"),
                pt("b - t", "h/2 - t"),
                pt("b - t", "h/2 - c"),
                pt("b", "h/2 - c"),
                pt("b", "h/2"),
                pt("0", "h/2"),
            ],
            voids: vec![],
            plates: vec![],
            welds: vec![],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(name: &str, c: [f64; 2], w: f64, h: f64) -> SectionPlate {
        SectionPlate {
            name: name.into(),
            center: c,
            width: w,
            height: h,
        }
    }

    #[test]
    fn welded_i_outline_is_the_i_contour() {
        let plates = [
            rect("top", [0., 290.], 200., 20.),
            rect("web", [0., 0.], 10., 560.),
            rect("bottom", [0., -290.], 200., 20.),
        ];
        let (outer, voids) = welded_outline(&plates).unwrap();
        assert_eq!(outer.len(), 12);
        assert!(voids.is_empty());
        let area = crate::polygon_area(&outer).unwrap();
        assert!((area - (2. * 200. * 20. + 10. * 560.)).abs() < 1e-6);
    }

    #[test]
    fn box_has_one_void_and_bad_layouts_fail() {
        let plates = [
            rect("top", [0., 240.], 400., 20.),
            rect("l", [-150., 0.], 12., 460.),
            rect("r", [150., 0.], 12., 460.),
            rect("bottom", [0., -240.], 400., 20.),
        ];
        let (outer, voids) = welded_outline(&plates).unwrap();
        // Webs set in from the flange edges: the outer loop steps in.
        assert_eq!((outer.len(), voids.len(), voids[0].len()), (12, 1, 4));
        // Overlap, a gap and corner-only contact are rejected.
        let overlap = [
            rect("a", [0., 0.], 100., 10.),
            rect("b", [0., 0.], 10., 100.),
        ];
        assert!(
            welded_outline(&overlap)
                .unwrap_err()
                .to_string()
                .contains("перекрываются")
        );
        let gap = [
            rect("a", [0., 0.], 100., 10.),
            rect("b", [0., 50.], 100., 10.),
        ];
        assert!(
            welded_outline(&gap)
                .unwrap_err()
                .to_string()
                .contains("касаться")
        );
        let corner = [
            rect("a", [0., 0.], 10., 10.),
            rect("b", [10., 10.], 10., 10.),
        ];
        assert!(welded_outline(&corner).is_err());
    }

    #[test]
    fn templates_build_and_name_profiles() {
        for t in templates() {
            t.validate().unwrap_or_else(|e| panic!("{}: {e}", t.name));
            let p = t.profile(ObjectId(99), &BTreeMap::new()).unwrap();
            assert!(p.area_mm2().unwrap() > 0., "{}", t.name);
        }
        let i = &templates()[0];
        let inputs = BTreeMap::from([("h".to_string(), 800.), ("tw".to_string(), 12.)]);
        let p = i.profile(ObjectId(5), &inputs).unwrap();
        assert_eq!(p.name, "Двутавр сварной 800×250×12×16");
        let ProfileShape::Welded { plates, welds } = &p.shape else {
            panic!()
        };
        assert_eq!(plates[1].height, 800. - 32.);
        assert_eq!(welds[0].leg, 8.);
        assert_eq!(welds[0].plates, [0, 1]);
        assert_eq!(welds[0].process, WeldProcess::Submerged);
        // Derived variables are not inputs.
        assert_eq!(i.defaults().unwrap().len(), 4);
    }

    #[test]
    fn contour_sections_use_formulas_and_report_errors() {
        let mut z = templates()
            .into_iter()
            .find(|t| t.name == "Z-профиль")
            .unwrap();
        let shape = z.shape(&BTreeMap::new()).unwrap();
        let area = shape.area_mm2().unwrap();
        assert!((area - (200. * 3. + 2. * (70. - 3.) * 3.)).abs() < 1e-6);
        z.outer[2][0] = "q + 1".into();
        assert!(z.validate().unwrap_err().to_string().contains("«q»"));
    }
}
