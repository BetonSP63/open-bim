//! Views of an open drawing: new projection views and sections placed by
//! clicks, their properties, alignment with the main view and deletion.
use super::{DrawingEditor, next_id};
use crate::tools::Editor;
use open_bim_model::{DrawingKind, DrawingView, ViewDirection};

/// Default name of a projection view.
pub fn view_name(direction: ViewDirection) -> &'static str {
    match direction {
        ViewDirection::Front => "Вид спереди",
        ViewDirection::Top => "Вид сверху",
        ViewDirection::Left => "Вид слева",
        ViewDirection::Right => "Вид справа",
        ViewDirection::Bottom => "Вид снизу",
        ViewDirection::Back => "Вид сзади",
    }
}

/// Changes to a view from the properties panel; `None` keeps the value.
#[derive(Clone, Debug, Default)]
pub struct ViewEdit {
    pub name: Option<String>,
    pub direction: Option<ViewDirection>,
    /// `Some(None)` returns the view to the drawing scale.
    pub scale: Option<Option<f64>>,
    pub depth: Option<f64>,
}

impl DrawingEditor {
    /// Object shown by new views of a «Комплексный чертёж»: that of the
    /// selected view, otherwise the first one.
    fn new_view_source(
        &self,
        drawing: &open_bim_model::Drawing,
    ) -> Option<open_bim_model::ObjectId> {
        let DrawingKind::Multi { objects } = &drawing.kind else {
            return None;
        };
        self.selected_view
            .and_then(|id| drawing.views.iter().find(|v| v.id == id))
            .and_then(|v| v.source)
            .or_else(|| objects.first().copied())
    }

    /// New projection view looking from `direction`, centred at `at`.
    pub fn add_view(
        &mut self,
        editor: &mut Editor,
        direction: ViewDirection,
        at: [f64; 2],
    ) -> Result<u32, String> {
        let mut d = self.require(editor.document.project())?;
        if matches!(d.kind, DrawingKind::GeneralData { .. }) {
            return Err("На листе общих данных нет видов".into());
        }
        let id = next_id(&d.views, |v| v.id);
        let source = self.new_view_source(&d);
        d.views.push(DrawingView {
            id,
            name: view_name(direction).into(),
            direction,
            section: None,
            position: at,
            scale: None,
            node: None,
            source,
        });
        d.validate().map_err(|e| e.to_string())?;
        self.selected_view = Some(id);
        self.replace(
            editor,
            &format!("{} (вид {id})", view_name(direction).to_lowercase()),
            d,
        )?;
        self.load_view_fields(editor.document.project());
        Ok(id)
    }

    /// Section through the line `a`–`b` drawn on a view, placed at `place`.
    pub fn add_section(
        &mut self,
        editor: &mut Editor,
        a: [f64; 2],
        b: [f64; 2],
        place: [f64; 2],
    ) -> Result<u32, String> {
        let project = editor.document.project();
        let mut d = self.require(project)?;
        let cut = open_bim_drawings::section_cut(project, &d, a, b, place)?;
        let id = next_id(&d.views, |v| v.id);
        let scale = d
            .views
            .iter()
            .find(|v| v.id == cut.host)
            .and_then(|v| v.scale);
        d.views.push(DrawingView {
            id,
            name: "Разрез".into(),
            direction: cut.direction,
            section: Some(cut.depth),
            position: place,
            scale,
            node: None,
            source: cut.source,
        });
        d.validate().map_err(|e| e.to_string())?;
        let n = d.views.iter().filter(|v| v.section.is_some()).count();
        self.selected_view = Some(id);
        self.replace(editor, &format!("разрез {n}-{n} (вид {id})"), d)?;
        self.load_view_fields(editor.document.project());
        self.message = Some(if cut.snapped {
            format!("Разрез {n}-{n} добавлен; линия выровнена по оси вида")
        } else {
            format!("Разрез {n}-{n} добавлен; «Развернуть» меняет сторону взгляда")
        });
        Ok(id)
    }

    /// Name, direction, scale and section depth of a view.
    pub fn edit_view(
        &mut self,
        editor: &mut Editor,
        id: u32,
        edit: ViewEdit,
    ) -> Result<(), String> {
        let mut d = self.require(editor.document.project())?;
        let view = d
            .views
            .iter_mut()
            .find(|v| v.id == id)
            .ok_or("Вид не найден")?;
        if let Some(name) = edit.name {
            let name = name.trim();
            if name.is_empty() {
                return Err("Введите имя вида".into());
            }
            view.name = name.into();
        }
        if let Some(direction) = edit.direction {
            if view.node.is_some() {
                return Err("Направление узла задаёт его основной вид".into());
            }
            view.direction = direction;
        }
        if let Some(scale) = edit.scale {
            view.scale = scale;
        }
        if let Some(depth) = edit.depth {
            if view.section.is_none() {
                return Err("Глубина задаётся только у разреза".into());
            }
            view.section = Some(depth);
        }
        d.validate().map_err(|e| e.to_string())?;
        self.replace(editor, &format!("вид {id} изменён"), d)?;
        self.load_view_fields(editor.document.project());
        Ok(())
    }

    /// Look at a section from the other side of the same plane.
    pub fn flip_section(&mut self, editor: &mut Editor, id: u32) -> Result<(), String> {
        let mut d = self.require(editor.document.project())?;
        let view = d
            .views
            .iter_mut()
            .find(|v| v.id == id)
            .ok_or("Вид не найден")?;
        let depth = view.section.ok_or("Развернуть можно только разрез")?;
        view.direction = view.direction.opposite();
        view.section = Some(-depth);
        self.replace(editor, &format!("разрез (вид {id}) развёрнут"), d)
    }

    /// Put a view in projection with the main (first) view: a view from
    /// above or below under it, a side view level with it.
    pub fn align_view(&mut self, editor: &mut Editor, id: u32) -> Result<(), String> {
        let mut d = self.require(editor.document.project())?;
        let main = d.views.first().ok_or("Вид не найден")?.position;
        if d.views.first().is_some_and(|v| v.id == id) {
            return Err("Это главный вид: выравнивайте другие виды по нему".into());
        }
        let view = d
            .views
            .iter_mut()
            .find(|v| v.id == id)
            .ok_or("Вид не найден")?;
        match view.direction {
            ViewDirection::Top | ViewDirection::Bottom => view.position[0] = main[0],
            _ => view.position[1] = main[1],
        }
        d.validate().map_err(|e| e.to_string())?;
        self.replace(editor, &format!("вид {id} выровнен по главному"), d)?;
        self.load_view_fields(editor.document.project());
        Ok(())
    }

    /// Delete a view with the dimensions drawn on it; the last view stays.
    pub fn delete_view(&mut self, editor: &mut Editor, id: u32) -> Result<(), String> {
        let mut d = self.require(editor.document.project())?;
        if !d.views.iter().any(|v| v.id == id) {
            return Err("Вид не найден".into());
        }
        if d.views.len() == 1 {
            return Err("Последний вид чертежа удалить нельзя".into());
        }
        d.views.retain(|v| v.id != id);
        let before = d.dimensions.len();
        d.dimensions.retain(|x| x.view != id);
        let dropped = before - d.dimensions.len();
        if self
            .selected_dimension
            .is_some_and(|x| !d.dimensions.iter().any(|y| y.id == x))
        {
            self.selected_dimension = None;
        }
        self.selected_view = d.views.first().map(|v| v.id);
        let label = if dropped > 0 {
            format!("вид {id} удалён вместе с размерами ({dropped})")
        } else {
            format!("вид {id} удалён")
        };
        self.replace(editor, &label, d)?;
        self.load_view_fields(editor.document.project());
        Ok(())
    }

    /// Fill the view fields of the panel from the selected view.
    pub fn load_view_fields(&mut self, project: &open_bim_model::Project) {
        let Some(view) = self
            .selected_view
            .and_then(|id| self.drawing(project)?.views.iter().find(|v| v.id == id))
            .cloned()
        else {
            return;
        };
        self.fields.view = view.position.map(|x| format!("{x:.1}"));
        self.fields.view_name = view.name.clone();
        self.fields.view_depth = view.section.map(|s| format!("{s:.1}")).unwrap_or_default();
    }
}
