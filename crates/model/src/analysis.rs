//! Analysis («Анализ»): analysis models built from the physical model, load
//! groups and loads, user nodes, rigid links, merged nodes and per-part
//! analysis properties. The analytical nodes and elements themselves are
//! derived, not stored (see the `analysis` module of the desktop crate).
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::ObjectId;

/// What parts an analysis model takes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Members {
    /// Every member of the model.
    All,
    /// The listed parts only.
    Listed { parts: BTreeSet<ObjectId> },
}

/// «Расчётные и проектные модели»: one analysis model.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalysisModel {
    pub number: u32,
    pub name: String,
    pub members: Members,
    /// Member ends and crossings closer than this are one node, mm.
    pub merge_tolerance: f64,
    /// Load groups taken into this model (empty = all).
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub load_groups: BTreeSet<u32>,
}

/// Kind of action of a load group (СП 20.13330).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoadKind {
    Dead,
    Long,
    Short,
    Snow,
    Wind,
    Crane,
    Special,
}

impl LoadKind {
    pub const ALL: [Self; 7] = [
        Self::Dead,
        Self::Long,
        Self::Short,
        Self::Snow,
        Self::Wind,
        Self::Crane,
        Self::Special,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Dead => "Постоянная",
            Self::Long => "Длительная",
            Self::Short => "Кратковременная",
            Self::Snow => "Снеговая",
            Self::Wind => "Ветровая",
            Self::Crane => "Крановая",
            Self::Special => "Особая",
        }
    }
    /// Usual load safety factor γf of the kind.
    pub fn factor(self) -> f64 {
        match self {
            Self::Dead => 1.05,
            Self::Long | Self::Short | Self::Crane => 1.2,
            Self::Snow | Self::Wind => 1.4,
            Self::Special => 1.0,
        }
    }
}

/// «Группы нагрузок»: loads acting together.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoadGroup {
    pub number: u32,
    pub name: String,
    pub kind: LoadKind,
    /// Load safety factor γf.
    pub factor: f64,
}

/// Where a load acts; forces in kN, kN/m or kN/m² in world axes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LoadShape {
    /// Concentrated force at a point, kN.
    Point { at: [f64; 3], force: [f64; 3] },
    /// Linear load from `a` to `b`, kN/m at each end.
    Line {
        a: [f64; 3],
        b: [f64; 3],
        start: [f64; 3],
        end: [f64; 3],
    },
    /// Uniform area load on a horizontal or sloped polygon, kN/m².
    Area {
        polygon: Vec<[f64; 3]>,
        pressure: [f64; 3],
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Load {
    pub number: u32,
    pub group: u32,
    pub shape: LoadShape,
}

/// «Жёсткая связь» between two points (the nodes nearest to them).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RigidLink {
    pub number: u32,
    pub a: [f64; 3],
    pub b: [f64; 3],
}

/// How a part enters the analysis («Свойства детали в расчётах»).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartRole {
    /// Beam element with the ends as set below.
    #[default]
    Member,
    /// Truss element: hinged at both ends, axial force only.
    Truss,
    /// Left out of the analysis.
    Ignore,
}

impl PartRole {
    pub const ALL: [Self; 3] = [Self::Member, Self::Truss, Self::Ignore];
    pub fn label(self) -> &'static str {
        match self {
            Self::Member => "Стержень",
            Self::Truss => "Ферменный (шарниры)",
            Self::Ignore => "Не учитывать",
        }
    }
}

/// Analysis properties of one part; the default is a rigidly connected
/// member on its axis.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PartAnalysis {
    pub role: PartRole,
    /// Hinge (moment release) at the start and at the end.
    pub hinges: [bool; 2],
    /// Analysis axis moved off the part axis, local y and z, mm.
    pub offset: [f64; 2],
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Analysis {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<AnalysisModel>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub load_groups: Vec<LoadGroup>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub loads: Vec<Load>,
    /// «Узел»: extra nodes at points.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<[f64; 3]>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<RigidLink>,
    /// «Объединить узлы»: nodes near the second point move to the first.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub merges: Vec<[[f64; 3]; 2]>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub parts: BTreeMap<ObjectId, PartAnalysis>,
}

impl Analysis {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn validate(&self) -> Result<(), crate::ModelError> {
        let bad = |m: String| Err(crate::ModelError::InvalidValue(format!("Анализ: {m}")));
        let finite = |p: &[f64; 3]| p.iter().all(|v| v.is_finite() && v.abs() < 1e9);
        let name = |s: &str| !s.trim().is_empty() && s.len() <= 256;
        let unique = |numbers: Vec<u32>| {
            let set: BTreeSet<u32> = numbers.iter().copied().collect();
            set.len() == numbers.len() && !set.contains(&0)
        };
        if !unique(self.models.iter().map(|m| m.number).collect()) {
            return bad("номера расчётных моделей повторяются".into());
        }
        for m in &self.models {
            if !name(&m.name)
                || !(m.merge_tolerance.is_finite() && (0. ..=1000.).contains(&m.merge_tolerance))
            {
                return bad(format!("модель {}: имя или допуск узлов", m.number));
            }
        }
        if !unique(self.load_groups.iter().map(|g| g.number).collect()) {
            return bad("номера групп нагрузок повторяются".into());
        }
        for g in &self.load_groups {
            if !name(&g.name) || !(g.factor.is_finite() && (0.1..=10.).contains(&g.factor)) {
                return bad(format!("группа {}: имя или коэффициент", g.number));
            }
        }
        if !unique(self.loads.iter().map(|l| l.number).collect()) {
            return bad("номера нагрузок повторяются".into());
        }
        let groups: BTreeSet<u32> = self.load_groups.iter().map(|g| g.number).collect();
        for l in &self.loads {
            if !groups.contains(&l.group) {
                return bad(format!("нагрузка {}: нет группы {}", l.number, l.group));
            }
            let ok = match &l.shape {
                LoadShape::Point { at, force } => finite(at) && finite(force),
                LoadShape::Line { a, b, start, end } => {
                    finite(a) && finite(b) && finite(start) && finite(end) && a != b
                }
                LoadShape::Area { polygon, pressure } => {
                    (3..=256).contains(&polygon.len())
                        && polygon.iter().all(finite)
                        && finite(pressure)
                }
            };
            if !ok {
                return bad(format!("нагрузка {}: координаты или величина", l.number));
            }
        }
        if !unique(self.links.iter().map(|l| l.number).collect()) {
            return bad("номера жёстких связей повторяются".into());
        }
        if !self.nodes.iter().all(finite)
            || !self.links.iter().all(|l| finite(&l.a) && finite(&l.b))
            || !self.merges.iter().all(|[a, b]| finite(a) && finite(b))
        {
            return bad("координаты узлов".into());
        }
        if self
            .parts
            .values()
            .any(|p| !p.offset.iter().all(|v| v.is_finite() && v.abs() < 1e5))
        {
            return bad("смещение оси детали".into());
        }
        Ok(())
    }
}
