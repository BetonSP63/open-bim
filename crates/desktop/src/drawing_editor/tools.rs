//! Drawing editing tools: dimensions picked on view points, lines,
//! polylines, rectangles, circles, hatching, masking and texts placed by
//! clicks on the sheet; selection, moving and deleting of sheet objects.
use super::{DrawingEditor, next_id};
use crate::tools::Editor;
use open_bim_model::{
    Anchor, DimensionAxis, DimensionDef, HatchPattern, LineType, SheetObject, SheetObjectKind,
    ViewDirection,
};

/// Drawing editor tools («Чертёж → Размеры / Графика»).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawTool {
    Dimension,
    Line,
    Polyline,
    Rectangle,
    Circle,
    Hatch,
    Wipeout,
    Text,
    /// Node «Узел n»: centre and edge of the region on a view (5.5.3).
    Node,
    /// New projection view in the chosen direction, placed by a click.
    View,
    /// Section: two points of the cutting line on a view, then its place.
    Section,
}

impl DrawTool {
    pub fn label(self) -> &'static str {
        match self {
            Self::Dimension => "Размер",
            Self::Line => "Линия",
            Self::Polyline => "Полилиния",
            Self::Rectangle => "Прямоугольник",
            Self::Circle => "Окружность",
            Self::Hatch => "Штриховка",
            Self::Wipeout => "Маскировка",
            Self::Text => "Текст",
            Self::Node => "Узел",
            Self::View => "Вид",
            Self::Section => "Разрез",
        }
    }

    pub fn prompt(self, picks: usize) -> &'static str {
        match (self, picks) {
            (Self::Dimension, 0) => {
                "Размер: щёлкните первую точку детали (конец, вершину, отверстие)"
            }
            (Self::Dimension, 1) => "Размер: щёлкните вторую точку",
            (Self::Dimension, _) => "Размер: следующая точка цепи или Enter — поставить",
            (Self::Line, 0) | (Self::Rectangle, 0) => "Укажите первую точку",
            (Self::Line, _) => "Укажите вторую точку",
            (Self::Rectangle, _) => "Укажите противоположный угол",
            (Self::Circle, 0) => "Укажите центр окружности",
            (Self::Circle, _) => "Укажите точку на окружности",
            (Self::Polyline, _) => "Следующая точка или Enter — закончить",
            (Self::Hatch | Self::Wipeout, n) if n < 3 => "Укажите вершины области",
            (Self::Hatch | Self::Wipeout, _) => "Следующая вершина или Enter — замкнуть область",
            (Self::Text, _) => "Щёлкните, где поставить текст (текст — в поле «Текст примечания»)",
            (Self::Node, 0) => "Узел: щёлкните центр узла на виде",
            (Self::Node, _) => "Узел: щёлкните край области узла",
            (Self::View, _) => "Вид: щёлкните место нового вида на листе",
            (Self::Section, 0) => "Разрез: щёлкните первую точку линии разреза на виде",
            (Self::Section, 1) => "Разрез: щёлкните вторую точку линии разреза",
            (Self::Section, _) => {
                "Разрез: щёлкните место вида (стрелки разреза смотрят в его сторону)"
            }
        }
    }
}

/// Style of new objects.
#[derive(Clone, Debug)]
pub struct DrawStyle {
    pub line_type: LineType,
    pub thick: bool,
    pub pattern: HatchPattern,
    pub angle: f64,
    pub spacing: f64,
    pub axis: DimensionAxis,
    pub offset: f64,
    /// Direction of views added by the «Вид» tool.
    pub view_direction: ViewDirection,
}

impl Default for DrawStyle {
    fn default() -> Self {
        Self {
            line_type: LineType::Solid,
            thick: false,
            pattern: HatchPattern::Metal,
            angle: 45.,
            spacing: 3.,
            axis: DimensionAxis::Horizontal,
            // Clear of the automatic chains of the drawing.
            offset: 25.,
            view_direction: ViewDirection::Top,
        }
    }
}

/// Picks of the running tool.
#[derive(Clone, Debug, Default)]
pub struct ToolState {
    pub tool: Option<DrawTool>,
    pub points: Vec<[f64; 2]>,
    /// Dimension: view and the anchors picked so far.
    pub view: Option<u32>,
    pub anchors: Vec<Anchor>,
}

/// Sheet distance (mm) within which a click picks a dimension point.
pub const ANCHOR_TOLERANCE: f64 = 3.;

impl DrawingEditor {
    pub fn start_tool(&mut self, tool: DrawTool) {
        self.tool = ToolState {
            tool: Some(tool),
            ..Default::default()
        };
        self.message = Some(tool.prompt(0).into());
    }

    pub fn cancel_tool(&mut self) -> bool {
        let active = self.tool.tool.is_some();
        self.tool = ToolState::default();
        active
    }

    fn add_object(&mut self, editor: &mut Editor, kind: SheetObjectKind) -> Result<u32, String> {
        let mut d = self.require(editor.document.project())?;
        let id = next_id(&d.objects, |o| o.id);
        let label = match &kind {
            SheetObjectKind::Line { .. } => "линия",
            SheetObjectKind::Rectangle { .. } => "прямоугольник",
            SheetObjectKind::Circle { .. } => "окружность",
            SheetObjectKind::Hatch { .. } => "штриховка",
            SheetObjectKind::Wipeout { .. } => "маскировка",
        };
        d.objects.push(SheetObject {
            id,
            kind,
            line_type: self.style.line_type,
            thick: self.style.thick,
        });
        d.validate().map_err(|e| e.to_string())?;
        self.selected_object = Some(id);
        self.replace(editor, &format!("{label} {id}"), d)?;
        Ok(id)
    }

    /// A click on the sheet while a tool runs.
    pub fn tool_click(&mut self, editor: &mut Editor, at: [f64; 2]) -> Result<(), String> {
        let Some(tool) = self.tool.tool else {
            return Ok(());
        };
        match tool {
            DrawTool::Dimension => return self.dimension_click(editor, at),
            DrawTool::View => {
                self.add_view(editor, self.style.view_direction, at)?;
                return Ok(());
            }
            DrawTool::Text => {
                let text = self.fields.text.clone();
                if text.trim().is_empty() {
                    return Err("Введите текст в поле «Текст примечания»".into());
                }
                let height = crate::tools::number(&self.fields.text_height)?;
                self.add_text(editor, &text, at, height)?;
                return Ok(());
            }
            _ => self.tool.points.push(at),
        }
        let points = self.tool.points.clone();
        if let (DrawTool::Section, [a, b, place]) = (tool, &points[..]) {
            self.tool.points.clear();
            // A failed cut starts over from the first point.
            self.add_section(editor, *a, *b, *place)?;
            return Ok(());
        }
        if let (DrawTool::Node, [c, p]) = (tool, &points[..]) {
            self.tool.points.clear();
            self.add_node(editor, *c, (p[0] - c[0]).hypot(p[1] - c[1]))?;
            return Ok(());
        }
        let done = match (tool, &points[..]) {
            (DrawTool::Line, [a, b]) => Some(SheetObjectKind::Line {
                points: vec![*a, *b],
            }),
            (DrawTool::Rectangle, [a, b]) => Some(SheetObjectKind::Rectangle { a: *a, b: *b }),
            (DrawTool::Circle, [c, p]) => Some(SheetObjectKind::Circle {
                center: *c,
                radius: (p[0] - c[0]).hypot(p[1] - c[1]),
            }),
            _ => None,
        };
        match done {
            Some(kind) => {
                self.tool.points.clear();
                self.add_object(editor, kind)?;
                self.message = Some(tool.prompt(0).into());
            }
            None => self.message = Some(tool.prompt(points.len()).into()),
        }
        Ok(())
    }

    /// Enter / double click: finish a polyline, area or dimension chain.
    pub fn tool_finish(&mut self, editor: &mut Editor) -> Result<(), String> {
        let Some(tool) = self.tool.tool else {
            return Ok(());
        };
        let points = std::mem::take(&mut self.tool.points);
        let kind = match tool {
            DrawTool::Polyline if points.len() >= 2 => SheetObjectKind::Line { points },
            DrawTool::Hatch if points.len() >= 3 => SheetObjectKind::Hatch {
                polygon: points,
                pattern: self.style.pattern,
                angle: self.style.angle,
                spacing: self.style.spacing,
            },
            DrawTool::Wipeout if points.len() >= 3 => SheetObjectKind::Wipeout { polygon: points },
            DrawTool::Dimension => return self.dimension_finish(editor),
            _ => return Err("Недостаточно точек".into()),
        };
        self.add_object(editor, kind)?;
        self.message = Some(tool.prompt(0).into());
        Ok(())
    }

    /// A node view of the region around sheet point `at` with half side
    /// `size` sheet mm on the view under it, at a standard scale giving about
    /// 60 mm on the sheet, placed in the upper right corner.
    pub fn add_node(&mut self, editor: &mut Editor, at: [f64; 2], size: f64) -> Result<(), String> {
        let project = editor.document.project();
        let mut d = self.require(project)?;
        let (host, centre, scale) = open_bim_drawings::view_at(project, &d, at)?
            .ok_or("Щёлкните внутри вида (не узла и не разреза)")?;
        let radius = (size.max(2.) * scale).max(10.);
        let node_scale = open_bim_model::STANDARD_SCALES
            .iter()
            .copied()
            .find(|s| *s >= 1. && 2. * radius / s <= 60.)
            .unwrap_or(scale);
        let (direction, host_source) = d
            .views
            .iter()
            .find(|v| v.id == host)
            .map(|v| (v.direction, v.source))
            .ok_or("Вид не найден")?;
        let side = 2. * radius / node_scale;
        let [w, h] = d.sheet_mm();
        let id = next_id(&d.views, |v| v.id);
        d.views.push(open_bim_model::DrawingView {
            id,
            name: "Узел".into(),
            direction,
            section: None,
            position: [
                w - open_bim_drawings::MARGIN - 15. - side / 2.,
                h - open_bim_drawings::MARGIN - 20. - side / 2.,
            ],
            scale: Some(node_scale),
            node: Some(open_bim_model::NodeRegion { centre, radius }),
            source: host_source,
        });
        d.validate().map_err(|e| e.to_string())?;
        self.selected_view = Some(id);
        self.replace(editor, &format!("узел (вид {id})"), d)?;
        self.message = Some(format!(
            "Узел добавлен в масштабе 1:{node_scale}; переместите вид полями «Центр X/Y»"
        ));
        Ok(())
    }

    fn dimension_click(&mut self, editor: &mut Editor, at: [f64; 2]) -> Result<(), String> {
        let project = editor.document.project();
        let d = self.require(project)?;
        let candidates = open_bim_drawings::anchor_candidates(project, &d)?;
        let (view, anchor, _) = candidates
            .into_iter()
            .filter(|(v, _, _)| self.tool.view.is_none_or(|w| w == *v))
            .map(|(v, a, p)| (v, a, (p[0] - at[0]).hypot(p[1] - at[1])))
            .filter(|(_, _, dist)| *dist <= ANCHOR_TOLERANCE)
            .min_by(|a, b| a.2.total_cmp(&b.2))
            .ok_or("Рядом нет точки детали: щёлкните конец, вершину или отверстие")?;
        if self.tool.anchors.contains(&anchor) {
            return Err("Эта точка уже в размере".into());
        }
        self.tool.view = Some(view);
        self.tool.anchors.push(anchor);
        self.tool.points.push(at);
        self.message = Some(DrawTool::Dimension.prompt(self.tool.anchors.len()).into());
        Ok(())
    }

    fn dimension_finish(&mut self, editor: &mut Editor) -> Result<(), String> {
        let anchors = std::mem::take(&mut self.tool.anchors);
        let view = self.tool.view.take();
        self.tool.points.clear();
        let (Some(view), true) = (view, anchors.len() >= 2) else {
            return Err("Размер требует двух точек".into());
        };
        let mut d = self.require(editor.document.project())?;
        let id = next_id(&d.dimensions, |x| x.id);
        d.dimensions.push(DimensionDef {
            id,
            view,
            anchors,
            axis: self.style.axis,
            offset: self.style.offset,
        });
        self.selected_dimension = Some(id);
        self.replace(editor, &format!("размер {id} добавлен"), d)?;
        self.message = Some(DrawTool::Dimension.prompt(0).into());
        Ok(())
    }

    pub fn move_object(
        &mut self,
        editor: &mut Editor,
        id: u32,
        by: [f64; 2],
    ) -> Result<(), String> {
        let mut d = self.require(editor.document.project())?;
        let o = d
            .objects
            .iter_mut()
            .find(|o| o.id == id)
            .ok_or("Объект не найден")?;
        *o = o.moved(by);
        d.validate().map_err(|e| e.to_string())?;
        self.replace(editor, &format!("объект {id} перемещён"), d)
    }

    pub fn delete_object(&mut self, editor: &mut Editor, id: u32) -> Result<(), String> {
        let mut d = self.require(editor.document.project())?;
        let before = d.objects.len();
        d.objects.retain(|o| o.id != id);
        if before == d.objects.len() {
            return Err("Объект не найден".into());
        }
        self.selected_object = None;
        self.replace(editor, &format!("объект {id} удалён"), d)
    }

    /// Delete key: the selected object, text or dimension.
    pub fn delete_selected(&mut self, editor: &mut Editor) -> Result<(), String> {
        if let Some(id) = self.selected_object {
            self.delete_object(editor, id)
        } else if let Some(id) = self.selected_text {
            self.delete_text(editor, id)
        } else if let Some(id) = self.selected_dimension {
            self.delete_dimension(editor, id)
        } else {
            Err("Ничего не выбрано".into())
        }
    }

    /// Arrow keys: move the selected object, text or view.
    pub fn nudge_selected(&mut self, editor: &mut Editor, by: [f64; 2]) -> Result<(), String> {
        let project = editor.document.project();
        let d = self.require(project)?;
        if let Some(id) = self.selected_object {
            return self.move_object(editor, id, by);
        }
        if let Some(id) = self.selected_text {
            let note = d
                .texts
                .iter()
                .find(|t| t.id == id)
                .ok_or("Текст не найден")?;
            let (text, p) = (note.text.clone(), note.position);
            return self.edit_text(editor, id, &text, [p[0] + by[0], p[1] + by[1]]);
        }
        if let Some(view) = self.selected_view {
            let v = d
                .views
                .iter()
                .find(|v| v.id == view)
                .ok_or("Вид не найден")?;
            let p = v.position;
            return self.move_view(editor, view, [p[0] + by[0], p[1] + by[1]]);
        }
        Err("Ничего не выбрано".into())
    }

    /// Select what lies under a sheet click: a user object, a text, a
    /// dimension, otherwise nothing (views are selected by the preview).
    pub fn pick_at(&mut self, sheet: &open_bim_drawings::Sheet, at: [f64; 2]) -> bool {
        let near_line = |points: &[[f64; 2]]| {
            points.windows(2).any(|w| {
                let (a, b) = (w[0], w[1]);
                let d = [b[0] - a[0], b[1] - a[1]];
                let len2 = (d[0] * d[0] + d[1] * d[1]).max(1e-12);
                let t = (((at[0] - a[0]) * d[0] + (at[1] - a[1]) * d[1]) / len2).clamp(0., 1.);
                (a[0] + d[0] * t - at[0]).hypot(a[1] + d[1] * t - at[1]) <= 1.5
            })
        };
        let inside = |poly: &[[f64; 2]]| {
            let mut c = false;
            for i in 0..poly.len() {
                let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
                if (a[1] > at[1]) != (b[1] > at[1])
                    && at[0] < (b[0] - a[0]) * (at[1] - a[1]) / (b[1] - a[1]) + a[0]
                {
                    c = !c;
                }
            }
            c
        };
        self.selected_object = None;
        self.selected_text = None;
        self.selected_dimension = None;
        for o in sheet.objects.iter().rev() {
            if o.lines.iter().any(|l| near_line(&l.points)) || o.mask.as_deref().is_some_and(inside)
            {
                self.selected_object = Some(o.id);
                return true;
            }
        }
        for dim in &sheet.dimensions {
            if dim.lines.iter().any(|l| near_line(&l.points)) {
                self.selected_dimension = Some(dim.id);
                return true;
            }
        }
        false
    }
}
