//! Working without a mouse: KeyTips on the ribbon, grid nodes typed as
//! «Б/2», the keyboard snap cursor, keyboard selection and F6 focus areas.
//! Input reaches the real shell as egui key/text events (no pointer events).
use eframe::egui;
use open_bim_desktop::interaction::keyboard_cursor::Arrow;
use open_bim_desktop::interaction::selection::Click;
use open_bim_desktop::interaction::tools::default_project;
use open_bim_desktop::tools::{Editor, ToolKind, ui::Action};
use open_bim_desktop::ui::keytips::{KeyTips, button_tips, latin_of, matches, tab_items};
use open_bim_desktop::ui::ribbon::RibbonTab;
use open_bim_desktop::ui::{FocusArea, Request, Workbench, commands::Cmd, theme};
use open_bim_renderer::overlay::Projector;
use open_bim_renderer::{Bounds, Camera, DVec3, NamedView};
use std::collections::BTreeSet;

fn bench() -> Workbench {
    Workbench::new(Editor::new(default_project().unwrap()).unwrap())
}

fn apply(b: &mut Workbench, request: Option<Request>) -> bool {
    match request {
        Some(Request::Action(Action::Commit(label, commands))) => {
            b.editor.commit(&label, &commands).unwrap();
            true
        }
        _ => false,
    }
}

fn frame(ctx: &egui::Context, b: &mut Workbench, alt: bool, events: Vec<egui::Event>) {
    let mut all = vec![egui::Event::ModifiersChanged(egui::Modifiers {
        alt,
        ..Default::default()
    })];
    all.extend(events);
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1280., 800.),
        )),
        events: all,
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| {
        let requests = b.panels(ui);
        for r in requests {
            apply(b, Some(r));
        }
    });
    out.textures_delta.clear();
}

fn key(k: egui::Key) -> Vec<egui::Event> {
    [true, false]
        .map(|pressed| egui::Event::Key {
            key: k,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Default::default(),
        })
        .to_vec()
}

#[test]
fn alt_then_letters_runs_ribbon_commands() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut b = bench();
    frame(&ctx, &mut b, false, vec![]);
    // Alt pressed and released alone shows the tab letters.
    frame(&ctx, &mut b, true, vec![]);
    frame(&ctx, &mut b, false, vec![]);
    assert_eq!(b.keytips, KeyTips::Tabs);
    frame(&ctx, &mut b, false, vec![egui::Event::Text("М".into())]);
    assert_eq!(
        (b.keytips, b.ribbon_tab),
        (KeyTips::Buttons, RibbonTab::Steel)
    );
    frame(&ctx, &mut b, false, vec![egui::Event::Text("Б".into())]);
    assert_eq!(b.keytips, KeyTips::Off);
    assert_eq!(
        b.engine.tool,
        Some(ToolKind::Beam),
        "Alt, М, Б starts the beam tool"
    );
    // Esc: leave the command; F10 opens tips, Esc twice closes them.
    frame(&ctx, &mut b, false, key(egui::Key::Escape));
    assert!(!b.engine.active());
    frame(&ctx, &mut b, false, key(egui::Key::F10));
    frame(&ctx, &mut b, false, vec![egui::Event::Text("p".into())]); // Latin key of «З»? no: unmatched
    assert_eq!(b.keytips, KeyTips::Tabs);
    frame(&ctx, &mut b, false, vec![egui::Event::Text("g".into())]); // «П» on ЙЦУКЕН
    assert_eq!(
        (b.keytips, b.ribbon_tab),
        (KeyTips::Buttons, RibbonTab::Edit)
    );
    frame(&ctx, &mut b, false, key(egui::Key::Escape));
    assert_eq!(b.keytips, KeyTips::Tabs);
    frame(&ctx, &mut b, false, key(egui::Key::Escape));
    assert_eq!(b.keytips, KeyTips::Off);
    // Alt used as a modifier (Alt+B) does not toggle the tips.
    frame(&ctx, &mut b, true, key(egui::Key::B));
    frame(&ctx, &mut b, false, vec![]);
    assert_eq!(b.keytips, KeyTips::Off);
}

#[test]
fn every_tab_has_distinct_button_letters() {
    assert_eq!(latin_of('М'), Some('V'));
    assert!(matches('Б', ','));
    for tab in RibbonTab::ALL {
        let items = tab_items(tab);
        let tips: Vec<char> = button_tips(&items.iter().collect::<Vec<_>>())
            .into_iter()
            .flatten()
            .collect();
        let distinct: BTreeSet<char> = tips.iter().copied().collect();
        assert_eq!(
            distinct.len(),
            tips.len(),
            "{tab:?}: duplicate tips {tips:?}"
        );
        let enabled = items.iter().filter(|i| i.cmd.is_some()).count();
        assert_eq!(tips.len(), enabled, "{tab:?}");
    }
}

#[test]
fn typed_grid_nodes_pick_exact_intersections() {
    let mut b = bench();
    b.execute(Cmd::ToolBeam);
    b.pick_numeric("Б/2");
    assert_eq!(b.engine.picks, vec![DVec3::new(6000., 6000., 0.)]);
    let r = b.pick_numeric("в3/+3,600");
    assert!(apply(&mut b, r));
    let beam = b
        .editor
        .document
        .project()
        .elements
        .values()
        .next()
        .unwrap()
        .clone();
    let open_bim_model::ElementShape::Member { frame, length, .. } = &beam.shape else {
        panic!()
    };
    let end = frame.world(open_bim_model::Point3([0., 0., length.value()]));
    assert_eq!(end.0, [12000., 12000., 3600.]);
    for (text, z) in [
        ("Б2", 0.),
        ("2/Б", 0.),
        ("Б/2/3600", 3600.),
        ("Б/2/+3.600", 3600.),
    ] {
        b.pick_numeric(text);
        let p = *b.engine.picks.last().unwrap();
        assert_eq!(p, DVec3::new(6000., 6000., z), "{text}");
        b.execute(Cmd::UndoPick);
    }
    b.pick_numeric("Я/9");
    assert!(b.tools.message.clone().unwrap().contains("Нет такой оси"));
    assert!(b.engine.active(), "command stays active after an error");
}

fn top() -> Camera {
    let mut c = Camera::default();
    c.set_view(NamedView::Top);
    c.fit(Bounds {
        min: DVec3::new(-2000., -2000., 0.),
        max: DVec3::new(26000., 20000., 3600.),
    });
    c
}

#[test]
fn keyboard_cursor_steps_between_snap_points_and_picks() {
    let mut b = bench();
    let c = top();
    let p = Projector {
        camera: &c,
        origin: DVec3::ZERO,
        min: [0., 0.],
        size: [1200., 800.],
    };
    b.execute(Cmd::ToolColumn);
    b.keyboard_step(&p, Arrow::Right);
    assert_eq!(b.kb_cursor, Some(DVec3::new(6000., 0., 0.)), "А/1 → А/2");
    b.keyboard_step(&p, Arrow::Up);
    assert_eq!(b.kb_cursor, Some(DVec3::new(6000., 6000., 0.)), "А/2 → Б/2");
    let r = b.keyboard_space(Click::Replace);
    assert!(apply(&mut b, r));
    let col = b
        .editor
        .document
        .project()
        .elements
        .values()
        .next()
        .unwrap();
    assert_eq!(col.shape.frame().origin.0, [6000., 6000., 0.]);
    // Without a command: arrows walk over parts, Space selects, Alt+Space toggles.
    b.execute(Cmd::CancelTool);
    b.kb_cursor = None;
    b.kb_object = None;
    b.execute(Cmd::ToolColumn);
    let r = b.pick_numeric("Г/5");
    assert!(apply(&mut b, r));
    b.execute(Cmd::CancelTool);
    b.keyboard_step(&p, Arrow::Right);
    let first = b.kb_object.unwrap();
    assert!(b.keyboard_space(Click::Replace).is_none());
    assert_eq!(b.editor.selected, BTreeSet::from([first]));
    b.keyboard_step(&p, Arrow::Right);
    let second = b.kb_object.unwrap();
    assert_ne!(first, second);
    b.keyboard_space(Click::Toggle);
    assert_eq!(b.editor.selected, BTreeSet::from([first, second]));
    b.execute(Cmd::CancelTool);
    assert!(b.editor.selected.is_empty());
}

#[test]
fn f6_moves_keyboard_focus_between_window_areas() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut b = bench();
    frame(&ctx, &mut b, false, vec![]);
    let mut seen = Vec::new();
    for _ in 0..5 {
        frame(&ctx, &mut b, false, key(egui::Key::F6));
        frame(&ctx, &mut b, false, vec![]);
        seen.push((b.focus_area, ctx.memory(|m| m.focused())));
    }
    let areas: Vec<FocusArea> = seen.iter().map(|s| s.0).collect();
    assert_eq!(
        areas,
        [
            FocusArea::Ribbon,
            FocusArea::QuickLaunch,
            FocusArea::SidePane,
            FocusArea::Toolbar,
            FocusArea::Viewport
        ]
    );
    for (area, focused) in &seen {
        match area {
            FocusArea::Viewport => assert!(focused.is_none(), "viewport: no widget focus"),
            FocusArea::QuickLaunch => assert_eq!(*focused, Some(egui::Id::new("quick launch"))),
            _ => assert!(focused.is_some(), "{area:?} takes focus"),
        }
    }
    frame(&ctx, &mut b, false, {
        let mut e = key(egui::Key::F);
        for ev in &mut e {
            if let egui::Event::Key { modifiers, .. } = ev {
                // ⌘F (Ctrl+F opens the component catalogue).
                *modifiers = egui::Modifiers::COMMAND;
            }
        }
        e
    });
    frame(&ctx, &mut b, false, vec![]);
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(egui::Id::new("model search"))
    );
}

#[test]
fn typed_node_goes_into_the_coordinate_box_once() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut b = bench();
    frame(&ctx, &mut b, false, vec![]);
    b.execute(Cmd::ToolColumn);
    frame(&ctx, &mut b, false, vec![egui::Event::Text("А".into())]);
    assert_eq!(
        b.engine.numeric.as_deref(),
        Some("А"),
        "opening letter once"
    );
    frame(&ctx, &mut b, false, vec![]);
    frame(&ctx, &mut b, false, vec![egui::Event::Text("/1".into())]);
    assert_eq!(b.engine.numeric.as_deref(), Some("А/1"));
    frame(&ctx, &mut b, false, key(egui::Key::Enter));
    frame(&ctx, &mut b, false, vec![]);
    let col = b
        .editor
        .document
        .project()
        .elements
        .values()
        .next()
        .unwrap();
    assert_eq!(col.shape.frame().origin.0, [0., 0., 0.]);
}
