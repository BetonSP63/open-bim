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
    BasePlate { overhang: f64 },
    EndPlate { overhang: f64 },
    ClipAngle { leg: f64, height: f64, pitch: f64 },
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
