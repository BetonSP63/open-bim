use open_bim_geometry::{Body, Point3};

#[test]
fn exact_circular_bores_change_body_and_mass() {
    let contour = [[0., 0.], [400., 0.], [400., 300.], [0., 300.]];
    let holes = [([100., 100.], 11.), ([300., 200.], 11.)];
    let body = Body::prism_with_holes(&contour, 20., &holes, Point3([1e6, 1e6, 1e6])).unwrap();
    let expected = (400. * 300. - 2. * std::f64::consts::PI * 11f64.powi(2)) * 20.;
    assert!((body.volume_mm3().unwrap() / expected - 1.).abs() < 1e-12);
    let (points, triangles) = body.tessellation_f64().unwrap();
    // Independent tetrahedral volume of actual B-rep tessellation, no nominal subtraction.
    let volume: f64 = triangles
        .iter()
        .map(|face| {
            let [a, b, c] = face.map(|i| points[i as usize]);
            (a[0] * (b[1] * c[2] - b[2] * c[1])
                + a[1] * (b[2] * c[0] - b[0] * c[2])
                + a[2] * (b[0] * c[1] - b[1] * c[0]))
                / 6.
        })
        .sum();
    assert!(
        (volume / expected - 1.).abs() < 1e-6,
        "{volume} versus {expected}"
    );
    assert!(
        points
            .iter()
            .any(|p| ((p[0] - 100.).hypot(p[1] - 100.) - 11.).abs() < 1e-6)
    );
}

#[test]
fn bores_reject_contact_overlap_and_outside() {
    let contour = [[0., 0.], [100., 0.], [100., 100.], [0., 100.]];
    for holes in [
        vec![([10., 10.], 10.)],
        vec![([150., 50.], 5.)],
        vec![([50., 50.], 10.), ([60., 50.], 10.)],
        vec![([50., 50.], f64::NAN)],
    ] {
        assert!(Body::prism_with_holes(&contour, 10., &holes, Point3([0.; 3])).is_err());
    }
}

#[test]
fn diagonal_plane_and_rectangular_cut_change_exact_body() {
    let body = Body::cuboid([100., 100., 20.], Point3([0.; 3])).unwrap();
    let clipped = body.clip_plane([1., 1., 0.], 100.).unwrap();
    assert!((clipped.volume_mm3().unwrap() / 100000. - 1.).abs() < 1e-6);
    let cutter = Body::cuboid([20., 20., 40.], Point3([20., 20., -10.])).unwrap();
    let cut = clipped.difference(&cutter).unwrap();
    assert!((cut.volume_mm3().unwrap() / 92000. - 1.).abs() < 1e-6);
    assert!(body.clip_plane([0., 0., 0.], 1.).is_err());
}
