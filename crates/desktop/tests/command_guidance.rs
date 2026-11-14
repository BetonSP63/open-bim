mod headless;
use eframe::egui;
use headless::HeadlessFrame;
use open_bim_desktop::tools::Editor;
use open_bim_desktop::ui::{
    Workbench,
    commands::{self, Cmd},
};
use open_bim_renderer::DVec3;

fn frame(ctx: &egui::Context, b: &mut Workbench, events: Vec<egui::Event>) -> egui::FullOutput {
    ctx.run_headless(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1024., 768.),
            )),
            events,
            ..Default::default()
        },
        |ui| {
            b.panels(ui);
        },
    )
}

#[test]
fn visible_cancel_uses_dispatcher_without_changing_model_or_selection() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut b = Workbench::new(
        Editor::new(open_bim_desktop::interaction::tools::default_project().unwrap()).unwrap(),
    );
    b.editor
        .selected
        .insert(*b.editor.document.project().axes.keys().next().unwrap());
    let before = b.editor.document.project().clone();
    let selected = b.editor.selected.clone();
    b.execute(Cmd::Measure);
    b.pick(DVec3::ZERO);
    let output = frame(&ctx, &mut b, vec![]);
    let caption = commands::caption(Cmd::CancelTool);
    let bounds = output
        .platform_output
        .accesskit_update
        .unwrap()
        .nodes
        .into_iter()
        .find_map(|(_, n)| {
            (n.role() == egui::accesskit::Role::Button && n.label() == Some(caption.as_str()))
                .then(|| n.bounds())
                .flatten()
        })
        .expect("active command has a visible cancel button");
    assert!(bounds.y0 >= 0. && bounds.y1 <= 768.);
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
    assert!(!b.in_command());
    assert_eq!(*b.editor.document.project(), before);
    assert_eq!(b.editor.selected, selected);
}

#[test]
fn switching_commands_replaces_the_visible_step_and_keeps_model_unchanged() {
    let mut b = Workbench::new(
        Editor::new(open_bim_desktop::interaction::tools::default_project().unwrap()).unwrap(),
    );
    let before = b.editor.document.project().clone();
    b.execute(Cmd::Measure);
    assert!(b.prompt().contains("первую точку измерения"));
    b.pick(DVec3::ZERO);
    assert!(b.prompt().contains("вторую точку измерения"));
    b.execute(Cmd::WorkPlaneThreePoints);
    assert_eq!(b.workplane_picks, Some(vec![]));
    assert!(!b.prompt().contains("точку измерения"));
    assert!(b.measure.is_none());
    assert_eq!(*b.editor.document.project(), before);
}
