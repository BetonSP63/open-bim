use open_bim_commands::project_hash;
use open_bim_desktop::tools::{
    Editor, ToolKind,
    detailing_ui::{DetailTool, DetailingUi},
};
use open_bim_model::{ObjectId, Project};

#[test]
fn detailing_ui_commands_render_persist_and_undo() {
    for kind in [
        DetailTool::Hole,
        DetailTool::Plane,
        DetailTool::Box,
        DetailTool::Bolts,
        DetailTool::Weld,
    ] {
        let mut editor = Editor::new(Project::default()).unwrap();
        for z in [0, 20] {
            let _ = editor.begin(ToolKind::Plate);
            editor.draft.start = ["0".into(), "0".into(), z.to_string()];
            editor.create().unwrap();
        }
        editor.selected = if matches!(kind, DetailTool::Bolts | DetailTool::Weld) {
            [ObjectId(8), ObjectId(9)].into()
        } else {
            [ObjectId(8)].into()
        };
        let mut ui = DetailingUi {
            kind,
            ..Default::default()
        };
        if kind == DetailTool::Plane {
            ui.axis = 0;
        }
        let hash = project_hash(editor.document.project()).unwrap();
        let commands = ui
            .commands(editor.document.project(), &editor.selected)
            .unwrap();
        let scene = editor.commit("GUI detailing", &commands).unwrap();
        assert_eq!(scene.instances.len(), 2);
        assert!(!editor.document.project().details.is_empty());
        let bytes = open_bim_persistence::encode(editor.document.project()).unwrap();
        assert_eq!(
            open_bim_persistence::decode(&bytes).unwrap(),
            *editor.document.project()
        );
        editor.history(false).unwrap();
        assert_eq!(project_hash(editor.document.project()).unwrap(), hash);
        ui.diameter = "NaN".into();
        ui.kind = DetailTool::Hole;
        assert!(
            ui.commands(editor.document.project(), &editor.selected)
                .is_err()
        );
        assert_eq!(project_hash(editor.document.project()).unwrap(), hash);
    }
}
