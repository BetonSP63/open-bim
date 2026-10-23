//! Windows of the «Анализ» tab: analysis models, load groups, loads (the
//! values of the next load and the list of loads) and the analysis
//! properties of parts. Edits go through `interaction::analysis`.
use super::{Action, Request, Workbench};
use crate::interaction::analysis as an;
use crate::interaction::trim::{TrimPicker, TrimTool};
use eframe::egui;
use open_bim_model::{LoadKind, LoadShape, Members, PartAnalysis, PartRole};

/// Project revision, history length and active model of a built model.
type CacheKey = (u64, usize, Option<u32>);

/// Kind of the next load placed by «Загрузить».
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LoadForm {
    #[default]
    Point,
    Line,
    Area,
}

impl LoadForm {
    pub fn tool(self) -> TrimTool {
        match self {
            Self::Point => TrimTool::PointLoad,
            Self::Line => TrimTool::LineLoad,
            Self::Area => TrimTool::AreaLoad,
        }
    }
    pub fn unit(self) -> &'static str {
        match self {
            Self::Point => "кН",
            Self::Line => "кН/м",
            Self::Area => "кН/м²",
        }
    }
}

pub struct AnalysisUi {
    pub models_open: bool,
    pub groups_open: bool,
    pub load_open: bool,
    pub loads_open: bool,
    pub part_open: bool,
    /// Model shown and used for links and merges.
    pub active: Option<u32>,
    /// Analytical model drawn over the view.
    pub show: bool,
    pub node_numbers: bool,
    pub bar_numbers: bool,
    pub form: LoadForm,
    /// Next load: group and value along X, Y, Z (and Z at the end for a
    /// linear load, empty = uniform).
    pub group: u32,
    pub value: [String; 3],
    pub value_end: String,
    /// Load edited in «Свойства нагрузки».
    pub load: Option<u32>,
    pub part: PartAnalysis,
    pub offset: [String; 2],
    pub message: Option<String>,
    /// Analytical model drawn, with the state it was built for.
    cache: Option<(CacheKey, std::sync::Arc<open_bim_analysis::Analytical>)>,
}

impl Default for AnalysisUi {
    fn default() -> Self {
        Self {
            models_open: false,
            groups_open: false,
            load_open: false,
            loads_open: false,
            part_open: false,
            active: None,
            show: false,
            node_numbers: false,
            bar_numbers: false,
            form: LoadForm::Point,
            group: 1,
            value: [String::new(), String::new(), "-10".into()],
            value_end: String::new(),
            load: None,
            part: PartAnalysis::default(),
            offset: ["0".into(), "0".into()],
            message: None,
            cache: None,
        }
    }
}

impl AnalysisUi {
    /// The load shape for the picked points, with the values of the window.
    pub fn shape(&self, points: &[[f64; 3]]) -> Result<LoadShape, String> {
        let num = |s: &str| -> Result<f64, String> {
            if s.trim().is_empty() {
                Ok(0.)
            } else {
                crate::tools::number(s)
            }
        };
        let value = [
            num(&self.value[0])?,
            num(&self.value[1])?,
            num(&self.value[2])?,
        ];
        if value == [0.; 3] {
            return Err("Задайте величину нагрузки".into());
        }
        Ok(match (self.form, points) {
            (LoadForm::Point, [at]) => LoadShape::Point {
                at: *at,
                force: value,
            },
            (LoadForm::Line, [a, b]) => {
                let end = if self.value_end.trim().is_empty() {
                    value
                } else {
                    let z = num(&self.value_end)?;
                    let k = if value[2] != 0. { z / value[2] } else { 1. };
                    [value[0] * k, value[1] * k, z]
                };
                LoadShape::Line {
                    a: *a,
                    b: *b,
                    start: value,
                    end,
                }
            }
            (LoadForm::Area, polygon) if polygon.len() >= 3 => LoadShape::Area {
                polygon: polygon.to_vec(),
                pressure: value,
            },
            _ => return Err("Неверное число точек нагрузки".into()),
        })
    }
}

impl Workbench {
    /// The analytical model of the active analysis model, rebuilt when the
    /// model changes.
    pub fn analytical(&mut self) -> std::sync::Arc<open_bim_analysis::Analytical> {
        let project = self.editor.document.project();
        let key = (
            project.revision,
            self.editor.events.len(),
            self.analysis.active,
        );
        if let Some((k, a)) = &self.analysis.cache
            && *k == key
        {
            return a.clone();
        }
        let built = std::sync::Arc::new(open_bim_analysis::build(
            project,
            &an::model(project, self.analysis.active),
        ));
        self.analysis.cache = Some((key, built.clone()));
        built
    }

    fn commit_analysis(
        &mut self,
        requests: &mut Vec<Request>,
        label: &str,
        result: Result<Vec<open_bim_commands::Command>, String>,
    ) {
        match result {
            Ok(commands) => {
                requests.push(Request::Action(Action::Commit(label.into(), commands)));
                self.analysis.message = None;
            }
            Err(e) => self.analysis.message = Some(e),
        }
    }

    /// Start placing loads of the window's kind.
    pub fn start_load(&mut self) {
        self.engine.cancel();
        self.detail.cancel();
        self.trim = TrimPicker::start(self.analysis.form.tool());
        self.message(self.prompt());
    }

    pub(super) fn analysis_windows(&mut self, ctx: &egui::Context, requests: &mut Vec<Request>) {
        self.models_window(ctx, requests);
        self.groups_window(ctx, requests);
        self.load_window(ctx);
        self.loads_window(ctx, requests);
        self.part_window(ctx, requests);
    }

    fn models_window(&mut self, ctx: &egui::Context, requests: &mut Vec<Request>) {
        if !self.analysis.models_open {
            return;
        }
        let project = self.editor.document.project().clone();
        let mut model = an::model(&project, self.analysis.active);
        let stored = project
            .analysis
            .models
            .iter()
            .any(|m| m.number == model.number);
        let built = open_bim_analysis::build(&project, &model);
        let mut open = true;
        let (mut add, mut remove, mut save, mut export, mut take) =
            (false, false, false, false, false);
        let ui_state = &mut self.analysis;
        egui::Window::new("Расчётные и проектные модели")
            .id(egui::Id::new("analysis models"))
            .open(&mut open)
            .default_width(460.)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for m in &project.analysis.models {
                        if ui
                            .selectable_label(m.number == model.number, &m.name)
                            .clicked()
                        {
                            ui_state.active = Some(m.number);
                        }
                    }
                    add = ui.button("Новая").clicked();
                    remove = ui
                        .add_enabled(stored, egui::Button::new("Удалить"))
                        .clicked();
                });
                ui.separator();
                egui::Grid::new("analysis model grid")
                    .num_columns(2)
                    .show(ui, |ui| {
                        let l = ui.label("Имя");
                        save |= ui
                            .text_edit_singleline(&mut model.name)
                            .labelled_by(l.id)
                            .lost_focus();
                        ui.end_row();
                        let l = ui.label("Детали");
                        ui.horizontal(|ui| {
                            let all = matches!(model.members, Members::All);
                            if ui.radio(all, "Все стержни").clicked() && !all {
                                model.members = Members::All;
                                save = true;
                            }
                            let listed = match &model.members {
                                Members::Listed { parts } => parts.len(),
                                Members::All => 0,
                            };
                            if ui.radio(!all, format!("Выбранные ({listed})")).clicked() {
                                take = true;
                            }
                            if !all && ui.button("Взять выбранные").clicked() {
                                take = true;
                            }
                        })
                        .response
                        .labelled_by(l.id);
                        ui.end_row();
                        let l = ui.label("Допуск узлов, мм");
                        save |= ui
                            .add(
                                egui::DragValue::new(&mut model.merge_tolerance).range(0.1..=1000.),
                            )
                            .labelled_by(l.id)
                            .changed();
                        ui.end_row();
                    });
                if !project.analysis.load_groups.is_empty() {
                    ui.label("Группы нагрузок в модели (ни одной — все):");
                    ui.horizontal_wrapped(|ui| {
                        for g in &project.analysis.load_groups {
                            let mut on = model.load_groups.contains(&g.number);
                            if ui.checkbox(&mut on, &g.name).changed() {
                                if on {
                                    model.load_groups.insert(g.number);
                                } else {
                                    model.load_groups.remove(&g.number);
                                }
                                save = true;
                            }
                        }
                    });
                }
                ui.separator();
                ui.label(format!(
                    "Узлов: {}, элементов: {}, жёстких связей: {}, нагрузок: {}",
                    built.nodes.len(),
                    built.bars.len(),
                    built.links.len(),
                    built.loads.len()
                ));
                if built.plates > 0 {
                    ui.weak(format!(
                        "Пластин в модели: {} — в стержневую модель не входят",
                        built.plates
                    ));
                }
                ui.horizontal(|ui| {
                    ui.checkbox(&mut ui_state.show, "Показать в модели");
                    ui.checkbox(&mut ui_state.node_numbers, "Номера узлов");
                    ui.checkbox(&mut ui_state.bar_numbers, "Номера элементов");
                });
                export = ui.button("Экспорт в CSV").clicked();
                if let Some(m) = &ui_state.message {
                    ui.label(m);
                }
            });
        self.analysis.models_open = open;
        if take {
            model.members = Members::Listed {
                parts: self
                    .editor
                    .selected
                    .iter()
                    .copied()
                    .filter(|id| project.elements.contains_key(id))
                    .collect(),
            };
            save = true;
        }
        if add {
            match an::add_model(&project) {
                Ok((commands, n)) => {
                    self.analysis.active = Some(n);
                    requests.push(Request::Action(Action::Commit(
                        "Расчётная модель".into(),
                        commands,
                    )));
                }
                Err(e) => self.analysis.message = Some(e),
            }
            return;
        }
        if remove {
            self.analysis.active = None;
            let r = an::remove_model(&project, model.number);
            self.commit_analysis(requests, "Удалить расчётную модель", r);
            return;
        }
        if save && model.name.trim().is_empty() {
            self.analysis.message = Some("Имя модели не может быть пустым".into());
        } else if save
            && (!stored
                || project
                    .analysis
                    .models
                    .iter()
                    .any(|m| m.number == model.number && *m != model))
        {
            self.analysis.active = Some(model.number);
            let r = an::put_model(&project, model.clone());
            self.commit_analysis(requests, "Расчётная модель", r);
        }
        if export {
            self.analysis.message = Some(match an::export(&project, &model, &self.tools.path) {
                Ok(folder) => format!("Записано в {}", folder.display()),
                Err(e) => e,
            });
        }
    }

    fn groups_window(&mut self, ctx: &egui::Context, requests: &mut Vec<Request>) {
        if !self.analysis.groups_open {
            return;
        }
        let project = self.editor.document.project().clone();
        let mut groups = project.analysis.load_groups.clone();
        let mut open = true;
        let (mut add, mut remove, mut changed) = (None, None, None);
        let current = &mut self.analysis.group;
        egui::Window::new("Группы нагрузок")
            .id(egui::Id::new("load groups"))
            .open(&mut open)
            .default_width(520.)
            .show(ctx, |ui| {
                egui::Grid::new("load groups grid")
                    .num_columns(5)
                    .striped(true)
                    .show(ui, |ui| {
                        for h in ["Текущая", "Имя", "Вид", "γf", ""] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        for (i, g) in groups.iter_mut().enumerate() {
                            ui.radio_value(current, g.number, g.number.to_string());
                            if ui.text_edit_singleline(&mut g.name).lost_focus() {
                                changed = Some(i);
                            }
                            egui::ComboBox::from_id_salt(("load kind", g.number))
                                .selected_text(g.kind.label())
                                .show_ui(ui, |ui| {
                                    for k in LoadKind::ALL {
                                        if ui.selectable_value(&mut g.kind, k, k.label()).clicked() {
                                            g.factor = k.factor();
                                            changed = Some(i);
                                        }
                                    }
                                });
                            if ui
                                .add(egui::DragValue::new(&mut g.factor).speed(0.01).range(0.1..=10.))
                                .changed()
                            {
                                changed = Some(i);
                            }
                            if ui.button("Удалить").clicked() {
                                remove = Some(g.number);
                            }
                            ui.end_row();
                        }
                    });
                ui.horizontal(|ui| {
                    for k in [LoadKind::Dead, LoadKind::Long, LoadKind::Short, LoadKind::Snow, LoadKind::Wind] {
                        if ui.button(format!("+ {}", k.label())).clicked() {
                            add = Some(k);
                        }
                    }
                });
                ui.small("Новые нагрузки попадают в текущую группу. Удаление группы удаляет её нагрузки.");
            });
        self.analysis.groups_open = open;
        if let Some(kind) = add {
            match an::add_group(&project, kind) {
                Ok((commands, n)) => {
                    self.analysis.group = n;
                    requests.push(Request::Action(Action::Commit(
                        "Группа нагрузок".into(),
                        commands,
                    )));
                }
                Err(e) => self.analysis.message = Some(e),
            }
        } else if let Some(n) = remove {
            let r = an::remove_group(&project, n);
            self.commit_analysis(requests, "Удалить группу нагрузок", r);
        } else if let Some(i) = changed
            && groups[i] != project.analysis.load_groups[i]
        {
            let r = an::put_group(&project, groups[i].clone());
            self.commit_analysis(requests, "Группа нагрузок", r);
        }
    }

    fn load_window(&mut self, ctx: &egui::Context) {
        if !self.analysis.load_open {
            return;
        }
        let project = self.editor.document.project();
        let mut open = true;
        let mut restart = false;
        let a = &mut self.analysis;
        egui::Window::new("Загрузить")
            .id(egui::Id::new("add load"))
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for (f, label) in [
                        (LoadForm::Point, "Сосредоточенная"),
                        (LoadForm::Line, "Линейная"),
                        (LoadForm::Area, "По площади"),
                    ] {
                        restart |= ui.radio_value(&mut a.form, f, label).clicked();
                    }
                });
                let group_name = project
                    .analysis
                    .load_groups
                    .iter()
                    .find(|g| g.number == a.group)
                    .map_or(
                        "будет создана «Постоянная 1»".to_string(),
                        |g| g.name.clone(),
                    );
                egui::ComboBox::new("load group", "Группа")
                    .selected_text(group_name)
                    .show_ui(ui, |ui| {
                        for g in &project.analysis.load_groups {
                            ui.selectable_value(&mut a.group, g.number, &g.name);
                        }
                    });
                egui::Grid::new("load values")
                    .num_columns(2)
                    .show(ui, |ui| {
                        let unit = a.form.unit();
                        for (k, axis) in ["X", "Y", "Z"].iter().enumerate() {
                            let l = ui.label(format!("{axis}, {unit}"));
                            ui.text_edit_singleline(&mut a.value[k]).labelled_by(l.id);
                            ui.end_row();
                        }
                        if a.form == LoadForm::Line {
                            let l = ui.label(format!("Z в конце, {unit} (пусто — равномерная)"));
                            ui.text_edit_singleline(&mut a.value_end).labelled_by(l.id);
                            ui.end_row();
                        }
                    });
                ui.small("Вниз — отрицательное Z. Укажите точки в модели.");
                if let Some(m) = &a.message {
                    ui.label(m);
                }
            });
        self.analysis.load_open = open;
        if restart {
            self.start_load();
        }
        if !open
            && matches!(
                self.trim.tool,
                Some(TrimTool::PointLoad | TrimTool::LineLoad | TrimTool::AreaLoad)
            )
        {
            self.trim.cancel();
        }
    }

    fn loads_window(&mut self, ctx: &egui::Context, requests: &mut Vec<Request>) {
        if !self.analysis.loads_open {
            return;
        }
        let project = self.editor.document.project().clone();
        let loads = &project.analysis.loads;
        if self
            .analysis
            .load
            .is_none_or(|n| !loads.iter().any(|l| l.number == n))
        {
            self.analysis.load = loads.first().map(|l| l.number);
        }
        let mut edited = self
            .analysis
            .load
            .and_then(|n| loads.iter().find(|l| l.number == n))
            .cloned();
        let mut open = true;
        let (mut apply, mut delete) = (false, false);
        let chosen = &mut self.analysis.load;
        egui::Window::new("Свойства нагрузки")
            .id(egui::Id::new("load properties"))
            .open(&mut open)
            .default_width(460.)
            .show(ctx, |ui| {
                if loads.is_empty() {
                    ui.weak("Нагрузок нет: «Загрузить» на вкладке «Анализ»");
                    return;
                }
                egui::ScrollArea::vertical()
                    .max_height(160.)
                    .show(ui, |ui| {
                        for l in loads {
                            let kind = match l.shape {
                                LoadShape::Point { .. } => "сосредоточенная",
                                LoadShape::Line { .. } => "линейная",
                                LoadShape::Area { .. } => "по площади",
                            };
                            let group = project
                                .analysis
                                .load_groups
                                .iter()
                                .find(|g| g.number == l.group)
                                .map_or("?", |g| g.name.as_str());
                            if ui
                                .selectable_label(
                                    *chosen == Some(l.number),
                                    format!("{} — {kind}, {group}", l.number),
                                )
                                .clicked()
                            {
                                *chosen = Some(l.number);
                            }
                        }
                    });
                let Some(l) = &mut edited else { return };
                ui.separator();
                egui::ComboBox::new("edited load group", "Группа")
                    .selected_text(
                        project
                            .analysis
                            .load_groups
                            .iter()
                            .find(|g| g.number == l.group)
                            .map_or(String::new(), |g| g.name.clone()),
                    )
                    .show_ui(ui, |ui| {
                        for g in &project.analysis.load_groups {
                            ui.selectable_value(&mut l.group, g.number, &g.name);
                        }
                    });
                let vector = |ui: &mut egui::Ui, title: &str, v: &mut [f64; 3]| {
                    ui.horizontal(|ui| {
                        ui.label(title);
                        for c in v.iter_mut() {
                            ui.add(egui::DragValue::new(c).speed(0.1));
                        }
                    });
                };
                match &mut l.shape {
                    LoadShape::Point { force, .. } => vector(ui, "Сила X Y Z, кН", force),
                    LoadShape::Line { start, end, .. } => {
                        vector(ui, "В начале X Y Z, кН/м", start);
                        vector(ui, "В конце X Y Z, кН/м", end);
                    }
                    LoadShape::Area { pressure, .. } => {
                        vector(ui, "Давление X Y Z, кН/м²", pressure)
                    }
                }
                ui.horizontal(|ui| {
                    apply = ui.button("Изменить").clicked();
                    delete = ui.button("Удалить").clicked();
                });
            });
        self.analysis.loads_open = open;
        if let Some(l) = edited {
            if apply {
                let r = an::put_load(&project, l);
                self.commit_analysis(requests, "Свойства нагрузки", r);
            } else if delete {
                let r = an::remove_load(&project, l.number);
                self.commit_analysis(requests, "Удалить нагрузку", r);
            }
        }
    }

    fn part_window(&mut self, ctx: &egui::Context, requests: &mut Vec<Request>) {
        if !self.analysis.part_open {
            return;
        }
        let project = self.editor.document.project().clone();
        let mut open = true;
        let (mut apply, mut load) = (false, false);
        let a = &mut self.analysis;
        let count = self
            .editor
            .selected
            .iter()
            .filter(|id| project.elements.contains_key(id))
            .count();
        egui::Window::new("Свойства детали в расчётах")
            .id(egui::Id::new("analysis part"))
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(format!("Выбрано деталей: {count}"));
                egui::ComboBox::new("part role", "Роль")
                    .selected_text(a.part.role.label())
                    .show_ui(ui, |ui| {
                        for r in PartRole::ALL {
                            ui.selectable_value(&mut a.part.role, r, r.label());
                        }
                    });
                ui.horizontal(|ui| {
                    ui.checkbox(&mut a.part.hinges[0], "Шарнир в начале");
                    ui.checkbox(&mut a.part.hinges[1], "Шарнир в конце");
                });
                egui::Grid::new("part offset")
                    .num_columns(2)
                    .show(ui, |ui| {
                        for (k, axis) in [
                            "Смещение оси по x профиля, мм",
                            "Смещение оси по y профиля, мм",
                        ]
                        .iter()
                        .enumerate()
                        {
                            let l = ui.label(*axis);
                            ui.text_edit_singleline(&mut a.offset[k]).labelled_by(l.id);
                            ui.end_row();
                        }
                    });
                ui.horizontal(|ui| {
                    load = ui.button("Взять у выбранной").clicked();
                    apply = ui.button("Изменить").clicked();
                });
                if let Some(m) = &a.message {
                    ui.label(m);
                }
            });
        self.analysis.part_open = open;
        if load {
            let props = self
                .editor
                .selected
                .iter()
                .find_map(|id| project.analysis.parts.get(id))
                .cloned()
                .unwrap_or_default();
            self.analysis.offset = props.offset.map(open_bim_reports::fmt);
            self.analysis.part = props;
        }
        if apply {
            let offset = [0, 1].map(|k| crate::tools::number(&self.analysis.offset[k]));
            let r = match offset {
                [Ok(x), Ok(y)] => {
                    let mut props = self.analysis.part.clone();
                    props.offset = [x, y];
                    an::set_parts(&project, &self.editor.selected, &props)
                }
                [Err(e), _] | [_, Err(e)] => Err(e),
            };
            self.commit_analysis(requests, "Свойства детали в расчётах", r);
        }
    }
}
