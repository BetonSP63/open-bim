use open_bim_geometry::{Body, BodyError, Point3};

const ORIGIN: Point3 = Point3([0.0; 3]);

#[test]
fn analytical_body_corpus() {
    let corpus = include_str!("../../../tests/fixtures/geometry/corpus.csv");
    let mut count = 0;
    let mut report =
        String::from("id,expected_volume_mm3,measured_volume_mm3,relative_error,closed_oriented\n");
    for line in corpus.lines().skip(1) {
        let fields: Vec<_> = line.split(',').collect();
        let id = fields[0];
        let operation = fields[1];
        let s: f64 = fields[2].parse().unwrap();
        let expected: f64 = fields[3].parse().unwrap();
        let result = match operation {
            "cuboid" => Body::cuboid([s, s, s], ORIGIN),
            "angle" => Body::prism(
                &[
                    [0.0, 0.0],
                    [s, 0.0],
                    [s, 0.2 * s],
                    [0.2 * s, 0.2 * s],
                    [0.2 * s, s],
                    [0.0, s],
                ],
                2.0 * s,
                ORIGIN,
            ),
            "plate" => Body::cuboid([s, 2.0 * s, 0.1 * s], ORIGIN),
            "hole" => {
                let base = Body::cuboid([s, s, s], ORIGIN).unwrap();
                let tool =
                    Body::cuboid([0.2 * s, 0.3 * s, 3.0 * s], Point3([0.3 * s, 0.2 * s, -s]))
                        .unwrap();
                base.difference(&tool)
            }
            "intersection" => {
                let base = Body::cuboid([s, s, s], ORIGIN).unwrap();
                let tool = Body::cuboid([0.8 * s; 3], Point3([0.5 * s, 0.4 * s, 0.3 * s])).unwrap();
                base.intersection(&tool)
            }
            "clip" => {
                let base = Body::cuboid([s, s, s], ORIGIN).unwrap();
                let tool = Body::prism(
                    &[[-s, -s], [1.8 * s, -s], [-s, 1.8 * s]],
                    3.0 * s,
                    Point3([0.0, 0.0, -s]),
                )
                .unwrap();
                base.intersection(&tool)
            }
            _ => panic!("Unknown fixture operation"),
        };
        let body = result.unwrap_or_else(|error| panic!("{id}: {error}"));
        let volume = body.volume_mm3().unwrap();
        let relative_error = (volume - expected).abs() / expected;
        assert!(
            relative_error <= 1e-6,
            "{id}: volume {volume} expected {expected}, relative error {relative_error}"
        );
        let mesh = body.display_mesh().unwrap();
        assert!(!mesh.triangles.is_empty(), "{id}: empty mesh");
        let mut edges = std::collections::BTreeMap::<(u32, u32), [usize; 2]>::new();
        for triangle in &mesh.triangles {
            for i in 0..3 {
                let a = triangle[i];
                let b = triangle[(i + 1) % 3];
                let key = (a.min(b), a.max(b));
                edges.entry(key).or_default()[usize::from(a > b)] += 1;
            }
        }
        assert!(
            edges.values().all(|count| *count == [1, 1]),
            "{id}: nonmanifold or unoriented display mesh"
        );
        report.push_str(&format!("{id},{expected},{volume},{relative_error},true\n"));
        count += 1;
    }
    assert!(count >= 30);
    if let Ok(path) = std::env::var("OPEN_BIM_GEOMETRY_REPORT") {
        std::fs::write(path, report).unwrap();
    }
}

#[test]
fn large_world_coordinates_keep_local_precision() {
    let body = Body::cuboid([10.0, 20.0, 0.005], Point3([1e6, 1e6, 1e6])).unwrap();
    assert!((body.volume_mm3().unwrap() - 1.0).abs() < 1e-6);
    let mesh = body.display_mesh().unwrap();
    assert_eq!(mesh.origin, Point3([1e6; 3]));
    assert!(mesh.positions.iter().flatten().all(|v| v.abs() <= 20.0));
}

#[test]
fn malformed_input_returns_errors() {
    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert_eq!(
            Body::cuboid([10.0, 10.0, bad], ORIGIN).unwrap_err(),
            BodyError::InvalidInput
        );
    }
    assert_eq!(
        Body::cuboid([10.0; 3], Point3([f64::NAN, 0.0, 0.0])).unwrap_err(),
        BodyError::InvalidInput
    );
    assert_eq!(
        Body::prism(
            &[[0.0, 0.0], [1.0, 1.0], [0.0, 1.0], [1.0, 0.0]],
            10.0,
            ORIGIN
        )
        .unwrap_err(),
        BodyError::DegenerateContour
    );
}

#[test]
fn touching_and_coplanar_operations_are_explicit() {
    let base = Body::cuboid([10.0; 3], ORIGIN).unwrap();
    for x in [10.0, 5.0] {
        let tool = Body::cuboid([10.0; 3], Point3([x, 0.0, 0.0])).unwrap();
        let result = base.intersection(&tool);
        match result {
            Ok(body) => {
                assert_eq!(x, 5.0, "Touching surfaces have no solid volume");
                assert!((body.volume_mm3().unwrap() - 500.0).abs() < 0.0005);
            }
            Err(error) => assert!(matches!(
                error,
                BodyError::KernelFailure | BodyError::OpenOrInvertedBody
            )),
        }
    }
}
