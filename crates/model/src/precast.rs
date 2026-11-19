//! Precast cast units and their specification.
use crate::pours::{CAST_UNIT_KEY, PRECAST};
use crate::{Assembly, ModelError, ObjectId, Project, PropertyValue};
use std::collections::{BTreeMap, BTreeSet};

/// Default density for a concrete part without an assigned density, kg/m³.
const DEFAULT_CONCRETE_DENSITY: f64 = 2500.;

#[derive(Clone, Debug, PartialEq)]
pub struct PrecastRow {
    pub mark: String,
    pub designation: String,
    pub name: String,
    pub count: usize,
    pub unit_mass_kg: f64,
    pub unit_volume_m3: f64,
    pub grades: BTreeSet<String>,
}

impl PrecastRow {
    pub fn note(&self) -> String {
        let volume = format!("{:.3}", self.unit_volume_m3).replace('.', ",");
        if self.grades.is_empty() {
            format!("{volume} м³")
        } else {
            let grades = self.grades.iter().cloned().collect::<Vec<_>>().join(", ");
            format!("{grades}, {volume} м³")
        }
    }
}

impl Assembly {
    pub fn is_precast(&self) -> bool {
        matches!(self.properties.get(CAST_UNIT_KEY), Some(PropertyValue::Text(kind)) if kind == PRECAST)
    }
}

impl Project {
    /// Group the supplied precast units by the marks from the numbering engine.
    pub fn precast_rows(
        &self,
        marks: &BTreeMap<ObjectId, String>,
    ) -> Result<Vec<PrecastRow>, ModelError> {
        let mut rows = BTreeMap::<_, PrecastRow>::new();
        for (id, mark) in marks {
            let Some(unit) = self.assemblies.get(id).filter(|unit| unit.is_precast()) else {
                continue;
            };
            if let Some(row) = rows.get_mut(mark) {
                row.count += 1;
                continue;
            }
            let mut grades = BTreeSet::new();
            let mut volume = 0.;
            let mut mass = 0.;
            let mut parts = BTreeSet::new();
            let mut seen = BTreeSet::new();
            let mut pending = vec![unit];
            while let Some(assembly) = pending.pop() {
                if !seen.insert(assembly.id) {
                    continue;
                }
                parts.extend(assembly.members.iter().copied());
                pending.extend(
                    assembly
                        .children
                        .iter()
                        .filter_map(|id| self.assemblies.get(id)),
                );
            }
            for part in parts {
                let element = self
                    .elements
                    .get(&part)
                    .ok_or(ModelError::MissingReference(part))?;
                let part_volume = self.element_volume_mm3(part)? / 1e9;
                let density = self
                    .materials
                    .get(&element.material)
                    .map(|material| material.density_kg_m3)
                    .filter(|density| density.is_finite() && *density > 0.)
                    .unwrap_or(DEFAULT_CONCRETE_DENSITY);
                mass += part_volume * density;
                if self.concrete_type(part).is_some() {
                    volume += part_volume;
                    let grade = self.concrete_grade(part);
                    if !grade.is_empty() {
                        grades.insert(grade);
                    }
                }
            }
            let designation = match unit.properties.get("designation") {
                Some(PropertyValue::Text(text)) => text.clone(),
                _ => String::new(),
            };
            rows.insert(
                mark.clone(),
                PrecastRow {
                    mark: mark.clone(),
                    designation,
                    name: format!("{} {mark}", self.concrete_element_name(unit.main_part)),
                    count: 1,
                    unit_mass_kg: mass,
                    unit_volume_m3: volume,
                    grades,
                },
            );
        }
        Ok(rows.into_values().collect())
    }
}
