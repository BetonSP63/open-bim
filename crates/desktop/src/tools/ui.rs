use super::{Edit, Editor, SnapMode, ToolKind, number, parse_contour, point};
use eframe::egui;
use open_bim_model::{ObjectId, Point3, PropertyValue};
#[derive(Clone, Debug)]
pub enum Action {
    Create,
    Detail(Vec<open_bim_commands::Command>),
    Connection(Vec<open_bim_commands::Command>),
    Edit(Edit),
    Undo,
    Redo,
    Save,
    Open,
    ExportIfc,
    ImportIfc,
    New,
    Example,
}
pub struct ToolsUi {
    pub connection: super::connection_ui::ConnectionUi,
    pub coordination: super::coordination_ui::CoordinationUi,
    pub detailing: super::detailing_ui::DetailingUi,
    pub plate_contour: String,
    pub plate_thickness: String,
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
    /// Keyboard command ⌘L: focus the property name field on the next frame.
    pub focus_property: bool,
}
impl Default for ToolsUi {
    fn default() -> Self {
        Self {
            connection: Default::default(),
            coordination: Default::default(),
            detailing: Default::default(),
            plate_contour: String::new(),
            plate_thickness: "20".into(),
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
            focus_property: false,
        }
    }
}
pub(super) fn vector_fields(ui: &mut egui::Ui, id: &str, values: &mut [String; 3]) {
    ui.push_id(id, |ui| {
        ui.horizontal(|ui| {
            for (i, value) in values.iter_mut().enumerate() {
                let label = ui.label(["X", "Y", "Z"][i]);
                let response = ui
                    .add(egui::TextEdit::singleline(value).desired_width(58.0))
                    .labelled_by(label.id);
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::TextEdit,
                        true,
                        format!("{id} {}", ["X", "Y", "Z"][i]),
                    )
                });
            }
        });
    });
}
impl ToolsUi {
    pub fn property_action(&self) -> Result<Action, String> {
        if self.key.trim().is_empty() {
            return Err("Введите имя свойства".into());
        }
        Ok(Action::Edit(Edit::Property {
            key: self.key.trim().into(),
            value: PropertyValue::Text(self.value.clone()),
        }))
    }
    pub fn show(&mut self, ui: &mut egui::Ui, editor: &mut Editor) -> Option<Action> {
        let mut action = None;
        ui.heading("Моделирование");
        ui.horizontal_wrapped(|ui| {
            for kind in [
                ToolKind::Axis,
                ToolKind::Beam,
                ToolKind::Column,
                ToolKind::Plate,
                ToolKind::Gusset,
                ToolKind::Stiffener,
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
            let name_label = ui.label("Имя");
            ui.text_edit_singleline(&mut editor.draft.name)
                .labelled_by(name_label.id);
            ui.label("Начало / начало плиты, мм");
            vector_fields(ui, "start", &mut editor.draft.start);
            if editor.draft.kind.is_plate() {
                let contour_label = ui.label("Контур XY: X,Y; X,Y; …");
                ui.add(egui::TextEdit::multiline(&mut editor.draft.contour).desired_rows(3))
                    .labelled_by(contour_label.id);
                ui.horizontal(|ui| {
                    let t = ui.label("Толщина, мм");
                    ui.text_edit_singleline(&mut editor.draft.thickness)
                        .labelled_by(t.id);
                });
            } else {
                ui.label("Конец, мм");
                vector_fields(ui, "end", &mut editor.draft.end);
            }
            if matches!(editor.draft.kind, ToolKind::Beam | ToolKind::Column) {
                let project = editor.document.project();
                egui::ComboBox::new("profile", "Профиль")
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
                egui::ComboBox::new("material", "Материал")
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
                if ui.button("Создать · ⌘↩").clicked() {
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
        egui::ComboBox::new("snap mode", "Режим привязки")
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
            let z = ui.label("Рабочая Z, мм");
            ui.text_edit_singleline(&mut self.workplane)
                .labelled_by(z.id);
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
            let a = ui.label("Угол, °");
            ui.add(egui::TextEdit::singleline(&mut self.angle).desired_width(50.0))
                .labelled_by(a.id);
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
            let m = ui.label("Плоскость зеркала, мм");
            ui.add(egui::TextEdit::singleline(&mut self.offset).desired_width(50.0))
                .labelled_by(m.id);
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
        if editor.selected.len() == 1 {
            let id = *editor.selected.first().unwrap();
            if let Some(element) = editor.document.project().elements.get(&id)
                && let open_bim_model::ElementShape::Plate {
                    contour, thickness, ..
                } = &element.shape
            {
                ui.collapsing("Контур и толщина пластины", |ui| {
                    if ui.button("Загрузить размеры выбранной").clicked() {
                        self.plate_contour = contour
                            .iter()
                            .map(|p| format!("{},{}", p[0], p[1]))
                            .collect::<Vec<_>>()
                            .join("; ");
                        self.plate_thickness = thickness.value().to_string();
                    }
                    let c = ui.label("Контур XY, мм");
                    ui.text_edit_multiline(&mut self.plate_contour)
                        .labelled_by(c.id);
                    let t = ui.label("Толщина, мм");
                    ui.text_edit_singleline(&mut self.plate_thickness)
                        .labelled_by(t.id);
                    if ui.button("Применить размеры").clicked() {
                        match parse_contour(&self.plate_contour).and_then(|contour| {
                            number(&self.plate_thickness)
                                .map(|thickness| Edit::PlateShape { contour, thickness })
                        }) {
                            Ok(edit) => action = Some(Action::Edit(edit)),
                            Err(e) => self.message = Some(e),
                        }
                    }
                });
            }
        }
        if let Some(result) = self
            .detailing
            .show(ui, editor.document.project(), &editor.selected)
        {
            match result {
                Ok(commands) => action = Some(Action::Detail(commands)),
                Err(e) => self.message = Some(e),
            }
        }
        if let Some(result) = self
            .connection
            .show(ui, editor.document.project(), &editor.selected)
        {
            match result {
                Ok(commands) => action = Some(Action::Connection(commands)),
                Err(e) => self.message = Some(e),
            }
        }
        self.coordination.show(ui, editor);
        ui.label("Свойство выбранных · ⌘L");
        let key_label = ui.label("Имя свойства");
        let key = ui
            .add(egui::TextEdit::singleline(&mut self.key).id(egui::Id::new("property key")))
            .labelled_by(key_label.id);
        if std::mem::take(&mut self.focus_property) {
            key.request_focus();
        }
        let value_label = ui.label("Значение (Enter — применить)");
        let value = ui
            .add(egui::TextEdit::singleline(&mut self.value).id(egui::Id::new("property value")))
            .labelled_by(value_label.id);
        let enter = value.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if ui
            .add_enabled(
                !editor.selected.is_empty(),
                egui::Button::new("Применить к выбранным"),
            )
            .clicked()
            || (enter && !editor.selected.is_empty())
        {
            match self.property_action() {
                Ok(a) => action = Some(a),
                Err(e) => self.message = Some(e),
            }
        }
        ui.separator();
        let path_label = ui.label("Файл проекта .obim");
        ui.text_edit_singleline(&mut self.path)
            .labelled_by(path_label.id);
        ui.horizontal(|ui| {
            if ui.button("Сохранить").clicked() {
                action = Some(Action::Save);
            }
            if ui.button("Открыть").clicked() {
                action = Some(Action::Open);
            }
        });
        ui.horizontal(|ui| {
            if ui
                .button("Экспорт IFC4")
                .on_hover_text("Файл с тем же именем и расширением .ifc")
                .clicked()
            {
                action = Some(Action::ExportIfc);
            }
            if ui.button("Импорт IFC4").clicked() {
                action = Some(Action::ImportIfc);
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
