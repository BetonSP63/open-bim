mod headless;
use headless::HeadlessFrame;
use open_bim_desktop::interaction::Workplane;
use open_bim_desktop::tools::{Editor, number, ui::Action};
use open_bim_desktop::ui::{Request, Workbench, commands::Cmd, views::ModelView};
use open_bim_model::{ElementShape, Point3};
use open_bim_renderer::DVec3;

fn bench() -> Workbench {
    Workbench::new(
        Editor::new(open_bim_desktop::interaction::tools::default_project().unwrap()).unwrap(),
    )
}

#[test]
fn switching_views_preserves_fractional_elevations_in_the_editable_field() {
    let mut b = bench();
    let mut view = ModelView::model_3d();
    view.name = "Дробная отметка".into();
    view.workplane = Workplane::at_elevation(3600.123456789);
    b.views.push(view);
    for _ in 0..4 {
        b.apply_view(1);
        assert_eq!(number(&b.tools.workplane).unwrap(), 3600.123456789);
        // The viewport applies the value from this field before the next pick.
        b.engine.workplane.origin.z = number(&b.tools.workplane).unwrap();
        b.apply_view(0);
    }
    b.apply_view(1);
    assert_eq!(b.engine.workplane.origin.z, 3600.123456789);
}

#[test]
fn three_point_horizontal_planes_replace_stale_height_for_both_point_orders() {
    for clockwise in [false, true] {
        let mut b = bench();
        b.tools.workplane = "27.25".into();
        b.execute(Cmd::WorkPlaneThreePoints);
        let origin = DVec3::new(100., 200., 4512.875123);
        b.pick(origin);
        b.pick(origin + DVec3::X * 1000.);
        b.pick(origin + DVec3::Y * if clockwise { -1000. } else { 1000. });
        assert_eq!(b.engine.workplane.origin, origin);
        assert_eq!(number(&b.tools.workplane).unwrap(), origin.z);
        b.tools.workplane = "4513.125123".into();
        b.sync_workplane_elevation();
        assert_eq!(b.engine.workplane.origin.z, 4513.125123);
        assert!(
            b.engine.workplane.axes[2].distance(if clockwise { DVec3::NEG_Z } else { DVec3::Z })
                < 1e-12
        );
    }
}

#[test]
fn loading_saved_views_restores_the_height_field_without_rounding() {
    let mut b = bench();
    let plane = Workplane::at_elevation(-125.123456789);
    b.views[0].workplane = plane;
    b.store_model_views("Рабочая плоскость вида").unwrap();
    let bytes = open_bim_persistence::encode(b.editor.document.project()).unwrap();
    let restored =
        Workbench::new(Editor::new(open_bim_persistence::decode(&bytes).unwrap()).unwrap());
    assert_eq!(restored.engine.workplane, plane);
    assert_eq!(number(&restored.tools.workplane).unwrap(), plane.origin.z);
}

#[test]
fn beam_numeric_input_uses_the_tilted_plane_axes_and_origin() {
    let mut b = bench();
    let origin = DVec3::new(100., 200., 300.125);
    b.execute(Cmd::WorkPlaneThreePoints);
    for point in [
        origin,
        origin + DVec3::new(6000., 0., 1200.),
        origin + DVec3::Y * 4000.,
    ] {
        b.pick(point);
    }
    let plane = b.engine.workplane;
    b.tools.workplane = "99".into();
    b.sync_workplane_elevation();
    assert_eq!(
        b.engine.workplane, plane,
        "height edits cannot flatten a tilted plane"
    );
    b.execute(Cmd::ToolBeam);
    assert!(b.pick_numeric("$0,0,0").is_none());
    let Some(Request::Action(Action::Commit(label, commands))) = b.pick_numeric("$6000,0,0") else {
        panic!("beam command");
    };
    b.editor.commit(&label, &commands).unwrap();
    let element = b
        .editor
        .document
        .project()
        .elements
        .values()
        .next()
        .unwrap();
    let ElementShape::Member { frame, length, .. } = &element.shape else {
        panic!("beam");
    };
    assert!(DVec3::from_array(frame.origin.0).distance(origin) < 1e-6);
    let end = DVec3::from_array(frame.world(Point3([0., 0., length.value()])).0);
    assert!(end.distance(plane.to_world(DVec3::new(6000., 0., 0.))) < 1e-6);
}

#[test]
fn invalid_height_input_does_not_move_the_plane() {
    let mut b = bench();
    let plane = Workplane::at_elevation(12.3456789);
    b.set_workplane(plane);
    for text in ["-", "NaN", "inf", "ошибка"] {
        b.tools.workplane = text.into();
        b.sync_workplane_elevation();
        assert_eq!(b.engine.workplane, plane);
    }
}

#[test]
fn height_control_is_readonly_for_tilted_planes_but_editable_for_either_horizontal_normal() {
    use eframe::egui;
    let tilted =
        Workplane::from_points(DVec3::ZERO, DVec3::new(1000., 0., 200.), DVec3::Y * 1000.).unwrap();
    let reversed = Workplane::from_points(DVec3::ZERO, DVec3::X, DVec3::NEG_Y).unwrap();
    for (plane, disabled) in [
        (Workplane::default(), false),
        (reversed, false),
        (tilted, true),
    ] {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        open_bim_desktop::ui::theme::apply(&ctx);
        let mut b = bench();
        b.set_workplane(plane);
        let output = ctx.run_headless(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280., 800.),
                )),
                ..Default::default()
            },
            |ui| {
                let _ = b.panels(ui);
                egui::CentralPanel::default().show(ui, |_| {});
            },
        );
        let nodes = output.platform_output.accesskit_update.unwrap().nodes;
        let field = nodes
            .iter()
            .map(|(_, n)| n)
            .find(|n| {
                n.role() == egui::accesskit::Role::TextInput
                    && n.labelled_by().iter().any(|id| {
                        nodes.iter().any(|(key, label)| {
                            key == id && label.value() == Some("Высота плоскости, мм")
                        })
                    })
            })
            .expect("named elevation input");
        assert_eq!(field.is_disabled(), disabled);
    }
}

#[test]
fn fitting_selected_part_bounds_keeps_all_its_corners_on_screen() {
    use open_bim_renderer::{Bounds, Camera, NamedView};
    let p = open_bim_model::example_steel_frame().unwrap();
    let id = *p.elements.keys().next().unwrap();
    let points = open_bim_desktop::interaction::selection::corners(&p, id).unwrap();
    let (min, max) = points
        .iter()
        .fold((points[0], points[0]), |(min, max), point| {
            (min.min(*point), max.max(*point))
        });
    for view in [
        NamedView::Isometric,
        NamedView::Top,
        NamedView::Front,
        NamedView::Right,
    ] {
        for aspect in [1.0, 1.6, 2.0] {
            let mut camera = Camera::default();
            camera.set_view(view);
            camera.fit(Bounds { min, max });
            for point in &points {
                let uv = camera
                    .project((min + max) * 0.5, *point, aspect)
                    .expect("selected corner remains in depth range");
                assert!(
                    uv.into_iter().all(|v| (-1e-9..=1. + 1e-9).contains(&v)),
                    "{view:?}, {aspect}: {uv:?}"
                );
            }
        }
    }
}
