//! Property pane fields kept as part properties: numbering series,
//! deformation and the IFC export class. Each field shows the value shared
//! by the selection or «— разные —»; «Изменить» writes only edited fields.
use super::property_pane::{Shared, VARIES, number_text};
use eframe::egui;
use open_bim_commands::Command;
use open_bim_model::{ObjectId, Project, PropertyValue};
use std::collections::BTreeMap;

/// Deformation keys: twist of the start and end in degrees, camber and
/// shortening in mm. Stored on the part and exported with its properties.
pub const WARP_START: &str = "deformation_warp_start";
pub const WARP_END: &str = "deformation_warp_end";
pub const CAMBER: &str = "deformation_camber";
pub const SHORTENING: &str = "deformation_shortening";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Text,
    Number,
    /// IFC class from the export list; empty = by object type.
    IfcEntity,
    /// Predefined type allowed for the chosen IFC class.
    IfcPredefined,
}

#[derive(Clone, Copy, Debug)]
pub struct Field {
    pub section: &'static str,
    pub label: &'static str,
    pub key: &'static str,
    pub kind: Kind,
    /// Stored on the part's assembly rather than on the part.
    pub assembly: bool,
}

const fn field(section: &'static str, label: &'static str, key: &'static str, kind: Kind) -> Field {
    Field {
        section,
        label,
        key,
        kind,
        assembly: false,
    }
}

/// Buffer key of an assembly field (the property key itself is shared with
/// the part series).
const fn assembly(
    label: &'static str,
    key: &'static str,
    buffer: &'static str,
    kind: Kind,
) -> Field {
    let _ = key;
    Field {
        section: SERIES,
        label,
        key: buffer,
        kind,
        assembly: true,
    }
}

/// Property key a field writes.
fn property_key(f: &Field) -> &'static str {
    match f.key {
        ASSEMBLY_PREFIX => open_bim_numbering::SERIES_PREFIX,
        ASSEMBLY_START => open_bim_numbering::SERIES_START,
        key => key,
    }
}

const ASSEMBLY_PREFIX: &str = "assembly numbering_prefix";
const ASSEMBLY_START: &str = "assembly numbering_start";

/// Assemblies holding any of `parts`.
fn assemblies_of(project: &Project, parts: &[ObjectId]) -> Vec<ObjectId> {
    project
        .assemblies
        .values()
        .filter(|a| a.members.iter().any(|m| parts.contains(m)))
        .map(|a| a.id)
        .collect()
}

pub const SERIES: &str = "Серия нумерации";
pub const DEFORMATION: &str = "Деформация";
pub const IFC: &str = "Экспорт в IFC";

pub const FIELDS: &[Field] = &[
    field(
        SERIES,
        "Префикс деталей",
        open_bim_numbering::SERIES_PREFIX,
        Kind::Text,
    ),
    field(
        SERIES,
        "Начальный номер деталей",
        open_bim_numbering::SERIES_START,
        Kind::Number,
    ),
    assembly(
        "Префикс сборок",
        open_bim_numbering::SERIES_PREFIX,
        ASSEMBLY_PREFIX,
        Kind::Text,
    ),
    assembly(
        "Начальный номер сборок",
        open_bim_numbering::SERIES_START,
        ASSEMBLY_START,
        Kind::Number,
    ),
    field(DEFORMATION, "Закрутка начала, °", WARP_START, Kind::Number),
    field(DEFORMATION, "Закрутка конца, °", WARP_END, Kind::Number),
    field(DEFORMATION, "Выгиб, мм", CAMBER, Kind::Number),
    field(DEFORMATION, "Укорачивание, мм", SHORTENING, Kind::Number),
    field(
        IFC,
        "Объект IFC",
        open_bim_ifc::IFC_ENTITY_PROPERTY,
        Kind::IfcEntity,
    ),
    field(
        IFC,
        "Подтип (IFC4)",
        open_bim_ifc::IFC_PREDEFINED_PROPERTY,
        Kind::IfcPredefined,
    ),
    field(
        IFC,
        "Пользовательский тип",
        open_bim_ifc::IFC_OBJECT_TYPE_PROPERTY,
        Kind::Text,
    ),
];

/// A property value as the pane shows it; empty when not set.
fn shown(value: Option<&PropertyValue>) -> String {
    match value {
        Some(PropertyValue::Text(t)) => t.clone(),
        Some(PropertyValue::Number(n)) => number_text(*n),
        Some(PropertyValue::Boolean(b)) => if *b { "да" } else { "нет" }.into(),
        None => String::new(),
    }
}

/// Values shared by the selected parts, per field key.
pub fn common(project: &Project, parts: &[ObjectId]) -> BTreeMap<&'static str, Shared<String>> {
    let assemblies = assemblies_of(project, parts);
    FIELDS
        .iter()
        .filter_map(|f| {
            let key = property_key(f);
            let values: Vec<String> = if f.assembly {
                assemblies
                    .iter()
                    .filter_map(|id| project.assemblies.get(id))
                    .map(|a| shown(a.properties.get(key)))
                    .collect()
            } else {
                parts
                    .iter()
                    .filter_map(|id| project.elements.get(id))
                    .map(|e| shown(e.properties.get(key)))
                    .collect()
            };
            let first = values.first()?.clone();
            Some((
                f.key,
                if values.iter().all(|v| *v == first) {
                    Shared::Same(first)
                } else {
                    Shared::Varies
                },
            ))
        })
        .collect()
}

/// Edit buffer of the fields.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FieldBuffer {
    pub values: BTreeMap<&'static str, String>,
}

impl FieldBuffer {
    pub fn fill(&mut self, shown: &BTreeMap<&'static str, Shared<String>>) {
        self.values = shown
            .iter()
            .map(|(k, v)| (*k, v.value().cloned().unwrap_or_else(|| VARIES.into())))
            .collect();
    }

    /// Commands for the edited fields on every part; an emptied field
    /// removes the property, an unreadable number is an error.
    pub fn commands(
        &self,
        project: &Project,
        shown: &BTreeMap<&'static str, Shared<String>>,
        parts: &[ObjectId],
    ) -> Result<Vec<Command>, String> {
        let assemblies = assemblies_of(project, parts);
        let mut commands = Vec::new();
        for f in FIELDS {
            let Some(text) = self.values.get(f.key) else {
                continue;
            };
            let was = shown
                .get(f.key)
                .and_then(|s| s.value())
                .map(String::as_str)
                .unwrap_or(VARIES);
            if text == was || text == VARIES {
                continue;
            }
            let text = text.trim();
            let value = if text.is_empty() {
                None
            } else if f.kind == Kind::Number {
                Some(PropertyValue::Number(
                    crate::tools::number(&text.replace(',', "."))
                        .map_err(|_| format!("{}: введите число", f.label))?,
                ))
            } else {
                Some(PropertyValue::Text(text.to_string()))
            };
            let targets = if f.assembly { &assemblies[..] } else { parts };
            commands.extend(targets.iter().map(|id| Command::ChangeProperty {
                id: *id,
                key: property_key(f).into(),
                value: value.clone(),
            }));
        }
        Ok(commands)
    }

    /// The fields of one section as a two-column grid.
    pub fn section(&mut self, ui: &mut egui::Ui, section: &str) {
        let entity = self
            .values
            .get(open_bim_ifc::IFC_ENTITY_PROPERTY)
            .cloned()
            .unwrap_or_default();
        super::property_pane::rows(ui, ("property fields", section), |ui| {
            for f in FIELDS.iter().filter(|f| f.section == section) {
                let label = ui.label(f.label);
                let value = self.values.entry(f.key).or_default();
                match f.kind {
                    Kind::Text | Kind::Number => {
                        ui.add(
                            egui::TextEdit::singleline(value)
                                .desired_width(super::property_pane::field_width(ui)),
                        )
                        .labelled_by(label.id);
                    }
                    Kind::IfcEntity => {
                        let shown = if value.is_empty() {
                            "По типу объекта".to_string()
                        } else {
                            value.clone()
                        };
                        egui::ComboBox::new(("field", f.key), "")
                            .selected_text(shown)
                            .width(super::property_pane::field_width(ui))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(value, String::new(), "По типу объекта");
                                for (e, _) in open_bim_ifc::ENTITIES {
                                    ui.selectable_value(value, e.to_string(), *e);
                                }
                            })
                            .response
                            .widget_info(|| {
                                egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, f.label)
                            });
                    }
                    Kind::IfcPredefined => {
                        let types = open_bim_ifc::predefined_types(&entity).unwrap_or(&[]);
                        let shown = if value.is_empty() {
                            "По умолчанию".to_string()
                        } else {
                            value.clone()
                        };
                        egui::ComboBox::new(("field", f.key), "")
                            .selected_text(shown)
                            .width(super::property_pane::field_width(ui))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(value, String::new(), "По умолчанию");
                                for t in types {
                                    ui.selectable_value(value, t.to_string(), *t);
                                }
                            })
                            .response
                            .widget_info(|| {
                                egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, f.label)
                            });
                    }
                }
                ui.end_row();
            }
        });
    }
}

/// Labels of a section's fields (for the pane search).
pub fn labels(section: &'static str) -> Vec<&'static str> {
    std::iter::once(section)
        .chain(
            FIELDS
                .iter()
                .filter(|f| f.section == section)
                .map(|f| f.label),
        )
        .collect()
}
