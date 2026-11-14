mod headless;
use eframe::egui;
use headless::HeadlessFrame;
use open_bim_commands::Command;
use open_bim_desktop::autosave::autosave_path;
use open_bim_desktop::tools::Editor;
use open_bim_desktop::ui::{Workbench, close::CloseChoice, project_change::ProjectChange};
use open_bim_model::Project;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "open-bim-change-{}-{}",
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
    b.set_project_path(temp.0.join("Текущий проект.obim"));
    b.save_project().unwrap();
    b
}

fn change(b: &mut Workbench) {
    b.editor
        .commit_data(
            "Правка",
            &[Command::RenameProject {
                name: "Несохранённая работа".into(),
            }],
        )
        .unwrap();
}

fn write_project(path: &Path, name: &str) -> Project {
    let p = Project {
        name: name.into(),
        ..Default::default()
    };
    open_bim_persistence::save_atomic(path, &p).unwrap();
    p
}

fn wait(b: &mut Workbench) {
    let start = Instant::now();
    while b.busy() && start.elapsed() < Duration::from_secs(10) {
        b.poll_job();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!b.busy(), "open timed out");
}

#[test]
fn cancelling_each_replacement_preserves_model_history_and_destination() {
    for replacement in [
        ProjectChange::New,
        ProjectChange::Example,
        ProjectChange::Open("unused.obim".into()),
    ] {
        let temp = Temp::new();
        let mut b = bench(&temp);
        let current_path = b.tools.path.clone();
        let original = std::fs::read(&current_path).unwrap();
        change(&mut b);
        let changed = b.editor.document.project().clone();
        let history = b.editor.document.history_depth();
        b.tools.path = temp.0.join("Кандидат.obim").to_string_lossy().into_owned();
        assert!(b.change_project(replacement).is_none());
        assert!(b.project_change_pending());
        assert!(!b.busy());
        assert_eq!(b.tools.path, current_path);
        assert!(b.resolve_project_change(CloseChoice::Cancel).is_none());
        assert!(!b.project_change_pending());
        assert_eq!(b.editor.document.project(), &changed);
        assert_eq!(b.editor.document.history_depth(), history);
        assert_eq!(std::fs::read(&current_path).unwrap(), original);
    }
}

#[test]
fn save_then_open_writes_outgoing_model_only_to_its_own_file() {
    let temp = Temp::new();
    let mut b = bench(&temp);
    let current_path = b.tools.path.clone();
    let target = temp.0.join("Другой проект.obim");
    let other = write_project(&target, "Другой проект");
    let other_bytes = std::fs::read(&target).unwrap();
    change(&mut b);
    let changed = b.editor.document.project().clone();
    b.tools.path = target.to_string_lossy().into_owned();
    b.start_open(target.clone());
    assert!(b.project_change_pending());
    assert_eq!(b.tools.path, current_path);
    b.resolve_project_change(CloseChoice::Save);
    assert_eq!(open_bim_persistence::load(&current_path).unwrap(), changed);
    assert_eq!(std::fs::read(&target).unwrap(), other_bytes);
    assert_eq!(
        b.tools.path, current_path,
        "destination changes only on success"
    );
    wait(&mut b);
    assert_eq!(b.editor.document.project(), &other);
    assert_eq!(b.tools.path, target.to_string_lossy());
    assert_eq!(b.editor.document.history_depth(), (0, 0));
}

#[test]
fn new_project_uses_free_name_and_is_unsaved_even_before_the_first_edit() {
    let temp = Temp::new();
    let mut b = bench(&temp);
    let original_path = b.tools.path.clone();
    let original = std::fs::read(&original_path).unwrap();
    write_project(&temp.0.join("Новый проект.obim"), "Уже существует");
    std::fs::write(
        autosave_path(&temp.0.join("Новый проект 2.obim")),
        "recovery",
    )
    .unwrap();
    change(&mut b);
    b.change_project(ProjectChange::New);
    assert!(b.resolve_project_change(CloseChoice::Discard).is_some());
    assert_eq!(
        PathBuf::from(&b.tools.path),
        temp.0.join("Новый проект 3.obim")
    );
    assert!(!Path::new(&b.tools.path).exists());
    assert!(
        !b.request_close(),
        "the generated document has not been saved yet"
    );
    b.resolve_close(CloseChoice::Cancel);
    b.save_project().unwrap();
    assert_eq!(std::fs::read(&original_path).unwrap(), original);
    assert_eq!(
        open_bim_persistence::load(&b.tools.path).unwrap(),
        *b.editor.document.project()
    );
}

#[test]
fn example_gets_a_separate_destination_and_an_unchanged_example_can_autosave() {
    let temp = Temp::new();
    let mut b = bench(&temp);
    let original = b.tools.path.clone();
    assert!(b.change_project(ProjectChange::Example).is_some());
    assert_ne!(b.tools.path, original);
    assert!(b.tools.autosave.modified(b.editor.document.project()));
    b.autosave_tick(Instant::now() + b.tools.autosave.interval() + Duration::from_secs(1));
    assert_eq!(
        open_bim_persistence::load(autosave_path(Path::new(&b.tools.path))).unwrap(),
        *b.editor.document.project()
    );
}

#[test]
fn failed_and_cancelled_open_leave_unsaved_work_and_save_path_intact() {
    let temp = Temp::new();
    let mut b = bench(&temp);
    let current_path = b.tools.path.clone();
    let target = temp.0.join("broken.obim");
    std::fs::write(&target, "broken").unwrap();
    change(&mut b);
    let changed = b.editor.document.project().clone();
    let history = b.editor.document.history_depth();
    for cancelled in [false, true] {
        b.load_delay_ms = if cancelled { 5000 } else { 0 };
        b.tools.path = target.to_string_lossy().into_owned();
        b.start_open(target.clone());
        b.resolve_project_change(CloseChoice::Discard);
        // A file field edited while the background job is running must not
        // become the current document's destination when that job fails.
        b.tools.path = target.to_string_lossy().into_owned();
        if cancelled {
            b.cancel_job();
        } else {
            wait(&mut b);
        }
        assert_eq!(b.tools.path, current_path);
        assert_eq!(b.editor.document.project(), &changed);
        assert_eq!(b.editor.document.history_depth(), history);
        assert!(b.tools.autosave.modified(b.editor.document.project()));
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "broken");
    }
    b.save_project().unwrap();
    assert_eq!(open_bim_persistence::load(&current_path).unwrap(), changed);
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "broken");
}

#[test]
fn failed_save_blocks_new_document_and_can_be_retried() {
    let temp = Temp::new();
    let mut b = bench(&temp);
    let parent = temp.0.join("blocked");
    std::fs::write(&parent, "occupied").unwrap();
    b.set_project_path(parent.join("current.obim"));
    change(&mut b);
    let changed = b.editor.document.project().clone();
    b.change_project(ProjectChange::New);
    assert!(b.resolve_project_change(CloseChoice::Save).is_none());
    assert!(b.project_change_pending());
    assert_eq!(b.editor.document.project(), &changed);
    std::fs::remove_file(&parent).unwrap();
    std::fs::create_dir(&parent).unwrap();
    assert!(b.resolve_project_change(CloseChoice::Save).is_some());
    assert_eq!(
        open_bim_persistence::load(parent.join("current.obim")).unwrap(),
        changed
    );
}

#[test]
fn autosave_ignores_an_unaccepted_path_in_the_file_field() {
    let temp = Temp::new();
    let mut b = bench(&temp);
    let current_path = PathBuf::from(&b.tools.path);
    let candidate = temp.0.join("not-opened.obim");
    change(&mut b);
    b.tools.path = candidate.to_string_lossy().into_owned();
    b.autosave_tick(Instant::now() + b.tools.autosave.interval() + Duration::from_secs(1));
    assert!(autosave_path(&current_path).exists());
    assert!(!autosave_path(&candidate).exists());
}

#[test]
fn recovery_waits_for_consent_and_declining_does_not_prompt_twice() {
    let temp = Temp::new();
    let mut b = bench(&temp);
    let target = temp.0.join("recover.obim");
    let other = write_project(&target, "Файл");
    let snapshot = autosave_path(&target);
    write_project(&snapshot, "Автосохранение");
    let later = SystemTime::now() + Duration::from_secs(5);
    std::fs::File::options()
        .write(true)
        .open(&snapshot)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(later))
        .unwrap();
    change(&mut b);
    b.start_open(target.clone());
    assert!(b.tools.autosave.offer.is_none());
    b.resolve_project_change(CloseChoice::Discard);
    assert!(b.tools.autosave.offer.is_some());
    b.decline_autosave();
    assert!(!b.project_change_pending());
    wait(&mut b);
    assert_eq!(b.editor.document.project(), &other);
    assert_eq!(b.tools.path, target.to_string_lossy());
    assert!(snapshot.exists());
}

#[test]
fn corrupt_recovery_does_not_change_current_path_or_model() {
    let temp = Temp::new();
    let mut b = bench(&temp);
    let original_path = b.tools.path.clone();
    change(&mut b);
    let changed = b.editor.document.project().clone();
    let target = temp.0.join("missing.obim");
    std::fs::write(autosave_path(&target), "broken snapshot").unwrap();
    b.start_open(target);
    b.resolve_project_change(CloseChoice::Discard);
    b.recover_autosave();
    wait(&mut b);
    assert_eq!(b.tools.path, original_path);
    assert_eq!(b.editor.document.project(), &changed);
    assert!(b.tools.autosave.modified(b.editor.document.project()));
}

#[test]
fn escape_cancels_replacement_and_native_close_cannot_bypass_the_prompt() {
    let temp = Temp::new();
    let mut b = bench(&temp);
    change(&mut b);
    b.change_project(ProjectChange::New);
    let ctx = egui::Context::default();
    let mut input = egui::RawInput::default();
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .unwrap()
        .events
        .push(egui::ViewportEvent::Close);
    let output = ctx.run_headless(input, |ui| {
        b.native_close_request(ui.ctx());
        b.project_change_dialog(ui.ctx());
    });
    assert!(
        output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::CancelClose))
    );
    assert!(!b.close_pending());
    let _ = ctx.run_headless(egui::RawInput::default(), |ui| {
        b.project_change_dialog(ui.ctx());
    });
    let _ = ctx.run_headless(
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
        |ui| {
            b.project_change_dialog(ui.ctx());
        },
    );
    assert!(!b.project_change_pending());
    assert_eq!(b.editor.document.project().name, "Несохранённая работа");
}
