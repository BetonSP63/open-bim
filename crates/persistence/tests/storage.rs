use open_bim_model::*;
use open_bim_persistence::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

#[test]
fn arbitrary_float_coordinates_and_rotations_keep_checksum_identity() {
    for i in 1..128 {
        let mut project = example_steel_frame().unwrap();
        let angle = i as f64 * 0.317;
        let frame = project
            .elements
            .get_mut(&ObjectId(8))
            .unwrap()
            .shape
            .frame_mut();
        frame.origin = Point3([angle.sin() * 1e6, angle.cos() * 123456.789, angle * 137.19]);
        frame.axes = [
            [angle.cos(), angle.sin(), 0.0],
            [-angle.sin(), angle.cos(), 0.0],
            [0.0, 0.0, 1.0],
        ];
        let bytes = encode(&project).unwrap();
        assert_eq!(decode(&bytes).unwrap(), project);
    }
}

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "open-bim-storage-{}-{}",
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

#[test]
fn versioned_unicode_project_round_trip_preserves_identity_and_geometry() {
    let temp = Temp::new();
    let path = temp.0.join("Проект с пробелами.obim");
    let project = example_steel_frame().unwrap();
    assert_eq!(
        decode(include_bytes!(
            "../../../tests/fixtures/projects/steel-frame-v1.obim"
        ))
        .unwrap(),
        project
    );
    save_atomic(&path, &project).unwrap();
    assert_eq!(load(&path).unwrap(), project);
    let bytes = std::fs::read(&path).unwrap();
    let envelope: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(envelope["format_version"], 1);
    assert_eq!(envelope["payload_sha256"].as_str().unwrap().len(), 64);
}

#[test]
fn injected_failures_leave_previous_bytes_and_no_temporary_files() {
    let temp = Temp::new();
    let path = temp.0.join("project.obim");
    let original = example_steel_frame().unwrap();
    save_atomic(&path, &original).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let mut changed = original.clone();
    changed.name = "Новая версия".into();
    for fault in [
        SaveFault::AfterCreate,
        SaveFault::AfterWrite,
        SaveFault::AfterSync,
    ] {
        assert!(matches!(
            save_atomic_with_fault(&path, &changed, fault),
            Err(PersistenceError::InjectedFailure)
        ));
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(load(&path).unwrap(), original);
        assert_eq!(std::fs::read_dir(&temp.0).unwrap().count(), 1);
    }
    save_atomic(&path, &changed).unwrap();
    assert_eq!(load(path).unwrap(), changed);
}

#[test]
fn corrupt_unknown_and_oversized_files_are_rejected_without_writes() {
    let temp = Temp::new();
    let path = temp.0.join("invalid.obim");
    for bytes in [
        b"bad json".to_vec(),
        b"{\"magic\":\"open-bim-project\",\"format_version\":999}".to_vec(),
    ] {
        std::fs::write(&path, &bytes).unwrap();
        let error = load(&path).unwrap_err();
        assert!(!error.to_string().is_empty());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
    let mut envelope: serde_json::Value =
        serde_json::from_slice(&encode(&example_steel_frame().unwrap()).unwrap()).unwrap();
    envelope["project"]["name"] = serde_json::json!("Corrupted content");
    let bytes = serde_json::to_vec(&envelope).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    assert!(matches!(
        load(&path),
        Err(PersistenceError::ChecksumMismatch)
    ));
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(MAX_PROJECT_BYTES + 1).unwrap();
    assert!(matches!(load(&path), Err(PersistenceError::LimitExceeded)));
    assert_eq!(file.metadata().unwrap().len(), MAX_PROJECT_BYTES + 1);
}

#[test]
fn legacy_version_migrates_in_memory_and_unknown_version_is_precise() {
    let bytes = include_bytes!("../../../tests/fixtures/projects/legacy-v0.obim");
    let project = decode(bytes).unwrap();
    assert_eq!(project, example_steel_frame().unwrap());
    let new = encode(&project).unwrap();
    assert_eq!(decode(&new).unwrap(), project);
    assert!(matches!(
        decode(br#"{"magic":"open-bim-project","format_version":999}"#),
        Err(PersistenceError::UnsupportedVersion(999))
    ));
}

#[test]
fn autosave_recovers_last_committed_checkpoint_and_reports_partial_tail() {
    let temp = Temp::new();
    let path = temp.0.join("autosave.obima");
    let first = example_steel_frame().unwrap();
    append_checkpoint(&path, &first).unwrap();
    let mut second = first.clone();
    second.name = "Committed edit".into();
    second.revision = 1;
    append_checkpoint(&path, &second).unwrap();
    use std::io::Write;
    std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(&[25, 0, 0])
        .unwrap();
    let original = std::fs::read(&path).unwrap();
    let recovered = recover_checkpoints(&path).unwrap();
    assert_eq!(recovered.project, second);
    assert_eq!(recovered.checkpoints, 2);
    assert_eq!(recovered.discarded_tail_bytes, 3);
    assert!(matches!(
        append_checkpoint(&path, &first),
        Err(PersistenceError::InvalidFormat(_))
    ));
    assert_eq!(std::fs::read(path).unwrap(), original);
}

#[test]
fn committed_autosave_corruption_is_rejected() {
    let temp = Temp::new();
    let path = temp.0.join("autosave.obima");
    append_checkpoint(&path, &example_steel_frame().unwrap()).unwrap();
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[20] ^= 1;
    std::fs::write(&path, &bytes).unwrap();
    assert!(matches!(
        recover_checkpoints(&path),
        Err(PersistenceError::ChecksumMismatch)
    ));
    assert_eq!(std::fs::read(path).unwrap(), bytes);
}

#[test]
#[ignore = "Executed by forced_termination_recovers_last_completed_transaction subprocess"]
fn autosave_child() {
    let path = std::env::var("OPEN_BIM_CRASH_JOURNAL").unwrap();
    let marker = std::env::var("OPEN_BIM_CRASH_READY").unwrap();
    let mut document = open_bim_commands::Document::new(example_steel_frame().unwrap()).unwrap();
    document
        .transact(
            "Committed before crash",
            &[open_bim_commands::Command::RenameProject {
                name: "Последняя завершённая транзакция".into(),
            }],
        )
        .unwrap();
    append_checkpoint(&path, document.project()).unwrap();
    std::fs::write(marker, b"ready").unwrap();
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

#[test]
fn forced_termination_recovers_last_completed_transaction() {
    let temp = Temp::new();
    let path = temp.0.join("crash.obima");
    let marker = temp.0.join("ready");
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "autosave_child"])
        .env("OPEN_BIM_CRASH_JOURNAL", &path)
        .env("OPEN_BIM_CRASH_READY", &marker)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !marker.exists() && std::time::Instant::now() < deadline {
        if child.try_wait().unwrap().is_some() {
            panic!("Autosave child exited before checkpoint");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let ready = marker.exists();
    child.kill().unwrap();
    let status = child.wait().unwrap();
    assert!(ready, "Autosave checkpoint timeout");
    assert!(!status.success());
    let recovered = recover_checkpoints(path).unwrap();
    assert_eq!(recovered.project.name, "Последняя завершённая транзакция");
    assert_eq!(recovered.project.revision, 1);
    assert_eq!(recovered.checkpoints, 1);
    assert_eq!(recovered.discarded_tail_bytes, 0);
}

#[test]
fn cyrillic_folders_hold_saves_overwrites_and_autosave_journals() {
    let temp = Temp::new();
    let folder = temp.0.join("Объект «Склад» №2").join("Конструкции КМ");
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join("Каркас.obim");
    let mut project = example_steel_frame().unwrap();
    save_atomic(&path, &project).unwrap();
    project.name = "Каркас, изм. 1".into();
    project.revision = 1;
    save_atomic(&path, &project).unwrap();
    assert_eq!(load(&path).unwrap(), project);

    let journal = folder.join("Каркас.obima");
    append_checkpoint(&journal, &project).unwrap();
    append_checkpoint(&journal, &project).unwrap();
    let recovered = recover_checkpoints(&journal).unwrap();
    assert_eq!(recovered.project, project);
    assert_eq!(recovered.checkpoints, 2);
    let names: Vec<_> = std::fs::read_dir(&folder)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(names.len(), 2, "temporary files left behind: {names:?}");
}
