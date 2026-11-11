//! The modeling session: select by default, one next step, Esc back.
use open_bim_desktop::interaction::tools::default_project;
use open_bim_desktop::interaction::workflow::STEEL_GROUP_ORDER;
use open_bim_desktop::tools::Editor;
use open_bim_desktop::ui::Workbench;
use open_bim_desktop::ui::chrome::{self, SidePanel};
use open_bim_desktop::ui::commands::Cmd;

#[test]
fn steel_ribbon_follows_the_frame() {
    let names: Vec<_> = chrome::groups(chrome::RibbonTab::Steel)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(names, STEEL_GROUP_ORDER);
    // Сетка остаётся и среди построений: привычное место не пропадает.
    let edit: Vec<_> = chrome::groups(chrome::RibbonTab::Edit)
        .into_iter()
        .flat_map(|(_, items)| items)
        .filter_map(|item| item.cmd)
        .collect();
    assert!(edit.contains(&Cmd::GridDialog));
    assert!(edit.contains(&Cmd::ToolAxis));
}

#[test]
fn column_starts_as_a_tool_on_the_storey_and_escape_returns_to_select() {
    let project = default_project().unwrap();
    let mut bench = Workbench::new(Editor::new(project).unwrap());
    assert!(!bench.in_command());
    let idle = bench.prompt();
    assert!(
        idle.contains("Шаг 2"),
        "новая модель уже с сеткой, следующий шаг — колонна: {idle}"
    );

    bench.execute(Cmd::ToolColumn);
    assert!(bench.in_command());
    assert_eq!(bench.side_panel, Some(SidePanel::Properties));
    let step = bench.prompt();
    assert!(
        step.contains("положени") || step.contains("колонн"),
        "{step}"
    );
    assert!(step.contains("Esc — вернуться к выбору"), "{step}");
    assert!(bench.editor.draft.bottom_level.is_some());
    assert!(bench.editor.draft.top_level.is_some());
    assert_eq!(bench.editor.draft.bottom, "0");
    assert_eq!(bench.editor.draft.top, "0");

    bench.execute(Cmd::CancelTool);
    assert!(!bench.in_command());
    assert!(bench.engine.tool.is_none());
    assert!(
        bench.prompt().contains("Шаг 2"),
        "после Esc снова подсказка выбора: {}",
        bench.prompt()
    );
}

#[test]
fn escape_closes_a_creation_dialog_and_a_beam_opens_its_properties() {
    let project = default_project().unwrap();
    let mut bench = Workbench::new(Editor::new(project).unwrap());
    bench.execute(Cmd::ToolCurvedBeam);
    assert!(bench.swept_dialog.is_some());
    assert!(
        bench.prompt().contains("Гнутая деталь"),
        "{}",
        bench.prompt()
    );
    assert!(bench.prompt().contains("Esc"));
    bench.execute(Cmd::CancelTool);
    assert!(bench.swept_dialog.is_none());
    assert!(!bench.in_command());

    bench.side_panel = None;
    bench.execute(Cmd::ToolBeam);
    assert_eq!(bench.side_panel, Some(SidePanel::Properties));
    assert!(
        bench.editor.draft.bottom_level.is_none(),
        "балке уровни колонны не нужны"
    );
    assert!(bench.prompt().contains("начальную точку"));
}
