//! Tekla-style ribbon: «Файл» menu, tabs (МЕТАЛЛ … УПРАВЛЕНИЕ; concrete and
//! rebar shown but disabled) and button groups, each button running a command
//! of the registry. Shift+click opens the tool's properties first.
use super::chrome::{SidePanel, tooltip};
use super::icons::{self, Icon};
use super::{Cmd, Mode, Request, Workbench, commands, theme};
use eframe::egui;

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

/// A ribbon button: command (None = not available in this release).
pub struct Item {
    pub cmd: Option<Cmd>,
    pub icon: Icon,
    pub label: &'static str,
    /// Drawn as a small button, three to a column.
    pub small: bool,
}

const fn item(cmd: Cmd, icon: Icon, label: &'static str) -> Item {
    Item {
        cmd: Some(cmd),
        icon,
        label,
        small: false,
    }
}
const fn small(cmd: Cmd, icon: Icon, label: &'static str) -> Item {
    Item {
        cmd: Some(cmd),
        icon,
        label,
        small: true,
    }
}
const fn future(icon: Icon, label: &'static str) -> Item {
    Item {
        cmd: None,
        icon,
        label,
        small: false,
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
                    item(Cmd::ToolBolt, Icon::Bolt, "Болт"),
                    item(Cmd::ToolWeld, Icon::Weld, "Сварной шов"),
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
                    small(Cmd::AssemblyCreate, Icon::Assembly, "Создать сборку"),
                    small(Cmd::AssemblyAdd, Icon::Assembly, "Добавить в сборку"),
                    small(Cmd::AssemblyRemove, Icon::Assembly, "Удалить из сборки"),
                    small(Cmd::AssemblySetMain, Icon::Assembly, "Главная деталь"),
                    small(Cmd::AssemblyExplode, Icon::Assembly, "Разбить сборку"),
                    small(Cmd::AutoAssemblies, Icon::Assembly, "Сборки по швам"),
                    item(Cmd::PanelConnections, Icon::Component, "Узлы"),
                ],
            ),
            (
                "Сетка",
                vec![
                    item(Cmd::GridDialog, Icon::Grid, "Сетка"),
                    item(Cmd::ToolAxis, Icon::Grid, "Ось"),
                ],
            ),
            (
                "Построения",
                vec![
                    small(Cmd::ConstructionLine, Icon::Grid, "Линия"),
                    small(Cmd::ConstructionCircle, Icon::Grid, "Окружность"),
                    small(Cmd::ModelDimension, Icon::Grid, "Размер"),
                ],
            ),
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
                    item(Cmd::MoveByPicks, Icon::Move, "Переместить"),
                    item(Cmd::CopyByPicks, Icon::Copy, "Копировать"),
                    item(Cmd::MirrorByPicks, Icon::Mirror, "Зеркало"),
                    item(Cmd::RotateByPicks, Icon::Rotate, "Повернуть"),
                    item(Cmd::DeleteSelected, Icon::Delete, "Удалить"),
                    item(Cmd::PanelEdit, Icon::Properties, "Числами"),
                ],
            ),
            (
                "Подгонка и резка",
                vec![
                    item(Cmd::FitEnd, Icon::Move, "Подогнать"),
                    item(Cmd::CutLine, Icon::Delete, "Рез линией"),
                    item(Cmd::CutPart, Icon::Delete, "Рез деталью"),
                    item(Cmd::CutPolygon, Icon::Delete, "Рез контуром"),
                    item(Cmd::SplitPart, Icon::Move, "Разделить"),
                    item(Cmd::CombineParts, Icon::Move, "Объединить"),
                    item(Cmd::ExtendTo, Icon::Move, "Удлинить"),
                ],
            ),
            (
                "Рабочая плоскость",
                vec![
                    item(Cmd::WorkPlaneThreePoints, Icon::WorkPlane, "По 3 точкам"),
                    item(Cmd::WorkPlaneReset, Icon::WorkPlane, "Глобальная XY"),
                ],
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
                    item(Cmd::GridViews, Icon::Grid, "По сетке"),
                    item(Cmd::PlaneOr3d, Icon::View3d, "3D / план"),
                ],
            ),
            (
                "Видимость",
                vec![
                    item(Cmd::ShowAll, Icon::Show, "Показать все"),
                    item(Cmd::HideSelected, Icon::Show, "Скрыть выбранные"),
                    item(Cmd::ToggleWorkArea, Icon::WorkArea, "Рабочая область"),
                    item(Cmd::ToggleSection, Icon::WorkArea, "Сечение"),
                ],
            ),
            (
                "Отображение",
                vec![
                    item(Cmd::DisplayWireframe, Icon::View3d, "Каркас"),
                    item(Cmd::DisplayTransparent, Icon::View3d, "Прозрачный"),
                    item(Cmd::DisplayShaded, Icon::View3d, "Тонированный"),
                    item(Cmd::ToggleTheme, Icon::Show, "Тема"),
                ],
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
                    item(Cmd::DocumentInfo, Icon::Drawing, "Основная надпись"),
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
                    item(Cmd::PanelModelTree, Icon::Grid, "Дерево модели"),
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

impl Workbench {
    pub(super) fn ribbon(&mut self, ui: &mut egui::Ui, requests: &mut Vec<Request>) {
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
                    if tab == self.ribbon_tab && self.focus_target == Some(super::FocusArea::Ribbon)
                    {
                        self.focus_target = None;
                        response.request_focus();
                    }
                    if self.keytips == super::keytips::KeyTips::Tabs
                        && let Some(tip) = super::keytips::tab_tip(tab)
                    {
                        super::keytips::badge(ui.ctx(), response.rect, tip);
                    }
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
                        let all = super::keytips::tab_items(self.ribbon_tab);
                        let tips = super::keytips::button_tips(&all.iter().collect::<Vec<_>>());
                        let mut k = 0;
                        ui.horizontal(|ui| {
                            for (name, items) in groups(self.ribbon_tab) {
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        // Large buttons in a row, small ones in columns of three.
                                        let mut column: Vec<&Item> = Vec::new();
                                        let mut cells: Vec<Vec<&Item>> = Vec::new();
                                        for it in &items {
                                            if it.small {
                                                column.push(it);
                                                if column.len() == 3 {
                                                    cells.push(std::mem::take(&mut column));
                                                }
                                            } else {
                                                if !column.is_empty() {
                                                    cells.push(std::mem::take(&mut column));
                                                }
                                                cells.push(vec![it]);
                                            }
                                        }
                                        if !column.is_empty() {
                                            cells.push(column);
                                        }
                                        for cell in cells {
                                            ui.vertical(|ui| {
                                                ui.spacing_mut().item_spacing.y = 1.;
                                                for it in cell {
                                                    let enabled = it.cmd.is_some() && !self.busy();
                                                    let tip = it
                                                        .cmd
                                                        .map_or(UNAVAILABLE.to_string(), tooltip);
                                                    let selected = it
                                                        .cmd
                                                        .is_some_and(|c| self.ribbon_selected(c));
                                                    let response = if it.small {
                                                        icons::small_button(
                                                            ui, it.icon, it.label, &tip, enabled,
                                                            selected,
                                                        )
                                                    } else {
                                                        icons::button(
                                                            ui,
                                                            it.icon,
                                                            30.,
                                                            Some(it.label),
                                                            it.label,
                                                            &tip,
                                                            enabled,
                                                            selected,
                                                        )
                                                    };
                                                    if self.keytips
                                                        == super::keytips::KeyTips::Buttons
                                                        && let Some(t) =
                                                            tips.get(k).copied().flatten()
                                                    {
                                                        super::keytips::badge(
                                                            ui.ctx(),
                                                            response.rect,
                                                            t,
                                                        );
                                                    }
                                                    k += 1;
                                                    if response.clicked()
                                                        && let Some(cmd) = it.cmd
                                                    {
                                                        self.run(cmd, requests);
                                                        // Tekla: Shift+click a tool opens its properties.
                                                        if ui.input(|i| i.modifiers.shift) {
                                                            self.mode = Mode::Model;
                                                            self.side_panel = Some(match cmd {
                                                                Cmd::ToolBolt | Cmd::ToolWeld => {
                                                                    SidePanel::Detailing
                                                                }
                                                                _ => SidePanel::Properties,
                                                            });
                                                        }
                                                    }
                                                }
                                            });
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
            Cmd::ToolAxis => Some(K::Axis),
            _ => None,
        };
        tool.is_some() && self.engine.tool == tool
            || cmd == Cmd::ModeDrawings && self.mode == Mode::Drawings
            || cmd == Cmd::ModeReports && self.mode == Mode::Reports
            || cmd == Cmd::ToggleSection && self.section_enabled
            || cmd == Cmd::ToolBolt
                && self.detail.tool == Some(crate::interaction::tools::DetailTool::Bolt)
            || cmd == Cmd::ToolWeld
                && self.detail.tool == Some(crate::interaction::tools::DetailTool::Weld)
            || cmd == Cmd::ToggleWorkArea && self.work_area
            || cmd == Cmd::ToolGusset
                && self.trim.tool == Some(crate::interaction::trim::TrimTool::Gusset)
            || cmd == Cmd::ToolStiffener
                && self.trim.tool == Some(crate::interaction::trim::TrimTool::Stiffener)
    }
}
