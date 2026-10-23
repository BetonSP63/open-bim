//! A placed plugin: what it was run with and what it made. The plugin
//! depends on its inputs: when an input part changes, the host runs the
//! plugin again with the stored inputs and attributes and replaces its
//! outputs.

use crate::{ModelError, ObjectId, Point3, Project};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_PLUGIN_INPUTS: usize = 10;

/// A stored input value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginInput {
    Point(Point3),
    Points(Vec<Point3>),
    Part(ObjectId),
    Parts(Vec<ObjectId>),
}

impl PluginInput {
    pub fn parts(&self) -> &[ObjectId] {
        match self {
            Self::Part(id) => std::slice::from_ref(id),
            Self::Parts(ids) => ids,
            Self::Point(_) | Self::Points(_) => &[],
        }
    }
}

/// A stored attribute value, plain JSON number or text.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PluginValue {
    Number(f64),
    Text(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginInstance {
    pub id: ObjectId,
    /// Plugin name; the host finds the plugin by it.
    pub plugin: String,
    pub inputs: BTreeMap<String, PluginInput>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub attributes: BTreeMap<String, PluginValue>,
    /// Objects the last run created; replaced by the next run.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub outputs: BTreeSet<ObjectId>,
}

impl PluginInstance {
    /// Parts the plugin depends on.
    pub fn input_parts(&self) -> BTreeSet<ObjectId> {
        self.inputs
            .values()
            .flat_map(|i| i.parts().iter().copied())
            .collect()
    }

    pub fn validate(&self, project: &Project) -> Result<(), ModelError> {
        let invalid = |m: String| Err(ModelError::InvalidValue(m));
        if self.plugin.trim().is_empty() || self.plugin.chars().count() > 64 {
            return invalid("Имя плагина: от 1 до 64 символов".into());
        }
        if self.inputs.len() > MAX_PLUGIN_INPUTS {
            return invalid(format!("Плагин: не больше {MAX_PLUGIN_INPUTS} входов"));
        }
        let finite = |p: &Point3| p.0.iter().all(|v| v.is_finite() && v.abs() <= 1e9);
        for input in self.inputs.values() {
            let ok = match input {
                PluginInput::Point(p) => finite(p),
                PluginInput::Points(points) => !points.is_empty() && points.iter().all(finite),
                PluginInput::Part(_) => true,
                PluginInput::Parts(ids) => !ids.is_empty(),
            };
            if !ok {
                return invalid(format!("Плагин «{}»: недопустимый вход", self.plugin));
            }
        }
        for id in self.input_parts() {
            if !project.elements.contains_key(&id) {
                return Err(ModelError::MissingReference(id));
            }
        }
        for value in self.attributes.values() {
            if let PluginValue::Number(n) = value
                && !n.is_finite()
            {
                return invalid(format!("Плагин «{}»: недопустимое число", self.plugin));
            }
        }
        for id in &self.outputs {
            if *id == self.id || !project.contains_id(*id) {
                return Err(ModelError::MissingReference(*id));
            }
        }
        Ok(())
    }
}
