use open_bim_model::{
    Detail, DetailKind, Element, ElementShape, Frame, LengthMm, ObjectId, Point3, Project,
};

#[test]
fn combined_hole_and_cuts_match_independent_mass_fixtures() {
    let rows: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/steel/mixed-masses.json"
    ))
    .unwrap();
    for row in rows.as_array().unwrap() {
        let mut p = Project::default();
        p.elements.insert(
            ObjectId(8),
            Element {
                id: ObjectId(8),
                name: "Mixed fixture".into(),
                material: ObjectId(1),
                properties: Default::default(),
                shape: ElementShape::Plate {
                    contour: vec![[0., 0.], [100., 0.], [100., 100.], [0., 100.]],
                    thickness: LengthMm::new(20.).unwrap(),
                    frame: Frame::default(),
                },
            },
        );
        let mut features = vec![DetailKind::Hole {
            part: ObjectId(8),
            frame: Frame {
                origin: Point3([50., 50., -1.]),
                ..Frame::default()
            },
            diameter: 20.,
            depth: 22.,
            slot: 0.0,
        }];
        let case = row["case"].as_str().unwrap();
        if case.contains("box") {
            features.push(DetailKind::BoxCut {
                part: ObjectId(8),
                origin: Point3([20., 20., -1.]),
                size: [20., 20., 22.],
            });
        }
        if case.contains("plane") {
            features.push(DetailKind::PlaneCut {
                part: ObjectId(8),
                normal: [1., 0., 0.],
                offset: 75.,
            });
        }
        p.next_id = 9;
        for kind in features {
            let id = p.allocate_id().unwrap();
            p.details.insert(id, Detail { id, kind });
        }
        p.validate().unwrap();
        let body = p
            .element_body(ObjectId(8))
            .unwrap_or_else(|e| panic!("{case}: {e}"));
        let expected = row["volume_mm3"].as_f64().unwrap();
        let volume = body.volume_mm3().unwrap();
        assert!(
            (volume - expected).abs() / expected <= 1e-6,
            "{case}: {volume} != {expected}"
        );
        // Integrate the exported double-precision mesh independently of Body's
        // volume field, so analytic metadata cannot conceal an incorrect cut.
        let (vertices, triangles) = body.tessellation_f64().unwrap();
        let mesh_volume: f64 = triangles
            .iter()
            .map(|triangle| {
                let [a, b, c] = triangle.map(|i| vertices[i as usize]);
                (a[0] * (b[1] * c[2] - b[2] * c[1])
                    + a[1] * (b[2] * c[0] - b[0] * c[2])
                    + a[2] * (b[0] * c[1] - b[1] * c[0]))
                    / 6.0
            })
            .sum();
        assert!(
            (mesh_volume - expected).abs() / expected <= 1e-6,
            "{case} mesh: {mesh_volume} != {expected}"
        );
        let expected_mass = row["mass_kg"].as_f64().unwrap();
        let mass = p.element_mass_kg(ObjectId(8)).unwrap();
        assert!((mass - expected_mass).abs() / expected_mass <= 1e-6);
    }
}
