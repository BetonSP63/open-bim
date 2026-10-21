//! Component catalogue (Ctrl+F): the parametric connections in one
//! searchable list. Choosing a component arms it; the next clicks pick the
//! main part and, when the component needs one, the secondary part, and the
//! connection is created at the end of the main part nearer the secondary.
//! The command stays armed for the next joint until Esc.
use super::Workbench;
use crate::tools::ui::Action;
use eframe::egui;
use open_bim_model::{
    ConnectionInstance, ConnectionTemplate, ElementShape, ObjectId, Point3, Project, TemplateKind,
};
use open_bim_renderer::DVec3;

/// One catalogue entry.
#[derive(Clone, Debug, PartialEq)]
pub struct Component {
    pub name: String,
    pub group: String,
    pub description: String,
    pub needs_support: bool,
    pub kind: TemplateKind,
    pub thickness: f64,
    /// A saved user component carries all its parameters: bolts, welds and
    /// edge distances too. Built-in ones take those from the connection panel.
    pub template: Option<ConnectionTemplate>,
}

pub fn catalog() -> Vec<Component> {
    vec![
        Component {
            name: "Опорная плита".into(),
            group: "Колонны".into(),
            description: "Плита под колонной с четырьмя анкерными отверстиями и швом по контуру сечения. Выберите колонну.".into(),
            needs_support: false,
            kind: TemplateKind::BasePlate { overhang: 70. },
            thickness: 20.,
            template: None,
        },
        Component {
            name: "Торцевая пластина".into(),
            group: "Балка — колонна / балка".into(),
            description: "Пластина на торце балки на четырёх болтах к грани опоры. Выберите балку, затем опору.".into(),
            needs_support: true,
            kind: TemplateKind::EndPlate { overhang: 70. },
            thickness: 20.,
            template: None,
        },
        Component {
            name: "Шарнир на пластине".into(),
            group: "Балка — колонна / балка".into(),
            description: "Пластина приварена к опоре и прикреплена к стенке балки одним рядом болтов, с зазором между торцом балки и опорой. Выберите балку, затем опору.".into(),
            needs_support: true,
            kind: TemplateKind::FinPlate {
                height: 160.,
                lap: 90.,
                gap: 15.,
                pitch: 70.,
                rows: 2,
            },
            thickness: 10.,
            template: None,
        },
        Component {
            name: "Рёбра жёсткости колонны".into(),
            group: "Колонны".into(),
            description: "Четыре ребра между полками колонны на уровне полок балки, приварены к стенке и полкам. Выберите колонну, затем балку.".into(),
            needs_support: true,
            kind: TemplateKind::ColumnStiffeners { chamfer: 20. },
            thickness: 12.,
            template: None,
        },
        Component {
            name: "Стык балок на накладках".into(),
            group: "Балка — балка".into(),
            description: "Две балки встык на одной оси: накладки с двух сторон стенки и снаружи полок на болтах. Выберите одну балку, затем вторую.".into(),
            needs_support: true,
            kind: TemplateKind::BeamSplice {
                rows: 2,
                pitch: 70.,
            },
            thickness: 10.,
            template: None,
        },
        Component {
            name: "Вута".into(),
            group: "Балка — колонна / балка".into(),
            description: "Сварная вута под концом балки: треугольная стенка и наклонная полка, приварены к нижней полке балки и к опоре. Выберите балку, затем опору.".into(),
            needs_support: true,
            kind: TemplateKind::Haunch {
                length: 600.,
                depth: 300.,
            },
            thickness: 10.,
            template: None,
        },
        Component {
            name: "Фасонка связи".into(),
            group: "Связи".into(),
            description: "Фасонка за полкой уголковой связи: болты в ряд вдоль связи, шов к ближайшей грани опоры. Выберите связь, затем колонну или балку.".into(),
            needs_support: true,
            kind: TemplateKind::Gusset {
                bolts: 2,
                pitch: 70.,
            },
            thickness: 10.,
            template: None,
        },
        Component {
            name: "Уголковое соединение".into(),
            group: "Балка — колонна / балка".into(),
            description: "Уголок на двух болтах к стенке балки и двух болтах к опоре. Выберите балку, затем опору.".into(),
            needs_support: true,
            kind: TemplateKind::ClipAngle {
                leg: 100.,
                height: 170.,
                pitch: 80.,
            },
            thickness: 8.,
            template: None,
        },
    ]
}

/// Catalogue entries whose name, group or description contain every word.
pub fn search(query: &str) -> Vec<Component> {
    search_in(catalog(), query)
}

/// Entries of `list` whose name, group or description contain every word.
pub fn search_in(list: Vec<Component>, query: &str) -> Vec<Component> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    list.into_iter()
        .filter(|c| {
            let text = format!("{} {} {}", c.name, c.group, c.description).to_lowercase();
            words.iter().all(|w| text.contains(w.as_str()))
        })
        .collect()
}

/// Catalogue window and the armed component.
#[derive(Clone, Debug, Default)]
pub struct ComponentState {
    pub window: bool,
    pub query: String,
    /// Armed component and the main part picked so far.
    pub armed: Option<Component>,
    pub main: Option<ObjectId>,
}

impl ComponentState {
    pub fn picks_objects(&self) -> bool {
        self.armed.is_some()
    }

    pub fn prompt(&self) -> Option<String> {
        let c = self.armed.as_ref()?;
        Some(match self.main {
            None => format!("{}: выберите главную деталь", c.name),
            Some(_) => format!("{}: выберите второстепенную деталь", c.name),
        })
    }

    pub fn cancel(&mut self) -> bool {
        self.main = None;
        self.armed.take().is_some()
    }
}

fn v(p: Point3) -> DVec3 {
    DVec3::from_array(p.0)
}

/// Whether the connection goes at the end (not the start) of the main part:
/// the end nearer the secondary part's axis or plate.
pub fn at_end(project: &Project, main: ObjectId, support: Option<ObjectId>) -> bool {
    let Some(ElementShape::Member { length, frame, .. }) =
        project.elements.get(&main).map(|e| &e.shape)
    else {
        return false;
    };
    let (a, b) = (
        v(frame.origin),
        v(frame.world(Point3([0., 0., length.value()]))),
    );
    let Some(target) = support.and_then(|s| project.elements.get(&s)) else {
        // A column base goes at the lower end.
        return b.z < a.z;
    };
    let distance = |p: DVec3| match &target.shape {
        ElementShape::Member { length, frame, .. } => {
            let (s, e) = (
                v(frame.origin),
                v(frame.world(Point3([0., 0., length.value()]))),
            );
            let d = e - s;
            let t = ((p - s).dot(d) / d.dot(d).max(1e-12)).clamp(0., 1.);
            p.distance(s + d * t)
        }
        ElementShape::Plate { frame, .. } => p.distance(v(frame.origin)),
    };
    distance(b) < distance(a)
}

/// The connection of the armed component between the picked parts.
pub fn connection(
    project: &Project,
    component: &Component,
    template: &open_bim_model::ConnectionTemplate,
    main: ObjectId,
    support: Option<ObjectId>,
) -> Result<Vec<open_bim_commands::Command>, String> {
    let template = match &component.template {
        Some(saved) => saved.clone(),
        None => {
            let mut template = template.clone();
            template.kind = component.kind.clone();
            template.thickness = component.thickness;
            template.name = component.name.clone();
            // СП 16.13330: the weld leg grows with the plate.
            template.weld_size = template
                .weld_size
                .max(open_bim_connections::norms::min_weld(template.thickness));
            template
        }
    };
    template.validate().map_err(|e| e.to_string())?;
    Ok(vec![open_bim_commands::Command::CreateConnection {
        connection: ConnectionInstance {
            id: ObjectId(project.next_id),
            primary: main,
            secondary: support,
            at_end: at_end(project, main, support),
            template,
            children: Default::default(),
            general: Default::default(),
        },
    }])
}

impl Workbench {
    /// Arm a component from the catalogue.
    pub fn arm_component(&mut self, component: Component) {
        self.components.window = false;
        self.components.main = None;
        self.components.armed = Some(component);
        let prompt = self.components.prompt().unwrap_or_default();
        self.message(prompt);
    }

    /// A part clicked while a component is armed.
    pub(super) fn pick_component_part(&mut self, id: ObjectId) -> Option<super::Request> {
        let component = self.components.armed.clone()?;
        let (main, support) = match (component.needs_support, self.components.main) {
            (true, None) => {
                self.components.main = Some(id);
                self.editor.selected = [id].into();
                let prompt = self.components.prompt().unwrap_or_default();
                self.message(prompt);
                return None;
            }
            (true, Some(main)) => (main, Some(id)),
            (false, _) => (id, None),
        };
        self.components.main = None;
        let project = self.editor.document.project();
        let result = connection(
            project,
            &component,
            &self.tools.connection.template,
            main,
            support,
        );
        let prompt = self.components.prompt().unwrap_or_default();
        match result {
            Ok(commands) => {
                self.message(prompt);
                Some(super::Request::Action(Action::Commit(
                    format!("Компонент: {}", component.name),
                    commands,
                )))
            }
            Err(e) => {
                self.message(e);
                None
            }
        }
    }

    /// «Каталог компонентов» window: built-in components, then the saved
    /// ones under «Мои компоненты» with buttons to change, copy or delete.
    pub(super) fn components_window(&mut self, ctx: &egui::Context) {
        if !self.components.window {
            return;
        }
        let mut open = true;
        let mut chosen = None;
        let mut based_on = None;
        let mut edit = None;
        let mut delete = None;
        let saved = self.user_components();
        let saved_list: Vec<Component> = saved.iter().map(|u| u.component()).collect();
        let confirm = self.component_editor.confirm_delete.clone();
        let mut confirmed = None;
        egui::Window::new("Каталог компонентов")
            .open(&mut open)
            .collapsible(false)
            .default_width(400.)
            .show(ctx, |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.components.query)
                        .hint_text("Поиск: пластина, шарнир, колонна")
                        .desired_width(f32::INFINITY),
                )
                .request_focus();
                let mut all = catalog();
                all.extend(saved_list.iter().cloned());
                let found = search_in(all, &self.components.query);
                if found.is_empty() {
                    ui.weak("Ничего не найдено");
                }
                egui::ScrollArea::vertical().max_height(460.).show(ui, |ui| {
                    let mut group = String::new();
                    for c in found {
                        if c.group != group {
                            group = c.group.clone();
                            ui.add_space(4.);
                            ui.weak(&group);
                        }
                        let mine = c.template.is_some();
                        let r = ui
                            .selectable_label(false, egui::RichText::new(&c.name).strong())
                            .on_hover_text(&c.description);
                        ui.small(&c.description);
                        if r.clicked() || r.double_clicked() {
                            chosen = Some(c.clone());
                        }
                        r.context_menu(|ui| {
                            if ui.button("Создать свой на основе…").clicked() {
                                based_on = Some(c.clone());
                                ui.close();
                            }
                        });
                        if mine {
                            ui.horizontal(|ui| {
                                if confirm.as_deref() == Some(c.name.as_str()) {
                                    ui.label("Удалить без возврата?");
                                    if ui.small_button("Да, удалить").clicked() {
                                        delete = Some(c.name.clone());
                                    }
                                    if ui.small_button("Нет").clicked() {
                                        confirmed = Some(None);
                                    }
                                } else {
                                    if ui.small_button("Изменить").clicked() {
                                        edit = Some(c.name.clone());
                                    }
                                    if ui.small_button("Копия").clicked() {
                                        based_on = Some(c.clone());
                                    }
                                    if ui.small_button("Удалить").clicked() {
                                        confirmed = Some(Some(c.name.clone()));
                                    }
                                }
                            });
                        }
                    }
                });
                ui.separator();
                if ui
                    .button("＋ Создать свой компонент…")
                    .on_hover_text("Взять готовый узел за основу, поменять размеры, болты и швы и сохранить под своим именем")
                    .clicked()
                {
                    based_on = Some(catalog().remove(0));
                }
                ui.small("Свой компонент можно сделать и из любого узла: правая кнопка → «Создать свой на основе…».");
                ui.small("Болты, шов и толщину узла можно изменить после создания в панели «Параметрические соединения».");
            });
        if !open {
            self.components.window = false;
        }
        if let Some(c) = confirmed {
            self.component_editor.confirm_delete = c;
        }
        if let Some(name) = delete {
            self.component_editor.confirm_delete = None;
            if let Err(e) = self.delete_component(&name) {
                self.message(e);
            }
        }
        if let Some(name) = edit {
            self.edit_component(&name);
        }
        if let Some(c) = based_on {
            self.new_component(&c);
        }
        if let Some(c) = chosen {
            self.arm_component(c);
        }
    }
}
