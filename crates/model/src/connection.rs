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
    /// Semantic child keys retain IDs across regeneration.
    pub children: BTreeMap<String, ObjectId>,
}

impl ConnectionInstance {
    pub(crate) fn validate_child_roles(&self, p: &crate::Project) -> Result<(), ModelError> {
        let error =
            || ModelError::InvalidValue("Состав соединения не соответствует шаблону".into());
        if let TemplateKind::FinPlate { rows, .. } = self.template.kind {
            return self.validate_fin_plate(p, rows);
        }
        if let TemplateKind::ColumnStiffeners { .. } = self.template.kind {
            return self.validate_stiffeners(p);
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
            TemplateKind::FinPlate { .. } | TemplateKind::ColumnStiffeners { .. } => {
                unreachable!()
            }
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
