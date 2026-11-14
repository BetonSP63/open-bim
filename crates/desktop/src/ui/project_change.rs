//! Guard document replacement and accept file destinations only on success.
use super::{Workbench, close::CloseChoice};
use eframe::egui;
use open_bim_renderer::Scene;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub enum ProjectChange {
    New,
    Example,
    Open(PathBuf),
}

#[derive(Default)]
pub(super) struct ChangeState {
    pending: Option<ProjectChange>,
    error: Option<String>,
}

impl Workbench {
    /// Initialise the accepted destination (also used by native startup).
    pub fn set_project_path(&mut self, path: PathBuf) {
        self.tools.path = path.to_string_lossy().into_owned();
        self.project_path = path;
    }

    pub fn project_change_pending(&self) -> bool {
        self.change.pending.is_some()
    }

    /// All replacement entry points go here: ribbon, shortcuts and file field.
    /// New/example return a scene; opening finishes asynchronously in poll_job.
    pub fn change_project(&mut self, change: ProjectChange) -> Option<Arc<Scene>> {
        if self.busy() || self.tools.autosave.offer.is_some() {
            self.message("Завершите или отмените текущую операцию открытия проекта");
            return None;
        }
        if self.close_pending() || self.project_change_pending() {
            return None;
        }
        // The edited field may contain the file being opened. Saving the
        // outgoing document must still use its own accepted destination.
        self.tools.path = self.project_path.to_string_lossy().into_owned();
        if self.tools.autosave.modified(self.editor.document.project()) {
            self.change.pending = Some(change);
            self.change.error = None;
            return None;
        }
        self.perform_project_change(change)
    }

    pub fn resolve_project_change(&mut self, choice: CloseChoice) -> Option<Arc<Scene>> {
        self.change.pending.as_ref()?;
        if matches!(choice, CloseChoice::Cancel) {
            self.change = ChangeState::default();
            return None;
        }
        if matches!(choice, CloseChoice::Save) {
            self.tools.path = self.project_path.to_string_lossy().into_owned();
            if let Err(error) = self.save_project() {
                self.change.error = Some(error);
                return None;
            }
        }
        let change = self.change.pending.take()?;
        self.change.error = None;
        self.perform_project_change(change)
    }

    fn perform_project_change(&mut self, change: ProjectChange) -> Option<Arc<Scene>> {
        let (project, label) = match change {
            ProjectChange::Open(path) => {
                self.open_project(path);
                return None;
            }
            ProjectChange::New => (crate::interaction::tools::default_project(), "Новый проект"),
            ProjectChange::Example => (
                open_bim_model::example_steel_frame().map_err(|e| e.to_string()),
                "Контрольный каркас",
            ),
        };
        let result = project.and_then(|project| {
            let path = self.free_project_path(label)?;
            let scene = self.editor.replace(project, label)?;
            self.set_project_path(path);
            self.tools.autosave.mark_new();
            self.drawings = crate::drawing_editor::DrawingEditor::new();
            self.reports.reports = None;
            self.reload_model_views();
            Ok(scene)
        });
        match result {
            Ok(scene) => Some(scene),
            Err(error) => {
                self.message(error);
                None
            }
        }
    }

    fn free_project_path(&self, name: &str) -> Result<PathBuf, String> {
        let parent = self.project_path.parent().unwrap_or_else(|| Path::new("."));
        for number in 1..=10_000 {
            let filename = if number == 1 {
                format!("{name}.obim")
            } else {
                format!("{name} {number}.obim")
            };
            let path = parent.join(filename);
            if path != self.project_path
                && !path.exists()
                && !crate::autosave::autosave_path(&path).exists()
            {
                return Ok(path);
            }
        }
        Err("Не удалось подобрать свободное имя нового проекта".into())
    }

    pub fn project_change_dialog(&mut self, ctx: &egui::Context) -> Option<Arc<Scene>> {
        let title = match self.change.pending.as_ref()? {
            ProjectChange::New => "Сохранить изменения перед созданием проекта?",
            ProjectChange::Example => "Сохранить изменения перед загрузкой примера?",
            ProjectChange::Open(_) => "Сохранить изменения перед открытием файла?",
        };
        let choice = super::close::unsaved_dialog(
            ctx,
            title,
            "Сохранить и продолжить",
            self,
            self.change.error.as_deref(),
        )?;
        self.resolve_project_change(choice)
    }
}
