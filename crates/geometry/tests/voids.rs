use open_bim_geometry::{Body, Point3};

fn rectangle(x: f64, y: f64, w: f64, h: f64) -> Vec<[f64; 2]> {
    vec![[x, y], [x + w, y], [x + w, y + h], [x, y + h]]
}

#[test]
fn polygon_voids_reject_contact_overlap_and_containment() {
    let outer = rectangle(0., 0., 100., 100.);
    for voids in [
        vec![rectangle(0., 20., 20., 20.)],
        vec![rectangle(90., 20., 20., 20.)],
        vec![rectangle(10., 10., 30., 30.), rectangle(20., 20., 30., 30.)],
        vec![rectangle(10., 10., 30., 30.), rectangle(20., 20., 5., 5.)],
        vec![rectangle(40., 40., 20., 20.)],
    ] {
        assert!(
            Body::prism_with_voids(&outer, 20., &[([50., 50.], 10.)], &voids, Point3([0.; 3]))
                .is_err()
        );
    }
    // The inner rectangle's vertices lie inside this concave outline, but
    // an edge crosses its re-entrant boundary and must be rejected.
    let concave = [
        [0., 0.],
        [100., 0.],
        [100., 100.],
        [60., 100.],
        [60., 40.],
        [40., 40.],
        [40., 100.],
        [0., 100.],
    ];
    assert!(
        Body::prism_with_voids(
            &concave,
            20.,
            &[],
            &[rectangle(20., 60., 60., 20.)],
            Point3([0.; 3])
        )
        .is_err()
    );
}

#[test]
fn reversed_loops_preserve_exact_void_body_and_closed_mesh_volume() {
    let mut outer = rectangle(0., 0., 100., 100.);
    outer.reverse();
    let mut first = rectangle(10., 10., 20., 20.);
    first.reverse();
    let body = Body::prism_with_voids(
        &outer,
        20.,
        &[([50., 50.], 10.)],
        &[first, rectangle(70., 70., 10., 10.)],
        Point3([0.; 3]),
    )
    .unwrap();
    let expected = (10000. - 500. - std::f64::consts::PI * 100.) * 20.;
    assert!((body.volume_mm3().unwrap() / expected - 1.).abs() < 1e-12);
    let (vertices, triangles) = body.tessellation_f64().unwrap();
    let volume: f64 = triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| vertices[i as usize]);
            (a[0] * (b[1] * c[2] - b[2] * c[1])
                + a[1] * (b[2] * c[0] - b[0] * c[2])
                + a[2] * (b[0] * c[1] - b[1] * c[0]))
                / 6.
        })
        .sum();
    assert!((volume / expected - 1.).abs() < 1e-6);
}
