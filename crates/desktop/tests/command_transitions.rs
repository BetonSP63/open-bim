use open_bim_desktop::tools::{Editor, ui::Action};
use open_bim_desktop::ui::{Request, Workbench, commands::Cmd};
use open_bim_renderer::DVec3;

fn bench() -> Workbench {
    Workbench::new(
        Editor::new(open_bim_desktop::interaction::tools::default_project().unwrap()).unwrap(),
    )
}

#[test]
fn starting_a_point_command_replaces_the_previous_point_owner() {
    let commands = [
        Cmd::ToolBeam,
        Cmd::ConstructionLine,
        Cmd::Measure,
        Cmd::MeasureAngle,
        Cmd::WorkPlaneThreePoints,
        Cmd::ClipPlanePick,
        Cmd::RotationCentre,
    ];
    for old in commands {
        for next in commands {
            let mut b = bench();
            let before = b.editor.document.project().clone();
            b.execute(old);
            if !matches!(old, Cmd::ClipPlanePick | Cmd::RotationCentre) {
                assert!(b.pick(DVec3::new(100., 200., 300.)).is_none());
            }
            b.engine.numeric = Some("!900".into());
            b.engine.reference = Some(DVec3::splat(100.));
            b.engine.error = Some("old input error".into());
            b.kb_cursor = Some(DVec3::splat(500.));
            b.execute(next);
            let owners = [
                b.engine.active(),
                b.trim.tool.is_some(),
                b.measure.is_some(),
                b.angle_picks.is_some(),
                b.workplane_picks.is_some(),
                b.clip_pick,
                b.view_tools.centre_pick,
            ];
            assert_eq!(
                owners.into_iter().filter(|v| *v).count(),
                1,
                "{old:?} -> {next:?}: multiple commands still own picks"
            );
            assert!(b.engine.numeric.is_none(), "{old:?} -> {next:?}");
            assert!(b.engine.reference.is_none(), "{old:?} -> {next:?}");
            assert!(b.engine.error.is_none(), "{old:?} -> {next:?}");
            assert!(b.kb_cursor.is_none(), "{old:?} -> {next:?}");
            assert_eq!(*b.editor.document.project(), before);
            let point = DVec3::new(700., 800., 900.);
            let result = b.pick(point);
            match next {
                Cmd::ToolBeam => assert_eq!(b.engine.picks, vec![point]),
                Cmd::ConstructionLine => assert_eq!(b.trim.points, vec![point]),
                Cmd::Measure => assert_eq!(b.measure, Some(vec![point])),
                Cmd::MeasureAngle => assert_eq!(b.angle_picks, Some(vec![point])),
                Cmd::WorkPlaneThreePoints => assert_eq!(b.workplane_picks, Some(vec![point])),
                Cmd::ClipPlanePick => {
                    assert!(matches!(result, Some(Request::ClipAt(p)) if p == point))
                }
                Cmd::RotationCentre => {
                    assert!(matches!(result, Some(Request::CenterAt(p)) if p == point))
                }
                _ => unreachable!(),
            }
        }
    }
}

#[test]
fn a_beam_after_interrupted_measurement_commits_as_one_undoable_operation() {
    let mut b = bench();
    let before = b.editor.document.project().clone();
    b.execute(Cmd::Measure);
    b.pick(DVec3::splat(100.));
    b.execute(Cmd::ToolBeam);
    b.pick(DVec3::ZERO);
    let Some(Request::Action(Action::Commit(label, commands))) = b.pick(DVec3::X * 6000.) else {
        panic!("beam points were intercepted by the old measurement");
    };
    b.editor.commit(&label, &commands).unwrap();
    assert_eq!(
        b.editor.document.project().elements.len(),
        before.elements.len() + 1
    );
    b.editor.document.undo().unwrap();
    assert_eq!(*b.editor.document.project(), before);
}

#[test]
fn switching_commands_keeps_selection_tool_properties_and_workplane() {
    let project = open_bim_model::example_steel_frame().unwrap();
    let selected = std::collections::BTreeSet::from([*project.elements.keys().next().unwrap()]);
    let mut b = Workbench::new(Editor::new(project).unwrap());
    b.editor.selected = selected.clone();
    b.set_workplane(open_bim_desktop::interaction::Workplane::at_elevation(
        3600.125,
    ));
    let plane = b.engine.workplane;
    let before = b.editor.document.project().clone();
    b.execute(Cmd::ToolBeam);
    b.editor.draft.name = "Балка с моими параметрами".into();
    b.execute(Cmd::Measure);
    b.execute(Cmd::WorkPlaneThreePoints);
    b.pick(DVec3::splat(500.));
    b.execute(Cmd::ToolBeam);
    assert_eq!(b.editor.selected, selected);
    assert_eq!(b.editor.draft.name, "Балка с моими параметрами");
    assert_eq!(b.engine.workplane, plane);
    assert_eq!(*b.editor.document.project(), before);
}

#[test]
fn copy_and_paste_replace_an_interrupted_measurement() {
    for command in [Cmd::CopyByPicks, Cmd::Paste] {
        let project = open_bim_model::example_steel_frame().unwrap();
        let id = *project.elements.keys().next().unwrap();
        let mut b = Workbench::new(Editor::new(project).unwrap());
        b.editor.selected.insert(id);
        b.execute(Cmd::CopyToClipboard);
        b.execute(Cmd::Measure);
        b.pick(DVec3::ZERO);
        b.execute(command);
        assert!(b.measure.is_none(), "{command:?} still has a measurement");
        let result = b.pick(DVec3::X * 1000.);
        if command == Cmd::Paste {
            assert!(matches!(
                result,
                Some(Request::Action(Action::Commit(_, _)))
            ));
        } else {
            assert!(result.is_none());
            assert!(matches!(
                b.pick(DVec3::X * 2000.),
                Some(Request::Action(Action::Edit(_)))
            ));
        }
    }
}

#[test]
fn remaining_point_command_entries_replace_measurement() {
    for command in [
        Cmd::AddedMaterial,
        Cmd::PourBreak,
        Cmd::AnalysisNode,
        Cmd::RebarSingle,
        Cmd::FormworkPanels,
        Cmd::ToolCurvedBeam,
    ] {
        let mut b = bench();
        b.execute(Cmd::Measure);
        b.pick(DVec3::ZERO);
        b.execute(command);
        assert!(b.measure.is_none(), "{command:?} still has a measurement");
    }
    let mut b = bench();
    b.execute(Cmd::Measure);
    b.start_load();
    assert!(b.measure.is_none());

    b.execute(Cmd::Measure);
    b.arm_component(open_bim_desktop::ui::components::catalog().remove(0));
    assert!(b.measure.is_none());
    assert!(b.components.armed.is_some());
}
