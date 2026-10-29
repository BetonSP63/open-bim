//! New views of an open part drawing: projection views and sections placed
//! by clicks, their properties, alignment, flipping and deletion.
use open_bim_desktop::drawing_editor::DrawingEditor;
use open_bim_desktop::drawing_editor::tools::DrawTool;
use open_bim_desktop::drawing_editor::views::ViewEdit;
use open_bim_desktop::tools::Editor;
use open_bim_drawings::LineStyle;
use open_bim_drawings::sample::control_project;
use open_bim_model::{ObjectId, ViewDirection};

fn setup() -> (Editor, DrawingEditor, ObjectId) {
    let f = control_project();
    let mut editor = Editor::new(f.project.clone()).unwrap();
    let mut drawings = DrawingEditor::new();
    editor.selected = [f.beam].into();
    drawings.create_part(&mut editor).unwrap();
    let id = drawings.current.unwrap();
    (editor, drawings, id)
}

fn main_bounds(drawings: &mut DrawingEditor, editor: &Editor) -> [[f64; 2]; 2] {
    drawings.sheet(editor.document.project()).unwrap().views[0].bounds
}

#[test]
fn view_tool_adds_a_projection_view_in_the_chosen_direction() {
    let (mut editor, mut drawings, id) = setup();
    let before = editor.document.project().drawings[&id].views.len();
    drawings.style.view_direction = ViewDirection::Bottom;
    drawings.start_tool(DrawTool::View);
    drawings.tool_click(&mut editor, [200., 120.]).unwrap();
    let d = editor.document.project().drawings[&id].clone();
    assert_eq!(d.views.len(), before + 1);
    let view = d.views.last().unwrap();
    assert_eq!(view.direction, ViewDirection::Bottom);
    assert_eq!(view.name, "Вид снизу");
    assert_eq!(view.position, [200., 120.]);
    assert_eq!(drawings.selected_view, Some(view.id));
    // The new view is drawn with the part outline.
    let sheet = drawings.sheet(editor.document.project()).unwrap();
    let g = sheet.views.iter().find(|g| g.id == view.id).unwrap();
    assert!(g.lines.iter().any(|l| l.style == LineStyle::Visible));
    // Align puts a view from below under the main view.
    let main_x = d.views[0].position[0];
    drawings.align_view(&mut editor, view.id).unwrap();
    let d = editor.document.project().drawings[&id].clone();
    let aligned = d.views.iter().find(|v| v.id == view.id).unwrap();
    assert_eq!(aligned.position, [main_x, 120.]);
    assert!(drawings.align_view(&mut editor, d.views[0].id).is_err());
}

#[test]
fn section_by_a_line_on_the_front_view_cuts_the_beam_across() {
    let (mut editor, mut drawings, id) = setup();
    let [lo, hi] = main_bounds(&mut drawings, &editor);
    // A vertical line across the middle of the beam, the view to its right.
    let x = (lo[0] + hi[0]) / 2.;
    drawings.start_tool(DrawTool::Section);
    drawings.tool_click(&mut editor, [x, hi[1] + 2.]).unwrap();
    drawings.tool_click(&mut editor, [x, lo[1] - 2.]).unwrap();
    drawings.tool_click(&mut editor, [x + 60., 100.]).unwrap();
    let d = editor.document.project().drawings[&id].clone();
    let section = d
        .views
        .iter()
        .find(|v| v.section.is_some())
        .unwrap()
        .clone();
    // View to the right ⇒ looked at from the left (first-angle projection).
    assert_eq!(section.direction, ViewDirection::Left);
    let depth = section.section.unwrap();
    assert!((depth + 1500.).abs() < 0.5 * d.scale + 1., "{depth}");
    let sheet = drawings.sheet(editor.document.project()).unwrap().clone();
    let g = sheet.views.iter().find(|g| g.id == section.id).unwrap();
    assert!(g.lines.iter().any(|l| l.style == LineStyle::Section));
    assert!(g.lines.iter().any(|l| l.style == LineStyle::Hatch));
    assert!(g.labels[0].text.starts_with("1-1"), "{}", g.labels[0].text);
    // The trace with the number is drawn on the front view.
    assert!(sheet.views[0].labels.iter().any(|l| l.text == "1"));
    // Flip: the other side of the same plane.
    drawings.flip_section(&mut editor, section.id).unwrap();
    let flipped = editor.document.project().drawings[&id]
        .views
        .iter()
        .find(|v| v.id == section.id)
        .unwrap()
        .clone();
    assert_eq!(flipped.direction, ViewDirection::Right);
    assert!((flipped.section.unwrap() + depth).abs() < 1e-9);
    // Undo returns the previous side, undo again removes the section.
    editor.history(false).unwrap();
    editor.history(false).unwrap();
    assert!(
        editor.document.project().drawings[&id]
            .views
            .iter()
            .all(|v| v.section.is_none())
    );
}

#[test]
fn section_line_must_start_on_a_view() {
    let (mut editor, mut drawings, _) = setup();
    drawings.start_tool(DrawTool::Section);
    drawings.tool_click(&mut editor, [380., 15.]).unwrap();
    drawings.tool_click(&mut editor, [380., 25.]).unwrap();
    assert!(drawings.tool_click(&mut editor, [300., 270.]).is_err());
    // The tool starts over from the first point.
    assert!(drawings.tool.points.is_empty());
}

#[test]
fn view_properties_are_edited_and_views_deleted_with_their_dimensions() {
    let (mut editor, mut drawings, id) = setup();
    let d = editor.document.project().drawings[&id].clone();
    let main = d.views[0].id;
    let top = d.views[1].id;
    drawings
        .edit_view(
            &mut editor,
            top,
            ViewEdit {
                name: Some("План детали".into()),
                scale: Some(Some(5.)),
                ..Default::default()
            },
        )
        .unwrap();
    let view = editor.document.project().drawings[&id].views[1].clone();
    assert_eq!(view.name, "План детали");
    assert_eq!(view.scale, Some(5.));
    // A non-standard scale and an empty name are refused.
    let bad = ViewEdit {
        scale: Some(Some(3.)),
        ..Default::default()
    };
    assert!(drawings.edit_view(&mut editor, top, bad).is_err());
    let empty = ViewEdit {
        name: Some(" ".into()),
        ..Default::default()
    };
    assert!(drawings.edit_view(&mut editor, top, empty).is_err());
    // Depth applies to sections only.
    let depth = ViewEdit {
        depth: Some(10.),
        ..Default::default()
    };
    assert!(drawings.edit_view(&mut editor, top, depth).is_err());
    // Deleting the main view takes its dimensions with it.
    assert!(d.dimensions.iter().any(|x| x.view == main));
    drawings.delete_view(&mut editor, main).unwrap();
    let d = editor.document.project().drawings[&id].clone();
    assert!(d.views.iter().all(|v| v.id != main));
    assert!(d.dimensions.iter().all(|x| x.view != main));
    assert!(drawings.sheet(editor.document.project()).is_ok());
    // The last view stays.
    for v in d.views[1..].iter().map(|v| v.id).collect::<Vec<_>>() {
        drawings.delete_view(&mut editor, v).unwrap();
    }
    let last = editor.document.project().drawings[&id].views[0].id;
    assert!(drawings.delete_view(&mut editor, last).is_err());
}
