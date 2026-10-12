use open_bim_geometry::{Body, Point3};

#[test]
fn hollow_profiles_are_closed_exact_bodies() {
    let tube = Body::tube(57., 51., 500., Point3([0.; 3])).unwrap();
    let expected = std::f64::consts::PI * (57. * 57. - 51. * 51.) * 500.;
    assert!((tube.volume_mm3().unwrap() - expected).abs() / expected < 1e-12);
    let (positions, triangles) = tube.tessellation_f64().unwrap();
    assert!(!triangles.is_empty());
    let volume: f64 = triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| positions[i as usize]);
            (a[0] * (b[1] * c[2] - b[2] * c[1])
                + a[1] * (b[2] * c[0] - b[0] * c[2])
                + a[2] * (b[0] * c[1] - b[1] * c[0]))
                / 6.0
        })
        .sum();
    assert!((volume - expected).abs() / expected < 1e-6);

    assert!(
        positions
            .iter()
            .any(|p| (p[0].hypot(p[1]) - 51.).abs() < 1e-6)
    );
    let rhs = Body::rectangular_tube(100., 150., 6., 500., Point3([0.; 3])).unwrap();
    assert!((rhs.volume_mm3().unwrap() - (100. * 150. - 88. * 138.) * 500.).abs() < 1e-6);
    assert!(Body::tube(50., 50., 500., Point3([0.; 3])).is_err());
    assert!(Body::rectangular_tube(100., 150., 50., 500., Point3([0.; 3])).is_err());
}

#[test]
fn cylinder_rigid_placement_preserves_volume_and_closed_mesh() {
    let body = Body::cylinder(5., 11., Point3([0.; 3])).unwrap();
    let axes = [[0., 1., 0.], [0., 0., 1.], [1., 0., 0.]];
    let placed = body.rigid_transform(axes, [-1., 50., 10.]).unwrap();
    let expected = std::f64::consts::PI * 25. * 11.;
    assert!((placed.volume_mm3().unwrap() - expected).abs() < 1e-10);
    let (points, triangles) = placed.tessellation_f64().unwrap();
    let volume: f64 = triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| points[i as usize]);
            (a[0] * (b[1] * c[2] - b[2] * c[1])
                + a[1] * (b[2] * c[0] - b[0] * c[2])
                + a[2] * (b[0] * c[1] - b[1] * c[0]))
                / 6.
        })
        .sum();
    assert!((volume - expected).abs() / expected < 1e-6);
    assert!(points.iter().all(|p| p[0] >= -1.0000001
        && p[0] <= 10.0000001
        && (p[1] - 50.).hypot(p[2] - 10.) <= 5.0000001));
    assert!(
        body.rigid_transform([[1., 0., 0.], [1., 0., 0.], [0., 0., 1.]], [0.; 3])
            .is_err()
    );
}
