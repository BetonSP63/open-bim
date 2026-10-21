//! «Экспорт NC (DSTV)»: one .nc1 file per part mark in a folder next to the project.
use open_bim_desktop::tools::{Editor, ui::Action};
use open_bim_desktop::ui::{Request, Workbench, commands::Cmd};

fn source() -> open_bim_model::Project {
    open_bim_persistence::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/steel_release/source.obim"
    ))
    .unwrap()
}

#[test]
fn writes_one_file_per_mark_next_to_the_project() {
    let dir = std::env::temp_dir().join(format!("open-bim-nc-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let project = source();
    let marks: std::collections::BTreeSet<String> = open_bim_numbering::number(&project)
        .unwrap()
        .parts
        .into_values()
        .collect();
    let mut editor = Editor::new(project).unwrap();
    let report = editor.export_nc(&dir.join("model.obim")).unwrap();
    let folder = dir.join("model.nc");
    let files: Vec<_> = std::fs::read_dir(&folder)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(files.len(), marks.len(), "{report}");
    for file in &files {
        assert_eq!(file.extension().unwrap(), "nc1");
        let text = std::fs::read_to_string(file).unwrap();
        assert!(text.starts_with("ST\n") && text.ends_with("EN\n"));
    }
    assert!(report.contains(&format!("{}", marks.len())), "{report}");
    // A second export replaces the old files instead of mixing them.
    std::fs::write(folder.join("old.nc1"), "ST\nEN\n").unwrap();
    editor.export_nc(&dir.join("model.obim")).unwrap();
    assert!(!folder.join("old.nc1").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn palette_command_requests_the_export() {
    let mut b = Workbench::new(Editor::new(source()).unwrap());
    assert!(matches!(
        b.execute(Cmd::ExportNc),
        Some(Request::Action(Action::ExportNc))
    ));
}
