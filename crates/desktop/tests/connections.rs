use open_bim_commands::project_hash;
use open_bim_desktop::tools::{Edit, Editor, connection_ui::ConnectionUi};
use open_bim_model::{ObjectId, Point3, example_steel_frame};

#[test]
fn connection_panel_commands_render_persist_and_follow_undo_redo() {
    let mut editor = Editor::new(example_steel_frame().unwrap()).unwrap();
    let mut ui = ConnectionUi {
        primary: Some(ObjectId(8)),
        ..Default::default()
    };
    let id = ObjectId(editor.document.project().next_id);
    let scene = editor
        .commit(
            "Connection UI",
            &ui.commands(editor.document.project()).unwrap(),
        )
        .unwrap();
    assert_eq!(scene.instances.len(), 8);
    ui.load(editor.document.project(), id).unwrap();
    let children = editor.document.project().connections[&id].children.clone();
    ui.template.thickness = 30.;
    editor
        .commit(
            "UI update",
            &ui.commands(editor.document.project()).unwrap(),
        )
        .unwrap();
    ui.sync(editor.document.project());
    assert_eq!(
        editor.document.project().connections[&id].children,
        children
    );
    editor.history(false).unwrap();
    ui.sync(editor.document.project());
    assert_eq!(ui.template.thickness, 20.);
    editor.history(true).unwrap();
    ui.sync(editor.document.project());
    assert_eq!(ui.template.thickness, 30.);
    editor.selected = [ObjectId(8)].into();
    editor.edit(&Edit::Move(Point3([100., 0., 0.]))).unwrap();
    assert_eq!(
        editor.document.project().connections[&id].children,
        children
    );
    let plate = children["plate"];
    assert_eq!(
        editor.document.project().elements[&plate]
            .shape
            .frame()
            .origin
            .0[0],
        -45.
    );
    let path =
        std::env::temp_dir().join(format!("open-bim-template-ui-{}.json", std::process::id()));
    ui.path = path.to_string_lossy().into_owned();
    ui.export_template().unwrap();
    let expected = ui.template.clone();
    ui.template.thickness = 40.;
    ui.import_template().unwrap();
    assert_eq!(ui.template, expected);
    std::fs::write(&path, b"{\"version\":999}").unwrap();
    assert!(ui.import_template().is_err());
    assert_eq!(ui.template, expected);
    std::fs::remove_file(path).unwrap();
    let bytes = open_bim_persistence::encode(editor.document.project()).unwrap();
    assert_eq!(
        open_bim_persistence::decode(&bytes).unwrap(),
        *editor.document.project()
    );
    let hash = project_hash(editor.document.project()).unwrap();
    ui.template.bolt_diameter = f64::NAN;
    assert!(ui.commands(editor.document.project()).is_err());
    assert_eq!(project_hash(editor.document.project()).unwrap(), hash);
}
