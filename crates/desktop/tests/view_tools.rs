//! View tab: «Свойства вида» depth, «Окрасить» / «Перечертить», zoom,
//! «Центр вращения», «Облёт» and the screenshot file name.
use open_bim_desktop::tools::Editor;
use open_bim_desktop::ui::commands::Cmd;
use open_bim_desktop::ui::views::{ViewKind, grid_views};
use open_bim_desktop::ui::{Request, Workbench, view_tools};
use open_bim_renderer::DVec3;

fn bench() -> Workbench {
    let project = open_bim_persistence::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/steel_release/source.obim"
    ))
    .unwrap();
    Workbench::new(Editor::new(project).unwrap())
}

#[test]
fn view_depth_reads_and_sets_for_plans_and_elevations() {
    let project =
        open_bim_desktop::interaction::tools::with_default_grid(open_bim_model::Project::default())
            .unwrap();
    let views = grid_views(&project);
    assert!(views[0].depth().is_none(), "3D has no depth");
    let mut plan = views
        .iter()
        .find(|v| matches!(v.kind, ViewKind::Plan { .. }))
        .unwrap()
        .clone();
    assert_eq!(plan.depth(), Some([1000., 1000.]));
    plan.set_depth(200., 3000.).unwrap();
    assert_eq!(plan.depth(), Some([200., 3000.]));
    let ViewKind::Plan { elevation } = plan.kind else {
        unreachable!()
    };
    assert!(plan.keeps(DVec3::new(0., 0., elevation + 2900.)));
    assert!(!plan.keeps(DVec3::new(0., 0., elevation - 300.)));
    assert!(plan.set_depth(-1., 10.).is_err());
    let mut elevation = views
        .iter()
        .find(|v| matches!(v.kind, ViewKind::Elevation { .. }))
        .unwrap()
        .clone();
    assert_eq!(elevation.depth(), Some([500., 500.]));
    elevation.set_depth(100., 900.).unwrap();
    let d = elevation.depth().unwrap();
    assert!((d[0] - 100.).abs() < 1e-6 && (d[1] - 900.).abs() < 1e-6);
    assert!(views[0].clone().set_depth(1., 1.).is_err());
}

#[test]
fn paint_colours_selected_parts_until_redraw() {
    let mut b = bench();
    let id = *b.editor.document.project().elements.keys().next().unwrap();
    // Nothing selected: nothing painted, the window opens.
    b.execute(Cmd::Paint);
    assert!(b.view_tools.paint_open);
    assert!(b.view_tools.painted.is_empty());
    b.view_tools.paint_colour = [255, 0, 0];
    b.editor.selected = [id].into();
    b.execute(Cmd::Paint);
    let shown = b.applied_representation(1);
    assert_eq!(shown.appearance[&id].color, Some([1., 0., 0.]));
    b.execute(Cmd::Redraw);
    let shown = b.applied_representation(1);
    assert!(
        shown
            .appearance
            .get(&id)
            .is_none_or(|a| a.color != Some([1., 0., 0.]))
    );
    assert_eq!(b.tools.message.as_deref(), Some("Вид перечерчен"));
}

#[test]
fn navigation_commands_ask_the_viewport() {
    let mut b = bench();
    assert!(matches!(b.execute(Cmd::ZoomIn), Some(Request::Zoom(d)) if d > 0.));
    assert!(matches!(b.execute(Cmd::ZoomOut), Some(Request::Zoom(d)) if d < 0.));
    assert!(matches!(
        b.execute(Cmd::PreviousView),
        Some(Request::PreviousView)
    ));
    assert!(matches!(
        b.execute(Cmd::Screenshot),
        Some(Request::Screenshot)
    ));
    // Centre of rotation: the next pick, then the command ends.
    b.execute(Cmd::RotationCentre);
    assert!(b.picking());
    let at = DVec3::new(1000., 2000., 0.);
    assert!(matches!(b.pick(at), Some(Request::CenterAt(p)) if p == at));
    assert!(!b.picking());
    // Esc cancels it; it also ends «Облёт».
    b.execute(Cmd::RotationCentre);
    b.execute(Cmd::CancelTool);
    assert!(!b.view_tools.centre_pick);
    b.execute(Cmd::Fly);
    assert!(b.view_tools.fly);
    b.execute(Cmd::CancelTool);
    assert!(!b.view_tools.fly);
}

#[test]
fn screenshots_get_free_numbered_names_next_to_the_project() {
    let dir = std::env::temp_dir().join(format!("obim-shots-{}", std::process::id()));
    let project = dir.join("модель.obim");
    let first = view_tools::screenshot_path(project.to_str().unwrap());
    assert_eq!(first, dir.join("Снимки").join("снимок-1.png"));
    std::fs::create_dir_all(first.parent().unwrap()).unwrap();
    std::fs::write(&first, b"png").unwrap();
    assert_eq!(
        view_tools::screenshot_path(project.to_str().unwrap()),
        dir.join("Снимки").join("снимок-2.png")
    );
    std::fs::remove_dir_all(&dir).unwrap();
}
