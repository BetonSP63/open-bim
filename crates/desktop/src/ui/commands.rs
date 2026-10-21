//! Command registry: every main command has a Russian label, a category, an
//! optional keyboard shortcut and search keywords for the command palette.
use eframe::egui::{Key, KeyboardShortcut, Modifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cmd {
    NewProject,
    OpenProject,
    SaveProject,
    ExampleFrame,
    Undo,
    Redo,
    ModeModel,
    ModeDrawings,
    ModeReports,
    Palette,
    Help,
    Fit,
    ViewIso,
    DisplayWireframe,
    DisplayTransparent,
    DisplayShaded,
    ViewFront,
    ViewTop,
    ViewRight,
    ToolAxis,
    ToolBeam,
    ToolColumn,
    ToolPlate,
    ToolGusset,
    ToolStiffener,
    ToolBolt,
    ToolWeld,
    GridDialog,
    DrawDimension,
    DrawLine,
    DrawPolyline,
    DrawRectangle,
    DrawCircle,
    DrawHatch,
    DrawWipeout,
    DrawText,
    DrawNode,
    AssemblyCreate,
    AssemblyAdd,
    AssemblyRemove,
    AssemblySetMain,
    AssemblyExplode,
    ConstructionLine,
    ConstructionCircle,
    ModelDimension,
    FitEnd,
    CutLine,
    CutPart,
    CutPolygon,
    SplitPart,
    CombineParts,
    ExtendTo,
    DocumentInfo,
    MoveByPicks,
    CopyByPicks,
    MirrorByPicks,
    RotateByPicks,
    CopySpecial,
    MoveSpecial,
    SelectPrevious,
    ToggleRollover,
    SelectionFilter,
    ToggleDragDrop,
    FitSelected,
    ShowOnlySelected,
    ToggleDirectModification,
    SelectAll,
    UndoPick,
    CycleFocus,
    FocusSearch,
    ComponentCatalog,
    ComponentEditor,
    AutoConnections,
    GridViews,
    PanelViews,
    PlaneOr3d,
    ToggleContextual,
    Measure,
    CreateElement,
    CancelTool,
    FinishPicks,
    ToggleOrtho,
    PanelProperties,
    PanelEdit,
    PanelDetailing,
    PanelConnections,
    PanelClash,
    PanelModelTree,
    DeleteSelected,
    ToggleProjection,
    ShowAll,
    HideSelected,
    ToggleSection,
    ToggleWorkArea,
    ToggleTheme,
    WorkPlaneThreePoints,
    WorkPlaneReset,
    EditProperty,
    ApplyProperty,
    NumberAndCheck,
    ExportIfc,
    ImportIfc,
    DocumentManager,
    DrawingWizard,
    PartDrawing,
    AssemblyDrawing,
    GeneralDrawing,
    GeneralData,
    UpdateDrawing,
    ExportPdf,
    ExportDxf,
    RefreshReports,
    AutoAssemblies,
    ExportCsv,
    IssueSet,
    RecordChange,
    ExportPackage,
    ReleaseSet,
    CancelJob,
}

const CMD: Modifiers = Modifiers::COMMAND;
const SHIFT_CMD: Modifiers = Modifiers {
    shift: true,
    ..Modifiers::COMMAND
};
const ALT: Modifiers = Modifiers::ALT;
const SHIFT_ALT: Modifiers = Modifiers {
    shift: true,
    ..Modifiers::ALT
};
/// The usual detailing shortcuts use Ctrl; on macOS this is the Control key,
/// distinct from ⌘ (system clipboard, window commands).
const CTRL: Modifiers = Modifiers::CTRL;
const SHIFT: Modifiers = Modifiers::SHIFT;
const NONE: Modifiers = Modifiers::NONE;

pub struct Spec {
    pub cmd: Cmd,
    pub label: &'static str,
    pub category: &'static str,
    pub shortcut: Option<KeyboardShortcut>,
    pub keywords: &'static str,
}

const fn s(modifiers: Modifiers, key: Key) -> Option<KeyboardShortcut> {
    Some(KeyboardShortcut::new(modifiers, key))
}

pub const ALL: &[Spec] = &[
    Spec {
        cmd: Cmd::NewProject,
        label: "Новый проект",
        category: "Файл",
        shortcut: s(CMD, Key::N),
        keywords: "создать пустой new",
    },
    Spec {
        cmd: Cmd::OpenProject,
        label: "Открыть проект",
        category: "Файл",
        shortcut: s(CMD, Key::O),
        keywords: "загрузить obim open",
    },
    Spec {
        cmd: Cmd::SaveProject,
        label: "Сохранить проект",
        category: "Файл",
        shortcut: s(CMD, Key::S),
        keywords: "записать save",
    },
    Spec {
        cmd: Cmd::ExampleFrame,
        label: "Открыть контрольный каркас",
        category: "Файл",
        shortcut: None,
        keywords: "пример example",
    },
    Spec {
        cmd: Cmd::ExportIfc,
        label: "Экспорт IFC4",
        category: "Файл",
        shortcut: s(CMD, Key::E),
        keywords: "ifc обмен выгрузить",
    },
    Spec {
        cmd: Cmd::ImportIfc,
        label: "Импорт IFC4",
        category: "Файл",
        shortcut: s(CMD, Key::I),
        keywords: "ifc обмен загрузить",
    },
    Spec {
        cmd: Cmd::Undo,
        label: "Отменить",
        category: "Правка",
        shortcut: s(CMD, Key::Z),
        keywords: "undo назад",
    },
    Spec {
        cmd: Cmd::Redo,
        label: "Повторить",
        category: "Правка",
        shortcut: s(SHIFT_CMD, Key::Z),
        keywords: "redo вперёд",
    },
    Spec {
        cmd: Cmd::EditProperty,
        label: "Изменить свойство выбранных",
        category: "Правка",
        shortcut: s(CMD, Key::L),
        keywords: "свойство марка атрибут property",
    },
    Spec {
        cmd: Cmd::ApplyProperty,
        label: "Применить свойство",
        category: "Правка",
        shortcut: None,
        keywords: "свойство применить",
    },
    Spec {
        cmd: Cmd::ModeModel,
        label: "Режим: 3D модель",
        category: "Вид",
        shortcut: s(NONE, Key::F2),
        keywords: "модель 3d",
    },
    Spec {
        cmd: Cmd::ModeDrawings,
        label: "Режим: чертежи",
        category: "Вид",
        shortcut: s(NONE, Key::F3),
        keywords: "чертёж лист",
    },
    Spec {
        cmd: Cmd::ModeReports,
        label: "Режим: ведомости",
        category: "Вид",
        shortcut: s(NONE, Key::F4),
        keywords: "отчёт спецификация",
    },
    Spec {
        cmd: Cmd::Fit,
        label: "Вписать модель",
        category: "Вид",
        shortcut: s(NONE, Key::Home),
        keywords: "zoom fit показать всё",
    },
    Spec {
        cmd: Cmd::ViewIso,
        label: "Вид: изометрия",
        category: "Вид",
        shortcut: s(NONE, Key::Num1),
        keywords: "изометрия",
    },
    Spec {
        cmd: Cmd::ViewFront,
        label: "Вид: спереди",
        category: "Вид",
        shortcut: s(NONE, Key::Num2),
        keywords: "фасад",
    },
    Spec {
        cmd: Cmd::ViewTop,
        label: "Вид: сверху",
        category: "Вид",
        shortcut: s(NONE, Key::Num3),
        keywords: "план",
    },
    Spec {
        cmd: Cmd::DisplayWireframe,
        label: "Отображение: каркас",
        category: "Вид",
        shortcut: s(CTRL, Key::Num1),
        keywords: "рёбра линии wireframe",
    },
    Spec {
        cmd: Cmd::DisplayTransparent,
        label: "Отображение: полупрозрачный",
        category: "Вид",
        shortcut: s(CTRL, Key::Num2),
        keywords: "прозрачность прозрачный",
    },
    Spec {
        cmd: Cmd::DisplayShaded,
        label: "Отображение: тонированный",
        category: "Вид",
        shortcut: s(CTRL, Key::Num4),
        keywords: "заливка shaded",
    },
    Spec {
        cmd: Cmd::ViewRight,
        label: "Вид: справа",
        category: "Вид",
        shortcut: s(NONE, Key::Num4),
        keywords: "боковой",
    },
    Spec {
        cmd: Cmd::Palette,
        label: "Поиск команд",
        category: "Справка",
        shortcut: s(CMD, Key::P),
        keywords: "команда найти palette",
    },
    Spec {
        cmd: Cmd::Help,
        label: "Горячие клавиши",
        category: "Справка",
        shortcut: s(NONE, Key::F1),
        keywords: "справка помощь клавиши",
    },
    Spec {
        cmd: Cmd::ToolAxis,
        label: "Инструмент: ось",
        category: "Моделирование",
        shortcut: s(ALT, Key::A),
        keywords: "сетка axis",
    },
    Spec {
        cmd: Cmd::ToolBeam,
        label: "Инструмент: балка",
        category: "Моделирование",
        shortcut: s(ALT, Key::B),
        keywords: "beam",
    },
    Spec {
        cmd: Cmd::ToolColumn,
        label: "Инструмент: колонна",
        category: "Моделирование",
        shortcut: s(ALT, Key::C),
        keywords: "column стойка",
    },
    Spec {
        cmd: Cmd::ToolPlate,
        label: "Инструмент: контурная плита",
        category: "Моделирование",
        shortcut: s(SHIFT_ALT, Key::P),
        keywords: "пластина plate лист",
    },
    Spec {
        cmd: Cmd::ToolGusset,
        label: "Инструмент: косынка",
        category: "Моделирование",
        shortcut: s(ALT, Key::G),
        keywords: "фасонка gusset",
    },
    Spec {
        cmd: Cmd::ToolStiffener,
        label: "Инструмент: ребро жёсткости",
        category: "Моделирование",
        shortcut: s(ALT, Key::S),
        keywords: "ребро stiffener",
    },
    Spec {
        cmd: Cmd::ToolBolt,
        label: "Болт по двум точкам",
        category: "Моделирование",
        shortcut: None,
        keywords: "болт болтовая группа bolt отверстия",
    },
    Spec {
        cmd: Cmd::ToolWeld,
        label: "Сварной шов между деталями",
        category: "Моделирование",
        shortcut: None,
        keywords: "шов сварка weld",
    },
    Spec {
        cmd: Cmd::MoveByPicks,
        label: "Переместить по точкам",
        category: "Правка",
        shortcut: s(CTRL, Key::M),
        keywords: "переместить сдвинуть move",
    },
    Spec {
        cmd: Cmd::CopyByPicks,
        label: "Копировать по точкам",
        category: "Правка",
        shortcut: s(CTRL, Key::C),
        keywords: "копировать копия copy",
    },
    Spec {
        cmd: Cmd::MirrorByPicks,
        label: "Зеркально по двум точкам",
        category: "Правка",
        shortcut: None,
        keywords: "зеркало отразить mirror",
    },
    Spec {
        cmd: Cmd::RotateByPicks,
        label: "Повернуть по точкам",
        category: "Правка",
        shortcut: None,
        keywords: "повернуть поворот rotate",
    },
    Spec {
        cmd: Cmd::CopySpecial,
        label: "Копировать — специальное",
        category: "Правка",
        shortcut: None,
        keywords: "копировать специальное линейно поворот зеркало плоскость массив copy special",
    },
    Spec {
        cmd: Cmd::MoveSpecial,
        label: "Переместить — специальное",
        category: "Правка",
        shortcut: None,
        keywords: "переместить специальное линейно поворот зеркало плоскость move special",
    },
    Spec {
        cmd: Cmd::SelectPrevious,
        label: "Предыдущий выбор",
        category: "Выбор",
        shortcut: s(ALT, Key::P),
        keywords: "предыдущий выбор select previous",
    },
    Spec {
        cmd: Cmd::ToggleRollover,
        label: "Подсветка под курсором",
        category: "Выбор",
        shortcut: s(NONE, Key::H),
        keywords: "подсветка наведение rollover highlight",
    },
    Spec {
        cmd: Cmd::SelectionFilter,
        label: "Фильтр выбора",
        category: "Выбор",
        shortcut: s(CTRL, Key::G),
        keywords: "фильтр выбора профиль материал имя класс filter",
    },
    Spec {
        cmd: Cmd::ToggleDragDrop,
        label: "Перетаскивание",
        category: "Выбор",
        shortcut: None,
        keywords: "перетаскивание drag drop переместить мышью",
    },
    Spec {
        cmd: Cmd::FitSelected,
        label: "Вписать выбранные",
        category: "Вид",
        shortcut: s(SHIFT, Key::Space),
        keywords: "масштаб выбранные zoom selected",
    },
    Spec {
        cmd: Cmd::ShowOnlySelected,
        label: "Показать только выбранные",
        category: "Вид",
        shortcut: None,
        keywords: "показать только выбранные изолировать",
    },
    Spec {
        cmd: Cmd::ToggleDirectModification,
        label: "Прямое изменение (ручки)",
        category: "Правка",
        shortcut: s(NONE, Key::D),
        keywords: "прямое изменение ручки direct modification",
    },
    Spec {
        cmd: Cmd::SelectAll,
        label: "Выбрать все",
        category: "Правка",
        shortcut: s(CTRL, Key::A),
        keywords: "выбрать все select all",
    },
    Spec {
        cmd: Cmd::GridViews,
        label: "Виды по линиям сетки",
        category: "Вид",
        shortcut: None,
        keywords: "виды планы фасады сетка",
    },
    Spec {
        cmd: Cmd::PanelViews,
        label: "Панель: виды модели",
        category: "Вид",
        shortcut: s(CTRL, Key::I),
        keywords: "виды модели список",
    },
    Spec {
        cmd: Cmd::PlaneOr3d,
        label: "Переключить 3D / плоский вид",
        category: "Вид",
        shortcut: s(CTRL, Key::P),
        keywords: "3d план плоский вид",
    },
    Spec {
        cmd: Cmd::ToggleContextual,
        label: "Контекстная панель",
        category: "Вид",
        shortcut: s(CTRL, Key::K),
        keywords: "контекстная панель свойства у курсора",
    },
    Spec {
        cmd: Cmd::Measure,
        label: "Измерить расстояние",
        category: "Вид",
        shortcut: s(NONE, Key::F),
        keywords: "измерить расстояние рулетка measure",
    },
    Spec {
        cmd: Cmd::GridDialog,
        label: "Сетка осей…",
        category: "Моделирование",
        shortcut: None,
        keywords: "сетка оси grid уровни",
    },
    Spec {
        cmd: Cmd::CreateElement,
        label: "Создать элемент по полям инструмента",
        category: "Моделирование",
        shortcut: s(CMD, Key::Enter),
        keywords: "создать построить",
    },
    Spec {
        cmd: Cmd::CycleFocus,
        label: "Следующая область окна",
        category: "Вид",
        shortcut: s(NONE, Key::F6),
        keywords: "фокус область панель клавиатура f6",
    },
    Spec {
        cmd: Cmd::FocusSearch,
        label: "Поиск в модели",
        category: "Правка",
        shortcut: s(CMD, Key::F),
        keywords: "поиск найти объект",
    },
    Spec {
        cmd: Cmd::ComponentCatalog,
        label: "Каталог компонентов…",
        category: "Детали",
        shortcut: s(CTRL, Key::F),
        keywords: "компоненты узлы соединения каталог пластина шарнир база",
    },
    Spec {
        cmd: Cmd::ComponentEditor,
        label: "Редактор компонентов…",
        category: "Детали",
        shortcut: None,
        keywords: "компонент свой создать редактор узел шаблон мои компоненты изменить",
    },
    Spec {
        cmd: Cmd::AutoConnections,
        label: "Автосоединения",
        category: "Детали",
        shortcut: None,
        keywords: "автосоединения автоматически узлы соединения правила все балки база",
    },
    Spec {
        cmd: Cmd::UndoPick,
        label: "Убрать последнюю точку",
        category: "Моделирование",
        shortcut: s(NONE, Key::Backspace),
        keywords: "отменить точку назад backspace",
    },
    Spec {
        cmd: Cmd::FinishPicks,
        label: "Завершить контур / повторить команду",
        category: "Моделирование",
        shortcut: s(NONE, Key::Enter),
        keywords: "enter завершить повторить",
    },
    Spec {
        cmd: Cmd::PanelProperties,
        label: "Панель: свойства",
        category: "Панели",
        shortcut: None,
        keywords: "свойства параметры",
    },
    Spec {
        cmd: Cmd::PanelEdit,
        label: "Панель: правка выбранных",
        category: "Панели",
        shortcut: None,
        keywords: "переместить копировать повернуть зеркало",
    },
    Spec {
        cmd: Cmd::PanelDetailing,
        label: "Панель: болты, отверстия и швы",
        category: "Панели",
        shortcut: None,
        keywords: "болт отверстие сварка деталировка",
    },
    Spec {
        cmd: Cmd::PanelConnections,
        label: "Панель: узлы",
        category: "Панели",
        shortcut: None,
        keywords: "узел соединение база торцевая уголок",
    },
    Spec {
        cmd: Cmd::PanelClash,
        label: "Панель: нумерация и коллизии",
        category: "Панели",
        shortcut: None,
        keywords: "коллизии отчёт позиции",
    },
    Spec {
        cmd: Cmd::PanelModelTree,
        label: "Панель: дерево модели",
        category: "Панели",
        shortcut: None,
        keywords: "дерево список объекты",
    },
    Spec {
        cmd: Cmd::DeleteSelected,
        label: "Удалить выбранные",
        category: "Правка",
        shortcut: s(NONE, Key::Delete),
        keywords: "удалить delete",
    },
    Spec {
        cmd: Cmd::ToggleProjection,
        label: "Перспектива / ортогональная проекция",
        category: "Вид",
        shortcut: s(NONE, Key::P),
        keywords: "перспектива орто проекция",
    },
    Spec {
        cmd: Cmd::ShowAll,
        label: "Показать все объекты",
        category: "Вид",
        shortcut: None,
        keywords: "показать видимость",
    },
    Spec {
        cmd: Cmd::HideSelected,
        label: "Скрыть выбранные",
        category: "Вид",
        shortcut: None,
        keywords: "скрыть видимость",
    },
    Spec {
        cmd: Cmd::ToggleSection,
        label: "Сечение по высоте",
        category: "Вид",
        shortcut: None,
        keywords: "сечение отсечение клип",
    },
    Spec {
        cmd: Cmd::ToggleWorkArea,
        label: "Рабочая область",
        category: "Вид",
        shortcut: None,
        keywords: "рабочая область габарит рамка",
    },
    Spec {
        cmd: Cmd::ToggleTheme,
        label: "Светлая / тёмная тема",
        category: "Вид",
        shortcut: None,
        keywords: "тема светлая тёмная",
    },
    Spec {
        cmd: Cmd::WorkPlaneThreePoints,
        label: "Рабочая плоскость по трём точкам",
        category: "Правка",
        shortcut: None,
        keywords: "рабочая плоскость 3 точки",
    },
    Spec {
        cmd: Cmd::WorkPlaneReset,
        label: "Рабочая плоскость: глобальная XY",
        category: "Правка",
        shortcut: None,
        keywords: "рабочая плоскость сброс начало",
    },
    Spec {
        cmd: Cmd::ToggleOrtho,
        label: "Ортогонально (углы 45°)",
        category: "Привязки",
        shortcut: s(NONE, Key::O),
        keywords: "орто ortho угол 90",
    },
    Spec {
        cmd: Cmd::CancelTool,
        label: "Прервать",
        category: "Моделирование",
        shortcut: s(NONE, Key::Escape),
        keywords: "escape выход",
    },
    Spec {
        cmd: Cmd::NumberAndCheck,
        label: "Нумерация и проверка коллизий",
        category: "Проверка",
        shortcut: s(CMD, Key::K),
        keywords: "коллизии позиции марки clash",
    },
    Spec {
        cmd: Cmd::DrawingWizard,
        label: "Мастер чертежей",
        category: "Чертежи",
        shortcut: None,
        keywords: "автоматически чертежи марки клонирование образец правила",
    },
    Spec {
        cmd: Cmd::DocumentManager,
        label: "Менеджер документов",
        category: "Чертежи",
        shortcut: s(CTRL, Key::L),
        keywords: "список чертежей документы выдать обновить комплект",
    },
    Spec {
        cmd: Cmd::PartDrawing,
        label: "Чертёж детали",
        category: "Чертежи",
        shortcut: s(CMD, Key::D),
        keywords: "кмд деталь",
    },
    Spec {
        cmd: Cmd::AssemblyDrawing,
        label: "Чертёж сборки",
        category: "Чертежи",
        shortcut: None,
        keywords: "кмд сборка",
    },
    Spec {
        cmd: Cmd::GeneralDrawing,
        label: "Общий вид A1",
        category: "Чертежи",
        shortcut: None,
        keywords: "км общий вид",
    },
    Spec {
        cmd: Cmd::GeneralData,
        label: "Общие данные",
        category: "Чертежи",
        shortcut: None,
        keywords: "ведомость рабочих чертежей общие указания первый лист",
    },
    Spec {
        cmd: Cmd::UpdateDrawing,
        label: "Обновить чертёж по модели",
        category: "Чертежи",
        shortcut: s(CMD, Key::U),
        keywords: "актуализировать устарел",
    },
    Spec {
        cmd: Cmd::ExportPdf,
        label: "Экспорт чертежа в PDF",
        category: "Чертежи",
        shortcut: s(SHIFT_CMD, Key::P),
        keywords: "pdf печать",
    },
    Spec {
        cmd: Cmd::ExportDxf,
        label: "Экспорт чертежа в DXF",
        category: "Чертежи",
        shortcut: None,
        keywords: "dxf cad",
    },
    Spec {
        cmd: Cmd::RefreshReports,
        label: "Сформировать ведомости",
        category: "Ведомости",
        shortcut: s(CMD, Key::R),
        keywords: "спецификация масса",
    },
    Spec {
        cmd: Cmd::AutoAssemblies,
        label: "Сборки по заводским швам",
        category: "Ведомости",
        shortcut: None,
        keywords: "сборка марка",
    },
    Spec {
        cmd: Cmd::IssueSet,
        label: "Выдать комплект",
        category: "Чертежи",
        shortcut: None,
        keywords: "выпуск выдача комплект км кмд",
    },
    Spec {
        cmd: Cmd::RecordChange,
        label: "Записать изменение комплекта",
        category: "Чертежи",
        shortcut: None,
        keywords: "изменение изм разрешение зам нов аннул",
    },
    Spec {
        cmd: Cmd::ExportPackage,
        label: "Выпустить пакет ПДЭ",
        category: "Чертежи",
        shortcut: None,
        keywords: "пакет электронных документов удостоверяющий лист ул обложка титульный",
    },
    Spec {
        cmd: Cmd::ExportCsv,
        label: "Экспорт ведомостей CSV",
        category: "Ведомости",
        shortcut: None,
        keywords: "csv excel",
    },
    Spec {
        cmd: Cmd::ConstructionLine,
        label: "Вспомогательная линия",
        category: "Построения",
        shortcut: None,
        keywords: "построение линия вспомогательная",
    },
    Spec {
        cmd: Cmd::ConstructionCircle,
        label: "Вспомогательная окружность",
        category: "Построения",
        shortcut: None,
        keywords: "построение окружность круг",
    },
    Spec {
        cmd: Cmd::ModelDimension,
        label: "Размер в модели",
        category: "Построения",
        shortcut: None,
        keywords: "размер измерить расстояние",
    },
    Spec {
        cmd: Cmd::FitEnd,
        label: "Подогнать торец",
        category: "Подгонка",
        shortcut: None,
        keywords: "подгонка торец обрезать fit",
    },
    Spec {
        cmd: Cmd::CutLine,
        label: "Разрезать линией",
        category: "Подгонка",
        shortcut: None,
        keywords: "рез линия обрезать",
    },
    Spec {
        cmd: Cmd::CutPart,
        label: "Разрезать деталью",
        category: "Подгонка",
        shortcut: None,
        keywords: "рез деталью вырез",
    },
    Spec {
        cmd: Cmd::CutPolygon,
        label: "Разрезать многоугольником",
        category: "Подгонка",
        shortcut: None,
        keywords: "рез контур многоугольник вырез",
    },
    Spec {
        cmd: Cmd::SplitPart,
        label: "Разделить",
        category: "Подгонка",
        shortcut: None,
        keywords: "разделить разбить split",
    },
    Spec {
        cmd: Cmd::CombineParts,
        label: "Объединить",
        category: "Подгонка",
        shortcut: None,
        keywords: "объединить соединить combine",
    },
    Spec {
        cmd: Cmd::ExtendTo,
        label: "Удлинить до точки",
        category: "Подгонка",
        shortcut: None,
        keywords: "удлинить продлить extend",
    },
    Spec {
        cmd: Cmd::AssemblyCreate,
        label: "Создать сборку",
        category: "Сборка",
        shortcut: None,
        keywords: "сборка создать из выбранных assembly",
    },
    Spec {
        cmd: Cmd::AssemblyAdd,
        label: "Добавить в сборку",
        category: "Сборка",
        shortcut: None,
        keywords: "сборка добавить деталь",
    },
    Spec {
        cmd: Cmd::AssemblyRemove,
        label: "Удалить из сборки",
        category: "Сборка",
        shortcut: None,
        keywords: "сборка удалить исключить деталь",
    },
    Spec {
        cmd: Cmd::AssemblySetMain,
        label: "Сделать главной деталью",
        category: "Сборка",
        shortcut: None,
        keywords: "сборка главная деталь",
    },
    Spec {
        cmd: Cmd::AssemblyExplode,
        label: "Разбить сборку",
        category: "Сборка",
        shortcut: None,
        keywords: "сборка разбить разобрать",
    },
    Spec {
        cmd: Cmd::DrawDimension,
        label: "Чертёж: размер по точкам",
        category: "Чертежи",
        shortcut: None,
        keywords: "размер чертёж цепь",
    },
    Spec {
        cmd: Cmd::DrawLine,
        label: "Чертёж: линия",
        category: "Чертежи",
        shortcut: None,
        keywords: "линия чертёж",
    },
    Spec {
        cmd: Cmd::DrawPolyline,
        label: "Чертёж: полилиния",
        category: "Чертежи",
        shortcut: None,
        keywords: "полилиния ломаная",
    },
    Spec {
        cmd: Cmd::DrawRectangle,
        label: "Чертёж: прямоугольник",
        category: "Чертежи",
        shortcut: None,
        keywords: "прямоугольник рамка",
    },
    Spec {
        cmd: Cmd::DrawCircle,
        label: "Чертёж: окружность",
        category: "Чертежи",
        shortcut: None,
        keywords: "окружность круг",
    },
    Spec {
        cmd: Cmd::DrawHatch,
        label: "Чертёж: штриховка",
        category: "Чертежи",
        shortcut: None,
        keywords: "штриховка разрез сечение",
    },
    Spec {
        cmd: Cmd::DrawWipeout,
        label: "Чертёж: маскировка",
        category: "Чертежи",
        shortcut: None,
        keywords: "маскировка скрыть wipeout",
    },
    Spec {
        cmd: Cmd::DrawText,
        label: "Чертёж: текст",
        category: "Чертежи",
        shortcut: None,
        keywords: "текст надпись примечание",
    },
    Spec {
        cmd: Cmd::DrawNode,
        label: "Чертёж: узел",
        category: "Чертежи",
        shortcut: None,
        keywords: "узел выносной элемент фрагмент",
    },
    Spec {
        cmd: Cmd::DocumentInfo,
        label: "Сведения о документе",
        category: "Чертежи",
        shortcut: None,
        keywords: "основная надпись штамп гост обозначение организация разраб подпись",
    },
    Spec {
        cmd: Cmd::ReleaseSet,
        label: "Выпустить комплект",
        category: "Ведомости",
        shortcut: s(SHIFT_CMD, Key::R),
        keywords: "комплект pdf выпуск",
    },
    Spec {
        cmd: Cmd::CancelJob,
        label: "Отменить длительную операцию",
        category: "Справка",
        shortcut: None,
        keywords: "стоп прервать",
    },
];

pub fn spec(cmd: Cmd) -> &'static Spec {
    ALL.iter().find(|s| s.cmd == cmd).unwrap_or(&ALL[0])
}

/// macOS-style shortcut text, e.g. «⇧⌘Z», «Alt+B», «F2».
pub fn shortcut_text(cmd: Cmd) -> String {
    let Some(sc) = spec(cmd).shortcut else {
        return String::new();
    };
    let mut text = String::new();
    if sc.modifiers.alt {
        // The bundled fonts have no ⌥ glyph; Option is labelled «Alt».
        text.push_str("Alt+");
    }
    if sc.modifiers.ctrl && !sc.modifiers.command {
        text.push_str("Ctrl+");
    }
    if sc.modifiers.shift {
        text.push('⇧');
    }
    if sc.modifiers.command {
        text.push('⌘');
    }
    let key = match sc.logical_key {
        Key::Enter => "↩".to_string(),
        Key::Escape => "Esc".to_string(),
        Key::Num1 => "1".into(),
        Key::Num2 => "2".into(),
        Key::Num3 => "3".into(),
        Key::Num4 => "4".into(),
        Key::Space => "Пробел".into(),
        other => other.name().to_string(),
    };
    text + &key
}

/// Button caption with the shortcut, e.g. «Сохранить проект · ⌘S».
pub fn caption(cmd: Cmd) -> String {
    let shortcut = shortcut_text(cmd);
    if shortcut.is_empty() {
        spec(cmd).label.into()
    } else {
        format!("{} · {shortcut}", spec(cmd).label)
    }
}

/// Case-insensitive search over label, category and keywords; every word
/// of the query must match.
pub fn search(query: &str) -> Vec<Cmd> {
    let query = query.to_lowercase();
    let words: Vec<&str> = query.split_whitespace().collect();
    ALL.iter()
        .filter(|s| {
            let hay = format!(
                "{} {} {} {}",
                s.label,
                s.category,
                s.keywords,
                shortcut_text(s.cmd)
            )
            .to_lowercase();
            words.iter().all(|w| hay.contains(w))
        })
        .map(|s| s.cmd)
        .collect()
}

/// Shortcuts pressed this frame. Single keys without modifiers are ignored
/// while a text field has keyboard focus, so typing never triggers commands.
pub fn pressed(ctx: &eframe::egui::Context) -> Vec<Cmd> {
    let typing = ctx.egui_wants_keyboard_input();
    let mut result = Vec::new();
    // Longer modifier sets first, so ⇧⌘Z is not also read as ⌘Z.
    let mut specs: Vec<&Spec> = ALL.iter().filter(|s| s.shortcut.is_some()).collect();
    specs.sort_by_key(|s| {
        let m = s.shortcut.map(|k| k.modifiers).unwrap_or_default();
        std::cmp::Reverse(m.shift as u8 + m.command as u8 + m.alt as u8 + m.ctrl as u8)
    });
    for spec in specs {
        let Some(sc) = spec.shortcut else { continue };
        let plain = sc.modifiers.is_none()
            && !matches!(
                sc.logical_key,
                Key::F1 | Key::F2 | Key::F3 | Key::F4 | Key::F6 | Key::Escape
            );
        if typing && plain {
            continue;
        }
        if ctx.input_mut(|i| i.consume_shortcut(&sc)) {
            result.push(spec.cmd);
        }
    }
    result
}
