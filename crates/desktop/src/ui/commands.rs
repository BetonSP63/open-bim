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
    ViewFront,
    ViewTop,
    ViewRight,
    ToolAxis,
    ToolBeam,
    ToolColumn,
    ToolPlate,
    ToolGusset,
    ToolStiffener,
    CreateElement,
    CancelTool,
    FinishPicks,
    EditProperty,
    ApplyProperty,
    NumberAndCheck,
    ExportIfc,
    ImportIfc,
    PartDrawing,
    AssemblyDrawing,
    GeneralDrawing,
    UpdateDrawing,
    ExportPdf,
    ExportDxf,
    RefreshReports,
    AutoAssemblies,
    ExportCsv,
    ReleaseSet,
    CancelJob,
}

const CMD: Modifiers = Modifiers::COMMAND;
const SHIFT_CMD: Modifiers = Modifiers {
    shift: true,
    ..Modifiers::COMMAND
};
const ALT: Modifiers = Modifiers::ALT;
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
        shortcut: s(NONE, Key::F),
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
        shortcut: s(ALT, Key::P),
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
        cmd: Cmd::CreateElement,
        label: "Создать элемент по полям инструмента",
        category: "Моделирование",
        shortcut: s(CMD, Key::Enter),
        keywords: "создать построить",
    },
    Spec {
        cmd: Cmd::FinishPicks,
        label: "Завершить контур / повторить команду",
        category: "Моделирование",
        shortcut: s(NONE, Key::Enter),
        keywords: "enter завершить повторить",
    },
    Spec {
        cmd: Cmd::CancelTool,
        label: "Отменить инструмент",
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
        cmd: Cmd::ExportCsv,
        label: "Экспорт ведомостей CSV",
        category: "Ведомости",
        shortcut: None,
        keywords: "csv excel",
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
        std::cmp::Reverse(m.shift as u8 + m.command as u8 + m.alt as u8)
    });
    for spec in specs {
        let Some(sc) = spec.shortcut else { continue };
        let plain = sc.modifiers.is_none()
            && !matches!(
                sc.logical_key,
                Key::F1 | Key::F2 | Key::F3 | Key::F4 | Key::Escape
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
