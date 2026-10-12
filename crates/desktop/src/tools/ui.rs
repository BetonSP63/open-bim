use super::{Edit, Editor, SnapMode, ToolKind, number, point};
use eframe::egui;
use open_bim_model::{ObjectId, Point3, PropertyValue};
#[derive(Clone, Debug)]
pub enum Action {
    Create,
    Edit(Edit),
    Undo,
    Redo,
    Save,
    Open,
    New,
    Example,
}
pub struct ToolsUi {
    pub delta: [String; 3],
    pub pivot: [String; 3],
    pub angle: String,
    pub axis: usize,
    pub offset: String,
    pub key: String,
    pub value: String,
    pub path: String,
    pub workplane: String,
    pub snap_enabled: bool,
    pub snap_mode: SnapMode,
    pub snap_tolerance: f64,
    pub message: Option<String>,
}
impl Default for ToolsUi {
    fn default() -> Self {
        Self {
            delta: ["1000".into(), "0".into(), "0".into()],
            pivot: ["0".into(), "0".into(), "0".into()],
            angle: "90".into(),
            axis: 2,
            offset: "0".into(),
            key: "Марка".into(),
            value: "К1".into(),
            path: "project.obim".into(),
            workplane: "0".into(),
            snap_enabled: true,
            snap_mode: SnapMode::All,
            snap_tolerance: 40.0,
            message: None,
        }
    }
}
fn vector_fields(ui: &mut egui::Ui, id: &str, values: &mut [String; 3]) {
    ui.push_id(id, |ui| {
        ui.horizontal(|ui| {
            for (i, value) in values.iter_mut().enumerate() {
                ui.label(["X", "Y", "Z"][i]);
                ui.add(egui::TextEdit::singleline(value).desired_width(58.0));
            }
        });
    });
}
impl ToolsUi {
    pub fn show(&mut self, ui: &mut egui::Ui, editor: &mut Editor) -> Option<Action> {
        let mut action = None;
        ui.heading("Моделирование");
        ui.horizontal_wrapped(|ui| {
            for kind in [
                ToolKind::Axis,
                ToolKind::Beam,
                ToolKind::Column,
                ToolKind::Plate,
            ] {
                if ui
                    .selectable_label(editor.active && editor.draft.kind == kind, kind.label())
                    .clicked()
                {
                    editor.begin(kind);
                }
            }
        });
        if editor.active {
            ui.separator();
            ui.strong(format!("Создать: {}", editor.draft.kind.label()));
            ui.label("Имя");
            ui.text_edit_singleline(&mut editor.draft.name);
            ui.label("Начало / начало плиты, мм");
            vector_fields(ui, "start", &mut editor.draft.start);
            if editor.draft.kind == ToolKind::Plate {
                ui.label("Контур XY: X,Y; X,Y; …");
                ui.add(egui::TextEdit::multiline(&mut editor.draft.contour).desired_rows(3));
                ui.horizontal(|ui| {
                    ui.label("Толщина, мм");
                    ui.text_edit_singleline(&mut editor.draft.thickness);
                });
            } else {
                ui.label("Конец, мм");
                vector_fields(ui, "end", &mut editor.draft.end);
            }
            if matches!(editor.draft.kind, ToolKind::Beam | ToolKind::Column) {
                let project = editor.document.project();
                egui::ComboBox::from_id_salt("profile")
                    .selected_text(
                        project
                            .profiles
                            .get(&editor.draft.profile)
                            .map_or("Выберите профиль", |p| p.name.as_str()),
                    )
                    .show_ui(ui, |ui| {
                        for profile in project.profiles.values() {
                            ui.selectable_value(
                                &mut editor.draft.profile,
                                profile.id,
                                &profile.name,
                            );
                        }
                    });
            }
            if editor.draft.kind != ToolKind::Axis {
                let project = editor.document.project();
                egui::ComboBox::from_id_salt("material")
                    .selected_text(
                        project
                            .materials
                            .get(&editor.draft.material)
                            .map_or("Выберите материал", |m| m.name.as_str()),
                    )
                    .show_ui(ui, |ui| {
                        for material in project.materials.values() {
                            ui.selectable_value(
                                &mut editor.draft.material,
                                material.id,
                                &material.name,
                            );
                        }
                    });
            }
            ui.horizontal(|ui| {
                if ui.button("Создать").clicked() {
                    action = Some(Action::Create);
                }
                if ui.button("Отмена · Esc").clicked() {
                    editor.cancel();
                }
            });
            ui.label("Точки можно указать в 3D-виде. Плита лежит в рабочей плоскости XY.");
        }
        ui.separator();
        ui.checkbox(&mut self.snap_enabled, "Привязки");
        egui::ComboBox::from_id_salt("snap mode")
            .selected_text(snap_label(self.snap_mode))
            .show_ui(ui, |ui| {
                for mode in [
                    SnapMode::All,
                    SnapMode::Axis,
                    SnapMode::End,
                    SnapMode::Midpoint,
                    SnapMode::Intersection,
                ] {
                    ui.selectable_value(&mut self.snap_mode, mode, snap_label(mode));
                }
            });
        ui.add(egui::Slider::new(&mut self.snap_tolerance, 1.0..=200.0).text("Допуск, мм"));
        ui.horizontal(|ui| {
            ui.label("Рабочая Z, мм");
            ui.text_edit_singleline(&mut self.workplane);
        });
        ui.separator();
        ui.strong(format!("Выбрано: {}", editor.selected.len()));
        ui.label("Смещение, мм");
        vector_fields(ui, "delta", &mut self.delta);
        ui.horizontal(|ui| {
            for (label, copy) in [("Переместить", false), ("Копировать", true)]
            {
                if ui
                    .add_enabled(!editor.selected.is_empty(), egui::Button::new(label))
                    .clicked()
                {
                    match point(&self.delta) {
                        Ok(delta) => {
                            action = Some(Action::Edit(if copy {
                                Edit::Copy(delta)
                            } else {
                                Edit::Move(delta)
                            }))
                        }
                        Err(e) => self.message = Some(e),
                    }
                }
            }
        });
        ui.label("Центр поворота, мм");
        vector_fields(ui, "pivot", &mut self.pivot);
        ui.horizontal(|ui| {
            ui.label("Ось");
            for (i, name) in ["X", "Y", "Z"].into_iter().enumerate() {
                ui.selectable_value(&mut self.axis, i, name);
            }
            ui.label("Угол, °");
            ui.add(egui::TextEdit::singleline(&mut self.angle).desired_width(50.0));
        });
        if ui
            .add_enabled(!editor.selected.is_empty(), egui::Button::new("Повернуть"))
            .clicked()
        {
            match number(&self.angle)
                .and_then(|degrees| point(&self.pivot).map(|pivot| (degrees, pivot)))
            {
                Ok((degrees, pivot)) => {
                    action = Some(Action::Edit(Edit::Rotate {
                        axis: unit(self.axis),
                        degrees,
                        pivot,
                    }))
                }
                Err(e) => self.message = Some(e),
            }
        }
        ui.horizontal(|ui| {
            ui.label("Плоскость зеркала, мм");
            ui.add(egui::TextEdit::singleline(&mut self.offset).desired_width(50.0));
        });
        if ui
            .add_enabled(!editor.selected.is_empty(), egui::Button::new("Зеркало"))
            .clicked()
        {
            match number(&self.offset) {
                Ok(offset) => {
                    action = Some(Action::Edit(Edit::Mirror {
                        normal: unit(self.axis),
                        offset,
                    }))
                }
                Err(e) => self.message = Some(e),
            }
        }
        ui.separator();
        ui.label("Свойство / текстовое значение");
        ui.text_edit_singleline(&mut self.key);
        ui.text_edit_singleline(&mut self.value);
        if ui
            .add_enabled(
                !editor.selected.is_empty(),
                egui::Button::new("Применить к выбранным"),
            )
            .clicked()
        {
            action = Some(Action::Edit(Edit::Property {
                key: self.key.clone(),
                value: PropertyValue::Text(self.value.clone()),
            }));
        }
        ui.separator();
        ui.label("Файл проекта .obim");
        ui.text_edit_singleline(&mut self.path);
        ui.horizontal(|ui| {
            if ui.button("Сохранить").clicked() {
                action = Some(Action::Save);
            }
            if ui.button("Открыть").clicked() {
                action = Some(Action::Open);
            }
        });
        if let Some(message) = &self.message {
            ui.separator();
            ui.label(message);
        }
        if let Some(warning) = &editor.record_warning {
            ui.colored_label(egui::Color32::YELLOW, warning);
        }
        action
    }
}
pub fn snap_label(mode: SnapMode) -> &'static str {
    match mode {
        SnapMode::All => "Все",
        SnapMode::Axis => "Ось",
        SnapMode::End => "Конец",
        SnapMode::Midpoint => "Середина",
        SnapMode::Intersection => "Пересечение осей",
    }
}
fn unit(axis: usize) -> Point3 {
    Point3(std::array::from_fn(|i| if i == axis { 1.0 } else { 0.0 }))
}
pub fn select(
    selected: &mut std::collections::BTreeSet<ObjectId>,
    id: Option<ObjectId>,
    extend: bool,
) {
    if !extend {
        selected.clear();
    }
    if let Some(id) = id {
        if extend && selected.contains(&id) {
            selected.remove(&id);
        } else {
            selected.insert(id);
        }
    }
}
