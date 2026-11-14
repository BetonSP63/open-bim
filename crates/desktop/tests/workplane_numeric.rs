use eframe::egui;
use open_bim_desktop::interaction::Workplane;
use open_bim_desktop::tools::Editor;
use open_bim_desktop::ui::{Workbench, commands::Cmd, theme};
use open_bim_renderer::DVec3;

fn bench() -> Workbench {
    Workbench::new(
        Editor::new(open_bim_desktop::interaction::tools::default_project().unwrap()).unwrap(),
    )
}

fn frame(ctx: &egui::Context, b: &mut Workbench, events: Vec<egui::Event>) {
    let _ = ctx.run_ui(
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
    );
}

fn key(key: egui::Key) -> Vec<egui::Event> {
    [true, false]
        .map(|pressed| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Default::default(),
        })
        .to_vec()
}

#[test]
fn global_and_relative_coordinates_define_a_tilted_plane() {
    let mut b = bench();
    b.execute(Cmd::WorkPlaneThreePoints);
    for text in ["!100,200,300.125", "R 6000,0,1200", "R -6000,4000,-1200"] {
        assert!(b.pick_numeric(text).is_none());
    }
    let expected = Workplane::from_points(
        DVec3::new(100., 200., 300.125),
        DVec3::new(6100., 200., 1500.125),
        DVec3::new(100., 4200., 300.125),
    )
    .unwrap();
    assert_eq!(b.engine.workplane, expected);
    assert!(b.workplane_picks.is_none());
    assert_eq!(b.tools.workplane, "300.125");
    assert!(b.editor.document.project().elements.is_empty());
}

#[test]
fn invalid_input_preserves_previous_picks_and_can_be_corrected() {
    let mut b = bench();
    b.execute(Cmd::WorkPlaneThreePoints);
    b.pick_numeric("!100,200,300.125");
    let points = b.workplane_picks.clone();
    b.pick_numeric("!NaN,0,0");
    assert!(b.engine.error.is_some());
    assert_eq!(b.workplane_picks, points);
    b.pick_numeric("1000,0,0");
    assert!(b.engine.error.is_none());
    b.pick_numeric("-1000,1000,0");
    assert!(b.workplane_picks.is_none());
    assert!(b.engine.workplane.is_horizontal());
    assert_eq!(b.engine.workplane.origin.z, 300.125);
}

#[test]
fn coordinate_window_submits_each_point_and_escape_only_closes_the_input_first() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut b = bench();
    frame(&ctx, &mut b, vec![]);
    b.execute(Cmd::WorkPlaneThreePoints);
    for (index, text) in ["!100,200,300.125", "1000,0,0", "-1000,1000,0"]
        .into_iter()
        .enumerate()
    {
        frame(&ctx, &mut b, vec![egui::Event::Text(text.into())]);
        assert_eq!(b.engine.numeric.as_deref(), Some(text));
        for _ in 0..4 {
            frame(&ctx, &mut b, vec![]);
        }
        frame(&ctx, &mut b, key(egui::Key::Enter));
        assert!(b.engine.numeric.is_none());
        if index < 2 {
            assert_eq!(b.workplane_picks.as_ref().unwrap().len(), index + 1);
        }
    }
    assert_eq!(b.engine.workplane.origin, DVec3::new(100., 200., 300.125));
    assert!(b.engine.workplane.is_horizontal());
    assert!(b.workplane_picks.is_none());

    b.execute(Cmd::WorkPlaneThreePoints);
    frame(&ctx, &mut b, vec![egui::Event::Text("!900".into())]);
    frame(&ctx, &mut b, key(egui::Key::Escape));
    assert!(b.engine.numeric.is_none());
    assert_eq!(b.workplane_picks, Some(vec![]));
    frame(&ctx, &mut b, key(egui::Key::Escape));
    assert!(b.workplane_picks.is_none());
}
