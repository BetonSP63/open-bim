use eframe::egui;
use open_bim_commands::Command;
use open_bim_model::{ConnectionInstance, ConnectionTemplate, ObjectId, Project, TemplateKind};
use std::collections::BTreeSet;
use std::io::Read;

pub struct ConnectionUi {
    pub editing: Option<ObjectId>,
    pub primary: Option<ObjectId>,
    pub secondary: Option<ObjectId>,
    pub at_end: bool,
    pub template: ConnectionTemplate,
    pub path: String,
    pub message: Option<String>,
    pub loaded: Option<ConnectionInstance>,
}
impl Default for ConnectionUi {
    fn default() -> Self {
        Self {
            editing: None,
            primary: None,
            secondary: None,
            at_end: false,
            template: ConnectionTemplate {
                version: 1,
                name: "Соединение".into(),
                kind: TemplateKind::BasePlate { overhang: 70. },
                thickness: 20.,
                edge_distance: 45.,
                bolt_diameter: 20.,
                clearance: 2.,
                bolt_standard: open_bim_connections::norms::DEFAULT_BOLT_STANDARD.into(),
                bolt_grade: "8.8".into(),
                weld_size: 6.,
            },
            path: "connection-template.json".into(),
            message: None,
            loaded: None,
        }
    }
}
impl ConnectionUi {
    pub fn commands(&self, p: &Project) -> Result<Vec<Command>, String> {
        self.template.validate().map_err(|e| e.to_string())?;
        let primary = self.primary.ok_or("Выберите основной элемент")?;
        let secondary = if matches!(self.template.kind, TemplateKind::BasePlate { .. }) {
            None
        } else {
            Some(self.secondary.ok_or("Выберите опору")?)
        };
        let mut connection = if let Some(id) = self.editing {
            p.connections
                .get(&id)
                .ok_or("Соединение не найдено")?
                .clone()
        } else {
            ConnectionInstance {
                id: ObjectId(p.next_id),
                primary,
                secondary,
                at_end: self.at_end,
                template: self.template.clone(),
                children: Default::default(),
            }
        };
        connection.primary = primary;
        connection.secondary = secondary;
        connection.at_end = self.at_end;
        connection.template = self.template.clone();
        Ok(vec![if self.editing.is_some() {
            Command::ReplaceConnection { connection }
        } else {
            Command::CreateConnection { connection }
        }])
    }
    pub fn load(&mut self, p: &Project, id: ObjectId) -> Result<(), String> {
        let c = p.connections.get(&id).ok_or("Соединение не найдено")?;
        self.editing = Some(id);
        self.primary = Some(c.primary);
        self.secondary = c.secondary;
        self.at_end = c.at_end;
        self.template = c.template.clone();
        self.loaded = Some(c.clone());
        Ok(())
    }
    pub fn sync(&mut self, p: &Project) {
        if let Some(id) = self.editing {
            if let Some(c) = p.connections.get(&id) {
                if self.loaded.as_ref() != Some(c) {
                    let _ = self.load(p, id);
                }
            } else {
                self.editing = None;
                self.loaded = None;
            }
        }
    }
    pub fn export_template(&self) -> Result<(), String> {
        let json = self.template.to_json().map_err(|e| e.to_string())?;
        std::fs::write(&self.path, json).map_err(|e| e.to_string())
    }
    pub fn import_template(&mut self) -> Result<(), String> {
        let mut bytes = Vec::new();
        std::fs::File::open(&self.path)
            .map_err(|e| e.to_string())?
            .take(65537)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        let template = ConnectionTemplate::from_json(&bytes).map_err(|e| e.to_string())?;
        self.template = template;
        Ok(())
    }
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        p: &Project,
        selected: &BTreeSet<ObjectId>,
    ) -> Option<Result<Vec<Command>, String>> {
        let mut result = None;
        ui.collapsing("Параметрические соединения", |ui| {
            self.sync(p);
            let mut chosen = self.editing;
            egui::ComboBox::new("connection-instance", "Узел")
                .selected_text(
                    self.editing
                        .and_then(|id| p.connections.get(&id))
                        .map(|c| format!("{} · {}", c.template.name, c.id))
                        .unwrap_or("Новый узел".into()),
                )
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut chosen, None, "Новый узел");
                    for c in p.connections.values() {
                        ui.selectable_value(
                            &mut chosen,
                            Some(c.id),
                            format!("{} · {}", c.template.name, c.id),
                        );
                    }
                });
            if chosen != self.editing {
                if let Some(id) = chosen {
                    if let Err(e) = self.load(p, id) {
                        self.message = Some(e);
                    }
                } else {
                    self.editing = None;
                    self.loaded = None;
                }
            }
            let owned: BTreeSet<_> = p
                .connections
                .values()
                .flat_map(|c| c.children.values().copied())
                .collect();
            if ui.button("Родители из выбора").clicked() {
                let sources: Vec<_> = selected
                    .iter()
                    .filter(|id| p.elements.contains_key(id) && !owned.contains(id))
                    .copied()
                    .collect();
                self.primary = sources.first().copied();
                self.secondary = sources.get(1).copied();
            }
            for (label, value) in [
                ("Основной элемент", &mut self.primary),
                ("Опора", &mut self.secondary),
            ] {
                egui::ComboBox::new(label, label)
                    .selected_text(
                        value
                            .and_then(|id| p.elements.get(&id))
                            .map(|e| format!("{} · {}", e.name, e.id))
                            .unwrap_or(label.into()),
                    )
                    .show_ui(ui, |ui| {
                        ui.selectable_value(value, None, "Не выбран");
                        for e in p.elements.values().filter(|e| !owned.contains(&e.id)) {
                            ui.selectable_value(
                                value,
                                Some(e.id),
                                format!("{} · {}", e.name, e.id),
                            );
                        }
                    });
            }
            ui.checkbox(&mut self.at_end, "Конец элемента (иначе начало)");
            // Types come from the component catalogue (same names and defaults).
            let catalog = crate::ui::components::catalog();
            let same = |a: &TemplateKind, b: &TemplateKind| {
                std::mem::discriminant(a) == std::mem::discriminant(b)
            };
            let old = catalog
                .iter()
                .position(|c| same(&c.kind, &self.template.kind))
                .unwrap_or(0);
            let mut kind = old;
            egui::ComboBox::new("connection-kind", "Тип узла")
                .selected_text(catalog[kind].name.as_str())
                .show_ui(ui, |ui| {
                    for (i, c) in catalog.iter().enumerate() {
                        ui.selectable_value(&mut kind, i, c.name.as_str());
                    }
                });
            if old != kind {
                self.template.kind = catalog[kind].kind.clone();
                self.template.thickness = catalog[kind].thickness;
            }
            let field_label = ui.label("Имя узла");
            ui.text_edit_singleline(&mut self.template.name)
                .labelled_by(field_label.id);
            fn dimension(ui: &mut egui::Ui, label: &str, value: &mut f64) {
                ui.horizontal(|ui| {
                    ui.label(label);
                    ui.add(egui::DragValue::new(value).speed(1.));
                });
            }
            match &mut self.template.kind {
                TemplateKind::BasePlate { overhang } | TemplateKind::EndPlate { overhang } => {
                    dimension(ui, "Свес, мм", overhang)
                }
                TemplateKind::BeamSplice { rows, pitch } => {
                    dimension(ui, "Шаг болтов, мм", pitch);
                    ui.horizontal(|ui| {
                        ui.label("Болтов по высоте стенки");
                        ui.add(egui::DragValue::new(rows).range(1..=12));
                    });
                }
                TemplateKind::Gusset { bolts, pitch } => {
                    dimension(ui, "Шаг болтов, мм", pitch);
                    ui.horizontal(|ui| {
                        ui.label("Болтов в ряду");
                        ui.add(egui::DragValue::new(bolts).range(1..=12));
                    });
                }
                TemplateKind::Haunch { length, depth } => {
                    dimension(ui, "Длина вуты вдоль балки, мм", length);
                    dimension(ui, "Высота вуты под балкой, мм", depth);
                }
                TemplateKind::ColumnStiffeners { chamfer } => {
                    dimension(ui, "Срез угла у стенки, мм", chamfer);
                }
                TemplateKind::ClipAngle { leg, height, pitch } => {
                    dimension(ui, "Полка уголка, мм", leg);
                    dimension(ui, "Высота уголка, мм", height);
                    dimension(ui, "Шаг болтов, мм", pitch);
                }
                TemplateKind::FinPlate {
                    height,
                    lap,
                    gap,
                    pitch,
                    rows,
                } => {
                    dimension(ui, "Высота пластины, мм", height);
                    dimension(ui, "Нахлёст на балку, мм", lap);
                    dimension(ui, "Зазор до опоры, мм", gap);
                    dimension(ui, "Шаг болтов, мм", pitch);
                    ui.horizontal(|ui| {
                        ui.label("Болтов в ряду");
                        ui.add(egui::DragValue::new(rows).range(1..=12));
                    });
                }
            }
            dimension(ui, "Толщина, мм", &mut self.template.thickness);
            // Stiffeners and haunches are welded only: no bolt fields.
            if !matches!(
                self.template.kind,
                TemplateKind::ColumnStiffeners { .. } | TemplateKind::Haunch { .. }
            ) {
                dimension(ui, "Отступ от края, мм", &mut self.template.edge_distance);
                dimension(ui, "Диаметр болта, мм", &mut self.template.bolt_diameter);
                dimension(ui, "Зазор отверстия, мм", &mut self.template.clearance);
                let field_label = ui.label("Стандарт болта");
                ui.text_edit_singleline(&mut self.template.bolt_standard)
                    .labelled_by(field_label.id);
                let field_label = ui.label("Класс прочности");
                ui.text_edit_singleline(&mut self.template.bolt_grade)
                    .labelled_by(field_label.id);
            }
            if !matches!(self.template.kind, TemplateKind::ClipAngle { .. }) {
                dimension(ui, "Размер шва, мм", &mut self.template.weld_size);
            }
            if ui
                .button(if self.editing.is_some() {
                    "Перестроить узел"
                } else {
                    "Создать узел"
                })
                .clicked()
            {
                let command = self.commands(p);
                if command.is_ok() && self.editing.is_none() {
                    self.editing = Some(ObjectId(p.next_id));
                }
                result = Some(command);
            }
            let field_label = ui.label("Файл шаблона JSON");
            ui.text_edit_singleline(&mut self.path)
                .labelled_by(field_label.id);
            ui.horizontal(|ui| {
                if ui.button("Экспорт шаблона").clicked() {
                    self.message = Some(match self.export_template() {
                        Ok(()) => "Шаблон сохранён".into(),
                        Err(e) => e,
                    });
                }
                if ui.button("Импорт шаблона").clicked() {
                    self.message = Some(match self.import_template() {
                        Ok(()) => "Шаблон загружен; выберите родителей и примените".into(),
                        Err(e) => e,
                    });
                }
            });
            if let Some(message) = &self.message {
                ui.label(message);
            }
        });
        result
    }
}
