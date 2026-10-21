use crate::{ModelError, ObjectId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionInstance {
    pub id: ObjectId,
    pub primary: ObjectId,
    pub secondary: Option<ObjectId>,
    pub at_end: bool,
    pub template: ConnectionTemplate,
    /// Semantic child keys retain IDs across regeneration. Empty when the
    /// component could not be built after its parts changed («не создан»).
    pub children: BTreeMap<String, ObjectId>,
    #[serde(default, skip_serializing_if = "ComponentGeneral::is_default")]
    pub general: ComponentGeneral,
}

/// «Общие»: settings every component has besides its template.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ComponentGeneral {
    /// Changes to the component's properties are refused while it is set.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub locked: bool,
    /// Class given to every part the component makes (part colour by class).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub class: String,
    /// Connection code shown next to the component symbol.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub code: String,
}

impl ComponentGeneral {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

impl ConnectionInstance {
    pub(crate) fn validate_child_roles(&self, p: &crate::Project) -> Result<(), ModelError> {
        let error =
            || ModelError::InvalidValue("Состав соединения не соответствует шаблону".into());
        if self.children.is_empty() {
            return Ok(());
        }
        if let TemplateKind::FinPlate { rows, .. } = self.template.kind {
            return self.validate_fin_plate(p, rows);
        }
        if let TemplateKind::ColumnStiffeners { .. } = self.template.kind {
            return self.validate_stiffeners(p);
        }
        if let TemplateKind::BeamSplice { rows, .. } = self.template.kind {
            return self.validate_splice(p, rows);
        }
        if let TemplateKind::Haunch { .. } = self.template.kind {
            return self.validate_haunch(p);
        }
        if let TemplateKind::Gusset { bolts, .. } = self.template.kind {
            return self.validate_gusset(p, bolts);
        }
        let roles: &[&str] = match self.template.kind {
            TemplateKind::BasePlate { .. } => &[
                "plate", "hole-0", "hole-1", "hole-2", "hole-3", "bolts", "weld",
            ],
            TemplateKind::EndPlate { .. } => &[
                "plate",
                "hole-0",
                "hole-1",
                "hole-2",
                "hole-3",
                "support-hole-0",
                "support-hole-1",
                "support-hole-2",
                "support-hole-3",
                "bolts",
                "weld",
            ],
            TemplateKind::ClipAngle { .. } => &[
                "angle",
                "angle-profile",
                "angle-hole-0",
                "angle-hole-1",
                "angle-hole-2",
                "angle-hole-3",
                "primary-hole-0",
                "primary-hole-1",
                "support-hole-0",
                "support-hole-1",
                "primary-bolts",
                "support-bolts",
            ],
            TemplateKind::FinPlate { .. }
            | TemplateKind::ColumnStiffeners { .. }
            | TemplateKind::BeamSplice { .. }
            | TemplateKind::Haunch { .. }
            | TemplateKind::Gusset { .. } => unreachable!(),
        };
        if self.children.len() != roles.len()
            || roles.iter().any(|r| !self.children.contains_key(*r))
        {
            return Err(error());
        }
        for (role, id) in &self.children {
            if role.contains("hole-")
                && !matches!(
                    p.details.get(id).map(|d| &d.kind),
                    Some(crate::DetailKind::Hole { .. })
                )
            {
                return Err(error());
            }
        }
        let part = self.children[if matches!(self.template.kind, TemplateKind::ClipAngle { .. }) {
            "angle"
        } else {
            "plate"
        }];
        let element = p
            .elements
            .get(&part)
            .ok_or(ModelError::MissingReference(part))?;
        if let TemplateKind::ClipAngle { leg, height, .. } = self.template.kind {
            let profile = self.children["angle-profile"];
            let expected = crate::ProfileShape::Angle {
                height: leg,
                width: leg,
                thickness: self.template.thickness,
            };
            if p.profiles.get(&profile).is_none_or(|v| v.shape != expected)
                || !matches!(&element.shape,crate::ElementShape::Member {profile:id,length,..} if *id==profile && length.value()==height)
            {
                return Err(error());
            }
        } else if !matches!(element.shape, crate::ElementShape::Plate { .. }) {
            return Err(error());
        }
        let groups: &[&str] = if matches!(self.template.kind, TemplateKind::ClipAngle { .. }) {
            &["primary-bolts", "support-bolts"]
        } else {
            &["bolts"]
        };
        for role in groups {
            let Some(crate::DetailKind::BoltGroup {
                parts,
                diameter,
                clearance,
                standard,
                grade,
                ..
            }) = p.details.get(&self.children[*role]).map(|d| &d.kind)
            else {
                return Err(error());
            };
            let mut expected = std::collections::BTreeSet::from([part]);
            if *role == "primary-bolts" {
                expected.insert(self.primary);
            } else if !matches!(self.template.kind, TemplateKind::BasePlate { .. }) {
                expected.insert(self.secondary.ok_or_else(error)?);
            }
            if *parts != expected
                || *diameter != self.template.bolt_diameter
                || *clearance != self.template.clearance
                || *standard != self.template.bolt_standard
                || *grade != self.template.bolt_grade
            {
                return Err(error());
            }
        }
        if let Some(id) = self.children.get("weld") {
            let Some(crate::DetailKind::Weld { parts, size, .. }) =
                p.details.get(id).map(|d| &d.kind)
            else {
                return Err(error());
            };
            if !parts.contains(&part)
                || !parts.contains(&self.primary)
                || *size != self.template.weld_size
            {
                return Err(error());
            }
        }
        Ok(())
    }
}

impl ConnectionInstance {
    /// Fin plate: the plate, one hole per bolt in the plate and in the web,
    /// the bolt group joining both, and the weld to the support.
    fn validate_fin_plate(&self, p: &crate::Project, rows: u32) -> Result<(), ModelError> {
        let error =
            || ModelError::InvalidValue("Состав соединения не соответствует шаблону".into());
        let mut roles = vec!["plate".to_string(), "bolts".into(), "weld".into()];
        for i in 0..rows {
            roles.push(format!("plate-hole-{i}"));
            roles.push(format!("web-hole-{i}"));
        }
        if self.children.len() != roles.len()
            || roles.iter().any(|r| !self.children.contains_key(r))
        {
            return Err(error());
        }
        let plate = self.children["plate"];
        if !matches!(
            p.elements.get(&plate).map(|e| &e.shape),
            Some(crate::ElementShape::Plate { .. })
        ) {
            return Err(error());
        }
        for (role, id) in &self.children {
            if role.contains("hole-")
                && !matches!(
                    p.details.get(id).map(|d| &d.kind),
                    Some(crate::DetailKind::Hole { .. })
                )
            {
                return Err(error());
            }
        }
        let Some(crate::DetailKind::BoltGroup {
            parts,
            diameter,
            clearance,
            standard,
            grade,
            ..
        }) = p.details.get(&self.children["bolts"]).map(|d| &d.kind)
        else {
            return Err(error());
        };
        if *parts != std::collections::BTreeSet::from([plate, self.primary])
            || *diameter != self.template.bolt_diameter
            || *clearance != self.template.clearance
            || *standard != self.template.bolt_standard
            || *grade != self.template.bolt_grade
        {
            return Err(error());
        }
        let support = self.secondary.ok_or_else(error)?;
        let Some(crate::DetailKind::Weld { parts, size, .. }) =
            p.details.get(&self.children["weld"]).map(|d| &d.kind)
        else {
            return Err(error());
        };
        if !parts.contains(&plate) || !parts.contains(&support) || *size != self.template.weld_size
        {
            return Err(error());
        }
        Ok(())
    }
}

impl ConnectionInstance {
    /// Column stiffeners: four plates, each welded to the column.
    fn validate_stiffeners(&self, p: &crate::Project) -> Result<(), ModelError> {
        let error =
            || ModelError::InvalidValue("Состав соединения не соответствует шаблону".into());
        self.secondary.ok_or_else(error)?;
        if self.children.len() != 8 {
            return Err(error());
        }
        for i in 0..4 {
            let (Some(plate), Some(weld)) = (
                self.children.get(&format!("stiffener-{i}")),
                self.children.get(&format!("weld-{i}")),
            ) else {
                return Err(error());
            };
            if !matches!(
                p.elements.get(plate).map(|e| &e.shape),
                Some(crate::ElementShape::Plate { .. })
            ) {
                return Err(error());
            }
            let Some(crate::DetailKind::Weld { parts, size, .. }) =
                p.details.get(weld).map(|d| &d.kind)
            else {
                return Err(error());
            };
            if *parts != std::collections::BTreeSet::from([*plate, self.primary])
                || *size != self.template.weld_size
            {
                return Err(error());
            }
        }
        Ok(())
    }
}

/// Bolt groups of a beam splice: (role, hole prefix, part tags). Side `a` is
/// on the main beam, side `b` on the second beam.
const SPLICE_GROUPS: [(&str, &str, &[&str]); 6] = [
    ("web-bolts-a", "web-a", &["plate0", "plate1", "beam"]),
    ("web-bolts-b", "web-b", &["plate0", "plate1", "beam"]),
    ("top-bolts-a", "top-a", &["plate", "beam"]),
    ("top-bolts-b", "top-b", &["plate", "beam"]),
    ("bottom-bolts-a", "bottom-a", &["plate", "beam"]),
    ("bottom-bolts-b", "bottom-b", &["plate", "beam"]),
];

/// Child roles of a beam splice with `rows` web bolts in each column: two
/// web plates, two flange plates, six bolt groups and one hole per bolt and
/// part. Web groups hold 2 × `rows` bolts, flange groups 2 × 2.
pub fn beam_splice_roles(rows: u32) -> Vec<String> {
    let mut roles: Vec<String> = [
        "web-plate-0",
        "web-plate-1",
        "flange-plate-top",
        "flange-plate-bottom",
    ]
    .map(String::from)
    .to_vec();
    for (group, prefix, tags) in SPLICE_GROUPS {
        roles.push(group.into());
        let bolts = if prefix.starts_with("web") {
            2 * rows
        } else {
            4
        };
        for tag in tags {
            for k in 0..bolts {
                roles.push(format!("{prefix}-{tag}-{k}"));
            }
        }
    }
    roles
}

impl ConnectionInstance {
    /// Beam splice: plates, holes and the bolt groups joining each plate set
    /// to its beam.
    fn validate_splice(&self, p: &crate::Project, rows: u32) -> Result<(), ModelError> {
        let error =
            || ModelError::InvalidValue("Состав соединения не соответствует шаблону".into());
        let second = self.secondary.ok_or_else(error)?;
        let roles = beam_splice_roles(rows);
        if self.children.len() != roles.len()
            || roles.iter().any(|r| !self.children.contains_key(r))
        {
            return Err(error());
        }
        let plate = |role: &str| -> Result<ObjectId, ModelError> {
            let id = self.children[role];
            match p.elements.get(&id).map(|e| &e.shape) {
                Some(crate::ElementShape::Plate { .. }) => Ok(id),
                _ => Err(error()),
            }
        };
        let web = [plate("web-plate-0")?, plate("web-plate-1")?];
        let top = plate("flange-plate-top")?;
        let bottom = plate("flange-plate-bottom")?;
        for (role, id) in &self.children {
            if role.starts_with("web-plate")
                || role.starts_with("flange-plate")
                || role.contains("-bolts-")
            {
                continue;
            }
            if !matches!(
                p.details.get(id).map(|d| &d.kind),
                Some(crate::DetailKind::Hole { .. })
            ) {
                return Err(error());
            }
        }
        for (group, _, _) in SPLICE_GROUPS {
            let beam = if group.ends_with('a') {
                self.primary
            } else {
                second
            };
            let expected: std::collections::BTreeSet<ObjectId> = match &group[..3] {
                "web" => [web[0], web[1], beam].into(),
                "top" => [top, beam].into(),
                _ => [bottom, beam].into(),
            };
            let Some(crate::DetailKind::BoltGroup {
                parts,
                diameter,
                clearance,
                standard,
                grade,
                ..
            }) = p.details.get(&self.children[group]).map(|d| &d.kind)
            else {
                return Err(error());
            };
            if *parts != expected
                || *diameter != self.template.bolt_diameter
                || *clearance != self.template.clearance
                || *standard != self.template.bolt_standard
                || *grade != self.template.bolt_grade
            {
                return Err(error());
            }
        }
        Ok(())
    }
}

/// Child roles of a haunch.
pub const HAUNCH_ROLES: [&str; 6] = [
    "web-plate",
    "flange-plate",
    "weld-web",
    "weld-flange",
    "weld-tip",
    "weld-support",
];

impl ConnectionInstance {
    /// Haunch: web and flange plates, and the welds joining them to the
    /// beam, to each other and to the support.
    fn validate_haunch(&self, p: &crate::Project) -> Result<(), ModelError> {
        let error =
            || ModelError::InvalidValue("Состав соединения не соответствует шаблону".into());
        let support = self.secondary.ok_or_else(error)?;
        if self.children.len() != HAUNCH_ROLES.len()
            || HAUNCH_ROLES.iter().any(|r| !self.children.contains_key(*r))
        {
            return Err(error());
        }
        let (web, flange) = (self.children["web-plate"], self.children["flange-plate"]);
        for id in [web, flange] {
            if !matches!(
                p.elements.get(&id).map(|e| &e.shape),
                Some(crate::ElementShape::Plate { .. })
            ) {
                return Err(error());
            }
        }
        for (role, expected) in [
            ("weld-web", vec![web, self.primary]),
            ("weld-flange", vec![flange, web]),
            ("weld-tip", vec![flange, self.primary]),
            ("weld-support", vec![support, web, flange]),
        ] {
            let Some(crate::DetailKind::Weld { parts, size, .. }) =
                p.details.get(&self.children[role]).map(|d| &d.kind)
            else {
                return Err(error());
            };
            if *parts != expected.into_iter().collect() || *size != self.template.weld_size {
                return Err(error());
            }
        }
        Ok(())
    }
}

/// Child roles of a brace gusset with `bolts` bolts.
pub fn gusset_roles(bolts: u32) -> Vec<String> {
    let mut roles = vec!["gusset".to_string(), "bolts".into(), "weld".into()];
    for i in 0..bolts {
        roles.push(format!("gusset-hole-{i}"));
        roles.push(format!("brace-hole-{i}"));
    }
    roles
}

impl ConnectionInstance {
    /// Brace gusset: the plate, its holes and the brace holes, the bolt
    /// group joining plate and brace, and the weld to the support.
    fn validate_gusset(&self, p: &crate::Project, bolts: u32) -> Result<(), ModelError> {
        let error =
            || ModelError::InvalidValue("Состав соединения не соответствует шаблону".into());
        let support = self.secondary.ok_or_else(error)?;
        let roles = gusset_roles(bolts);
        if self.children.len() != roles.len()
            || roles.iter().any(|r| !self.children.contains_key(r))
        {
            return Err(error());
        }
        let gusset = self.children["gusset"];
        if !matches!(
            p.elements.get(&gusset).map(|e| &e.shape),
            Some(crate::ElementShape::Plate { .. })
        ) {
            return Err(error());
        }
        for (role, id) in &self.children {
            if role.contains("hole-")
                && !matches!(
                    p.details.get(id).map(|d| &d.kind),
                    Some(crate::DetailKind::Hole { .. })
                )
            {
                return Err(error());
            }
        }
        let Some(crate::DetailKind::BoltGroup {
            parts,
            diameter,
            clearance,
            standard,
            grade,
            ..
        }) = p.details.get(&self.children["bolts"]).map(|d| &d.kind)
        else {
            return Err(error());
        };
        if *parts != std::collections::BTreeSet::from([gusset, self.primary])
            || *diameter != self.template.bolt_diameter
            || *clearance != self.template.clearance
            || *standard != self.template.bolt_standard
            || *grade != self.template.bolt_grade
        {
            return Err(error());
        }
        let Some(crate::DetailKind::Weld { parts, size, .. }) =
            p.details.get(&self.children["weld"]).map(|d| &d.kind)
        else {
            return Err(error());
        };
        if *parts != std::collections::BTreeSet::from([gusset, support])
            || *size != self.template.weld_size
        {
            return Err(error());
        }
        Ok(())
    }
}

/// Dimensions use millimetres. This contract makes no capacity calculation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionTemplate {
    pub version: u32,
    pub name: String,
    pub kind: TemplateKind,
    pub thickness: f64,
    pub edge_distance: f64,
    pub bolt_diameter: f64,
    pub clearance: f64,
    pub bolt_standard: String,
    pub bolt_grade: String,
    pub weld_size: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum TemplateKind {
    BasePlate {
        overhang: f64,
    },
    EndPlate {
        overhang: f64,
    },
    ClipAngle {
        leg: f64,
        height: f64,
        pitch: f64,
    },
    /// Plate welded to the support and bolted to the beam web: plate
    /// height, lap on the beam, gap between beam end and support, bolt
    /// pitch and number of bolts in the row.
    FinPlate {
        height: f64,
        lap: f64,
        gap: f64,
        pitch: f64,
        rows: u32,
    },
    /// Stiffeners in an I-section column at the levels of the beam flanges,
    /// with the web-side corners cut by `chamfer`.
    ColumnStiffeners {
        chamfer: f64,
    },
    /// Bolted splice of two beams end to end: plates on both sides of the
    /// web with two columns of `rows` bolts per beam, and plates outside
    /// both flanges with 2 × 2 bolts per beam; bolt pitch along the beam and
    /// up the web.
    BeamSplice {
        rows: u32,
        pitch: f64,
    },
    /// Welded haunch under the beam end at the support: a triangular web
    /// plate `length` along the beam and `depth` below its bottom flange, and
    /// a flange plate along the sloped edge.
    Haunch {
        length: f64,
        depth: f64,
    },
    /// Gusset plate at the end of an angle brace: bolted to the brace leg
    /// with one row of `bolts` at `pitch` and welded to the support face.
    Gusset {
        bolts: u32,
        pitch: f64,
    },
}

impl ConnectionTemplate {
    pub fn validate(&self) -> Result<(), ModelError> {
        let positive = |v: f64| v.is_finite() && v > 0.001 && v <= 10000.0;
        let invalid =
            || ModelError::InvalidValue("Некорректные параметры шаблона соединения".into());
        if self.version != 1
            || self.name.trim().is_empty()
            || self.name.len() > 1024
            || self.bolt_standard.trim().is_empty()
            || self.bolt_standard.len() > 256
            || self.bolt_grade.trim().is_empty()
            || self.bolt_grade.len() > 256
            || ![
                self.thickness,
                self.edge_distance,
                self.bolt_diameter,
                self.weld_size,
            ]
            .into_iter()
            .all(positive)
            || !self.clearance.is_finite()
            || self.clearance < 0.0
            || self.clearance > 100.0
            || self.edge_distance <= (self.bolt_diameter + self.clearance) / 2.0 + 0.001
        {
            return Err(invalid());
        }
        match self.kind {
            TemplateKind::BasePlate { overhang } | TemplateKind::EndPlate { overhang } => {
                if !positive(overhang) || overhang <= self.edge_distance + self.bolt_diameter / 2.0
                {
                    return Err(invalid());
                }
            }
            TemplateKind::FinPlate {
                height,
                lap,
                gap,
                pitch,
                rows,
            } => {
                let hole = self.bolt_diameter + self.clearance;
                if ![height, lap, pitch].into_iter().all(positive)
                    || !(0.0..=100.0).contains(&gap)
                    || !(1..=12).contains(&rows)
                    || (rows > 1 && pitch <= hole)
                    || height < 2.0 * self.edge_distance + pitch * (rows - 1) as f64
                    || lap < 2.0 * self.edge_distance
                {
                    return Err(invalid());
                }
            }
            TemplateKind::Gusset { bolts, pitch } => {
                if !positive(pitch)
                    || !(1..=12).contains(&bolts)
                    || (bolts > 1 && pitch <= self.bolt_diameter + self.clearance)
                {
                    return Err(invalid());
                }
            }
            TemplateKind::Haunch { length, depth } => {
                if !positive(length) || !positive(depth) {
                    return Err(invalid());
                }
            }
            TemplateKind::BeamSplice { rows, pitch } => {
                if !positive(pitch)
                    || !(1..=12).contains(&rows)
                    || pitch <= self.bolt_diameter + self.clearance
                {
                    return Err(invalid());
                }
            }
            TemplateKind::ColumnStiffeners { chamfer } => {
                if !chamfer.is_finite() || !(0.0..=100.0).contains(&chamfer) {
                    return Err(invalid());
                }
            }
            TemplateKind::ClipAngle { leg, height, pitch } => {
                if ![leg, height, pitch].into_iter().all(positive)
                    || leg <= 2.0 * self.edge_distance + self.thickness
                    || height < 2.0 * self.edge_distance + pitch
                    || pitch <= self.bolt_diameter + self.clearance
                {
                    return Err(invalid());
                }
            }
        }
        Ok(())
    }

    pub fn to_json(&self) -> Result<String, ModelError> {
        self.validate()?;
        serde_json::to_string_pretty(self).map_err(|e| ModelError::InvalidValue(e.to_string()))
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self, ModelError> {
        if bytes.len() > 65536 {
            return Err(ModelError::InvalidValue(
                "Шаблон превышает допустимый размер".into(),
            ));
        }
        let template: Self =
            serde_json::from_slice(bytes).map_err(|e| ModelError::InvalidValue(e.to_string()))?;
        template.validate()?;
        Ok(template)
    }
}
