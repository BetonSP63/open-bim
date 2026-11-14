use open_bim_renderer::{Bounds, Camera, DVec3, NamedView};

fn camera(shift: DVec3, view: NamedView) -> Camera {
    let mut c = Camera::default();
    c.fit(Bounds {
        min: shift + DVec3::new(-10000., -10000., 0.),
        max: shift + DVec3::new(10000., 10000., 6000.),
    });
    c.set_view(view);
    c
}

#[test]
fn zoom_keeps_the_surface_anchor_under_the_cursor_in_all_views() {
    for shift in [DVec3::ZERO, DVec3::new(900_000_000., -800_000_000., 0.)] {
        for view in [
            NamedView::Isometric,
            NamedView::Top,
            NamedView::Front,
            NamedView::Right,
        ] {
            let mut c = camera(shift, view);
            let anchor = shift + DVec3::new(500., 700., 3000.);
            let before = c.project(shift, anchor, 1.6).unwrap();
            for _ in 0..6 {
                let ray = c.ray(shift, before, 1.6).unwrap();
                c.zoom_to(100., Some(ray), Some(anchor));
                let after = c.project(shift, anchor, 1.6).unwrap();
                assert!(
                    (after[0] - before[0]).abs() < 2e-7 && (after[1] - before[1]).abs() < 2e-7,
                    "{view:?} {shift:?}: {before:?} -> {after:?}"
                );
                assert!(c.matrix(shift, 1.6).is_finite());
            }
        }
    }
}

#[test]
fn orbit_about_a_pivot_keeps_its_screen_position_far_from_origin() {
    for shift in [DVec3::ZERO, DVec3::new(900_000_000., -800_000_000., 0.)] {
        for view in [NamedView::Isometric, NamedView::Top] {
            let mut c = camera(shift, view);
            let pivot = shift + DVec3::new(500., 700., 3000.);
            let before = c.project(shift, pivot, 1.6).unwrap();
            c.orbit_about(pivot, 20., -10.);
            let after = c.project(shift, pivot, 1.6).unwrap();
            assert!(
                (after[0] - before[0]).abs() < 2e-7 && (after[1] - before[1]).abs() < 2e-7,
                "{view:?} {shift:?}: {before:?} -> {after:?}"
            );
            assert!(c.matrix(shift, 1.6).is_finite());
        }
    }
}

#[test]
fn zero_motion_does_not_change_a_top_view() {
    let mut c = camera(DVec3::ZERO, NamedView::Top);
    let before = c.matrix(DVec3::ZERO, 1.6);
    let angles = (c.yaw, c.pitch);
    c.orbit(0., 0.);
    c.orbit_about(DVec3::ZERO, 0., 0.);
    assert_eq!((c.yaw, c.pitch), angles);
    assert_eq!(c.matrix(DVec3::ZERO, 1.6), before);
}

#[test]
fn leaving_a_plan_view_does_not_roll_its_screen_axes() {
    let mut c = camera(DVec3::ZERO, NamedView::Top);
    let point = c.target + DVec3::X * 1000.;
    let before = c.project(DVec3::ZERO, point, 1.6).unwrap();
    c.orbit(0., -1.);
    let after = c.project(DVec3::ZERO, point, 1.6).unwrap();
    assert!((after[0] - before[0]).abs() < 1e-7 && (after[1] - before[1]).abs() < 1e-7);
}
