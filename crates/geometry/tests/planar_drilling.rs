use open_bim_geometry::{Body, Point3};

fn independently_integrated(body: &Body) -> f64 {
    let (p, t) = body.tessellation_f64().unwrap();
    t.iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| p[i as usize]);
            (a[0] * (b[1] * c[2] - b[2] * c[1])
                + a[1] * (b[2] * c[0] - b[0] * c[2])
                + a[2] * (b[0] * c[1] - b[1] * c[0]))
                / 6.
        })
        .sum()
}

#[test]
fn four_transverse_bores_on_both_angle_legs_preserve_closed_exact_body() {
    let contour = [
        [0., 0.],
        [100., 0.],
        [100., 20.],
        [20., 20.],
        [20., 100.],
        [0., 100.],
    ];
    let mut body = Body::prism(&contour, 170., Point3([0.; 3])).unwrap();
    let mut expected = 3600. * 170.;
    for (axes, start) in [
        ([[1., 0., 0.], [0., 0., -1.], [0., 1., 0.]], [45., -1., 45.]),
        (
            [[1., 0., 0.], [0., 0., -1.], [0., 1., 0.]],
            [45., -1., 125.],
        ),
        ([[0., 1., 0.], [0., 0., 1.], [1., 0., 0.]], [-1., 45., 45.]),
        ([[0., 1., 0.], [0., 0., 1.], [1., 0., 0.]], [-1., 45., 125.]),
    ] {
        body = body.drill(axes, start, 11., 22.).unwrap();
        expected -= std::f64::consts::PI * 121. * 20.;
        assert!((body.volume_mm3().unwrap() / expected - 1.).abs() < 1e-12);
        assert!((independently_integrated(&body) / expected - 1.).abs() < 1e-6);
    }
}

#[test]
fn repeated_transverse_bores_survive_rigid_placement_and_reverse_direction() {
    let mut body = Body::cuboid([100., 100., 100.], Point3([0.; 3])).unwrap();
    body = body
        .drill(
            [[0., 1., 0.], [0., 0., 1.], [1., 0., 0.]],
            [-1., 25., 25.],
            5.,
            102.,
        )
        .unwrap();
    body = body
        .drill(
            [[0., 1., 0.], [0., 0., -1.], [-1., 0., 0.]],
            [101., 75., 75.],
            5.,
            102.,
        )
        .unwrap();
    body = body
        .rigid_transform(
            [[0., 1., 0.], [-1., 0., 0.], [0., 0., 1.]],
            [200., 300., 400.],
        )
        .unwrap();
    body = body
        .drill(
            [[1., 0., 0.], [0., 0., 1.], [0., -1., 0.]],
            [150., 401., 450.],
            5.,
            102.,
        )
        .unwrap();
    let expected = 1e6 - 3. * std::f64::consts::PI * 25. * 100.;
    assert!((body.volume_mm3().unwrap() / expected - 1.).abs() < 1e-12);
    assert!((independently_integrated(&body) / expected - 1.).abs() < 1e-6);
}

#[test]
fn small_valid_bore_vertices_are_not_collapsed_by_seam_welding() {
    let body = Body::cuboid([10., 10., 10.], Point3([0.; 3]))
        .unwrap()
        .drill(
            [[0., 1., 0.], [0., 0., 1.], [1., 0., 0.]],
            [-1., 5., 5.],
            0.002,
            12.,
        )
        .unwrap();
    let expected = 1000. - std::f64::consts::PI * 0.002_f64.powi(2) * 10.;
    assert!((body.volume_mm3().unwrap() / expected - 1.).abs() < 1e-12);
    assert!((independently_integrated(&body) / expected - 1.).abs() < 1e-6);
}
