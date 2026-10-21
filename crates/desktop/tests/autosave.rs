//! Autosave next to the project file, «.bak» on save and recovery on open.
use open_bim_commands::Command;
use open_bim_desktop::autosave::{Autosave, autosave_path, backup_path};
use open_bim_desktop::tools::Editor;
use open_bim_desktop::ui::Workbench;
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn source() -> open_bim_model::Project {
    open_bim_persistence::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/steel_release/source.obim"
    ))
    .unwrap()
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("open-bim-autosave-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn rename(editor: &mut Editor, name: &str) {
    editor
        .commit_data(
            "Переименовать",
            &[Command::RenameProject { name: name.into() }],
        )
        .unwrap();
}

fn wait(b: &mut Workbench) {
    let started = Instant::now();
    while b.job.is_some() && started.elapsed().as_secs() < 60 {
        let _ = b.poll_job();
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn snapshot_only_when_due_and_changed() {
    let dir = temp_dir("tick");
    let path = dir.join("model.obim");
    assert_eq!(
        autosave_path(&path).file_name().unwrap(),
        "model.obim.autosave"
    );
    assert_eq!(backup_path(&path).file_name().unwrap(), "model.obim.bak");
    let mut editor = Editor::new(source()).unwrap();
    let mut autosave = Autosave::default();
    autosave.mark_saved(editor.document.project());
    let start = Instant::now();
    let later = start + autosave.interval() + Duration::from_secs(1);
    // Unchanged model: nothing is written even when due.
    assert_eq!(
        autosave
            .tick(later, &path, editor.document.project())
            .unwrap(),
        None
    );
    rename(&mut editor, "После правки");
    // Changed, but the interval has not passed since the last check.
    assert_eq!(
        autosave
            .tick(
                later + Duration::from_secs(5),
                &path,
                editor.document.project()
            )
            .unwrap(),
        None
    );
    let due = later + autosave.interval();
    let written = autosave
        .tick(due, &path, editor.document.project())
        .unwrap()
        .expect("snapshot written");
    assert_eq!(written, autosave_path(&path));
    assert_eq!(
        open_bim_persistence::load(&written).unwrap().name,
        "После правки"
    );
    // The same model is not written twice.
    let modified = std::fs::metadata(&written).unwrap().modified().unwrap();
    assert_eq!(
        autosave
            .tick(due + autosave.interval(), &path, editor.document.project())
            .unwrap(),
        None
    );
    assert_eq!(
        std::fs::metadata(&written).unwrap().modified().unwrap(),
        modified
    );
    // Switched off: nothing is written.
    rename(&mut editor, "Ещё правка");
    autosave.enabled = false;
    assert_eq!(
        autosave
            .tick(
                due + autosave.interval() * 3,
                &path,
                editor.document.project()
            )
            .unwrap(),
        None
    );
}

#[test]
fn save_keeps_backup_and_removes_snapshot() {
    let dir = temp_dir("save");
    let path = dir.join("model.obim");
    let mut editor = Editor::new(source()).unwrap();
    let mut autosave = Autosave::default();
    autosave.save(&path, editor.document.project()).unwrap();
    assert!(!backup_path(&path).exists(), "no backup of a new file");
    let first = editor.document.project().name.clone();
    rename(&mut editor, "Вторая версия");
    assert!(autosave.modified(editor.document.project()));
    autosave
        .snapshot_now(Instant::now(), &path, editor.document.project())
        .unwrap()
        .expect("snapshot");
    assert!(autosave_path(&path).exists());
    let message = autosave.save(&path, editor.document.project()).unwrap();
    assert!(message.contains(".bak"), "{message}");
    assert_eq!(
        open_bim_persistence::load(backup_path(&path)).unwrap().name,
        first
    );
    assert_eq!(
        open_bim_persistence::load(&path).unwrap().name,
        "Вторая версия"
    );
    assert!(!autosave_path(&path).exists(), "snapshot removed on save");
    assert!(!autosave.modified(editor.document.project()));
}

#[test]
fn opening_offers_newer_snapshot_and_restores_it() {
    let dir = temp_dir("recover");
    let path = dir.join("model.obim");
    open_bim_persistence::save_atomic(&path, &source()).unwrap();
    let mut crashed = source();
    crashed.name = "Несохранённая работа".into();
    // The snapshot is written after the file, as after a crash.
    std::thread::sleep(Duration::from_millis(20));
    open_bim_persistence::save_atomic(autosave_path(&path), &crashed).unwrap();

    let mut b = Workbench::new(Editor::new(source()).unwrap());
    b.tools.path = path.to_string_lossy().into_owned();
    b.start_open(path.clone());
    assert!(!b.busy(), "nothing is loaded before the answer");
    let offer = b.tools.autosave.offer.clone().expect("recovery offered");
    assert_eq!(offer.snapshot, autosave_path(&path));
    assert!(!offer.project_missing);

    b.recover_autosave();
    wait(&mut b);
    assert_eq!(b.editor.document.project().name, "Несохранённая работа");
    assert_eq!(
        b.tools.path,
        path.to_string_lossy(),
        "saves go to the project file"
    );
    assert!(b.tools.autosave.modified(b.editor.document.project()));
    let message = b.tools.message.clone().unwrap_or_default();
    assert!(message.starts_with("Восстановлено"), "{message}");

    // Saving writes the restored model to the project file.
    b.save_project().unwrap();
    assert_eq!(
        open_bim_persistence::load(&path).unwrap().name,
        "Несохранённая работа"
    );
    assert!(!autosave_path(&path).exists());
    b.start_open(path.clone());
    assert!(b.tools.autosave.offer.is_none(), "no snapshot, no offer");
    wait(&mut b);
}

#[test]
fn declined_snapshot_opens_the_file_and_stays_on_disk() {
    let dir = temp_dir("decline");
    let path = dir.join("model.obim");
    open_bim_persistence::save_atomic(&path, &source()).unwrap();
    let mut crashed = source();
    crashed.name = "Несохранённая работа".into();
    std::thread::sleep(Duration::from_millis(20));
    open_bim_persistence::save_atomic(autosave_path(&path), &crashed).unwrap();

    let mut b = Workbench::new(Editor::new(source()).unwrap());
    b.start_open(path.clone());
    assert!(b.tools.autosave.offer.is_some());
    b.decline_autosave();
    wait(&mut b);
    assert_eq!(b.editor.document.project().name, source().name);
    assert!(autosave_path(&path).exists(), "snapshot kept");
    // Not offered again in this session.
    b.start_open(path.clone());
    assert!(b.tools.autosave.offer.is_none());
    wait(&mut b);
}

#[test]
fn older_snapshot_is_not_offered() {
    let dir = temp_dir("older");
    let path = dir.join("model.obim");
    open_bim_persistence::save_atomic(autosave_path(&path), &source()).unwrap();
    std::thread::sleep(Duration::from_millis(20));
    open_bim_persistence::save_atomic(&path, &source()).unwrap();
    assert!(Autosave::default().recovery(&path).is_none());
    // No project file at all: the snapshot is the only copy.
    let lost = dir.join("lost.obim");
    open_bim_persistence::save_atomic(autosave_path(&lost), &source()).unwrap();
    assert!(Autosave::default().recovery(&lost).unwrap().project_missing);
}
