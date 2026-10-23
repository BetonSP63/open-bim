//! Concrete parts («Бетон»): columns, beams, panels, slabs and footings
//! are placed with the steel tools' picks, with a rectangular section and a
//! concrete grade; the section profile and the grade material are added to
//! the project on first use in the same undo step. «ЖБ элемент» gathers
//! concrete parts into a cast unit.
use super::assembly_edit;
use crate::tools::{Draft, ToolKind, number};
use open_bim_commands::Command;
use open_bim_model::{Element, Material, ObjectId, Profile, ProfileShape, Project, PropertyValue};
use std::collections::BTreeSet;

/// Element property holding the concrete part type.
pub const KEY: &str = "concrete";
/// Assembly property marking a cast unit and its type.
pub const CAST_UNIT: &str = "cast_unit";
pub const GRADES: [&str; 8] = ["В15", "В20", "В25", "В30", "В35", "В40", "В45", "В50"];
pub const DENSITY: f64 = 2500.;

/// Property value of a concrete tool kind.
pub fn code(kind: ToolKind) -> &'static str {
    match kind {
        ToolKind::ConcreteColumn => "column",
        ToolKind::ConcreteBeam => "beam",
        ToolKind::Panel => "panel",
        ToolKind::Slab => "slab",
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
    format!("Бетон {grade}")
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
            d.section = "1500x1500".into();
            d.bottom = "-600".into();
            d.top = "0".into();
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
    let grade = draft.grade.trim();
    if !GRADES.contains(&grade) {
        return Err(format!("Класс бетона: {}", GRADES.join(", ")));
    }
    let mut out = Vec::new();
    let mut next = project.next_id;
    let name = material_name(grade);
    let material = match project.materials.values().find(|m| m.name == name) {
        Some(m) => m.id,
        None => {
            let id = ObjectId(next);
            next += 1;
            out.push(Command::CreateMaterial {
                material: Material {
                    id,
                    name,
                    density_kg_m3: DENSITY,
                },
            });
            id
        }
    };
    let mut d = draft.clone();
    d.kind = kind.base();
    d.material = material;
    d.twin = crate::tools::twin::Twin::Single;
    if kind != ToolKind::Slab {
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
    for c in commands {
        if let Command::Create { element } = c {
            element.properties.remove("fabrication_role");
            element
                .properties
                .insert(KEY.into(), PropertyValue::Text(code(kind).into()));
        }
    }
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
                PropertyValue::Text(
                    if precast {
                        "сборный"
                    } else {
                        "монолитный"
                    }
                    .into(),
                ),
            );
        }
    }
    Ok(out)
}
