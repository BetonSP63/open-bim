//! Command palette (⌘P): type to search, ↑/↓ to choose, Enter to run, Esc to close.
use super::commands::{ALL, Cmd, caption, search, shortcut_text, spec};
use eframe::egui;

#[derive(Default)]
pub struct Palette {
    pub open: bool,
    pub query: String,
    pub selected: usize,
    focus: bool,
}

impl Palette {
    pub fn toggle(&mut self) {
        self.open = !self.open;
        self.query.clear();
        self.selected = 0;
        self.focus = self.open;
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Option<Cmd> {
        if !self.open {
            return None;
        }
        let results = search(&self.query);
        let mut chosen = None;
        let (up, down, enter, escape) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::ArrowUp),
                i.key_pressed(egui::Key::ArrowDown),
                i.key_pressed(egui::Key::Enter),
                i.key_pressed(egui::Key::Escape),
            )
        });
        if down && self.selected + 1 < results.len() {
            self.selected += 1;
        }
        if up {
            self.selected = self.selected.saturating_sub(1);
        }
        egui::Window::new("Поиск команд")
            .id(egui::Id::new("command palette"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0., 80.))
            .default_width(460.)
            .show(ctx, |ui| {
                let label = ui.label("Команда:");
                let edit = ui
                    .add(
                        egui::TextEdit::singleline(&mut self.query)
                            .hint_text("например: балка, pdf, коллизии")
                            .desired_width(440.),
                    )
                    .labelled_by(label.id);
                if self.focus {
                    edit.request_focus();
                    self.focus = false;
                }
                if edit.changed() {
                    self.selected = 0;
                }
                ui.separator();
                if results.is_empty() {
                    ui.label("Ничего не найдено");
                }
                egui::ScrollArea::vertical()
                    .max_height(320.)
                    .show(ui, |ui| {
                        for (i, cmd) in results.iter().enumerate() {
                            let text = format!("{} — {}", spec(*cmd).category, caption(*cmd));
                            let response = ui.selectable_label(i == self.selected, text);
                            if i == self.selected {
                                response.scroll_to_me(None);
                            }
                            if response.clicked() {
                                chosen = Some(*cmd);
                            }
                        }
                    });
                ui.label("↑/↓ — выбор, Enter — выполнить, Esc — закрыть");
            });
        if enter && chosen.is_none() {
            chosen = results.get(self.selected).copied();
        }
        if escape || chosen.is_some() {
            self.open = false;
        }
        chosen
    }
}

/// Every keyboard-only path through the application (shown in F1).
pub const KEYBOARD_PATHS: &[&str] = &[
    "Alt (нажать и отпустить) или F10 — буквы на вкладках ленты; буква вкладки, затем буква команды; Esc — назад",
    "⌘P или быстрый запуск — любая команда по названию; Enter — выполнить первую",
    "F6 — следующая область окна: вид → лента → быстрый запуск → боковые панели → панель привязок; Tab / ⇧Tab — элементы области; Пробел — нажать",
    "Ctrl+F — поиск в модели (имя, профиль, материал, #ID); Enter — выбрать найденное",
    "В команде построения: набрать координаты (x,y,z, $, !, d<угол) или узел сетки «Б/2», «Б/2/+3,600»; Enter — указать",
    "Alt+стрелки — курсор к следующей точке привязки (в команде) или к следующей детали (без команды); Пробел — указать точку / выбрать деталь; Alt+Пробел — добавить/убрать из выбора",
    "Enter — замкнуть контур или повторить последнюю команду; Backspace — убрать последнюю точку (без команды — удалить выбранное); Esc — прервать",
    "Стрелки — сдвиг вида; Ctrl+стрелки — поворот 15°, ⇧стрелки — 5°; PgUp/PgDn — масштаб; Home — вписать; ⇧Пробел — вписать выбранное",
    "Мышь: колесо — масштаб к точке под курсором, средняя кнопка — сдвиг, Ctrl+средняя — вращение вокруг детали под курсором",
    "Трекпад: щипок — масштаб, два пальца — вращение вокруг точки под курсором, ⇧ + два пальца — сдвиг, жест поворота — поворот вида",
    "Ctrl+1 — каркас, Ctrl+2 — прозрачный (ползунок «Прозрачность» в панели видов), Ctrl+4 — тонированный",
    "Подгонка и резка (Правка): выбрать деталь, затем указать линию или деталь; линии — в рабочей плоскости; Enter — замкнуть контур выреза",
];

/// Step-by-step workflows of the steel commands (docs/help/bolts-welds.md).
pub const STEEL_WORKFLOWS: &[&str] = &[
    "Болты: основная деталь → присоединяемые → Enter → начало группы → направление X (болты видны заранее)",
    "Шаги болтов: «80 80 80» или «3*80»; сдвиг по X/Y и поворот на 90° — в панели свойств",
    "Шов: деталь, к которой приваривают → привариваемая деталь; тип, катет, стороны — в панели; заводской шов объединяет детали в сборку",
    "Сборки: создать из выбранных, добавить (выбрать детали → команда → деталь сборки), удалить, главная деталь, разбить",
    "Пластина — контур по точкам; косынка — две детали угла; ребро жёсткости — двутавр и место со стороны стенки",
];

pub fn help(ctx: &egui::Context, open: &mut bool) {
    egui::Window::new("Горячие клавиши")
        .id(egui::Id::new("shortcut help"))
        .open(open)
        .collapsible(false)
        .default_width(520.)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .max_height(520.)
                .show(ui, |ui| {
                    ui.strong("Работа без мыши");
                    for line in KEYBOARD_PATHS {
                        ui.label(*line);
                    }
                    ui.separator();
                    ui.strong("Болты, сварка, сборки, пластины");
                    for line in STEEL_WORKFLOWS {
                        ui.label(*line);
                    }
                    ui.separator();
                    egui::Grid::new("shortcut grid")
                        .striped(true)
                        .num_columns(3)
                        .show(ui, |ui| {
                            for spec in ALL {
                                ui.label(spec.category);
                                ui.label(spec.label);
                                ui.monospace(shortcut_text(spec.cmd));
                                ui.end_row();
                            }
                        });
                });
        });
}
