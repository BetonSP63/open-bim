//! Column ends while a column is being created: each end is a level plus an
//! offset, or an absolute Z. The stored binding is `Project::level_ends`;
//! `ReplaceLevel` moves those ends.
use open_bim_model::{ObjectId, Project};

/// One end of a column: an offset from a level, or an absolute Z (mm).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Height {
    pub level: Option<ObjectId>,
    pub value: f64,
}

impl Height {
    pub fn z(&self, project: &Project) -> Result<f64, String> {
        match self.level {
            None => Ok(self.value),
            Some(id) => project
                .levels
                .get(&id)
                .map(|l| l.elevation_mm + self.value)
                .ok_or_else(|| "Уровень колонны удалён, выберите другой".into()),
        }
    }
}

/// Bottom and top Z of a new column; the top must be above the bottom.
pub fn heights(project: &Project, bottom: Height, top: Height) -> Result<(f64, f64), String> {
    let (b, t) = (bottom.z(project)?, top.z(project)?);
    if t <= b {
        return Err("Верх колонны должен быть выше низа".into());
    }
    Ok((b, t))
}
