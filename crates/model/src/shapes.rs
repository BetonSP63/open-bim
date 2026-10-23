//! Shape catalogue («Формы»): named groups of part geometry saved from the
//! model and placed again as items («Элемент»).
use serde::{Deserialize, Serialize};

use crate::{ElementShape, ModelError, ObjectId};

/// One part of a shape, placed relative to the shape's base point.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShapePart {
    pub name: String,
    pub material: ObjectId,
    pub shape: ElementShape,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemShape {
    pub name: String,
    pub parts: Vec<ShapePart>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Shapes {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub shapes: Vec<ItemShape>,
}

impl Shapes {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn get(&self, name: &str) -> Option<&ItemShape> {
        self.shapes.iter().find(|s| s.name == name)
    }

    pub fn validate(&self) -> Result<(), ModelError> {
        let bad = |m: String| Err(ModelError::InvalidValue(m));
        let mut names = std::collections::BTreeSet::new();
        for s in &self.shapes {
            let name = s.name.trim();
            if name.is_empty() || name.len() > 128 || !names.insert(name) {
                return bad(format!("Форма «{}»: имя пустое или повторяется", s.name));
            }
            if s.parts.is_empty() || s.parts.len() > 10_000 {
                return bad(format!("Форма «{}»: нет деталей", s.name));
            }
            for p in &s.parts {
                p.shape.frame().validate()?;
            }
        }
        if self.shapes.len() > 10_000 {
            return bad("Слишком много форм".into());
        }
        Ok(())
    }
}
