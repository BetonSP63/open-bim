//! Closing is explicit about unsaved work, including recovered snapshots.
mod headless;
use eframe::egui;
use headless::HeadlessFrame;
use open_bim_commands::Command;
use open_bim_desktop::autosave::{autosave_path, backup_path};
use open_bim_desktop::tools::Editor;
use open_bim_desktop::ui::{Workbench, close::CloseChoice};
use open_bim_model::Project;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "open-bim-close-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn bench(temp: &Temp) -> Workbench {
    let mut b = Workbench::new(Editor::new(Project::default()).unwrap());
    b.tools.path = temp.0.join("Проект.obim").to_string_lossy().into_owned();
    b
}

fn change(b: &mut Workbench) {
    b.editor
        .commit_data(
            "Название проекта",
            &[Command::RenameProject {
                name: "Несохранённые изменения".into(),
            }],
        )
        .unwrap();
}

#[test]
fn closing_unchanged_or_undone_work_does_not_prompt_but_redo_does() {
    let temp = Temp::new();
    let mut b = bench(&temp);
    assert!(b.request_close());
    change(&mut b);
    b.editor.document.undo().unwrap();
    assert!(b.request_close(), "undo restored the saved state");
    b.editor.document.redo().unwrap();
    assert!(!b.request_close());
    assert!(b.close_pending());
}

#[test]
fn cancel_preserves_model_and_history_even_with_autosave_disabled() {
    let temp = Temp::new();
    let mut b = bench(&temp);
    b.tools.autosave.enabled = false;
    change(&mut b);
    let project = b.editor.document.project().clone();
    let history = b.editor.document.history_depth();
    assert!(!b.request_close());
    assert!(!b.resolve_close(CloseChoice::Cancel));
    assert!(!b.close_pending());
    assert_eq!(b.editor.document.project(), &project);
    assert_eq!(b.editor.document.history_depth(), history);
    assert!(!PathBuf::from(&b.tools.path).exists());
    assert!(!b.request_close(), "a later close must prompt again");
}

#[test]
fn save_before_close_round_trips_the_project_and_keeps_previous_file() {
    let temp = Temp::new();
    let mut b = bench(&temp);
    b.save_project().unwrap();
    let original = b.editor.document.project().clone();
    change(&mut b);
    let changed = b.editor.document.project().clone();
    assert!(!b.request_close());
    assert!(b.resolve_close(CloseChoice::Save));
    assert!(!b.close_pending());
    assert!(b.request_close(), "confirmed exit is not prompted twice");
    let path = PathBuf::from(&b.tools.path);
    assert_eq!(open_bim_persistence::load(&path).unwrap(), changed);
    assert_eq!(
        open_bim_persistence::load(backup_path(&path)).unwrap(),
        original
    );
    assert!(!b.tools.autosave.modified(b.editor.document.project()));
}

#[test]
fn discard_does_not_save_or_delete_a_recovery_snapshot() {
    let temp = Temp::new();
    let mut b = bench(&temp);
    b.save_project().unwrap();
    let path = PathBuf::from(&b.tools.path);
    let original = std::fs::read(&path).unwrap();
    change(&mut b);
    b.tools
        .autosave
        .snapshot_now(
            std::time::Instant::now(),
            &path,
            b.editor.document.project(),
        )
        .unwrap();
    let snapshot = std::fs::read(autosave_path(&path)).unwrap();
    assert!(!b.request_close());
    assert!(b.resolve_close(CloseChoice::Discard));
    assert!(b.request_close());
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert_eq!(std::fs::read(autosave_path(&path)).unwrap(), snapshot);
}

#[test]
fn failed_save_keeps_dialog_and_history_until_retry_succeeds() {
    let temp = Temp::new();
    let mut b = bench(&temp);
    change(&mut b);
    let project = b.editor.document.project().clone();
    let history = b.editor.document.history_depth();
    let blocker = temp.0.join("not-a-directory");
    std::fs::write(&blocker, "occupied").unwrap();
    b.tools.path = blocker.join("project.obim").to_string_lossy().into_owned();
    assert!(!b.request_close());
    assert!(!b.resolve_close(CloseChoice::Save));
    assert!(b.close_pending());
    assert!(!b.request_close());
    assert_eq!(b.editor.document.project(), &project);
    assert_eq!(b.editor.document.history_depth(), history);
    assert_eq!(std::fs::read_to_string(&blocker).unwrap(), "occupied");
    // Fix the filesystem problem while the dialog is still open, then retry.
    std::fs::remove_file(&blocker).unwrap();
    std::fs::create_dir(&blocker).unwrap();
    assert!(b.resolve_close(CloseChoice::Save));
    assert_eq!(open_bim_persistence::load(&b.tools.path).unwrap(), project);
}

#[test]
fn recovered_project_is_unsaved_even_before_any_further_edit() {
    let temp = Temp::new();
    let mut b = bench(&temp);
    b.tools.autosave.mark_recovered(b.editor.document.project());
    assert!(!b.request_close());
    assert!(!b.resolve_close(CloseChoice::Cancel));
    assert!(!b.request_close());
}

#[test]
fn native_close_is_cancelled_and_escape_dismisses_the_dialog() {
    let temp = Temp::new();
    let mut b = bench(&temp);
    change(&mut b);
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut input = egui::RawInput::default();
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .unwrap()
        .events
        .push(egui::ViewportEvent::Close);
    let output = ctx.run_headless(input, |ui| {
        b.native_close_request(ui.ctx());
        b.close_dialog(ui.ctx());
    });
    assert!(b.close_pending());
    assert!(
        output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::CancelClose))
    );
    let update = output.platform_output.accesskit_update.unwrap();
    for label in ["Сохранить и выйти", "Не сохранять", "Отмена"] {
        assert!(
            update.nodes.iter().any(|(_, n)| n.label() == Some(label)),
            "{label}"
        );
    }
    let _ = ctx.run_headless(egui::RawInput::default(), |ui| b.close_dialog(ui.ctx()));
    let output = ctx.run_headless(
        egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        },
        |ui| b.close_dialog(ui.ctx()),
    );
    assert!(!b.close_pending());
    assert!(
        !output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::Close))
    );
}
