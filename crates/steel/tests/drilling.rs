use open_bim_model::{
    Detail, DetailKind, Element, ElementShape, Frame, LengthMm, ObjectId, Project,
};

#[test]
fn blind_and_transverse_holes_change_exact_body_and_mass() {
    let fixtures: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/steel/drilling-masses.json"
    ))
    .unwrap();
    for row in fixtures.as_array().unwrap() {
        let frame: Frame = serde_json::from_value(row["frame"].clone()).unwrap();
        let depth = row["depth"].as_f64().unwrap();
        let mut p = Project::default();
        p.elements.insert(
            ObjectId(8),
            Element {
                id: ObjectId(8),
                name: "Drilled plate".into(),
                material: ObjectId(1),
                properties: Default::default(),
                shape: ElementShape::Plate {
                    contour: vec![[0., 0.], [100., 0.], [100., 100.], [0., 100.]],
                    thickness: LengthMm::new(20.).unwrap(),
                    frame: Frame::default(),
                },
            },
        );
        p.details.insert(
            ObjectId(9),
            Detail {
                id: ObjectId(9),
                kind: DetailKind::Hole {
                    part: ObjectId(8),
                    frame,
                    diameter: 10.,
                    depth,
                },
            },
        );
        p.next_id = 10;
        p.validate().unwrap();
        let expected = row["volume_mm3"].as_f64().unwrap();
        let body = p.element_body(ObjectId(8)).unwrap_or_else(|e| {
            panic!(
                "drill start {:?}, axis {:?}: {e}",
                frame.origin, frame.axes[2]
            )
        });
        let volume = body.volume_mm3().unwrap();
        assert!(
            (volume - expected).abs() / expected <= 1e-6,
            "{volume} != {expected}"
        );
        let (vertices, faces) = body.tessellation_f64().unwrap();
        assert!(!faces.is_empty());
        let mesh_volume: f64 = faces
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|i| vertices[i as usize]);
                (a[0] * (b[1] * c[2] - b[2] * c[1])
                    + a[1] * (b[2] * c[0] - b[0] * c[2])
                    + a[2] * (b[0] * c[1] - b[1] * c[0]))
                    / 6.0
            })
            .sum();
        assert!(
            (mesh_volume - expected).abs() / expected <= 1e-6,
            "{} mesh: {mesh_volume} != {expected}",
            row["case"]
        );

        assert!(vertices.iter().any(|v| {
            let relative: [f64; 3] = std::array::from_fn(|i| v[i] - frame.origin.0[i]);
            let local: [f64; 3] = frame
                .axes
                .map(|axis| (0..3).map(|i| axis[i] * relative[i]).sum());
            (local[0].hypot(local[1]) - 5.).abs() < 0.001
                && local[2] > 0.001
                && local[2] < depth - 0.001
        }));
        let mass = p.element_mass_kg(ObjectId(8)).unwrap();
        assert!((mass - row["mass_kg"].as_f64().unwrap()).abs() / mass <= 1e-6);
    }
}
