//! Saved model views and node views from selected parts.
use super::{
    Mode, Request, Workbench,
    views::{ModelView, ViewKind},
};
use crate::interaction::{Workplane, selection};
use open_bim_commands::Command;
use open_bim_model::{ObjectId, SheetSize};
use open_bim_renderer::DVec3;

impl Workbench {
    pub fn store_model_views(&mut self, label: &str) -> Result<(), String> {
        let definitions: Vec<_> = self.views.iter().map(ModelView::definition).collect();
        if definitions == self.editor.document.project().model_views {
            return Ok(());
        }
        if let Err(error) = self.editor.commit_data(
            label,
            &[Command::SetModelViews {
                views: definitions.clone(),
            }],
        ) {
            self.reload_model_views();
            return Err(error);
        }
        self.stored_model_views = definitions;
        Ok(())
    }

    /// Also called after undo/redo or opening another project.
    pub fn reload_model_views(&mut self) {
        self.stored_model_views = self.editor.document.project().model_views.clone();
        self.views = if self.stored_model_views.is_empty() {
            vec![ModelView::model_3d()]
        } else {
            self.stored_model_views
                .iter()
                .map(ModelView::from_definition)
                .collect()
        };
        self.active_view = self.active_view.min(self.views.len() - 1);
        self.hidden = self.views[self.active_view].hidden.clone();
        self.clip = self.views[self.active_view].clip;
        self.display.mode = self.views[self.active_view].render;
        self.engine.workplane = self.views[self.active_view].workplane;
    }

    pub(super) fn sync_model_views(&mut self) -> Option<Request> {
        if self.stored_model_views == self.editor.document.project().model_views {
            return None;
        }
        self.reload_model_views();
        self.view_tools.reset_edit();
        Some(Request::ApplyView(Box::new(
            self.views[self.active_view].clone(),
        )))
    }

    pub(super) fn store_view_visibility(&mut self) {
        if let Some(view) = self.views.get_mut(self.active_view) {
            view.hidden = self.hidden.clone();
        }
        if let Err(error) = self.store_model_views("Видимость в виде") {
            self.message(error);
        }
    }

    /// The chosen parts and their neighbourhood define a node's extent.
    /// Parts outside that box are hidden in this view only.
    pub fn create_node_view(&mut self) -> Result<Request, String> {
        let project = self.editor.document.project();
        let points: Vec<_> = self
            .editor
            .selected
            .iter()
            .filter(|id| project.elements.contains_key(id))
            .flat_map(|id| selection::corners(project, *id).unwrap_or_default())
            .collect();
        let first = *points.first().ok_or("Выберите детали узла в модели")?;
        let (min, max) = points
            .iter()
            .fold((first, first), |(a, b), p| (a.min(*p), b.max(*p)));
        let margin = ((max - min).max_element() * 0.1).max(100.);
        let (min, max) = (min - DVec3::splat(margin), max + DVec3::splat(margin));
        let mut node = self
            .views
            .get(self.active_view)
            .cloned()
            .unwrap_or_else(ModelView::model_3d);
        node.name = (1..)
            .map(|n| format!("Узел {n}"))
            .find(|name| !self.views.iter().any(|v| v.name == *name))
            .expect("free node name");
        node.kind = ViewKind::Node {
            bounds: [min.to_array(), max.to_array()],
        };
        node.orthographic = true;
        node.workplane = Workplane::facing((min + max) * 0.5, node.towards_viewer())
            .ok_or("Направление вида не задано")?;
        let depth = (max - min).length() * 0.5;
        node.set_depth(depth, depth)?;
        node.hidden = project
            .elements
            .keys()
            .filter(|id| {
                let corners = selection::corners(project, **id).unwrap_or_default();
                if corners.is_empty() {
                    return true;
                }
                let (a, b) = corners
                    .iter()
                    .fold((corners[0], corners[0]), |(a, b), p| (a.min(*p), b.max(*p)));
                (0..3).any(|k| b[k] < min[k] || a[k] > max[k])
            })
            .copied()
            .collect();
        self.views.push(node);
        self.store_model_views("Создать вид узла")?;
        self.view_tools.properties_open = true;
        self.apply_view(self.views.len() - 1)
            .ok_or_else(|| "Вид не найден".into())
    }

    pub fn delete_model_view(&mut self) -> Result<Request, String> {
        if self.views.len() < 2 {
            return Err("Последний вид удалить нельзя".into());
        }
        self.views.remove(self.active_view);
        self.active_view = 0;
        self.store_model_views("Удалить вид модели")?;
        self.view_tools.reset_edit();
        self.apply_view(0).ok_or_else(|| "Вид не найден".into())
    }

    /// Draw the currently visible node parts with editable line settings.
    pub fn create_node_drawing(&mut self) -> Result<(), String> {
        let view = self.views.get(self.active_view).ok_or("Вид не найден")?;
        if !matches!(view.kind, ViewKind::Node { .. }) {
            return Err("Выберите вид узла".into());
        }
        let project = self.editor.document.project();
        let objects: Vec<_> = project
            .elements
            .keys()
            .filter(|id| !self.hidden.contains(id))
            .copied()
            .collect();
        if objects.is_empty() {
            return Err("В узле нет видимых деталей".into());
        }
        let id = ObjectId(project.next_id);
        let mut drawing = open_bim_drawings::general_drawing(
            project,
            id,
            objects,
            &format!("УЗ-{}", id.0),
            SheetSize::A3,
        )?;
        drawing.title = view.name.clone();
        for v in &mut drawing.views {
            v.name = format!("{} — {}", view.name, v.name);
        }
        let drawing = open_bim_drawings::updated(project, &drawing);
        self.editor
            .commit_data("Чертёж узла", &[Command::CreateDrawing { drawing }])?;
        self.drawings.select(self.editor.document.project(), id);
        self.view_tools.properties_open = false;
        self.mode = Mode::Drawings;
        self.message("Чертёж узла создан. Выберите вид и откройте «Линии и видимость»");
        Ok(())
    }
}
