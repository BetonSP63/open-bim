use open_bim_commands::Command;
use open_bim_desktop::drawing_editor::{DrawingEditor, ExportFormat};
use open_bim_desktop::tools::Editor;
use open_bim_drawings::{Status, sample::control_project};
use open_bim_model::{Anchor, AnchorPoint, DimensionAxis, ElementShape, LengthMm, SheetSize};

#[test]
fn editor_changes_sheet_scale_view_chain_and_text_with_undo_redo() {
    let f = control_project();
    let mut editor = Editor::new(f.project.clone()).unwrap();
    let mut drawings = DrawingEditor::new();
    editor.selected = [f.beam].into();
    drawings.create_part(&mut editor).unwrap();
    let id = drawings.current.unwrap();
    let snapshot = |e: &Editor| e.document.project().drawings[&id].clone();
    let mut states = vec![snapshot(&editor)];
    drawings.set_sheet(&mut editor, SheetSize::A2).unwrap();
    states.push(snapshot(&editor));
    drawings.set_scale(&mut editor, 20.).unwrap();
    states.push(snapshot(&editor));
    drawings.move_view(&mut editor, 2, [200., 150.]).unwrap();
    states.push(snapshot(&editor));
    editor.selected = [f.beam, f.plate].into();
    let dim = drawings
        .add_dimension(&mut editor, 1, DimensionAxis::Horizontal)
        .unwrap();
    states.push(snapshot(&editor));
    drawings
        .add_anchor(
            &mut editor,
            dim,
            Anchor {
                object: f.beam,
                point: AnchorPoint::End,
            },
        )
        .unwrap();
    states.push(snapshot(&editor));
    drawings.remove_anchor(&mut editor, dim, 0).unwrap();
    states.push(snapshot(&editor));
    let text = drawings
        .add_text(&mut editor, "Сварка Э42 · ГОСТ 5264", [40., 80.], 5.)
        .unwrap();
    states.push(snapshot(&editor));
    drawings
        .edit_text(&mut editor, text, "Швы по ГОСТ 5264-80", [45., 85.])
        .unwrap();
    states.push(snapshot(&editor));
    let last = snapshot(&editor);
    assert_eq!(last.sheet, SheetSize::A2);
    assert_eq!(last.scale, 20.);
    assert_eq!(last.views[1].position, [200., 150.]);
    assert_eq!(last.texts[0].text, "Швы по ГОСТ 5264-80");
    // Sheet regenerates with the new scale and text.
    let sheet = drawings.sheet(editor.document.project()).unwrap();
    assert_eq!(sheet.size, [594., 420.]);
    assert!(sheet.labels.iter().any(|l| l.text == "Швы по ГОСТ 5264-80"));
    // Every edit is one undo step; redo restores in order.
    for expected in states.iter().rev().skip(1) {
        editor.history(false).unwrap();
        assert_eq!(&snapshot(&editor), expected);
    }
    for expected in states.iter().skip(1) {
        editor.history(true).unwrap();
        assert_eq!(&snapshot(&editor), expected);
    }
}

#[test]
fn model_revision_outdates_drawing_until_update_and_exports_all_formats() {
    let f = control_project();
    let mut editor = Editor::new(f.project.clone()).unwrap();
    let mut drawings = DrawingEditor::new();
    editor.selected = [f.beam].into();
    drawings.create_part(&mut editor).unwrap();
    let id = drawings.current.unwrap();
    let status = |e: &Editor| {
        open_bim_drawings::status(e.document.project(), &e.document.project().drawings[&id])
    };
    assert_eq!(status(&editor), Status::UpToDate);
    let mut beam = editor.document.project().elements[&f.beam].clone();
    if let ElementShape::Member { length, .. } = &mut beam.shape {
        *length = LengthMm::new(3300.).unwrap();
    }
    editor
        .commit("Длина", &[Command::ReplaceElement { element: beam }])
        .unwrap();
    assert!(matches!(status(&editor), Status::Outdated { .. }));
    assert!(drawings.issue(&mut editor).is_err());
    drawings.update(&mut editor).unwrap();
    assert_eq!(status(&editor), Status::UpToDate);
    drawings.issue(&mut editor).unwrap();
    let dir = std::env::temp_dir().join(format!("open-bim-export-{}", std::process::id()));
    for format in [ExportFormat::Pdf, ExportFormat::Svg, ExportFormat::Dxf] {
        let path = drawings
            .export(editor.document.project(), format, &dir)
            .unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let head = match format {
            ExportFormat::Pdf => &b"%PDF-1.7"[..],
            ExportFormat::Svg => &b"<svg"[..],
            ExportFormat::Dxf => &b"  0\nSECTION"[..],
        };
        assert!(bytes.starts_with(head), "{path:?}");
    }
    std::fs::remove_dir_all(dir).unwrap();
}
