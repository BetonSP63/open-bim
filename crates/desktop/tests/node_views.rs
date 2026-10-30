use open_bim_desktop::{
    tools::Editor,
    ui::{Request, Workbench, commands::Cmd, views::ViewKind},
};
use open_bim_drawings::sample::control_project;
use open_bim_model::Project;

#[test]
fn node_view_has_local_visibility_and_survives_save_history_and_reload() {
    let f = control_project();
    let mut bench = Workbench::new(Editor::new(f.project).unwrap());
    assert!(bench.create_node_view().is_err());
    bench.editor.selected.insert(f.beam);
    let request = bench.create_node_view().unwrap();
    assert!(matches!(request, Request::ApplyView(_)));
    let node = bench.active_view;
    bench.views[node].edge_pen = Some(open_bim_model::LinePen {
        line_type: open_bim_model::LineType::DashDot,
        width: 1.5,
    });
    bench.store_model_views("Линии узла").unwrap();
    assert!(matches!(bench.views[node].kind, ViewKind::Node { .. }));
    assert!(!bench.hidden.contains(&f.beam));
    let baseline_hidden = bench.hidden.clone();
    bench.execute(Cmd::HideSelected);
    assert!(bench.hidden.contains(&f.beam));
    bench.apply_view(0).unwrap();
    assert!(!bench.hidden.contains(&f.beam));
    bench.apply_view(node).unwrap();
    assert!(bench.hidden.contains(&f.beam));
    let json = serde_json::to_string(bench.editor.document.project()).unwrap();
    let restored = Project::from_json(&json).unwrap();
    let other = Workbench::new(Editor::new(restored).unwrap());
    assert_eq!(other.views[node].hidden, bench.hidden);
    assert_eq!(
        other.views[node].definition(),
        bench.views[node].definition()
    );
    bench.editor.history(false).unwrap();
    bench.reload_model_views();
    assert_eq!(bench.hidden, baseline_hidden);
    bench.editor.history(false).unwrap();
    bench.reload_model_views();
    assert!(bench.views[node].edge_pen.is_none());
    bench.editor.history(false).unwrap();
    bench.reload_model_views();
    assert_eq!(bench.views.len(), 1);
    bench.editor.history(true).unwrap();
    bench.reload_model_views();
    assert_eq!(bench.views.len(), 2);
}

#[test]
fn node_drawing_uses_only_parts_visible_in_that_view() {
    let f = control_project();
    let mut bench = Workbench::new(Editor::new(f.project).unwrap());
    bench.editor.selected.insert(f.beam);
    bench.create_node_view().unwrap();
    bench.hidden.insert(f.plate);
    bench.create_node_drawing().unwrap();
    let project = bench.editor.document.project();
    let d = &project.drawings[&bench.drawings.current.unwrap()];
    let parts = open_bim_drawings::drawing_parts(project, &d.kind);
    assert!(parts.contains(&f.beam));
    assert!(!parts.contains(&f.plate));
    assert!(d.title.starts_with("Узел"));
    assert!(open_bim_drawings::generate(project, d).is_ok());
    bench.apply_view(0).unwrap();
    assert!(!bench.hidden.contains(&f.plate));
}
