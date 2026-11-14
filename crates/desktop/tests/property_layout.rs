//! The property pane must leave a usable viewport across frames and selections.
use eframe::egui;
use open_bim_desktop::tools::Editor;
use open_bim_desktop::ui::{Workbench, theme};

fn bench() -> Workbench {
    let p = open_bim_persistence::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/steel_release/source.obim"
    ))
    .unwrap();
    Workbench::new(Editor::new(p).unwrap())
}

fn frame(ctx: &egui::Context, b: &mut Workbench, size: [f32; 2]) -> egui::Rect {
    frame_output(ctx, b, size, vec![]).0
}

fn frame_output(
    ctx: &egui::Context,
    b: &mut Workbench,
    size: [f32; 2],
    events: Vec<egui::Event>,
) -> (egui::Rect, egui::FullOutput) {
    let mut viewport = egui::Rect::NOTHING;
    let output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(size[0], size[1]),
            )),
            events,
            ..Default::default()
        },
        |ui| {
            b.panels(ui);
            egui::CentralPanel::default().show(ui, |ui| {
                viewport = ui.max_rect();
            });
        },
    );
    (viewport, output)
}

#[test]
fn long_names_fit_the_pane_and_modify_remains_reachable_by_scrolling() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    theme::apply(&ctx);
    let mut p = bench().editor.document.project().clone();
    for (id, profile) in &mut p.profiles {
        profile.name = format!("{} {}", "Длинное название профиля ".repeat(12), id.0);
    }
    for (id, material) in &mut p.materials {
        material.name = format!("{} {}", "Длинное название материала ".repeat(12), id.0);
    }
    let id = *p
        .elements
        .iter()
        .find(|(_, e)| matches!(e.shape, open_bim_model::ElementShape::Member { .. }))
        .unwrap()
        .0;
    let mut b = Workbench::new(Editor::new(p).unwrap());
    b.editor.selected.insert(id);
    let size = [1280., 800.];
    for _ in 0..25 {
        frame(&ctx, &mut b, size);
    }
    let (viewport, output) = frame_output(&ctx, &mut b, size, vec![]);
    assert!(viewport.width() >= size[0] * 0.5);
    // Side and central panels have different margins; use the control's own panel.
    let pane = egui::containers::panel::PanelState::load(&ctx, egui::Id::new("model tools"))
        .unwrap()
        .outer_rect;
    let nodes = &output.platform_output.accesskit_update.unwrap().nodes;
    for wanted in ["Имя", "Профиль", "Материал"] {
        let node = nodes
            .iter()
            .map(|(_, n)| n)
            .find(|n| {
                let name = n.label().map(str::to_owned).unwrap_or_else(|| {
                    n.labelled_by()
                        .iter()
                        .filter_map(|id| {
                            nodes
                                .iter()
                                .find(|(key, _)| key == id)
                                .and_then(|(_, label)| label.label().or(label.value()))
                        })
                        .collect::<Vec<_>>()
                        .join(" ")
                });
                name == wanted
                    && matches!(
                        n.role(),
                        egui::accesskit::Role::TextInput
                            | egui::accesskit::Role::ComboBox
                            | egui::accesskit::Role::Button
                    )
            })
            .unwrap_or_else(|| panic!("missing field {wanted}"));
        let bounds = node.bounds().unwrap();
        assert!(
            bounds.x0 >= pane.left() as f64 && bounds.x1 <= pane.right() as f64,
            "{wanted} outside the property pane: {bounds:?}"
        );
    }
    let mut reachable = false;
    let mut observed = Vec::new();
    for _ in 0..20 {
        let (_, output) = frame_output(
            &ctx,
            &mut b,
            size,
            vec![
                egui::Event::PointerMoved(pane.center()),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0., -200.),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let update = output.platform_output.accesskit_update.unwrap();
        observed = update
            .nodes
            .iter()
            .filter(|(_, n)| n.label() == Some("Изменить"))
            .map(|(_, n)| format!("{:?}: {:?}; pane {pane:?}", n.role(), n.bounds()))
            .collect::<Vec<_>>();
        reachable |= update.nodes.iter().any(|(_, n)| {
            n.role() == egui::accesskit::Role::Button
                && n.label() == Some("Изменить")
                && n.bounds()
                    .is_some_and(|r| r.y0 >= pane.top() as f64 && r.y1 <= pane.bottom() as f64)
        });
        if reachable {
            break;
        }
    }
    assert!(
        reachable,
        "Modify button must be reachable by vertical scrolling: {observed:?}"
    );
}

#[test]
fn group_properties_do_not_grow_into_the_viewport_frame_after_frame() {
    for size in [[1280., 800.], [1920., 1080.]] {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut b = bench();
        b.editor.selected = b
            .editor
            .document
            .project()
            .elements
            .keys()
            .copied()
            .collect();
        for pass in 0..30 {
            let rect = frame(&ctx, &mut b, size);
            assert!(
                rect.width() >= size[0] * 0.5,
                "{size:?}, frame {pass}: viewport is only {} wide",
                rect.width()
            );
        }
    }
}

#[test]
fn selection_changes_and_window_resize_keep_the_pane_bounded() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut b = bench();
    let ids = b
        .editor
        .document
        .project()
        .elements
        .keys()
        .copied()
        .collect::<Vec<_>>();
    for size in [[1920., 1080.], [1280., 800.], [1024., 768.]] {
        for selected in [vec![], vec![ids[0]], ids.clone(), vec![]] {
            b.editor.selected = selected.into_iter().collect();
            for pass in 0..12 {
                let rect = frame(&ctx, &mut b, size);
                assert!(
                    rect.width() >= size[0] * 0.5,
                    "{size:?}, frame {pass}: viewport is only {} wide",
                    rect.width()
                );
            }
        }
    }
}
