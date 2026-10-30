//! Persistent graphics of an individual drawing view.
use crate::{LineType, ModelError, ObjectId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinePen {
    pub line_type: LineType,
    /// Printed width in paper millimetres.
    pub width: f64,
}
impl LinePen {
    pub fn validate(&self) -> Result<(), ModelError> {
        if !self.width.is_finite() || !(0.05..=2.).contains(&self.width) {
            return Err(ModelError::InvalidValue("Толщина линии: 0,05…2 мм".into()));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ViewAppearance {
    pub visible: LinePen,
    pub hidden: LinePen,
    pub section: LinePen,
    pub show_hidden: bool,
    pub hidden_objects: BTreeSet<ObjectId>,
    /// An object's pen overrides visible and concealed edges in this view.
    pub objects: BTreeMap<ObjectId, LinePen>,
}
impl Default for ViewAppearance {
    fn default() -> Self {
        Self {
            visible: LinePen {
                line_type: LineType::Solid,
                width: 0.5,
            },
            hidden: LinePen {
                line_type: LineType::Dashed,
                width: 0.25,
            },
            section: LinePen {
                line_type: LineType::Solid,
                width: 0.7,
            },
            show_hidden: true,
            hidden_objects: BTreeSet::new(),
            objects: BTreeMap::new(),
        }
    }
}
impl ViewAppearance {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
    pub fn validate(&self) -> Result<(), ModelError> {
        if self.hidden_objects.len() > 1_000_000 || self.objects.len() > 1_000_000 {
            return Err(ModelError::InvalidValue(
                "Слишком много настроек вида".into(),
            ));
        }
        self.visible.validate()?;
        self.hidden.validate()?;
        self.section.validate()?;
        for pen in self.objects.values() {
            pen.validate()?;
        }
        Ok(())
    }
}
