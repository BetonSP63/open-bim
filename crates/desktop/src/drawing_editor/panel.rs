//! Right-hand drawing panel (Russian UI); all edits go through DrawingEditor.
use super::{DrawingEditor, ExportFormat};
use crate::tools::{Editor, number};
use eframe::egui;
use open_bim_drawings::{SCALES, Status};
use open_bim_model::{Anchor, AnchorPoint, DimensionAxis, Drawing, DrawingKind, SheetSize};

#[derive(Clone, Debug, Default)]
pub struct Fields {
    pub number: String,
    pub title: String,
    pub scale: String,
    pub view: [String; 2],
    pub view_name: String,
    /// Section depth, model mm.
    pub view_depth: String,
    pub offset: String,
    pub text: String,
    pub text_position: [String; 2],
    pub text_height: String,
}

impl Fields {
    pub fn load(&mut self, drawing: Option<&Drawing>) {
        let Some(d) = drawing else {
            *self = Self::default();
            return;
        };
        self.number = d.number.clone();
        self.title = d.title.clone();
        self.scale = format!("{}", d.scale);
        if let Some(v) = d.views.first() {
            self.view = v.position.map(|x| format!("{x:.1}"));
        }
        if self.text_height.is_empty() {
            self.text_height = "5".into();
            self.text = "Примечание".into();
            self.text_position = ["40".into(), "70".into()];
        }
    }
}

pub fn direction_label(direction: open_bim_model::ViewDirection) -> &'static str {
    use open_bim_model::ViewDirection as D;
    match direction {
        D::Front => "спереди",
        D::Top => "сверху",
        D::Left => "слева",
        D::Right => "справа",
        D::Bottom => "снизу",
        D::Back => "сзади",
    }
}

fn anchor_label(point: AnchorPoint) -> String {
    match point {
        AnchorPoint::Start => "начало".into(),
        AnchorPoint::End => "конец".into(),
        AnchorPoint::Vertex { index } => format!("вершина {}", index + 1),
        AnchorPoint::Hole { hole } => format!("отверстие #{}", hole.0),
    }
}

pub fn status_color(status: &Status, visuals: &egui::Visuals) -> egui::Color32 {
    match status {
        Status::UpToDate => crate::ui::theme::ok(visuals),
        Status::Outdated { .. } => visuals.warn_fg_color,
        Status::LostReferences { .. } => visuals.error_fg_color,
    }
}

fn report(editor: &mut DrawingEditor, result: Result<(), String>) {
    if let Err(error) = result {
        editor.message = Some(error);
    }
}

impl DrawingEditor {
    pub fn show(&mut self, ui: &mut egui::Ui, editor: &mut Editor) {
        ui.heading("Чертежи");
        ui.horizontal_wrapped(|ui| {
            if ui
                .button("Менеджер документов · Ctrl+L")
                .on_hover_text("Все чертежи в таблице: фильтры, обновление, выдача и экспорт")
                .clicked()
            {
                self.manager.open = true;
            }
            if ui
                .button("Мастер чертежей")
                .on_hover_text("Чертежи сборок и деталей по правилам, по одному на марку")
                .clicked()
            {
                self.wizard.open = true;
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui
                .button("Чертёж детали")
                .on_hover_text("По выбранной детали")
                .clicked()
            {
                let r = self.create_part(editor);
                report(self, r);
            }
            if ui.button("Чертёж сборки").clicked() {
                let r = self.create_assembly(editor);
                report(self, r);
            }
            if ui.button("Общий вид A1").clicked() {
                let r = self.create_general(editor, SheetSize::A1);
                report(self, r);
            }
            if ui.button("Общий вид A3").clicked() {
                let r = self.create_general(editor, SheetSize::A3);
                report(self, r);
            }
            if ui
                .button("Общие данные")
                .on_hover_text("Первый лист комплекта: ведомости и общие указания")
                .clicked()
            {
                let r = self.create_general_data(editor);
                report(self, r);
            }
        });
        ui.separator();
        self.changes_section(ui, editor);
        ui.separator();
        let rows: Vec<(open_bim_model::ObjectId, String, Status)> = {
            let project = editor.document.project();
            project
                .drawings
                .values()
                .map(|d| {
                    (
                        d.id,
                        format!(
                            "{} · {} · {}",
                            d.number,
                            Self::kind_label(&d.kind),
                            d.sheet.label()
                        ),
                        open_bim_drawings::status(project, d),
                    )
                })
                .collect()
        };
        if rows.is_empty() {
            ui.label("Чертежей нет. Выберите деталь и создайте чертёж.");
        }
        egui::ScrollArea::vertical()
            .id_salt("drawing list")
            .max_height(140.)
            .show(ui, |ui| {
                for (id, text, status) in &rows {
                    ui.horizontal(|ui| {
                        // On the selection fill the status colour is unreadable: use
                        // the selection text colour there.
                        let color = if self.current == Some(*id) {
                            ui.visuals().selection.stroke.color
                        } else {
                            status_color(status, ui.visuals())
                        };
                        let text = egui::RichText::new(text).color(color);
                        if ui
                            .selectable_label(self.current == Some(*id), text)
                            .clicked()
                        {
                            self.select(editor.document.project(), *id);
                        }
                    });
                }
            });
        let Some(drawing) = self.drawing(editor.document.project()).cloned() else {
            if let Some(m) = &self.message {
                ui.label(m);
            }
            return;
        };
        let status = open_bim_drawings::status(editor.document.project(), &drawing);
        ui.separator();
        ui.colored_label(status_color(&status, ui.visuals()), status.label());
        ui.horizontal(|ui| {
            if ui.button("Обновить по модели · ⌘U").clicked()
                || ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::U))
            {
                let r = self.update(editor);
                report(self, r);
            }
            if ui
                .add_enabled(status == Status::UpToDate, egui::Button::new("Выдать"))
                .on_disabled_hover_text("Устаревший чертёж нельзя выдать")
                .clicked()
            {
                let r = self.issue(editor);
                report(self, r);
            }
            if ui.button("Удалить").clicked() {
                let r = self.delete(editor);
                report(self, r);
            }
        });
        if let Some(r) = drawing.issued_revision {
            ui.label(format!("Выдан на ревизии {r}"));
        }
        egui::Grid::new("drawing header")
            .num_columns(2)
            .show(ui, |ui| {
                let field_label = ui.label("Обозначение");
                ui.text_edit_singleline(&mut self.fields.number)
                    .labelled_by(field_label.id);
                ui.end_row();
                let field_label = ui.label("Наименование");
                ui.text_edit_singleline(&mut self.fields.title)
                    .labelled_by(field_label.id);
                ui.end_row();
            });
        if ui.button("Применить обозначение").clicked() {
            let (n, t) = (self.fields.number.clone(), self.fields.title.clone());
            let r = self.set_header(editor, &n, &t);
            report(self, r);
        }
        let sheet_list = matches!(drawing.kind, DrawingKind::GeneralData { .. });
        ui.horizontal(|ui| {
            let mut sheet = drawing.sheet;
            egui::ComboBox::new("sheet size", "Лист")
                .selected_text(sheet.label())
                .show_ui(ui, |ui| {
                    for s in [
                        SheetSize::A4,
                        SheetSize::A3,
                        SheetSize::A2,
                        SheetSize::A1,
                        SheetSize::A0,
                    ] {
                        ui.selectable_value(&mut sheet, s, s.label());
                    }
                });
            if sheet != drawing.sheet {
                let r = self.set_sheet(editor, sheet);
                report(self, r);
            }
            if sheet_list {
                return;
            }
            let mut scale = drawing.scale;
            egui::ComboBox::new("sheet scale", "Масштаб")
                .selected_text(format!("1:{scale}"))
                .show_ui(ui, |ui| {
                    for s in SCALES {
                        ui.selectable_value(&mut scale, s, format!("{s}"));
                    }
                });
            if scale != drawing.scale {
                let r = self.set_scale(editor, scale);
                report(self, r);
            }
        });
        // «Общие данные» has neither views nor dimensions.
        if !sheet_list {
            ui.separator();
            ui.strong("Виды");
            ui.horizontal_wrapped(|ui| {
                use super::tools::DrawTool;
                for (tool, hint) in [
                    (
                        DrawTool::View,
                        "Новый вид по выбранному направлению: щёлкните его место",
                    ),
                    (
                        DrawTool::Section,
                        "Две точки линии разреза на виде, затем место разреза",
                    ),
                    (DrawTool::Node, "Центр и край узла на виде"),
                ] {
                    if ui
                        .selectable_label(
                            self.tool.tool == Some(tool),
                            format!("+ {}", tool.label()),
                        )
                        .on_hover_text(hint)
                        .clicked()
                    {
                        self.start_tool(tool);
                    }
                }
            });
            for view in &drawing.views {
                let kind = if view.section.is_some() {
                    let n = open_bim_drawings::conventions::section_number(&drawing, view.id);
                    format!(" · разрез {n}-{n}")
                } else if view.node.is_some() {
                    " · узел".into()
                } else {
                    String::new()
                };
                let text = format!("{} · {}{kind}", view.name, direction_label(view.direction),);
                if ui
                    .selectable_label(self.selected_view == Some(view.id), text)
                    .clicked()
                {
                    self.selected_view = Some(view.id);
                    self.load_view_fields(editor.document.project());
                }
            }
            if let Some(view) = self
                .selected_view
                .and_then(|id| drawing.views.iter().find(|v| v.id == id))
                .cloned()
            {
                self.view_properties(ui, editor, &drawing, &view);
            }
            ui.separator();
            ui.strong("Размеры");
            for dim in &drawing.dimensions {
                let text = format!(
                    "№{} · вид {} · точек {}",
                    dim.id,
                    dim.view,
                    dim.anchors.len()
                );
                if ui
                    .selectable_label(self.selected_dimension == Some(dim.id), text)
                    .clicked()
                {
                    self.selected_dimension = Some(dim.id);
                    self.fields.offset = format!("{}", dim.offset);
                }
            }
            ui.horizontal_wrapped(|ui| {
                if ui
                    .button("Новая цепь по выбранным")
                    .on_hover_text("Начало/конец выбранных балок или вершины пластин")
                    .clicked()
                {
                    let view = self.selected_view.unwrap_or(1);
                    let r = self
                        .add_dimension(editor, view, DimensionAxis::Horizontal)
                        .map(|_| ());
                    report(self, r);
                }
            });
            if let Some(dim) = self
                .selected_dimension
                .and_then(|id| drawing.dimensions.iter().find(|d| d.id == id))
                .cloned()
            {
                let mut remove = None;
                for (i, a) in dim.anchors.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!("#{} {}", a.object.0, anchor_label(a.point)));
                        if ui
                            .small_button("✕")
                            .on_hover_text("Удалить точку")
                            .clicked()
                        {
                            remove = Some(i);
                        }
                    });
                }
                if let Some(i) = remove {
                    let r = self.remove_anchor(editor, dim.id, i);
                    report(self, r);
                }
                ui.horizontal(|ui| {
                    let part = editor
                        .selected
                        .iter()
                        .copied()
                        .find(|id| editor.document.project().elements.contains_key(id));
                    for (label, point) in [
                        ("+ начало", AnchorPoint::Start),
                        ("+ конец", AnchorPoint::End),
                    ] {
                        if ui
                            .add_enabled(part.is_some(), egui::Button::new(label))
                            .on_hover_text("Точка выбранной детали")
                            .clicked()
                            && let Some(object) = part
                        {
                            let r = self.add_anchor(editor, dim.id, Anchor { object, point });
                            report(self, r);
                        }
                    }
                });
                ui.horizontal(|ui| {
                    let l = ui.label("Отступ, мм");
                    ui.add(egui::TextEdit::singleline(&mut self.fields.offset).desired_width(60.))
                        .labelled_by(l.id)
                        .on_hover_text("Отступ размерной линии");
                    if ui.button("Применить").clicked() {
                        let r = number(&self.fields.offset)
                            .and_then(|o| self.set_dimension_offset(editor, dim.id, o));
                        report(self, r);
                    }
                    if ui.button("Удалить размер").clicked() {
                        let r = self.delete_dimension(editor, dim.id);
                        report(self, r);
                    }
                });
            }
        }
        ui.separator();
        ui.strong("Тексты");
        for note in &drawing.texts {
            if ui
                .selectable_label(self.selected_text == Some(note.id), &note.text)
                .clicked()
            {
                self.selected_text = Some(note.id);
                self.fields.text = note.text.clone();
                self.fields.text_position = note.position.map(|x| format!("{x:.1}"));
            }
        }
        let note = ui.label("Текст примечания");
        ui.add(egui::TextEdit::singleline(&mut self.fields.text).hint_text("Текст примечания"))
            .labelled_by(note.id);
        ui.horizontal(|ui| {
            let x = ui.label("X");
            ui.add(
                egui::TextEdit::singleline(&mut self.fields.text_position[0]).desired_width(50.),
            )
            .labelled_by(x.id);
            let y = ui.label("Y");
            ui.add(
                egui::TextEdit::singleline(&mut self.fields.text_position[1]).desired_width(50.),
            )
            .labelled_by(y.id);
            let h = ui.label("Высота");
            ui.add(egui::TextEdit::singleline(&mut self.fields.text_height).desired_width(36.))
                .labelled_by(h.id);
        });
        ui.horizontal(|ui| {
            let position = number(&self.fields.text_position[0])
                .and_then(|x| Ok([x, number(&self.fields.text_position[1])?]));
            if ui.button("Добавить текст").clicked() {
                let text = self.fields.text.clone();
                let r = position.clone().and_then(|p| {
                    let h = number(&self.fields.text_height)?;
                    self.add_text(editor, &text, p, h).map(|_| ())
                });
                report(self, r);
            }
            if let Some(id) = self.selected_text {
                if ui.button("Изменить").clicked() {
                    let text = self.fields.text.clone();
                    let r = position.and_then(|p| self.edit_text(editor, id, &text, p));
                    report(self, r);
                }
                if ui.button("Удалить").clicked() {
                    let r = self.delete_text(editor, id);
                    report(self, r);
                }
            }
        });
        ui.separator();
        ui.strong("Экспорт");
        ui.horizontal(|ui| {
            let field_label = ui.label("Папка");
            ui.text_edit_singleline(&mut self.export_dir)
                .labelled_by(field_label.id);
        });
        ui.horizontal(|ui| {
            for format in [ExportFormat::Pdf, ExportFormat::Svg, ExportFormat::Dxf] {
                if ui.button(format.extension().to_uppercase()).clicked() {
                    let dir = std::path::PathBuf::from(&self.export_dir);
                    match self.export(editor.document.project(), format, &dir) {
                        Ok(path) => {
                            self.message = Some(format!("Сохранено: {}", path.display()));
                            editor.record(&format!("Экспорт {}", path.display()));
                        }
                        Err(e) => self.message = Some(e),
                    }
                }
            }
        });
        if let Some(m) = &self.message {
            ui.label(m);
        }
    }

    /// Properties of the selected view: place, name, direction, scale,
    /// section depth; flipping, alignment and deletion.
    fn view_properties(
        &mut self,
        ui: &mut egui::Ui,
        editor: &mut Editor,
        drawing: &Drawing,
        view: &open_bim_model::DrawingView,
    ) {
        use super::views::ViewEdit;
        let id = view.id;
        egui::Grid::new("view properties")
            .num_columns(2)
            .show(ui, |ui| {
                let l = ui.label("Имя вида");
                ui.text_edit_singleline(&mut self.fields.view_name)
                    .labelled_by(l.id);
                ui.end_row();
                ui.label("Направление");
                let mut direction = view.direction;
                ui.add_enabled_ui(view.node.is_none() && !view.unfolded, |ui| {
                    egui::ComboBox::from_id_salt("view direction")
                        .selected_text(direction_label(direction))
                        .show_ui(ui, |ui| {
                            for d in open_bim_model::ViewDirection::ALL {
                                ui.selectable_value(&mut direction, d, direction_label(d));
                            }
                        })
                        .response
                        .widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::WidgetType::ComboBox,
                                true,
                                "Направление вида",
                            )
                        });
                });
                ui.end_row();
                if direction != view.direction {
                    let r = self.edit_view(
                        editor,
                        id,
                        ViewEdit {
                            direction: Some(direction),
                            ..Default::default()
                        },
                    );
                    report(self, r);
                }
                ui.label("Масштаб");
                let mut scale = view.scale;
                let text = |s: Option<f64>| match s {
                    Some(s) => format!("1:{s}"),
                    None => format!("как у чертежа (1:{})", drawing.scale),
                };
                egui::ComboBox::from_id_salt("view scale")
                    .selected_text(text(scale))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut scale, None, text(None));
                        for s in SCALES {
                            ui.selectable_value(&mut scale, Some(s), format!("1:{s}"));
                        }
                    })
                    .response
                    .widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, "Масштаб вида")
                    });
                ui.end_row();
                if scale != view.scale {
                    let r = self.edit_view(
                        editor,
                        id,
                        ViewEdit {
                            scale: Some(scale),
                            ..Default::default()
                        },
                    );
                    report(self, r);
                }
                if view.section.is_some() {
                    let l = ui.label("Глубина, мм");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.fields.view_depth).desired_width(80.),
                    )
                    .labelled_by(l.id)
                    .on_hover_text("Положение секущей плоскости вдоль взгляда");
                    ui.end_row();
                }
            });
        let mut appearance = view.appearance.clone();
        ui.collapsing("Линии и видимость", |ui| {
            for (caption, pen) in [
                ("Видимые линии", &mut appearance.visible),
                ("Невидимые линии", &mut appearance.hidden),
                ("Линии среза", &mut appearance.section),
            ] {
                ui.push_id(caption, |ui| {
                    pen_editor(ui, caption, pen);
                });
            }
            ui.checkbox(&mut appearance.show_hidden, "Показывать невидимые линии");
            ui.label("Детали в этом виде");
            let parts = open_bim_drawings::drawing_parts(editor.document.project(), &drawing.kind);
            egui::ScrollArea::vertical()
                .id_salt("view visibility")
                .max_height(180.)
                .show(ui, |ui| {
                    for id in parts {
                        let Some(element) = editor.document.project().elements.get(&id) else {
                            continue;
                        };
                        let mut shown = !appearance.hidden_objects.contains(&id);
                        if ui
                            .checkbox(&mut shown, format!("#{} {}", id.0, element.name))
                            .changed()
                        {
                            if shown {
                                appearance.hidden_objects.remove(&id);
                            } else {
                                appearance.hidden_objects.insert(id);
                            }
                        }
                        ui.push_id(id.0, |ui| {
                            let mut custom = appearance.objects.contains_key(&id);
                            if ui.checkbox(&mut custom, "Свои линии").changed() {
                                if custom {
                                    appearance.objects.insert(id, appearance.visible);
                                } else {
                                    appearance.objects.remove(&id);
                                }
                            }
                            if let Some(pen) = appearance.objects.get_mut(&id) {
                                pen_editor(ui, "Линии детали", pen);
                            }
                        });
                    }
                });
        });
        if appearance != view.appearance {
            let r = self.edit_view(
                editor,
                id,
                ViewEdit {
                    appearance: Some(appearance),
                    ..Default::default()
                },
            );
            report(self, r);
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button("Применить").clicked() {
                let depth = if view.section.is_some() {
                    number(&self.fields.view_depth).map(Some)
                } else {
                    Ok(None)
                };
                let name = self.fields.view_name.clone();
                let r = depth.and_then(|depth| {
                    self.edit_view(
                        editor,
                        id,
                        ViewEdit {
                            name: Some(name),
                            depth,
                            ..Default::default()
                        },
                    )
                });
                report(self, r);
            }
            if view.section.is_some()
                && ui
                    .button("Развернуть")
                    .on_hover_text("Смотреть на ту же плоскость с другой стороны")
                    .clicked()
            {
                let r = self.flip_section(editor, id);
                report(self, r);
            }
            if drawing.views.first().is_some_and(|v| v.id != id)
                && ui
                    .button("Выровнять по главному")
                    .on_hover_text("Вид сверху/снизу — под главным, боковые — на одном уровне")
                    .clicked()
            {
                let r = self.align_view(editor, id);
                report(self, r);
            }
            if ui
                .add_enabled(drawing.views.len() > 1, egui::Button::new("Удалить вид"))
                .on_hover_text("Вместе с размерами на нём")
                .clicked()
            {
                let r = self.delete_view(editor, id);
                report(self, r);
            }
        });
        ui.horizontal(|ui| {
            let l = ui.label("Центр X");
            ui.add(egui::TextEdit::singleline(&mut self.fields.view[0]).desired_width(60.))
                .labelled_by(l.id)
                .on_hover_text("Центр вида X, мм");
            let l = ui.label("Y");
            ui.add(egui::TextEdit::singleline(&mut self.fields.view[1]).desired_width(60.))
                .labelled_by(l.id)
                .on_hover_text("Центр вида Y, мм");
            if ui.button("Переместить вид").clicked() {
                let r = number(&self.fields.view[0])
                    .and_then(|x| Ok([x, number(&self.fields.view[1])?]))
                    .and_then(|p| self.move_view(editor, id, p));
                report(self, r);
            }
        });
    }

    /// Drawing tools and the style of new objects, above the sheet.
    fn toolbar(&mut self, ui: &mut egui::Ui, editor: &mut Editor) {
        use super::tools::DrawTool;
        use open_bim_model::{DimensionAxis, HatchPattern, LineType};
        ui.horizontal(|ui| {
            let select = ui.selectable_label(self.tool.tool.is_none(), "Выбор");
            if select.clicked() {
                self.cancel_tool();
            }
            for tool in [DrawTool::View, DrawTool::Section, DrawTool::Node] {
                if ui
                    .selectable_label(self.tool.tool == Some(tool), tool.label())
                    .clicked()
                {
                    self.start_tool(tool);
                }
            }
            ui.separator();
            for tool in [
                DrawTool::Dimension,
                DrawTool::Line,
                DrawTool::Polyline,
                DrawTool::Rectangle,
                DrawTool::Circle,
                DrawTool::Hatch,
                DrawTool::Wipeout,
                DrawTool::Text,
            ] {
                if ui
                    .selectable_label(self.tool.tool == Some(tool), tool.label())
                    .clicked()
                {
                    self.start_tool(tool);
                }
            }
        });
        // Second row always present, so the sheet does not jump when a tool
        // with options is chosen.
        ui.horizontal(|ui| {
            let style = &mut self.style;
            egui::ComboBox::from_id_salt("sheet line type")
                .selected_text(style.line_type.label())
                .show_ui(ui, |ui| {
                    for t in LineType::ALL {
                        ui.selectable_value(&mut style.line_type, t, t.label());
                    }
                })
                .response
                .on_hover_text("Тип линии новых объектов")
                .widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, "Тип линии")
                });
            ui.checkbox(&mut style.thick, "Толстая (S)");
            match self.tool.tool {
                Some(DrawTool::Hatch) => {
                    egui::ComboBox::from_id_salt("hatch pattern")
                        .selected_text(style.pattern.label())
                        .show_ui(ui, |ui| {
                            for p in HatchPattern::ALL {
                                ui.selectable_value(&mut style.pattern, p, p.label());
                            }
                        })
                        .response
                        .widget_info(|| {
                            egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, "Штриховка")
                        });
                    ui.add(
                        egui::DragValue::new(&mut style.angle)
                            .suffix("°")
                            .range(-180.0..=180.0),
                    );
                    ui.add(
                        egui::DragValue::new(&mut style.spacing)
                            .suffix(" мм шаг")
                            .range(0.5..=50.0)
                            .speed(0.1),
                    );
                }
                Some(DrawTool::View) => {
                    egui::ComboBox::from_id_salt("new view direction")
                        .selected_text(super::views::view_name(style.view_direction))
                        .show_ui(ui, |ui| {
                            for d in open_bim_model::ViewDirection::ALL {
                                ui.selectable_value(
                                    &mut style.view_direction,
                                    d,
                                    super::views::view_name(d),
                                );
                            }
                        })
                        .response
                        .widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::WidgetType::ComboBox,
                                true,
                                "Направление вида",
                            )
                        });
                }
                Some(DrawTool::Dimension) => {
                    for (axis, label) in [
                        (DimensionAxis::Horizontal, "Горизонтальный"),
                        (DimensionAxis::Vertical, "Вертикальный"),
                        (DimensionAxis::Aligned, "Параллельный"),
                    ] {
                        ui.selectable_value(&mut style.axis, axis, label);
                    }
                    ui.add(
                        egui::DragValue::new(&mut style.offset)
                            .suffix(" мм отступ")
                            .range(-200.0..=200.0),
                    );
                }
                _ => {}
            }
            if self.tool.tool.is_some() {
                ui.label("Enter — закончить, Esc — выход");
            } else if self.selected_object.is_some()
                || self.selected_dimension.is_some()
                || self.selected_text.is_some()
            {
                if ui.button("Удалить выбранное").clicked() {
                    let r = self.delete_selected(editor);
                    report(self, r);
                }
                ui.label("Стрелки — сдвиг на 1 мм, ⇧ — на 10 мм");
            }
        });
    }

    /// Central sheet area; drag moves views, click selects them.
    pub fn show_sheet(&mut self, ui: &mut egui::Ui, editor: &mut Editor) {
        let project = editor.document.project().clone();
        if self.drawing(&project).is_none() {
            ui.centered_and_justified(|ui| {
                ui.label("Выберите или создайте чертёж справа");
            });
            return;
        }
        self.toolbar(ui, editor);
        let picks = self.tool.points.clone();
        let overlay = super::preview::Overlay {
            selected_view: self.selected_view,
            selected_object: self.selected_object,
            selected_dimension: self.selected_dimension,
            tool: self.tool.tool.is_some(),
            picks: &picks,
        };
        let mut state = std::mem::take(&mut self.preview);
        let event = match self.sheet(&project) {
            Ok(sheet) => super::preview::show(ui, sheet, &mut state, &overlay),
            Err(error) => {
                ui.label(error);
                None
            }
        };
        self.preview = state;
        match event {
            Some(super::preview::PreviewEvent::Click(at, view)) => {
                self.fields.text_position = at.map(|x| format!("{x:.1}"));
                self.selected_view = None;
                // Objects and dimensions drawn over a view win over the view.
                let picked = match self.sheet(&project).cloned() {
                    Ok(sheet) => self.pick_at(&sheet, at),
                    Err(_) => false,
                };
                if !picked && let Some(id) = view {
                    self.selected_view = Some(id);
                    self.load_view_fields(&project);
                }
            }
            Some(super::preview::PreviewEvent::MoveView(id, offset)) => {
                if let Some(v) = self
                    .drawing(&project)
                    .and_then(|d| d.views.iter().find(|v| v.id == id))
                {
                    let position = [v.position[0] + offset[0], v.position[1] + offset[1]];
                    let r = self.move_view(editor, id, position);
                    self.selected_view = Some(id);
                    self.fields.view = position.map(|x| format!("{x:.1}"));
                    report(self, r);
                }
            }
            Some(super::preview::PreviewEvent::Paper(at)) => {
                self.fields.text_position = at.map(|x| format!("{x:.1}"));
                let r = self.tool_click(editor, at);
                report(self, r);
            }
            Some(super::preview::PreviewEvent::Finish) => {
                let r = self.tool_finish(editor);
                report(self, r);
            }
            None => {}
        }
    }
}

fn pen_editor(ui: &mut egui::Ui, caption: &str, pen: &mut open_bim_model::LinePen) {
    ui.label(caption);
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("line type")
            .selected_text(pen.line_type.label())
            .show_ui(ui, |ui| {
                for kind in open_bim_model::LineType::ALL {
                    ui.selectable_value(&mut pen.line_type, kind, kind.label());
                }
            });
        ui.add(
            egui::DragValue::new(&mut pen.width)
                .speed(0.01)
                .range(0.05..=2.)
                .suffix(" мм"),
        )
        .on_hover_text("Толщина на бумаге");
    });
}
