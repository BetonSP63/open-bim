mod headless;
use eframe::egui;
use headless::HeadlessFrame;
use open_bim_desktop::interaction::Workplane;
use open_bim_desktop::tools::Editor;
use open_bim_desktop::ui::{Workbench, commands::Cmd};
use open_bim_renderer::DVec3;

fn bench() -> Workbench {
    Workbench::new(
        Editor::new(open_bim_desktop::interaction::tools::default_project().unwrap()).unwrap(),
    )
}

fn frame(ctx: &egui::Context, b: &mut Workbench, events: Vec<egui::Event>) -> egui::FullOutput {
    ctx.run_headless(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280., 800.),
            )),
            events,
            ..Default::default()
        },
        |ui| {
            assert!(b.panels(ui).is_empty());
        },
    )
}

#[test]
fn preview_and_submission_agree_on_rotated_plane_and_all_coordinate_modes() {
    for text in [
        "!500,600,700",
        "$20,30,40",
        "@20,30,40",
        "R 20,30,40",
        "250<90",
        "250",
    ] {
        let mut b = bench();
        b.execute(Cmd::Measure);
        b.engine.workplane = Workplane::from_points(
            DVec3::new(100., 200., 300.),
            DVec3::new(100., 201., 300.),
            DVec3::new(100., 200., 301.),
        )
        .unwrap();
        b.pick(DVec3::new(400., 500., 600.));
        b.cursor = Some(DVec3::new(400., 800., 1000.));
        let before = b.editor.document.project().clone();
        let picks = b.measure.clone();
        let preview = b.numeric_point(text).unwrap();
        assert_eq!(b.measure, picks, "preview must not accept a point");
        assert_eq!(*b.editor.document.project(), before);
        b.pick_numeric(text);
        assert_eq!(b.measured.unwrap().1, preview, "{text}");
    }
}

#[test]
fn grid_node_preview_preserves_grid_priority() {
    let mut b = bench();
    b.execute(Cmd::Measure);
    let point = b.numeric_point("Б/2").unwrap();
    assert!(b.measure.as_ref().unwrap().is_empty());
    b.pick_numeric("Б/2");
    assert_eq!(b.measure, Some(vec![point]));
}

#[test]
fn invalid_preview_preserves_input_and_picks_and_correction_is_accepted() {
    let mut b = bench();
    b.execute(Cmd::Measure);
    b.pick(DVec3::ZERO);
    b.engine.numeric = Some("!NaN,0,0".into());
    assert!(b.numeric_point("!NaN,0,0").is_err());
    b.pick_numeric("!NaN,0,0");
    assert_eq!(b.engine.numeric.as_deref(), Some("!NaN,0,0"));
    assert_eq!(b.measure, Some(vec![DVec3::ZERO]));
    assert!(b.engine.error.is_some());
    b.pick_numeric("R 300,400,0");
    assert_eq!(b.measured.unwrap().1, DVec3::new(300., 400., 0.));
    assert!(b.engine.error.is_none());
}

#[test]
fn clicking_confirmation_accepts_exactly_the_previewed_point() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut b = bench();
    b.execute(Cmd::Measure);
    b.pick(DVec3::ZERO);
    let before = b.editor.document.project().clone();
    frame(&ctx, &mut b, vec![egui::Event::Text("R 300,400,0".into())]);
    // A floating window needs a sizing pass before its widgets receive input.
    for _ in 0..4 {
        frame(&ctx, &mut b, vec![]);
    }
    let output = frame(&ctx, &mut b, vec![]);
    let bounds = output
        .platform_output
        .accesskit_update
        .unwrap()
        .nodes
        .into_iter()
        .find_map(|(_, n)| {
            (n.role() == egui::accesskit::Role::Button
                && n.label() == Some("Указать точку · Enter"))
            .then(|| n.bounds())
            .flatten()
        })
        .expect("numeric confirmation is accessible");
    assert!(bounds.y0 >= 0. && bounds.y1 <= 800.);
    let pos = egui::pos2(
        ((bounds.x0 + bounds.x1) / 2.) as f32,
        ((bounds.y0 + bounds.y1) / 2.) as f32,
    );
    for pressed in [true, false] {
        frame(
            &ctx,
            &mut b,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
    assert_eq!(b.measured, Some((DVec3::ZERO, DVec3::new(300., 400., 0.))));
    assert!(b.engine.numeric.is_none());
    assert_eq!(*b.editor.document.project(), before);
}
