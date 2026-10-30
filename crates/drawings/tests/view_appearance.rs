use open_bim_drawings::sample::control_project;
use open_bim_drawings::{LineStyle, general_drawing, generate, svg};
use open_bim_model::{LinePen, LineType, ObjectId, Project, SheetSize};

#[test]
fn hidden_parts_do_not_draw_or_occlude_and_settings_are_local_and_persistent() {
    let f = control_project();
    let mut d = general_drawing(
        &f.project,
        ObjectId(900),
        vec![f.beam, f.plate],
        "УЗ-1",
        SheetSize::A3,
    )
    .unwrap();
    let pen = LinePen {
        line_type: LineType::DashDot,
        width: 1.2,
    };
    d.views[0].appearance.visible = pen;
    d.views[0].appearance.hidden_objects.insert(f.plate);
    d.views[0].appearance.show_hidden = false;
    let sheet = generate(&f.project, &d).unwrap();
    assert!(
        sheet.views[0]
            .lines
            .iter()
            .any(|l| l.style == LineStyle::Custom(pen))
    );
    assert!(
        sheet.views[0]
            .lines
            .iter()
            .all(|l| l.object != Some(f.plate) && l.style != LineStyle::Hidden)
    );
    assert!(
        sheet.views[1]
            .lines
            .iter()
            .any(|l| l.object == Some(f.plate))
    );
    assert!(
        sheet.views[1]
            .lines
            .iter()
            .all(|l| !matches!(l.style, LineStyle::Custom(_)))
    );
    let rendered = svg::render(&sheet);
    assert!(rendered.contains("stroke-width=\"1.2\" stroke-dasharray=\"12 3 1 3\""));
    let mut project = f.project.clone();
    project.next_id = 901;
    project.drawings.insert(d.id, d.clone());
    let restored = Project::from_json(&serde_json::to_string(&project).unwrap()).unwrap();
    assert_eq!(restored.drawings[&d.id], d);
    assert_eq!(svg::render(&generate(&restored, &d).unwrap()), rendered);
    // Hiding the plate must produce the same beam visibility as removing it
    // from the drawing's sources, while retaining the original view centre.
    let mut beam_only = d.clone();
    beam_only.kind = open_bim_model::DrawingKind::General {
        objects: vec![f.beam],
    };
    let beam_sheet = generate(&f.project, &beam_only).unwrap();
    let runs = |g: &open_bim_drawings::ViewGraphic| {
        g.lines
            .iter()
            .filter(|l| l.object == Some(f.beam))
            .map(|l| (l.style, l.points.len()))
            .collect::<Vec<_>>()
    };
    assert_eq!(runs(&sheet.views[0]), runs(&beam_sheet.views[0]));
}

#[test]
fn invalid_line_width_and_missing_section_host_are_rejected() {
    let f = control_project();
    let mut d = general_drawing(
        &f.project,
        ObjectId(900),
        vec![f.beam],
        "УЗ-1",
        SheetSize::A3,
    )
    .unwrap();
    for width in [0., -1., 2.1, f64::NAN, f64::INFINITY] {
        d.views[0].appearance.visible.width = width;
        assert!(d.validate().is_err());
    }
    d.views[0].appearance.visible.width = 0.5;
    d.views[0].section_host = Some(999);
    assert!(d.validate().is_err());
}
