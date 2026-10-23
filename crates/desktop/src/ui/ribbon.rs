//! Ribbon: «Файл» menu, tabs (МЕТАЛЛ … ОПАЛУБКА; concrete, rebar,
//! analysis and formwork shown but disabled) and button groups, each button running a command
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
    Analysis,
    Formwork,
}

impl RibbonTab {
    pub const ALL: [RibbonTab; 9] = [
        Self::Steel,
        Self::Concrete,
        Self::Rebar,
        Self::Edit,
        Self::View,
        Self::Drawings,
        Self::Manage,
        Self::Analysis,
        Self::Formwork,
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
            Self::Analysis => "РАСЧЁТ И ПРОЕКТИРОВАНИЕ",
            Self::Formwork => "ОПАЛУБКА",
        }
    }
    /// Tabs whose every button is still a future tool are drawn dimmed.
    pub fn available(self) -> bool {
        !matches!(
            self,
            Self::Concrete | Self::Rebar | Self::Analysis | Self::Formwork
        )
    }
}

/// Which groups fold into a single button so the ribbon fits `available`
/// points: the rightmost groups go first, as in the usual ribbon. Groups not
/// measured yet (width 0) count as expanded.
pub fn collapse(widths: &[f32], available: f32) -> Vec<bool> {
    const FOLDED: f32 = 96.;
    let mut collapsed = vec![false; widths.len()];
    let total = |c: &[bool]| -> f32 {
        widths
            .iter()
            .zip(c)
            .map(|(w, f)| if *f { FOLDED } else { *w } + 8.)
            .sum()
    };
    for i in (0..widths.len()).rev() {
        if total(&collapsed) <= available {
            break;
        }
        if widths[i] > FOLDED {
            collapsed[i] = true;
        }
    }
    collapsed
}

/// A ribbon button: command (None = not available in this release).
#[derive(Clone, Copy)]
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
const fn small_future(icon: Icon, label: &'static str) -> Item {
    Item {
        cmd: None,
        icon,
        label,
        small: true,
    }
}

pub const UNAVAILABLE: &str = "Будет в следующем выпуске";

/// Ribbon content: groups of buttons per tab, in the order of the usual
/// steel detailing workflow. Buttons without a command are tools of a later
/// release: shown disabled with a tooltip so the layout stays complete.
pub fn groups(tab: RibbonTab) -> Vec<(&'static str, Vec<Item>)> {
    match tab {
        RibbonTab::Steel => vec![
            (
                "Детали",
                vec![
                    item(Cmd::ToolColumn, Icon::Column, "Колонна"),
                    item(Cmd::ToolBeam, Icon::Beam, "Балка"),
                    item(Cmd::ToolPlate, Icon::Plate, "Пластина"),
                    small(Cmd::ToolGusset, Icon::Plate, "Косынка"),
                    small(Cmd::ToolStiffener, Icon::Plate, "Ребро"),
                    small(Cmd::ToolTwin, Icon::Beam, "Сдвоенный профиль"),
                    small_future(Icon::Plate, "Элемент"),
                ],
            ),
            (
                "Крепёж",
                vec![
                    item(Cmd::ToolBolt, Icon::Bolt, "Болт"),
                    item(Cmd::ToolWeld, Icon::Weld, "Сварной шов"),
                    small(
                        Cmd::PanelDetailing,
                        Icon::BoltDetail,
                        "Детали болтового соединения",
                    ),
                    small(Cmd::ToolWeldPolygon, Icon::Weld, "Шов по ломаной"),
                    small(Cmd::ToolWeldOnPart, Icon::Weld, "Шов на детали"),
                ],
            ),
            (
                "Сборка",
                vec![
                    small(Cmd::AssemblyCreate, Icon::Assembly, "Сборка"),
                    small(Cmd::AssemblyAdd, Icon::Assembly, "Добавить в сборку"),
                    small(Cmd::AssemblyRemove, Icon::Assembly, "Удалить из сборки"),
                    small(Cmd::AssemblySetMain, Icon::Assembly, "Главная деталь"),
                    small(Cmd::AssemblyExplode, Icon::Assembly, "Разбить сборку"),
                    small(Cmd::AutoAssemblies, Icon::Assembly, "Сборки по швам"),
                ],
            ),
            (
                "Компоненты",
                vec![
                    item(Cmd::ComponentCatalog, Icon::Catalog, "Компоненты"),
                    small(Cmd::PanelConnections, Icon::Component, "Узлы"),
                    small(Cmd::AutoConnections, Icon::Component, "Автосоединения"),
                    small(Cmd::FitToProfile, Icon::Component, "Узлы по профилю"),
                ],
            ),
        ],
        RibbonTab::Concrete => vec![
            (
                "Детали",
                vec![
                    future(Icon::ConcreteColumn, "Колонна"),
                    future(Icon::ConcreteBeam, "Балка"),
                    future(Icon::Panel, "Панель"),
                    future(Icon::Slab, "Перекрытие"),
                    future(Icon::Footing, "Фундамент"),
                ],
            ),
            (
                "Элементы",
                vec![
                    small_future(Icon::Assembly, "Элемент"),
                    small_future(Icon::Assembly, "ЖБ элемент"),
                ],
            ),
            (
                "Бетонирование",
                vec![
                    small_future(Icon::Slab, "Захватки бетонирования"),
                    small_future(Icon::Slab, "Шов бетонирования"),
                    small_future(Icon::Slab, "Рассчитать единицы бетонирования"),
                ],
            ),
        ],
        RibbonTab::Rebar => vec![
            (
                "Армирование",
                vec![
                    small_future(Icon::Rebar, "Поперечные"),
                    small_future(Icon::Rebar, "Продольные"),
                    small_future(Icon::Rebar, "По грани"),
                    small_future(Icon::Rebar, "По направляющим"),
                ],
            ),
            (
                "Стержни",
                vec![
                    future(Icon::Rebar, "Ещё"),
                    future(Icon::Show, "Видимость"),
                    future(Icon::Rebar, "Группа стержней"),
                    future(Icon::Rebar, "Один стержень"),
                ],
            ),
        ],
        RibbonTab::Edit => vec![
            (
                "Резка и подгонка",
                vec![
                    small(Cmd::CutPolygon, Icon::Delete, "Вырез по многоугольнику"),
                    small(Cmd::CutLine, Icon::Delete, "Срез по линии"),
                    small(Cmd::CutPart, Icon::Delete, "Вырез по детали"),
                    small(Cmd::FitEnd, Icon::Move, "Подогнать торец детали"),
                    small(Cmd::SplitPart, Icon::Move, "Разбить"),
                    small(Cmd::CombineParts, Icon::Move, "Объединить"),
                    small(Cmd::ExtendTo, Icon::Move, "Удлинить"),
                    small(Cmd::EdgeChamfer, Icon::Plate, "Фаска кромки"),
                    small_future(Icon::Plate, "Добавленный материал"),
                ],
            ),
            (
                "Элементы и проверка",
                vec![
                    small_future(Icon::Assembly, "Преобразовать деталь в элемент"),
                    small_future(Icon::Assembly, "Создать форму из геометрии"),
                    small(
                        Cmd::ComponentEditor,
                        Icon::Component,
                        "Редактор компонентов",
                    ),
                    small(Cmd::Surfaces, Icon::Plate, "Поверхности"),
                    small(Cmd::CompareParts, Icon::Properties, "Сравнить"),
                    small(Cmd::InquireObject, Icon::Properties, "Запросить"),
                ],
            ),
            (
                "Изменить",
                vec![
                    item(Cmd::Measure, Icon::Move, "Измерить"),
                    item(Cmd::CopyByPicks, Icon::Copy, "Копировать"),
                    item(Cmd::Paste, Icon::Copy, "Вставить"),
                    small(Cmd::CopyToClipboard, Icon::Copy, "Копировать в буфер"),
                    item(Cmd::MoveByPicks, Icon::Move, "Переместить"),
                    small(Cmd::CopySpecial, Icon::Copy, "Копировать специально"),
                    small(Cmd::MoveSpecial, Icon::Move, "Переместить специально"),
                    small(Cmd::MeasureAngle, Icon::Rotate, "Измерить угол"),
                    small(Cmd::MirrorByPicks, Icon::Mirror, "Зеркало"),
                    small(Cmd::RotateByPicks, Icon::Rotate, "Повернуть"),
                    small(Cmd::DeleteSelected, Icon::Delete, "Удалить"),
                    small(Cmd::PanelEdit, Icon::Properties, "Изменить числами"),
                    small(Cmd::BatchEditor, Icon::Properties, "Пакетный редактор"),
                ],
            ),
            (
                "Построения",
                vec![
                    item(Cmd::GridDialog, Icon::Grid, "Сетка"),
                    small(Cmd::ToolAxis, Icon::Grid, "Ось"),
                    small(Cmd::ConstructionPoint, Icon::SnapEnd, "Точка"),
                    small(Cmd::PointsOnLine, Icon::SnapEnd, "Точки на линии"),
                    small_future(Icon::Grid, "Параметрическое моделирование"),
                    small(Cmd::ConstructionLine, Icon::Grid, "Вспомогательная линия"),
                    small(
                        Cmd::ConstructionCircle,
                        Icon::Grid,
                        "Вспомогательная окружность",
                    ),
                    small(Cmd::ModelDimension, Icon::Grid, "Размер"),
                ],
            ),
        ],
        RibbonTab::View => vec![
            (
                "Виды",
                vec![
                    item(Cmd::PanelViews, Icon::View3d, "Виды модели"),
                    small(Cmd::ViewIso, Icon::View3d, "3D"),
                    small(Cmd::ViewFront, Icon::View3d, "Спереди"),
                    small(Cmd::ViewTop, Icon::View3d, "Сверху"),
                    small(Cmd::ViewRight, Icon::View3d, "Справа"),
                    small(Cmd::GridViews, Icon::Grid, "По сетке"),
                    small(Cmd::PlaneOr3d, Icon::View3d, "3D/плоскость"),
                    small(Cmd::ToggleProjection, Icon::View3d, "Перспектива"),
                    small(Cmd::ViewProperties, Icon::Properties, "Свойства вида"),
                ],
            ),
            (
                "Отсечение",
                vec![
                    item(Cmd::ClipPlanePick, Icon::WorkArea, "Отсечение"),
                    small(Cmd::ClipPlaneRemove, Icon::WorkArea, "Убрать отсечение"),
                    small(Cmd::ToggleSection, Icon::WorkArea, "Сечение"),
                    small(Cmd::ToggleWorkArea, Icon::WorkArea, "Рабочая область"),
                    item(
                        Cmd::WorkPlaneThreePoints,
                        Icon::WorkPlane,
                        "Рабочая плоскость",
                    ),
                    small(Cmd::WorkPlaneReset, Icon::WorkPlane, "Глобальная XY"),
                    small(Cmd::Redraw, Icon::View3d, "Перечертить"),
                ],
            ),
            (
                "Визуализация",
                vec![
                    small(Cmd::DisplayWireframe, Icon::View3d, "Каркас"),
                    small(Cmd::DisplayTransparent, Icon::View3d, "Прозрачный"),
                    small(Cmd::DisplayShaded, Icon::View3d, "Тонированный"),
                    small(Cmd::ObjectRepresentation, Icon::Show, "Представление"),
                    small(Cmd::Paint, Icon::Show, "Окрасить"),
                    small_future(Icon::View3d, "Визуализатор"),
                    small(Cmd::ShowAll, Icon::Show, "Показать все"),
                    small(Cmd::HideSelected, Icon::Show, "Скрыть выбранные"),
                    small(Cmd::ShowOnlySelected, Icon::Show, "Только выбранные"),
                ],
            ),
            (
                "Навигация",
                vec![
                    small(Cmd::Fit, Icon::WorkArea, "Вписать"),
                    small(Cmd::FitSelected, Icon::WorkArea, "Вписать выбранные"),
                    small(Cmd::ToggleTheme, Icon::Show, "Тема"),
                    small(Cmd::ZoomIn, Icon::Search, "Увеличить"),
                    small(Cmd::ZoomOut, Icon::Search, "Уменьшить"),
                    small(Cmd::PreviousView, Icon::Search, "Предыдущий вид"),
                    small(Cmd::RotationCentre, Icon::View3d, "Центр вращения"),
                    small(Cmd::Fly, Icon::View3d, "Облёт"),
                    small(Cmd::Screenshot, Icon::View3d, "Снимок экрана"),
                ],
            ),
        ],
        RibbonTab::Drawings => vec![
            (
                "Чертежи",
                vec![
                    item(Cmd::DocumentManager, Icon::Drawing, "Диспетчер документов"),
                    small(Cmd::PartDrawing, Icon::Drawing, "Чертёж детали"),
                    small(Cmd::AssemblyDrawing, Icon::Drawing, "Чертёж сборки"),
                    small(Cmd::GeneralDrawing, Icon::Drawing, "Чертёж общего вида"),
                    small(Cmd::DrawingWizard, Icon::Drawing, "Мастер чертежей"),
                    small(Cmd::ModeDrawings, Icon::Drawing, "Список чертежей"),
                    small(Cmd::DrawingProperties, Icon::Properties, "Свойства чертежа"),
                    small(
                        Cmd::DrawingPrototypes,
                        Icon::Catalog,
                        "Каталог чертежей-прототипов",
                    ),
                    small_future(Icon::Drawing, "Комплексный чертёж"),
                    small(Cmd::UpdateDrawing, Icon::Drawing, "Обновить"),
                    small(Cmd::ExportPdf, Icon::Drawing, "PDF"),
                    small(Cmd::ExportDxf, Icon::Drawing, "DXF"),
                ],
            ),
            (
                "Нумерация",
                vec![
                    item(Cmd::NumberAndCheck, Icon::Numbering, "Выполнить нумерацию"),
                    small(Cmd::PanelClash, Icon::Numbering, "Настройки нумерации"),
                    small(Cmd::ControlNumbers, Icon::Numbering, "Контрольные номера"),
                    small(Cmd::ChangeNumber, Icon::Numbering, "Изменить номер"),
                ],
            ),
            (
                "Отчёты и выпуск",
                vec![
                    item(Cmd::ModeReports, Icon::Report, "Отчёты"),
                    small(Cmd::RefreshReports, Icon::Report, "Сформировать"),
                    small(Cmd::ExportCsv, Icon::Report, "CSV"),
                    small(Cmd::GeneralData, Icon::Drawing, "Общие данные"),
                    small(Cmd::DocumentInfo, Icon::Drawing, "Основная надпись"),
                    small(Cmd::ReleaseSet, Icon::Report, "Выпустить комплект"),
                    small(Cmd::IssueSet, Icon::Report, "Выдать комплект"),
                    small(Cmd::RecordChange, Icon::Report, "Записать изменение"),
                    small(Cmd::ExportPackage, Icon::Report, "Пакет ПДЭ"),
                ],
            ),
            (
                "Графика",
                vec![
                    small(Cmd::DrawDimension, Icon::Drawing, "Размер"),
                    small(Cmd::DrawLine, Icon::Drawing, "Линия"),
                    small(Cmd::DrawPolyline, Icon::Drawing, "Полилиния"),
                    small(Cmd::DrawRectangle, Icon::Drawing, "Прямоугольник"),
                    small(Cmd::DrawCircle, Icon::Drawing, "Окружность"),
                    small(Cmd::DrawHatch, Icon::Drawing, "Штриховка"),
                    small(Cmd::DrawWipeout, Icon::Drawing, "Маскировка"),
                    small(Cmd::DrawText, Icon::Drawing, "Текст"),
                    small(Cmd::DrawNode, Icon::Drawing, "Узел"),
                ],
            ),
        ],
        RibbonTab::Manage => vec![
            (
                "Проект",
                vec![
                    item(Cmd::PanelModelTree, Icon::Grid, "Организатор"),
                    small(Cmd::BuildingHierarchy, Icon::Grid, "Иерархия здания"),
                    small(Cmd::Phases, Icon::Assembly, "Стадии"),
                    small(Cmd::PanelClash, Icon::Clash, "Проверка на коллизии"),
                ],
            ),
            (
                "Обмен",
                vec![
                    small(Cmd::ExportIfc, Icon::Ifc, "Экспорт IFC"),
                    small(Cmd::ImportIfc, Icon::Ifc, "Импорт IFC"),
                    small_future(Icon::Ifc, "Преобразовать объекты IFC"),
                    small(Cmd::ExportNc, Icon::Ifc, "Экспорт NC"),
                    small(Cmd::Plugins, Icon::Component, "Плагины"),
                ],
            ),
            (
                "Планирование",
                vec![
                    small(Cmd::LayoutManager, Icon::Grid, "Диспетчер разбивок"),
                    small(Cmd::Locks, Icon::Properties, "Блокировки"),
                    small(Cmd::Tasks, Icon::Report, "Задания"),
                    small(Cmd::Sequences, Icon::Numbering, "Последовательности"),
                    small(Cmd::Lots, Icon::Assembly, "Партии"),
                    small(Cmd::ProjectStatus, Icon::Report, "Статус проекта"),
                ],
            ),
        ],
        RibbonTab::Analysis => vec![
            (
                "Расчётная модель",
                vec![
                    future(Icon::Assembly, "Расчётные и проектные модели"),
                    future(Icon::Properties, "Группы нагрузок"),
                    future(Icon::Move, "Загрузить"),
                ],
            ),
            (
                "Узлы",
                vec![
                    future(Icon::Component, "Узел"),
                    small_future(Icon::Component, "Жёсткая связь"),
                    small_future(Icon::Component, "Объединить узлы"),
                ],
            ),
            (
                "Свойства",
                vec![
                    small_future(Icon::Properties, "Свойства детали в расчётах"),
                    small_future(Icon::Properties, "Свойства нагрузки"),
                    small_future(Icon::Properties, "Сбросить изменения"),
                    small_future(Icon::Properties, "Получить расчётные свойства"),
                    small_future(Icon::Numbering, "Номера узлов"),
                    small_future(Icon::Numbering, "Номера элементов"),
                ],
            ),
        ],
        RibbonTab::Formwork => vec![(
            "Опалубка",
            vec![
                future(Icon::Panel, "Щиты опалубки"),
                future(Icon::Column, "Опоры"),
                small_future(Icon::Report, "Ведомость опалубки"),
            ],
        )],
    }
}

impl Workbench {
    pub(super) fn ribbon(&mut self, ui: &mut egui::Ui, requests: &mut Vec<Request>) {
        egui::Panel::top("ribbon tabs").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                // «Файл»: white on the blue of the status bar, at the left edge.
                let file =
                    egui::Button::new(egui::RichText::new("☰ Файл").color(theme::STATUS_TEXT))
                        .fill(theme::STATUS_BG);
                egui::containers::menu::MenuButton::from_button(file).ui(ui, |ui| {
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
                    // The open tab is blue text with a blue underline, not a filled box.
                    let open = self.ribbon_tab == tab;
                    let accent = if ui.visuals().dark_mode {
                        theme::FOCUS
                    } else {
                        theme::LIGHT_FOCUS
                    };
                    let response = ui
                        .scope(|ui| {
                            ui.visuals_mut().selection.bg_fill = egui::Color32::TRANSPARENT;
                            ui.visuals_mut().selection.stroke.color = accent;
                            ui.selectable_label(open, text)
                        })
                        .inner;
                    if open {
                        let r = response.rect;
                        ui.painter().line_segment(
                            [r.left_bottom(), r.right_bottom()],
                            egui::Stroke::new(2.5, accent),
                        );
                    }
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
                let available = ui.available_width();
                let tab = self.ribbon_tab;
                let groups = groups(tab);
                // Widths of the expanded groups, measured on earlier frames.
                let widths_id = egui::Id::new(("ribbon group widths", tab.title()));
                let mut widths: Vec<f32> = ui
                    .data(|d| d.get_temp(widths_id))
                    .unwrap_or_else(|| vec![0.; groups.len()]);
                widths.resize(groups.len(), 0.);
                let collapsed = collapse(&widths, available);
                let all = super::keytips::tab_items(tab);
                let tips = super::keytips::button_tips(&all.iter().collect::<Vec<_>>());
                let mut k = 0;
                egui::ScrollArea::horizontal()
                    .id_salt("ribbon scroll")
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for (i, (name, items)) in groups.iter().enumerate() {
                                if collapsed[i] {
                                    // Too narrow a window: the group folds into one
                                    // button that lists its tools.
                                    let response = icons::button(
                                        ui,
                                        items[0].icon,
                                        30.,
                                        Some(name),
                                        name,
                                        name,
                                        true,
                                        false,
                                    );
                                    egui::Popup::menu(&response).show(|ui| {
                                        ui.vertical(|ui| {
                                            for it in items {
                                                let item = Item { small: true, ..*it };
                                                self.ribbon_button(
                                                    ui, &item, &tips, &mut k, requests,
                                                );
                                            }
                                        });
                                    });
                                    // Keep the KeyTip numbering of the folded tools.
                                    k = k.max(groups[..=i].iter().map(|(_, it)| it.len()).sum());
                                    ui.separator();
                                    continue;
                                }
                                let group = ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        // Large buttons in a row, small ones in columns of three.
                                        let mut column: Vec<&Item> = Vec::new();
                                        let mut cells: Vec<Vec<&Item>> = Vec::new();
                                        for it in items {
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
                                                    self.ribbon_button(
                                                        ui, it, &tips, &mut k, requests,
                                                    );
                                                }
                                            });
                                        }
                                    });
                                    ui.small(*name);
                                });
                                widths[i] = group.response.rect.width();
                                ui.separator();
                            }
                        })
                    });
                ui.data_mut(|d| d.insert_temp(widths_id, widths));
            });
    }

    /// One ribbon button: runs its command, shows its KeyTip.
    fn ribbon_button(
        &mut self,
        ui: &mut egui::Ui,
        it: &Item,
        tips: &[Option<char>],
        k: &mut usize,
        requests: &mut Vec<Request>,
    ) {
        let enabled = it.cmd.is_some() && !self.busy();
        let tip = it.cmd.map_or(UNAVAILABLE.to_string(), tooltip);
        let selected = it.cmd.is_some_and(|c| self.ribbon_selected(c));
        let response = if it.small {
            icons::small_button(ui, it.icon, it.label, &tip, enabled, selected)
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
        if self.keytips == super::keytips::KeyTips::Buttons
            && let Some(t) = tips.get(*k).copied().flatten()
        {
            super::keytips::badge(ui.ctx(), response.rect, t);
        }
        *k += 1;
        if response.clicked()
            && let Some(cmd) = it.cmd
        {
            self.run(cmd, requests);
            // Shift+click a tool opens its properties.
            if ui.input(|i| i.modifiers.shift) {
                self.mode = Mode::Model;
                self.side_panel = Some(match cmd {
                    Cmd::ToolBolt | Cmd::ToolWeld | Cmd::ToolWeldPolygon | Cmd::ToolWeldOnPart => {
                        SidePanel::Detailing
                    }
                    _ => SidePanel::Properties,
                });
            }
        }
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
            || cmd == Cmd::ToolWeldPolygon
                && self.detail.tool == Some(crate::interaction::tools::DetailTool::WeldPolygon)
            || cmd == Cmd::ToolWeldOnPart
                && self.detail.tool == Some(crate::interaction::tools::DetailTool::WeldOnPart)
            || cmd == Cmd::ToggleWorkArea && self.work_area
            || cmd == Cmd::Fly && self.view_tools.fly
            || cmd == Cmd::ToolGusset
                && self.trim.tool == Some(crate::interaction::trim::TrimTool::Gusset)
            || cmd == Cmd::ToolStiffener
                && self.trim.tool == Some(crate::interaction::trim::TrimTool::Stiffener)
    }
}
