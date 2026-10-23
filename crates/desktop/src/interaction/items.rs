//! Items and shapes: «Создать форму из геометрии» saves the selected parts
//! as a named shape, «Преобразовать деталь в элемент» also makes them an
//! item, «Элемент» places a shape from the catalogue; «Добавленный
//! материал» adds a plate of the host's material to a part.
use super::assembly_edit;
use super::workplane::Workplane;
use crate::tools::{Draft, ToolKind};
use eframe::egui;
use open_bim_commands::Command;
use open_bim_model::{
    Element, ElementShape, ItemShape, ObjectId, Project, PropertyValue, ShapePart,
};
use open_bim_renderer::DVec3;
use std::collections::BTreeSet;

/// Assembly property: the shape an item was placed from.
pub const ITEM: &str = "item";
/// Element property: the part an added material belongs to.
pub const ADDED: &str = "added_to";

fn parts_of(project: &Project, selected: &BTreeSet<ObjectId>) -> Result<Vec<ObjectId>, String> {
    let parts: Vec<ObjectId> = selected
        .iter()
        .copied()
        .filter(|id| project.elements.contains_key(id))
        .collect();
    if parts.is_empty() {
        return Err("Выберите детали".into());
    }
    Ok(parts)
}

/// Lowest corner of the parts' bounds: the base point of a shape.
fn base_point(project: &Project, parts: &[ObjectId]) -> Result<[f64; 3], String> {
    let mut lo = [f64::INFINITY; 3];
    for id in parts {
        let b = project.element_bounds(*id).map_err(|e| e.to_string())?;
        lo = std::array::from_fn(|k| lo[k].min(b[0][k]));
    }
    Ok(lo)
}

fn moved(shape: &ElementShape, d: [f64; 3]) -> ElementShape {
    let mut s = shape.clone();
    let o = &mut s.frame_mut().origin.0;
    *o = std::array::from_fn(|k| o[k] + d[k]);
    s
}

fn next_name(project: &Project) -> String {
    (1..)
        .map(|n| format!("Форма {n}"))
        .find(|n| project.shapes.get(n).is_none())
        .unwrap_or_default()
}

/// The selected parts as a new shape relative to their lowest corner.
pub fn shape_from(project: &Project, selected: &BTreeSet<ObjectId>) -> Result<ItemShape, String> {
    let parts = parts_of(project, selected)?;
    let base = base_point(project, &parts)?;
    let back = base.map(|v| -v);
    Ok(ItemShape {
        name: next_name(project),
        parts: parts
            .iter()
            .map(|id| {
                let e = &project.elements[id];
                ShapePart {
                    name: e.name.clone(),
                    material: e.material,
                    shape: moved(&e.shape, back),
                }
            })
            .collect(),
    })
}

/// «Создать форму из геометрии».
pub fn create_shape(
    project: &Project,
    selected: &BTreeSet<ObjectId>,
) -> Result<(Vec<Command>, String), String> {
    let shape = shape_from(project, selected)?;
    let name = shape.name.clone();
    let mut shapes = project.shapes.clone();
    shapes.shapes.push(shape);
    Ok((vec![Command::SetShapes { shapes }], name))
}

fn mark_item(commands: &mut [Command], name: &str) {
    for c in commands {
        if let Command::CreateAssembly { assembly } = c {
            assembly
                .properties
                .insert(ITEM.into(), PropertyValue::Text(name.into()));
        }
    }
}

/// «Преобразовать деталь в элемент»: the selected parts become an item of
/// a new shape made from them.
pub fn convert(
    project: &Project,
    selected: &BTreeSet<ObjectId>,
) -> Result<(Vec<Command>, String), String> {
    let (mut out, name) = create_shape(project, selected)?;
    let parts: BTreeSet<ObjectId> = parts_of(project, selected)?.into_iter().collect();
    let mut asm = assembly_edit::create_from(project, &parts)?;
    mark_item(&mut asm, &name);
    out.extend(asm);
    Ok((out, name))
}

/// «Элемент»: the shape's parts with its base point at `at`, as one item.
pub fn place(project: &Project, name: &str, at: DVec3) -> Result<Vec<Command>, String> {
    let shape = project
        .shapes
        .get(name)
        .ok_or_else(|| format!("Нет формы «{name}»"))?;
    let mut out: Vec<Command> = shape
        .parts
        .iter()
        .zip(project.next_id..)
        .map(|(p, id)| Command::Create {
            element: Element {
                id: ObjectId(id),
                name: p.name.clone(),
                material: p.material,
                shape: moved(&p.shape, at.to_array()),
                properties: Default::default(),
            },
        })
        .collect();
    let mut staged = project.clone();
    open_bim_commands::apply_staged(&mut staged, &out).map_err(|e| e.to_string())?;
    let ids: BTreeSet<ObjectId> = out
        .iter()
        .filter_map(|c| match c {
            Command::Create { element } => Some(element.id),
            _ => None,
        })
        .collect();
    let mut asm = assembly_edit::create_from(&staged, &ids)?;
    mark_item(&mut asm, name);
    out.extend(asm);
    Ok(out)
}

pub fn rename(project: &Project, old: &str, new: &str) -> Result<Vec<Command>, String> {
    let new = new.trim();
    let mut shapes = project.shapes.clone();
    let s = shapes
        .shapes
        .iter_mut()
        .find(|s| s.name == old)
        .ok_or("Нет такой формы")?;
    s.name = new.into();
    shapes.validate().map_err(|e| e.to_string())?;
    Ok(vec![Command::SetShapes { shapes }])
}

pub fn remove(project: &Project, name: &str) -> Result<Vec<Command>, String> {
    let mut shapes = project.shapes.clone();
    shapes.shapes.retain(|s| s.name != name);
    Ok(vec![Command::SetShapes { shapes }])
}

/// «Добавленный материал»: a plate of the host's material, picked as a
/// polygon in the work plane, joined to the host's assembly.
pub fn added_material(
    project: &Project,
    host: ObjectId,
    polygon: &[DVec3],
    workplane: &Workplane,
    draft: &Draft,
) -> Result<Vec<Command>, String> {
    let e = project.elements.get(&host).ok_or("Деталь не найдена")?;
    let mut d = draft.clone();
    d.kind = ToolKind::Plate;
    d.material = e.material;
    d.name = format!("{} (добавлено)", e.name);
    if d.plate_mode != super::polygon::PlateMode::Polygon {
        d.plate_mode = super::polygon::PlateMode::Polygon;
    }
    let mut out = super::commands(project, ToolKind::Plate, polygon, workplane, &d)?;
    let mut id = None;
    for c in &mut out {
        if let Command::Create { element } = c {
            element.properties.remove("fabrication_role");
            element
                .properties
                .insert(ADDED.into(), PropertyValue::Number(host.0 as f64));
            id = Some(element.id);
        }
    }
    let id = id.ok_or("Не удалось построить пластину")?;
    let mut staged = project.clone();
    open_bim_commands::apply_staged(&mut staged, &out).map_err(|e| e.to_string())?;
    out.extend(assembly_edit::add_to(&staged, &[id].into(), host)?);
    Ok(out)
}

/// What the «Формы» window asks for.
pub enum ShapesAction {
    Commit(String, Vec<Command>),
    /// Pick the base point of the current shape.
    Place,
}

#[derive(Default)]
pub struct ShapesWindow {
    pub open: bool,
    /// Shape placed by «Элемент».
    pub current: Option<String>,
    edit: Option<(String, String)>,
}

impl ShapesWindow {
    pub fn show(&mut self, ctx: &egui::Context, project: &Project) -> Option<ShapesAction> {
        if !self.open {
            return None;
        }
        if self
            .current
            .as_ref()
            .is_none_or(|c| project.shapes.get(c).is_none())
        {
            self.current = project.shapes.shapes.first().map(|s| s.name.clone());
        }
        let mut action = None;
        let mut open = true;
        egui::Window::new("Формы")
            .id(egui::Id::new("shapes"))
            .open(&mut open)
            .default_width(360.)
            .show(ctx, |ui| {
                if project.shapes.shapes.is_empty() {
                    ui.label(
                        "Каталог пуст: выберите детали и выполните «Создать форму из геометрии».",
                    );
                }
                for s in &project.shapes.shapes {
                    ui.horizontal(|ui| {
                        if let Some((old, buf)) = &mut self.edit
                            && *old == s.name
                        {
                            ui.text_edit_singleline(buf);
                            if ui.small_button("ОК").clicked() {
                                action = Some(match rename(project, old, buf) {
                                    Ok(c) => ShapesAction::Commit("Переименовать форму".into(), c),
                                    Err(_) => return,
                                });
                                self.current = Some(buf.trim().into());
                                self.edit = None;
                            }
                            return;
                        }
                        let on = self.current.as_deref() == Some(s.name.as_str());
                        if ui
                            .selectable_label(on, format!("{} ({} дет.)", s.name, s.parts.len()))
                            .clicked()
                        {
                            self.current = Some(s.name.clone());
                        }
                        if ui.small_button("Имя").clicked() {
                            self.edit = Some((s.name.clone(), s.name.clone()));
                        }
                        if ui.small_button("Удалить").clicked()
                            && let Ok(c) = remove(project, &s.name)
                        {
                            action = Some(ShapesAction::Commit("Удалить форму".into(), c));
                        }
                    });
                }
                ui.separator();
                if ui
                    .add_enabled(
                        self.current.is_some(),
                        egui::Button::new("Разместить элемент"),
                    )
                    .clicked()
                {
                    action = Some(ShapesAction::Place);
                }
            });
        self.open = open;
        action
    }
}
