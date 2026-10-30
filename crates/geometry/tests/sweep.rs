use open_bim_geometry::{Body, Point3};

const ORIGIN: Point3 = Point3([0.0; 3]);
const SECTION: [[f64; 2]; 4] = [[-5.0, -10.0], [5.0, -10.0], [5.0, 10.0], [-5.0, 10.0]];

#[test]
fn arc_volume_and_orientation() {
    for angle in [std::f64::consts::FRAC_PI_2, -std::f64::consts::FRAC_PI_2] {
        let body = Body::sweep_arc(&SECTION, [0.0, 0.0], 100.0, 0.0, angle, ORIGIN).unwrap();
        let expected = 200.0 * 100.0 * angle.abs();
        assert!((body.volume_mm3().unwrap() - expected).abs() < 1e-6);
        assert_closed(&body);
    }
}

#[test]
fn polyline_volume() {
    let body = Body::sweep_polyline(
        &SECTION,
        &[[0.0, 0.0], [100.0, 0.0], [100.0, 100.0]],
        ORIGIN,
    )
    .unwrap();
    assert!((body.volume_mm3().unwrap() - 40_000.0).abs() < 1e-6);
    assert_closed(&body);
}

#[test]
fn arc_volume_uses_section_centroid_and_mass() {
    let section = [[0.0, 0.0], [10.0, 0.0], [10.0, 20.0], [0.0, 20.0]];
    let body = Body::sweep_arc(&section, [12.0, -7.0], 100.0, 0.4, 1.0, ORIGIN).unwrap();
    let expected_volume = 200.0 * 105.0;
    assert!((body.volume_mm3().unwrap() - expected_volume).abs() < 1e-8);
    // Steel density 7850 kg/m³; dimensions are in millimetres.
    assert!((body.volume_mm3().unwrap() * 7850.0 / 1e9 - 0.16485).abs() < 1e-10);
}

fn assert_closed(body: &Body) {
    let mesh = body.display_mesh().unwrap();
    assert!(!mesh.triangles.is_empty());
    let mut edges = std::collections::BTreeMap::<(u32, u32), [usize; 2]>::new();
    for triangle in mesh.triangles {
        for i in 0..3 {
            let a = triangle[i];
            let b = triangle[(i + 1) % 3];
            edges.entry((a.min(b), a.max(b))).or_default()[usize::from(a > b)] += 1;
        }
    }
    assert!(edges.values().all(|counts| *counts == [1, 1]));
}

#[test]
fn rejects_bad_paths() {
    assert!(Body::sweep_polyline(&SECTION, &[[0.0, 0.0], [0.0, 0.0]], ORIGIN).is_err());
    assert!(
        Body::sweep_polyline(&SECTION, &[[0.0, 0.0], [10.0, 0.0], [0.0, 0.0]], ORIGIN).is_err()
    );
    assert!(Body::sweep_arc(&SECTION, [0.0, 0.0], 4.0, 0.0, 1.0, ORIGIN).is_err());
    // The path itself does not cross, but remote parts of the section overlap.
    let wide = [[-10., -10.], [10., -10.], [10., 10.], [-10., 10.]];
    assert!(
        Body::sweep_polyline(
            &wide,
            &[
                [0., 0.],
                [100., 0.],
                [100., 100.],
                [0., 100.],
                [0., 12.],
                [90., 12.]
            ],
            ORIGIN
        )
        .is_err()
    );
}

#[test]
fn rounded_polyline_does_not_fail_envelope_checks() {
    let mut path = vec![[-500., 0.], [0., 0.]];
    for i in 1..=30 {
        let a = -std::f64::consts::FRAC_PI_2 + std::f64::consts::FRAC_PI_2 * i as f64 / 30.;
        path.push([300. * a.cos(), 300. + 300. * a.sin()]);
    }
    path.push([300., 800.]);
    let body = Body::sweep_polyline(&SECTION, &path, ORIGIN).unwrap();
    assert_closed(&body);
}
