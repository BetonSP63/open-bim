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
