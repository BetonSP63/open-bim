//! Concrete parts («Бетон»): columns, beams, panels, slabs and the three
//! kinds of foundation (pad with an optional pedestal, strip, raft) are
//! placed with the steel tools' picks, with a rectangular section and a
//! concrete class of СП 63; the section profile and the class material are
//! added to the project on first use in the same undo step. «ЖБ элемент»
//! gathers concrete parts into a cast-in-place or precast cast unit; a part
//! placed as precast becomes its own precast unit, a pad with a pedestal
//! one unit of both.
use super::assembly_edit;
use crate::tools::{Draft, ToolKind, number};
use open_bim_commands::Command;
use open_bim_model::{Element, ObjectId, Profile, ProfileShape, Project, PropertyValue};
use std::collections::BTreeSet;

/// Element property holding the concrete part type.
pub const KEY: &str = "concrete";
/// Assembly property marking a cast unit and its type.
pub const CAST_UNIT: &str = "cast_unit";
pub const PRECAST: &str = "сборный";
pub const CAST_IN_PLACE: &str = "монолитный";

/// Concrete classes offered by the tools: В15–В60 of the catalogue.
pub fn grades() -> Vec<&'static str> {
    open_bim_model::library::concrete_classes()
        .into_iter()
        .map(|c| c.name)
        .collect()
}

/// Property value of a concrete tool kind.
pub fn code(kind: ToolKind) -> &'static str {
    match kind {
        ToolKind::ConcreteColumn => "column",
        ToolKind::ConcreteBeam => "beam",
        ToolKind::Panel => "panel",
        ToolKind::Slab => "slab",
        ToolKind::StripFooting => "strip_footing",
        ToolKind::Raft => "raft",
        _ => "footing",
    }
}

/// Concrete part type of an element, if it is one.
pub fn part_type(element: &Element) -> Option<&str> {
    match element.properties.get(KEY) {
        Some(PropertyValue::Text(t)) => Some(t),
        _ => None,
    }
}

pub fn material_name(grade: &str) -> String {
    open_bim_model::library::concrete_name(grade)
}

/// Shown name of a concrete part type code.
pub fn type_label(code: &str) -> &'static str {
    match code {
        "column" => "Колонна",
        "beam" => "Балка",
        "panel" => "Панель",
        "slab" => "Перекрытие",
        "strip_footing" => "Ленточный фундамент",
        "raft" => "Фундаментная плита",
        "pedestal" => "Подколонник",
        _ => "Столбчатый фундамент",
    }
}

/// Section «ширина x высота», mm (`x`, `х`, `×` or `*`).
pub fn section(text: &str) -> Result<(f64, f64), String> {
    let t = text.trim().replace(['х', 'Х', '×', '*', 'X'], "x");
    let (b, h) = t
        .split_once('x')
        .ok_or("Сечение: ширина x высота, мм (например 400x600)")?;
    let (b, h) = (number(b)?, number(h)?);
    if !(1.0..=100_000.).contains(&b) || !(1.0..=100_000.).contains(&h) {
        return Err("Сечение: размеры от 1 до 100 000 мм".into());
    }
    Ok((b, h))
}

/// Tool defaults on first use of a concrete kind.
pub fn defaults(kind: ToolKind, d: &mut Draft) {
    use open_bim_model::{AtDepth, PartPosition};
    d.twin = crate::tools::twin::Twin::Single;
    d.position = PartPosition::default();
    d.bottom_level = None;
    d.top_level = None;
    match kind {
        ToolKind::ConcreteColumn => {
            d.section = "400x400".into();
            d.bottom = "0".into();
            d.top = "3000".into();
        }
        ToolKind::Footing => {
            // Sole 1500x1500x300 with a 900x900 pedestal up to -0.150.
            d.section = "1500x1500".into();
            d.bottom = "-1800".into();
            d.top = "-1500".into();
            d.pedestal = "900x900".into();
            d.pedestal_top = "-150".into();
        }
        ToolKind::StripFooting => {
            d.section = "600x500".into();
            // The reference line on the top face.
            d.position.depth = AtDepth::Behind;
        }
        ToolKind::Raft => {
            d.thickness = "400".into();
            d.plate_position = super::polygon::PlatePosition::Down;
        }
        ToolKind::ConcreteBeam => {
            d.section = "300x600".into();
            // The reference line on the top face.
            d.position.depth = AtDepth::Behind;
        }
        ToolKind::Panel => {
            d.section = "200x3000".into();
            // A wall standing on the picked line.
            d.position.depth = AtDepth::Front;
        }
        _ => {
            d.thickness = "200".into();
            d.plate_position = super::polygon::PlatePosition::Down;
        }
    }
}

/// Commands adding the grade material (and the section profile) when the
/// project lacks them, the project with them applied, and the draft of the
/// base steel kind using them.
pub fn prepare(
    project: &Project,
    kind: ToolKind,
    draft: &Draft,
) -> Result<(Vec<Command>, Project, Draft), String> {
    let Some(class) = open_bim_model::library::concrete_class(&draft.grade) else {
        return Err(format!("Класс бетона: {}", grades().join(", ")));
    };
    let mut out = Vec::new();
    let mut next = project.next_id;
    let name = material_name(class.name);
    let material = match project.materials.values().find(|m| m.name == name) {
        Some(m) => m.id,
        None => {
            let id = ObjectId(next);
            next += 1;
            let material = open_bim_model::library::concrete(id, class.name)
                .ok_or("Нет класса бетона в каталоге")?;
            out.push(Command::CreateMaterial { material });
            id
        }
    };
    let mut d = draft.clone();
    d.kind = kind.base();
    d.material = material;
    d.twin = crate::tools::twin::Twin::Single;
    if !matches!(kind, ToolKind::Slab | ToolKind::Raft) {
        let (b, h) = section(&draft.section)?;
        let found = project.profiles.values().find(|p| {
            matches!(p.shape, ProfileShape::Rectangle { height, width }
                if (height - h).abs() < 1e-6 && (width - b).abs() < 1e-6)
        });
        d.profile = match found {
            Some(p) => p.id,
            None => {
                let id = ObjectId(next);
                out.push(Command::CreateProfile {
                    profile: Profile {
                        id,
                        name: format!("{}x{}", fmt(b), fmt(h)),
                        shape: ProfileShape::Rectangle {
                            height: h,
                            width: b,
                        },
                        source: "Бетонное сечение".into(),
                        standard: None,
                        area_mm2: None,
                    },
                });
                id
            }
        };
    }
    let mut staged = project.clone();
    open_bim_commands::apply_staged(&mut staged, &out).map_err(|e| e.to_string())?;
    Ok((out, staged, d))
}

fn fmt(v: f64) -> String {
    open_bim_reports::fmt(v)
}

/// Marks the created parts with the concrete part type.
pub fn mark(commands: &mut [Command], kind: ToolKind) {
    mark_as(commands, code(kind));
}

/// IFC class and predefined type of a concrete part type, when its steel
/// base kind's default (beam, column, plate) does not fit.
pub fn ifc_class(code: &str) -> Option<(&'static str, &'static str)> {
    match code {
        "footing" | "pedestal" => Some(("IfcFooting", "PAD_FOOTING")),
        "strip_footing" => Some(("IfcFooting", "STRIP_FOOTING")),
        "raft" => Some(("IfcSlab", "BASESLAB")),
        "slab" => Some(("IfcSlab", "FLOOR")),
        _ => None,
    }
}

fn mark_as(commands: &mut [Command], code: &str) {
    for c in commands {
        if let Command::Create { element } = c {
            let p = &mut element.properties;
            p.remove("fabrication_role");
            p.insert(KEY.into(), PropertyValue::Text(code.into()));
            if let Some((entity, predefined)) = ifc_class(code) {
                p.insert(
                    open_bim_ifc::IFC_ENTITY_PROPERTY.into(),
                    PropertyValue::Text(entity.into()),
                );
                p.insert(
                    open_bim_ifc::IFC_PREDEFINED_PROPERTY.into(),
                    PropertyValue::Text(predefined.into()),
                );
            }
        }
    }
}

fn created(commands: &[Command]) -> impl Iterator<Item = ObjectId> + '_ {
    commands.iter().filter_map(|c| match c {
        Command::Create { element } => Some(element.id),
        _ => None,
    })
}

/// Commands placing a concrete part: the class material and section, the
/// part built by `base` (the steel kind's geometry from the prepared
/// draft), the pedestal of a pad footing, and the cast unit when the part
/// is precast or has a pedestal.
pub fn build(
    project: &Project,
    kind: ToolKind,
    draft: &Draft,
    base: impl Fn(&Project, &Draft) -> Result<Vec<Command>, String>,
) -> Result<Vec<Command>, String> {
    let (mut out, mut staged, d) = prepare(project, kind, draft)?;
    let mut parts = base(&staged, &d)?;
    mark(&mut parts, kind);
    open_bim_commands::apply_staged(&mut staged, &parts).map_err(|e| e.to_string())?;
    out.extend(parts);
    if kind == ToolKind::Footing && !draft.pedestal.trim().is_empty() {
        let (_, t) = draft.column_ends()?;
        let top = crate::tools::number(&draft.pedestal_top.replace(',', "."))?;
        if top <= t.value {
            return Err("Верх подколонника должен быть выше верха подошвы".into());
        }
        // From the top of the sole up, with the same level binding.
        let mut p = draft.clone();
        p.name = type_label("pedestal").into();
        p.section = draft.pedestal.clone();
        p.bottom = draft.top.clone();
        p.bottom_level = t.level;
        p.top = draft.pedestal_top.clone();
        p.top_level = t.level;
        let (more, mut next, pd) = prepare(&staged, kind, &p)?;
        let mut pedestal = base(&next, &pd)?;
        mark_as(&mut pedestal, "pedestal");
        open_bim_commands::apply_staged(&mut next, &pedestal).map_err(|e| e.to_string())?;
        out.extend(more);
        out.extend(pedestal);
        staged = next;
    }
    let parts: BTreeSet<ObjectId> = created(&out).collect();
    if draft.precast || parts.len() > 1 {
        out.extend(cast_unit(&staged, &parts, draft.precast)?);
    }
    Ok(out)
}

/// «ЖБ элемент»: an assembly (cast unit) of the selected concrete parts;
/// `precast` gives a precast unit, otherwise cast in place.
pub fn cast_unit(
    project: &Project,
    selected: &BTreeSet<ObjectId>,
    precast: bool,
) -> Result<Vec<Command>, String> {
    let parts: BTreeSet<ObjectId> = selected
        .iter()
        .copied()
        .filter(|id| {
            project
                .elements
                .get(id)
                .is_some_and(|e| part_type(e).is_some())
        })
        .collect();
    if parts.is_empty() {
        return Err("Выберите железобетонные детали".into());
    }
    let mut out = assembly_edit::create_from(project, &parts)?;
    for c in &mut out {
        if let Command::CreateAssembly { assembly } = c {
            assembly.properties.insert(
                CAST_UNIT.into(),
                PropertyValue::Text(if precast { PRECAST } else { CAST_IN_PLACE }.into()),
            );
        }
    }
    Ok(out)
}
