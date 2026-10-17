//! Right-hand drawing panel (Russian UI); all edits go through DrawingEditor.
use super::{DrawingEditor, ExportFormat};
use crate::tools::{Editor, number};
use eframe::egui;
use open_bim_drawings::{SCALES, Status};
use open_bim_model::{Anchor, AnchorPoint, DimensionAxis, Drawing, SheetSize};

#[derive(Clone, Debug, Default)]
pub struct Fields {
    pub number: String,
    pub title: String,
    pub scale: String,
    pub view: [String; 2],
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

pub fn status_color(status: &Status) -> egui::Color32 {
    match status {
        Status::UpToDate => egui::Color32::from_rgb(90, 200, 120),
        Status::Outdated { .. } => egui::Color32::from_rgb(255, 196, 0),
        Status::LostReferences { .. } => egui::Color32::from_rgb(255, 90, 90),
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
        });
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
                        let text = egui::RichText::new(text).color(status_color(status));
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
        ui.colored_label(status_color(&status), status.label());
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
                ui.label("Обозначение");
                ui.text_edit_singleline(&mut self.fields.number);
                ui.end_row();
                ui.label("Наименование");
                ui.text_edit_singleline(&mut self.fields.title);
                ui.end_row();
            });
        if ui.button("Применить обозначение").clicked() {
            let (n, t) = (self.fields.number.clone(), self.fields.title.clone());
            let r = self.set_header(editor, &n, &t);
            report(self, r);
        }
        ui.horizontal(|ui| {
            ui.label("Лист");
            let mut sheet = drawing.sheet;
            egui::ComboBox::from_id_salt("sheet size")
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
            ui.label("Масштаб 1:");
            let mut scale = drawing.scale;
            egui::ComboBox::from_id_salt("sheet scale")
                .selected_text(format!("{scale}"))
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
        ui.separator();
        ui.strong("Виды");
        for view in &drawing.views {
            let text = format!(
                "{} · {}{}",
                view.name,
                direction_label(view.direction),
                if view.section.is_some() {
                    " · сечение"
                } else {
                    ""
                }
            );
            if ui
                .selectable_label(self.selected_view == Some(view.id), text)
                .clicked()
            {
                self.selected_view = Some(view.id);
                self.fields.view = view.position.map(|x| format!("{x:.1}"));
            }
        }
        if let Some(view) = self.selected_view {
            ui.horizontal(|ui| {
                ui.label("Центр X");
                ui.add(egui::TextEdit::singleline(&mut self.fields.view[0]).desired_width(60.));
                ui.label("Y");
                ui.add(egui::TextEdit::singleline(&mut self.fields.view[1]).desired_width(60.));
                if ui.button("Переместить вид").clicked() {
                    let r = number(&self.fields.view[0])
                        .and_then(|x| Ok([x, number(&self.fields.view[1])?]))
                        .and_then(|p| self.move_view(editor, view, p));
                    report(self, r);
                }
            });
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
                ui.label("Отступ, мм");
                ui.add(egui::TextEdit::singleline(&mut self.fields.offset).desired_width(60.));
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
        ui.add(egui::TextEdit::singleline(&mut self.fields.text).hint_text("Текст примечания"));
        ui.horizontal(|ui| {
            ui.label("X");
            ui.add(
                egui::TextEdit::singleline(&mut self.fields.text_position[0]).desired_width(50.),
            );
            ui.label("Y");
            ui.add(
                egui::TextEdit::singleline(&mut self.fields.text_position[1]).desired_width(50.),
            );
            ui.label("h");
            ui.add(egui::TextEdit::singleline(&mut self.fields.text_height).desired_width(36.));
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
            ui.label("Папка");
            ui.text_edit_singleline(&mut self.export_dir);
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

    /// Central sheet area; drag moves views, click selects them.
    pub fn show_sheet(&mut self, ui: &mut egui::Ui, editor: &mut Editor) {
        let project = editor.document.project().clone();
        if self.drawing(&project).is_none() {
            ui.centered_and_justified(|ui| {
                ui.label("Выберите или создайте чертёж справа");
            });
            return;
        }
        let selected = self.selected_view;
        let mut state = std::mem::take(&mut self.preview);
        let event = match self.sheet(&project) {
            Ok(sheet) => super::preview::show(ui, sheet, &mut state, selected),
            Err(error) => {
                ui.label(error);
                None
            }
        };
        self.preview = state;
        match event {
            Some(super::preview::PreviewEvent::SelectView(id)) => {
                self.selected_view = Some(id);
                if let Some(v) = self
                    .drawing(&project)
                    .and_then(|d| d.views.iter().find(|v| v.id == id))
                {
                    self.fields.view = v.position.map(|x| format!("{x:.1}"));
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
            }
            None => {}
        }
    }
}
