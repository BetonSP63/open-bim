//! Saved model views are document data; UI and camera types stay in desktop.
use crate::{Frame, ModelError, ObjectId, Point3};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelViewRender {
    Wireframe,
    Transparent,
    #[default]
    Shaded,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ModelViewKind {
    Model3d,
    Plan { elevation: f64 },
    Elevation { from: Point3, to: Point3 },
    Node { bounds: [[f64; 3]; 2] },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelViewDefinition {
    pub name: String,
    pub kind: ModelViewKind,
    pub yaw: f64,
    pub pitch: f64,
    pub orthographic: bool,
    #[serde(default)]
    pub render: ModelViewRender,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge_pen: Option<crate::LinePen>,
    pub workplane: Frame,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<[f64; 2]>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub hidden: BTreeSet<ObjectId>,
}
impl ModelViewDefinition {
    pub fn validate(&self) -> Result<(), ModelError> {
        let bad = || ModelError::InvalidValue("Некорректный вид модели".into());
        if self.name.trim().is_empty()
            || self.name.len() > 4096
            || self.name.contains('\0')
            || !self.yaw.is_finite()
            || !self.pitch.is_finite()
            || self.pitch.abs() > std::f64::consts::FRAC_PI_2
            || self.hidden.len() > 1_000_000
        {
            return Err(bad());
        }
        self.workplane.validate()?;
        if let Some(pen) = self.edge_pen {
            pen.validate()?;
        }
        if let Some(depth) = self.depth
            && depth
                .iter()
                .any(|v| !v.is_finite() || !(0. ..=1e6).contains(v))
        {
            return Err(bad());
        }
        match &self.kind {
            ModelViewKind::Model3d => {}
            ModelViewKind::Plan { elevation } => {
                crate::validation::valid_point(Point3([0., 0., *elevation]))?
            }
            ModelViewKind::Elevation { from, to } => {
                crate::validation::valid_point(*from)?;
                crate::validation::valid_point(*to)?;
                if (to.0[0] - from.0[0]).hypot(to.0[1] - from.0[1]) < 0.001 {
                    return Err(bad());
                }
            }
            ModelViewKind::Node { bounds: [min, max] } => {
                crate::validation::valid_point(Point3(*min))?;
                crate::validation::valid_point(Point3(*max))?;
                if (0..3).any(|k| max[k] <= min[k]) {
                    return Err(bad());
                }
            }
        }
        Ok(())
    }
}
