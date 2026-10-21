//! «Менеджер документов»: table rows, filters and batch commands.
use open_bim_commands::Command;
use open_bim_desktop::drawing_editor::manager::{self, KindFilter, Manager, StatusFilter};
use open_bim_desktop::drawing_editor::{DrawingEditor, ExportFormat};
use open_bim_desktop::tools::Editor;
use open_bim_drawings::{Status, sample::control_project};
use open_bim_model::{ElementShape, LengthMm, SheetSize};
use std::collections::BTreeSet;

#[test]
fn rows_filters_and_batch_update_issue_export_delete() {
    let f = control_project();
    let mut editor = Editor::new(f.project.clone()).unwrap();
    let mut drawings = DrawingEditor::new();
    editor.selected = [f.beam].into();
    drawings.create_part(&mut editor).unwrap();
    let beam_drawing = drawings.current.unwrap();
    editor.selected = [f.plate].into();
    drawings.create_part(&mut editor).unwrap();
    let plate_drawing = drawings.current.unwrap();
    drawings.create_general(&mut editor, SheetSize::A3).unwrap();
    let general = drawings.current.unwrap();
    drawings.create_general_data(&mut editor).unwrap();
    let general_data = drawings.current.unwrap();

    let mut m = Manager::default();
    let rows = m.visible(editor.document.project());
    assert_eq!(rows.len(), 4);
    // By set, «Общие данные» first in each set.
    let order: Vec<_> = rows.iter().map(|r| (r.set, r.sheet_no)).collect();
    let mut sorted = order.clone();
    sorted.sort();
    assert_eq!(order, sorted);
    let part_row = rows.iter().find(|r| r.id == beam_drawing).unwrap();
    assert_eq!(part_row.kind, "Деталь");
    assert!(!part_row.mark.is_empty(), "part drawings show the position");
    assert!(
        rows.iter()
            .find(|r| r.id == general)
            .unwrap()
            .mark
            .is_empty()
    );

    m.kind = KindFilter::Part;
    let ids: BTreeSet<_> = m
        .visible(editor.document.project())
        .iter()
        .map(|r| r.id)
        .collect();
    assert_eq!(ids, BTreeSet::from([beam_drawing, plate_drawing]));
    m.kind = KindFilter::GeneralData;
    assert_eq!(m.visible(editor.document.project())[0].id, general_data);
    m.kind = KindFilter::All;
    m.text = part_row.number.to_lowercase();
    assert!(
        m.visible(editor.document.project())
            .iter()
            .any(|r| r.id == beam_drawing)
    );

    // The beam changes: its drawing becomes outdated.
    let mut beam = editor.document.project().elements[&f.beam].clone();
    if let ElementShape::Member { length, .. } = &mut beam.shape {
        *length = LengthMm::new(3300.).unwrap();
    }
    editor
        .commit("Длина", &[Command::ReplaceElement { element: beam }])
        .unwrap();
    m.text.clear();
    m.status = StatusFilter::Outdated;
    let outdated: BTreeSet<_> = m
        .visible(editor.document.project())
        .iter()
        .map(|r| r.id)
        .collect();
    assert!(outdated.contains(&beam_drawing));
    assert!(!outdated.contains(&plate_drawing));

    let parts = BTreeSet::from([beam_drawing, plate_drawing]);
    // Issue refuses the outdated drawing and issues the other.
    let batch = manager::issue(&mut editor, &parts).unwrap();
    assert_eq!(batch.done, 1);
    assert_eq!(batch.skipped.len(), 1);
    // One transaction updates every outdated drawing.
    let revision = editor.document.project().revision;
    let batch = manager::update(&mut editor, &parts).unwrap();
    assert_eq!(batch.done, 1);
    assert_eq!(editor.document.project().revision, revision + 1);
    for id in &parts {
        let d = &editor.document.project().drawings[id];
        assert_eq!(
            open_bim_drawings::status(editor.document.project(), d),
            Status::UpToDate
        );
    }
    assert_eq!(manager::issue(&mut editor, &parts).unwrap().done, 1);
    m.status = StatusFilter::Issued;
    assert_eq!(m.visible(editor.document.project()).len(), 2);

    let dir = std::env::temp_dir().join(format!("open-bim-manager-{}", std::process::id()));
    let files =
        manager::export(editor.document.project(), &parts, ExportFormat::Pdf, &dir).unwrap();
    assert_eq!(files.len(), 2);
    let all: BTreeSet<_> = editor.document.project().drawings.keys().copied().collect();
    let set = manager::export_set_pdf(editor.document.project(), &all, &dir, "Комплект").unwrap();
    let bytes = std::fs::read(&set).unwrap();
    assert!(bytes.starts_with(b"%PDF"));
    assert!(
        String::from_utf8_lossy(&bytes).contains("/Type /Pages /Count 4 "),
        "all four sheets in one file"
    );
    std::fs::remove_dir_all(&dir).unwrap();

    // Delete is one step and undoable.
    assert_eq!(manager::delete(&mut editor, &parts).unwrap().done, 2);
    assert_eq!(editor.document.project().drawings.len(), 2);
    editor.history(false).unwrap();
    assert_eq!(editor.document.project().drawings.len(), 4);
}

#[test]
fn window_opens_from_the_command_and_lists_drawings() {
    use eframe::egui;
    use open_bim_desktop::ui::{Workbench, commands::Cmd};
    let f = control_project();
    let mut b = Workbench::new(Editor::new(f.project.clone()).unwrap());
    b.editor.selected = [f.beam].into();
    b.drawings.create_part(&mut b.editor).unwrap();
    b.execute(Cmd::DocumentManager);
    assert!(b.drawings.manager.open);
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut names = Vec::new();
    for _ in 0..3 {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280., 800.),
                )),
                ..Default::default()
            },
            |ui| {
                let _ = b.panels(ui);
            },
        );
        output.textures_delta.clear();
        if let Some(update) = output.platform_output.accesskit_update {
            names = update
                .nodes
                .iter()
                .filter_map(|(_, n)| n.label().map(str::to_string))
                .collect();
        }
    }
    assert!(
        names.iter().any(|n| n == "Менеджер документов"),
        "{names:?}"
    );
    assert!(
        names.iter().any(|n| n.starts_with("Отметить ")),
        "row checkbox"
    );
    b.execute(Cmd::DocumentManager);
    assert!(!b.drawings.manager.open);
}
