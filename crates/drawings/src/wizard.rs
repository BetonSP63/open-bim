//! Drawing wizard rules: which part and assembly drawings a model needs,
//! one per mark (equal parts share a drawing), optionally cloned from a
//! template drawing. Used by the interface and by batch runs.
use crate::Error;
use open_bim_model::{Drawing, DrawingKind, ElementShape, ObjectId, Project};
use std::collections::{BTreeMap, BTreeSet};

/// Which parts get a part drawing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartRule {
    #[default]
    All,
    /// Plates only (gussets, stiffeners, end plates…).
    Plates,
    /// Every part except the main parts of assemblies.
    Secondary,
}

impl PartRule {
    pub const ALL: [Self; 3] = [Self::All, Self::Plates, Self::Secondary];
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "все детали",
            Self::Plates => "только пластины",
            Self::Secondary => "кроме главных деталей сборок",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// Only the selection (parts and assemblies of selected parts), or the
    /// whole model.
    pub selection_only: bool,
    pub assemblies: bool,
    pub parts: bool,
    pub part_rule: PartRule,
    /// Add «Общие данные» of the КМД set when it is missing.
    pub general_data: bool,
    /// Template drawings to clone the layout from, per kind.
    pub assembly_template: Option<ObjectId>,
    pub part_template: Option<ObjectId>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            selection_only: false,
            assemblies: true,
            parts: true,
            part_rule: PartRule::All,
            general_data: true,
            assembly_template: None,
            part_template: None,
        }
    }
}

/// One drawing the wizard will create.
#[derive(Clone, Debug, PartialEq)]
pub struct Planned {
    pub kind: DrawingKind,
    pub mark: String,
    pub number: String,
}

/// The plan and the marks skipped because they already have a drawing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    pub drawings: Vec<Planned>,
    pub existing: Vec<String>,
    pub general_data: bool,
}

fn is_plate(project: &Project, id: ObjectId) -> bool {
    matches!(
        project.elements.get(&id).map(|e| &e.shape),
        Some(ElementShape::Plate { .. })
    )
}

/// Drawings to create: one per mark of the scope, skipping marks that
/// already have a drawing of that kind.
pub fn plan(
    project: &Project,
    selected: &BTreeSet<ObjectId>,
    settings: &Settings,
) -> Result<Plan, Error> {
    let numbering = open_bim_numbering::number(project).map_err(|e| e.to_string())?;
    let in_scope_part = |id: &ObjectId| !settings.selection_only || selected.contains(id);
    let in_scope_assembly = |id: &ObjectId| {
        !settings.selection_only
            || selected.contains(id)
            || project.assemblies[id]
                .members
                .iter()
                .any(|m| selected.contains(m))
    };
    let main_parts: BTreeSet<ObjectId> = project.assemblies.values().map(|a| a.main_part).collect();
    let mut covered: BTreeSet<(bool, String)> = BTreeSet::new();
    for d in project.drawings.values() {
        match d.kind {
            DrawingKind::Part { part } => {
                if let Some(m) = numbering.parts.get(&part) {
                    covered.insert((false, m.clone()));
                }
            }
            DrawingKind::Assembly { assembly } => {
                if let Some(m) = numbering.assemblies.get(&assembly) {
                    covered.insert((true, m.clone()));
                }
            }
            _ => {}
        }
    }
    let mut out = Plan::default();
    let mut seen: BTreeSet<(bool, String)> = BTreeSet::new();
    let mut add = |assembly: bool, id: ObjectId, mark: &String, out: &mut Plan| {
        let key = (assembly, mark.clone());
        if !seen.insert(key.clone()) {
            return;
        }
        if covered.contains(&key) {
            out.existing.push(mark.clone());
            return;
        }
        out.drawings.push(Planned {
            kind: if assembly {
                DrawingKind::Assembly { assembly: id }
            } else {
                DrawingKind::Part { part: id }
            },
            mark: mark.clone(),
            number: format!("КМД-{mark}"),
        });
    };
    if settings.assemblies {
        for (id, mark) in numbering
            .assemblies
            .iter()
            .filter(|(id, _)| in_scope_assembly(id))
        {
            add(true, *id, mark, &mut out);
        }
    }
    if settings.parts {
        for (id, mark) in numbering.parts.iter().filter(|(id, _)| in_scope_part(id)) {
            let accepted = match settings.part_rule {
                PartRule::All => true,
                PartRule::Plates => is_plate(project, *id),
                PartRule::Secondary => !main_parts.contains(id),
            };
            if accepted {
                add(false, *id, mark, &mut out);
            }
        }
    }
    out.general_data = settings.general_data
        && !project
            .drawings
            .values()
            .any(|d| matches!(&d.kind, DrawingKind::GeneralData { mark } if mark == "КМД"))
        && (!out.drawings.is_empty()
            || project.drawings.values().any(|d| {
                matches!(
                    d.kind,
                    DrawingKind::Part { .. } | DrawingKind::Assembly { .. }
                )
            }));
    Ok(out)
}

/// Clone the layout of `template` onto a generated drawing: sheet, format,
/// scale, view placement and scales (matched by direction) and notes.
/// Dimensions stay the generated ones: they belong to the template's objects.
pub fn clone_layout(drawing: &mut Drawing, template: &Drawing) {
    drawing.sheet = template.sheet;
    drawing.multiple = template.multiple;
    drawing.portrait = template.portrait;
    drawing.scale = template.scale;
    for view in &mut drawing.views {
        if let Some(t) = template
            .views
            .iter()
            .find(|t| t.direction == view.direction)
        {
            view.position = t.position;
            view.scale = t.scale;
        }
    }
    drawing.texts = template.texts.clone();
    drawing.objects = template.objects.clone();
}

/// The planned drawings under IDs from `first_id` on, with the template
/// layouts applied («Общие данные» last, when planned).
pub fn build(
    project: &Project,
    plan: &Plan,
    assembly_template: Option<&Drawing>,
    part_template: Option<&Drawing>,
    first_id: u64,
) -> Result<Vec<Drawing>, Error> {
    let templates: BTreeMap<bool, &Drawing> = [(true, assembly_template), (false, part_template)]
        .into_iter()
        .filter_map(|(k, d)| Some((k, d?)))
        .collect();
    let mut next = first_id;
    let mut out = Vec::new();
    for planned in &plan.drawings {
        let id = ObjectId(next);
        next += 1;
        let (mut drawing, assembly) = match planned.kind {
            DrawingKind::Assembly { assembly } => (
                crate::assembly_drawing(project, id, assembly, &planned.number)?,
                true,
            ),
            DrawingKind::Part { part } => (
                crate::part_drawing(project, id, part, &planned.number)?,
                false,
            ),
            _ => continue,
        };
        if let Some(template) = templates.get(&assembly) {
            clone_layout(&mut drawing, template);
        }
        out.push(drawing);
    }
    if plan.general_data {
        // Its sheet lists are generated from the set, so it can come last.
        out.push(crate::general_data_drawing(project, ObjectId(next), "КМД")?);
    }
    Ok(out)
}

/// Why a plan creates nothing.
pub fn empty_reason(plan: &Plan) -> String {
    if plan.existing.is_empty() {
        "Нет деталей и сборок для чертежей по выбранным правилам".into()
    } else {
        format!(
            "У всех марок уже есть чертежи ({})",
            plan.existing.join(", ")
        )
    }
}
