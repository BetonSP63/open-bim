//! Editing drawings: dimensions picked on view points stay associative,
//! lines, hatching and masking are added, moved, deleted and undone, stored
//! with the model and exported; the tools run from commands and keys.
use open_bim_desktop::drawing_editor::tools::DrawTool;
use open_bim_desktop::drawing_editor::{DrawingEditor, ExportFormat};
use open_bim_desktop::tools::Editor;
use open_bim_drawings::sample::control_project;
use open_bim_model::{AnchorPoint, HatchPattern, ObjectId, SheetObjectKind};

fn setup() -> (Editor, DrawingEditor, ObjectId, ObjectId) {
    let f = control_project();
    let mut editor = Editor::new(f.project.clone()).unwrap();
    let mut drawings = DrawingEditor::new();
    editor.selected = [f.beam].into();
    drawings.create_part(&mut editor).unwrap();
    let id = drawings.current.unwrap();
    (editor, drawings, id, f.beam)
}

fn dimension_text(drawings: &mut DrawingEditor, editor: &Editor, dim: u32) -> Vec<String> {
    let sheet = drawings.sheet(editor.document.project()).unwrap();
    sheet
        .dimensions
        .iter()
        .find(|d| d.id == dim)
        .unwrap()
        .labels
        .iter()
        .map(|l| l.text.clone())
        .collect()
}

#[test]
fn dimension_by_clicks_follows_the_model() {
    let (mut editor, mut drawings, id, beam) = setup();
    let project = editor.document.project().clone();
    let candidates =
        open_bim_drawings::anchor_candidates(&project, &project.drawings[&id]).unwrap();
    let at = |point: AnchorPoint| {
        candidates
            .iter()
            .find(|(v, a, _)| *v == 1 && a.object == beam && a.point == point)
            .unwrap()
            .2
    };
    let (start, end) = (at(AnchorPoint::Start), at(AnchorPoint::End));
    drawings.start_tool(DrawTool::Dimension);
    // A click 1 mm off the point still picks it; empty paper is refused.
    drawings
        .tool_click(&mut editor, [start[0] + 1., start[1]])
        .unwrap();
    assert!(
        drawings
            .tool_click(&mut editor, [start[0] + 40., start[1] + 40.])
            .is_err()
    );
    drawings.tool_click(&mut editor, end).unwrap();
    drawings.tool_finish(&mut editor).unwrap();
    let dim = drawings.selected_dimension.unwrap();
    let def = editor.document.project().drawings[&id]
        .dimensions
        .iter()
        .find(|d| d.id == dim)
        .unwrap()
        .clone();
    assert_eq!(
        def.anchors.iter().map(|a| a.point).collect::<Vec<_>>(),
        [AnchorPoint::Start, AnchorPoint::End]
    );
    assert!(dimension_text(&mut drawings, &editor, dim).contains(&"3000".to_string()));
    // Lengthen the beam by 500 at its end; after updating the drawing the
    // dimension shows the new length.
    let shape = editor.document.project().elements[&beam].shape.clone();
    let longer = open_bim_desktop::interaction::member_edit::resize(
        &shape,
        3500.,
        open_bim_desktop::interaction::member_edit::LengthMode::End,
    )
    .unwrap();
    let mut element = editor.document.project().elements[&beam].clone();
    element.shape = longer;
    editor
        .commit(
            "Длина",
            &[open_bim_commands::Command::ReplaceElement { element }],
        )
        .unwrap();
    drawings.update(&mut editor).unwrap();
    assert!(dimension_text(&mut drawings, &editor, dim).contains(&"3500".to_string()));
}

#[test]
fn line_hatch_and_mask_are_added_moved_deleted_and_saved() {
    let (mut editor, mut drawings, id, _) = setup();
    let objects = |e: &Editor| e.document.project().drawings[&id].objects.clone();
    let depth = |e: &Editor| e.document.history_depth().0;
    // Line by two clicks.
    drawings.start_tool(DrawTool::Line);
    drawings.tool_click(&mut editor, [40., 40.]).unwrap();
    drawings.tool_click(&mut editor, [120., 40.]).unwrap();
    // Hatch: four corners and Enter.
    drawings.style.pattern = HatchPattern::Metal;
    drawings.style.angle = 45.;
    drawings.style.spacing = 3.;
    drawings.start_tool(DrawTool::Hatch);
    for p in [[60., 60.], [100., 60.], [100., 90.], [60., 90.]] {
        drawings.tool_click(&mut editor, p).unwrap();
    }
    drawings.tool_finish(&mut editor).unwrap();
    // Mask over part of the views.
    drawings.start_tool(DrawTool::Wipeout);
    for p in [[150., 150.], [200., 150.], [200., 190.], [150., 190.]] {
        drawings.tool_click(&mut editor, p).unwrap();
    }
    drawings.tool_finish(&mut editor).unwrap();
    let all = objects(&editor);
    assert_eq!(all.len(), 3);
    assert!(matches!(all[0].kind, SheetObjectKind::Line { .. }));
    assert!(
        matches!(all[1].kind, SheetObjectKind::Hatch { spacing, angle, .. } if spacing == 3. && angle == 45.)
    );
    assert!(matches!(all[2].kind, SheetObjectKind::Wipeout { .. }));
    // The sheet has 45° hatch lines inside the area and the mask polygon.
    let sheet = drawings.sheet(editor.document.project()).unwrap().clone();
    let hatch = sheet.objects.iter().find(|o| o.id == all[1].id).unwrap();
    assert!(hatch.lines.len() > 5);
    for l in &hatch.lines {
        let (a, b) = (l.points[0], l.points[1]);
        assert!(((b[1] - a[1]).atan2(b[0] - a[0]).to_degrees() - 45.).abs() < 1e-9);
    }
    assert!(sheet.objects.iter().any(|o| o.mask.is_some()));
    // Select by click on the line, move it with the arrows, delete, undo.
    drawings.cancel_tool();
    assert!(drawings.pick_at(&sheet, [80., 40.5]));
    assert_eq!(drawings.selected_object, Some(all[0].id));
    let before = depth(&editor);
    drawings.nudge_selected(&mut editor, [10., 0.]).unwrap();
    assert_eq!(depth(&editor), before + 1);
    let SheetObjectKind::Line { points } = &objects(&editor)[0].kind else {
        unreachable!()
    };
    assert_eq!(points[0], [50., 40.]);
    drawings.delete_selected(&mut editor).unwrap();
    assert_eq!(objects(&editor).len(), 2);
    editor.history(false).unwrap();
    assert_eq!(objects(&editor).len(), 3);
    // Saved with the project and read back unchanged.
    let bytes = open_bim_persistence::encode(editor.document.project()).unwrap();
    let back = open_bim_persistence::decode(&bytes).unwrap();
    assert_eq!(back.drawings[&id].objects, objects(&editor));
    // Export for the independent check.
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/ui/drawing-editing");
    std::fs::create_dir_all(&dir).unwrap();
    let project = editor.document.project().clone();
    for format in [ExportFormat::Dxf, ExportFormat::Pdf, ExportFormat::Svg] {
        drawings.export(&project, format, &dir).unwrap();
    }
}

#[test]
fn drawing_tools_run_from_commands_and_keys() {
    use open_bim_desktop::ui::{Workbench, commands::Cmd};
    let f = control_project();
    let mut b = Workbench::new(Editor::new(f.project.clone()).unwrap());
    b.editor.selected = [f.beam].into();
    b.drawings.create_part(&mut b.editor).unwrap();
    let id = b.drawings.current.unwrap();
    b.execute(Cmd::DrawPolyline);
    assert_eq!(b.drawings.tool.tool, Some(DrawTool::Polyline));
    for p in [[40., 40.], [80., 60.], [120., 40.]] {
        b.drawings.tool_click(&mut b.editor, p).unwrap();
    }
    // Enter finishes the polyline, Esc leaves the tool, Delete removes it.
    b.execute(Cmd::FinishPicks);
    let objects = |b: &Workbench| b.editor.document.project().drawings[&id].objects.clone();
    assert!(matches!(&objects(&b)[0].kind, SheetObjectKind::Line { points } if points.len() == 3));
    b.execute(Cmd::CancelTool);
    assert!(b.drawings.tool.tool.is_none());
    assert!(b.drawings.selected_object.is_some());
    b.execute(Cmd::DeleteSelected);
    assert!(objects(&b).is_empty());
    // Every tool is a quick-launch command.
    for (cmd, tool) in [
        (Cmd::DrawDimension, DrawTool::Dimension),
        (Cmd::DrawLine, DrawTool::Line),
        (Cmd::DrawRectangle, DrawTool::Rectangle),
        (Cmd::DrawCircle, DrawTool::Circle),
        (Cmd::DrawHatch, DrawTool::Hatch),
        (Cmd::DrawWipeout, DrawTool::Wipeout),
        (Cmd::DrawText, DrawTool::Text),
        (Cmd::DrawNode, DrawTool::Node),
    ] {
        b.execute(cmd);
        assert_eq!(b.drawings.tool.tool, Some(tool));
        let label = open_bim_desktop::ui::commands::spec(cmd).label;
        assert!(
            !open_bim_desktop::ui::commands::search(label).is_empty(),
            "{label}"
        );
    }
}

#[test]
fn node_tool_adds_a_node_view_marked_on_its_view() {
    let (mut editor, mut drawings, id, _) = setup();
    let sheet = drawings.sheet(editor.document.project()).unwrap().clone();
    let main = &sheet.views[0];
    let c = [
        (main.bounds[0][0] + main.bounds[1][0]) / 2.,
        (main.bounds[0][1] + main.bounds[1][1]) / 2.,
    ];
    drawings.start_tool(DrawTool::Node);
    drawings.tool_click(&mut editor, c).unwrap();
    drawings.tool_click(&mut editor, [c[0] + 8., c[1]]).unwrap();
    let d = &editor.document.project().drawings[&id];
    let node = d.views.iter().find(|v| v.node.is_some()).expect("вид узла");
    assert!(open_bim_model::standard_scale(node.scale.unwrap()));
    let sheet = drawings.sheet(editor.document.project()).unwrap();
    let view = sheet.views.iter().find(|v| v.id == node.id).unwrap();
    assert!(view.labels[0].text.starts_with("Узел 1 1:"));
    // Undo removes the node view.
    editor.history(false).unwrap();
    assert!(
        editor.document.project().drawings[&id]
            .views
            .iter()
            .all(|v| v.node.is_none())
    );
    // A click outside every view is refused.
    drawings.start_tool(DrawTool::Node);
    drawings.tool_click(&mut editor, [30., 30.]).unwrap();
    assert!(drawings.tool_click(&mut editor, [35., 30.]).is_err());
}
