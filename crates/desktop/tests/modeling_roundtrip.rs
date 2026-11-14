//! Daily modeling workflow through the same picks, property commands and file
//! lifecycle as the desktop application, with real screen-space snapping.
use eframe::egui;
use open_bim_desktop::interaction::{
    Workplane,
    snap::{Snapper, Switches, View},
};
use open_bim_desktop::tools::{Editor, ui::Action};
use open_bim_desktop::ui::property_pane::{Changes, modify};
use open_bim_desktop::ui::{Request, Workbench, commands::Cmd, theme};
use open_bim_desktop::ui::{close::CloseChoice, project_change::ProjectChange};
use open_bim_model::{ElementShape, ObjectId, Point3, PropertyValue};
use open_bim_renderer::{Bounds, Camera, DVec3, NamedView, Scene};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

fn apply(b: &mut Workbench, r: Option<Request>) -> bool {
    match r {
        Some(Request::Action(Action::Commit(label, commands))) => {
            b.editor.commit(&label, &commands).unwrap();
            true
        }
        Some(Request::Action(Action::Edit(edit))) => {
            b.editor.edit(&edit).unwrap();
            true
        }
        _ => false,
    }
}

fn pick(b: &mut Workbench, target: [f64; 3]) -> bool {
    let scene = std::sync::Arc::new(Scene::from_project(b.editor.document.project()).unwrap());
    let mut camera = Camera::default();
    camera.set_view(NamedView::Top);
    camera.fit(Bounds {
        min: DVec3::new(-2000., -2000., -1000.),
        max: DVec3::new(14000., 14000., 6000.),
    });
    let view = View {
        camera: &camera,
        origin: scene.origin,
        size: [1200., 800.],
    };
    let point = DVec3::from_array(target);
    let screen = view
        .screen(point)
        .unwrap_or_else(|| panic!("{target:?} outside camera depth"));
    let hit = Snapper::default()
        .snap(
            b.editor.document.project(),
            &scene,
            view,
            [screen[0] + 3., screen[1] - 2.],
            Switches {
                depth_plane: true,
                geometry: false,
                ..Default::default()
            },
            12.,
            None,
            &Workplane::at_elevation(target[2]),
        )
        .expect("snap near grid node");
    assert!(
        hit.point.distance(point) < 1e-6,
        "{target:?} snapped to {:?}",
        hit.point
    );
    let request = b.pick(hit.point);
    apply(b, request)
}

fn ends(b: &Workbench, id: ObjectId) -> [DVec3; 2] {
    let ElementShape::Member { frame, length, .. } =
        &b.editor.document.project().elements[&id].shape
    else {
        panic!("member");
    };
    [
        DVec3::from_array(frame.origin.0),
        DVec3::from_array(frame.world(Point3([0., 0., length.value()])).0),
    ]
}

fn same_points(actual: [DVec3; 2], expected: [DVec3; 2]) -> bool {
    actual
        .into_iter()
        .zip(expected)
        .all(|(a, e)| a.distance(e) < 1e-6)
}

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("open-bim-roundtrip-{}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn column_beam_group_edit_snapped_copy_undo_redo_save_and_reopen() {
    let temp = Temp::new();
    let path = temp.0.join("Каркас.obim");
    let mut b = Workbench::new(
        Editor::new(open_bim_desktop::interaction::tools::default_project().unwrap()).unwrap(),
    );
    b.set_project_path(path.clone());
    b.save_project().unwrap();
    let initial = b.editor.document.project().clone();
    b.execute(Cmd::ToolColumn);
    assert!(pick(&mut b, [0., 0., 0.]));
    let column = *b.editor.document.project().elements.keys().next().unwrap();
    b.execute(Cmd::CancelTool);
    b.execute(Cmd::ToolBeam);
    assert!(!pick(&mut b, [0., 0., 3600.]));
    assert!(pick(&mut b, [6000., 0., 3600.]));
    let beam = *b
        .editor
        .document
        .project()
        .elements
        .keys()
        .find(|id| **id != column)
        .unwrap();
    b.execute(Cmd::CancelTool);
    assert!(same_points(
        ends(&b, column),
        [DVec3::ZERO, DVec3::new(0., 0., 3600.)]
    ));
    assert!(same_points(
        ends(&b, beam),
        [DVec3::new(0., 0., 3600.), DVec3::new(6000., 0., 3600.)]
    ));

    b.editor.selected = [column, beam].into();
    let before_edit = b.editor.document.project().clone();
    let depth = b.editor.document.history_depth().0;
    let commands = modify(
        b.editor.document.project(),
        &[column, beam],
        &Changes {
            name: Some("Каркас".into()),
            class: Some("3".into()),
            ..Default::default()
        },
    )
    .unwrap();
    b.editor.commit("Групповая правка", &commands).unwrap();
    assert_eq!(b.editor.document.history_depth().0, depth + 1);
    let edited = b.editor.document.project().clone();
    b.editor.history(false).unwrap();
    assert_eq!(b.editor.document.project(), &before_edit);
    b.editor.history(true).unwrap();
    assert_eq!(b.editor.document.project(), &edited);

    b.execute(Cmd::CopyByPicks);
    assert!(!pick(&mut b, [0., 0., 0.]));
    assert!(pick(&mut b, [0., 6000., 0.]));
    b.execute(Cmd::CancelTool);
    let copied = b.editor.document.project().clone();
    assert_eq!(copied.elements.len(), 4);
    for original in [column, beam] {
        let wanted = ends(&b, original).map(|p| p + DVec3::new(0., 6000., 0.));
        let matches = copied
            .elements
            .keys()
            .filter(|id| **id != column && **id != beam && same_points(ends(&b, **id), wanted))
            .count();
        assert_eq!(matches, 1, "one translated copy of each member");
    }
    assert!(copied.elements.values().all(|e| e.name == "Каркас"
        && e.properties.get("class") == Some(&PropertyValue::Text("3".into()))));
    assert_eq!(b.editor.document.history_depth().0, depth + 2);
    b.editor.history(false).unwrap();
    assert_eq!(b.editor.document.project(), &edited);
    b.editor.history(true).unwrap();
    assert_eq!(b.editor.document.project(), &copied);
    b.save_project().unwrap();
    assert_eq!(
        open_bim_persistence::load(open_bim_desktop::autosave::backup_path(&path)).unwrap(),
        initial
    );
    assert!(b.change_project(ProjectChange::New).is_some());
    b.start_open(path.clone());
    assert!(b.project_change_pending());
    b.resolve_project_change(CloseChoice::Discard);
    let start = Instant::now();
    while b.busy() && start.elapsed() < Duration::from_secs(10) {
        b.poll_job();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!b.busy());
    assert_eq!(
        b.editor.document.project(),
        &copied,
        "IDs, properties, level bindings and geometry all survive reopening"
    );
    assert_eq!(b.tools.path, path.to_string_lossy());
    assert_eq!(b.editor.document.history_depth(), (0, 0));
    assert_eq!(
        Scene::from_project(b.editor.document.project())
            .unwrap()
            .instances
            .len(),
        4
    );
    assert!(b.request_close(), "reopened model is saved");
}

fn ui_frame(ctx: &egui::Context, b: &mut Workbench, events: Vec<egui::Event>) -> egui::FullOutput {
    ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280., 800.),
            )),
            events,
            ..Default::default()
        },
        |ui| {
            let _ = b.panels(ui);
            egui::CentralPanel::default().show(ui, |_| {});
        },
    )
}

#[test]
fn escape_closes_material_popup_before_cancelling_group_selection() {
    let p = open_bim_persistence::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/steel_release/source.obim"
    ))
    .unwrap();
    let mut b = Workbench::new(Editor::new(p).unwrap());
    b.editor.selected = b
        .editor
        .document
        .project()
        .elements
        .keys()
        .copied()
        .collect();
    let selected = b.editor.selected.clone();
    let model = b.editor.document.project().clone();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    theme::apply(&ctx);
    for _ in 0..8 {
        ui_frame(&ctx, &mut b, vec![]);
    }
    let output = ui_frame(&ctx, &mut b, vec![]);
    let update = output.platform_output.accesskit_update.unwrap();
    let rect = update
        .nodes
        .iter()
        .find(|(_, n)| n.role() == egui::accesskit::Role::ComboBox && n.label() == Some("Материал"))
        .unwrap()
        .1
        .bounds()
        .unwrap();
    let pos = egui::pos2(
        ((rect.x0 + rect.x1) / 2.) as f32,
        ((rect.y0 + rect.y1) / 2.) as f32,
    );
    ui_frame(
        &ctx,
        &mut b,
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    assert!(
        egui::Popup::is_any_open(&ctx),
        "the real material popup is open"
    );
    let escape = || {
        [true, false]
            .into_iter()
            .map(|pressed| egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
            .collect()
    };
    ui_frame(&ctx, &mut b, escape());
    assert_eq!(
        b.editor.selected, selected,
        "closing a material list must preserve the group for copy/edit"
    );
    assert!(!egui::Popup::is_any_open(&ctx));
    ui_frame(&ctx, &mut b, escape());
    assert!(
        b.editor.selected.is_empty(),
        "a second Escape cancels the selection"
    );
    assert_eq!(b.editor.document.project(), &model);
}
