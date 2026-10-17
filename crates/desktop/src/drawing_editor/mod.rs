//! Drawing editor: every change is a `ReplaceDrawing` transaction, so undo,
//! redo, saving and the revision journal work exactly as for model edits.
mod panel;
mod preview;

use crate::tools::Editor;
use open_bim_commands::Command;
use open_bim_drawings::{Sheet, Status};
use open_bim_model::{
    Anchor, AnchorPoint, DimensionAxis, DimensionDef, Drawing, DrawingKind, ElementShape, ObjectId,
    Project, SheetSize, TextNote,
};
use std::path::Path;

pub use preview::PreviewState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Pdf,
    Svg,
    Dxf,
}
impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            Self::Svg => "svg",
            Self::Dxf => "dxf",
        }
    }
}

#[derive(Default)]
pub struct DrawingEditor {
    pub current: Option<ObjectId>,
    pub selected_view: Option<u32>,
    pub selected_dimension: Option<u32>,
    pub selected_text: Option<u32>,
    pub message: Option<String>,
    pub export_dir: String,
    pub fields: panel::Fields,
    pub preview: PreviewState,
    cache: Option<(Drawing, u64, Sheet)>,
}

fn next_id<T>(items: &[T], id: impl Fn(&T) -> u32) -> u32 {
    items.iter().map(id).max().unwrap_or(0) + 1
}

impl DrawingEditor {
    pub fn new() -> Self {
        Self {
            export_dir: "drawings".into(),
            ..Default::default()
        }
    }

    pub fn drawing<'a>(&self, project: &'a Project) -> Option<&'a Drawing> {
        self.current.and_then(|id| project.drawings.get(&id))
    }

    fn require(&self, project: &Project) -> Result<Drawing, String> {
        self.drawing(project)
            .cloned()
            .ok_or_else(|| "Выберите чертёж".into())
    }

    fn replace(
        &mut self,
        editor: &mut Editor,
        label: &str,
        drawing: Drawing,
    ) -> Result<(), String> {
        editor.commit_data(
            &format!("Чертёж {}: {label}", drawing.number),
            &[Command::ReplaceDrawing { drawing }],
        )?;
        self.message = Some(format!(
            "{label} · ревизия {}",
            editor.document.project().revision
        ));
        Ok(())
    }

    fn create(&mut self, editor: &mut Editor, drawing: Drawing) -> Result<(), String> {
        let id = drawing.id;
        editor.commit_data(
            &format!("Создать чертёж {}", drawing.number),
            &[Command::CreateDrawing { drawing }],
        )?;
        self.select(editor.document.project(), id);
        Ok(())
    }

    pub fn select(&mut self, project: &Project, id: ObjectId) {
        self.current = project.drawings.contains_key(&id).then_some(id);
        self.selected_view = self
            .drawing(project)
            .and_then(|d| d.views.first().map(|v| v.id));
        self.selected_dimension = None;
        self.selected_text = None;
        self.fields.load(self.drawing(project));
    }

    /// Part drawing of the first selected part (or the first part).
    pub fn create_part(&mut self, editor: &mut Editor) -> Result<(), String> {
        let project = editor.document.project();
        let part = editor
            .selected
            .iter()
            .copied()
            .find(|id| project.elements.contains_key(id))
            .ok_or("Выберите деталь в модели")?;
        let id = ObjectId(project.next_id);
        let number = format!("КМД-{}", project.drawings.len() + 1);
        let drawing = open_bim_drawings::part_drawing(project, id, part, &number)?;
        self.create(editor, drawing)
    }

    /// Assembly drawing of the assembly owning the selection (or the first one).
    pub fn create_assembly(&mut self, editor: &mut Editor) -> Result<(), String> {
        let project = editor.document.project();
        let assembly = project
            .assemblies
            .values()
            .find(|a| {
                editor.selected.contains(&a.id)
                    || a.members.iter().any(|m| editor.selected.contains(m))
            })
            .or_else(|| project.assemblies.values().next())
            .ok_or("В модели нет сборок")?
            .id;
        let id = ObjectId(project.next_id);
        let number = format!("КМД-{}", project.drawings.len() + 1);
        let drawing = open_bim_drawings::assembly_drawing(project, id, assembly, &number)?;
        self.create(editor, drawing)
    }

    pub fn create_general(&mut self, editor: &mut Editor, sheet: SheetSize) -> Result<(), String> {
        let project = editor.document.project();
        let id = ObjectId(project.next_id);
        let number = format!("КМ-{}", project.drawings.len() + 1);
        let drawing = open_bim_drawings::general_drawing(project, id, Vec::new(), &number, sheet)?;
        self.create(editor, drawing)
    }

    pub fn set_header(
        &mut self,
        editor: &mut Editor,
        number: &str,
        title: &str,
    ) -> Result<(), String> {
        let mut d = self.require(editor.document.project())?;
        d.number = number.trim().into();
        d.title = title.trim().into();
        self.replace(editor, "обозначение", d)
    }

    /// Changing the sheet keeps view positions inside the new frame.
    pub fn set_sheet(&mut self, editor: &mut Editor, sheet: SheetSize) -> Result<(), String> {
        let mut d = self.require(editor.document.project())?;
        let [w, h] = sheet.landscape_mm();
        d.sheet = sheet;
        for view in &mut d.views {
            view.position = [
                view.position[0].clamp(25., w - 10.),
                view.position[1].clamp(65., h - 10.),
            ];
        }
        for note in &mut d.texts {
            note.position = [
                note.position[0].clamp(25., w - 10.),
                note.position[1].clamp(10., h - 10.),
            ];
        }
        self.replace(editor, &format!("лист {}", sheet.label()), d)
    }

    pub fn set_scale(&mut self, editor: &mut Editor, scale: f64) -> Result<(), String> {
        let mut d = self.require(editor.document.project())?;
        d.scale = scale;
        self.replace(editor, &format!("масштаб 1:{scale}"), d)
    }

    pub fn move_view(
        &mut self,
        editor: &mut Editor,
        view: u32,
        position: [f64; 2],
    ) -> Result<(), String> {
        let mut d = self.require(editor.document.project())?;
        d.views
            .iter_mut()
            .find(|v| v.id == view)
            .ok_or("Вид не найден")?
            .position = position;
        self.replace(editor, &format!("вид {view} перемещён"), d)
    }

    /// Append an anchor of a model object to a dimension chain.
    pub fn add_anchor(
        &mut self,
        editor: &mut Editor,
        dimension: u32,
        anchor: Anchor,
    ) -> Result<(), String> {
        let mut d = self.require(editor.document.project())?;
        d.dimensions
            .iter_mut()
            .find(|x| x.id == dimension)
            .ok_or("Размер не найден")?
            .anchors
            .push(anchor);
        self.replace(editor, &format!("размер {dimension}: точка добавлена"), d)
    }

    pub fn remove_anchor(
        &mut self,
        editor: &mut Editor,
        dimension: u32,
        index: usize,
    ) -> Result<(), String> {
        let mut d = self.require(editor.document.project())?;
        let dim = d
            .dimensions
            .iter_mut()
            .find(|x| x.id == dimension)
            .ok_or("Размер не найден")?;
        if dim.anchors.len() <= 2 {
            return Err("Размерная цепь требует минимум двух точек".into());
        }
        if index >= dim.anchors.len() {
            return Err("Нет такой точки".into());
        }
        dim.anchors.remove(index);
        self.replace(editor, &format!("размер {dimension}: точка удалена"), d)
    }

    pub fn set_dimension_offset(
        &mut self,
        editor: &mut Editor,
        dimension: u32,
        offset: f64,
    ) -> Result<(), String> {
        let mut d = self.require(editor.document.project())?;
        d.dimensions
            .iter_mut()
            .find(|x| x.id == dimension)
            .ok_or("Размер не найден")?
            .offset = offset;
        self.replace(editor, &format!("размер {dimension}: отступ"), d)
    }

    /// New horizontal chain over the start/end of the selected parts.
    pub fn add_dimension(
        &mut self,
        editor: &mut Editor,
        view: u32,
        axis: DimensionAxis,
    ) -> Result<u32, String> {
        let project = editor.document.project();
        let mut d = self.require(project)?;
        let mut anchors = Vec::new();
        for id in &editor.selected {
            match project.elements.get(id).map(|e| &e.shape) {
                Some(ElementShape::Member { .. }) => {
                    anchors.push(Anchor {
                        object: *id,
                        point: AnchorPoint::Start,
                    });
                    anchors.push(Anchor {
                        object: *id,
                        point: AnchorPoint::End,
                    });
                }
                Some(ElementShape::Plate { contour, .. }) => {
                    for index in 0..contour.len() {
                        anchors.push(Anchor {
                            object: *id,
                            point: AnchorPoint::Vertex { index },
                        });
                    }
                }
                None => {}
            }
        }
        if anchors.len() < 2 {
            return Err("Выберите детали для размерной цепи".into());
        }
        let id = next_id(&d.dimensions, |x| x.id);
        d.dimensions.push(DimensionDef {
            id,
            view,
            anchors,
            axis,
            offset: 25.,
        });
        self.selected_dimension = Some(id);
        self.replace(editor, &format!("размер {id} добавлен"), d)?;
        Ok(id)
    }

    pub fn delete_dimension(&mut self, editor: &mut Editor, dimension: u32) -> Result<(), String> {
        let mut d = self.require(editor.document.project())?;
        let before = d.dimensions.len();
        d.dimensions.retain(|x| x.id != dimension);
        if before == d.dimensions.len() {
            return Err("Размер не найден".into());
        }
        self.selected_dimension = None;
        self.replace(editor, &format!("размер {dimension} удалён"), d)
    }

    pub fn add_text(
        &mut self,
        editor: &mut Editor,
        text: &str,
        position: [f64; 2],
        height: f64,
    ) -> Result<u32, String> {
        let mut d = self.require(editor.document.project())?;
        let id = next_id(&d.texts, |x| x.id);
        d.texts.push(TextNote {
            id,
            position,
            text: text.into(),
            height,
        });
        self.selected_text = Some(id);
        self.replace(editor, "текст добавлен", d)?;
        Ok(id)
    }

    pub fn edit_text(
        &mut self,
        editor: &mut Editor,
        id: u32,
        text: &str,
        position: [f64; 2],
    ) -> Result<(), String> {
        let mut d = self.require(editor.document.project())?;
        let note = d
            .texts
            .iter_mut()
            .find(|x| x.id == id)
            .ok_or("Текст не найден")?;
        note.text = text.into();
        note.position = position;
        self.replace(editor, "текст изменён", d)
    }

    pub fn delete_text(&mut self, editor: &mut Editor, id: u32) -> Result<(), String> {
        let mut d = self.require(editor.document.project())?;
        d.texts.retain(|x| x.id != id);
        self.selected_text = None;
        self.replace(editor, "текст удалён", d)
    }

    /// Re-read referenced objects; clears the outdated mark.
    pub fn update(&mut self, editor: &mut Editor) -> Result<(), String> {
        let project = editor.document.project();
        let d = self.require(project)?;
        if let Status::LostReferences { missing } = open_bim_drawings::status(project, &d)
            && d.dimensions
                .iter()
                .any(|dim| dim.anchors.iter().any(|a| missing.contains(&a.object)))
        {
            return Err("Удалите размеры с потерянными привязками перед обновлением".into());
        }
        let fresh = open_bim_drawings::updated(project, &d);
        self.replace(editor, "обновлён по модели", fresh)
    }

    pub fn issue(&mut self, editor: &mut Editor) -> Result<(), String> {
        let project = editor.document.project();
        let d = open_bim_drawings::issue(project, &self.require(project)?)?;
        self.replace(editor, "выдан", d)
    }

    pub fn delete(&mut self, editor: &mut Editor) -> Result<(), String> {
        let d = self.require(editor.document.project())?;
        editor.commit_data(
            &format!("Удалить чертёж {}", d.number),
            &[Command::Delete {
                id: d.id,
                policy: open_bim_commands::DeletePolicy::RejectDependencies,
            }],
        )?;
        self.current = None;
        Ok(())
    }

    /// Regenerated sheet; cached per drawing record and model revision.
    pub fn sheet(&mut self, project: &Project) -> Result<&Sheet, String> {
        let drawing = self.require(project)?;
        let fresh = !matches!(&self.cache, Some((d, revision, _)) if *d == drawing && *revision == project.revision);
        if fresh {
            let sheet = open_bim_drawings::generate(project, &drawing)?;
            self.cache = Some((drawing, project.revision, sheet));
        }
        Ok(&self.cache.as_ref().ok_or("Лист не построен")?.2)
    }

    pub fn export(
        &mut self,
        project: &Project,
        format: ExportFormat,
        directory: &Path,
    ) -> Result<std::path::PathBuf, String> {
        let drawing = self.require(project)?;
        let sheet = self.sheet(project)?.clone();
        std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
        let safe: String = drawing
            .number
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let path = directory.join(format!("{safe}.{}", format.extension()));
        let bytes = match format {
            ExportFormat::Pdf => open_bim_drawing_export::pdf::render(&[sheet], &drawing.number)?,
            ExportFormat::Svg => open_bim_drawing_export::svg(&sheet).into_bytes(),
            ExportFormat::Dxf => open_bim_drawing_export::dxf::render(&sheet).into_bytes(),
        };
        std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
        Ok(path)
    }

    pub fn kind_label(kind: &DrawingKind) -> &'static str {
        match kind {
            DrawingKind::General { .. } => "Общий вид",
            DrawingKind::Part { .. } => "Деталь",
            DrawingKind::Assembly { .. } => "Сборка",
        }
    }
}
