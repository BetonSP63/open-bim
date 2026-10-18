//! Tekla-style window chrome: title row with quick access and quick launch,
//! «Файл» menu, ribbon tabs and groups, side pane strip with panels, two bottom
//! toolbars (work plane, model search, snap and selection switches) and the
//! blue status bar with the current prompt.
use super::icons::{self, Icon};
use super::{Cmd, Mode, Request, Workbench, commands, theme};
use crate::interaction::snap::SnapKind;
use crate::tools::ui::{Action, ToolsPanel, select};
use eframe::egui;
use open_bim_model::ObjectId;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RibbonTab {
    #[default]
    Steel,
    Concrete,
    Rebar,
    Edit,
    View,
    Drawings,
    Manage,
}

impl RibbonTab {
    pub const ALL: [RibbonTab; 7] = [
        Self::Steel,
        Self::Concrete,
        Self::Rebar,
        Self::Edit,
        Self::View,
        Self::Drawings,
        Self::Manage,
    ];
    pub fn title(self) -> &'static str {
        match self {
            Self::Steel => "МЕТАЛЛ",
            Self::Concrete => "БЕТОН",
            Self::Rebar => "АРМАТУРА",
            Self::Edit => "ПРАВКА",
            Self::View => "ВИД",
            Self::Drawings => "ЧЕРТЕЖИ И ОТЧЁТЫ",
            Self::Manage => "УПРАВЛЕНИЕ",
        }
    }
    pub fn available(self) -> bool {
        !matches!(self, Self::Concrete | Self::Rebar)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidePanel {
    Properties,
    Edit,
    Detailing,
    Connections,
    Clash,
    Tree,
}

impl SidePanel {
    pub fn title(self) -> &'static str {
        match self {
            Self::Properties => "Свойства",
            Self::Edit => "Правка выбранных",
            Self::Detailing => "Болты, отверстия и швы",
            Self::Connections => "Узлы",
            Self::Clash => "Нумерация и коллизии",
            Self::Tree => "Дерево модели",
        }
    }
}

/// A ribbon button: command (None = not available in this release).
pub struct Item {
    pub cmd: Option<Cmd>,
    pub icon: Icon,
    pub label: &'static str,
}

const fn item(cmd: Cmd, icon: Icon, label: &'static str) -> Item {
    Item {
        cmd: Some(cmd),
        icon,
        label,
    }
}
const fn future(icon: Icon, label: &'static str) -> Item {
    Item {
        cmd: None,
        icon,
        label,
    }
}

pub const UNAVAILABLE: &str = "Будет в следующем выпуске";

/// Ribbon content: groups of buttons per tab.
pub fn groups(tab: RibbonTab) -> Vec<(&'static str, Vec<Item>)> {
    match tab {
        RibbonTab::Steel => vec![
            (
                "Детали",
                vec![
                    item(Cmd::ToolColumn, Icon::Column, "Колонна"),
                    item(Cmd::ToolBeam, Icon::Beam, "Балка"),
                    item(Cmd::ToolPlate, Icon::Plate, "Пластина"),
                    item(Cmd::ToolGusset, Icon::Plate, "Косынка"),
                    item(Cmd::ToolStiffener, Icon::Plate, "Ребро"),
                ],
            ),
            (
                "Крепёж",
                vec![
                    item(Cmd::PanelDetailing, Icon::Bolt, "Болт"),
                    item(Cmd::PanelDetailing, Icon::Weld, "Сварной шов"),
                    item(
                        Cmd::PanelDetailing,
                        Icon::BoltDetail,
                        "Детали болт. соединения",
                    ),
                ],
            ),
            (
                "Сборки и узлы",
                vec![
                    item(Cmd::AutoAssemblies, Icon::Assembly, "Сборка"),
                    item(Cmd::PanelConnections, Icon::Component, "Узлы"),
                ],
            ),
            ("Сетка", vec![item(Cmd::ToolAxis, Icon::Grid, "Ось")]),
        ],
        RibbonTab::Concrete => vec![(
            "Бетон",
            vec![
                future(Icon::ConcreteColumn, "Колонна"),
                future(Icon::ConcreteBeam, "Балка"),
                future(Icon::Panel, "Панель"),
                future(Icon::Slab, "Перекрытие"),
                future(Icon::Footing, "Фундамент"),
            ],
        )],
        RibbonTab::Rebar => vec![(
            "Арматура",
            vec![
                future(Icon::Rebar, "Стержень"),
                future(Icon::Rebar, "Группа стержней"),
            ],
        )],
        RibbonTab::Edit => vec![
            (
                "Изменить",
                vec![
                    item(Cmd::PanelEdit, Icon::Move, "Переместить"),
                    item(Cmd::PanelEdit, Icon::Copy, "Копировать"),
                    item(Cmd::PanelEdit, Icon::Mirror, "Зеркало"),
                    item(Cmd::PanelEdit, Icon::Rotate, "Повернуть"),
                    item(Cmd::DeleteSelected, Icon::Delete, "Удалить"),
                ],
            ),
            (
                "История",
                vec![
                    item(Cmd::Undo, Icon::Undo, "Отменить"),
                    item(Cmd::Redo, Icon::Redo, "Повторить"),
                ],
            ),
            (
                "Рабочая плоскость",
                vec![
                    item(Cmd::WorkPlaneThreePoints, Icon::WorkPlane, "По 3 точкам"),
                    item(Cmd::WorkPlaneReset, Icon::WorkPlane, "Глобальная XY"),
                ],
            ),
            (
                "Свойства",
                vec![item(Cmd::PanelProperties, Icon::Properties, "Свойства")],
            ),
        ],
        RibbonTab::View => vec![
            (
                "Виды",
                vec![
                    item(Cmd::ViewIso, Icon::View3d, "3D"),
                    item(Cmd::ViewFront, Icon::View3d, "Спереди"),
                    item(Cmd::ViewTop, Icon::View3d, "Сверху"),
                    item(Cmd::ViewRight, Icon::View3d, "Справа"),
                    item(Cmd::Fit, Icon::WorkArea, "Вписать"),
                    item(Cmd::ToggleProjection, Icon::View3d, "Перспектива"),
                ],
            ),
            (
                "Видимость",
                vec![
                    item(Cmd::ShowAll, Icon::Show, "Показать все"),
                    item(Cmd::HideSelected, Icon::Show, "Скрыть выбранные"),
                    item(Cmd::ToggleSection, Icon::WorkArea, "Сечение"),
                    item(Cmd::PanelModelTree, Icon::Grid, "Дерево модели"),
                ],
            ),
            (
                "Оформление",
                vec![item(Cmd::ToggleTheme, Icon::Show, "Тема")],
            ),
        ],
        RibbonTab::Drawings => vec![
            (
                "Чертежи",
                vec![
                    item(Cmd::ModeDrawings, Icon::Drawing, "Список чертежей"),
                    item(Cmd::PartDrawing, Icon::Drawing, "Чертёж детали"),
                    item(Cmd::AssemblyDrawing, Icon::Drawing, "Чертёж сборки"),
                    item(Cmd::GeneralDrawing, Icon::Drawing, "Общий вид"),
                    item(Cmd::UpdateDrawing, Icon::Drawing, "Обновить"),
                    item(Cmd::ExportPdf, Icon::Drawing, "PDF"),
                    item(Cmd::ExportDxf, Icon::Drawing, "DXF"),
                ],
            ),
            (
                "Отчёты",
                vec![
                    item(Cmd::ModeReports, Icon::Report, "Ведомости"),
                    item(Cmd::RefreshReports, Icon::Report, "Сформировать"),
                    item(Cmd::ExportCsv, Icon::Report, "CSV"),
                    item(Cmd::ReleaseSet, Icon::Report, "Выпустить комплект"),
                ],
            ),
        ],
        RibbonTab::Manage => vec![
            (
                "Проверка",
                vec![
                    item(Cmd::NumberAndCheck, Icon::Numbering, "Нумерация"),
                    item(Cmd::PanelClash, Icon::Clash, "Коллизии"),
                    item(Cmd::AutoAssemblies, Icon::Assembly, "Сборки по швам"),
                ],
            ),
            (
                "Обмен",
                vec![
                    item(Cmd::ExportIfc, Icon::Ifc, "Экспорт IFC"),
                    item(Cmd::ImportIfc, Icon::Ifc, "Импорт IFC"),
                ],
            ),
        ],
    }
}

fn tooltip(cmd: Cmd) -> String {
    let shortcut = commands::shortcut_text(cmd);
    let label = commands::spec(cmd).label;
    if shortcut.is_empty() {
        label.to_string()
    } else {
        format!("{label} · {shortcut}")
    }
}

impl Workbench {
    fn run(&mut self, cmd: Cmd, requests: &mut Vec<Request>) {
        if let Some(r) = self.execute(cmd) {
            requests.push(r);
        }
    }

    /// Whether viewport clicks go to a pick command.
    pub fn picking(&self) -> bool {
        self.engine.active() || self.workplane_picks.is_some()
    }

    pub fn prompt(&self) -> String {
        if self.workplane_picks.is_some() {
            self.workplane_prompt()
        } else {
            self.engine.prompt()
        }
    }

    /// Viewport click selection honouring the selection switches: parts,
    /// whole assemblies (all members of the picked part's assembly) or nothing.
    pub fn select_picked(&mut self, id: Option<ObjectId>, extend: bool) {
        if !self.select_parts && !self.select_assemblies {
            return;
        }
        let assembly = id.filter(|_| self.select_assemblies).and_then(|id| {
            self.editor
                .document
                .project()
                .assemblies
                .values()
                .find(|a| a.members.contains(&id))
                .map(|a| a.members.clone())
        });
        match assembly {
            Some(members) => {
                if !extend {
                    self.editor.selected.clear();
                }
                self.editor.selected.extend(members);
            }
            None => select(&mut self.editor.selected, id, extend),
        }
    }

    /// Select parts whose name, profile, material or ID contains the text.
    pub fn search_model(&mut self) -> usize {
        let query = self.search.trim().to_lowercase();
        if query.is_empty() {
            return 0;
        }
        let project = self.editor.document.project();
        let found: std::collections::BTreeSet<ObjectId> = project
            .elements
            .values()
            .filter(|e| {
                let profile = match &e.shape {
                    open_bim_model::ElementShape::Member { profile, .. } => project
                        .profiles
                        .get(profile)
                        .map(|p| p.name.as_str())
                        .unwrap_or(""),
                    open_bim_model::ElementShape::Plate { .. } => "пластина",
                };
                let material = project
                    .materials
                    .get(&e.material)
                    .map(|m| m.name.as_str())
                    .unwrap_or("");
                format!("{} {} {} #{}", e.name, profile, material, e.id.0)
                    .to_lowercase()
                    .contains(&query)
            })
            .map(|e| e.id)
            .collect();
        let n = found.len();
        self.editor.selected = found;
        self.message(format!("Найдено и выбрано: {n}"));
        n
    }

    pub(super) fn chrome(&mut self, ui: &mut egui::Ui, requests: &mut Vec<Request>) {
        if self.theme_applied != Some(self.light_theme) {
            theme::set_theme(ui.ctx(), self.light_theme);
            self.theme_applied = Some(self.light_theme);
        }
        self.title_row(ui, requests);
        self.ribbon(ui, requests);
        self.status_bar(ui);
        self.selection_toolbar(ui, requests);
        self.snap_toolbar(ui, requests);
        self.side(ui, requests);
    }

    fn title_row(&mut self, ui: &mut egui::Ui, requests: &mut Vec<Request>) {
        egui::Panel::top("title").show(ui, |ui| {
            ui.horizontal(|ui| {
                let (badge, _) = ui.allocate_exact_size(egui::vec2(22., 22.), egui::Sense::hover());
                ui.painter().rect_filled(badge, 3., theme::STATUS_BG);
                ui.painter().text(
                    badge.center(),
                    egui::Align2::CENTER_CENTER,
                    "OB",
                    egui::FontId::proportional(11.),
                    egui::Color32::WHITE,
                );
                for (cmd, icon) in [
                    (Cmd::SaveProject, Icon::Save),
                    (Cmd::Undo, Icon::Undo),
                    (Cmd::Redo, Icon::Redo),
                ] {
                    let enabled = match cmd {
                        Cmd::Undo => self.editor.document.history_depth().0 > 0,
                        Cmd::Redo => self.editor.document.history_depth().1 > 0,
                        _ => true,
                    } && !self.busy();
                    if icons::button(
                        ui,
                        icon,
                        16.,
                        None,
                        commands::spec(cmd).label,
                        &tooltip(cmd),
                        enabled,
                        false,
                    )
                    .clicked()
                    {
                        self.run(cmd, requests);
                    }
                }
                ui.separator();
                ui.label(format!(
                    "Open BIM — {} ({})",
                    self.editor.document.project().name,
                    self.tools.path
                ));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icons::button(
                        ui,
                        Icon::Help,
                        18.,
                        None,
                        "Горячие клавиши",
                        &tooltip(Cmd::Help),
                        true,
                        self.help,
                    )
                    .clicked()
                    {
                        self.run(Cmd::Help, requests);
                    }
                    let edit = ui.add(
                        egui::TextEdit::singleline(&mut self.quick_launch)
                            .hint_text("Быстрый запуск")
                            .desired_width(200.),
                    );
                    edit.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::TextEdit,
                            true,
                            "Быстрый запуск",
                        )
                    });
                    let results = if self.quick_launch.trim().is_empty() {
                        Vec::new()
                    } else {
                        commands::search(&self.quick_launch)
                    };
                    if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        if let Some(cmd) = results.first().copied() {
                            self.quick_launch.clear();
                            self.run(cmd, requests);
                        }
                    } else if edit.has_focus() && !results.is_empty() {
                        let mut chosen = None;
                        egui::Area::new(egui::Id::new("quick launch results"))
                            .fixed_pos(edit.rect.left_bottom() + egui::vec2(0., 2.))
                            .order(egui::Order::Foreground)
                            .show(ui.ctx(), |ui| {
                                egui::Frame::popup(ui.style()).show(ui, |ui| {
                                    for cmd in results.iter().take(8) {
                                        if ui
                                            .selectable_label(false, commands::caption(*cmd))
                                            .clicked()
                                        {
                                            chosen = Some(*cmd);
                                        }
                                    }
                                });
                            });
                        if let Some(cmd) = chosen {
                            self.quick_launch.clear();
                            self.run(cmd, requests);
                        }
                    }
                });
            });
        });
    }

    fn ribbon(&mut self, ui: &mut egui::Ui, requests: &mut Vec<Request>) {
        egui::Panel::top("ribbon tabs").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.menu_button("Файл", |ui| {
                    for cmd in [
                        Cmd::NewProject,
                        Cmd::OpenProject,
                        Cmd::SaveProject,
                        Cmd::ExampleFrame,
                        Cmd::ExportIfc,
                        Cmd::ImportIfc,
                        Cmd::ReleaseSet,
                    ] {
                        if ui.button(commands::caption(cmd)).clicked() {
                            ui.close();
                            self.run(cmd, requests);
                        }
                    }
                });
                ui.separator();
                for tab in RibbonTab::ALL {
                    let text = egui::RichText::new(tab.title());
                    let text = if tab.available() || self.ribbon_tab == tab {
                        text
                    } else {
                        text.color(ui.visuals().weak_text_color())
                    };
                    let response = ui.selectable_label(self.ribbon_tab == tab, text);
                    let response = if tab.available() {
                        response
                    } else {
                        response.on_hover_text(UNAVAILABLE)
                    };
                    if response.clicked() {
                        self.ribbon_tab = tab;
                        if matches!(
                            tab,
                            RibbonTab::Steel
                                | RibbonTab::Edit
                                | RibbonTab::View
                                | RibbonTab::Manage
                        ) {
                            self.mode = Mode::Model;
                        }
                    }
                }
            });
        });
        let fill = theme::ribbon_bg(ui.visuals());
        egui::Panel::top("ribbon")
            .frame(egui::Frame::side_top_panel(ui.style()).fill(fill))
            .show(ui, |ui| {
                egui::ScrollArea::horizontal()
                    .id_salt("ribbon scroll")
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for (name, items) in groups(self.ribbon_tab) {
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        for it in &items {
                                            let enabled = it.cmd.is_some() && !self.busy();
                                            let tip =
                                                it.cmd.map_or(UNAVAILABLE.to_string(), tooltip);
                                            let selected =
                                                it.cmd.is_some_and(|c| self.ribbon_selected(c));
                                            if icons::button(
                                                ui,
                                                it.icon,
                                                30.,
                                                Some(it.label),
                                                it.label,
                                                &tip,
                                                enabled,
                                                selected,
                                            )
                                            .clicked()
                                                && let Some(cmd) = it.cmd
                                            {
                                                self.run(cmd, requests);
                                            }
                                        }
                                    });
                                    ui.small(name);
                                });
                                ui.separator();
                            }
                        })
                    });
            });
    }

    fn ribbon_selected(&self, cmd: Cmd) -> bool {
        use crate::tools::ToolKind as K;
        let tool = match cmd {
            Cmd::ToolColumn => Some(K::Column),
            Cmd::ToolBeam => Some(K::Beam),
            Cmd::ToolPlate => Some(K::Plate),
            Cmd::ToolGusset => Some(K::Gusset),
            Cmd::ToolStiffener => Some(K::Stiffener),
            Cmd::ToolAxis => Some(K::Axis),
            _ => None,
        };
        tool.is_some() && self.engine.tool == tool
            || cmd == Cmd::ModeDrawings && self.mode == Mode::Drawings
            || cmd == Cmd::ModeReports && self.mode == Mode::Reports
            || cmd == Cmd::ToggleSection && self.section_enabled
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("status")
            .frame(egui::Frame::side_top_panel(ui.style()).fill(theme::STATUS_BG))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let white = |t: String| egui::RichText::new(t).color(theme::STATUS_TEXT);
                    ui.label(white(if self.picking() {
                        self.prompt()
                    } else {
                        self.tools.message.clone().unwrap_or_else(|| self.prompt())
                    }));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(white(format!(
                            "{} объектов выбрано",
                            self.editor.selected.len()
                        )));
                        ui.label(white(format!(
                            "{} · {} объектов · рев. {}",
                            self.gpu,
                            self.objects,
                            self.editor.document.project().revision
                        )));
                        if let Some(kind) = self.last_snap.filter(|_| self.picking()) {
                            ui.label(white(kind.label().to_string()));
                        }
                    });
                });
            });
    }

    fn selection_toolbar(&mut self, ui: &mut egui::Ui, requests: &mut Vec<Request>) {
        egui::Panel::bottom("selection toolbar").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("Выбор:");
                let parts = icons::button(
                    ui,
                    Icon::SelectParts,
                    18.,
                    None,
                    "Выбор деталей",
                    "Выбирать детали",
                    true,
                    self.select_parts,
                );
                if parts.clicked() {
                    self.select_parts = !self.select_parts;
                }
                let assemblies = icons::button(
                    ui,
                    Icon::SelectAssemblies,
                    18.,
                    None,
                    "Выбор сборок",
                    "Выбирать сборку целиком",
                    true,
                    self.select_assemblies,
                );
                if assemblies.clicked() {
                    self.select_assemblies = !self.select_assemblies;
                }
                ui.checkbox(&mut self.extend_selection, "Добавлять к выбору");
                ui.separator();
                for cmd in [Cmd::Undo, Cmd::Redo] {
                    let enabled = match cmd {
                        Cmd::Undo => self.editor.document.history_depth().0 > 0,
                        _ => self.editor.document.history_depth().1 > 0,
                    };
                    if ui
                        .add_enabled(enabled, egui::Button::new(commands::caption(cmd)))
                        .clicked()
                    {
                        self.run(cmd, requests);
                    }
                }
                ui.separator();
                for mode in [Mode::Model, Mode::Drawings, Mode::Reports] {
                    let (label, cmd) = match mode {
                        Mode::Model => ("3D модель", Cmd::ModeModel),
                        Mode::Drawings => ("Чертежи", Cmd::ModeDrawings),
                        Mode::Reports => ("Ведомости", Cmd::ModeReports),
                    };
                    if ui
                        .selectable_label(
                            self.mode == mode,
                            format!("{label} · {}", commands::shortcut_text(cmd)),
                        )
                        .clicked()
                    {
                        self.mode = mode;
                    }
                }
            });
        });
    }

    fn snap_toolbar(&mut self, ui: &mut egui::Ui, requests: &mut Vec<Request>) {
        egui::Panel::bottom("snap toolbar").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let custom = self.engine.workplane != crate::interaction::Workplane::default()
                    && self.engine.workplane.axes[2] != open_bim_renderer::DVec3::Z;
                let mut choice = custom;
                egui::ComboBox::new("coordinate system", "Система координат")
                    .selected_text(if custom {
                        "Рабочая плоскость"
                    } else {
                        "Начало координат модели"
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut choice, false, "Начало координат модели");
                        ui.selectable_value(&mut choice, true, "Рабочая плоскость по 3 точкам…");
                    });
                if choice != custom {
                    self.run(
                        if choice {
                            Cmd::WorkPlaneThreePoints
                        } else {
                            Cmd::WorkPlaneReset
                        },
                        requests,
                    );
                }
                ui.separator();
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut self.search)
                        .hint_text("Поиск в модели")
                        .desired_width(150.),
                );
                edit.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Поиск в модели")
                });
                let go = icons::button(
                    ui,
                    Icon::Search,
                    16.,
                    None,
                    "Найти в модели",
                    "Выбрать найденные объекты",
                    true,
                    false,
                );
                if go.clicked()
                    || (edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                {
                    self.search_model();
                }
                ui.separator();
                ui.label("Привязки:");
                let sw = &mut self.switches;
                for (icon, name, flag) in [
                    (
                        Icon::SnapReference,
                        SnapKind::Reference.label(),
                        &mut sw.reference,
                    ),
                    (Icon::SnapEnd, SnapKind::EndPoint.label(), &mut sw.end),
                    (Icon::SnapMid, SnapKind::MidPoint.label(), &mut sw.mid),
                    (
                        Icon::SnapIntersection,
                        SnapKind::Intersection.label(),
                        &mut sw.intersection,
                    ),
                    (
                        Icon::SnapPerpendicular,
                        SnapKind::Perpendicular.label(),
                        &mut sw.perpendicular,
                    ),
                    (
                        Icon::SnapNearest,
                        SnapKind::Nearest.label(),
                        &mut sw.nearest,
                    ),
                ] {
                    if icons::button(
                        ui,
                        icon,
                        16.,
                        None,
                        name,
                        &format!("Привязка: {}", name.to_lowercase()),
                        true,
                        *flag,
                    )
                    .clicked()
                    {
                        *flag = !*flag;
                    }
                }
                let ortho = self.switches.ortho;
                if icons::button(
                    ui,
                    Icon::Ortho,
                    16.,
                    None,
                    "Ортогонально",
                    &tooltip(Cmd::ToggleOrtho),
                    true,
                    ortho,
                )
                .clicked()
                {
                    self.run(Cmd::ToggleOrtho, requests);
                }
                ui.add(
                    egui::DragValue::new(&mut self.snap_tolerance_px)
                        .range(4.0..=40.0)
                        .suffix(" px")
                        .prefix("Допуск "),
                );
                ui.separator();
                let z = ui.label("Высота плоскости, мм");
                ui.add(egui::TextEdit::singleline(&mut self.tools.workplane).desired_width(60.))
                    .labelled_by(z.id);
            });
        });
    }

    fn side(&mut self, ui: &mut egui::Ui, requests: &mut Vec<Request>) {
        egui::Panel::right("side strip")
            .exact_size(40.)
            .resizable(false)
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    for (panel, icon, cmd) in [
                        (
                            SidePanel::Properties,
                            Icon::Properties,
                            Cmd::PanelProperties,
                        ),
                        (SidePanel::Edit, Icon::Move, Cmd::PanelEdit),
                        (SidePanel::Detailing, Icon::Bolt, Cmd::PanelDetailing),
                        (
                            SidePanel::Connections,
                            Icon::Component,
                            Cmd::PanelConnections,
                        ),
                        (SidePanel::Clash, Icon::Clash, Cmd::PanelClash),
                        (SidePanel::Tree, Icon::Grid, Cmd::PanelModelTree),
                    ] {
                        let open = self.mode == Mode::Model && self.side_panel == Some(panel);
                        if icons::button(
                            ui,
                            icon,
                            20.,
                            None,
                            panel.title(),
                            &tooltip(cmd),
                            true,
                            open,
                        )
                        .clicked()
                        {
                            self.run(cmd, requests);
                        }
                    }
                    ui.separator();
                    for (mode, icon, cmd) in [
                        (Mode::Drawings, Icon::Drawing, Cmd::ModeDrawings),
                        (Mode::Reports, Icon::Report, Cmd::ModeReports),
                    ] {
                        if icons::button(
                            ui,
                            icon,
                            20.,
                            None,
                            commands::spec(cmd).label,
                            &tooltip(cmd),
                            true,
                            self.mode == mode,
                        )
                        .clicked()
                        {
                            self.mode = if self.mode == mode { Mode::Model } else { mode };
                        }
                    }
                });
            });
        let open = match self.mode {
            Mode::Model => self.side_panel.is_some(),
            _ => true,
        };
        if !open {
            return;
        }
        egui::Panel::right("model tools")
            .default_size(330.0)
            .min_size(280.)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.add_enabled_ui(!self.busy(), |ui| match self.mode {
                        Mode::Drawings => self.drawings.show(ui, &mut self.editor),
                        Mode::Reports => {
                            if let Some(cmd) = self.reports.show_panel(ui, &mut self.editor) {
                                self.run(cmd, requests);
                            }
                        }
                        Mode::Model => match self.side_panel {
                            Some(SidePanel::Tree) => self.tree_panel(ui),
                            Some(panel) => {
                                self.tools.panel = match panel {
                                    SidePanel::Edit => ToolsPanel::Edit,
                                    SidePanel::Detailing => ToolsPanel::Detailing,
                                    SidePanel::Connections => ToolsPanel::Connections,
                                    SidePanel::Clash => ToolsPanel::Clash,
                                    _ => ToolsPanel::Properties,
                                };
                                if let Some(a) = self.tools.show(ui, &mut self.editor) {
                                    match a {
                                        Action::Open => self
                                            .start_open(std::path::PathBuf::from(&self.tools.path)),
                                        other => requests.push(Request::Action(other)),
                                    }
                                }
                            }
                            None => {}
                        },
                    });
                });
            });
    }

    fn tree_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading(SidePanel::Tree.title());
        let project = self.editor.document.project();
        if project.elements.is_empty() && project.axes.is_empty() {
            ui.label("Модель пуста: Alt+B — балка, ⌘O — открыть, ⌘P — все команды");
        }
        let key = (self.editor.events.len(), project.revision);
        if self.tree.0 != key {
            let rows: Vec<(ObjectId, String)> = project
                .elements
                .values()
                .map(|e| (e.id, format!("{} · #{}", e.name, e.id.0)))
                .chain(
                    project
                        .axes
                        .values()
                        .map(|a| (a.id, format!("{} · #{}", a.name, a.id.0))),
                )
                .collect();
            self.tree = (key, Arc::new(rows));
        }
        let rows = self.tree.1.clone();
        let row_height = ui.spacing().interact_size.y + ui.spacing().item_spacing.y;
        egui::ScrollArea::vertical()
            .id_salt("model tree rows")
            .max_height(ui.available_height().max(120.))
            .auto_shrink([false, true])
            .show_rows(ui, row_height, rows.len(), |ui, range| {
                for (id, label) in &rows[range] {
                    let id = *id;
                    ui.push_id(id.0, |ui| {
                        ui.horizontal(|ui| {
                            let mut visible = !self.hidden.contains(&id);
                            let toggle = ui
                                .add(egui::Checkbox::without_text(&mut visible))
                                .on_hover_text("Видимость");
                            toggle.widget_info(|| {
                                egui::WidgetInfo::selected(
                                    egui::WidgetType::Checkbox,
                                    true,
                                    visible,
                                    format!("Видимость: {label}"),
                                )
                            });
                            if toggle.changed() {
                                if visible {
                                    self.hidden.remove(&id);
                                } else {
                                    self.hidden.insert(id);
                                }
                            }
                            let row = ui.add(egui::Button::selectable(
                                self.editor.selected.contains(&id),
                                label.as_str(),
                            ));
                            if row.clicked() {
                                let extend = self.extend_selection
                                    || ui.input(|i| i.modifiers.shift || i.modifiers.command);
                                select(&mut self.editor.selected, Some(id), extend);
                            }
                        });
                    });
                }
            });
        ui.separator();
        ui.checkbox(&mut self.section_enabled, "Сечение Z ≥ высота");
        ui.add_enabled(
            self.section_enabled,
            egui::Slider::new(&mut self.section_height, 0.0..=50000.0)
                .suffix(" мм")
                .text("Высота сечения"),
        );
    }
}
