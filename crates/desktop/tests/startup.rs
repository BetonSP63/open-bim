use open_bim_desktop::startup::{DEFAULT_PROJECT_NAME, packaged_project, prepare_project};
use std::fs;
use std::sync::{Arc, Barrier};

#[test]
fn first_launch_creates_an_independent_copy_in_redirected_documents() {
    let root = tempfile::tempdir().unwrap();
    let app = root.path().join("Программы с пробелами/Open BIM");
    fs::create_dir_all(app.join("examples")).unwrap();
    let example = app.join("examples/steel-release.obim");
    fs::write(&example, b"packaged example").unwrap();
    let documents = root.path().join("OneDrive/Документы пользователя");
    let project = packaged_project(&app.join("Open BIM.exe"), &documents)
        .unwrap()
        .unwrap();
    assert_eq!(project, documents.join(DEFAULT_PROJECT_NAME));
    assert_eq!(fs::read(&project).unwrap(), b"packaged example");
    fs::write(&project, b"my work").unwrap();
    assert_eq!(fs::read(&example).unwrap(), b"packaged example");
    assert_eq!(
        packaged_project(&app.join("Open BIM.exe"), &documents).unwrap(),
        Some(project.clone())
    );
    assert_eq!(fs::read(project).unwrap(), b"my work");
}

#[test]
fn existing_project_does_not_require_a_bundled_example() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join(DEFAULT_PROJECT_NAME);
    fs::write(&project, b"existing").unwrap();
    assert_eq!(
        packaged_project(&root.path().join("missing/Open BIM.exe"), root.path()).unwrap(),
        Some(project.clone())
    );
    assert_eq!(fs::read(project).unwrap(), b"existing");
}

#[test]
fn simultaneous_first_launches_publish_one_complete_copy() {
    let root = tempfile::tempdir().unwrap();
    let example = root.path().join("example.obim");
    let content = vec![42; 1024 * 1024];
    fs::write(&example, &content).unwrap();
    let project = root.path().join("Документы/Модель.obim");
    let barrier = Arc::new(Barrier::new(8));
    std::thread::scope(|scope| {
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let (example, project, barrier) = (&example, &project, barrier.clone());
                scope.spawn(move || {
                    barrier.wait();
                    prepare_project(example, project)
                })
            })
            .collect();
        for thread in threads {
            assert_eq!(thread.join().unwrap().unwrap(), project);
        }
    });
    assert_eq!(fs::read(&project).unwrap(), content);
    assert_eq!(
        fs::read_dir(project.parent().unwrap()).unwrap().count(),
        1,
        "no staging files left"
    );
}

#[test]
fn invalid_paths_and_missing_example_leave_no_partial_project() {
    let root = tempfile::tempdir().unwrap();
    let example = root.path().join("example.obim");
    let project = root.path().join("project.obim");
    assert!(prepare_project(&example, &project).is_err());
    assert!(!project.exists());
    fs::write(&example, b"example").unwrap();
    fs::create_dir(&project).unwrap();
    assert!(prepare_project(&example, &project).is_err());
    assert!(project.is_dir());
    assert_eq!(fs::read(example).unwrap(), b"example");
}

#[test]
fn development_binary_without_example_stays_unbound() {
    let root = tempfile::tempdir().unwrap();
    let documents = root.path().join("Documents");
    assert!(
        packaged_project(&root.path().join("Open BIM.exe"), &documents)
            .unwrap()
            .is_none()
    );
    assert!(!documents.exists());
}

#[test]
fn linux_documents_uses_xdg_and_falls_back_to_home() {
    use open_bim_desktop::startup::linux_documents;
    let root = tempfile::tempdir().unwrap();
    let home = root.path().to_path_buf();
    let xdg = home.join("Документы с пробелами");
    assert_eq!(
        linux_documents(Some(xdg.clone()), Some(home.clone())),
        Some(xdg)
    );
    assert_eq!(
        linux_documents(None, Some(home.clone())),
        Some(home.clone())
    );
    let documents = home.join("Documents");
    fs::create_dir(&documents).unwrap();
    assert_eq!(linux_documents(None, Some(home)), Some(documents));
}
