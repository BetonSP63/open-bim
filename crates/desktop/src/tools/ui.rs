use super::{Edit, Editor, SnapMode, ToolKind, number, parse_contour, point};
use eframe::egui;
use open_bim_model::{ObjectId, Point3, PropertyValue};
#[derive(Clone, Debug)]
pub enum Action {
    Create,
    Detail(Vec<open_bim_commands::Command>),
    Connection(Vec<open_bim_commands::Command>),
    Edit(Edit),
    /// Labelled transaction built by an interactive pick command.
    Commit(String, Vec<open_bim_commands::Command>),
    /// Follow-up changes added to the last undo step (placed plugins run
    /// again after their inputs changed).
    Amend(Vec<open_bim_commands::Command>),
    Undo,
    Redo,
    /// Several undo (false) or redo (true) steps from the undo history list.
    History(bool, usize),
    Save,
    Open,
    ExportIfc,
    ExportNc,
    ImportIfc,
    New,
    Example,
}
/// Side-pane section shown by the model tools panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ToolsPanel {
    #[default]
    Properties,
    Edit,
    Detailing,
    Connections,
    Clash,
}
impl ToolsPanel {
    pub fn title(self) -> &'static str {
        match self {
            Self::Properties => "Свойства",
            Self::Edit => "Правка выбранных",
            Self::Detailing => "Болты, отверстия и швы",
            Self::Connections => "Узлы",
            Self::Clash => "Нумерация и коллизии",
        }
    }
}
pub struct ToolsUi {
    pub panel: ToolsPanel,
    pub connection: super::connection_ui::ConnectionUi,
    pub coordination: super::coordination_ui::CoordinationUi,
    pub detailing: super::detailing_ui::DetailingUi,
    /// Property file row of the create and bolt dialogs.
    pub files: super::property_files::FilesUi,
    /// The bolt command ran in this session («standard» is loaded once).
    pub bolts_used: bool,
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
    /// Autosave of the project file and recovery after a crash.
    pub autosave: crate::autosave::Autosave,
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
            panel: ToolsPanel::Properties,
            connection: Default::default(),
            coordination: Default::default(),
            detailing: Default::default(),
            files: Default::default(),
            bolts_used: false,
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
            autosave: Default::default(),
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
    /// User-defined property of the selected objects (UDA), ⌘L focuses it.
    pub fn user_property(&mut self, ui: &mut egui::Ui, editor: &mut Editor) -> Option<Action> {
        let mut action = None;
        ui.label("Свойство выбранных · ⌘L");
        let mut key = None;
        let mut value = None;
        crate::ui::property_pane::rows(ui, "user property", |ui| {
            let key_label = ui.label("Имя свойства");
            key = Some(
                ui.add(
                    egui::TextEdit::singleline(&mut self.key)
                        .id(egui::Id::new("property key"))
                        .desired_width(crate::ui::property_pane::field_width(ui)),
                )
                .labelled_by(key_label.id),
            );
            ui.end_row();
            let value_label = ui
                .label("Значение")
                .on_hover_text("Enter — применить к выбранным");
            value = Some(
                ui.add(
                    egui::TextEdit::singleline(&mut self.value)
                        .id(egui::Id::new("property value"))
                        .desired_width(crate::ui::property_pane::field_width(ui)),
                )
                .labelled_by(value_label.id),
            );
            ui.end_row();
        });
        let (Some(key), Some(value)) = (key, value) else {
            return None;
        };
        if std::mem::take(&mut self.focus_property) {
            key.request_focus();
        }
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
        action
    }

    /// Properties of the part being created, in the sections of the pane of
    /// a selected part: the file row, «Общие», «Положение», the points, and
    /// «Создать» / «Отмена» at the bottom.
    pub fn draft(&mut self, ui: &mut egui::Ui, editor: &mut Editor) -> Option<Action> {
        use crate::ui::property_pane::{field_width, rows, section};
        let mut action = None;
        let kind = editor.draft.kind;
        if let Some(file) = super::property_files::FileKind::of_tool(kind) {
            let dir = super::property_files::folder(&self.path);
            if let Some(request) = self.files.row(ui, &dir, file) {
                self.message = Some(draft_file(&dir, file, editor, request));
            }
        }
        section("Общие").default_open(true).show(ui, |ui| {
            rows(ui, "draft general", |ui| {
                let l = ui.label("Имя");
                ui.add(
                    egui::TextEdit::singleline(&mut editor.draft.name)
                        .desired_width(field_width(ui)),
                )
                .labelled_by(l.id);
                ui.end_row();
                if matches!(kind, ToolKind::Beam | ToolKind::Column) {
                    let project = editor.document.project();
                    ui.label("Профиль");
                    let shown = project
                        .profiles
                        .get(&editor.draft.profile)
                        .map_or("Выберите профиль", |p| p.name.as_str());
                    let width = field_width(ui);
                    use crate::ui::catalog_picker::Pick;
                    match crate::ui::profile_dialog::profile_field(
                        ui,
                        "draft",
                        project,
                        Some(editor.draft.profile),
                        shown,
                        "Профиль",
                        width,
                    ) {
                        Some(Pick::Project(id)) => editor.draft.profile = id,
                        // A catalogue profile is copied into the project
                        // first (one undo step), then used.
                        Some(Pick::New(p)) => {
                            editor.draft.profile = p.id;
                            action = Some(Action::Commit(
                                format!("Профиль из каталога: {}", p.name),
                                vec![open_bim_commands::Command::CreateProfile { profile: p }],
                            ));
                        }
                        None => {}
                    }
                    ui.end_row();
                }
                if kind != ToolKind::Axis && !kind.is_concrete() {
                    let project = editor.document.project();
                    let l = ui.label("Материал");
                    egui::ComboBox::new("material", "")
                        .selected_text(
                            project
                                .materials
                                .get(&editor.draft.material)
                                .map_or("Выберите материал", |m| m.name.as_str()),
                        )
                        .width(field_width(ui))
                        .show_ui(ui, |ui| {
                            use crate::ui::catalog_picker::{Pick, material_menu};
                            match material_menu(ui, project, Some(editor.draft.material)) {
                                Some(Pick::Project(id)) => editor.draft.material = id,
                                Some(Pick::New(m)) => {
                                    editor.draft.material = m.id;
                                    action = Some(Action::Commit(
                                        format!("Материал из каталога: {}", m.name),
                                        vec![open_bim_commands::Command::CreateMaterial {
                                            material: m,
                                        }],
                                    ));
                                }
                                None => {}
                            }
                        })
                        .response
                        .labelled_by(l.id);
                    ui.end_row();
                }
                if kind.is_plate() {
                    let l = ui.label("Толщина, мм");
                    ui.add(
                        egui::TextEdit::singleline(&mut editor.draft.thickness)
                            .desired_width(field_width(ui)),
                    )
                    .labelled_by(l.id);
                    ui.end_row();
                }
                if kind.is_concrete() {
                    if !matches!(kind, ToolKind::Slab | ToolKind::Raft) {
                        let l = ui.label("Сечение b x h, мм");
                        ui.add(
                            egui::TextEdit::singleline(&mut editor.draft.section)
                                .desired_width(field_width(ui)),
                        )
                        .labelled_by(l.id);
                        ui.end_row();
                    }
                    let l = ui.label("Класс бетона");
                    egui::ComboBox::new("grade", "")
                        .selected_text(editor.draft.grade.as_str())
                        .width(field_width(ui))
                        .show_ui(ui, |ui| {
                            for c in open_bim_model::library::concrete_classes() {
                                ui.selectable_value(&mut editor.draft.grade, c.name.into(), c.name)
                                    .on_hover_text(format!(
                                        "Rb = {} МПа, Rbt = {} МПа, Eb = {} МПа (СП 63.13330)",
                                        open_bim_reports::fmt(c.rb),
                                        open_bim_reports::fmt(c.rbt),
                                        open_bim_reports::fmt(c.eb)
                                    ));
                            }
                        })
                        .response
                        .labelled_by(l.id);
                    ui.end_row();
                    if kind == ToolKind::Footing {
                        let l = ui.label("Подколонник b x h, мм");
                        ui.add(
                            egui::TextEdit::singleline(&mut editor.draft.pedestal)
                                .hint_text("нет")
                                .desired_width(field_width(ui)),
                        )
                        .labelled_by(l.id)
                        .on_hover_text("Пусто — подошва без подколонника");
                        ui.end_row();
                        let l = ui.label("Верх подколонника, мм");
                        ui.add(
                            egui::TextEdit::singleline(&mut editor.draft.pedestal_top)
                                .desired_width(field_width(ui)),
                        )
                        .labelled_by(l.id)
                        .on_hover_text("Отсчёт как у верха подошвы");
                        ui.end_row();
                    }
                    let l = ui.label("Тип элемента");
                    egui::ComboBox::new("cast type", "")
                        .selected_text(if editor.draft.precast {
                            "Сборный"
                        } else {
                            "Монолитный"
                        })
                        .width(field_width(ui))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut editor.draft.precast, false, "Монолитный");
                            ui.selectable_value(&mut editor.draft.precast, true, "Сборный");
                        })
                        .response
                        .labelled_by(l.id);
                    ui.end_row();
                }
            });
        });
        if matches!(kind, ToolKind::Beam | ToolKind::Column) {
            section("Положение").default_open(true).show(ui, |ui| {
                crate::ui::property_pane::position_editor(ui, "draft", &mut editor.draft.position);
            });
            section("Сдвоенный профиль")
                .default_open(editor.draft.twin != super::twin::Twin::Single)
                .show(ui, |ui| twin_editor(ui, &mut editor.draft));
        }
        if kind.is_plate() {
            section("Контур").default_open(true).show(ui, |ui| {
                use crate::interaction::polygon::{PlateMode, PlatePosition};
                rows(ui, "draft plate", |ui| {
                    let l = ui.label("Способ построения");
                    egui::ComboBox::new("plate mode", "")
                        .selected_text(editor.draft.plate_mode.label())
                        .width(field_width(ui))
                        .show_ui(ui, |ui| {
                            for m in PlateMode::ALL {
                                ui.selectable_value(&mut editor.draft.plate_mode, m, m.label());
                            }
                        })
                        .response
                        .labelled_by(l.id);
                    ui.end_row();
                    let l = ui.label("Положение по толщине");
                    egui::ComboBox::new("plate position", "")
                        .selected_text(editor.draft.plate_position.label())
                        .width(field_width(ui))
                        .show_ui(ui, |ui| {
                            for m in PlatePosition::ALL {
                                ui.selectable_value(&mut editor.draft.plate_position, m, m.label());
                            }
                        })
                        .response
                        .labelled_by(l.id);
                    ui.end_row();
                });
                let contour_label = ui.label("Контур XY: X,Y; X,Y; …");
                ui.add(
                    egui::TextEdit::multiline(&mut editor.draft.contour)
                        .desired_rows(2)
                        .desired_width(f32::INFINITY),
                )
                .labelled_by(contour_label.id);
                ui.small("Точки — в 3D-виде с привязкой; Enter или средняя кнопка замыкает контур, Backspace убирает последнюю точку.");
            });
        }
        let points = if kind.base() == ToolKind::Column {
            "Низ и верх"
        } else {
            "Точки"
        };
        section(points)
            .default_open(kind.base() == ToolKind::Column)
            .show(ui, |ui| {
                ui.label(if kind.is_plate() {
                    "Начало плиты, мм"
                } else {
                    "Начало, мм"
                });
                vector_fields(ui, "start", &mut editor.draft.start);
                if kind.base() == ToolKind::Column {
                    let project = editor.document.project();
                    column_ends(ui, project, &mut editor.draft);
                } else if !kind.is_plate() {
                    ui.label("Конец, мм");
                    vector_fields(ui, "end", &mut editor.draft.end);
                }
                ui.small("Точки удобнее указывать в 3D-виде; поля — для точного ввода.");
            });
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Создать · ⌘↩").clicked() {
                action = Some(Action::Create);
            }
            if ui.button("Отмена · Esc").clicked() {
                editor.cancel();
            }
        });
        if let Some(message) = &self.message {
            ui.label(message);
        }
        if let Some(warning) = &editor.record_warning {
            ui.colored_label(ui.visuals().warn_fg_color, warning);
        }
        action
    }

    pub fn show(&mut self, ui: &mut egui::Ui, editor: &mut Editor) -> Option<Action> {
        let mut action = None;
        // The property pane draws its own header and object type list.
        if self.panel != ToolsPanel::Properties {
            ui.heading(self.panel.title());
        }
        if self.panel == ToolsPanel::Edit {
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
        }
        if self.panel == ToolsPanel::Properties {
            ui.separator();
            if editor.selected.len() == 1 {
                let id = *editor.selected.first().unwrap();
                if let Some(element) = editor.document.project().elements.get(&id)
                    && let open_bim_model::ElementShape::Plate {
                        contour, thickness, ..
                    } = &element.shape
                {
                    ui.collapsing("Контур и толщина пластины", |ui| {
                        if ui.button("Загрузить размеры выбранной").clicked()
                        {
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
        }
        if self.panel == ToolsPanel::Detailing
            && let Some(result) =
                self.detailing
                    .show(ui, editor.document.project(), &editor.selected)
        {
            match result {
                Ok(commands) => action = Some(Action::Detail(commands)),
                Err(e) => self.message = Some(e),
            }
        }
        if self.panel == ToolsPanel::Connections
            && let Some(result) =
                self.connection
                    .show(ui, editor.document.project(), &editor.selected)
        {
            match result {
                Ok(commands) => action = Some(Action::Connection(commands)),
                Err(e) => self.message = Some(e),
            }
        }
        if self.panel == ToolsPanel::Clash {
            self.coordination.show(ui, editor);
        }
        if self.panel == ToolsPanel::Properties {
            if !editor.selected.is_empty()
                && let Some(a) = self.user_property(ui, editor)
            {
                action = Some(a);
            }
            ui.separator();
            ui.label(egui::RichText::new("Проект").strong());
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
                ui.checkbox(&mut self.autosave.enabled, "Автосохранение каждые")
                    .on_hover_text("Копия рядом с проектом: файл .obim.autosave");
                let minutes = ui.add_enabled(
                    self.autosave.enabled,
                    egui::DragValue::new(&mut self.autosave.minutes)
                        .range(1..=60)
                        .suffix(" мин"),
                );
                minutes.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::DragValue,
                        true,
                        "Интервал автосохранения, мин",
                    )
                });
            });
            ui.horizontal(|ui| {
                if ui
                    .button("Экспорт IFC4")
                    .on_hover_text("Файл с тем же именем и расширением .ifc")
                    .clicked()
                {
                    action = Some(Action::ExportIfc);
                }
                if ui
                    .button("Импорт IFC")
                    .on_hover_text("Выбрать файл IFC2x3 или IFC4; объекты добавляются в модель")
                    .clicked()
                {
                    action = Some(Action::ImportIfc);
                }
                if ui
                    .button("Экспорт NC")
                    .on_hover_text("Файлы DSTV .nc1 по маркам деталей в папке .nc рядом с проектом")
                    .clicked()
                {
                    action = Some(Action::ExportNc);
                }
            });
        }
        if let Some(message) = &self.message {
            ui.separator();
            ui.label(message);
        }
        if let Some(warning) = &editor.record_warning {
            ui.colored_label(ui.visuals().warn_fg_color, warning);
        }
        action
    }
}
/// A millimetre value as typed in a field: «0», «-150», «12.5».
pub fn mm_text(v: f64) -> String {
    crate::ui::property_pane::number_text(v)
}

/// Load or save the create dialog of `kind` from/to a property file;
/// returns the message for the user.
pub fn draft_file(
    dir: &std::path::Path,
    kind: super::property_files::FileKind,
    editor: &mut Editor,
    request: super::property_files::FileRequest,
) -> String {
    use super::property_files::{
        FileRequest, apply_to_draft, from_draft, import_catalogue, load, loaded_message, save,
    };
    match request {
        FileRequest::Load(name) => {
            match load(dir, kind, &name).and_then(|v| import_catalogue(editor, &v).map(|()| v)) {
                Ok(values) => {
                    let project = editor.document.project();
                    loaded_message(&name, &apply_to_draft(project, &mut editor.draft, &values))
                }
                Err(e) => e,
            }
        }
        FileRequest::Save(name) => {
            let project = editor.document.project();
            match save(dir, &name, &from_draft(project, &editor.draft, kind)) {
                Ok(path) => format!("Свойства сохранены: {}", path.display()),
                Err(e) => e,
            }
        }
    }
}

/// Column «Низ»/«Верх»: a level (or absolute Z) and the value in mm, which
/// is an offset from the level when one is chosen.
pub fn column_ends(ui: &mut egui::Ui, project: &open_bim_model::Project, draft: &mut super::Draft) {
    use open_bim_model::numbering_rules::level_mark;
    let mut levels: Vec<&open_bim_model::Level> = project.levels.values().collect();
    levels.sort_by(|a, b| a.elevation_mm.total_cmp(&b.elevation_mm));
    let name = |id: Option<ObjectId>| match id.and_then(|id| project.levels.get(&id)) {
        Some(l) if l.name == level_mark(l.elevation_mm) => l.name.clone(),
        Some(l) => format!("{} ({})", l.name, level_mark(l.elevation_mm)),
        None => "Абс. отметка".into(),
    };
    egui::Grid::new("column ends")
        .num_columns(3)
        .show(ui, |ui| {
            for (caption, level, value) in [
                ("Низ", &mut draft.bottom_level, &mut draft.bottom),
                ("Верх", &mut draft.top_level, &mut draft.top),
            ] {
                let l = ui.label(caption);
                egui::ComboBox::new(("column level", caption), "")
                    .selected_text(name(*level))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(level, None, name(None));
                        for l in &levels {
                            ui.selectable_value(level, Some(l.id), name(Some(l.id)));
                        }
                    })
                    .response
                    .labelled_by(l.id);
                let unit = if level.is_some() {
                    "смещение, мм"
                } else {
                    "Z, мм"
                };
                ui.add(
                    egui::TextEdit::singleline(value)
                        .id(egui::Id::new(("column end", caption)))
                        .desired_width(70.),
                )
                .on_hover_text(unit)
                .widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::TextEdit,
                        true,
                        format!("{caption}, {unit}"),
                    )
                });
                ui.label(unit);
                ui.end_row();
            }
        });
    match draft
        .column_ends()
        .and_then(|(b, t)| super::column_levels::heights(project, b, t))
    {
        Ok((b, t)) => {
            ui.small(format!(
                "Низ {} · верх {} · высота {:.0} мм",
                level_mark(b),
                level_mark(t),
                t - b
            ));
        }
        Err(e) => {
            ui.colored_label(ui.visuals().error_fg_color, e);
        }
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

/// «Сдвоенный профиль»: one or two members and the gap between them.
pub fn twin_editor(ui: &mut egui::Ui, draft: &mut crate::tools::Draft) {
    use crate::tools::twin::Twin;
    ui.horizontal(|ui| {
        let l = ui.label("Сдвоенный профиль");
        egui::ComboBox::new("twin", "")
            .selected_text(draft.twin.label())
            .show_ui(ui, |ui| {
                for t in Twin::ALL {
                    ui.selectable_value(&mut draft.twin, t, t.label());
                }
            })
            .response
            .labelled_by(l.id);
    });
    if draft.twin != Twin::Single {
        ui.horizontal(|ui| {
            let l = ui.label("Зазор, мм");
            ui.add(egui::TextEdit::singleline(&mut draft.twin_gap).desired_width(60.))
                .labelled_by(l.id);
        });
        ui.small("Вторая деталь зеркальна первой; зазор — под фасонку или прокладки.");
    }
}
