//! Native close requests must not discard work before the autosave timer fires.
use super::Workbench;
use eframe::egui;

#[derive(Clone, Copy, Debug)]
pub enum CloseChoice {
    Save,
    Discard,
    Cancel,
}

#[derive(Default)]
pub(super) struct CloseState {
    pending: bool,
    approved: bool,
    error: Option<String>,
}

impl Workbench {
    /// True when the native window may close. Otherwise ask the user first.
    pub fn request_close(&mut self) -> bool {
        if self.close.approved {
            return true;
        }
        if self.close.pending {
            return false;
        }
        if !self.tools.autosave.modified(self.editor.document.project()) {
            return true;
        }
        self.close.pending = true;
        self.close.error = None;
        false
    }

    pub fn close_pending(&self) -> bool {
        self.close.pending
    }

    /// A failed save keeps the model, history and dialog available for retry.
    pub fn resolve_close(&mut self, choice: CloseChoice) -> bool {
        if !self.close.pending {
            return false;
        }
        match choice {
            CloseChoice::Cancel => {
                self.close = CloseState::default();
                return false;
            }
            CloseChoice::Save => {
                if let Err(error) = self.save_project() {
                    self.close.error = Some(error);
                    return false;
                }
            }
            CloseChoice::Discard => {}
        }
        self.close.pending = false;
        self.close.approved = true;
        true
    }

    /// Called before processing model commands, including the first close frame.
    pub fn native_close_request(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && !self.request_close() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
    }

    pub fn close_dialog(&mut self, ctx: &egui::Context) {
        if !self.close.pending {
            return;
        }
        let mut choice = None;
        let response = egui::Modal::new(egui::Id::new("unsaved project close")).show(ctx, |ui| {
            ui.set_max_width(440.);
            ui.heading("Сохранить изменения перед выходом?");
            ui.label(format!("Проект: {}", self.editor.document.project().name));
            ui.label("Последние изменения ещё не сохранены в файл проекта.");
            ui.label(format!("Файл: {}", self.tools.path));
            if let Some(error) = &self.close.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
                ui.label("Проект остаётся открытым. Исправьте причину ошибки или отмените выход.");
            }
            ui.horizontal(|ui| {
                if ui.button("Сохранить и выйти").clicked() {
                    choice = Some(CloseChoice::Save);
                }
                if ui.button("Не сохранять").clicked() {
                    choice = Some(CloseChoice::Discard);
                }
                if ui.button("Отмена").clicked() {
                    choice = Some(CloseChoice::Cancel);
                }
            });
        });
        if choice.is_none() && response.should_close() {
            choice = Some(CloseChoice::Cancel);
        }
        if let Some(choice) = choice
            && self.resolve_close(choice)
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}
